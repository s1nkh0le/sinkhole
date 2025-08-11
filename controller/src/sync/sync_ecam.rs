// controller/src/sync/data_pull.rs

use crate::db::insertion::insert_ecam;
use crate::db::DbHandle;
use crate::state::ctrl::{ControllerState, Service};
use crate::state::health::HealthLayer;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio::time::{sleep, Instant};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use crate::fetch::fetch_ecam::fetch_ecam;

const TICK: Duration = Duration::from_millis(200);

pub fn spawn_for_service(
    service_id: String,
    state: ControllerState,
    http: reqwest::Client,
    db: DbHandle,
    duration: Duration,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(ecam_loop(
        service_id,
        state,
        http,
        db,
        duration,
        cancel,
    ))
}

async fn ecam_loop(
    svc_id: String,
    state: ControllerState,
    http: reqwest::Client,
    db: DbHandle,
    cadence: Duration,
    cancel: CancellationToken,
) {
    let layer = HealthLayer::EcamPull;
    let mut next_at = Instant::now();

    loop {
        if cancel.is_cancelled() {
            break;
        }

        // cadence throttle (sleep only as much as needed, in small slices)
        let now = Instant::now();
        if now < next_at {
            let wait = (next_at - now).min(TICK);
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = sleep(wait) => {}
            }
            continue;
        }

        // snapshot immutable bits + health gate
        let (base_url, token, can_poll, next_allowed_ns, since) = {
            let g = state.read();
            let svc: &Service = match g.services.get(&svc_id) {
                Some(s) => s,
                None => return, // service removed → stop task
            };
            (
                svc.cfg.base_url.clone(),
                svc.cfg.token.clone(),
                svc.health.can_poll(layer),
                svc.health.next_allowed_ns(layer),
                svc.service_cursors.next_ecam_since,
            )
        };

        // respect backoff; do not move next_at here (we want to run as soon as allowed)
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

        // pull from filter
        match fetch_ecam(&http, &base_url, &token, since).await {
            Ok((events, next_since)) => {
                if !events.is_empty() {
                    insert_ecam(&db, &svc_id, &events).await;
                    debug!(%svc_id, pulled = events.len(), "ECAM batch written");
                }

                let mut w = state.write();
                if let Some(svc) = w.services.get_mut(&svc_id) {
                    svc.service_cursors.next_ecam_since = next_since;
                    svc.health.mark_success(layer);
                } else {
                    return; // service removed during write
                }
            }
            Err(err) => {
                let detail = format!("ECAM pull failed: {err}");
                warn!(%svc_id, %detail);
                let mut w = state.write();
                if let Some(svc) = w.services.get_mut(&svc_id) {
                    svc.health.mark_failure(layer, Some(detail));
                } else {
                    return;
                }
            }
        }

        // schedule next run by cadence
        next_at = Instant::now() + cadence;
    }
}