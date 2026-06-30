use crate::api::routes_filter::SharedState;
use crate::db::commands::FilterDbCmd;
use crate::observability::sinks::sink_ecam;
use crate::observability::sinks::sink_ecam::{Origin, Severity};
use axum::Json;
use axum::extract::{OriginalUri, Path, State};
use axum::http::StatusCode;
use serde_json::json;
use shared::api::err::ErrorEnvelope;
use shared::api::ok::{OkEnvelope, ok};
use shared::types::base_types::Scope;
use shared::types::routing::filter::ports::{
    FilterReplySuccessPortAndVersion, FilterReplySuccessPortAndVersionVec,
    FilterRequestPutBodyConfigUpdate,
};
use shared::utils::time::now_ns;
use shared::utils::version::calc_version_info;
use std::collections::HashSet;
use std::sync::Arc;
use tracing::info;

/// /ports
///
/// Retrieves from the registry *all* port information, and for each one
/// pairs the `PortInfo` and freshly calculates its version.
/// Returns the list of all pairs.
#[utoipa::path(
    get,
    path = "/ports",
    responses(
        (status = 200, description = "OK",                      body = OkEnvelope<FilterReplySuccessPortAndVersionVec>),
        (status = 401, description = "Unauthorized",            body=ErrorEnvelope),
    ),
    tag = "filter"
)]
pub async fn get_port_list(
    OriginalUri(_uri): OriginalUri,
    State((_cfg, registry, _db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let mut reply_ports_content: Vec<FilterReplySuccessPortAndVersion> = Vec::new();
    for entry in registry.iter() {
        let pr = entry.value();
        let info = pr.to_portinfo();
        let ver_info = calc_version_info(&info);

        reply_ports_content.push(FilterReplySuccessPortAndVersion { info, ver_info });
    }

    ok(FilterReplySuccessPortAndVersionVec {
        content: reply_ports_content,
    })
}

/// /ports/{port}/config
///
/// Retrieves from the registry a single port information, paired with its version.
#[utoipa::path(
    get,
    path = "/ports/{port}/config",
    params(
        ("port" = u16, Path,                                    description = "Public port"),
    ),
    responses(
        (status = 200, description = "OK",                      body = OkEnvelope<FilterReplySuccessPortAndVersion>),
        (status = 401, description = "Unauthorized",            body=ErrorEnvelope),
        (status = 404, description = "Unknown port",            body=ErrorEnvelope),
    ),
    tag = "filter"
)]
pub async fn get_config(
    OriginalUri(uri): OriginalUri,
    Path(port): Path<u16>,
    State((_cfg, registry, _db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match registry.get(&port) {
        Some(pr) => {
            let info = pr.value().to_portinfo();
            let ver_info = calc_version_info(&info);

            ok(FilterReplySuccessPortAndVersion { info, ver_info })
        }
        None => {
            sink_ecam::record(
                Severity::Warn,
                Origin::API,
                format!("[get_config(), GET {}]: port not found", uri.path()),
                None,
            );

            ErrorEnvelope::not_found("sanitization", uri.path())
                .with_title("Unknown Port")
                .into_result()
        }
    }
}

/// /ports/{port}/config
///
/// Validates the port configuration update, and if successful, will be
/// applied to the runtime port config of the given port.
#[utoipa::path(
    put,
    path = "/ports/{port}/config",
    params(
        ("port" = u16, Path,                                    description = "Public port"),
    ),
    responses(
        (status = 200, description = "OK",                      body = OkEnvelope<FilterReplySuccessPortAndVersion>),
        (status = 400, description = "Invalid request",         body = ErrorEnvelope),
        (status = 401, description = "Unauthorized",            body = ErrorEnvelope),
        (status = 404, description = "Unknown port",            body = ErrorEnvelope),
    ),
    tag = "filter"
)]
pub async fn put_config(
    OriginalUri(uri): OriginalUri,
    Path(port): Path<u16>,
    State((_cfg, registry, db)): State<SharedState>,
    Json(body): Json<FilterRequestPutBodyConfigUpdate>, // keep Json() after all else
) -> (StatusCode, Json<serde_json::Value>) {
    // ── port lookup ─────────────────────────────────────
    let port_runtime = match registry.get(&port) {
        Some(p) => p,
        None => {
            sink_ecam::record(
                Severity::Warn,
                Origin::API,
                format!(
                    "[put_config(), PUT {}]: rejecting config | port not found",
                    uri.path()
                ),
                None,
            );
            return ErrorEnvelope::not_found("sanitization", uri.path())
                .with_title("Unknown Port")
                .into_result();
        }
    };

    // ── validations (only for provided fields) ──────────
    let mut errs: Vec<String> = Vec::new();

    if port != port_runtime.binding.public {
        errs.push(format!("API port ({}) differs from port runtime object public port ({}). This is an internal error.", port, port_runtime.binding.public));
    }

    if let Some(ref scopes) = body.enabled_scopes
        && scopes.is_empty()
    {
        errs.push("enabled_scopes cannot be empty".to_string());
    }

    // A scope can only be disabled if the *active* ruleset has no rule using it.
    // Otherwise those markers/filters would keep evaluating in a scope the
    // operator believes is off — so we reject rather than leave them live.
    if let Some(ref scopes) = body.enabled_scopes
        && !scopes.is_empty()
    {
        let proposed: HashSet<Scope> = scopes.iter().cloned().collect();
        let active = port_runtime.to_rulesetraw();
        for m in &active.markers {
            if !proposed.contains(&m.scope) {
                errs.push(format!(
                    "cannot disable scope {:?}: still used by markers[id={}] in the active ruleset",
                    m.scope, m.id
                ));
            }
        }
        for f in &active.filters {
            if !proposed.contains(&f.scope) {
                errs.push(format!(
                    "cannot disable scope {:?}: still used by filters[id={}] in the active ruleset",
                    f.scope, f.id
                ));
            }
        }
    }

    if let Some(ref hex) = body.hex_color
        && !is_valid_hex_color(hex)
    {
        errs.push("hex_color must be in the form #RRGGBB".to_string());
    }

    if !errs.is_empty() {
        sink_ecam::record(
            Severity::Warn,
            Origin::API,
            format!(
                "[put_config(), PUT {}]: validation failed: {}",
                uri.path(),
                errs.join(", ")
            ),
            None,
        );
        return ErrorEnvelope::bad_request("config_validation_error", uri.path())
            .with_title("Port config validation failed")
            .with_detail(errs.join(", "))
            .into_result();
    }

    // ── apply updates (only provided fields) ─────────────
    if let Some(ref n) = body.name {
        port_runtime.name.store(Some(Arc::new(n.clone())));
    }
    if let Some(ref c) = body.comment {
        port_runtime.comment.store(Some(Arc::new(c.clone())));
    }
    if let Some(ref hc) = body.hex_color {
        port_runtime.hex_color.store(Some(Arc::new(hc.clone())));
    }
    if let Some(ref scopes) = body.enabled_scopes {
        let mut guard = port_runtime.enabled_scopes.write();
        guard.clear();
        guard.extend(scopes.iter().cloned());
        info!(port, scopes=?scopes, "enabled scopes updated");
    }

    // ── derive *effective* snapshot (post-update) ────────
    let info = port_runtime.to_portinfo();
    let ver_info = calc_version_info(&info);

    // ── persist effective config to DB ──────────────────
    let config_raw_json = json!({
        "name": info.name,
        "comment": info.comment,
        "hex_color": info.hex_color,
        "enabled_scopes": info.enabled_scopes
    })
    .to_string();

    let _ = db.tx.try_send(FilterDbCmd::SaveConfig {
        external_port: port,
        upstream_port: info.upstream,
        json: config_raw_json,
        ts_ns: now_ns() as i64,
    });

    info!(
        "[put_config(), PUT {}]: configuration updated, info={:?}, version={}",
        uri.path(),
        info,
        ver_info
    );

    ok(FilterReplySuccessPortAndVersion { info, ver_info })
}

/// strict #RRGGBB (6 hex digits) validator
fn is_valid_hex_color(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 7 || b[0] != b'#' {
        return false;
    }
    b[1..].iter().all(|c| c.is_ascii_hexdigit())
}
