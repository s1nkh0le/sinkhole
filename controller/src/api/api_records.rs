use crate::db::query;
use crate::state::ControllerState;
use axum::extract::{OriginalUri, Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use shared::types::record_types::EvaluationRecord;
use utoipa::{IntoParams, ToSchema};
use shared::api::err::{ErrorEnvelope};
use shared::api::normalization::{clamp_limit, normalize_order, u64_to_i64_sat, validate_range};
use shared::api::ok::{ok, OkEnvelope};



/// Database level normalized query parameters for records query
/// (Note: using i64 types for SQLite)
#[derive(Debug)]
struct DBNormalizedRecords {
    service_id: Option<String>,
    port: Option<u16>,
    since: i64,
    until: i64,
    limit: usize,
    asc: bool,
}

fn build_records_norm(
    service_id: Option<String>,
    port:       Option<u16>,
    since:      Option<u64>,
    until:      Option<u64>,
    limit:      Option<usize>,
    order:      Option<String>,
    instance:   &str,
) -> Result<DBNormalizedRecords, (StatusCode, Json<Value>)> {
    // `port` implies `service_id`
    if port.is_some() && service_id.is_none() {
        return Err(
            ErrorEnvelope::bad_request("invalid_query", instance)
                .with_title("Port requires service_id")
                .with_detail(format!(
                    "when `port` is set ({:?}), `service_id` is required (none supplied)",
                    port
                ))
                .into_result()
        );
    }

    let since = since.unwrap_or(0);
    let until = until.unwrap_or(i64::MAX as u64);

    let asc = match normalize_order(order.as_deref()) {
        Ok(v) => v,
        Err(m) => {
            return Err(
                ErrorEnvelope::bad_request("sanitization", instance)
                    .with_detail(m)
                    .into_result()
            )
        }
    };

    const MAX_LIMIT: usize = 10_000;
    let limit = match clamp_limit(limit, MAX_LIMIT) {
        Ok(v) => v,
        Err(m) => {
            return Err(
                ErrorEnvelope::bad_request("sanitization", instance)
                    .with_detail(m)
                    .into_result()
            )
        }
    };

    if let Err(m) = validate_range(since, until) {
        return Err(
            ErrorEnvelope::bad_request("sanitization", instance)
                .with_detail(m)
                .into_result()
        );
    }

    Ok(DBNormalizedRecords {
        service_id,
        port,
        since: u64_to_i64_sat(since),
        until: u64_to_i64_sat(until),
        limit,
        asc,
    })
}


























/// Successful record listing reply
#[derive(Serialize, ToSchema)]
pub struct CtrlReplyRecordsOkData {
    pub records: Vec<EvaluationRecord>,
    pub next_since: u64,
}
impl shared::api::ok::sealed::Sealed for CtrlReplyRecordsOkData {}
impl shared::api::ok::OkData for CtrlReplyRecordsOkData {}


/// Query format for /record
#[derive(Deserialize, Debug, IntoParams)]
pub struct CtrlRequestGetQueryRecordsAggregate {
    pub service_id: Option<String>,
    pub port: Option<u16>,
    pub since: Option<u64>,
    pub until: Option<u64>,
    pub limit: Option<usize>,
    /// "asc" | "desc"
    pub order: Option<String>,
}


/// /records
///
/// Queries the aggregate database and returns a list of messages (`EvaluationRecord`s) matching the given predicates
#[utoipa::path(
    get,
    path = "/records",
    params(
        ("ctrl_token" = String, Query,                          description = "Authentication token"),
    ),
    responses(
        (status = 200, description = "OK",                      body = OkEnvelope<CtrlReplyRecordsOkData>),
        (status = 400, description = "Invalid query",           body = ErrorEnvelope),
        (status = 401, description = "Unauthorized",            body = ErrorEnvelope),
        (status = 404, description = "Unknown service/port",    body = ErrorEnvelope),
        (status = 500, description = "Server error",            body = ErrorEnvelope),
    ),
    tags = ["controller", "records"]
)]
pub async fn get_aggregated_records(
    OriginalUri(uri): OriginalUri,
    Query(q): Query<CtrlRequestGetQueryRecordsAggregate>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<Value>) {
    let norm = match build_records_norm(
        q.service_id.clone(),
        q.port,
        q.since,
        q.until,
        q.limit,
        q.order.clone(),
        uri.path(),
    ) {
        Ok(n)  => n,
        Err(e) => return e,
    };

    records_core(state, norm, "/records").await
}




/// Query format for /services/{service_id}/ports/{port}/records
///
/// Doesn't allow to specify the service_id and port, since they are path parameters
#[derive(Deserialize, Debug, IntoParams)]
pub struct CtrlRequestGetQueryRecordsFromServicePort {
    pub since: Option<u64>,
    pub until: Option<u64>,
    pub limit: Option<usize>,
    pub order: Option<String>,
}




/// /services/{service_id}/ports/{port}/records
///
/// Queries the database and returns a list of messages (`EvaluationRecord`s) matching the given predicates,
/// for the given service id and port
#[utoipa::path(
    get,
    path = "/services/{service_id}/ports/{port}/records",
    params(
        ("ctrl_token" = String, Query,                          description = "Authentication token"),
        ("service_id" = String, Path,                           description = "Service id"),
        ("port"       = u16,    Path,                           description = "Port number")
    ),
    responses(
        (status = 200, description = "OK",                      body = OkEnvelope<CtrlReplyRecordsOkData>),
        (status = 400, description = "Invalid query",           body = ErrorEnvelope),
        (status = 401, description = "Unauthorized",            body = ErrorEnvelope),
        (status = 404, description = "Unknown service/port",    body = ErrorEnvelope),
        (status = 500, description = "Server error",            body = ErrorEnvelope),
    ),
    tags = ["controller", "records"]
)]
pub async fn get_service_port_records(
    OriginalUri(uri): OriginalUri,
    Path((service_id, port)): Path<(String, u16)>,
    Query(q): Query<CtrlRequestGetQueryRecordsFromServicePort>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<Value>) {
    let norm = match build_records_norm(
        Some(service_id),
        Some(port),
        q.since,
        q.until,
        q.limit,
        q.order.clone(),
        uri.path(),
    ) {
        Ok(n)  => n,
        Err(e) => return e,
    };

    records_core(state, norm, uri.path()).await
}


// Underlying DB-handler for records queries
async fn records_core(state: ControllerState, q: DBNormalizedRecords, instance: &str) -> (StatusCode, Json<Value>) {
    if let Some(ref sid) = q.service_id {
        let g = state.read();
        let svc = match g.services.get(sid) {
            Some(s) => s,
            None => {
                return ErrorEnvelope::not_found("service_not_found", instance).with_detail(format!("service_id={sid} not found")).into_result()
            }
        };

        if let Some(p) = q.port {
            if !svc.ports.contains_key(&p) {
                return ErrorEnvelope::not_found("port_not_found", instance).with_detail(format!("port={p} not found")).into_result()
            }
        }
    }

    let db = state.db();
    let svc = q.service_id.clone();
    let port = q.port;
    let join_res = tokio::task::spawn_blocking(move || {
        query::load_records_agg(&db.path, svc.as_deref(), port, q.since, q.until, q.limit, q.asc)
    }).await;

    match join_res {
        Ok(Ok((records, next_since))) => {
            ok(CtrlReplyRecordsOkData {
                records,
                next_since: u64::try_from(next_since).unwrap() }
            )
        },
        Ok(Err(e)) => ErrorEnvelope::internal("db_query_failure", instance).with_detail(e.to_string()).into_result(),
        Err(e) => ErrorEnvelope::internal("db_query_failure_join", instance).with_detail(e.to_string()).into_result(),
    }
}
