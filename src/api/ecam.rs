use axum::{extract::Query, Json};
use std::collections::HashMap;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Serialize;
use crate::api::auth;
use crate::api::routes::SharedState;
use crate::observability::sinks::sink_ecam::{EcamEvent, ECAM_SINK};

#[derive(Serialize)]
pub struct ApiResp<T> {
    pub status: &'static str,
    pub data: T,
}

impl<T> ApiResp<T> {
    pub fn ok(data: T) -> Self { Self { status: "ok", data } }
}


/// GET /ecam?token=…&since=<ns>
pub async fn get_ecam(
    Path(_port): Path<u16>,
    Query(q): Query<HashMap<String, String>>,
    State((cfg, _registry, _db)): State<SharedState>,
) -> (StatusCode, Json<ApiResp<Vec<EcamEvent>>>) {
    // ── auth ────────────────────────────────────────────
    if let Err(_e) = auth::check_token(q.get("token").map(String::as_str), &cfg) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiResp { status: "error", data: vec![] }),
        );
    }

    // ── param ───────────────────────────────────────────
    let since_ns = q.get("since").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);

    let events = ECAM_SINK.snapshot(since_ns);
    (StatusCode::OK, Json(ApiResp::ok(events)))
}