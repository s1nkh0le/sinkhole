use crate::api::middleware_auth;
use crate::api::routes::SharedState;
use crate::db::actor::DbCmd;
use crate::observability::sinks::sink_ecam;
use crate::observability::sinks::sink_ecam::{Origin, Severity};
use crate::observability::summary::dump_port_state;
use crate::rules;
use crate::rules::{CompiledRuleSet, calc_version_raw_ruleset};
use axum::Json;
use axum::extract::{OriginalUri, Path, Query, State};
use axum::http::StatusCode;
use serde_json::json;
use shared::utils::time::now_ns;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{error, info};

#[utoipa::path(
    get,
    path = "/ports/{port}/ruleset",
    params(
        ("token" = String, description = "Authentication token"),
        ("port" = u16, Path, description = "Public port"),
    ),
    responses(
        (status = 200, description = "OK"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Unknown port"),
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
            let rs_arc = pr.raw_ruleset.load();
            let version = calc_version_raw_ruleset(&rs_arc).unwrap_or_default();
            (
                StatusCode::OK,
                Json(
                    json!({"status":"ok", "data":{"version":version, "ruleset": rs_arc.as_ref() }}),
                ),
            )
        }
        None => {
            sink_ecam::record(
                Severity::Warn,
                Origin::API,
                format!("[get_ruleset(), GET {}]: port not found", uri.path()),
                None,
            );

            (
                StatusCode::NOT_FOUND,
                Json(json!({"status":"error", "errors":["unknown port"]})),
            )
        }
    }
}


#[utoipa::path(
    put,
    path = "/ports/{port}/ruleset",
    params(
        ("token" = String, description = "Authentication token"),
        ("port" = u16, Path, description = "Public port"),
    ),
    responses(
        (status = 200, description = "OK"),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Unknown port"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "filter"
)]
pub async fn put_ruleset(
    OriginalUri(uri): OriginalUri,
    Path(port): Path<u16>,
    State((cfg, registry, db)): State<SharedState>,
    Json(body): Json<rules::RuleSetRaw>,
) -> (StatusCode, Json<serde_json::Value>) {
    // locate port
    let pr = match registry.get(&port) {
        Some(p) => p,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"status":"error","errors":["unknown port"]})),
            );
        }
    };

    // validate
    let scopes_guard = pr.enabled_scopes.read().unwrap();
    let errs = rules::validate::validate_ruleset(&body, &*scopes_guard);
    drop(scopes_guard);

    if !errs.is_empty() {
        sink_ecam::record(
            Severity::Warn,
            Origin::API,
            format!(
                "[put_ruleset(), PUT {}]: rejecting rules | ruleset validation failed, errors={}", uri.path(), errs.join(", ")
            ),
            None,
        );

        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"status":"error","errors": errs })),
        );
    }

    // version + compile
    let version = match calc_version_raw_ruleset(&body) {
        Ok(v) => v,
        Err(e) => {
            sink_ecam::record(
                Severity::Warn,
                Origin::API,
                format!(
                    "[put_ruleset(), PUT {}]: rejecting rules | version calc failed, errors={e}", uri.path()
                ),
                None,
            );

            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"status":"error","errors":[e.to_string()]})),
            );
        }
    };

    let compiled: CompiledRuleSet = match rules::compile::compile_ruleset(&body) {
        Ok(c) => c,
        Err(e) => {
            sink_ecam::record(
                Severity::Warn,
                Origin::API,
                format!(
                    "[put_ruleset(), PUT {}]: rejecting rules | compilation failed, errors={e}", uri.path()
                ),
                None,
            );

            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"status":"error","errors":[e.to_string()]})),
            );
        }
    };

    // Print ruleset log
    info!(
        port,
        version,
        markers = compiled.markers.len(),
        filters = compiled.filters.len(),
        "ruleset accepted"
    );

    let raw_str = serde_json::to_string(&body).unwrap();
    let version_db = version.clone();

    pr.raw_ruleset.store(Arc::new(body));
    pr.compiled.store(Arc::new(compiled));

    // Log new port state
    dump_port_state(&*pr);

    let _ = db.tx.try_send(DbCmd::SaveRuleset {
        port,
        version: version_db,
        json: raw_str,
        ts_ns: now_ns() as i64,
    });

    (
        StatusCode::OK,
        Json(json!({"status":"ok","data":{"version":version}})),
    )
}



#[utoipa::path(
    put,
    path = "/ports/{port}/ruleset/validate",
    params(
        ("token" = String, description = "Authentication token"),
        ("port" = u16, Path, description = "Public port"),
    ),
    responses(
        (status = 200, description = "OK"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Unknown port"),
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
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"status":"error","errors":["unknown port"]})),
            );
        }
    };

    let scopes_guard = pr.enabled_scopes.read().unwrap();
    let errs = rules::validate::validate_ruleset(&body, &*scopes_guard);
    drop(scopes_guard);

    if !errs.is_empty() {
        sink_ecam::record(
            Severity::Warn,
            Origin::API,
            format!(
                "[validate_ruleset(), PUT {}]: ruleset validation failed, errors={}", uri.path(),
                errs.join(", ")
            ),
            None,
        );

        return (
            StatusCode::OK,
            Json(json!({
                "status": "ok",
                "validated": false,
                "errors": errs,
            }))
        );
    }

    // version + compile
    let version = match calc_version_raw_ruleset(&body) {
        Ok(v) => v,
        Err(e) => {
            sink_ecam::record(
                Severity::Warn,
                Origin::API,
                format!(
                    "[validate_ruleset(), PUT {}]:  ruleset validation failed, errors={e}",  uri.path()
                ),
                None,
            );

            return (
                StatusCode::OK,
                Json(json!({
                    "status": "ok",
                    "validated": false,
                    "errors": errs,
                }))
            );
        }
    };

    let compiled: CompiledRuleSet = match rules::compile::compile_ruleset(&body) {
        Ok(c) => c,
        Err(e) => {
            sink_ecam::record(
                Severity::Warn,
                Origin::API,
                format!(
                    "[validate_ruleset(), PUT {}]: ruleset validation unsuccessful, errors={e}", uri.path()
                ),
                None,
            );

            return (
                StatusCode::OK,
                Json(json!({
                    "status": "ok",
                    "validated": false,
                    "errors": errs,
                }))
            );
        }
    };

    info!(
        port,
        version,
        markers = compiled.markers.len(),
        filters = compiled.filters.len(),
        "ruleset validated"
    );

    (
        StatusCode::OK,
        Json(json!({
            "status": "ok",
            "validated": true,
            "validated_version": version
        })),
    )
}
