// ──────────────────────────────────────────────────────────────
// Ports
// ──────────────────────────────────────────────────────────────

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use reqwest::Client;
use serde::Serialize;
use serde_json::json;
use shared::rules_model::PortInfo;
use crate::api::util::{ensure_ports_loaded, ensure_service_exists, ensure_service_port_exists};
use crate::state::ControllerState;

#[derive(Serialize)]
struct PortRow {
    port: u16,
    has_info: bool,
    has_ruleset: bool,
    next_logs_since: i64,
}

pub async fn list_ports(
    Path(svc): Path<String>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(resp) = ensure_service_exists(&state, &svc) {
        return resp;
    }

    // lazy load from filter if empty
    if let Err(e) = ensure_ports_loaded(&state, &svc, &Client::new()).await {
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({"status":"error","errors":[e.to_string()]})),
        );
    }

    let g = state.read();
    let Some(s) = g.services.get(&svc) else {
        return (StatusCode::NOT_FOUND, Json(json!({"status":"error","errors":["unknown service"]})));
    };

    let rows: Vec<PortRow> = s.ports.iter().map(|(p, port)| PortRow {
        port: *p,
        has_info: port.info.is_some(),
        has_ruleset: port.ruleset.is_some(),
        next_logs_since: port.next_logs_since,
    }).collect();

    (StatusCode::OK, Json(json!({"status":"ok","data": rows})))
}




// ──────────────────────────────────────────────────────────────
// Per-port CONFIG (proxy to filter)
// ──────────────────────────────────────────────────────────────

/// GET /services/{svc}/ports/{port}/config
pub async fn get_port_config(
    Path((svc, port)): Path<(String, u16)>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let svc_cfg = {
        let guard = state.read();
        let c = guard.services.get(&svc).unwrap();
        c.cfg.clone()
    };

    if let Err(resp) = ensure_service_exists(&state, &svc) {
        return resp;
    }


    let url = format!("{}/ports/{}/config", svc_cfg.base_url, port);
    let client = Client::new();
    match client
        .get(&url)
        .query(&[("token", svc_cfg.token.as_str())])
        .send()
        .await
    {
        Ok(resp) => {
            let code = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            match resp.json::<serde_json::Value>().await {
                Ok(body) => (code, Json(body)),
                Err(e) => (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({"status":"error","errors":[format!("invalid filter JSON: {e}")]})),
                ),
            }
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"status":"error","errors":[format!("upstream request failed: {e}")]})),
        ),
    }
}

/// PUT /services/{svc}/ports/{port}/config
pub async fn put_port_config(
    Path((svc, port)): Path<(String, u16)>,
    State(state): State<ControllerState>,
    Json(body): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    let svc_cfg = {
        let guard = state.read();
        let c = guard.services.get(&svc).unwrap();
        c.cfg.clone()
    };

    if let Err(resp) = ensure_service_exists(&state, &svc) {
        return resp;
    }


    let url = format!("{}/ports/{}/config", svc_cfg.base_url, port);
    let client = reqwest::Client::new();
    match client
        .put(&url)
        .query(&[("token", svc_cfg.token.as_str())])
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => {
            let code = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            match resp.json::<serde_json::Value>().await {
                Ok(body) => (code, Json(body)),
                Err(e) => (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({"status":"error","errors":[format!("invalid filter JSON: {e}")]})),
                ),
            }
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"status":"error","errors":[format!("upstream request failed: {e}")]})),
        ),
    }
}



// ──────────────────────────────────────────────────────────────
// Port info (proxy GET /ports/{port}/config on filter)
// ──────────────────────────────────────────────────────────────

pub async fn get_port_info(
    Path((svc, port)): Path<(String, u16)>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(resp) = ensure_service_port_exists(&state, &svc, port) {
        return resp;
    }

    // If we already have it, return cached
    if let Some(pi) = state.read().services.get(&svc).and_then(|s| s.ports.get(&port)).and_then(|p| p.info.clone()) {
        return (StatusCode::OK, Json(json!({"status":"ok","data": pi})));
    }

    // Fetch from filter and cache
    let (base, token) = {
        let g = state.read();
        let s = g.services.get(&svc).unwrap();
        (s.cfg.base_url.clone(), s.cfg.token.clone())
    };

    let url = format!("{base}/ports/{port}/config");
    let resp = Client::new()
        .get(&url)
        .query(&[("token", token.as_str())])
        .send().await;

    let Ok(resp) = resp else {
        return (StatusCode::BAD_GATEWAY, Json(json!({"status":"error","errors":["filter unreachable"]})));
    };

    if !resp.status().is_success() {
        return (StatusCode::BAD_GATEWAY, Json(json!({"status":"error","errors":[format!("filter returned {}", resp.status())]})));
    }

    let env = resp.json::<serde_json::Value>().await.unwrap_or(json!({"status":"error"}));
    if env.get("status") != Some(&json!("ok")) {
        return (StatusCode::BAD_GATEWAY, Json(json!({"status":"error","errors":["filter returned non-ok"]})));
    }

    // Expect {"status":"ok","data":{...PortInfo...}}
    let pi: PortInfo = serde_json::from_value(env.get("data").cloned().unwrap_or(json!({})))
        .map_err(|e| e.to_string())
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({"status":"error","errors":[e]}))))
        .unwrap();

    // cache
    state.write().services.get_mut(&svc).and_then(|s| s.ports.get_mut(&port)).map(|p| p.info = Some(pi.clone()));

    (StatusCode::OK, Json(json!({"status":"ok","data": pi})))
}

