use crate::api::fetchers::put_port_config_http;
use crate::api::util::{
    ensure_port_exists, ensure_service_exists, ensure_service_or_fetch_filter_state,
};
use crate::state::ctrl::ControllerState;
use axum::Json;
use axum::extract::{OriginalUri, Path, State};
use axum::http::StatusCode;
use serde::Serialize;
use serde_json::Value;
use shared::api::err::ErrorEnvelope;
use shared::api::ok::{OkData, OkEnvelope, ok, sealed};
use shared::types::routing::filter::ports::FilterRequestPutBodyConfigUpdate;
use shared::types::ruleset_types::PortInfo;
use shared::utils::version::calc_version_combined_copyover;
use utoipa::ToSchema;

/// Successful port listing data
#[derive(Serialize, ToSchema)]
struct CtrlReplyPortListOkData {
    ports: Vec<u16>,
}
impl sealed::Sealed for CtrlReplyPortListOkData {}
impl OkData for CtrlReplyPortListOkData {}

/// /services/{service_id}/ports
///
/// Returns a brief listing of the ports for a given service
#[utoipa::path(
    get,
    path = "/services/{service_id}/ports",
    params(
        ("service_id" = String, Path,                                           description = "Service id")
    ),
    responses(
        (status = 200, description = "OK",                                      body = OkEnvelope<CtrlReplyPortListOkData>),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Unknown service",                         body = ErrorEnvelope),
        (status = 502, description = "Filter unreachable or returned an error",  body = ErrorEnvelope),
        (status = 504, description = "Filter timed out",                        body = ErrorEnvelope),
        (status = 500, description = "Server error",                            body = ErrorEnvelope),
    ),
    tags = ["controller", "ports"]
)]
pub async fn get_list_ports(
    OriginalUri(uri): OriginalUri,
    Path(service_id): Path<String>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<Value>) {
    if !ensure_service_exists(&state, &service_id) {
        return ErrorEnvelope::not_found("service_not_found", uri.path())
            .with_title("Unknown service")
            .with_detail(format!("service_id '{service_id}' is not known"))
            .into_result();
    }

    // Ensure snapshots (one-shot fetch if needed)
    if let Err(e) = ensure_service_or_fetch_filter_state(&state, &service_id, &state.http).await {
        return e.into_result(uri.path());
    }

    // Build rows from current fetch
    let rows: Vec<u16> = {
        let g = state.read();
        // Re-check under the lock: the service could have been removed between the
        // existence check above and here (concurrent delete via the API/supervisor).
        let Some(svc) = g.services.get(&service_id) else {
            return ErrorEnvelope::not_found("service_not_found", uri.path())
                .with_title("Unknown service")
                .with_detail(format!("service_id '{service_id}' is not known"))
                .into_result();
        };
        svc.ports.keys().copied().collect()
    };

    ok(CtrlReplyPortListOkData { ports: rows })
}

/// Successful single port info reply
#[derive(Serialize, ToSchema)]
struct CtrlReplyPortInfo {
    info: PortInfo,
}
impl sealed::Sealed for CtrlReplyPortInfo {}
impl OkData for CtrlReplyPortInfo {}

/// /services/{service_id}/ports/{port}/config
///
/// Returns the `PortInfo` and its version for the given service id and port
#[utoipa::path(
    get,
    path = "/services/{service_id}/ports/{port}/config",
    params(
        ("service_id" = String, Path,                                           description = "Service id"),
        ("port"       = u16,    Path,                                           description = "Port number")
    ),
    responses(
        (status = 200, description = "OK",                                      body = OkEnvelope<CtrlReplyPortInfo>),
        (status = 400, description = "Invalid request",                         body = ErrorEnvelope),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Unknown service/port",                    body = ErrorEnvelope),
        (status = 502, description = "Upstream filter error",                   body = ErrorEnvelope),
        (status = 504, description = "Filter timed out",                        body = ErrorEnvelope),
        (status = 500, description = "Server error",                            body = ErrorEnvelope),
    ),
    tags = ["controller", "ports"]
)]
pub async fn get_port_config(
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

    // ── 2) Ensure snapshots cached (one-shot remote fetch if needed) ────
    if let Err(e) = ensure_service_or_fetch_filter_state(&state, &service_id, &state.http).await {
        return e.into_result(uri.path());
    }

    // ── 3) Confirm the port exists after caching attempt ────────────────
    if !ensure_port_exists(&state, &service_id, port) {
        return ErrorEnvelope::not_found("port_not_found", uri.path())
            .with_title("Unknown port")
            .with_detail(format!(
                "port '{port}' is not known for service '{service_id}'"
            ))
            .into_result();
    }

    // ── 4) Extract PortInfo from snapshot and reply ─────────────────────
    let info = {
        let g = state.read();
        // Re-check under the lock: the service/port could have been removed
        // between `ensure_port_exists` above and here.
        let Some(info) = g
            .services
            .get(&service_id)
            .and_then(|svc| svc.ports.get(&port))
            .and_then(|p| p.snap.as_ref())
            .map(|snap| snap.info.clone())
        else {
            return ErrorEnvelope::not_found("port_not_found", uri.path())
                .with_title("Unknown port")
                .with_detail(format!(
                    "port '{port}' is not known for service '{service_id}'"
                ))
                .into_result();
        };
        info
    };

    ok(CtrlReplyPortInfo { info })
}

/// /services/{service_id}/ports/{port}/config
///
/// Modifies the `PortInfo` for the given service id and port on the filter, and updates the controller state accordingly
#[utoipa::path(
    put,
    path = "/services/{service_id}/ports/{port}/config",
    params(
        ("service_id" = String, Path,                                           description = "Service id"),
        ("port"       = u16,    Path,                                           description = "Port number")
    ),
    responses(
        (status = 200, description = "OK",                                      body = OkEnvelope<CtrlReplyPortInfo>),
        (status = 400, description = "Invalid request",                         body = ErrorEnvelope),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Unknown service/port",                    body = ErrorEnvelope),
        (status = 502, description = "Filter unreachable or returned an error",  body = ErrorEnvelope),
        (status = 504, description = "Filter timed out",                        body = ErrorEnvelope),
        (status = 500, description = "Server error",                            body = ErrorEnvelope),
    ),
    tags = ["controller", "ports"]
)]
pub async fn put_port_config(
    OriginalUri(uri): OriginalUri,
    Path((service_id, port)): Path<(String, u16)>,
    State(state): State<ControllerState>,
    Json(body): Json<FilterRequestPutBodyConfigUpdate>,
) -> (StatusCode, Json<Value>) {
    // ── 1) Validate service & port existence (cached or not) ────────────
    if !ensure_service_exists(&state, &service_id) {
        return ErrorEnvelope::not_found("service_not_found", uri.path())
            .with_title("Unknown service")
            .with_detail(format!("service_id '{service_id}' is not known"))
            .into_result();
    }

    // We call ensure_snapshots so we have a snapshot to update afterward
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

    // ── 2) Snapshot service creds --------------------------------------
    let (base_url, token) = {
        let g = state.read();
        // Re-check under the lock: a concurrent delete could have removed the
        // service between the existence checks above and here.
        let Some(s) = g.services.get(&service_id) else {
            return ErrorEnvelope::not_found("service_not_found", uri.path())
                .with_title("Unknown service")
                .with_detail(format!("service_id '{service_id}' is not known"))
                .into_result();
        };
        (s.cfg.base_url.clone(), s.cfg.token.clone())
    };

    // ── 3) Relay PUT to filter -----------------------------------------
    let (info, ver_info) =
        match put_port_config_http(&state.http, &base_url, &token, port, &body).await {
            Ok(t) => t,
            // Propagate the filter's real status/detail (e.g. a 400 scope-validation
            // error stays a 400) instead of flattening everything to a 502.
            Err(err) => return err.into_result(uri.path()),
        };

    // ── 4) Update cached snapshot --------------------------------------
    {
        let mut w = state.write();
        if let Some(svc) = w.services.get_mut(&service_id)
            && let Some(p) = svc.ports.get_mut(&port)
            && let Some(mut snap) = p.snap.take()
        {
            snap.info = info.clone();
            snap.ver_info = ver_info.clone();
            snap.ver_combined = calc_version_combined_copyover(&ver_info, &snap.ver_ruleset);
            p.snap = Some(snap);
        }
    }

    ok(CtrlReplyPortInfo { info })
}
