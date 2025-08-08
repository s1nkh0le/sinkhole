use crate::state::ControllerState;
use serde::Serialize;
use utoipa::ToSchema;
use shared::types::ruleset_types::{PortInfo, RuleSetRaw};
use shared::types::snapshot_types::PortSnapshot;
use crate::state::Health::Healthy;

#[derive(Debug, Serialize)]
pub struct ControllerSnapshot {
    pub services: Vec<ServiceSnapshot>,
}

/// Snapshot of a service fetch
#[derive(Debug, Serialize, ToSchema)]
pub struct ServiceSnapshot {
    /// Unique identifier for the service.
    pub id: String,
    /// Base URL where the service is reachable (IP address of the hosting machine)
    pub base_url: String,
    /// Health status of the service
    pub healthy: bool,
    /// Timestamp (in nanoseconds since UNIX epoch) when the service last synchronized
    pub last_sync_ns: u64,
    pub next_ecam_since: u64,
    pub ports: Vec<Snapshot>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Snapshot {
    pub port: u16,
    pub next_records_since: u64,
    pub snap: Option<PortSnapshot>
}

/// Build an immutable snapshot of the current in-memory fetch.
pub fn build_snapshot(state: &ControllerState) -> ControllerSnapshot {
    let g = state.read();

    let mut services = Vec::with_capacity(g.services.len());
    for (svc_id, svc) in g.services.iter() {
        let mut ports = Vec::with_capacity(svc.ports.len());
        for (pno, p) in svc.ports.iter() {
            ports.push(Snapshot {
                port: *pno,
                next_records_since: p.cursors.next_records_since,
                snap: p.snap.clone(),
            });
        }

        services.push(ServiceSnapshot {
            id: svc_id.clone(),
            base_url: svc.cfg.base_url.clone(),
            healthy: svc.metadata.health == Healthy,
            last_sync_ns: svc.metadata.last_state_sync_ns,
            next_ecam_since: svc.service_cursors.next_ecam_since,
            ports,
        });
    }

    ControllerSnapshot { services }
}

/// Pretty-print the current fetch snapshot to stdout.
pub fn dump_controller_state(state: &ControllerState) {
    let snap = build_snapshot(state);
    println!(
        "{}",
        serde_json::to_string_pretty(&snap).unwrap_or_else(|_| String::from("{}"))
    );
}
