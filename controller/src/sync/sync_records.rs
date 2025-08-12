// controller/src/sync/data_pull.rs

use crate::db::insertion::{insert_ecam, insert_records};
use crate::fetch::fetch_records::{fetch_records};
use crate::state::ctrl::{ControllerState, Service};
use crate::state::health::HealthLayer;
use reqwest::Client;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio::time::{sleep, Instant};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use crate::db::commands::DbHandle;

// Small tick so we can react quickly to cancel/backoff/cadence gates
const TICK: Duration = Duration::from_millis(200);

// Keep the batch limit here (only records use it)
const REC_BATCH_LIMIT: usize = 5000;

pub fn spawn_for_service(
    service_id: String,
    state: ControllerState,
    http: Client,
    db: DbHandle,
    cadence: Duration,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(records_loop(
        service_id, state, http, db, cadence, cancel,
    ))
}

async fn records_loop(
    svc_id: String,
    state: ControllerState,
    http: Client,
    db: DbHandle,
    cadence: Duration,
    cancel: CancellationToken,
) {
    let layer = HealthLayer::RecordsPull;
    let mut next_at = Instant::now();

    loop {
        if cancel.is_cancelled() {
            break;
        }

        // cadence throttle: wait just enough (in small slices)
        let now = Instant::now();
        if now < next_at {
            let wait = (next_at - now).min(TICK);
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = sleep(wait) => {}
            }
            continue;
        }

        // snapshot immutable bits + gate on health backoff
        let (base_url, token, can_poll, next_allowed_ns, port_cursors) = {
            let g = state.read();
            let svc: &Service = match g.services.get(&svc_id) {
                Some(s) => s,
                None => return, // service removed → stop task
            };

            let ports: Vec<(u16, u64)> = svc
                .ports
                .iter()
                .map(|(p, port)| (*p, port.cursors.next_records_since))
                .collect();

            (
                svc.cfg.base_url.clone(),
                svc.cfg.token.clone(),
                svc.health.can_poll(layer),
                svc.health.next_allowed_ns(layer),
                ports,
            )
        };

        // respect health backoff. Do NOT bump next_at here; we want to run as soon as allowed.
        if !can_poll {
            let now_ns = shared::utils::time::now_ns();
            let wait_ns = next_allowed_ns.saturating_sub(now_ns);
            let wait = Duration::from_nanos(wait_ns).min(TICK);
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = sleep(wait) => {}
            }
            continue;
        }

        // iterate ports; for each port, page until batch < limit
        for (port, mut since) in port_cursors {
            loop {
                if cancel.is_cancelled() {
                    break;
                }

                match fetch_records(&http, &base_url, port, &token, since).await {
                    Ok((records, next_since)) => {
                        if !records.is_empty() {
                            insert_records(&db, &svc_id, port, &records).await;
                            debug!(%svc_id, %port, pulled = records.len(), "records batch written");
                        }

                        // advance cursor and mark success for this layer
                        let mut w = state.write();
                        if let Some(svc) = w.services.get_mut(&svc_id) {
                            if let Some(p) = svc.ports.get_mut(&port) {
                                p.cursors.next_records_since = next_since;
                            }
                            svc.health.mark_success(layer);
                        } else {
                            return; // service removed during write
                        }

                        let done = records.len() < REC_BATCH_LIMIT;
                        since = next_since;
                        if done {
                            break;
                        }
                    }
                    Err(err) => {
                        let detail = format!("record pull failed for port {port}: {err}");
                        warn!(%svc_id, %port, %detail);
                        let mut w = state.write();
                        if let Some(svc) = w.services.get_mut(&svc_id) {
                            svc.health.mark_failure(layer, Some(detail));
                        } else {
                            return;
                        }
                        break; // stop paging this port; move to next
                    }
                }
            }
        }

        // schedule next cadence tick
        next_at = Instant::now() + cadence;
    }
}