use crate::api::auth;
use crate::api::routes::SharedState;
use crate::observability::sinks::sink_ecam::{ECAM_SINK, EcamEvent};
use axum::extract::State;
use axum::http::StatusCode;
use axum::{Json, extract::Query};
use serde::{Deserialize, Serialize};
use serde_json::json;

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

///     API: `GET /ecam`
///     method: `get_ecam`
pub async fn get_ecam(
    Query(q): Query<EcamRequest>,
    State((cfg, _registry, _db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(e) = auth::check_token(q.token.as_deref(), &cfg) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"status":"error","errors":[e.to_string()]})),
        );
    }

    // ── params ─────────────────────────────────────────
    let since_ns = q.since.and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);

    // grab a **Vec** snapshot
    let mut events = ECAM_SINK.snapshot(since_ns);

    // sort ascending
    events.sort_by_key(|e| e.ts_ns);

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
