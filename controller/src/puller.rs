use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::Client;
use tokio::time::{sleep, Instant};

use crate::{
    db,
    state::{ControllerState, Service},
};

const ECAM_INTERVAL: Duration = Duration::from_secs(5);
const LOGS_INTERVAL: Duration = Duration::from_secs(5);

const WATCHDOG: Duration = Duration::from_millis(2500);

/// Spawn all background pullers once, at startup.
pub fn spawn_all_pull_tasks(state: ControllerState, http: Client, db: db::DbHandle) {
    for (svc_id, _) in state.read().services.iter() {
        let svc_id = svc_id.clone();
        tokio::spawn(pull_loop(svc_id, state.clone(), http.clone(), db.clone()));
    }
}

/// One task per service – multiplexes ECAM & logs pulls.
async fn pull_loop(svc_id: String, state: ControllerState, http: Client, db: db::DbHandle) {
    info!("[pull_loop(svc_id={})] starting periodic sync", svc_id);

    let mut next_ecam_at = Instant::now();
    let mut next_logs_at = Instant::now();

    let mut sync_iter = 0;

    loop {
        let now = Instant::now();
        debug!(
            "[pull_loop(svc_id={})] sync iteration {}",
            svc_id, sync_iter
        );
        sync_iter += 1;


        if sync_iter == 3 {
            return
        }

        // ── snapshot immutable service config ───────────────
        let (base_url, token, port_snap, ecam_since) = {
            let guard = state.read();
            let svc: &Service = match guard.services.get(&svc_id) {
                Some(s) => s,
                None => return, // service removed → terminate
            };

            let ports: Vec<(u16, i64)> = svc
                .ports
                .iter()
                .map(|(p, port)| (*p, port.port_cursors.next_logs_since))
                .collect();

            (
                svc.cfg.base_url.clone(),
                svc.cfg.token.clone(),
                ports,
                svc.service_cursors.next_ecam_since,
            )
        };

        // ── ECAM ---------------------------------------------------------
        if now >= next_ecam_at {
            match fetch_ecam(&http, &base_url, &token, ecam_since).await {
                Ok((mut events, next_since)) => {
                    if !events.is_empty() {
                        debug!("[pull_loop(svc_id={}) -> ecam cache update] got {} new ecam entries: {:?}", svc_id, events.len(), events);
                        insert_ecam(&db, &svc_id, &events).await;
                        events.clear(); // drop big vec ASAP
                    } else {
                        debug!("[pull_loop(svc_id={}) -> ecam cache update] got no new ecam entries since {}", svc_id, ecam_since);
                    }

                    state
                        .write()
                        .services
                        .get_mut(&svc_id)
                        .map(|svc| svc.service_cursors.next_ecam_since = next_since);
                }
                Err(e) => {
                    tracing::warn!(%svc_id, error = ?e, "ECAM pull failed");
                }
            }
            next_ecam_at = Instant::now() + ECAM_INTERVAL;
        }

        // ── Logs per port -----------------------------------------------
        if now >= next_logs_at {
            for (port, since) in port_snap.iter() {
                match fetch_logs(&http, &base_url, *port, &token, *since).await {
                    Ok((mut recs, next_since)) => {
                        if !recs.is_empty() {
                            debug!(
                                "[pull_loop(svc_id={}) -> log cache update] got {} new logs: {:?}",
                                svc_id,
                                recs.len(),
                                recs
                            );
                            insert_logs(&db, &svc_id, *port, &recs).await;
                            recs.clear();
                        } else {
                            debug!(
                                "[pull_loop(svc_id={}) -> log cache update] no new logs since {}",
                                svc_id, since
                            );
                        }

                        state
                            .write()
                            .services
                            .get_mut(&svc_id)
                            .and_then(|svc| svc.ports.get_mut(port))
                            .map(|p| p.port_cursors.next_logs_since = next_since);
                    }
                    Err(e) => tracing::warn!(%svc_id, %port, error = ?e, "log pull failed"),
                }
            }
            next_logs_at = Instant::now() + LOGS_INTERVAL;
        }

        sleep(WATCHDOG).await;
    }
}

// ───────────────────────────────── fetch helpers ──────────────────────────
async fn fetch_ecam(
    http: &Client,
    base: &str,
    token: &str,
    since: i64,
) -> Result<(Vec<EcamEvent>, i64)> {
    debug!(
        "[fetch_ecam()] pulling from: {base}/ecam?token={}&since={}",
        token,
        since.to_string()
    );

    let resp: FilterEcamEnvelope = http
        .get(&format!("{base}/ecam"))
        .query(&[("token", token), ("since", &since.to_string())])
        .send()
        .await?
        .json()
        .await
        .context("invalid /ecam JSON")?;

    Ok((resp.data, resp.next_since))
}

async fn fetch_logs(
    http: &Client,
    base: &str,
    port: u16,
    token: &str,
    since: i64,
) -> Result<(Vec<EvaluationRecord>, i64)> {
    let url = format!("{base}/ports/{port}/logs");
    debug!(
        "[fetch_logs()] pulling from: {}?token={}&since={}&order=asc",
        url,
        token,
        since.to_string()
    );

    let resp: FilterLogsEnvelope = http
        .get(&url)
        .query(&[
            ("token", token),
            ("since", &since.to_string()),
            ("order", "asc"),
        ])
        .send()
        .await?
        .json()
        .await
        .context("invalid /logs JSON")?;

    Ok((resp.data.records, resp.data.next_since))
}

// ────────────────────────── minimal response structs ─────────────────────
use crate::db::insertion::{insert_ecam, insert_logs};
use serde::Deserialize;
use shared::types::record_types::EvaluationRecord;
use shared::types::ecam_types::EcamEvent;
use tracing::log::{debug, info};

#[derive(Deserialize)]
struct FilterEcamEnvelope {
    status: String,
    data: Vec<EcamEvent>,
    next_since: i64,
}

#[derive(Deserialize)]
struct FilterLogsEnvelope {
    status: String,
    data: LogsData,
}
#[derive(Deserialize)]
struct LogsData {
    records: Vec<EvaluationRecord>,
    next_since: i64,
}
