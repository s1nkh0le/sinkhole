use crate::api::middleware_auth;
use crate::api::routes_filter::SharedState;
use crate::observability::sinks::sink_ecam::{ECAM_SINK, EcamEvent};
use axum::extract::{OriginalUri, State};
use axum::http::StatusCode;
use axum::{Json, extract::Query};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::log::debug;

#[derive(Debug, Serialize)]
pub struct ApiRespEcam<'a> {
    pub status: &'static str,
    pub data: &'a [EcamEvent],
    pub next_since: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EcamRequest {
    since: Option<String>,
    token: Option<String>,
}


#[utoipa::path(
    get,
    path = "/ecam",
    params(
        ("token" = String, description = "Authentication token"),
    ),
    responses(
        (status = 200, description = "OK"),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "filter"
)]
pub async fn get_ecam(
    OriginalUri(uri): OriginalUri,
    Query(q): Query<EcamRequest>,
    State((_cfg, _registry, _db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    // ── params ─────────────────────────────────────────
    let since_ns = q.since.and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);

    // grab a **Vec** snapshot
    let mut events = ECAM_SINK.snapshot(since_ns);

    // sort ascending
    events.sort_by_key(|e| e.ts_ns);

    // print
    debug!("[get_ecam(), GET {}]: got {} ecam events: {:?}", uri.path(), events.clone().len(), events.clone());

    // compute next_since = (max + 1)  or stick to caller’s since_ns
    let next_since = events.last().map(|e| e.ts_ns + 1).unwrap_or(since_ns);

    // we need the events to live long enough for the Json body.
    //   Box::leak makes a &'static [EcamEvent]; cheap because we immediately drop the Box after response is built
    let leaked: &'static [EcamEvent] = Box::leak(events.into_boxed_slice());

    (
        StatusCode::OK,
        Json(json!({
            "status": "ok",
            "data": leaked,
            "next_since": next_since
        })),
    )
}
