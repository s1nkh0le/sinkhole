use axum::{
    extract::{OriginalUri, State},
    Json,
};
use axum::http::StatusCode;
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;
use shared::api::err::ErrorEnvelope;
use shared::api::ok::{ok, OkEnvelope};
use crate::state::ControllerState;

/// 200 OK payload for /health
#[derive(Serialize, ToSchema)]
pub struct HealthOkData {
    /// Indicates the controller process is up.
    pub healthy: bool,
}
impl shared::api::ok::sealed::Sealed for HealthOkData {}
impl shared::api::ok::OkData for HealthOkData {}



#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "OK", body = OkEnvelope<HealthOkData>),
        (status = 500, description = "Server error", body = ErrorEnvelope),
    ),
    tag = "controller"
)]
pub async fn get_health(
    OriginalUri(_uri): OriginalUri,
) -> (StatusCode, Json<Value>) {
    ok(HealthOkData { healthy: true })
}








/// Short summary for each known service.
#[derive(Serialize, ToSchema)]
pub struct ServiceSummary {
    pub id: String,
    pub url: String,
    pub healthy: bool,
    pub last_sync_ns: i64,
    pub ports: Vec<u16>,
}

#[derive(Serialize, ToSchema)]
pub struct ServicesOkData {
    pub services: Vec<ServiceSummary>,
}
impl shared::api::ok::sealed::Sealed for ServicesOkData {}
impl shared::api::ok::OkData for ServicesOkData {}

#[utoipa::path(
    get,
    path = "/services",
    responses(
        (status = 200, description = "OK", body = OkEnvelope<ServicesOkData>),
        (status = 401, description = "Unauthorized", body = ErrorEnvelope),
        (status = 500, description = "Server error", body = ErrorEnvelope),
    ),
    tag = "controller"
)]
pub async fn get_list_services(
    OriginalUri(_uri): OriginalUri,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<Value>) {
    let guard = state.read();
    let services = guard
        .services
        .iter()
        .map(|(id, svc)| ServiceSummary {
            id: id.clone(),
            url: svc.cfg.base_url.clone(),
            healthy: svc.healthy,
            last_sync_ns: svc.last_sync_ns,
            ports: svc.ports.keys().copied().collect(),
        })
        .collect();

    ok(ServicesOkData { services })
}
