use crate::api::routes_filter::SharedState;
use crate::db;
use crate::observability::sinks::sink_ecam;
use crate::observability::sinks::sink_ecam::{Origin, Severity};
use axum::Json;
use axum::extract::{OriginalUri, Path, Query, State};
use axum::http::StatusCode;
use serde::{Serialize};
use serde_json::{from_value, json, Value};
use tracing::log::debug;
use utoipa::{ToSchema};
use shared::api::err::ErrorEnvelope;
use shared::api::ok::{ok, OkEnvelope};
use shared::types::routing::filter::records::{FilterReplyRecordsAndCursor, FilterRequestGetQueryRecords};



/// /ports/{port}/records
///
/// Queries the database and returns a list of records (`EvaluationRecord`s) matching the given predicate, along with a cursor
#[utoipa::path(
    get,
    path = "/ports/{port}/records",
    params(
        ("token" = String, Query,                               description = "Authentication token"),
        ("port" = u16, Path,                                    description = "Public port"),
    ),
    responses(
        (status = 200, description = "OK",                      body=OkEnvelope<FilterReplyRecordsAndCursor>),
        (status = 400, description = "Invalid request",         body=ErrorEnvelope),
        (status = 401, description = "Unauthorized",            body=ErrorEnvelope),
        (status = 404, description = "Unknown port",            body=ErrorEnvelope),
        (status = 500, description = "Internal server error",   body=ErrorEnvelope),
    ),
    tag = "filter"
)]
pub async fn get_records(
    OriginalUri(uri): OriginalUri,
    Path(port): Path<u16>,
    Query(q): Query<FilterRequestGetQueryRecords>,
    State((_cfg, registry, db)): State<SharedState>,
) -> (StatusCode, Json<Value>) {
    // ensure port exists
    if registry.get(&port).is_none() {
        sink_ecam::record(
            Severity::Warn,
            Origin::API,
            format!("[get_records(), GET {}]: port not found", uri.path()),
            None,
        );

        return ErrorEnvelope::not_found("sanitization", uri.path())
            .with_title("Unknown Port")
            .into_result()
    }

    // ---- sanity checks -------------------------------------------------
    let mut errs = Vec::new();

    let decision = q.decision.as_deref().map(str::to_ascii_lowercase);
    if let Some(ref d) = decision {
        if d != "allow" && d != "drop" {
            errs.push(format!("decision must be 'allow' or 'drop', got {}", d));
        }
    }

    let dir = q.dir.as_deref().map(str::to_ascii_lowercase);
    if let Some(ref d) = dir {
        if d != "in" && d != "out" {
            errs.push(format!("dir must be 'in' or 'out', got {}", d));
        }
    }

    let reason = q.reason.as_deref().map(str::to_ascii_lowercase);
    if let Some(ref r) = reason {
        const ALLOWED: &[&str] = &["blacklist", "whitelist_all_miss", "whitelist_any_miss"];
        if !ALLOWED.contains(&r.as_str()) {
            errs.push(format!(
                "reason must be one of {:?}; got {}",
                ALLOWED,
                r.as_str()
            ));
        }
    }

    let order_raw = q.order.as_deref().unwrap_or("asc");
    let asc = match order_raw.to_ascii_lowercase().as_str() {
        "asc" => true,
        "desc" => false,
        _ => {
            errs.push(format!("order must be 'asc' or 'desc'; got {}", order_raw));
            true
        }
    };

    let limit = q.limit.unwrap_or(1000);
    if limit == 0 {
        errs.push("limit must be >0".to_string());
    }
    let limit = limit.min(10_000);

    let since: u64 = q.since.unwrap_or(0);
    let until = q.until.unwrap_or(i64::MAX as u64);
    if since < 0 || until < 0 || until < since {
        errs.push(format!("invalid since/until range: got since={}, until={}", since, until));
    }

    if !errs.is_empty() {
        sink_ecam::record(Severity::Warn, Origin::API, format!("[get_records(), GET {}]: errors while preparing query: {}", uri.path(), errs.join(", ")), None);

        return ErrorEnvelope::bad_request("sanitization", uri.path())
                .with_title("Bad Request")
                .with_detail(format!("Errors: {:?}", errs.join(", ")))
                .into_result()
    }

    let db_since = i64::try_from(since).unwrap_or(0);
    let db_until = i64::try_from(until).unwrap_or(i64::MAX);

    match db::query::fetch_records_filtered(db.path.clone(), port, db_since, db_until, limit, asc, q.decision.as_deref(), q.dir.as_deref(), q.reason.as_deref())
    .await
    {
        Ok((mut records, _last_ts_from_db)) => {
            let next_since = records
                .last()
                .and_then(|r| r.get("timestamp_ns").and_then(|v| v.as_u64()))
                .map(|ts| ts + 1)
                .unwrap_or(since);
            {
                debug!("[get_records(), GET {}]: got {} records: {:?}", uri.path(),records.len(),records);

                (
                    StatusCode::OK,
                    Json(json!({
                        "status": "ok",
                        "data": {
                            "records": records,
                            "next_since": next_since
                        }
                    })),
                )
            }
        }
        Err(e) => {
            sink_ecam::record(Severity::Warn, Origin::API, format!("[get_records(), GET {}]: database query failed: error={}", uri.path(), e), None);

            ErrorEnvelope::internal("db_query_failure", uri.path())
                .with_title("DB Query Failed")
                .with_detail(format!("Errors: {:?}", e))
                .into_result()
        }
    }
}


