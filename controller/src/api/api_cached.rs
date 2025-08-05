use crate::state::ControllerState;
use axum::http::StatusCode;
use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::json;

pub async fn get_cached_port_ruleset(
    Path((svc, port)): Path<(String, u16)>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let g = state.read();
    let Some(service) = g.services.get(&svc) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"status":"error","errors":["unknown service"]})),
        );
    };
    let Some(p) = service.ports.get(&port) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"status":"error","errors":["unknown port"]})),
        );
    };
    match &p.ruleset {
        Some(rs) => (
            StatusCode::OK,
            Json(json!({"status":"ok","data": rs, "version": p.ruleset_version})),
        ),
        None => (
            StatusCode::NO_CONTENT,
            Json(json!({"status":"ok","data": null})),
        ),
    }
}
