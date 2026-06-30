use crate::api::routes_filter::SharedState;
use crate::observability::sinks::sink_ecam;
use crate::observability::sinks::sink_ecam::{Origin, Severity};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde_json::json;

/// /ports/{port}/stats
///
/// Per-port statistics are not collected yet. This endpoint only validates the
/// port (404 if unknown) and reports that stats are disabled; it is a
/// placeholder for future counters.
#[utoipa::path(
    put,
    path = "/ports/{port}/stats",
    params(
        ("port" = u16, Path,                        description = "Public port"),
    ),
    responses(
        (status = 200,                              description = "OK"),
        (status = 401,                              description = "Unauthorized"),
        (status = 404,                              description = "Unknown port"),
    ),
    tag = "filter"
)]
pub async fn get_port_stats(
    Path(port): Path<u16>,
    State((_cfg, registry, _db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match registry.get(&port) {
        Some(_) => (
            StatusCode::OK,
            Json(json!({"status":"ok","data":"stats disabled"})),
        ),
        None => {
            sink_ecam::record(
                Severity::Warn,
                Origin::API,
                format!("[GET /ports/{port}/stats, get_stats()]: port not found"),
                None,
            );

            (
                StatusCode::NOT_FOUND,
                Json(json!({"status":"error","errors":["unknown port"]})),
            )
        }
    }
}
