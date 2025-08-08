use std::thread::sleep;
use std::time::{Duration, Instant};
use reqwest::Client;
use shared::utils::time::now_ns;
use crate::state::{ControllerState, Health};
use crate::fetch::fetch_state::fetch_state;

 /*
async fn state_sync_loop(
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
        let (base_url, token) = {
            let g = state.read();
            match g.services.get(&svc_id) {
                Some(svc) => (svc.cfg.base_url.clone(), svc.cfg.token.clone()),
                None => return,
            }
        };

        let fetch_res = fetch_state(&http, &base_url, &token, Duration::from_secs(5)).await;

        let mut w = state.write();
        if let Some(svc) = w.services.get_mut(&svc_id) {
            match fetch_res {
                Ok(snaps) => {
                    svc.metadata.health = Health::Healthy;
                    svc.metadata.last_state_sync_ns = now_ns();

                    svc.merge_snapshots(snaps, &mut w.boot_records_since, &svc_id);
                }
                Err(err) => {
                    tracing::warn!(%svc_id, ?err, "state sync failed");
                    let now = now_ns();
                    svc.metadata.health = Health::record_failure(now, svc.metadata.health);
                }
            }
        }

        next_at = Instant::now() + interval;
    }
} */