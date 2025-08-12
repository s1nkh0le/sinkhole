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
    /// Service id
    service_id: Option<String>,
    /// Port
    port: Option<u16>,
    /// Select records having a timestamp higher than `since`
    ///
    /// Values: < `until`
    since: i64,
    /// Select records having a timestamp lower than `until`
    ///
    /// Values: > `since`
    until: i64,
    /// Select fewer than `limit` records
    ///
    /// Values: >=0
    limit: usize,
    /// Select the query result sorting
    ///
    /// Values: `asc` | `desc`
    asc: bool,

    /// List of markers that a record must have matched for the selection to be possible
    ///
    /// Values: list of valid marker ids
    positive_markers: Vec<String>,
    /// List of markers that a record must not have matched for the selection to be possible
    ///
    /// Values: list of valid marker ids
    negative_markers: Vec<String>,

    /// Direction of the associated traffic.
    ///
    /// Values: `in` | `out`
    dir: Option<bool>,
    /// Whether the associated traffic has been allowed or dropped.
    ///
    /// Values: `drop` | `allow`
    decision: Option<&'static str>,
    /// HTTP method of the request
    ///
    /// Usual values: `GET` | `PUT` | `POST` | `DELETE` | `HEAD`
    method: Option<String>,
    /// HTTP path condition (auto wrapped with %…%)
    ///
    /// Note: under the hood, it's compared with SQL's `LIKE` instruction
    path_like: Option<String>,
    /// HTTP status, exact match
    ///
    /// Constraint: only applies to outbound messages
    ///
    /// Usual values: `200` | `400` | `404` | `500`
    status_eq: Option<i64>,
    /// HTTP status, minimum allowed numerical value
    ///
    /// Constraint: only applies to outbound messages; can't be used alongside `status=X`
    ///
    /// Values: < `status_max` (if set)
    status_min: Option<i64>,
    /// HTTP status, maximum allowed numerical value
    ///
    /// Constraint: only applies to outbound messages; can't be used alongside `status=X`
    ///
    /// Values: > `status_min` (if set)
    status_max: Option<i64>,
    /// HTTP length, exact match
    ///
    /// Values: >0
    len: Option<i64>,
    /// HTTP length, minimum allowed numerical value
    ///
    /// Constraint: can't be used alongside `len=X`
    ///
    /// Values: < `len_max` (if set)
    len_min: Option<i64>,
    /// HTTP length, maximum allowed numerical value
    ///
    /// Constraint: can't be used alongside `len=X`
    ///
    /// Values: > `len_min` (if set)
    len_max: Option<i64>,

    /// TCP session id, used for grouping messages from the same TCP session
    ///
    /// Note: for this constraint to be unique, you also need to specify `service_id` and `port` to the query
    ///
    /// Values: >0
    conn_id: Option<i64>,

    /// Specifies which type of droplist triggered the record to be discarted
    ///
    /// Constraint: can't be used alongside `decision=allow`
    ///
    /// Values: `blacklist` | `whitelist_all_miss` | `whitelist_any_miss`
    drop_kind: Option<String>,

    /// Specifies which filter id partecipated in the triggering of the drop
    ///
    /// Values: `filter_id` (restricts to `Blacklist.filter_ids`) | `missing_id` (restricts to `WhitelistAllMiss.missing_ids` | `hit_id` (restricts to `WhitelistAnyMiss.hit_ids`)
    drop_item_type: Option<String>,

    /// Specifies which filter id
    ///
    /// Values: string (valid filter id)
    drop_value: Option<String>,
}

fn build_records_norm(
    service_id: Option<String>,
    port:       Option<u16>,
    since:      Option<u64>,
    until:      Option<u64>,
    limit:      Option<usize>,
    order:      Option<String>,

    positive_markers: Option<Vec<String>>,
    negative_markers: Option<Vec<String>>,
    dir:        Option<String>,
    decision:   Option<String>,
    method:     Option<String>,
    path_like:  Option<String>,
    status:     Option<u16>,
    status_min: Option<u16>,
    status_max: Option<u16>,
    len:        Option<u64>,
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

    let mut pos = positive_markers.unwrap_or_default();
    let mut neg = negative_markers.unwrap_or_default();

    pos.retain(|s| !s.trim().is_empty());
    neg.retain(|s| !s.trim().is_empty());
    pos.sort_unstable(); pos.dedup();
    neg.sort_unstable(); neg.dedup();


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
    if status_eq.is_some() && (status_min.is_some() || status_max.is_some()) {
        return Err(ErrorEnvelope::bad_request("sanitization", instance)
            .with_detail(format!("Query has nonsensical constraints: specified status={}, along with a status_min/status_max parameter", status_eq.unwrap()))
            .into_result());
    }
    if inbound == Some(true) && (status_eq.is_some() || status_min.is_some() || status_max.is_some()) {
        return Err(ErrorEnvelope::bad_request("sanitization", instance)
            .with_detail("Query has `inbound=true` and a HTTP `status` parameter set; This can't yield any record, since only server replies (i.e. outbound messages) have a status code.")
            .into_result());
    }


    let len = len.map(|v| v as i64);
    let len_min = len_min.map(|v| v as i64);
    let len_max = len_max.map(|v| v as i64);

    if len.is_some() && (len_min.is_some() || len_max.is_some()) {
        return Err(ErrorEnvelope::bad_request("sanitization", instance)
            .with_detail(format!("Query has nonsensical constraints: specified len={}, along with a len_min/len_max parameter", len.unwrap()))
            .into_result());
    }
    if let Err(m) = validate_range_opt(len_min, len_max, "length") {
        return Err(ErrorEnvelope::bad_request("sanitization", instance).with_detail(m).into_result());
    }

    let path_like = wrap_like(path_like);

    let conn_id   = conn_id.map(|v| v as i64);
    if conn_id.is_some() && (service_id.is_none() || port.is_none()) {
        return Err(ErrorEnvelope::bad_request("sanitization", instance)
            .with_detail(format!("Query specifies `conn_id={}`, but doesn't provide `service_id` AND `port`; this will join together unrelated records (connection ids are not unique across filters)", conn_id.unwrap()))
            .into_result());
    }


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
        positive_markers: pos,
        negative_markers: neg,
        dir: inbound,
        decision,
        method,
        path_like,
        status_eq,
        status_min,
        status_max,
        len,
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
#[derive(Deserialize, Debug, IntoParams, ToSchema)]
pub struct CtrlRequestGetQueryRecordsAggregate {
    /// Service id
    pub service_id: Option<String>,
    /// Port
    pub port: Option<u16>,
    /// Select records having a timestamp higher than `since`
    ///
    /// Values: < `until`
    pub since: Option<u64>,
    /// Select records having a timestamp lower than `until`
    ///
    /// Values: > `since`
    pub until: Option<u64>,
    /// Select fewer than `limit` records
    ///
    /// Values: >=0
    pub limit: Option<usize>,
    /// Select the query result sorting
    ///
    /// Values: `asc` | `desc`
    pub order: Option<String>,

    /// List of markers that a record must have matched for the selection to be possible
    ///
    /// Values: list of valid marker ids
    pub positive_markers: Option<Vec<String>>,
    /// List of markers that a record must not have matched for the selection to be possible
    ///
    /// Values: list of valid marker ids
    pub negative_markers: Option<Vec<String>>,
    /// Direction of the associated traffic.
    ///
    /// Values: `in` | `out`
    pub dir: Option<String>,
    /// Whether the associated traffic has been allowed or dropped.
    ///
    /// Values: `drop` | `allow`
    pub decision: Option<String>,

    /// HTTP method of the request
    ///
    /// Usual values: `GET` | `PUT` | `POST` | `DELETE` | `HEAD`
    pub method: Option<String>,
    /// HTTP path condition (auto wrapped with %…%)
    ///
    /// Note: under the hood, it's compared with SQL's `LIKE` instruction
    pub path_like: Option<String>,
    /// HTTP status, exact match
    ///
    /// Constraint: only applies to outbound messages
    ///
    /// Usual values: `200` | `400` | `404` | `500`
    pub status: Option<u16>,
    /// HTTP status, minimum allowed numerical value
    ///
    /// Constraint: only applies to outbound messages; can't be used alongside `status=X`
    ///
    /// Values: < `status_max` (if set)
    pub status_min: Option<u16>,
    /// HTTP status, maximum allowed numerical value
    ///
    /// Constraint: only applies to outbound messages; can't be used alongside `status=X`
    ///
    /// Values: > `status_min` (if set)
    pub status_max: Option<u16>,
    /// HTTP length, exact match
    ///
    /// Values: >0
    pub len: Option<u64>,
    /// HTTP length, minimum allowed numerical value
    ///
    /// Constraint: can't be used alongside `len=X`
    ///
    /// Values: < `len_max` (if set)
    pub len_min: Option<u64>,
    /// HTTP length, maximum allowed numerical value
    ///
    /// Constraint: can't be used alongside `len=X`
    ///
    /// Values: > `len_min` (if set)
    pub len_max: Option<u64>,

    /// TCP session id, used for grouping messages from the same TCP session
    ///
    /// Values: >0
    pub conn_id: Option<u64>,

    /// Specifies which type of droplist triggered the record to be discarted
    ///
    /// Constraint: can't be used alongside `decision=allow`
    ///
    /// Values: `blacklist` | `whitelist_all_miss` | `whitelist_any_miss`
    pub drop_kind: Option<String>,
    /// Specifies which filter id partecipated in the triggering of the drop
    ///
    /// Values: `filter_id` (restricts to `Blacklist.filter_ids`) | `missing_id` (restricts to `WhitelistAllMiss.missing_ids` | `hit_id` (restricts to `WhitelistAnyMiss.hit_ids`)
    pub drop_item_type: Option<String>,
    /// Specifies which filter id
    ///
    /// Values: string (valid filter id)
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
        q.positive_markers, q.negative_markers,
        q.dir.clone(), q.decision.clone(),
        q.method.clone(), q.path_like.clone(),
        q.status, q.status_min, q.status_max,
        q.len, q.len_min, q.len_max,
        q.conn_id,
        q.drop_kind.clone(), q.drop_item_type.clone(), q.drop_value.clone(),
        uri.path(),
    ) { Ok(n) => n, Err(e) => return e };

    records_core(state, norm, "/records").await
}




/// Query format for /services/{service_id}/ports/{port}/records
///
/// Note:   Query constraints are additive.
///         Unspecified query parameters don't generate a query condition, thus they don't impose any filtering.
///
/// Internal: The method doesn't allow to specify the `service_id` and `port`, as those are path parameters
#[derive(Deserialize, Debug, IntoParams, ToSchema)]
pub struct CtrlRequestGetQueryRecordsFromServicePort {
    /// Select records having a timestamp higher than `since`
    ///
    /// Values: < `until`
    pub since: Option<u64>,
    /// Select records having a timestamp lower than `until`
    ///
    /// Values: > `since`
    pub until: Option<u64>,
    /// Select fewer than `limit` records
    ///
    /// Values: >=0
    pub limit: Option<usize>,
    /// Select the query result sorting
    ///
    /// Values: `asc` | `desc`
    pub order: Option<String>,

    /// List of markers that a record must have matched for the selection to be possible
    ///
    /// Values: list of valid marker ids
    pub positive_markers: Option<Vec<String>>,
    /// List of markers that a record must not have matched for the selection to be possible
    ///
    /// Values: list of valid marker ids
    pub negative_markers: Option<Vec<String>>,
    /// Direction of the associated traffic.
    ///
    /// Values: `in` | `out`
    pub dir: Option<String>,
    /// Whether the associated traffic has been allowed or dropped.
    ///
    /// Values: `drop` | `allow`
    pub decision: Option<String>,
    /// HTTP method of the request
    ///
    /// Usual values: `GET` | `PUT` | `POST` | `DELETE` | `HEAD`
    pub method: Option<String>,
    /// HTTP path condition (auto wrapped with %…%)
    ///
    /// Note: under the hood, it's compared with SQL's `LIKE` instruction
    pub path_like: Option<String>,
    /// HTTP status, exact match
    ///
    /// Constraint: only applies to outbound messages
    ///
    /// Usual values: `200` | `400` | `404` | `500`
    pub status: Option<u16>,
    /// HTTP status, minimum allowed numerical value
    ///
    /// Constraint: only applies to outbound messages; can't be used alongside `status=X`
    ///
    /// Values: < `status_max` (if set)
    pub status_min: Option<u16>,
    /// HTTP status, maximum allowed numerical value
    ///
    /// Constraint: only applies to outbound messages; can't be used alongside `status=X`
    ///
    /// Values: > `status_min` (if set)
    pub status_max: Option<u16>,
    /// HTTP length, exact match
    ///
    /// Values: >0
    pub len: Option<u64>,
    /// HTTP length, minimum allowed numerical value
    ///
    /// Constraint: can't be used alongside `len=X`
    ///
    /// Values: < `len_max` (if set)
    pub len_min: Option<u64>,
    /// HTTP length, maximum allowed numerical value
    ///
    /// Constraint: can't be used alongside `len=X`
    ///
    /// Values: > `len_min` (if set)
    pub len_max: Option<u64>,

    /// TCP session id, used for grouping messages from the same TCP session
    ///
    /// Values: >0
    pub conn_id: Option<u64>,

    /// Specifies which type of droplist triggered the record to be discarted
    ///
    /// Constraint: can't be used alongside `decision=allow`
    ///
    /// Values: `blacklist` | `whitelist_all_miss` | `whitelist_any_miss`
    pub drop_kind: Option<String>,

    /// Specifies which filter id partecipated in the triggering of the drop
    ///
    /// Values: `filter_id` (restricts to `Blacklist.filter_ids`) | `missing_id` (restricts to `WhitelistAllMiss.missing_ids` | `hit_id` (restricts to `WhitelistAnyMiss.hit_ids`)
    pub drop_item_type: Option<String>,

    /// Specifies which filter id
    ///
    /// Values: string (valid filter id)
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
        q.positive_markers, q.negative_markers,
        q.dir.clone(), q.decision.clone(),
        q.method.clone(), q.path_like.clone(),
        q.status, q.status_min, q.status_max,
        q.len, q.len_min, q.len_max,
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
            &q.positive_markers, &q.negative_markers,
            q.dir,
            q.decision,
            q.conn_id,
            q.method.as_deref(),
            q.path_like.as_deref(),
            q.status_eq,
            q.status_min, q.status_max,
            q.len, q.len_min, q.len_max,
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