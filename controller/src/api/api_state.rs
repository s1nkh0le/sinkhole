use crate::observability::summary::{build_snapshot, dump_controller_state, ServiceSnapshot};
use crate::state::{ControllerState, ServiceMetadata};
use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;
use shared::api::ok::{ok, OkEnvelope};
use utoipa::ToSchema;


/// Successful controller fetch reply
#[derive(Serialize, ToSchema)]
pub struct StatusOkData {
    /// Services present on the controller
    pub services: Vec<ServiceSnapshot>
}
impl shared::api::ok::sealed::Sealed for StatusOkData {}
impl shared::api::ok::OkData for StatusOkData {}


/// Returns the controller's fetch, with detailed information and metadata, along with all info for services and ports 
#[utoipa::path(
    get,
    path = "/fetch",
    responses(
        (status = 200, description = "OK", body = OkEnvelope<StatusOkData>),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "controller"
)]
pub async fn get_state(
    State(state): State<ControllerState>,
) -> (StatusCode, Json<serde_json::Value>) {
    //dump_controller_state(&fetch);

    let snap = build_snapshot(&state);
    ok(StatusOkData { services: snap.services })
}
