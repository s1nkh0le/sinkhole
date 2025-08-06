use crate::api::middleware_auth;
use crate::api::routes_filter::SharedState;
use crate::db::query::{db_health, db_stats};
use crate::observability::sinks::sink_ecam;
use crate::observability::sinks::sink_ecam::{Origin, Severity};
use axum::Json;
use axum::extract::{OriginalUri, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::info;
use tracing::log::warn;
use utoipa::{IntoParams, ToSchema};
use shared::api::err::ErrorEnvelope;
use crate::api::api_records::RecordsQuery;

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
) -> (StatusCode, Json<Value>) {
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
) -> (StatusCode, Json<Value>) {
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
) -> (StatusCode, Json<Value>) {
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
                .with_detail(format!("Database clear query error: {}", e.to_string()))
                .into_result()
        }
    }
}


#[derive(Deserialize, Debug, ToSchema, IntoParams)]
pub struct PruneQuery {
    /// Delete all database records before this timestamp, keep all records with tiemstamp greater than this
    pub prune_threshold_ts: i64,
}


#[utoipa::path(
    delete,
    path = "/db/prune/records",
    params(
        PruneQuery,
        ("token" = String, description = "Authentication token"),
    ),
    responses(
        (status = 200, description = "OK"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Server error"),
    ),
    tag = "filter"
)]
pub async fn delete_prune_db_records(
    OriginalUri(uri): OriginalUri,
    Query(q): Query<PruneQuery>,
    State((_cfg, _registry, db)): State<SharedState>,
) -> (StatusCode, Json<Value>) {
    info!("[delete_prune_db_records(), DELETE {}], pruning db records with record.timestamp_ns < {}", uri.path(), q.prune_threshold_ts);

    const BATCH_SIZE: usize = 10_000;

    match crate::db::query::prune_records_before(db.path.clone(), q.prune_threshold_ts, BATCH_SIZE).await {
        Ok((pr)) => {
            info!("pruned records successfully");
            (StatusCode::OK, Json(serde_json::to_value(pr).unwrap()))
        }
        Err(e) => {
            sink_ecam::record(
                Severity::Error,
                Origin::API,
                format!("[delete_prune_db_records(), {}]: server query error: {}", uri.path(), e),
                None,
            );

            ErrorEnvelope::internal("db_query_failure", uri.path())
                .with_title("DB pruning failed")
                .with_detail(format!("Database prune query error: {}", e.to_string()))
                .into_result()
        }
    }
}
