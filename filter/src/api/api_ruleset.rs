use crate::api::routes_filter::SharedState;
use crate::db::commands::FilterDbCmd;
use crate::observability::sinks::sink_ecam;
use crate::observability::sinks::sink_ecam::{Origin, Severity};
use crate::observability::summary::dump_port_state;
use crate::rules;
use crate::rules::{CompiledRuleSet, calc_version_ruleset};
use axum::Json;
use axum::extract::{OriginalUri, Path, State};
use axum::http::StatusCode;
use shared::api::err::ErrorEnvelope;
use shared::api::ok::{OkEnvelope, ok};
use shared::types::routing::filter::ruleset::{
    FilterReplyRulesetHistory, FilterReplySuccessRuleset, FilterReplySuccessValidationRuleset,
};
use shared::types::ruleset_types::RuleSetRaw;
use shared::utils::time::now_ns;
use std::sync::Arc;
use tokio::sync::oneshot;
use tracing::info;

/// /ports/{port}/ruleset
///
/// Fetches the active ruleset for the given port
#[utoipa::path(
    get,
    path = "/ports/{port}/ruleset",
    params(
        ("port" = u16, Path,                                        description = "Public port"),
    ),
    responses(
        (status = 200, description = "OK",                          body = OkEnvelope<FilterReplySuccessRuleset>),
        (status = 401, description = "Unauthorized",                body = ErrorEnvelope),
        (status = 404, description = "Unknown port",                body = ErrorEnvelope),
    ),
    tag = "filter"
)]
pub async fn get_ruleset(
    OriginalUri(uri): OriginalUri,
    Path(port): Path<u16>,
    State((_cfg, registry, _db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match registry.get(&port) {
        Some(pr) => {
            let runtime_ruleset_arc = pr.raw_ruleset.load_full();
            let version_ruleset = calc_version_ruleset(&runtime_ruleset_arc);

            ok(FilterReplySuccessRuleset {
                ruleset: (*runtime_ruleset_arc).clone(),
                ver_ruleset: version_ruleset,
            })
        }
        None => {
            sink_ecam::record(
                Severity::Warn,
                Origin::API,
                format!(
                    "[get_ruleset(), GET {}]: port {} not found",
                    uri.path(),
                    port
                ),
                None,
            );

            ErrorEnvelope::not_found("sanitization", uri.path())
                .with_title("Unknown Port")
                .into_result()
        }
    }
}

/// /ports/{port}/ruleset/history
///
/// Returns the full publish history for the port (newest first). Each entry
/// carries the full `RuleSetRaw` for that revision, so the caller can replay any
/// past ruleset by re-publishing it via PUT /ports/{port}/ruleset (rollback).
#[utoipa::path(
    get,
    path = "/ports/{port}/ruleset/history",
    params(
        ("port" = u16, Path,                                        description = "Public port"),
    ),
    responses(
        (status = 200, description = "OK",                          body = OkEnvelope<FilterReplyRulesetHistory>),
        (status = 401, description = "Unauthorized",                body = ErrorEnvelope),
        (status = 404, description = "Unknown port",                body = ErrorEnvelope),
        (status = 500, description = "Internal server error",       body = ErrorEnvelope),
    ),
    tag = "filter"
)]
pub async fn get_ruleset_history(
    OriginalUri(uri): OriginalUri,
    Path(port): Path<u16>,
    State((_cfg, registry, db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    // A port that the proxy doesn't know about has no history to show.
    if registry.get(&port).is_none() {
        sink_ecam::record(
            Severity::Warn,
            Origin::API,
            format!(
                "[get_ruleset_history(), GET {}]: port {} not found",
                uri.path(),
                port
            ),
            None,
        );
        return ErrorEnvelope::not_found("sanitization", uri.path())
            .with_title("Unknown Port")
            .into_result();
    }

    let (tx, rx) = oneshot::channel();
    if db
        .tx
        .send(FilterDbCmd::LoadRulesetHistory { port, reply: tx })
        .await
        .is_err()
    {
        return ErrorEnvelope::internal("db_unavailable", uri.path())
            .with_title("Database unavailable")
            .with_detail("the db actor channel is closed")
            .into_result();
    }

    match rx.await {
        Ok(entries) => ok(FilterReplyRulesetHistory { entries }),
        Err(_) => ErrorEnvelope::internal("db_unavailable", uri.path())
            .with_title("Database unavailable")
            .with_detail("the db actor dropped the history reply")
            .into_result(),
    }
}

/// /ports/{port}/ruleset
///
/// Takes a `RuleSetRaw`, validates it, and if everything checks out, applies it to the port runtime
#[utoipa::path(
    put,
    path = "/ports/{port}/ruleset",
    params(
        ("port" = u16, Path,                                        description = "Public port"),
    ),
    responses(
        (status = 200, description = "OK",                          body = OkEnvelope<FilterReplySuccessRuleset>),
        (status = 400, description = "Invalid request",             body = ErrorEnvelope),
        (status = 401, description = "Unauthorized",                body = ErrorEnvelope),
        (status = 404, description = "Unknown port",                body = ErrorEnvelope),
        (status = 500, description = "Internal server error",       body = ErrorEnvelope),
    ),
    tag = "filter"
)]
pub async fn put_ruleset(
    OriginalUri(uri): OriginalUri,
    Path(port): Path<u16>,
    State((_cfg, registry, db)): State<SharedState>,
    Json(body): Json<RuleSetRaw>,
) -> (StatusCode, Json<serde_json::Value>) {
    // locate port
    let pr = match registry.get(&port) {
        Some(p) => p,
        None => {
            sink_ecam::record(
                Severity::Warn,
                Origin::API,
                format!(
                    "[get_ruleset(), GET {}]: port {} not found",
                    uri.path(),
                    port
                ),
                None,
            );

            return ErrorEnvelope::not_found("sanitization", uri.path())
                .with_title("Unknown Port")
                .into_result();
        }
    };

    // validate
    let scopes_guard = pr.enabled_scopes.read();
    let errs = rules::validate::validate_ruleset(&body, &scopes_guard);
    drop(scopes_guard);

    if !errs.is_empty() {
        sink_ecam::record(
            Severity::Warn,
            Origin::API,
            format!(
                "[put_ruleset(), PUT {}]: rejecting rules | ruleset validation failed, errors={}",
                uri.path(),
                errs.join(", ")
            ),
            None,
        );

        return ErrorEnvelope::bad_request("ruleset_validation_error", uri.path())
            .with_title("Ruleset validation failed")
            .with_detail(format!(
                "The given ruleset failed verification with the following errors: {}",
                errs.join(", ")
            ))
            .into_result();
    }

    // version + compile
    let ver_ruleset = calc_version_ruleset(&body);
    let validated_ruleset_raw_json = serde_json::to_string(&body).unwrap();

    let compiled: CompiledRuleSet = match rules::compile::compile_ruleset(&body) {
        Ok(c) => c,
        Err(e) => {
            sink_ecam::record(
                Severity::Warn,
                Origin::API,
                format!(
                    "[put_ruleset(), PUT {}]: rejecting rules | compilation failed, errors={e}",
                    uri.path()
                ),
                None,
            );

            return ErrorEnvelope::bad_request("ruleset_validation_error", uri.path())
                .with_title("Ruleset validation failed")
                .with_detail(format!(
                    "The given ruleset failed verification with the following errors: {}",
                    e
                ))
                .into_result();
        }
    };

    // Print ruleset log
    info!(
        port,
        ver_ruleset,
        markers = compiled.clone().markers.len(),
        filters = compiled.clone().filters.len(),
        "ruleset accepted"
    );

    // Store data to port runtime
    pr.raw_ruleset.store(Arc::new(body));
    pr.compiled.store(Arc::new(compiled));

    // Log new port fetch
    dump_port_state(&pr);

    // Save config to db
    let _ = db.tx.try_send(FilterDbCmd::SaveRuleset {
        port,
        version: ver_ruleset.clone(),
        json: validated_ruleset_raw_json,
        ts_ns: now_ns() as i64,
    });

    ok(FilterReplySuccessRuleset {
        ruleset: pr.to_rulesetraw(),
        ver_ruleset,
    })
}

/// /ports/{port}/ruleset/validate
///
/// Takes a `RuleSetRaw`, validates it, and returns the validation output, withouth changing the runtime configuration.
/// Useful for dry-runs / pre-validation
#[utoipa::path(
    put,
    path = "/ports/{port}/ruleset/validate",
    params(
        ("port" = u16, Path,                                    description = "Public port"),
    ),
    responses(
        (status = 200, description = "OK",                      body = OkEnvelope<FilterReplySuccessValidationRuleset>),
        (status = 401, description = "Unauthorized",            body = ErrorEnvelope),
        (status = 404, description = "Unknown port",            body = ErrorEnvelope),
    ),
    tag = "filter"
)]
pub async fn put_ruleset_validate(
    OriginalUri(uri): OriginalUri,
    Path(port): Path<u16>,
    State((_cfg, registry, _db)): State<SharedState>,
    Json(body): Json<rules::RuleSetRaw>,
) -> (StatusCode, Json<serde_json::Value>) {
    let pr = match registry.get(&port) {
        Some(p) => p,
        None => {
            return ErrorEnvelope::not_found("sanitization", uri.path())
                .with_title("Unknown Port")
                .into_result();
        }
    };

    let scopes_guard = pr.enabled_scopes.read();
    let errs = rules::validate::validate_ruleset(&body, &scopes_guard);
    drop(scopes_guard);

    if !errs.is_empty() {
        sink_ecam::record(
            Severity::Warn,
            Origin::API,
            format!(
                "[validate_ruleset(), PUT {}]: ruleset validation failed, errors={}",
                uri.path(),
                errs.join(", ")
            ),
            None,
        );

        return ok(FilterReplySuccessValidationRuleset {
            ruleset: pr.to_rulesetraw(),
            validated: false,
            validation_errors: Some(errs.join(", ")),
        });
    }

    // version + compile
    let version_ruleset = calc_version_ruleset(&body);

    let compiled: CompiledRuleSet = match rules::compile::compile_ruleset(&body) {
        Ok(c) => c,
        Err(e) => {
            sink_ecam::record(
                Severity::Warn,
                Origin::API,
                format!(
                    "[validate_ruleset(), PUT {}]: ruleset validation unsuccessful, errors={e}",
                    uri.path()
                ),
                None,
            );

            return ok(FilterReplySuccessValidationRuleset {
                ruleset: pr.to_rulesetraw(),
                validated: false,
                validation_errors: Some(errs.join(", ")),
            });
        }
    };

    info!(
        port,
        version_ruleset,
        markers = compiled.markers.len(),
        filters = compiled.filters.len(),
        "ruleset validated (from /validate)"
    );

    ok(FilterReplySuccessValidationRuleset {
        ruleset: pr.to_rulesetraw(),
        validated: true,
        validation_errors: None,
    })
}
