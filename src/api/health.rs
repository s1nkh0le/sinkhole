use axum::Json;
use serde_json::json;

///     API: `GET /health`
///     method: `health`
pub async fn health() -> Json<serde_json::Value> {
    Json(json!({"status":"ok"}))
}

///     API: `GET /version`
///     method: `version`
pub async fn version() -> Json<serde_json::Value> {
    Json(json!({"status":"ok","data":{"version":"0.0.1"}}))
}
