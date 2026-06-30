use crate::api::api_records::{normalize_decision, normalize_dir, validate_range_opt, wrap_like};
use crate::db::query_records_grouped::load_records_grouped_agg_advanced;
use crate::state::ctrl::ControllerState;
use axum::Json;
use axum::extract::{OriginalUri, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use shared::api::err::ErrorEnvelope;
use shared::api::normalization::{
    clamp_limit, normalize_order, parse_protocols, split_csv, u64_to_i64_sat, validate_range,
};
use shared::api::ok::{OkEnvelope, ok};
use shared::types::record_types::EvaluationRecord;
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CtrlReplyGroupedRecordsOkData {
    pub groups: Vec<RecordGroup>,
    /// Cursor to page **forward** in time (use with `order=asc`): pass back as `since`.
    pub next_since: u64,
    /// Cursor to page **backward** in time (use with `order=desc`, the default):
    /// pass back as `until`.
    pub next_until: u64,
}
impl shared::api::ok::sealed::Sealed for CtrlReplyGroupedRecordsOkData {}
impl shared::api::ok::OkData for CtrlReplyGroupedRecordsOkData {}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RecordGroup {
    pub service_id: String,
    pub port: u16,
    pub conn_id: i64,
    pub first_ts_ns: u64,
    pub last_ts_ns: u64,
    pub records: Vec<EvaluationRecord>,
}

/// Query format for /records/grouped
///
/// The record-level filters (`method`, `path_like`, `status`, `len`, `dir`,
/// `decision`, markers, drop reasons) share the **exact syntax and validation of
/// `/records`. They are evaluated per single record, AND-ed together, and a
/// connection group is returned if **≥1 record satisfies all of them at once**.
///
/// Consequence: a record is *either* a request (`method`/`path`, no `status`) or a
/// response (`status`, no `method`/`path`), so request-scoped and response-scoped
/// filters cannot be combined in one query, for example `method=GET&status=200`
/// matches nothing, because no single record has both. Filter on one scope at a
/// time (the matching connection's other records are still returned).
#[derive(Debug, Clone, Serialize, Deserialize, IntoParams)]
pub struct CtrlRequestGetQueryRecordsGrouped {
    pub service_id: Option<String>,
    pub port: Option<u16>,
    pub since: Option<u64>,
    pub until: Option<u64>,
    /// Max number of connection groups to return.
    pub limit: Option<usize>,
    /// Cap records returned per group (earliest first); omit to return the whole
    /// connection. A safety valve against one chatty connection dominating the payload.
    pub record_limit_per_group: Option<usize>,
    pub order: Option<String>,

    /// Comma-separated marker ids the record **must** have matched (e.g.
    /// `positive_markers=auth_ok,is_admin`). Repeated query keys are not supported.
    ///
    /// Matched **by name**: a marker id is only unique within one port's ruleset,
    /// so *scope* is controlled by `service_id`/`port`, not by this term. Without a
    /// `service_id` the id is matched fleet-wide, every record on any filter/port
    /// whose evaluation produced a marker with that id. If two ports reuse an id
    /// for different meanings an unscoped query conflates them; scope by
    /// `service_id` (and `port`) when the meaning is port-specific.
    pub positive_markers: Option<String>,
    /// Comma-separated marker ids the record **must not** have matched. Matched by
    /// name with the same scoping rules as `positive_markers`.
    pub negative_markers: Option<String>,
    /// Comma-separated wire protocols to match (a connection matches if it has a
    /// record with one of these protocols). Values: `http/1.0` | `http/1.1` |
    /// `http/2` | `raw-ascii` | `raw-binary`.
    pub protocol: Option<String>,
    /// Exact number of (matching) messages in the connection. Can't be combined
    /// with `msg_count_min/max`.
    pub msg_count: Option<u64>,
    /// Minimum number of (matching) messages in the connection (inclusive).
    pub msg_count_min: Option<u64>,
    /// Maximum number of (matching) messages in the connection (inclusive).
    pub msg_count_max: Option<u64>,
    /// Direction of the associated traffic. Values: `in` | `out`
    pub dir: Option<String>,
    /// Whether the associated traffic has been allowed or dropped. Values: `allow` | `drop`
    pub decision: Option<String>,

    /// HTTP method of the request. Usual values: `GET` | `PUT` | `POST` | `DELETE` | `HEAD`
    pub method: Option<String>,
    /// HTTP path substring match (compared via SQL `LIKE`, auto-wrapped with `%…%`).
    pub path_like: Option<String>,
    /// HTTP status, exact match. Constraint: only applies to outbound messages.
    pub status: Option<u16>,
    /// HTTP status minimum (inclusive). Can't be combined with `status`.
    pub status_min: Option<u16>,
    /// HTTP status maximum (inclusive). Can't be combined with `status`.
    pub status_max: Option<u16>,

    /// HTTP length, exact match.
    pub len: Option<u64>,
    /// HTTP length minimum (inclusive). Can't be combined with `len`.
    pub len_min: Option<u64>,
    /// HTTP length maximum (inclusive). Can't be combined with `len`.
    pub len_max: Option<u64>,

    /// TCP session id, exact match. Requires `service_id` AND `port` (conn ids
    /// aren't unique across filters). Can't be combined with `conn_id_min/max`.
    pub conn_id: Option<u64>,
    /// TCP session id minimum (inclusive). Requires `service_id` AND `port`.
    pub conn_id_min: Option<u64>,
    /// TCP session id maximum (inclusive). Requires `service_id` AND `port`.
    pub conn_id_max: Option<u64>,

    pub drop_kind: Option<String>,
    pub drop_item_type: Option<String>,
    pub drop_value: Option<String>,
}

struct DBNormalizedGroups {
    service_id: Option<String>,
    port: Option<u16>,
    since: i64,
    until: i64,
    limit: usize,
    record_limit_per_group: Option<i64>,
    asc: bool,

    positive_markers: Vec<String>,
    negative_markers: Vec<String>,
    protocols: Vec<String>,
    msg_count: Option<i64>,
    msg_count_min: Option<i64>,
    msg_count_max: Option<i64>,
    inbound: Option<bool>,
    decision: Option<&'static str>,
    conn_id: Option<i64>,
    conn_id_min: Option<i64>,
    conn_id_max: Option<i64>,

    method: Option<String>,
    path_like: Option<String>,
    status_eq: Option<i64>,
    status_min: Option<i64>,
    status_max: Option<i64>,

    len: Option<i64>,
    len_min: Option<i64>,
    len_max: Option<i64>,

    drop_kind: Option<String>,
    drop_item_type: Option<String>,
    drop_value: Option<String>,
}

fn build_groups_norm(
    q: CtrlRequestGetQueryRecordsGrouped,
    instance: &str,
) -> Result<DBNormalizedGroups, (StatusCode, Json<Value>)> {
    // port ⇒ service_id
    if q.port.is_some() && q.service_id.is_none() {
        return Err(ErrorEnvelope::bad_request("invalid_query", instance)
            .with_title("Port requires service_id")
            .with_detail("when `port` is set, `service_id` is required")
            .into_result());
    }

    let since = q.since.unwrap_or(0);
    let until = q.until.unwrap_or(i64::MAX as u64);

    let asc = match normalize_order(q.order.as_deref()) {
        Ok(v) => v,
        Err(m) => {
            return Err(ErrorEnvelope::bad_request("sanitization", instance)
                .with_detail(m)
                .into_result());
        }
    };

    // The single-JOIN query binds no per-group params, so the cap is purely about
    // bounding the response size (groups × records/group), not the SQLite var limit.
    const MAX_GROUPS: usize = 2000;
    let limit = match clamp_limit(q.limit, MAX_GROUPS) {
        Ok(v) => v,
        Err(m) => {
            return Err(ErrorEnvelope::bad_request("sanitization", instance)
                .with_detail(m)
                .into_result());
        }
    };

    let record_limit_per_group = match q.record_limit_per_group {
        None => None,
        Some(0) => {
            return Err(ErrorEnvelope::bad_request("sanitization", instance)
                .with_detail("record_limit_per_group must be > 0 when set")
                .into_result());
        }
        Some(v) => Some(v as i64),
    };

    if let Err(m) = validate_range(since, until) {
        return Err(ErrorEnvelope::bad_request("sanitization", instance)
            .with_detail(m)
            .into_result());
    }

    let bad = |m: String| {
        Err(ErrorEnvelope::bad_request("sanitization", instance)
            .with_detail(m)
            .into_result())
    };

    let decision = match normalize_decision(q.decision.as_deref()) {
        Ok(v) => v,
        Err(m) => return bad(m),
    };

    let inbound = match normalize_dir(q.dir.as_deref()) {
        Ok(v) => v,
        Err(m) => return bad(m),
    };

    // status: u16 → i64, with the same guardrails as /records.
    let status_eq = q.status.map(|v| v as i64);
    let status_min = q.status_min.map(|v| v as i64);
    let status_max = q.status_max.map(|v| v as i64);
    if let Err(m) = validate_range_opt(status_min, status_max, "status") {
        return bad(m);
    }
    if let Some(se) = status_eq
        && (status_min.is_some() || status_max.is_some())
    {
        return bad(format!(
            "Query has nonsensical constraints: specified status={se}, along with a status_min/status_max parameter"
        ));
    }
    if inbound == Some(true)
        && (status_eq.is_some() || status_min.is_some() || status_max.is_some())
    {
        return bad("Query has `dir=in` and a HTTP `status` parameter set; this can't yield any record, since only server replies (outbound messages) carry a status code.".to_string());
    }

    // len: u64 → i64, with the same guardrails as /records.
    let len = q.len.map(|v| v as i64);
    let len_min = q.len_min.map(|v| v as i64);
    let len_max = q.len_max.map(|v| v as i64);
    if let Some(l) = len
        && (len_min.is_some() || len_max.is_some())
    {
        return bad(format!(
            "Query has nonsensical constraints: specified len={l}, along with a len_min/len_max parameter"
        ));
    }
    if let Err(m) = validate_range_opt(len_min, len_max, "length") {
        return bad(m);
    }

    // conn_id: u64 → i64, exact vs min/max conflict, and any conn_id filter needs service+port.
    let conn_id = q.conn_id.map(|v| v as i64);
    let conn_id_min = q.conn_id_min.map(|v| v as i64);
    let conn_id_max = q.conn_id_max.map(|v| v as i64);
    if conn_id.is_some() && (conn_id_min.is_some() || conn_id_max.is_some()) {
        return bad(
            "Query has nonsensical constraints: specified conn_id, along with a conn_id_min/conn_id_max parameter".to_string(),
        );
    }
    if let Err(m) = validate_range_opt(conn_id_min, conn_id_max, "conn_id") {
        return bad(m);
    }
    if (conn_id.is_some() || conn_id_min.is_some() || conn_id_max.is_some())
        && (q.service_id.is_none() || q.port.is_none())
    {
        return bad(
            "Query filters by `conn_id` (exact or range) without `service_id` AND `port`; this would join unrelated records (connection ids are not unique across filters)".to_string(),
        );
    }

    let protocols = match parse_protocols(q.protocol.as_deref()) {
        Ok(v) => v,
        Err(m) => return bad(m),
    };

    // message-count-per-connection: u64 → i64, exact vs min/max conflict, range.
    let msg_count = q.msg_count.map(|v| v as i64);
    let msg_count_min = q.msg_count_min.map(|v| v as i64);
    let msg_count_max = q.msg_count_max.map(|v| v as i64);
    if msg_count.is_some() && (msg_count_min.is_some() || msg_count_max.is_some()) {
        return bad(
            "Query has nonsensical constraints: specified msg_count, along with a msg_count_min/msg_count_max parameter".to_string(),
        );
    }
    if let Err(m) = validate_range_opt(msg_count_min, msg_count_max, "msg_count") {
        return bad(m);
    }

    // Drop-reason filters only activate when all three are present (same as /records).
    let (drop_kind, drop_item_type, drop_value) =
        match (&q.drop_kind, &q.drop_item_type, &q.drop_value) {
            (Some(_), Some(_), Some(_)) => (q.drop_kind, q.drop_item_type, q.drop_value),
            _ => (None, None, None),
        };

    Ok(DBNormalizedGroups {
        service_id: q.service_id,
        port: q.port,
        since: u64_to_i64_sat(since),
        until: u64_to_i64_sat(until),
        limit,
        record_limit_per_group,
        asc,

        positive_markers: split_csv(q.positive_markers.as_deref()),
        negative_markers: split_csv(q.negative_markers.as_deref()),
        protocols,
        msg_count,
        msg_count_min,
        msg_count_max,
        inbound,
        decision,
        conn_id,
        conn_id_min,
        conn_id_max,

        method: q.method,
        // The grouped SQL does `http_path LIKE ?` expecting a pre-wrapped pattern,
        // so wrap here for substring ("contains") matching — same as /records.
        path_like: wrap_like(q.path_like),
        status_eq,
        status_min,
        status_max,

        len,
        len_min,
        len_max,

        drop_kind,
        drop_item_type,
        drop_value,
    })
}

#[utoipa::path(
    get,
    path = "/records/grouped",
    params(CtrlRequestGetQueryRecordsGrouped),
    responses(
        (status = 200, description = "OK",                              body = OkEnvelope<CtrlReplyGroupedRecordsOkData>),
        (status = 400, description = "Invalid query",                   body = ErrorEnvelope),
        (status = 401, description = "Unauthorized",                    body = ErrorEnvelope),
        (status = 404, description = "Unknown service/port",            body = ErrorEnvelope),
        (status = 500, description = "Server error",                    body = ErrorEnvelope),
    ),
    tags = ["controller", "records"]
)]
pub async fn get_grouped_records(
    OriginalUri(uri): OriginalUri,
    Query(q): Query<CtrlRequestGetQueryRecordsGrouped>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<Value>) {
    // normalize & validate
    let norm = match build_groups_norm(q, uri.path()) {
        Ok(n) => n,
        Err(e) => return e,
    };

    // optional: verify service/port existence
    if let Some(ref sid) = norm.service_id {
        let g = state.read();
        let Some(svc) = g.services.get(sid) else {
            return ErrorEnvelope::not_found("service_not_found", uri.path())
                .with_detail(format!("service_id={sid} not found"))
                .into_result();
        };
        if let Some(p) = norm.port
            && !svc.ports.contains_key(&p)
        {
            return ErrorEnvelope::not_found("port_not_found", uri.path())
                .with_detail(format!("port={p} not found for service_id={sid}"))
                .into_result();
        }
    }

    // DB call in blocking thread
    let db = state.db();
    let res = tokio::task::spawn_blocking(move || {
        load_records_grouped_agg_advanced(
            &db.path,
            norm.service_id.as_deref(),
            norm.port,
            norm.since,
            norm.until,
            norm.limit,
            norm.asc,
            norm.record_limit_per_group,
            &norm.positive_markers,
            &norm.negative_markers,
            norm.inbound,
            norm.decision,
            norm.conn_id,
            norm.conn_id_min,
            norm.conn_id_max,
            norm.method.as_deref(),
            norm.path_like.as_deref(),
            norm.status_eq,
            norm.status_min,
            norm.status_max,
            norm.len,
            norm.len_min,
            norm.len_max,
            &norm.protocols,
            norm.msg_count,
            norm.msg_count_min,
            norm.msg_count_max,
            norm.drop_kind.as_deref(),
            norm.drop_item_type.as_deref(),
            norm.drop_value.as_deref(),
        )
    })
    .await;

    match res {
        Ok(Ok(page)) => ok(CtrlReplyGroupedRecordsOkData {
            groups: page.groups,
            next_since: u64::try_from(page.next_since).unwrap_or(0),
            next_until: u64::try_from(page.next_until).unwrap_or(0),
        }),
        Ok(Err(e)) => ErrorEnvelope::internal("db_query_failure", uri.path())
            .with_detail(e.to_string())
            .into_result(),
        Err(e) => ErrorEnvelope::internal("db_query_failure_join", uri.path())
            .with_detail(e.to_string())
            .into_result(),
    }
}
