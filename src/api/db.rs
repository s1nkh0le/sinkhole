use std::collections::HashMap;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::json;
use crate::api::auth;
use crate::api::routes::SharedState;
use crate::db::query::{db_health, db_stats};

pub async fn db_health_ep(
    Query(params): Query<HashMap<String, String>>,
    State((cfg, _registry, db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(e) = auth::check_token(params.get("token").map(String::as_str), &cfg) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"status":"error","errors":[e.to_string()]})));
    }
    match db_health(db.path.clone()).await {
        Ok(s) => (StatusCode::OK, Json(json!({"status":"ok","data":s}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"status":"error","errors":[e.to_string()]}))),
    }
}

pub async fn db_stats_ep(
    Query(params): Query<HashMap<String, String>>,
    State((cfg, _registry, db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(e) = auth::check_token(params.get("token").map(String::as_str), &cfg) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"status":"error","errors":[e.to_string()]})));
    }
    match db_stats(db.path.clone()).await {
        Ok(v) => (StatusCode::OK, Json(json!({"status":"ok","data":v}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"status":"error","errors":[e.to_string()]}))),
    }
}
