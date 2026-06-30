use std::sync::Arc;

use crate::supervisor::supervisor::Supervisor;
use crate::{env::ServiceCfg, state::ctrl::ControllerState};
use axum::{
    Json,
    extract::{OriginalUri, Path, State},
    http::StatusCode,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use shared::api::err::ErrorEnvelope;
use shared::api::ok::{OkEnvelope, ok};
use utoipa::ToSchema;

/// Request body for adding a new service (runtime).
#[derive(Debug, Deserialize, ToSchema)]
pub struct CtrlServiceAddRequest {
    /// Unique identifier (stable). Must not collide with existing services.
    pub id: String,
    /// Base URL of the filter, e.g. "http://127.0.0.1:7001" (no trailing slash; we’ll normalize).
    pub base_url: String,
    /// Bearer token to talk to the filter.
    pub token: String,
    /// Optional human-friendly name.
    pub name: Option<String>,
    /// Whether to validate `/auth/ping` before accepting (defaults to true).
    #[serde(default = "default_validate")]
    pub check_health_before_adding: bool,
}

fn default_validate() -> bool {
    true
}

/// Successful "add service" reply
#[derive(Debug, Serialize, ToSchema)]
pub struct CtrlServiceAddOkData {
    pub service_id: String,
}
impl shared::api::ok::sealed::Sealed for CtrlServiceAddOkData {}
impl shared::api::ok::OkData for CtrlServiceAddOkData {}

/// Successful "delete service" reply
#[derive(Debug, Serialize, ToSchema)]
pub struct CtrlServiceDeleteOkData {
    pub service_id: String,
}
impl shared::api::ok::sealed::Sealed for CtrlServiceDeleteOkData {}
impl shared::api::ok::OkData for CtrlServiceDeleteOkData {}

fn normalize_base_url(u: &str) -> String {
    let mut s = u.trim().to_string();
    while s.ends_with('/') {
        s.pop();
    }
    s
}

async fn quick_ping(http: &Client, base: &str, token: &str) -> Result<(), String> {
    use tokio::time::{Duration, timeout};
    let fut = http
        .get(format!("{base}/auth/ping"))
        .bearer_auth(token)
        .send();
    let resp = timeout(Duration::from_secs(3), fut)
        .await
        .map_err(|_| "timeout".to_string())
        .and_then(|r| r.map_err(|e| e.to_string()))?;

    match resp.status().as_u16() {
        200 => Ok(()),
        401 => Err("unauthorized (bad token)".into()),
        code => Err(format!("unexpected status {}", code)),
    }
}

/// /services/new
///
/// Add a service at runtime and start its background tasks
#[utoipa::path(
    put,
    path = "/services/new",
    request_body = CtrlServiceAddRequest,
    responses(
        (status = 200, description = "OK",                                      body = OkEnvelope<CtrlServiceAddOkData>),
        (status = 400, description = "Invalid service config",                  body = ErrorEnvelope),
        (status = 401, description = "Unauthorized",                            body = ErrorEnvelope),
        (status = 409, description = "Service already exists",                  body = ErrorEnvelope),
        (status = 502, description = "Filter unreachable",                      body = ErrorEnvelope),
    ),
    tags = ["controller","services"]
)]
pub async fn put_service_supervisor_new(
    OriginalUri(uri): OriginalUri,
    State(state): State<ControllerState>,
    State(supervisor): State<Arc<Supervisor>>,
    Json(req): Json<CtrlServiceAddRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    // 1) Basic validation
    let id = req.id.trim();
    if id.is_empty() {
        return ErrorEnvelope::bad_request("invalid_service_id", uri.path())
            .with_title("Invalid service id")
            .with_detail("id must be a non-empty string")
            .into_result();
    }
    let base = normalize_base_url(&req.base_url);
    if !(base.starts_with("http://") || base.starts_with("https://")) {
        return ErrorEnvelope::bad_request("invalid_base_url", uri.path())
            .with_title("Invalid base_url")
            .with_detail("base_url must start with http:// or https://")
            .into_result();
    }
    if req.token.trim().is_empty() {
        return ErrorEnvelope::bad_request("invalid_token", uri.path())
            .with_title("Invalid token")
            .with_detail("token must be non-empty")
            .into_result();
    }

    // 2) Optional preflight check (no state changes)
    if req.check_health_before_adding
        && let Err(msg) = quick_ping(&state.http, &base, &req.token).await
    {
        let title = if msg.contains("unauthorized") {
            "Bad filter token"
        } else if msg.contains("timeout") {
            "Filter timed out"
        } else {
            "Filter unreachable"
        };
        let mut detail = format!("{title} ({msg})");
        // Very common footgun: a controller running in a container can't reach
        // the host's loopback. If the address is loopback and we couldn't
        // authenticate-or-connect, nudge toward the right address.
        let loopback = ["localhost", "127.0.0.1", "[::1]"]
            .iter()
            .any(|h| base.contains(h));
        if loopback && !msg.contains("unauthorized") {
            detail.push_str(
                ". If the controller runs in a container, 'localhost' is the container \
                     itself — not the host. Use the filter's docker network name (e.g. \
                     http://filter-<name>:<control-port>) on a shared network, or \
                     host.docker.internal / the host's LAN IP.",
            );
        }
        return ErrorEnvelope::bad_request("filter_validation_failed", uri.path())
            .with_title(title)
            .with_detail(detail)
            .into_result();
    }

    // 3) Build cfg and hand off to supervisor
    let cfg = ServiceCfg {
        id: id.to_string(),
        base_url: base.clone(),
        token: req.token.clone(),
    };

    if let Err(e) = supervisor.add_service(cfg.clone()).await {
        // Map “already exists” to 409; everything else → 500
        let msg = e.to_string();
        if msg.contains("already exists") {
            return ErrorEnvelope::conflict("service_exists", uri.path())
                .with_title("Service already exists")
                .with_detail(format!("service_id '{id}' already present"))
                .into_result();
        }
        return ErrorEnvelope::internal("supervisor_spawn_failed", uri.path())
            .with_title("Unable to start background tasks")
            .with_detail(msg)
            .into_result();
    }

    // 4) Apply the optional display name in memory (metadata is the single home
    //    for names), then read back the resolved metadata.
    let meta = {
        let mut w = state.write();
        w.services.get_mut(&cfg.id).map(|svc| {
            if let Some(name) = req.name.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                svc.metadata.name = Some(name.to_string());
            }
            svc.metadata.clone()
        })
    };

    // 5) Persist the service (connection config + metadata) so the fleet is
    //    durable across restarts — only when hydration is enabled.
    if state.cfg().db_hydrate
        && let Some(meta) = meta
    {
        crate::db::insertion::persist_service(&state.db(), &cfg, &meta).await;
    }

    ok(CtrlServiceAddOkData { service_id: cfg.id })
}

/// /services/{service_id}/delete
///
/// Removes a service at runtime and stops its background tasks
#[utoipa::path(
    delete,
    path = "/services/{service_id}/delete",
    params(
        ("service_id" = String, Path,                               description = "Service id"),
    ),
    responses(
        (status = 200, description = "OK",                          body = OkEnvelope<CtrlServiceDeleteOkData>),
        (status = 401, description = "Unauthorized",                body = ErrorEnvelope),
        (status = 404, description = "Unknown service",             body = ErrorEnvelope),
        (status = 500, description = "Server error",                body = ErrorEnvelope),
    ),
    tags = ["controller", "services"]
)]
pub async fn delete_service_supervisor(
    OriginalUri(uri): OriginalUri,
    Path(service_id): Path<String>,
    State(state): State<ControllerState>,
    State(supervisor): State<Arc<Supervisor>>,
) -> (StatusCode, Json<serde_json::Value>) {
    if !state.read().services.contains_key(&service_id) {
        return ErrorEnvelope::not_found("service_not_found", uri.path())
            .with_title("Unknown service")
            .with_detail(format!("service_id '{service_id}' is not known"))
            .into_result();
    }

    // 1) stop background tasks first (so they don’t race with removal)
    if let Err(e) = supervisor.remove_service(&service_id).await {
        return ErrorEnvelope::internal("supervisor_remove_failed", uri.path())
            .with_title("Failed stopping tasks")
            .with_detail(e.to_string())
            .into_result();
    }

    // 2) drop from state
    {
        let mut w = state.write();
        w.services.remove(&service_id);
    }

    // 3) drop the persisted row so the removal is durable (when hydration is on).
    //    Records/ecam/cursors stay, so re-adding the same id later still resumes.
    if state.cfg().db_hydrate {
        crate::db::insertion::delete_service_row(&state.db(), &service_id).await;
    }

    ok(CtrlServiceDeleteOkData { service_id })
}
