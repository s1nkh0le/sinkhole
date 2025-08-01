use axum::{routing::get, Router};
use axum::http::StatusCode;
use axum::extract::State;
use axum::Json;

use crate::state::ControllerState;

// ──────────────────────────────────────────────────────────────
// Handlers (you can split these into files later if you want)
// ──────────────────────────────────────────────────────────────

pub async fn health() -> (StatusCode, &'static str) {
    (StatusCode::OK, "ok")
}

pub async fn list_services(
    State(state): State<ControllerState>
) -> Json<serde_json::Value> {
    let inner = state.read();
    let list = inner.services.iter().map(|(id, svc)| {
        serde_json::json!({
            "id": id,
            "url": svc.cfg.url,
            "healthy": svc.healthy,
            "last_sync_ns": svc.last_sync_ns
        })
    }).collect::<Vec<_>>();

    Json(serde_json::json!({ "status":"ok", "data": list }))
}

pub async fn get_ecam(State(state): State<ControllerState>) -> Json<serde_json::Value> {
    // you will likely add query params/cursor later
    let inner = state.read();
    Json(serde_json::json!({ "status":"ok", "data": "" }))
}

pub async fn get_records(State(state): State<ControllerState>) -> Json<serde_json::Value> {
    // you will likely route this to the controller DB query with filters
    let _inner = state.read();
    Json(serde_json::json!({ "status":"ok", "data": [] }))
}


pub fn build_router(state: ControllerState) -> Router {
    Router::new()
        .route("/health",  get(health))
        .route("/services", get(list_services))
        .route("/ecam",     get(get_ecam))
        .route("/records",  get(get_records))
        .with_state(state)
}
