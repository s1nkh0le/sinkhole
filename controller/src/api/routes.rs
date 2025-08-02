use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::log::{error, warn};
use shared::rules_model::{PortInfo, RuleSetRaw};
use crate::{
    db::query,
    state::{ControllerState, Service},
};
use crate::api::util::{ensure_ports_loaded, ensure_service_exists, ensure_service_port_exists};




pub fn build_router(state: ControllerState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/services", get(list_services))
        .route("/services/:svc/ports", get(list_ports))
        .route("/services/:svc/ports/:port/info", get(get_port_info))
        .route("/services/:svc/ports/:port/ruleset", get(get_ruleset).put(put_ruleset))
        .route("/services/:svc/ecam", get(get_ecam))
        .route("/services/:svc/ports/:port/logs", get(get_logs))
        .with_state(state)
}


// ──────────────────────────────────────────────────────────────
// Common envelopes
// ──────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ApiOk<T> { status: &'static str, data: T }
impl<T> ApiOk<T> { fn ok(data: T) -> Self { Self { status: "ok", data } } }


// ──────────────────────────────────────────────────────────────
// Health & service list
// ──────────────────────────────────────────────────────────────

pub async fn health() -> (StatusCode, &'static str) {
    (StatusCode::OK, "ok")
}

pub async fn list_services(State(state): State<ControllerState>) -> Json<serde_json::Value> {
    let inner = state.read();
    let list = inner
        .services
        .iter()
        .map(|(id, svc)| {
            json!({
                "id": id,
                "url": svc.cfg.base_url,
                "healthy": svc.healthy,
                "last_sync_ns": svc.last_sync_ns
            })
        })
        .collect::<Vec<_>>();

    Json(json!({ "status":"ok", "data": list }))
}


// ──────────────────────────────────────────────────────────────
// Ports
// ──────────────────────────────────────────────────────────────

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












// ──────────────────────────────────────────────────────────────
// Ruleset (proxy GET/PUT /ports/{port}/ruleset on filter)
// ──────────────────────────────────────────────────────────────

pub async fn get_ruleset(
    Path((svc, port)): Path<(String, u16)>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(resp) = ensure_service_port_exists(&state, &svc, port) {
        return resp;
    }

    if let Some(rs) = state.read().services.get(&svc).and_then(|s| s.ports.get(&port)).and_then(|p| p.ruleset.clone()) {
        return (StatusCode::OK, Json(json!({"status":"ok","data": rs})));
    }

    let (base, token) = {
        let g = state.read();
        let s = g.services.get(&svc).unwrap();
        (s.cfg.base_url.clone(), s.cfg.token.clone())
    };

    let url = format!("{base}/ports/{port}/ruleset");
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

    let rs: RuleSetRaw = serde_json::from_value(env.get("data").cloned().unwrap_or(json!({})))
        .map_err(|e| e.to_string())
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({"status":"error","errors":[e]}))))
        .unwrap();

    state.write().services.get_mut(&svc).and_then(|s| s.ports.get_mut(&port)).map(|p| p.ruleset = Some(rs.clone()));

    (StatusCode::OK, Json(json!({"status":"ok","data": rs})))
}

pub async fn put_ruleset(
    Path((svc, port)): Path<(String, u16)>,
    State(state): State<ControllerState>,
    Json(body): Json<RuleSetRaw>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(resp) = ensure_service_port_exists(&state, &svc, port) {
        return resp;
    }

    let (base, token) = {
        let g = state.read();
        let s = g.services.get(&svc).unwrap();
        (s.cfg.base_url.clone(), s.cfg.token.clone())
    };

    let url = format!("{base}/ports/{port}/ruleset");
    let resp = Client::new()
        .put(&url)
        .query(&[("token", token.as_str())])
        .json(&body)
        .send().await;

    let Ok(resp) = resp else {
        return (StatusCode::BAD_GATEWAY, Json(json!({"status":"error","errors":["filter unreachable"]})));
    };

    // Bubble up filter validation errors
    let payload = resp.json::<serde_json::Value>().await.unwrap_or(json!({"status":"error"}));
    if payload.get("status") != Some(&json!("ok")) {
        return (StatusCode::BAD_REQUEST, Json(payload));
    }

    // cache on success
    state.write().services.get_mut(&svc).and_then(|s| s.ports.get_mut(&port)).map(|p| p.ruleset = Some(body));

    (StatusCode::OK, Json(payload))
}














/// ----------------------------------------------------------------
///  /services/{svc}/ecam?since=…&limit=…
/// ----------------------------------------------------------------
#[derive(Deserialize, Debug)]
pub struct EcamQuery {
    #[serde(default)]
    pub since: Option<i64>,
    #[serde(default)]
    pub limit: Option<usize>,
}


pub async fn get_ecam(
    Path(svc): Path<String>,
    Query(q): Query<EcamQuery>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let since = q.since.unwrap_or(0);
    let limit = q.limit.unwrap_or(1_000).min(10_000);

    let db = state.db();

    if let Err(resp) = ensure_service_exists(&state, &svc) {
        return resp;
    }

    // blocking SQL in spawn_blocking
    let join_res = tokio::task::spawn_blocking({
        let db_path = db.path.clone();
        let svc = svc.clone();
        move || query::load_ecam(&db_path, &svc, since, limit)
    })
    .await;

    match join_res {
        Ok(Ok((evts, next_since))) => (
            StatusCode::OK,
            Json(json!({
                "status":"ok",
                "data": evts,
                "next_since": next_since
            })),
        ),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "status":"error", "errors":[e.to_string()] })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "status":"error", "errors":[e.to_string()] })),
        ),
    }
}






/// ----------------------------------------------------------------
///  /services/{svc}/ports/{port}/logs?since=&limit=
/// ----------------------------------------------------------------
#[derive(Debug, Copy, Clone, Deserialize, Serialize)]
pub struct LogsQuery {
    #[serde(default)]
    since: Option<i64>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct LogsPath {
    svc: String, // must match `{svc}` in the route
    port: u16
}

pub async fn get_logs(
    Path((svc, port)): Path<(String, u16)>,
    Query(q): Query<LogsQuery>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let since = q.since.unwrap_or(0);
    let limit = q.limit.unwrap_or(1000);

    if let Err(resp) = ensure_service_port_exists(&state, &svc, port) {
        return resp;
    }

    let db = state.db();
    let join_res = tokio::task::spawn_blocking({
        let db_path = db.path.clone();
        let svc = svc.clone();
        move || query::load_records(&db_path, &svc, port, since, limit)
    })
        .await;

    let resp: (StatusCode, Json<serde_json::Value>) = match join_res {
        Ok(Ok((recs, next_since))) => (
            StatusCode::OK,
            Json(json!({
                "status":"ok",
                "data": {
                    "records": recs,
                    "next_since": next_since
                }
            })),
        ),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "status":"error", "errors":[e.to_string()] })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "status":"error", "errors":[e.to_string()] })),
        ),
    };

    return resp;
}
