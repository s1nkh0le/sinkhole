use std::collections::HashMap;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::json;
use crate::api::auth;
use crate::api::routes::SharedState;

// ---------------------------------------------------------
// GET /stats  (currently stub)
// ---------------------------------------------------------
pub async fn get_stats(
    Path(port): Path<u16>,
    Query(params): Query<HashMap<String, String>>,
    State((cfg, registry, db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(e) = auth::check_token(params.get("token").map(String::as_str), &cfg) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"status":"error","errors":[e.to_string()]})),
        );
    }

    match registry.get(&port) {
        Some(_) => (
            StatusCode::OK,
            Json(json!({"status":"ok","data":"stats disabled"})),
        ),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"status":"error","errors":["unknown port"]})),
        ),
    }
}
