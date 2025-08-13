use axum::extract::{OriginalUri, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use shared::api::err::ErrorEnvelope;
use shared::api::normalization::{clamp_limit, normalize_order, u64_to_i64_sat, validate_range};
use shared::api::ok::{ok, OkEnvelope};
use shared::types::record_types::EvaluationRecord;
use crate::db::query_records_grouped::load_records_grouped_agg_advanced;
use crate::state::ctrl::ControllerState;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CtrlReplyGroupedRecordsOkData {
    pub groups: Vec<RecordGroup>,
    pub next_since: u64,
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









#[derive(Debug, Clone, Serialize, Deserialize, IntoParams)]
pub struct CtrlRequestGetQueryRecordsGrouped {
    pub service_id: Option<String>,
    pub port: Option<u16>,
    pub since: Option<u64>,
    pub until: Option<u64>,
    pub limit: Option<usize>,
    pub order: Option<String>,

    pub positive_markers: Option<Vec<String>>,
    pub negative_markers: Option<Vec<String>>,
    pub inbound: Option<bool>,
    pub decision: Option<String>,
    pub conn_id: Option<i64>,

    pub method: Option<String>,
    pub path_like: Option<String>,
    pub status_eq: Option<i64>,
    pub status_min: Option<i64>,
    pub status_max: Option<i64>,

    pub len: Option<i64>,
    pub len_min: Option<i64>,
    pub len_max: Option<i64>,

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
    asc: bool,

    positive_markers: Vec<String>,
    negative_markers: Vec<String>,
    inbound: Option<bool>,
    decision: Option<String>,
    conn_id: Option<i64>,

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

fn build_groups_norm(q: CtrlRequestGetQueryRecordsGrouped, instance: &str)
                     -> Result<DBNormalizedGroups, (StatusCode, Json<Value>)>
{
    // port ⇒ service_id
    if q.port.is_some() && q.service_id.is_none() {
        return Err(
            ErrorEnvelope::bad_request("invalid_query", instance)
                .with_title("Port requires service_id")
                .with_detail("when `port` is set, `service_id` is required")
                .into_result()
        );
    }

    let since = q.since.unwrap_or(0);
    let until = q.until.unwrap_or(i64::MAX as u64);

    let asc = match normalize_order(q.order.as_deref()) {
        Ok(v) => v,
        Err(m) => return Err(ErrorEnvelope::bad_request("sanitization", instance).with_detail(m).into_result()),
    };

    const MAX_GROUPS: usize = 500; // keep params under SQLite cap
    let limit = match clamp_limit(q.limit, MAX_GROUPS) {
        Ok(v) => v,
        Err(m) => return Err(ErrorEnvelope::bad_request("sanitization", instance).with_detail(m).into_result()),
    };

    if let Err(m) = validate_range(since, until) {
        return Err(ErrorEnvelope::bad_request("sanitization", instance).with_detail(m).into_result());
    }

    Ok(DBNormalizedGroups {
        service_id: q.service_id,
        port: q.port,
        since: u64_to_i64_sat(since),
        until: u64_to_i64_sat(until),
        limit,
        asc,

        positive_markers: q.positive_markers.unwrap_or_default(),
        negative_markers: q.negative_markers.unwrap_or_default(),
        inbound: q.inbound,
        decision: q.decision,
        conn_id: q.conn_id,

        method: q.method,
        path_like: q.path_like,
        status_eq: q.status_eq,
        status_min: q.status_min,
        status_max: q.status_max,

        len: q.len,
        len_min: q.len_min,
        len_max: q.len_max,

        drop_kind: q.drop_kind,
        drop_item_type: q.drop_item_type,
        drop_value: q.drop_value,
    })
}







#[utoipa::path(
    get,
    path = "/records/grouped",
    params(
        // ("ctrl_token" = String, Query,                               description = "Authentication token"),
    ),
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
        Ok(n)  => n,
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
        if let Some(p) = norm.port {
            if !svc.ports.contains_key(&p) {
                return ErrorEnvelope::not_found("port_not_found", uri.path())
                    .with_detail(format!("port={p} not found for service_id={sid}"))
                    .into_result();
            }
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
            &norm.positive_markers,
            &norm.negative_markers,
            norm.inbound,
            norm.decision.as_deref(),
            norm.conn_id,
            norm.method.as_deref(),
            norm.path_like.as_deref(),
            norm.status_eq,
            norm.status_min,
            norm.status_max,
            norm.len,
            norm.len_min,
            norm.len_max,
            norm.drop_kind.as_deref(),
            norm.drop_item_type.as_deref(),
            norm.drop_value.as_deref(),
        )
    }).await;

    match res {
        Ok(Ok((groups, next_since))) => ok(CtrlReplyGroupedRecordsOkData {
            groups,
            next_since: u64::try_from(next_since).unwrap_or(0),
        }),
        Ok(Err(e)) => ErrorEnvelope::internal("db_query_failure", uri.path()).with_detail(e.to_string()).into_result(),
        Err(e) => ErrorEnvelope::internal("db_query_failure_join", uri.path()).with_detail(e.to_string()).into_result(),
    }
}



