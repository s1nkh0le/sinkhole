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

/// Successful health reply
#[derive(Serialize, ToSchema)]
pub struct HealthOkData {
    /// Indicates the controller process is up.
    pub healthy: bool,
}
impl shared::api::ok::sealed::Sealed for HealthOkData {}
impl shared::api::ok::OkData for HealthOkData {}


/// Heartbeat endpoint, returning a rsponse for liveness and general system health information
#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "OK", body = OkEnvelope<HealthOkData>)
    ),
    tags = ["controller", "unauthenticated"]
)]
pub async fn get_health(
    OriginalUri(_uri): OriginalUri,
) -> (StatusCode, Json<Value>) {
    ok(HealthOkData { healthy: true })
}




/// Single service information
#[derive(Serialize, ToSchema)]
pub struct ServiceSummary {
    pub id: String,
    pub url: String,
    pub healthy: bool,
    pub last_sync_ns: i64,
    pub ports: Vec<u16>,
}

/// Successful service listing reply
#[derive(Serialize, ToSchema)]
pub struct ServicesOkData {
    pub services: Vec<ServiceSummary>,
}
impl shared::api::ok::sealed::Sealed for ServicesOkData {}
impl shared::api::ok::OkData for ServicesOkData {}



/// Returns brief summary of the controller's registered services.
/// This is meant to be a quick and synthethic version of a call to `/state`, which returns more extensive information for both services and ports.
#[utoipa::path(
    get,
    path = "/services",
    params(
        ("ctrl_token" = String, description = "Authentication token"),
    ),
    responses(
        (status = 200, description = "OK", body = OkEnvelope<ServicesOkData>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Server error", body = ErrorEnvelope),
    ),
    tag = "controller"
)]
pub async fn get_list_services(
    OriginalUri(_uri): OriginalUri,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<Value>) {
    let guard = state.read();
    // service summary derived from [Service], stripping
    let services = guard
        .services
        .iter()
        .map(|(id, svc)| ServiceSummary {
            id: id.clone(),
            url: svc.cfg.base_url.clone(),
            healthy: svc.metadata.healthy,
            last_sync_ns: svc.metadata.last_state_sync_ns,
            ports: svc.ports.keys().copied().collect(),
        })
        .collect();

    ok(ServicesOkData { services })
}
