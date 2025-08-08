use anyhow::Result;
use reqwest::Client;
use tokio::time::{sleep, Duration, Instant};

use crate::{
    state::{ControllerState, Health},
};
use shared::utils::time::now_ns;
use crate::fetch::fetch_state::fetch_state;

const REQ_TIMEOUT: Duration = Duration::from_secs(5);

pub fn spawn(state: ControllerState, http: Client, interval: Duration) {
    for svc_id in state.service_ids() {
        let st = state.clone();
        let cli = http.clone();
        tokio::spawn(async move {
            run_loop(svc_id, st, cli, interval).await;
        });
    }
}

async fn run_loop(
    svc_id: String,
    state: ControllerState,
    http: Client,
    interval: Duration,
) {
    let mut next_at = Instant::now();

    loop {
        let now = Instant::now();
        if now < next_at {
            sleep(Duration::from_millis(200)).await;
            continue;
        }

        // snapshot cfg
        let (base_url, token, poll_ok) = {
            let g = state.read();
            match g.services.get(&svc_id) {
                Some(svc) => {
                    let poll_ok = svc.metadata.health.can_poll(now_ns());
                    (svc.cfg.base_url.clone(), svc.cfg.token.clone(), poll_ok)
                }
                None => return,
            }
        };

        if !poll_ok {
            next_at = Instant::now() + interval;
            continue;
        }

        let fetch_res = fetch_state(&http, &base_url, &token, REQ_TIMEOUT).await;

        let mut w = state.write();
        let mut boot_map = std::mem::take(&mut w.boot_records_since);

        // ── inner block holds the &mut Service borrow ──────────────────────────
        {
            if let Some(svc) = w.services.get_mut(&svc_id) {
                match fetch_res {
                    Ok(snaps) => {
                        svc.metadata.health = Health::record_success();
                        svc.metadata.last_state_sync_ns = now_ns();

                        svc.merge_snapshots(snaps, &mut boot_map, &svc_id);
                    }
                    Err(err) => {
                        tracing::warn!(%svc_id, ?err, "state sync failed");
                        svc.metadata.health = Health::record_failure(now_ns(), svc.metadata.health);
                    }
                }
            }
        }
        w.boot_records_since = boot_map;
        
        next_at = Instant::now() + interval;
    }
}
