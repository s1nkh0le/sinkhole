use axum::{
    routing::{get},
    Json, Router,
};
use serde_json::json;

use crate::config::Config;

pub fn build_router(cfg: Config) -> Router {
    // For now: only health & version.
    Router::new()
        .route("/health", get(health))
        .route("/version", get(version))
        // .layer(... auth etc later)
        .with_state(cfg)
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({"status":"ok"}))
}

async fn version() -> Json<serde_json::Value> {
    // In future: embed git commit via build.rs or env! macro.
    Json(json!({"status":"ok","data":{"version":"0.0.1"}}))
}
