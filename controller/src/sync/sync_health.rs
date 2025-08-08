use crate::fetch::error::CtrlErr;
use crate::state::{ControllerState, Health, Service, ServiceMetadata};
use axum::http::StatusCode;
use reqwest::Client;
use shared::utils::time::now_ns;
use std::time::Duration;
use tokio::time::{sleep, timeout};

const INTERVAL: Duration = Duration::from_secs(1);
const TIMEOUT: Duration = Duration::from_secs(2);

pub fn spawn(state: ControllerState, http: Client) {
    for svc_id in state.service_ids() {
        let st = state.clone();
        let cli = http.clone();

        tokio::spawn(async move {
            probe_loop(svc_id, st, cli).await;
        });
    }
}

async fn probe_loop(svc_id: String, state: ControllerState, http: Client) {
    loop {
        let should_probe = {
            let g = state.read();
            matches!(g.services.get(&svc_id).map(|s| &s.metadata.health), Some(Health::Unreachable { .. }))
        };

        if !should_probe {
            sleep(INTERVAL).await;
            continue;
        }

        // snapshot
        let (base_url, token) = {
            let g = state.read();
            match g.services.get(&svc_id) {
                Some(svc) => (svc.cfg.base_url.clone(), svc.cfg.token.clone()),
                None => return,
            }
        };

        let ok = match ping(&http, &base_url, &token).await {
            Ok(_) => true,
            Err(CtrlErr::Auth) => {
                tracing::warn!(%svc_id, "probe 401 – token wrong?");
                false
            }
            Err(_) => false,
        };

        {
            let mut w = state.write();
            if let Some(svc) = w.services.get_mut(&svc_id) {
                if ok {
                    svc.metadata.health = Health::record_success();
                } else {
                    svc.metadata.health =
                        Health::record_failure(now_ns(), svc.metadata.health);
                }
            }
        }

        sleep(INTERVAL).await;
    }
}

async fn ping(http: &Client, base: &str, token: &str) -> Result<(), CtrlErr> {
    let fut = http
        .get(&format!("{base}/auth/ping"))
        .query(&[("token", token)])
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
