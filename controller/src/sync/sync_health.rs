use crate::fetch::error::CtrlErr;
use crate::state::ctrl::{ControllerState, Service, ServiceMetadata};
use reqwest::Client;
use std::time::Duration;
use tokio::time::{sleep, timeout};
use tracing::debug;
use tracing::log::warn;
use crate::state::health::{Health, HealthLayer};

const TICK:    Duration = Duration::from_millis(250); // scheduler cadence
const TIMEOUT: Duration = Duration::from_secs(2);

pub fn spawn(state: ControllerState, http: Client) {
    for svc_id in state.service_ids() {
        let st  = state.clone();
        let cli = http.clone();
        tokio::spawn(async move {
            probe_loop(svc_id, st, cli).await;
        });
    }
}

async fn probe_loop(svc_id: String, state: ControllerState, http: Client) {
    let layer = HealthLayer::Probe;

    loop {
        // snapshot creds & health gate without holding the lock across await
        let (base_url, token, can_poll, next_allowed) = {
            let g = state.read();
            let Some(svc) = g.services.get(&svc_id) else {
                return; // service removed → stop task
            };
            let can   = svc.health.can_poll(layer);
            let next  = svc.health.next_allowed_ns(layer);

            (svc.cfg.base_url.clone(), svc.cfg.token.clone(), can, next)
        };

        if !can_poll {
            // sleep until allowed (bounded by a small tick so we can respond to shutdowns etc.)
            let now_ns = shared::utils::time::now_ns();
            let wait_ns = next_allowed.saturating_sub(now_ns);
            let wait = Duration::from_nanos(wait_ns).min(TICK);

            sleep(wait).await;
            continue;
        }

        // do the probe
        let result = ping(&http, &base_url, &token).await;

        // update health in place
        {
            let mut w = state.write();
            if let Some(svc) = w.services.get_mut(&svc_id) {
                match result {
                    Ok(()) => {
                        debug!("[probe_loop({})] health probe ok", svc_id);
                        svc.health.mark_success(layer);
                    }
                    Err(CtrlErr::Auth) => {
                        // auth errors are “reachable but misconfigured” — still mark failure so UIs can show it
                        warn!("[probe_loop({})] health probe 401 – token wrong", svc_id);
                        svc.health.mark_failure(layer, Some("401 unauthorized".into()));
                    }
                    Err(e) => {
                        warn!("[probe_loop({})] health probe failed, error={}", svc_id, e);
                        svc.health.mark_failure(layer, Some(e.to_string()));
                    }
                }
            } else {
                // service disappeared while we were probing; just end the task
                return;
            }
        }

        // small cadence to avoid busy looping
        sleep(TICK).await;
    }
}

async fn ping(http: &Client, base: &str, token: &str) -> Result<(), CtrlErr> {
    let fut = http
        .get(&format!("{base}/auth/ping"))
        .query(&[("token", token)])
        .send();

    let resp = timeout(TIMEOUT, fut).await.map_err(|_| CtrlErr::Timeout)??;

    match resp.status().as_u16() {
        200 => Ok(()),
        401 => Err(CtrlErr::Auth),
        c   => Err(CtrlErr::Http(c)),
    }
}