use crate::state::{ControllerState, ServiceCursors, ServiceMetadata};
use axum::{extract::State, http::StatusCode, Json};
use axum::extract::OriginalUri;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use shared::api::ok::{ok, OkEnvelope};
use utoipa::ToSchema;
use shared::types::snapshot_types::PortSnapshot;

#[derive(Serialize, ToSchema)]
pub struct CtrlReplyState {
    pub controller: CtrlReplyController,
    pub services:   Vec<CtrlReplyService>,
}

impl shared::api::ok::sealed::Sealed for CtrlReplyState {}
impl shared::api::ok::OkData for CtrlReplyState {}

#[derive(Serialize, ToSchema)]
pub struct CtrlReplyController {
    /// Bind address the controller listens on
    pub bind: String,
    /// Path to the controller cache DB
    pub db_path: String,
    /// Pull interval (seconds) for records/ecam
    pub pull_interval_secs: u64,
    /// Port/ruleset sync interval (seconds)
    pub sync_interval_secs: u64,
    /// Max concurrent per-service ruleset fetches
    pub sync_concurrency: usize,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CtrlReplyService {
    pub id:        String,
    pub name:      Option<String>,
    pub base_url:  String,
    pub metadata:  ServiceMetadata,
    pub cursors:   ServiceCursors,
    pub ports:     Vec<CtrlReplyPort>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CtrlReplyPort {
    pub port:                 u16,
    pub next_records_since:   u64,
    pub snapshot:             Option<PortSnapshot>,
}

/// /state
///
/// Returns the controller's full internal state:
/// controller config summary, services with metadata/cursors,
/// and per-port cursors + (optional) snapshots.
#[utoipa::path(
    get,
    path = "/state",
    params(
        ("ctrl_token" = String, Query,                          description = "Authentication token"),
    ),
    responses(
        (status = 200, description = "OK",                      body = OkEnvelope<CtrlReplyState>),
        (status = 401, description = "Unauthorized")
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
        CtrlReplyController {
            bind:               cfg.bind.to_string(),
            db_path:            cfg.db_path.clone(),
            pull_interval_secs: cfg.pull_interval.as_secs(),
            sync_interval_secs: cfg.sync_interval_secs,
            sync_concurrency:   cfg.sync_concurrency,
        }
    };

    // Snapshot everything under a single read-lock and clone out
    let services = {
        let g = state.read();

        g.services.iter().map(|(id, svc)| {
            // ports
            let ports: Vec<CtrlReplyPort> = svc.ports.iter()
                .map(|(pnum, p)| CtrlReplyPort {
                    port: *pnum,
                    next_records_since: p.cursors.next_records_since,
                    snapshot: p.snap.clone(), // includes info, ruleset, versions
                })
                .collect();

            CtrlReplyService {
                id:       id.clone(),
                name:     svc.cfg.name.clone(),
                base_url: svc.cfg.base_url.clone(),
                metadata: svc.clone().metadata,           // ServiceMetadata is Serialize + Copy
                cursors:  svc.service_cursors.clone(),
                ports,
            }
        }).collect::<Vec<_>>()
    };

    let payload = CtrlReplyState {
        controller: ctrl_meta,
        services,
    };

    ok(payload)
}