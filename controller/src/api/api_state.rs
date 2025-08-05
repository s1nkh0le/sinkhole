use crate::observability::summary::{build_snapshot, dump_controller_state, ServiceSnapshot};
use crate::state::ControllerState;
use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;
use shared::api::ok::{ok, OkEnvelope};
use utoipa::ToSchema;



/// 200-OK `data` payload for ruleset endpoints.
#[derive(Serialize, ToSchema)]
pub struct ServicesOkData {
    pub services: Vec<ServiceSnapshot>,
}
impl shared::api::ok::sealed::Sealed for ServicesOkData {}
impl shared::api::ok::OkData for ServicesOkData {}


#[utoipa::path(
    get,
    path = "/state",
    responses(
        (status = 200, description = "OK", body = OkEnvelope<ServicesOkData>),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "controller"
)]
pub async fn get_state(
    State(state): State<ControllerState>,
) -> (StatusCode, Json<serde_json::Value>) {
    dump_controller_state(&state);

    let snap = build_snapshot(&state);
    ok(ServicesOkData { services: snap.services })
}
