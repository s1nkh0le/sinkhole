use crate::api::fetchers::put_port_config_http;
use crate::api::util::{ensure_port_exists, ensure_service_exists, ensure_snapshots};
use crate::state::ControllerState;
use axum::extract::{OriginalUri, Path, State};
use axum::http::StatusCode;
use axum::Json;
use reqwest::Client;
use serde::Serialize;
use serde_json::Value;
use shared::api::err::ErrorEnvelope;
use shared::api::ok::{ok, sealed, OkData, OkEnvelope};
use shared::types::ruleset_types::PortInfo;
use shared::utils::version::calc_version_combined_copyover;
use utoipa::ToSchema;
use shared::types::routing::filter::ports::FilterCtrlRequestPutBodyConfigUpdate;

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
        ("ctrl_token" = String, Query,                                          description = "Authentication token"),
        ("service_id" = String, Path,                                           description = "Service id")
    ),
    responses(
        (status = 200, description = "OK",                                      body = OkEnvelope<CtrlReplyPortListOkData>),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Unknown service/port or not cached",      body = ErrorEnvelope),
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
    if let Err(e) = ensure_snapshots(&state, &service_id, &Client::new()).await {
        return ErrorEnvelope::service_unavailable("state_unavailable", uri.path())
            .with_title("Ports not cached")
            .with_detail(e.to_string())
            .into_result();
    }

    // Build rows from current fetch
    let rows: Vec<u16> = {
        let g = state.read();
        let svc = g.services.get(&service_id).expect("checked above");
        svc.ports
            .iter()
            .map(|(p, port)| *p)
            .collect()
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
        ("ctrl_token" = String, Query,                                          description = "Authentication token"),
        ("service_id" = String, Path,                                           description = "Service id"),
        ("port"       = u16,    Path,                                           description = "Port number")
    ),
    responses(
        (status = 200, description = "OK",                                      body = OkEnvelope<CtrlReplyPortInfo>),
        (status = 400, description = "Invalid request",                         body = ErrorEnvelope),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Unknown service/port",                    body = ErrorEnvelope),
        (status = 502, description = "Upstream filter error",                   body = ErrorEnvelope),
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
    if let Err(e) = ensure_snapshots(&state, &service_id, &Client::new()).await {
        return ErrorEnvelope::service_unavailable("state_unavailable", uri.path())
            .with_title("Port config not cached")
            .with_detail(e.to_string())
            .into_result();
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
        g.services
            .get(&service_id)
            .and_then(|svc| svc.ports.get(&port))
            .and_then(|p| p.snap.as_ref())
            .map(|snap| snap.info.clone())
            .expect("checked with ensure_port_exists")
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
        ("ctrl_token" = String, Query,                                          description = "Authentication token"),
        ("service_id" = String, Path,                                           description = "Service id"),
        ("port"       = u16,    Path,                                           description = "Port number")
    ),
    responses(
        (status = 200, description = "OK",                                      body = OkEnvelope<CtrlReplyPortInfo>),
        (status = 400, description = "Invalid request",                         body = ErrorEnvelope),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Unknown service/port",                    body = ErrorEnvelope),
        (status = 502, description = "Upstream filter error",                   body = ErrorEnvelope),
        (status = 500, description = "Server error",                            body = ErrorEnvelope),
    ),
    tags = ["controller", "ports"]
)]
pub async fn put_port_config(
    OriginalUri(uri): OriginalUri,
    Path((service_id, port)): Path<(String, u16)>,
    State(state): State<ControllerState>,
    Json(body): Json<FilterCtrlRequestPutBodyConfigUpdate>,
) -> (StatusCode, Json<Value>) {
    // ── 1) Validate service & port existence (cached or not) ────────────
    if !ensure_service_exists(&state, &service_id) {
        return ErrorEnvelope::not_found("service_not_found", uri.path())
            .with_title("Unknown service")
            .with_detail(format!("service_id '{service_id}' is not known"))
            .into_result();
    }

    // We call ensure_snapshots so we have a snapshot to update afterward
    if let Err(e) = ensure_snapshots(&state, &service_id, &Client::new()).await {
        return ErrorEnvelope::service_unavailable("state_unavailable", uri.path())
            .with_title("Port config not cached")
            .with_detail(e.to_string())
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

    // ── 2) Snapshot service creds --------------------------------------
    let (base_url, token) = {
        let g = state.read();
        let s = g.services.get(&service_id).expect("validated");
        (s.cfg.base_url.clone(), s.cfg.token.clone())
    };

    // ── 3) Relay PUT to filter -----------------------------------------
    let (info, ver_info) = match put_port_config_http(&Client::new(), &base_url, &token, port, &body, ).await {
        Ok(t) => t,
        Err(err) => {
            return ErrorEnvelope::bad_gateway("filter_response_error", uri.path())
                .with_title("Filter unreachable")
                .with_detail(err.to_string())
                .into_result();
        }
    };

    // ── 4) Update cached snapshot --------------------------------------
    {
        let mut w = state.write();
        if let Some(svc) = w.services.get_mut(&service_id) {
            if let Some(p) = svc.ports.get_mut(&port) {
                if let Some(mut snap) = p.snap.take() {
                    snap.info       = info.clone();
                    snap.ver_info   = ver_info.clone();
                    snap.ver_combined = calc_version_combined_copyover(&ver_info, &snap.ver_ruleset);
                    p.snap = Some(snap);
                }
            }
        }
    }

    ok(CtrlReplyPortInfo { info })
}
