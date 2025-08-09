// controller/src/sync/data_pull.rs

use std::time::Duration;
use tokio::time::{sleep, Instant};
use tracing::{debug, warn};

use crate::db::insertion::{insert_ecam, insert_records};
use crate::fetch::fetch_records_ecam::{fetch_ecam, fetch_records};
use crate::state::ctrl::{ControllerState, Service};
use crate::state::health::HealthLayer;

const WATCHDOG:      Duration = Duration::from_millis(2500);

// tune independently
pub const ECAM_INTERVAL: Duration = Duration::from_secs(20);
pub const REC_INTERVAL:  Duration = Duration::from_secs(5);

// keep in sync with fetcher, or re-export it from there
const REC_BATCH_LIMIT: usize = 5000;

pub fn spawn(state: ControllerState, http: reqwest::Client, db: crate::db::DbHandle) {
    for svc_id in state.service_ids() {
        tokio::spawn(ecam_loop(svc_id.clone(), state.clone(), http.clone(), db.clone()));
        tokio::spawn(records_loop(svc_id, state.clone(), http.clone(), db.clone()));
    }
}

async fn ecam_loop(
    svc_id: String,
    state:  ControllerState,
    http:   reqwest::Client,
    db:     crate::db::DbHandle,
) {
    let mut next_at = Instant::now();

    loop {
        if Instant::now() < next_at {
            sleep(WATCHDOG).await;
            continue;
        }

        // snapshot
        let (base_url, token, can_poll, next_allowed_ns, since) = {
            let g = state.read();
            let svc: &Service = match g.services.get(&svc_id) { Some(s) => s, None => return };
            (
                svc.cfg.base_url.clone(),
                svc.cfg.token.clone(),
                svc.health.can_poll(HealthLayer::EcamPull),
                svc.health.next_allowed_ns(HealthLayer::EcamPull),
                svc.service_cursors.next_ecam_since,
            )
        };

        if !can_poll {
            let now_ns = shared::utils::time::now_ns();
            let wait = Duration::from_nanos(next_allowed_ns.saturating_sub(now_ns)).min(WATCHDOG);
            sleep(wait).await;
            next_at = Instant::now() + ECAM_INTERVAL;
            continue;
        }

        match fetch_ecam(&http, &base_url, &token, since).await {
            Ok((events, next_since)) => {
                if !events.is_empty() {
                    insert_ecam(&db, &svc_id, &events).await;
                    debug!(%svc_id, pulled = events.len(), "ECAM batch written");
                }
                let mut w = state.write();
                if let Some(svc) = w.services.get_mut(&svc_id) {
                    svc.service_cursors.next_ecam_since = next_since;
                    svc.health.mark_success(HealthLayer::EcamPull);
                }
            }
            Err(err) => {
                let detail = format!("ECAM pull failed: {err}");
                warn!(%svc_id, %detail);
                let mut w = state.write();
                if let Some(svc) = w.services.get_mut(&svc_id) {
                    svc.health.mark_failure(HealthLayer::EcamPull, Some(detail));
                }
            }
        }

        next_at = Instant::now() + ECAM_INTERVAL;
    }
}

async fn records_loop(
    svc_id: String,
    state:  ControllerState,
    http:   reqwest::Client,
    db:     crate::db::DbHandle,
) {
    let mut next_at = Instant::now();

    loop {
        if Instant::now() < next_at {
            sleep(WATCHDOG).await;
            continue;
        }

        // snapshot
        let (base_url, token, can_poll, next_allowed_ns, ports) = {
            let g = state.read();
            let svc: &Service = match g.services.get(&svc_id) { Some(s) => s, None => return };
            let ports: Vec<(u16, u64)> = svc
                .ports
                .iter()
                .map(|(p, port)| (*p, port.cursors.next_records_since))
                .collect();
            (
                svc.cfg.base_url.clone(),
                svc.cfg.token.clone(),
                svc.health.can_poll(HealthLayer::RecordsPull),
                svc.health.next_allowed_ns(HealthLayer::RecordsPull),
                ports,
            )
        };

        if !can_poll {
            let now_ns = shared::utils::time::now_ns();
            let wait = Duration::from_nanos(next_allowed_ns.saturating_sub(now_ns)).min(WATCHDOG);
            sleep(wait).await;
            next_at = Instant::now() + REC_INTERVAL;
            continue;
        }

        for (port, mut since) in ports {
            loop {
                match fetch_records(&http, &base_url, port, &token, since).await {
                    Ok((records, next_since)) => {
                        if !records.is_empty() {
                            insert_records(&db, &svc_id, port, &records).await;
                        }
                        // advance cursor + success
                        {
                            let mut w = state.write();
                            if let Some(svc) = w.services.get_mut(&svc_id) {
                                if let Some(p) = svc.ports.get_mut(&port) {
                                    p.cursors.next_records_since = next_since;
                                }
                                svc.health.mark_success(HealthLayer::RecordsPull);
                            }
                        }

                        let done = records.len() < REC_BATCH_LIMIT;
                        since = next_since;
                        if done { break; }
                    }
                    Err(err) => {
                        let detail = format!("record pull failed for port {port}: {err}");
                        warn!(%svc_id, %port, %detail);
                        let mut w = state.write();
                        if let Some(svc) = w.services.get_mut(&svc_id) {
                            svc.health.mark_failure(HealthLayer::RecordsPull, Some(detail));
                        }
                        break;
                    }
                }
            }
        }

        next_at = Instant::now() + REC_INTERVAL;
    }
}
