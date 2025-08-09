use crate::state::ctrl::{ControllerState, ServiceCursors, ServiceMetadata};
use crate::state::health::Health;
use axum::extract::OriginalUri;
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use shared::api::ok::{ok, OkEnvelope};
use utoipa::ToSchema;
use shared::api::err::ErrorEnvelope;
use shared::types::snapshot_types::PortSnapshot;

#[derive(Serialize, ToSchema)]
pub struct CtrlReplyBodyState {
    pub controller: CtrlReplyBodyController,
    pub services:   Vec<CtrlReplyBodyServiceState>,
}
impl shared::api::ok::sealed::Sealed for CtrlReplyBodyState {}
impl shared::api::ok::OkData for CtrlReplyBodyState {}

#[derive(Serialize, ToSchema)]
pub struct CtrlReplyBodyController {
    pub bind: String,
    pub db_path: String,
    pub pull_interval_secs: u64,
    pub sync_interval_secs: u64,
    pub sync_concurrency: usize,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CtrlReplyBodyServiceState {
    /// Service identifier
    pub id:        String,
    pub name:      Option<String>,
    pub base_url:  String,

    /// User-editable metadata (name/description/color/icon, etc.)
    pub metadata:  ServiceMetadata,

    /// Live service health
    pub health:    Health,

    pub cursors:   ServiceCursors,
    pub ports:     Vec<CtrlReplyPort>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CtrlReplyPort {
    pub port:               u16,
    pub next_records_since: u64,
    pub snapshot:           Option<PortSnapshot>,
}

// ── Handler ───────────────────────────────────────────────────────────────

/// /state
///
/// Returns the controller's full internal state:
/// controller config summary, services with metadata/health/cursors,
/// and per-port cursors + (optional) snapshots.
#[utoipa::path(
    get,
    path = "/state",
    params(
        ("ctrl_token" = String, Query,                              description = "Authentication token"),
    ),
    responses(
        (status = 200, description = "OK",                          body = OkEnvelope<CtrlReplyBodyState>),
        (status = 401, description = "Unauthorized",                body = ErrorEnvelope)
    ),
    tag = "controller"
)]
pub async fn get_state(
    OriginalUri(_uri): OriginalUri,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<Value>) {
    // Controller meta (friendly, OpenAPI-safe types)
    let ctrl_meta = {
        let cfg = state.cfg();
        CtrlReplyBodyController {
            bind:               cfg.bind.to_string(),
            db_path:            cfg.db_path.clone(),
            pull_interval_secs: cfg.pull_interval.as_secs(),
            sync_interval_secs: cfg.sync_interval_secs,
            sync_concurrency:   cfg.sync_concurrency,
        }
    };

    // Snapshot everything under a single read-lock and clone out
    let mut services: Vec<CtrlReplyBodyServiceState> = {
        let g = state.read();

        g.services
            .iter()
            .map(|(id, svc)| {
                // ports (sorted)
                let mut ports: Vec<CtrlReplyPort> = svc
                    .ports
                    .iter()
                    .map(|(pnum, p)| CtrlReplyPort {
                        port: *pnum,
                        next_records_since: p.cursors.next_records_since,
                        snapshot: p.snap.clone(),
                    })
                    .collect();
                ports.sort_by_key(|p| p.port);

                CtrlReplyBodyServiceState {
                    id:       id.clone(),
                    name:     svc.cfg.name.clone(),
                    base_url: svc.cfg.base_url.clone(),
                    metadata: svc.metadata.clone(),
                    health:   svc.health.clone(),
                    cursors:  svc.service_cursors.clone(),
                    ports,
                }
            })
            .collect()
    };

    // Sort services by id for stable output
    services.sort_by(|a, b| a.id.cmp(&b.id));

    ok(CtrlReplyBodyState {
        controller: ctrl_meta,
        services,
    })
}