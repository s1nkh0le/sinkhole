use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use crate::state::ControllerState;


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
