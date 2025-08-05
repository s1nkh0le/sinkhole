use crate::db::query;
use crate::state::ControllerState;
use axum::extract::{OriginalUri, Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use shared::api::err::ErrorEnvelope;
use shared::api::ok::{ok, OkEnvelope};
use shared::api::{clamp_limit, normalize_order, validate_range};
use shared::types::ecam_types::EcamEvent;
use utoipa::{IntoParams, ToSchema};

/// Successful ECAM reply
#[derive(Serialize, ToSchema)]
pub struct EcamOkData {
    pub records: Vec<EcamEvent>,
    pub next_since: i64,
}
impl shared::api::ok::sealed::Sealed for EcamOkData {}
impl shared::api::ok::OkData for EcamOkData {}

#[derive(Deserialize, Debug, IntoParams)]
pub struct EcamAggQuery {
    pub service_id: Option<String>,
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub limit: Option<usize>,
    pub order: Option<String>,
}

#[utoipa::path(
    get,
    path = "/ecam",
    params(EcamAggQuery),
    params(
        ("ctrl_token" = String, description = "Authentication token"),
    ),
    responses(
        (status = 200, description = "OK", body = OkEnvelope<EcamOkData>),
        (status = 400, description = "Invalid query", body = ErrorEnvelope),
        (status = 401, description = "Unauthorized", body = ErrorEnvelope),
        (status = 404, description = "Unknown service_id", body = ErrorEnvelope),
        (status = 500, description = "Server error", body = ErrorEnvelope),
    ),
    tags = ["controller", "ecam"]
)]
pub async fn get_aggregated_ecam(
    OriginalUri(instance): OriginalUri,
    Query(q): Query<EcamAggQuery>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<Value>) {
    let since = q.since.unwrap_or(0);
    let until = q.until.unwrap_or(i64::MAX);

    let asc = match normalize_order(q.order.as_deref()) {
        Ok(v) => v,
        Err(m) => return ErrorEnvelope::bad_request("sanitization", instance.to_string()).with_detail(m).into_result(),
    };
    let limit = match clamp_limit(q.limit, 10_000) {
        Ok(v) => v,
        Err(m) => return ErrorEnvelope::bad_request("sanitization", instance.to_string()).with_detail(m).into_result(),
    };

    if let Err(m) = validate_range(since, until) {
        return ErrorEnvelope::bad_request("sanitization", instance.to_string()).with_detail(m).into_result()
    }

    let norm = EcamNorm {
        service_id: q.service_id.clone(),
        since,
        until,
        limit,
        asc,
    };

    ecam_core(state, norm, &instance.to_string()).await
}





#[derive(Deserialize, Debug, IntoParams)]
pub struct EcamQueryNoSvc {
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub limit: Option<usize>,
    pub order: Option<String>,
}

#[utoipa::path(
    get,
    path = "/services/{service_id}/ecam",
    params(EcamServicePath),
    params(
        ("ctrl_token" = String, description = "Authentication token"),
        ("service_id" = String, Path, description = "Service id")
    ),
    responses(
        (status = 200, description = "OK", body = OkEnvelope<EcamOkData>),
        (status = 400, description = "Invalid query", body = ErrorEnvelope),
        (status = 401, description = "Unauthorized", body = ErrorEnvelope),
        (status = 404, description = "Unknown service_id", body = ErrorEnvelope),
        (status = 500, description = "Server error", body = ErrorEnvelope),
    ),
    tags = ["controller", "ecam"]
)]
pub async fn get_service_ecam(
    OriginalUri(instance): OriginalUri,
    Path(service_id): Path<String>,
    Query(q): Query<EcamQueryNoSvc>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<Value>) {
    let since = q.since.unwrap_or(0);
    let until = q.until.unwrap_or(i64::MAX);

    let asc = match normalize_order(q.order.as_deref()) {
        Ok(v) => v,
        Err(m) => return ErrorEnvelope::bad_request("sanitization", instance.to_string()).with_detail(m).into_result()
    };
    let limit = match clamp_limit(q.limit, 10_000) {
        Ok(v) => v,
        Err(m) => return ErrorEnvelope::bad_request("sanitization", instance.to_string()).with_detail(m).into_result()
    };
    if let Err(m) = validate_range(since, until) {
        return ErrorEnvelope::bad_request("sanitization", instance.to_string()).with_detail(m).into_result()
    }

    // path service_id prevails
    let norm = EcamNorm {
        service_id: Some(service_id),
        since,
        until,
        limit,
        asc,
    };

    ecam_core(state, norm, &instance.path()).await
}

struct EcamNorm {
    service_id: Option<String>,
    since: i64,
    until: i64,
    limit: usize,
    asc: bool,
}

async fn ecam_core(
    state: ControllerState,
    q: EcamNorm,
    instance: &str,
) -> (StatusCode, Json<Value>) {
    // Optional: validate service existence if present
    if let Some(ref sid) = q.service_id {
        if !state.read().services.contains_key(sid) {
            return ErrorEnvelope::not_found("service_not_found", instance).with_detail(format!("service_id={sid} not found")).into_result()
        }
    }

    // Query DB (spawn_blocking)
    let db = state.db();
    let svc = q.service_id.clone();
    let join_res = tokio::task::spawn_blocking(move || {
        query::load_ecam_agg(&db.path, svc.as_deref(), q.since, q.until, q.limit, q.asc)
    })
    .await;

    match join_res {
        Ok(Ok((events, next_since))) => ok(EcamOkData {
            records: events,
            next_since,
        }),
        Ok(Err(e)) => ErrorEnvelope::internal("db_query_failure", instance).with_detail(e.to_string()).into_result(),
        Err(e) => ErrorEnvelope::internal("db_query_failure_join", instance).with_detail(e.to_string()).into_result()
    }
}
