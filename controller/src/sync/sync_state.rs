use reqwest::Client;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant, sleep};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::fetch::fetch_state::fetch_state;
use crate::state::ctrl::ControllerState;
use crate::state::health::HealthLayer;

const REQ_TIMEOUT: Duration = Duration::from_secs(5);
const TICK: Duration = Duration::from_millis(200); // fine-grained wait slice

pub fn spawn_for_service(
    service_id: String,
    state: ControllerState,
    http: Client,
    cadence: Duration,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    let st = state.clone();
    let cli = http.clone();
    tokio::spawn(async move {
        run_loop(service_id, st, cli, cadence, cancel).await;
    })
}

async fn run_loop(
    svc_id: String,
    state: ControllerState,
    http: Client,
    cadence: Duration,
    cancel: CancellationToken,
) {
    let layer = HealthLayer::StateSync;
    let mut next_at = Instant::now();

    loop {
        if cancel.is_cancelled() {
            break;
        }

        // throttle by cadence; sleep only as much as needed, but in small slices so we can cancel
        let now = Instant::now();
        if now < next_at {
            let wait = (next_at - now).min(TICK);
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = sleep(wait) => {}
            }
            continue;
        }

        // snapshot creds & health-gate without holding the lock across awaits
        let (base_url, token, can_poll, next_allowed_ns) = {
            let g = state.read();
            match g.services.get(&svc_id) {
                Some(svc) => (
                    svc.cfg.base_url.clone(),
                    svc.cfg.token.clone(),
                    svc.health.can_poll(layer),
                    svc.health.next_allowed_ns(layer),
                ),
                None => return, // service removed → stop task
            }
        };

        if !can_poll {
            // backoff still active: wait a bit, but don’t busy loop
            let now_ns = shared::utils::time::now_ns();
            let wait_ns = next_allowed_ns.saturating_sub(now_ns);
            let wait = Duration::from_nanos(wait_ns).min(TICK);
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = sleep(wait) => {}
            }
            // do not advance next_at here — we want to retry as soon as allowed
            continue;
        }

        // pull /state from filter
        let res = fetch_state(&http, &base_url, &token, REQ_TIMEOUT).await;

        // apply results under a write lock; never hold it across .await
        {
            let mut w = state.write();

            // move out boot map temporarily to avoid double-borrows
            let mut boot_map = std::mem::take(&mut w.boot_cursors);

            if let Some(svc) = w.services.get_mut(&svc_id) {
                match res {
                    Ok(snaps) => {
                        debug!(%svc_id, ports = snaps.len(), "state sync ok");
                        // health: success for this layer
                        svc.health.mark_success(layer);

                        // merge snapshots into controller state (updates cursors etc.)
                        svc.merge_snapshots(snaps, &mut boot_map, &svc_id);
                    }
                    Err(err) => {
                        let detail = format!("state sync failed: {err}");
                        warn!(%svc_id, %detail);
                        svc.health.mark_failure(layer, Some(detail));
                    }
                }
            } else {
                // service vanished during fetch → stop task
                return;
            }

            // put boot_map back
            w.boot_cursors = boot_map;
        }

        // schedule next tick based on requested cadence
        next_at = Instant::now() + cadence;
    }
}
