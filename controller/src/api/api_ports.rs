use crate::api::util::{get_loaded_port_or_fetch};
use crate::state::ControllerState;
use axum::extract::{OriginalUri, Path, State};
use axum::http::StatusCode;
use axum::Json;
use reqwest::Client;
use serde::Serialize;
use serde_json::{json, Value};
use tracing::warn;
use shared::api::err::ErrorEnvelope;
use shared::api::ok::{ok, OkData, OkEnvelope, sealed};
use shared::rules_model::PortInfo;
use utoipa::{ToSchema, IntoParams};

/// Single Service information
#[derive(Serialize, ToSchema)]
struct PortRow {
    port: u16,
    has_info: bool,
    has_ruleset: bool,
    next_logs_since: i64,
}

/// Successful port listing data
#[derive(Serialize, ToSchema)]
struct PortListOkData {
    rows: Vec<PortRow>,
}
impl sealed::Sealed for PortListOkData {}
impl OkData for PortListOkData {}

#[utoipa::path(
    get,
    path = "/services/{service_id}/ports",
    params(
        ("ctrl_token" = String, description = "Authentication token"),
        ("service_id" = String, Path, description = "Service id")
    ),
    responses(
        (status = 200, description = "OK", body = OkEnvelope<PortInfoOkData>),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Unknown service/port or not cached", body = ErrorEnvelope),
        (status = 500, description = "Server error", body = ErrorEnvelope),
    ),
    tag = "controller"
)]
pub async fn get_list_ports(
    OriginalUri(uri): OriginalUri,
    Path(service_id): Path<String>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<Value>) {
    // Ensure service exists
    {
        let g = state.read();
        if !g.services.contains_key(&service_id) {
            return ErrorEnvelope::not_found("sanitization", uri.path())
                .with_title("Unknown service")
                .with_detail(format!("service_id '{service_id}' is not known"))
                .into_result();
        }
    }

    // Lazy-load from filter if empty (best-effort)
    if let Err(e) = get_loaded_port_or_fetch(&state, &service_id, &Client::new()).await {
        return ErrorEnvelope::service_unavailable("port_info_unavailable", uri.path())
            .with_title("Ports not loaded on controller")
            .with_detail(format!("[list_ports(), {}] ensure_ports_loaded(service_id={}) failed, error={}", uri.path(), service_id, e.to_string()))
            .into_result();
    }

    // Build rows from current state
    let rows: Vec<PortRow> = {
        let g = state.read();
        let svc = g.services.get(&service_id).expect("checked above");
        svc.ports
            .iter()
            .map(|(p, port)| PortRow {
                port: *p,
                has_info: port.info.is_some(),
                has_ruleset: port.ruleset.is_some(),
                next_logs_since: port.next_logs_since,
            })
            .collect()
    };

    ok(PortListOkData { rows })
}

/// Successful single port info reply
#[derive(Serialize, ToSchema)]
struct PortInfoOkData {
    port_info: PortInfo,
}
impl sealed::Sealed for PortInfoOkData {}
impl OkData for PortInfoOkData {}


#[utoipa::path(
    get,
    path = "/services/{service_id}/ports/{port}/config",
    params(
        ("ctrl_token" = String,         description = "Authentication token"),
        ("service_id" = String, Path,   description = "Service id"),
        ("port"       = u16,    Path,   description = "Port number")
    ),
    responses(
        (status = 200, description = "OK", body = OkEnvelope<PortInfoOkData>),
        (status = 400, description = "Invalid request", body = ErrorEnvelope),
        (status = 401, description = "Unauthorized", body = ErrorEnvelope),
        (status = 404, description = "Unknown service/port", body = ErrorEnvelope),
        (status = 502, description = "Upstream filter error", body = ErrorEnvelope),
        (status = 500, description = "Server error", body = ErrorEnvelope),
    ),
    tag = "controller"
)]
pub async fn get_port_config(
    OriginalUri(uri): OriginalUri,
    Path((service_id, port)): Path<(String, u16)>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<Value>) {
    // Check service & (best-effort) port existence in controller state
    {
        let g = state.read();
        let Some(svc) = g.services.get(&service_id) else {
            return ErrorEnvelope::not_found("sanitization", uri.path())
                .with_title("Unknown service")
                .with_detail(format!("service_id '{service_id}' is not known"))
                .into_result();
        };
        if !svc.ports.contains_key(&port) {
            // We allow lazy load below; this branch is only to give a clearer 404
            // when the controller is sure it doesn't know this port yet.
            // If you prefer best-effort fetch without 404, remove this check.
            return ErrorEnvelope::not_found("sanitization", uri.path())
                .with_title("Unknown port")
                .with_detail(format!("port '{port}' is not known for service '{service_id}'"))
                .into_result();
        }
    }

    // If cached, return immediately
    if let Some(info) = state
        .read()
        .services
        .get(&service_id)
        .and_then(|s| s.ports.get(&port))
        .and_then(|p| p.info.clone())
    {
        return ok(PortInfoOkData { port_info: info });
    }

    warn!("[get_port_config(service_id={}, port={}), GET {}] cache miss, fetching port config from filter", service_id, port, uri.path());


    // Fetch from filter, cache, and return
    let (base, token) = {
        let g = state.read();
        let s = g.services.get(&service_id).expect("checked above");
        (s.cfg.base_url.clone(), s.cfg.token.clone())
    };

    let url = format!("{base}/ports/{port}/config");
    let resp = Client::new()
        .get(&url)
        .query(&[("token", token.as_str())])
        .send()
        .await;

    let Ok(resp) = resp else {
        return ErrorEnvelope::bad_gateway("filter_response_error", uri.path())
            .with_title("Filter unreachable")
            .with_detail(format!("[get_port_info(service_id={}, port={}), GET {}] The request to the filter API 'GET {}' could not be compleated | can't connect to the filter", service_id, port, uri.path(), url))
            .into_result();
    };

    if !resp.status().is_success() {
        return ErrorEnvelope::bad_gateway("filter_response_error", uri.path())
            .with_title("Filter response unsuccessful")
            .with_detail(format!("[get_port_info(service_id={}, port={}), GET {}] The request to the filter API 'GET {}' returned an error status of {} | ", service_id, port, uri.path(), url, resp.status()))
            .into_result();
    }

    // Expect {"status":"ok","data":{...PortInfo...}}
    let env = match resp.json::<Value>().await {
        Ok(v) => v,
        Err(e) => {
            return ErrorEnvelope::bad_gateway("filter_response_error", uri.path())
                .with_title("Error parsing filter response")
                .with_detail(format!("[get_port_info(service_id={}, port={}), GET {}] The request to the filter API 'GET {}' returned, but has an invalid json content | error={}", service_id, port, uri.path(), url, e))
                .into_result();
        }
    };
    if env.get("status") != Some(&json!("ok")) {
        return ErrorEnvelope::bad_gateway("filter_response_error", uri.path())
            .with_title("Filter returned non-ok payload")
            .with_detail(format!("[get_port_info(service_id={}, port={}), GET {}] The request to the filter API 'GET {}' returned with an error status", service_id, port, uri.path(), url))
            .into_result();
    }
    let info: PortInfo = match serde_json::from_value(env.get("data").cloned().unwrap_or(json!({}))) {
        Ok(pi) => pi,
        Err(e) => {
            return ErrorEnvelope::bad_gateway("filter_response_error", uri.path())
                .with_title("Error parsing filter response")
                .with_detail(format!("[get_port_info(service_id={}, port={}), GET {}] The request to the filter API 'GET {}' returned a successful response, but could not parse its json content into PortInfo | error={}", service_id, port, uri.path(), url, e))
                .into_result();
        }
    };

    // Update cache with fetched port info
    {
        let mut w = state.write();
        if let Some(svc) = w.services.get_mut(&service_id) {
            if let Some(p) = svc.ports.get_mut(&port) {
                p.info = Some(info.clone());
            }
        }
    }

    ok(PortInfoOkData { port_info: info })
}



#[utoipa::path(
    get,
    path = "/services/{service_id}/ports/{port}/config/cached",
    params(
        ("ctrl_token" = String,         description = "Authentication token"),
        ("service_id" = String, Path,   description = "Service id"),
        ("port"       = u16,    Path,   description = "Port number")
    ),
    responses(
        (status = 200, description = "OK", body = OkEnvelope<PortInfoOkData>),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Unknown service/port or not cached", body = ErrorEnvelope),
        (status = 500, description = "Server error", body = ErrorEnvelope),
        (status = 503, description = "Service Unavailable", body = ErrorEnvelope),
    ),
    tag = "controller"
)]
pub async fn get_port_config_cached(
    OriginalUri(uri): OriginalUri,
    Path((service_id, port)): Path<(String, u16)>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<Value>) {
    // Validate service exists
    let guard = state.read();
    let Some(svc) = guard.services.get(&service_id) else {
        return ErrorEnvelope::not_found("sanitization", uri.path())
            .with_title("Unknown service")
            .with_detail(format!("servi ce_id '{service_id}' is not known"))
            .into_result();
    };

    // Validate port exists (best-effort against controller state)
    let Some(p) = svc.ports.get(&port) else {
        return ErrorEnvelope::not_found("sanitization", uri.path())
            .with_title("Unknown port")
            .with_detail(format!("port '{port}' is not known for service '{service_id}'"))
            .into_result();
    };

    // Return cached PortInfo if present
    if let Some(info) = p.info.clone() {
        return ok(PortInfoOkData { port_info: info });
    }

    // Known service+port, but cache miss
    ErrorEnvelope::service_unavailable("cache_miss", uri.path())
        .with_title("Port info not cached")
        .with_detail(format!(
            "Port info for service_id={service_id} and port={port} is not available in controller cache.\
             Call /services/{service_id}/ports/{port}/info to fetch manually from the filter, or wait for the sync daemon to pull it from the filter.\
             If you keep getting this error, the filter's syncer may be inoperative."
        ))
        .into_result()
}




#[utoipa::path(
    put,
    path = "/services/{service_id}/ports/{port}/config",
    params(
        ("ctrl_token" = String,         description = "Authentication token"),
        ("service_id" = String, Path,   description = "Service id"),
        ("port"       = u16,    Path,   description = "Port number")
    ),
    responses(
        (status = 200, description = "OK", body = OkEnvelope<PortInfoOkData>),
        (status = 400, description = "Invalid request", body = ErrorEnvelope),
        (status = 401, description = "Unauthorized", body = ErrorEnvelope),
        (status = 404, description = "Unknown service/port", body = ErrorEnvelope),
        (status = 502, description = "Upstream filter error", body = ErrorEnvelope),
        (status = 500, description = "Server error", body = ErrorEnvelope),
    ),
    tag = "controller"
)]
pub async fn put_port_config(
    OriginalUri(uri): OriginalUri,
    Path((service_id, port)): Path<(String, u16)>,
    State(state): State<ControllerState>,
    Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
    let svc_cfg = {
        let g = state.read();
        let Some(svc) = g.services.get(&service_id) else {
            return ErrorEnvelope::not_found("sanitization", uri.path())
                .with_title("Unknown service")
                .with_detail(format!("service_id '{service_id}' is not known"))
                .into_result();
        };
        svc.cfg.clone()
    };

    let url = format!("{}/ports/{}/config", svc_cfg.base_url, port);
    match Client::new()
        .put(&url)
        .query(&[("token", svc_cfg.token.as_str())])
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => {
            let code = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);

            if code != StatusCode::OK {
                return ErrorEnvelope::bad_gateway("filter_response_error", uri.path())
                    .with_title("Filter returned non-ok payload")
                    .with_detail(format!("[put_port_config(service_id={}, port={}), GET {}] The request to the filter API 'PUT {}' returned with an status_code={}", service_id, port, uri.path(), url, code))
                    .into_result();
            }

            match resp.json::<Value>().await {
                Ok(body) => {
                    let info: PortInfo = match serde_json::from_value(body.get("data").cloned().unwrap_or(json!({}))) {
                        Ok(pi) => pi,
                        Err(e) => {
                            return ErrorEnvelope::bad_gateway("filter_response_error", uri.path())
                                .with_title("Error parsing filter response")
                                .with_detail(format!("[put_port_config(service_id={}, port={}), PUT {}] The request to the filter API 'PUT {}' returned a successful response, but could not parse its json content into PortInfo | error={}", service_id, port, uri.path(), url, e))
                                .into_result();
                        }
                    };

                    ok(PortInfoOkData {
                        port_info: info
                    })
                },
                Err(e) => ErrorEnvelope::bad_gateway("filter_response_error", uri.path())
                    .with_title("Error parsing filter response")
                    .with_detail(format!("[put_port_config(service_id={}, port={}), PUT {}] The request to the filter API 'PUT {}' returned, but has an invalid json content | error={}", service_id, port, uri.path(), url, e))
                    .into_result()
            }
        }
        Err(e) => ErrorEnvelope::bad_gateway("filter_response_error", uri.path())
            .with_title("Filter unreachable")
            .with_detail(format!("[put_port_config(service_id={}, port={}), PUT {}] The request to the filter API 'PUT {}' could not be compleated | can't connect to the filter", service_id, port, uri.path(), url))
            .into_result()
    }
}
