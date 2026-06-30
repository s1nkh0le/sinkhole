use crate::api::fetchers::{
    get_ruleset_history_filter, put_ruleset_filter, validate_ruleset_filter,
};
use crate::api::util::{
    ensure_port_exists, ensure_service_exists, ensure_service_or_fetch_filter_state,
};
use crate::fetch::error::CtrlErr;
use crate::state::ctrl::ControllerState;
use axum::{
    Json,
    extract::{OriginalUri, Path, State},
    http::StatusCode,
};
use serde::Serialize;
use serde_json::Value;
use shared::api::err::ErrorEnvelope;
use shared::api::ok::{OkEnvelope, ok};
use shared::types::routing::filter::ruleset::RulesetHistoryEntry;
use shared::types::ruleset_types::RuleSetRaw;
use shared::utils::version::calc_version_combined_copyover;
use utoipa::ToSchema;

/// Successful ruleset reply
#[derive(Serialize, ToSchema)]
pub struct CtrlReplyRulesetOkData {
    pub ruleset: RuleSetRaw,
    pub version: String,
}
impl shared::api::ok::sealed::Sealed for CtrlReplyRulesetOkData {}
impl shared::api::ok::OkData for CtrlReplyRulesetOkData {}

/// /services/{service_id}/ports/{port}/ruleset
///
/// Returns the `RuleSetRaw` and its version for the given service id and port
#[utoipa::path(
    get,
    path = "/services/{service_id}/ports/{port}/ruleset",
    params(
        ("service_id" = String, Path,                                                   description = "Service identifier"),
        ("port" = u16, Path,                                                            description = "Public port"),
    ),
    responses(
        (status = 200, description = "OK",                                              body = OkEnvelope<CtrlReplyRulesetOkData>),
        (status = 400, description = "Invalid request",                                 body = ErrorEnvelope),
        (status = 401, description = "Unauthorized",                                    body = ErrorEnvelope),
        (status = 404, description = "Unknown service or port",                         body = ErrorEnvelope),
        (status = 500, description = "Server error",                                    body = ErrorEnvelope),
        (status = 502, description = "Filter unreachable or returned error",            body = ErrorEnvelope),
        (status = 504, description = "Filter timed out",                                body = ErrorEnvelope)
    ),
    tags = ["controller", "ruleset"]
)]
pub async fn get_ruleset(
    OriginalUri(uri): OriginalUri,
    Path((service_id, port)): Path<(String, u16)>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<Value>) {
    // ── 1) Validate service id ──────────────────────────────────────────
    if !ensure_service_exists(&state, &service_id) {
        return ErrorEnvelope::not_found("service_not_found", uri.path())
            .with_title("Unknown service")
            .with_detail(format!("service_id '{service_id}' is not known"))
            .into_result();
    }

    // ── 2) Ensure snapshots cached (one-shot fetch if needed) ───────────
    if let Err(e) = ensure_service_or_fetch_filter_state(&state, &service_id, &state.http).await {
        return e.into_result(uri.path());
    }

    // ── 3) Validate port existence after caching attempt ────────────────
    if !ensure_port_exists(&state, &service_id, port) {
        return ErrorEnvelope::not_found("port_not_found", uri.path())
            .with_title("Unknown port")
            .with_detail(format!(
                "port '{port}' is not known for service '{service_id}'"
            ))
            .into_result();
    }

    // ── 4) Extract ruleset + version from cached snapshot ───────────────
    let (ruleset, version) = {
        let g = state.read();
        // Re-check under the lock: the service/port could have been removed since
        // the existence check above, and the snapshot may simply not be cached yet
        // (port known but ruleset not fetched). Either way, don't panic.
        let Some(snap) = g
            .services
            .get(&service_id)
            .and_then(|svc| svc.ports.get(&port))
            .and_then(|p| p.snap.as_ref())
        else {
            return ErrorEnvelope::service_unavailable("ruleset_not_cached", uri.path())
                .with_title("Ruleset not available")
                .with_detail(format!(
                    "no cached ruleset snapshot for port '{port}' of service '{service_id}'"
                ))
                .into_result();
        };

        (snap.ruleset.clone(), snap.ver_ruleset.clone())
    };

    ok(CtrlReplyRulesetOkData { ruleset, version })
}

/// Ruleset publish history for a port (newest first).
#[derive(Serialize, ToSchema)]
pub struct CtrlReplyRulesetHistory {
    pub entries: Vec<RulesetHistoryEntry>,
}
impl shared::api::ok::sealed::Sealed for CtrlReplyRulesetHistory {}
impl shared::api::ok::OkData for CtrlReplyRulesetHistory {}

/// /services/{service_id}/ports/{port}/ruleset/history
///
/// Returns the full publish history for the port (newest first), relayed live
/// from the filter. Each entry carries the full `RuleSetRaw`, so the frontend
/// can implement rollback by re-publishing any past revision via PUT
/// /services/{service_id}/ports/{port}/ruleset; no dedicated rollback logic
/// exists server-side.
#[utoipa::path(
    get,
    path = "/services/{service_id}/ports/{port}/ruleset/history",
    params(
        ("service_id" = String, Path,                                                   description = "Service identifier"),
        ("port" = u16, Path,                                                            description = "Public port"),
    ),
    responses(
        (status = 200, description = "OK",                                              body = OkEnvelope<CtrlReplyRulesetHistory>),
        (status = 401, description = "Unauthorized",                                    body = ErrorEnvelope),
        (status = 404, description = "Unknown service or port",                         body = ErrorEnvelope),
        (status = 500, description = "Server error",                                    body = ErrorEnvelope),
        (status = 502, description = "Filter unreachable or returned error",            body = ErrorEnvelope),
        (status = 504, description = "Filter timed out",                                body = ErrorEnvelope)
    ),
    tags = ["controller", "ruleset"]
)]
pub async fn get_ruleset_history(
    OriginalUri(uri): OriginalUri,
    Path((service_id, port)): Path<(String, u16)>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<Value>) {
    // ── 1) Validate service id ──────────────────────────────────────────
    if !ensure_service_exists(&state, &service_id) {
        return ErrorEnvelope::not_found("service_not_found", uri.path())
            .with_title("Unknown service")
            .with_detail(format!("service_id '{service_id}' is not known"))
            .into_result();
    }

    // ── 2) Ensure snapshots cached so the port check is meaningful ───────
    if let Err(e) = ensure_service_or_fetch_filter_state(&state, &service_id, &state.http).await {
        return e.into_result(uri.path());
    }

    // ── 3) Validate port existence ──────────────────────────────────────
    if !ensure_port_exists(&state, &service_id, port) {
        return ErrorEnvelope::not_found("port_not_found", uri.path())
            .with_title("Unknown port")
            .with_detail(format!(
                "port '{port}' is not known for service '{service_id}'"
            ))
            .into_result();
    }

    // ── 4) Snapshot creds ───────────────────────────────────────────────
    let (base_url, token) = {
        let g = state.read();
        // Re-check under the lock: a concurrent delete could have removed the
        // service between the existence check above and here.
        let Some(svc) = g.services.get(&service_id) else {
            return ErrorEnvelope::not_found("service_not_found", uri.path())
                .with_title("Unknown service")
                .with_detail(format!("service_id '{service_id}' is not known"))
                .into_result();
        };
        (svc.cfg.base_url.clone(), svc.cfg.token.clone())
    };

    // ── 5) Relay GET to filter (live, not from the polled snapshot) ──────
    match get_ruleset_history_filter(&state.http, &base_url, &token, port).await {
        Ok(entries) => ok(CtrlReplyRulesetHistory { entries }),
        Err(ctrl_error) => ctrl_error.into_result(uri.path()),
    }
}

/// /services/{service_id}/ports/{port}/ruleset
///
/// Modifies the `RuleSetRaw` for the given service id and port on the filter, and updates the controller state accordingly.
///
/// The return types depend on the validity of the submitted `RuleSetRaw`:
/// - Successful rulesets return `OkEnvelope<CtrlReplyRulesetOkData>`;
/// - Invalid rulesets return an `ErrorEnvelope` (HTTP 400) with the validation logs in `detail`.
#[utoipa::path(
    put,
    path = "/services/{service_id}/ports/{port}/ruleset",
    params(
        ("service_id" = String, Path,                                               description = "Service identifier"),
        ("port" = u16, Path,                                                        description = "Public port"),
    ),
    request_body = RuleSetRaw,
    responses(
        (status = 200, description = "OK",                                          body = OkEnvelope<CtrlReplyRulesetOkData>),
        (status = 400, description = "Filter rejected ruleset",                     body = ErrorEnvelope),
        (status = 401, description = "Unauthorized",                                body = ErrorEnvelope),
        (status = 404, description = "Unknown service or port",                     body = ErrorEnvelope),
        (status = 500, description = "Server error",                                body = ErrorEnvelope),
        (status = 502, description = "Filter unreachable or returned error",        body = ErrorEnvelope),
        (status = 504, description = "Filter timed out",                            body = ErrorEnvelope)
    ),
    tags = ["controller", "ruleset"]
)]
pub async fn put_ruleset(
    OriginalUri(uri): OriginalUri,
    Path((service_id, port)): Path<(String, u16)>,
    State(state): State<ControllerState>,
    Json(body): Json<RuleSetRaw>,
) -> (StatusCode, Json<Value>) {
    // ── 1) Validate service & snapshots ─────────────────────────────────
    if !ensure_service_exists(&state, &service_id) {
        return ErrorEnvelope::not_found("service_not_found", uri.path())
            .with_title("Unknown service")
            .with_detail(format!("service_id '{service_id}' is not known"))
            .into_result();
    }

    if let Err(e) = ensure_service_or_fetch_filter_state(&state, &service_id, &state.http).await {
        return e.into_result(uri.path());
    }

    if !ensure_port_exists(&state, &service_id, port) {
        return ErrorEnvelope::not_found("port_not_found", uri.path())
            .with_title("Unknown port")
            .with_detail(format!(
                "port '{port}' is not known for service '{service_id}'"
            ))
            .into_result();
    }

    // ── 2) Snapshot creds ------------------------------------------------
    let (base_url, token) = {
        let g = state.read();
        // Re-check under the lock: a concurrent delete could have removed the
        // service between the existence check above and here.
        let Some(svc) = g.services.get(&service_id) else {
            return ErrorEnvelope::not_found("service_not_found", uri.path())
                .with_title("Unknown service")
                .with_detail(format!("service_id '{service_id}' is not known"))
                .into_result();
        };
        (svc.cfg.base_url.clone(), svc.cfg.token.clone())
    };

    // ── 3) Relay PUT to filter ------------------------------------------
    let (ruleset, ver_ruleset) =
        match put_ruleset_filter(&state.http, &base_url, &token, port, &body).await {
            Ok(t) => t,
            // A filter ruleset-validation rejection is surfaced as a standard
            // `ErrorEnvelope` (400, with the validation logs in `detail`), exactly
            // like the validate endpoint; every other relay failure goes through the
            // shared `CtrlErr` mapper (faithful status + detail).
            Err(CtrlErr::CustomEnvelope(env))
                if env.error.as_deref() == Some("ruleset_validation_error") =>
            {
                return ErrorEnvelope::bad_request("ruleset_validation_failed", uri.path())
                    .with_title("Filter rejected ruleset")
                    .with_detail(env.detail.clone().unwrap_or_default())
                    .into_result();
            }
            Err(ctrl_error) => return ctrl_error.into_result(uri.path()),
        };

    // ── 4) Update cached snapshot ---------------------------------------
    {
        let mut w = state.write();
        if let Some(p) = w
            .services
            .get_mut(&service_id)
            .and_then(|svc| svc.ports.get_mut(&port))
            && let Some(mut snap) = p.snap.take()
        {
            snap.ruleset = ruleset.clone();
            snap.ver_ruleset = ver_ruleset.clone();
            snap.ver_combined = calc_version_combined_copyover(&snap.ver_info, &ver_ruleset);
            p.snap = Some(snap);
        }
    }

    // ── 5) Reply to client ----------------------------------------------
    ok(CtrlReplyRulesetOkData {
        ruleset,
        version: ver_ruleset,
    })
}

/// /services/{service_id}/ports/{port}/ruleset/validate
///
/// Accepts a `RuleSetRaw` for the given service id and port on the filter, forwards it to the filter for validation,
/// and returns the validation result
///
/// The return types depend on the validity of the submitted `RuleSetRaw`:
/// - Successful rulesets returns `OkEnvelope<CtrlReplyRulesetOkData>`;
/// - Invalid rulesets return `ErrorEnvelope`.
#[utoipa::path(
    put,
    path = "/services/{service_id}/ports/{port}/ruleset/validate",
    params(
        ("service_id" = String, Path,                                                   description = "Service identifier"),
        ("port" = u16, Path,                                                            description = "Public port"),
    ),
    request_body = RuleSetRaw,
    responses(
        (status = 200, description = "OK",                                              body = OkEnvelope<CtrlReplyRulesetOkData>),
        (status = 400, description = "Filter rejected ruleset",                         body = ErrorEnvelope),
        (status = 401, description = "Unauthorized",                                    body = ErrorEnvelope),
        (status = 404, description = "Unknown service or port",                         body = ErrorEnvelope),
        (status = 500, description = "Server error",                                    body = ErrorEnvelope),
        (status = 502, description = "Filter unreachable or returned error",            body = ErrorEnvelope),
        (status = 504, description = "Filter timed out",                                body = ErrorEnvelope)
    ),
    tags = ["controller", "ruleset"]
)]
pub async fn put_validate_ruleset(
    OriginalUri(uri): OriginalUri,
    Path((service_id, port)): Path<(String, u16)>,
    State(state): State<ControllerState>,
    Json(body): Json<RuleSetRaw>,
) -> (StatusCode, Json<Value>) {
    // ── 1) Validate service/port (snapshots optional) ───────────────────
    if !ensure_service_exists(&state, &service_id) {
        return ErrorEnvelope::not_found("service_not_found", uri.path())
            .with_title("Unknown service")
            .with_detail(format!("service_id '{service_id}' is not known"))
            .into_result();
    }
    if !ensure_port_exists(&state, &service_id, port) {
        return ErrorEnvelope::not_found("port_not_found", uri.path())
            .with_title("Unknown port")
            .with_detail(format!(
                "port '{port}' is not known for service '{service_id}'"
            ))
            .into_result();
    }

    // ── 2) Snapshot creds ----------------------------------------------
    let (base_url, token) = {
        let g = state.read();
        // Re-check under the lock: a concurrent delete could have removed the
        // service between the existence check above and here.
        let Some(svc) = g.services.get(&service_id) else {
            return ErrorEnvelope::not_found("service_not_found", uri.path())
                .with_title("Unknown service")
                .with_detail(format!("service_id '{service_id}' is not known"))
                .into_result();
        };
        (svc.cfg.base_url.clone(), svc.cfg.token.clone())
    };

    // ── 3) Relay validate call to filter --------------------------------
    let (validated, _ruleset, maybe_err) =
        match validate_ruleset_filter(&state.http, &base_url, &token, port, &body).await {
            Ok(t) => t,
            Err(ctrl_error) => return ctrl_error.into_result(uri.path()),
        };

    if !validated {
        return ErrorEnvelope::bad_request("ruleset_validation_failed", uri.path())
            .with_title("Filter rejected ruleset")
            .with_detail(maybe_err.unwrap_or_else(|| "validation failed".into()))
            .into_result();
    }

    // ── 4) Success: echo back what we received (ruleset + version hash) ─
    // Version hash is meaningful only after install; we return placeholder.
    ok(CtrlReplyRulesetOkData {
        ruleset: body,
        version: "<not-installed>".into(),
    })
}
