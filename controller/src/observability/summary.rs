use crate::state::ControllerState;
use serde::Serialize;

use shared::rules_model::{PortInfo, RuleSetRaw};

#[derive(Serialize)]
pub struct ControllerSnapshot {
    pub services: Vec<ServiceSnapshot>,
}

#[derive(Serialize)]
pub struct ServiceSnapshot {
    pub id: String,
    pub base_url: String,
    pub healthy: bool,
    pub last_sync_ns: i64,
    pub next_ecam_since: i64,
    pub ports: Vec<PortSnapshot>,
}

#[derive(Serialize)]
pub struct PortSnapshot {
    pub port: u16,
    pub next_logs_since: i64,
    pub info: Option<PortInfo>,
    pub ruleset: Option<RuleSetRaw>,
}

/// Build an immutable snapshot of the current in-memory state.
pub fn build_snapshot(state: &ControllerState) -> ControllerSnapshot {
    let g = state.read();

    let mut services = Vec::with_capacity(g.services.len());
    for (svc_id, svc) in g.services.iter() {
        let mut ports = Vec::with_capacity(svc.ports.len());
        for (pno, p) in svc.ports.iter() {
            ports.push(PortSnapshot {
                port: *pno,
                next_logs_since: p.next_logs_since,
                info: p.info.clone(),
                ruleset: p.ruleset.clone(),
            });
        }

        services.push(ServiceSnapshot {
            id: svc_id.clone(),
            base_url: svc.cfg.base_url.clone(),
            healthy: svc.healthy,
            last_sync_ns: svc.last_sync_ns,
            next_ecam_since: svc.next_ecam_since,
            ports,
        });
    }

    ControllerSnapshot { services }
}

/// Pretty-print the current state snapshot to stdout.
pub fn dump_controller_state(state: &ControllerState) {
    let snap = build_snapshot(state);
    println!(
        "{}",
        serde_json::to_string_pretty(&snap).unwrap_or_else(|_| String::from("{}"))
    );
}
