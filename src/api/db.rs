use std::collections::HashMap;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use tracing::info;
use crate::api::auth;
use crate::api::routes::SharedState;
use crate::db::query::{db_health, db_stats};
use crate::observability::sinks::sink_ecam;
use crate::observability::sinks::sink_ecam::{Origin, Severity};


///     API: `GET /db/health`
///     method: `db_health_ep`
pub async fn db_health_ep(
    Query(params): Query<HashMap<String, String>>,
    State((cfg, _registry, db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(e) = auth::check_token(params.get("token").map(String::as_str), &cfg) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"status":"error","errors":[e.to_string()]})));
    }
    match db_health(db.path.clone()).await {
        Ok(s) => (StatusCode::OK, Json(json!({"status":"ok", "data":s}))),
        Err(e) => {
            sink_ecam::record(
                Severity::Error,
                Origin::API,
                format!("[GET /db/health, db_health_ep()]: server error: {}", e),
                None,
            );

            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"status":"error","errors":[e.to_string()]})))
        },
    }
}



///     API: `GET /db/stats`
///     method: `db_stats_ep`
pub async fn db_stats_ep(
    Query(params): Query<HashMap<String, String>>,
    State((cfg, _registry, db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(e) = auth::check_token(params.get("token").map(String::as_str), &cfg) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"status":"error","errors":[e.to_string()]})));
    }
    match db_stats(db.path.clone()).await {
        Ok(v) => (StatusCode::OK, Json(json!({"status":"ok","data":v}))),
        Err(e) => {
            sink_ecam::record(
                Severity::Error,
                Origin::API,
                format!("[GET /db/stats, db_stats_ep()]: server error: {}", e),
                None,
            );

            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"status":"error","errors":[e.to_string()]})))
        },
    }
}


#[derive(Deserialize)]
pub struct ClearQuery {
    token: String,
}


///     API: `GET /db/clear`
///     method: `db_clear_ep`
pub async fn db_clear_ep(
    Query(params): Query<ClearQuery>,
    State((cfg, _registry, db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(e) = auth::check_token(Some(&params.token), &cfg) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"status":"error","errors":[e.to_string()]})),
        );
    }

    info!("received /db/clear request, clearing db..");

    match crate::db::query::clear_db(db.path.clone()).await {
        Ok(()) => {
            info!("cleared db");

            (
                StatusCode::OK,
                Json(json!({"status":"ok"})),
            )
        },
        Err(e) => {
            sink_ecam::record(
                Severity::Error,
                Origin::API,
                format!("[GET /db/clear, db_clear_ep()]: server error: {}", e),
                None,
            );

            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"status":"error","errors":[e.to_string()]})))
        },
    }
}
