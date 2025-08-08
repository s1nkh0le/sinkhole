// controller/src/sync/data_pull.rs
use std::time::Duration;

use tokio::time::{sleep, Instant};

use crate::{
    db::insertion::{insert_ecam, insert_records},
    state::{ControllerState, Health, Service},
};
use shared::utils::time::now_ns;
use crate::fetch::fetch_records_ecam::{fetch_ecam, fetch_records};

const WATCHDOG: Duration = Duration::from_millis(2500);
const ECAM_INTERVAL:  Duration = Duration::from_secs(5);
const REC_INTERVAL:   Duration = Duration::from_secs(5);

pub fn spawn(state: ControllerState,
             http: reqwest::Client,
             db: crate::db::DbHandle)
{
    for svc_id in state.service_ids() {
        tokio::spawn(pull_loop(svc_id, state.clone(), http.clone(), db.clone()));
    }
}

async fn pull_loop(
    svc_id: String,
    state:  ControllerState,
    http:   reqwest::Client,
    db:     crate::db::DbHandle,
) {
    let mut next_ecam_at = Instant::now();
    let mut next_rec_at  = Instant::now();

    loop {
        // ---- snapshot immutable pieces ---------------------------------
        let (base_url, token, health, port_snap, ecam_since) = {
            let g = state.read();
            let svc: &Service = match g.services.get(&svc_id) {
                Some(s) => s,
                None => return, // service removed
            };

            let ports: Vec<(u16, u64)> = svc
                .ports
                .iter()
                .map(|(p, port)| (*p, port.cursors.next_records_since))
                .collect();

            (
                svc.cfg.base_url.clone(),
                svc.cfg.token.clone(),
                svc.metadata.health,
                ports,
                svc.service_cursors.next_ecam_since,
            )
        };

        let now_ns = now_ns();
        if !health.can_poll(now_ns) {
            sleep(WATCHDOG).await;
            continue;
        }

        // ---- ECAM ------------------------------------------------------
        if Instant::now() >= next_ecam_at {
            match fetch_ecam(&http, &base_url, &token, ecam_since).await {
                Ok((events, next_since)) => {
                    if !events.is_empty() {
                        insert_ecam(&db, &svc_id, &events).await;
                    }

                    state.write()
                        .services
                        .get_mut(&svc_id)
                        .map(|svc| svc.service_cursors.next_ecam_since = next_since);

                    state.write()
                        .services
                        .get_mut(&svc_id)
                        .map(|svc| svc.metadata.health = Health::record_success());
                }
                Err(err) => {
                    tracing::warn!(%svc_id, ?err, "ECAM pull failed");
                    state.write()
                        .services
                        .get_mut(&svc_id)
                        .map(|svc| svc.metadata.health = Health::record_failure(now_ns, svc.metadata.health));
                }
            }
            next_ecam_at = Instant::now() + ECAM_INTERVAL;
        }

        // ---- Records per port -----------------------------------------
        if Instant::now() >= next_rec_at {
            for (port, mut since) in port_snap {
                loop {
                    match fetch_records(&http, &base_url, port, &token, since).await {
                        Ok((records, next_since)) => {
                            if !records.is_empty() {
                                insert_records(&db, &svc_id, port, &records).await;
                            }

                            // update cursor
                            state.write()
                                .services
                                .get_mut(&svc_id)
                                .and_then(|svc| svc.ports.get_mut(&port))
                                .map(|p| p.cursors.next_records_since = next_since);

                            state.write()
                                .services
                                .get_mut(&svc_id)
                                .map(|svc| svc.metadata.health = Health::record_success());

                            let done = records.len() < 5000; // TODO
                            since = next_since;
                            if done { break; }
                        }
                        Err(err) => {
                            tracing::warn!(%svc_id, %port, ?err, "record pull failed");
                            state.write()
                                .services
                                .get_mut(&svc_id)
                                .map(|svc| svc.metadata.health = Health::record_failure(now_ns, svc.metadata.health));
                            break;
                        }
                    }
                }
            }
            next_rec_at = Instant::now() + REC_INTERVAL;
        }

        sleep(WATCHDOG).await;
    }
}
