use crate::api::middleware_auth;
use crate::api::routes_filter::SharedState;
use crate::observability::sinks::sink_ecam::{ECAM_SINK, EcamEvent};
use axum::extract::{OriginalUri, State};
use axum::http::StatusCode;
use axum::{Json, extract::Query};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::log::debug;
use utoipa::{IntoParams, ToSchema};
use shared::api::ok::{ok, OkEnvelope};
use shared::types::routing::filter::ecam::{FilterReplyEcamAndCursor, FilterRequestGetQueryEcamQuery};

/// /ecam
/// 
/// Queries the database and returns a list of ECAM messages (`EcamEvent`) matching the given predicates, along with a cursor
#[utoipa::path(
    get,
    path = "/ecam",
    params(
        ("token" = String, Query, description = "Authentication token"),
    ),
    responses(
        (status = 200, description = "OK", body = OkEnvelope<FilterReplyEcamAndCursor>),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "filter"
)]
pub async fn get_ecam(
    OriginalUri(uri): OriginalUri,
    Query(q): Query<FilterRequestGetQueryEcamQuery>,
    State((_cfg, _registry, _db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    // ── params ─────────────────────────────────────────
    let since_ns = q.since.unwrap_or(0);

    // grab a **Vec** snapshot
    let mut events = ECAM_SINK.snapshot(since_ns);

    // sort ascending by timestamp
    events.sort_by_key(|e| e.ts_ns);

    // compute next_since = (max + 1ns), or stick to caller’s since_ns
    let next_since = events.last().map(|e| e.ts_ns + 1).unwrap_or(since_ns);

    // print debug log
    debug!("[get_ecam(), GET {}]: got {} ecam events: {:?}", uri.path(), events.clone().len(), events.clone());

    ok(FilterReplyEcamAndCursor { events, next_since })
}
