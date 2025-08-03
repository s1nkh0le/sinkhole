use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;

use crate::observability::summary::{build_snapshot, dump_controller_state};
use crate::state::ControllerState;

#[derive(Serialize)]
pub struct ApiResp<T> {
    status: &'static str,
    data: T,
}

pub async fn get_state(
    State(state): State<ControllerState>,
) -> (StatusCode, Json<ApiResp<serde_json::Value>>) {
    dump_controller_state(&state);

    let snap = build_snapshot(&state);
    (
        StatusCode::OK,
        Json(ApiResp {
            status: "ok",
            data: serde_json::to_value(snap).unwrap(),
        }),
    )
}
