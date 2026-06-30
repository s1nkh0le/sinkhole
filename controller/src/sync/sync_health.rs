use crate::fetch::error::CtrlErr;
use crate::state::ctrl::ControllerState;
use crate::state::health::HealthLayer;
use reqwest::Client;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};
use tokio_util::sync::CancellationToken;
use tracing::log::warn;
use tracing::{debug, trace};

const TICK: Duration = Duration::from_millis(250); // small granularity for backoff waits
const TIMEOUT: Duration = Duration::from_secs(2); // per-request timeout

pub fn spawn_for_service(
    service_id: String,
    state: ControllerState,
    http: Client,
    cadence: Duration, // probe cadence between successful attempts
    cancel: CancellationToken,
) -> JoinHandle<()> {
    let st = state.clone();
    let cli = http.clone();
    tokio::spawn(async move {
        probe_loop(service_id, st, cli, cadence, cancel).await;
    })
}

async fn probe_loop(
    svc_id: String,
    state: ControllerState,
    http: Client,
    cadence: Duration,
    cancel: CancellationToken,
) {
    let layer = HealthLayer::Probe;

    loop {
        if cancel.is_cancelled() {
            break;
        }

        // Snapshot creds and gating
        let (base_url, token, can_poll, next_allowed_ns) = {
            let g = state.read();
            let Some(svc) = g.services.get(&svc_id) else {
                // Service removed -> stop task
                return;
            };
            (
                svc.cfg.base_url.clone(),
                svc.cfg.token.clone(),
                svc.health.can_poll(layer),
                svc.health.next_allowed_ns(layer),
            )
        };

        if !can_poll {
            // sleep until allowed (bounded so we can react to cancellation promptly)
            let now_ns = shared::utils::time::now_ns();
            let wait_ns = next_allowed_ns.saturating_sub(now_ns);
            let wait = Duration::from_nanos(wait_ns).min(TICK);
            sleep(wait).await;
            continue;
        }

        // Perform probe
        let result = ping_check(&http, &base_url, &token).await;

        // Update health in place (only if service still exists)
        {
            let mut w = state.write();
            if let Some(svc) = w.services.get_mut(&svc_id) {
                match result {
                    Ok(()) => {
                        debug!("[probe_loop({})] health probe ok", svc_id);
                        svc.health.mark_success(layer);
                    }
                    Err(CtrlErr::Auth) => {
                        warn!("[probe_loop({})] probe 401 – token wrong?", svc_id);
                        svc.health
                            .mark_failure(layer, Some("401 unauthorized".into()));
                    }
                    Err(e) => {
                        warn!("[probe_loop({})] probe failed: {}", svc_id, e);
                        svc.health.mark_failure(layer, Some(e.to_string()));
                    }
                }
            } else {
                trace!("[probe_loop({})] service removed, state vanished", svc_id);
                return;
            }
        }

        // Cadence between attempts (don’t busy-loop)
        let step = cadence.min(Duration::from_secs(5));
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = sleep(step) => {}
        }
    }
}

pub async fn ping_check(http: &Client, base: &str, token: &str) -> Result<(), CtrlErr> {
    let fut = http
        .get(format!("{base}/auth/ping"))
        .bearer_auth(token)
        .send();

    let resp = timeout(TIMEOUT, fut)
        .await
        .map_err(|_| CtrlErr::Timeout)??;

    match resp.status().as_u16() {
        200 => Ok(()),
        401 => Err(CtrlErr::Auth),
        c => Err(CtrlErr::Http(c)),
    }
}
