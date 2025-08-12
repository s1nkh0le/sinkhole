use crate::api::routes_filter::SharedState;
use axum::Json;
use axum::extract::{OriginalUri, State};
use axum::http::StatusCode;
use serde_json::json;

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

/// /version
///
/// Returns general metadata to the
#[utoipa::path(
    get,
    path = "/version",
    params(
        // ("token" = String, Query, description = "Authentication token"),
    ),
    responses(
        (status = 200, description = "OK")
    ),
    tag = "filter"
)]
pub async fn get_version(
    OriginalUri(_uri): OriginalUri,
    State((cfg, _registry, _db)): State<SharedState>,
) -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "data": {
            // use your hardcoded version or env!("CARGO_PKG_VERSION")
            "version": "0.0.1",
            "config": {
                "upstream_host": cfg.upstream_host,
                "ports": cfg.ports,                          // Vec<PortBinding> is Serialize
                "ctrl_bind": cfg.ctrl_bind.to_string(),      // SocketAddr -> String for stable JSON
                "debug": cfg.debug,
                "body_limit_bytes": cfg.body_limit_bytes,
                "hydrate_db": cfg.hydrate_db,
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
    params(
        // ("token" = String, Query, description = "Authentication token"),
    ),
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
