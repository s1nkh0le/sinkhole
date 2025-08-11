use crate::db::query;
use crate::state::ctrl::ControllerState;
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

    // New normalized filters
    inbound: Option<bool>,        // true = "in", false = "out"
    decision: Option<&'static str>, // "allow" | "drop"
    method: Option<String>,
    path_like: Option<String>,    // wrapped with %…%
    status_eq: Option<i64>,
    status_min: Option<i64>,
    status_max: Option<i64>,
    len_min: Option<i64>,
    len_max: Option<i64>,
    conn_id: Option<i64>,

    // Drop-reason filter (only active if all three are Some)
    drop_kind: Option<String>,
    drop_item_type: Option<String>,
    drop_value: Option<String>,
}

fn build_records_norm(
    service_id: Option<String>,
    port:       Option<u16>,
    since:      Option<u64>,
    until:      Option<u64>,
    limit:      Option<usize>,
    order:      Option<String>,

    decision:   Option<String>,
    dir:        Option<String>,
    method:     Option<String>,
    path_like:  Option<String>,
    status:     Option<u16>,
    status_min: Option<u16>,
    status_max: Option<u16>,
    len_min:    Option<u64>,
    len_max:    Option<u64>,
    conn_id:    Option<u64>,

    drop_kind:      Option<String>,
    drop_item_type: Option<String>,
    drop_value:     Option<String>,

    instance:   &str,
) -> Result<DBNormalizedRecords, (StatusCode, Json<Value>)> {
    if port.is_some() && service_id.is_none() {
        return Err(
            ErrorEnvelope::bad_request("invalid_query", instance)
                .with_title("Port requires service_id")
                .with_detail(format!("when `port` is set ({:?}), `service_id` is required", port))
                .into_result()
        );
    }

    let since = since.unwrap_or(0);
    let until = until.unwrap_or(i64::MAX as u64);

    let asc = match normalize_order(order.as_deref()) {
        Ok(v) => v,
        Err(m) => return Err(ErrorEnvelope::bad_request("sanitization", instance).with_detail(m).into_result()),
    };

    const MAX_LIMIT: usize = 10_000;
    let limit = match clamp_limit(limit, MAX_LIMIT) {
        Ok(v) => v,
        Err(m) => return Err(ErrorEnvelope::bad_request("sanitization", instance).with_detail(m).into_result()),
    };

    if let Err(m) = validate_range(since, until) {
        return Err(ErrorEnvelope::bad_request("sanitization", instance).with_detail(m).into_result());
    }

    let decision = match normalize_decision(decision.as_deref()) {
        Ok(v) => v,
        Err(m) => return Err(ErrorEnvelope::bad_request("sanitization", instance).with_detail(m).into_result()),
    };

    let inbound = match normalize_dir(dir.as_deref()) {
        Ok(v) => v,
        Err(m) => return Err(ErrorEnvelope::bad_request("sanitization", instance).with_detail(m).into_result()),
    };

    // Normalize ranges
    let status_eq  = status.map(|v| v as i64);
    let status_min = status_min.map(|v| v as i64);
    let status_max = status_max.map(|v| v as i64);
    if let Err(m) = validate_range_opt(status_min, status_max, "status") {
        return Err(ErrorEnvelope::bad_request("sanitization", instance).with_detail(m).into_result());
    }

    let len_min = len_min.map(|v| v as i64);
    let len_max = len_max.map(|v| v as i64);
    if let Err(m) = validate_range_opt(len_min, len_max, "length") {
        return Err(ErrorEnvelope::bad_request("sanitization", instance).with_detail(m).into_result());
    }

    let path_like = wrap_like(path_like);
    let conn_id   = conn_id.map(|v| v as i64);

    // Drop-reason filters only activate if all are present
    let (drop_kind, drop_item_type, drop_value) = match (&drop_kind, &drop_item_type, &drop_value) {
        (Some(_), Some(_), Some(_)) => (drop_kind, drop_item_type, drop_value),
        _ => (None, None, None),
    };

    Ok(DBNormalizedRecords {
        service_id,
        port,
        since: u64_to_i64_sat(since),
        until: u64_to_i64_sat(until),
        limit,
        asc,
        inbound,
        decision,
        method,
        path_like,
        status_eq,
        status_min,
        status_max,
        len_min,
        len_max,
        conn_id,
        drop_kind,
        drop_item_type,
        drop_value,
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

    /// "allow" | "drop"
    pub decision: Option<String>,
    /// "in" | "out"
    pub dir: Option<String>,

    pub method: Option<String>,          // HTTP method exact match (GET, POST, …)
    pub path_like: Option<String>,       // SQL LIKE, substring match
    pub status: Option<u16>,             // exact status
    pub status_min: Option<u16>,         // or range
    pub status_max: Option<u16>,
    pub len_min: Option<u64>,            // http_length range (bytes)
    pub len_max: Option<u64>,
    pub conn_id: Option<u64>,            // exact conn id

    // Drop-reason item match (only used if all 3 are present)
    // kind: "blacklist" | "whitelist_all_miss" | "whitelist_any_miss"
    pub drop_kind: Option<String>,
    // item_type: "filter_id" | "missing_id" | "hit_id"
    pub drop_item_type: Option<String>,
    pub drop_value: Option<String>,
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
        q.service_id.clone(), q.port,
        q.since, q.until, q.limit, q.order.clone(),
        q.decision.clone(), q.dir.clone(),
        q.method.clone(), q.path_like.clone(),
        q.status, q.status_min, q.status_max,
        q.len_min, q.len_max,
        q.conn_id,
        q.drop_kind.clone(), q.drop_item_type.clone(), q.drop_value.clone(),
        uri.path(),
    ) { Ok(n) => n, Err(e) => return e };

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

    pub decision: Option<String>,
    pub dir: Option<String>,
    pub method: Option<String>,
    pub path_like: Option<String>,
    pub status: Option<u16>,
    pub status_min: Option<u16>,
    pub status_max: Option<u16>,
    pub len_min: Option<u64>,
    pub len_max: Option<u64>,
    pub conn_id: Option<u64>,

    pub drop_kind: Option<String>,
    pub drop_item_type: Option<String>,
    pub drop_value: Option<String>,
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
        Some(service_id), Some(port),
        q.since, q.until, q.limit, q.order.clone(),
        q.decision.clone(), q.dir.clone(),
        q.method.clone(), q.path_like.clone(),
        q.status, q.status_min, q.status_max,
        q.len_min, q.len_max,
        q.conn_id,
        q.drop_kind.clone(), q.drop_item_type.clone(), q.drop_value.clone(),
        uri.path(),
    ) { Ok(n) => n, Err(e) => return e };

    records_core(state, norm, uri.path()).await
}


// Underlying DB-handler for records queries
async fn records_core(
    state: ControllerState,
    q: DBNormalizedRecords,
    instance: &str
) -> (StatusCode, Json<Value>) {
    // validate known service/port
    if let Some(ref sid) = q.service_id {
        let g = state.read();
        let Some(svc) = g.services.get(sid) else {
            return ErrorEnvelope::not_found("service_not_found", instance)
                .with_detail(format!("service_id={sid} not found")).into_result();
        };
        if let Some(p) = q.port {
            if !svc.ports.contains_key(&p) {
                return ErrorEnvelope::not_found("port_not_found", instance)
                    .with_detail(format!("port={p} not found")).into_result();
            }
        }
    }

    // DB call (spawn_blocking)
    let db = state.db();
    let join_res = tokio::task::spawn_blocking(move || {
        query::load_records_agg_advanced(
            &db.path,
            q.service_id.as_deref(),
            q.port,
            q.since, q.until, q.limit, q.asc,
            q.inbound,
            q.decision,
            q.conn_id,
            q.method.as_deref(),
            q.path_like.as_deref(),
            q.status_eq,
            q.status_min, q.status_max,
            q.len_min, q.len_max,
            q.drop_kind.as_deref(),
            q.drop_item_type.as_deref(),
            q.drop_value.as_deref(),
        )
    }).await;

    match join_res {
        Ok(Ok((records, next_since))) =>
            ok(CtrlReplyRecordsOkData {
                records,
                next_since: u64::try_from(next_since).unwrap_or(0),
            }),
        Ok(Err(e)) =>
            ErrorEnvelope::internal("db_query_failure", instance)
                .with_detail(e.to_string()).into_result(),
        Err(e) =>
            ErrorEnvelope::internal("db_query_failure_join", instance)
                .with_detail(e.to_string()).into_result(),
    }
}





fn normalize_decision(s: Option<&str>) -> Result<Option<&'static str>, String> {
    match s.map(|v| v.to_ascii_lowercase()) {
        None => Ok(None),
        Some(ref v) if v == "allow" => Ok(Some("allow")),
        Some(ref v) if v == "drop"  => Ok(Some("drop")),
        Some(v) => Err(format!("decision must be 'allow' or 'drop', got '{}'", v)),
    }
}

fn normalize_dir(s: Option<&str>) -> Result<Option<bool>, String> {
    match s.map(|v| v.to_ascii_lowercase()) {
        None => Ok(None),
        Some(ref v) if v == "in"  => Ok(Some(true)),
        Some(ref v) if v == "out" => Ok(Some(false)),
        Some(v) => Err(format!("dir must be 'in' or 'out', got '{}'", v)),
    }
}

fn wrap_like(v: Option<String>) -> Option<String> {
    v.map(|s| format!("%{}%", s))
}

fn validate_range_opt(min: Option<i64>, max: Option<i64>, name: &str) -> Result<(), String> {
    match (min, max) {
        (Some(a), Some(b)) if a > b => Err(format!("invalid {} range: min {} > max {}", name, a, b)),
        _ => Ok(()),
    }
}