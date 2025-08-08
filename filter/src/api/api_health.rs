use crate::api::routes_filter::SharedState;
use axum::extract::{OriginalUri, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::json;

#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "OK")
    ),
    tag = "filter"
)]
pub async fn get_health(
    OriginalUri(_uri): OriginalUri
) -> Json<serde_json::Value> {
    Json(json!({"status":"ok"}))
}


#[utoipa::path(
    get,
    path = "/version",
    params(
        ("token" = String, Query, description = "Authentication token"),
    ),
    responses(
        (status = 200, description = "OK")
    ),
    tag = "filter"
)]
pub async fn get_version(
    OriginalUri(_uri): OriginalUri,
    State((cfg, _registry, _db)): State<SharedState>
) -> Json<serde_json::Value> {
    Json(json!({
        "status":"ok",
        "data": {"version":"0.0.1"},
        "debug": cfg.debug
    }))
}


#[utoipa::path(
    get,
    path = "/teapot",
    params(
        ("token" = String, Query, description = "Authentication token"),
    ),
    responses(
        (status = 401, description = "Unauthorized"),
        (status = 418, description = "I'm a teapot")
    ),
    tag = "filter"
)]
pub async fn get_teapot(
    OriginalUri(_uri): OriginalUri,
    State((_cfg, _registry, _db)): State<SharedState>
) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::IM_A_TEAPOT,
        Json(json!({
            "status":"error",
            "message": "Refill tea filters"
        }))
    )
}