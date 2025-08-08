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
use shared::types::routing::filter::db::FilterRequestDeleteQueryPruneRecords;


/// /db/health
/// 
/// Health check for the database
#[utoipa::path(
    get,
    path = "/db/health",
    params(
        ("token" = String, Query, description = "Authentication token"),
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
            sink_ecam::record(Severity::Error, Origin::API, format!("[get_db_health(), GET {}]: server error: {}", uri.path(), e), None);

            ErrorEnvelope::internal("db_query_failure", uri.path())
                .with_title("DB health query failed")
                .with_detail(format!("Database health query error: {}", e.to_string()))
                .into_result()
        }
    }
}





/// /db/stats
/// 
/// Returns some database statistics
#[utoipa::path(
    get,
    path = "/db/stats",
    params(
        ("token" = String, Query, description = "Authentication token"),
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
                format!("[get_db_stats(), GET {}]: server error: {}", uri.path(), e),
                None,
            );

            ErrorEnvelope::internal("db_query_failure", uri.path())
                .with_title("DB stats query failed")
                .with_detail(format!("Database stats query error: {}", e.to_string()))
                .into_result()
        }
    }
}




/// /db/clear
/// 
/// Clears the whole database
#[utoipa::path(
    delete,
    path = "/db/clear",
    params(
        ("token" = String, Query, description = "Authentication token"),
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
                format!("[delete_clear_db(), {}]: server query error: {}", uri.path(), e),
                None,
            );

            ErrorEnvelope::internal("db_query_failure", uri.path())
                .with_title("DB clearing failed")
                .with_detail(format!("Database clear query error: {}", e.to_string()))
                .into_result()
        }
    }
}




/// /db/prune/records
/// 
/// Removes `EvaluationRecord`s from the database `records` table having a timestamp less than the given parameter `prune_threshold_ts`.
/// Should be used periodically once records have been transferred out of the filter's database.
#[utoipa::path(
    delete,
    path = "/db/prune/records",
    params(
        ("token" = String, Query, description = "Authentication token"),
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
    Query(q): Query<FilterRequestDeleteQueryPruneRecords>,
    State((_cfg, _registry, db)): State<SharedState>,
) -> (StatusCode, Json<Value>) {
    info!("[delete_prune_db_records(), DELETE {}], pruning db records with record.timestamp_ns < {}", uri.path(), q.prune_threshold_ts);
    
    let db_path = db.path.clone();
    let db_since = i64::try_from(q.prune_threshold_ts).unwrap();
    const BATCH_SIZE: usize = 10_000;
    
    match crate::db::query::prune_records_before(db_path, db_since, BATCH_SIZE).await {
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
