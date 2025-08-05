use crate::api::middleware_auth;
use crate::api::routes::SharedState;
use crate::db::query::{db_health, db_stats};
use crate::observability::sinks::sink_ecam;
use crate::observability::sinks::sink_ecam::{Origin, Severity};
use axum::Json;
use axum::extract::{OriginalUri, Query, State};
use axum::http::StatusCode;
use serde_json::json;
use tracing::info;
use tracing::log::warn;
use shared::api::err::ErrorEnvelope;


#[utoipa::path(
    get,
    path = "/db/health",
    params(
        ("token" = String, description = "Authentication token"),
    ),
    responses(
        (status = 200, description = "OK"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Server error"),
    ),
    tag = "filter"
)]


pub async fn get_db_health(
    OriginalUri(uri): OriginalUri,
    State((_cfg, _registry, db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match db_health(db.path.clone()).await {
        Ok(s) => (StatusCode::OK, Json(json!({"status":"ok", "data": s}))),
        Err(e) => {
            sink_ecam::record(Severity::Error, Origin::API, format!("[db_health_ep(), GET {}]: server error: {}", uri.path(), e), None, );
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"status":"error","errors":[e.to_string()]})),)
        }
    }
}

#[utoipa::path(
    get,
    path = "/db/stats",
    params(
        ("token" = String, description = "Authentication token"),
    ),
    responses(
        (status = 200, description = "OK"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Server error"),
    ),
    tag = "filter"
)]
pub async fn get_db_stats(
    OriginalUri(uri): OriginalUri,
    State((_cfg, _registry, db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match db_stats(db.path.clone()).await {
        Ok(v) => (StatusCode::OK, Json(json!({"status":"ok", "data":v}))),
        Err(e) => {
            sink_ecam::record(
                Severity::Error,
                Origin::API,
                format!("[db_stats_ep(), GET {}]: server error: {}", uri.path(), e),
                None,
            );

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"status":"error","errors":[e.to_string()]})),
            )
        }
    }
}


#[utoipa::path(
    delete,
    path = "/db/clear",
    params(
        ("token" = String, description = "Authentication token"),
    ),
    responses(
        (status = 200, description = "OK"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Server error"),
    ),
    tag = "filter"
)]
pub async fn delete_clear_db(
    OriginalUri(uri): OriginalUri,
    State((_cfg, _registry, db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    warn!("[delete_clear_db(), DELETE {}], clearing db...", uri.path());

    match crate::db::query::clear_db(db.path.clone()).await {
        Ok(()) => {
            info!("cleared db successfully");
            (StatusCode::OK, Json(json!({"status":"ok"})))
        }
        Err(e) => {
            sink_ecam::record(
                Severity::Error,
                Origin::API,
                format!("[db_clear_ep(), {}]: server query error: {}", uri.path(), e),
                None,
            );

            ErrorEnvelope::internal("db_query_failure", uri.path())
                .with_title("DB clearing failed")
                .with_detail(format!("Database query error: {}", e.to_string()))
                .into_result()
        }
    }
}
