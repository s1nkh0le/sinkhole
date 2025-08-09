use crate::state::Health::Healthy;
use crate::state::ControllerState;
use axum::http::StatusCode;
use axum::{
    extract::{OriginalUri, State},
    Json,
};
use serde::Serialize;
use serde_json::Value;
use shared::api::err::ErrorEnvelope;
use shared::api::ok::{ok, OkEnvelope};
use utoipa::ToSchema;

// ─────────────────────────── /health ───────────────────────────

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
    responses((status = 200, description = "OK", body = OkEnvelope<HealthOkData>)),
    tags = ["controller", "unauthenticated"]
)]
pub async fn get_health(
    OriginalUri(_uri): OriginalUri,
) -> (StatusCode, Json<Value>) {
    ok(HealthOkData { healthy: true })
}

// ─────────────────────────── /services ─────────────────────────

#[derive(Serialize, ToSchema)]
pub struct ServiceSummary {
    pub id: String,
    pub url: String,
    pub healthy: bool,
    pub last_sync_ns: u64,
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
    params(("ctrl_token" = String, Query, description = "Authentication token")),
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
    // Take a read snapshot and build a deterministic summary.
    let services = {
        let g = state.read();

        // Collect, then sort by service id for stable output
        let mut rows: Vec<ServiceSummary> = g.services.iter().map(|(id, svc)| {
            // Snapshot + sort port list for diff-friendly responses
            let mut ports: Vec<u16> = svc.ports.keys().copied().collect();
            ports.sort_unstable();

            ServiceSummary {
                id: id.clone(),
                url: svc.cfg.base_url.clone(),
                healthy: matches!(svc.metadata.health, Healthy),
                last_sync_ns: svc.metadata.last_state_sync_ns,
                ports,
            }
        }).collect();

        rows.sort_by(|a, b| a.id.cmp(&b.id));
        rows
    };

    ok(ServicesOkData { services })
}