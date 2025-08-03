use crate::api::api_cached::{get_cached_port_config, get_cached_port_ruleset};
use crate::api::util::{ensure_ports_loaded, ensure_service_exists, ensure_service_port_exists};
use crate::{db, state::ControllerState};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use shared::rules_model::{PortInfo, RuleSetRaw};


pub fn build_router(state: ControllerState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/services", get(list_services))
        .route("/services/{svc}/ports", get(list_ports))
        .route("/services/{svc}/ports/{port}/info", get(get_port_info))
        .route("/services/{svc}/ports/{port}/ruleset", get(get_ruleset).put(put_ruleset))
        .route("/services/{svc}/ecam", get(get_ecam))
        .route("/services/{svc}/ports/{port}/logs", get(get_records))
        .route("/services/{svc}/ports/{port}/config", get(get_port_config).put(put_port_config))
        .route("/services/{svc}/ports/{port}/config/cached", get(get_cached_port_config))
        .route("/services/{svc}/ports/{port}/ruleset/cached", get(get_cached_port_ruleset))
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
            serde_json::json!({
                "id": id,
                "url": svc.cfg.base_url,
                "healthy": svc.healthy,
                "last_sync_ns": svc.last_sync_ns,
                "ports": svc.ports.keys().collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();

    Json(serde_json::json!({ "status":"ok", "data": list }))
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













// ──────────────────────────────────────────────────────────────
// Aggregated ECAM & Records (controller DB)
// ──────────────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
pub struct EcamQuery {
    pub service_id: Option<String>,
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub limit: Option<usize>,
    pub order: Option<String>, // "asc" | "desc"
}

/// GET /ecam?service_id=&since=&until=&limit=&order=
pub async fn get_ecam(
    Query(q): Query<EcamQuery>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<serde_json::Value>) {
    // defaults
    let since = q.since.unwrap_or(0);
    let until = q.until.unwrap_or(i64::MAX);
    let limit = q.limit.unwrap_or(1000).min(10_000);
    let asc = q
        .order
        .as_deref()
        .map(|s| s.eq_ignore_ascii_case("asc"))
        .unwrap_or(true);

    // optional: validate service exists if provided
    if let Some(ref svc_id) = q.service_id {
        if !state.read().services.contains_key(svc_id) {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"status":"error","errors":["unknown service_id"]})),
            );
        }
    }

    // query DB (blocking)
    let db = state.db();
    let svc = q.service_id.clone();
    let join_res = tokio::task::spawn_blocking(move || {
        db::query::load_ecam_agg(&db.path, svc.as_deref(), since, until, limit, asc)
    })
        .await;

    match join_res {
        Ok(Ok((events, next_since))) => (
            StatusCode::OK,
            Json(json!({"status":"ok","data": events, "next_since": next_since})),
        ),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"status":"error","errors":[e.to_string()]})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"status":"error","errors":[format!("join error: {e}")]})),
        ),
    }
}




#[derive(Deserialize, Debug)]
pub struct RecordsQuery {
    pub service_id: Option<String>,
    pub port: Option<u16>,
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub limit: Option<usize>,
    pub order: Option<String>, // "asc" | "desc"
    // Future: decision/dir/reason (requires JSON1 or columnization)
}

/// GET /records?service_id=&port=&since=&until=&limit=&order=
pub async fn get_records(
    Query(q): Query<RecordsQuery>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<serde_json::Value>) {
    // defaults
    let since = q.since.unwrap_or(0);
    let until = q.until.unwrap_or(i64::MAX);
    let limit = q.limit.unwrap_or(1000).min(10_000);
    let asc = q
        .order
        .as_deref()
        .map(|s| s.eq_ignore_ascii_case("asc"))
        .unwrap_or(true);

    // validate service if provided
    if let Some(ref svc_id) = q.service_id {
        if !state.read().services.contains_key(svc_id) {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"status":"error","errors":["unknown service_id"]})),
            );
        }
        // if a port is provided, ensure controller knows it (best-effort)
        if let Some(p) = q.port {
            let guard = state.read();
            if let Some(svc_ref) = guard.services.get(svc_id) {
                if !svc_ref.ports.contains_key(&p) {
                    return (
                        StatusCode::NOT_FOUND,
                        Json(json!({"status":"error","errors":["unknown port for service"]})),
                    );
                }
            }
        }
    }

    // query DB (blocking)
    let db = state.db();
    let svc = q.service_id.clone();
    let port = q.port;
    let join_res = tokio::task::spawn_blocking(move || {
        db::query::load_records_agg(&db.path, svc.as_deref(), port, since, until, limit, asc)
    })
        .await;

    match join_res {
        Ok(Ok((records, next_since))) => (
            StatusCode::OK,
            Json(json!({"status":"ok","data":{"records": records, "next_since": next_since}})),
        ),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"status":"error","errors":[e.to_string()]})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"status":"error","errors":[format!("join error: {e}")]})),
        ),
    }
}
