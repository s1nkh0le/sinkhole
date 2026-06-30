use crate::api::routes_filter::SharedState;
use axum::Json;
use axum::extract::{OriginalUri, State};
use axum::http::StatusCode;
use serde_json::{Value, json};
use shared::api::ok::{OkEnvelope, ok};
use shared::types::routing::common::AuthPingData;

/// /health
#[utoipa::path(
    get,
    path = "/health",
    security(()),
    responses(
        (status = 200, description = "OK")
    ),
    tag = "filter"
)]
pub async fn get_health(OriginalUri(_uri): OriginalUri) -> Json<serde_json::Value> {
    Json(json!({"status":"ok"}))
}

/// /auth/ping
///
/// Authenticated reachability check: a 200 confirms the filter is up and the
/// caller's token is valid. `data.instance` is `"filter"` so the frontend can tell
/// which kind of node it reached (same shape as the controller's `/auth/ping`).
#[utoipa::path(
    get,
    path = "/auth/ping",
    responses(
        (status = 200, description = "OK", body = OkEnvelope<AuthPingData>),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "filter"
)]
pub async fn get_auth_ping(OriginalUri(_uri): OriginalUri) -> (StatusCode, Json<Value>) {
    ok(AuthPingData::healthy(AuthPingData::FILTER))
}

/// /version
///
/// Returns this filter's version string and a snapshot of its effective runtime
/// config (upstream host, port bindings, control bind, limits, db path) so the
/// controller can record and display what each service is running.
#[utoipa::path(
    get,
    path = "/version",
    responses(
        (status = 200, description = "OK")
    ),
    tag = "filter"
)]
pub async fn get_version(
    OriginalUri(_uri): OriginalUri,
    State((cfg, _registry, _db)): State<SharedState>,
) -> Json<serde_json::Value> {
    let ports: Vec<_> = cfg
        .ports
        .iter()
        .map(|p| {
            json!({
                "public": p.public,
                "upstream": p.upstream,
                "tls": p.tls_enabled(),
            })
        })
        .collect();
    Json(json!({
        "status": "ok",
        "data": {
            "version": env!("CARGO_PKG_VERSION"),
            "config": {
                "upstream_host": cfg.upstream_host,
                "ports": ports,
                "control_bind": cfg.control_bind.to_string(), // SocketAddr -> String for stable JSON
                "debug": cfg.debug,
                "body_limit_bytes": cfg.body_limit_bytes,
                "db_hydrate": cfg.db_hydrate,
                "db_path": cfg.db_path,
            }
        }
    }))
}

/// /teapot
///
/// Checks whether there are any active tea filters in the server
#[utoipa::path(
    get,
    path = "/teapot",
    responses(
        (status = 401, description = "Unauthorized"),
        (status = 418, description = "I'm a teapot")
    ),
    tag = "filter"
)]
pub async fn get_teapot(
    OriginalUri(_uri): OriginalUri,
    State((_cfg, _registry, _db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::IM_A_TEAPOT,
        Json(json!({
            "status":"error",
            "message": "Refill tea filters"
        })),
    )
}
