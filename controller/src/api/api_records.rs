use crate::db::query;
use crate::state::ControllerState;
use axum::extract::{OriginalUri, Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use shared::eval_record::EvaluationRecord;
use utoipa::{IntoParams, ToSchema};
use shared::api::err::{ErrorEnvelope};
use shared::api::{clamp_limit, normalize_order, validate_range};
use shared::api::ok::{ok, OkEnvelope};


// Ok envelope content
#[derive(Serialize, ToSchema)]
pub struct RecordsOkData {
    pub records: Vec<EvaluationRecord>,
    pub next_since: i64,
}
impl shared::api::ok::sealed::Sealed for RecordsOkData {}
impl shared::api::ok::OkData for RecordsOkData {}



#[derive(Deserialize, Debug, IntoParams)]
pub struct RecordsAggQuery {
    pub service_id: Option<String>,
    pub port: Option<u16>,
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub limit: Option<usize>,
    pub order: Option<String>, // "asc" | "desc"
}


#[utoipa::path(
    get,
    path = "/records",
    params(RecordsAggQuery),
    responses(
        (status = 200, description = "OK", body = OkEnvelope<RecordsOkData>),
        (status = 400, description = "Invalid query", body = ErrorEnvelope),
        (status = 401, description = "Unauthorized", body = ErrorEnvelope),
        (status = 404, description = "Unknown service/port", body = ErrorEnvelope),
        (status = 500, description = "Server error", body = ErrorEnvelope),
    ),
    tag = "controller"
)]
pub async fn get_aggregated_records(
    OriginalUri(uri): OriginalUri,
    Query(q): Query<RecordsAggQuery>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<Value>) {
    let since = q.since.unwrap_or(0);
    let until = q.until.unwrap_or(i64::MAX);

    let asc = match normalize_order(q.order.as_deref()) {
        Ok(v) => v,
        Err(m) => {
            return ErrorEnvelope::bad_request("sanitization", uri.to_string()).with_detail(m).into_result()
        }
    };

    let limit = match clamp_limit(q.limit, 10_000) {
        Ok(v) => v,
        Err(m) => { return ErrorEnvelope::bad_request("sanitization", uri.to_string()).with_detail(m).into_result() }
    };

    if let Err(m) = validate_range(since, until) {
        return ErrorEnvelope::bad_request("sanitization", uri.to_string()).with_detail(m).into_result()
    }

    if q.port.is_some() && q.service_id.is_none() {
        return ErrorEnvelope::bad_request("invalid_query", uri.to_string())
            .with_detail(format!("when `port` is set ({:?}), `service_id` is required (none supplied)", q.port))
            .into_result();
    }

    let norm = RecordsNorm {
        service_id: q.service_id.clone(),
        port: q.port,
        since,
        until,
        limit,
        asc,
    };

    records_core(state, norm, "/records").await
}





#[derive(Deserialize, Debug, IntoParams)]
pub struct RecordsQueryNoSvcPort {
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub limit: Option<usize>,
    pub order: Option<String>,
}
#[derive(Deserialize, Debug, IntoParams)]
pub struct ServicePortPath {
    pub service_id: String,
    pub port: u16,
}

#[utoipa::path(
    get,
    path = "/services/{service_id}/ports/{port}/records",
    params(ServicePortPath, RecordsQueryNoSvcPort),
    params(
        ("service_id" = String, Path, description = "Service id"),
        ("port"       = u16,    Path, description = "Port number")
    ),
    responses(
        (status = 200, description = "OK", body = OkEnvelope<RecordsOkData>),
        (status = 400, description = "Invalid query", body = ErrorEnvelope),
        (status = 401, description = "Unauthorized", body = ErrorEnvelope),
        (status = 404, description = "Unknown service/port", body = ErrorEnvelope),
        (status = 500, description = "Server error", body = ErrorEnvelope),
    ),
    tag = "controller"
)]
pub async fn get_service_port_records(
    OriginalUri(uri): OriginalUri,
    Path(path): Path<ServicePortPath>,
    Query(q): Query<RecordsQueryNoSvcPort>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<Value>) {
    let since = q.since.unwrap_or(0);
    let until = q.until.unwrap_or(i64::MAX);

    let asc = match normalize_order(q.order.as_deref()) {
        Ok(v) => v,
        Err(m) => {
            return ErrorEnvelope::bad_request("sanitization", uri.path()).with_detail(m).into_result()
        }
    };

    let limit = match clamp_limit(q.limit, 10_000) {
        Ok(v) => v,
        Err(m) => {
            return ErrorEnvelope::bad_request("sanitization", uri.path()).with_detail(m).into_result()
        }
    };

    if let Err(m) = validate_range(since, until) {
        return ErrorEnvelope::bad_request("sanitization", uri.path()).with_detail(m).into_result()
    }

    // Path values prevail
    let norm = RecordsNorm {
        service_id: Some(path.service_id),
        port: Some(path.port),
        since,
        until,
        limit,
        asc,
    };

    records_core(state, norm, uri.path()).await
}




struct RecordsNorm {
    service_id: Option<String>,
    port: Option<u16>,
    since: i64,
    until: i64,
    limit: usize,
    asc: bool,
}

async fn records_core(state: ControllerState, q: RecordsNorm, instance: &str) -> (StatusCode, Json<Value>) {
    // Best-effort existence checks (against controller state snapshot)
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

    // DB query (spawn_blocking)
    let db = state.db();
    let svc = q.service_id.clone();
    let port = q.port;
    let join_res = tokio::task::spawn_blocking(move || {
        query::load_records_agg(&db.path, svc.as_deref(), port, q.since, q.until, q.limit, q.asc)
    })
        .await;

    match join_res {
        Ok(Ok((records, next_since))) => ok(RecordsOkData { records, next_since }),
        Ok(Err(e)) => ErrorEnvelope::internal("db_query_failure", instance).with_detail(e.to_string()).into_result(),
        Err(e) => ErrorEnvelope::internal("db_query_failure_join", instance).with_detail(e.to_string()).into_result(),
    }
}
