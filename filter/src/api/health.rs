use crate::api::ecam::EcamRequest;
use crate::api::routes::SharedState;
use axum::Json;
use axum::extract::{Query, State};
use serde_json::json;

///     API: `GET /health`
///     method: `health`
pub async fn health() -> Json<serde_json::Value> {
    Json(json!({"status":"ok"}))
}

///     API: `GET /version`
///     method: `version`
pub async fn version(State((cfg, _registry, _db)): State<SharedState>) -> Json<serde_json::Value> {
    Json(json!({
        "status":"ok",
        "data": {"version":"0.0.1"},
        "debug": cfg.debug
    }))
}
