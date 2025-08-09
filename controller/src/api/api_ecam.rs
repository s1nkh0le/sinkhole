use crate::db::query;
use crate::state::ctrl::ControllerState;
use axum::extract::{OriginalUri, Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use shared::api::err::ErrorEnvelope;
use shared::api::ok::{ok, OkEnvelope};
use shared::types::ecam_types::EcamEvent;
use utoipa::{IntoParams, ToSchema};
use shared::api::normalization::{clamp_limit, normalize_order, u64_to_i64_sat, validate_range};


/// Database level normalized query parameters for records query
/// (Note: using i64 types for SQLite)
#[derive(Debug)]
struct DBEcamNorm {
    service_id: Option<String>,
    since: i64,
    until: i64,
    limit: usize,
    asc: bool,
}


/// Build the normalized DB query or return a ready-made error envelope.
fn build_ecam_norm(
    service_id: Option<String>,
    since: Option<u64>,
    until: Option<u64>,
    limit: Option<usize>,
    order: Option<String>,
    instance: &str,
) -> Result<DBEcamNorm, (StatusCode, Json<Value>)> {
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

    Ok(DBEcamNorm {
        service_id,
        since: u64_to_i64_sat(since),
        until: u64_to_i64_sat(until),
        limit,
        asc,
    })
}




/// Successful ECAM reply
#[derive(Serialize, ToSchema)]
pub struct EcamOkData {
    pub records: Vec<EcamEvent>,
    pub next_since: u64,
}
impl shared::api::ok::sealed::Sealed for EcamOkData {}
impl shared::api::ok::OkData for EcamOkData {}


/// Query format for /ecam
#[derive(Deserialize, Debug, IntoParams)]
pub struct CtrlRequestGetQueryEcamAgg {
    pub service_id: Option<String>,
    pub since: Option<u64>,
    pub until: Option<u64>,
    pub limit: Option<usize>,
    /// "asc" | "desc"
    pub order: Option<String>,
}

/// /ecam
///
/// Queries the aggregated database and returns a list of ECAM messages (`EcamEvent`) matching the given predicates, along with a cursor
#[utoipa::path(
    get,
    path = "/ecam",
    params(
        ("ctrl_token" = String, Query,                              description = "Authentication token"),
    ),
    responses(
        (status = 200, description = "OK",                          body = OkEnvelope<EcamOkData>),
        (status = 400, description = "Invalid query",               body = ErrorEnvelope),
        (status = 401, description = "Unauthorized",                body = ErrorEnvelope),
        (status = 404, description = "Unknown service_id",          body = ErrorEnvelope),
        (status = 500, description = "Server error",                body = ErrorEnvelope),
    ),
    tags = ["controller", "ecam"]
)]
pub async fn get_aggregated_ecam(
    OriginalUri(instance): OriginalUri,
    Query(q): Query<CtrlRequestGetQueryEcamAgg>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<Value>) {
    let norm = match build_ecam_norm(
        q.service_id.clone(),
        q.since,
        q.until,
        q.limit,
        q.order.clone(),
        instance.path(),
    ) {
        Ok(n)  => n,
        Err(e) => return e,
    };

    ecam_core(state, norm, instance.path()).await
}



/// Query format for /services/{service_id}/ecam
///
/// Doesn't allow to specify the service_id, since it's a path parameter
#[derive(Deserialize, Debug, IntoParams)]
pub struct CtrlRequestGetQueryEcamFromPort {
    pub since: Option<u64>,
    pub until: Option<u64>,
    pub limit: Option<usize>,
    pub order: Option<String>,
}

/// /services/{service_id}/ecam
///
/// Queries the aggregated database and returns a list of ECAM messages (`EcamEvent`) matching the given predicates, along with a cursor
#[utoipa::path(
    get,
    path = "/services/{service_id}/ecam",
    params(EcamServicePath),
    params(
        ("ctrl_token" = String, Query,                          description = "Authentication token"),
        ("service_id" = String, Path,                           description = "Service id")
    ),
    responses(
        (status = 200, description = "OK",                      body = OkEnvelope<EcamOkData>),
        (status = 400, description = "Invalid query",           body = ErrorEnvelope),
        (status = 401, description = "Unauthorized",            body = ErrorEnvelope),
        (status = 404, description = "Unknown service_id",      body = ErrorEnvelope),
        (status = 500, description = "Server error",            body = ErrorEnvelope),
    ),
    tags = ["controller", "ecam"]
)]
pub async fn get_service_ecam(
    OriginalUri(instance): OriginalUri,
    Path(service_id): Path<String>,
    Query(q): Query<CtrlRequestGetQueryEcamFromPort>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<Value>) {
    let norm = match build_ecam_norm(
        Some(service_id),
        q.since,
        q.until,
        q.limit,
        q.order.clone(),
        instance.path(),
    ) {
        Ok(n)  => n,
        Err(e) => return e,
    };

    ecam_core(state, norm, instance.path()).await
}




// Underlying DB-handler for ECAM queries
#[inline]
async fn ecam_core(
    state: ControllerState,
    q: DBEcamNorm,
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
            next_since: u64::try_from(next_since).unwrap(),
        }),
        Ok(Err(e)) => ErrorEnvelope::internal("db_query_failure", instance).with_detail(e.to_string()).into_result(),
        Err(e) => ErrorEnvelope::internal("db_query_failure_join", instance).with_detail(e.to_string()).into_result()
    }
}
