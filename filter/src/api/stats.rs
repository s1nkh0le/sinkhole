use crate::api::auth;
use crate::api::routes::SharedState;
use crate::observability::sinks::sink_ecam;
use crate::observability::sinks::sink_ecam::{Origin, Severity};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde_json::json;
use std::collections::HashMap;

///     API: `GET /ports/{port}/stats`
///     method: `get_stats`
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
        None => {
            sink_ecam::record(
                Severity::Warn,
                Origin::API,
                format!("[GET /ports/{port}/stats, get_stats()]: port not found"),
                None,
            );

            (
                StatusCode::NOT_FOUND,
                Json(json!({"status":"error","errors":["unknown port"]})),
            )
        }
    }
}
