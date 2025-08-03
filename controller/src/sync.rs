use crate::observability::summary::dump_controller_state;
use crate::state::ControllerState;
use anyhow::{Context, Result};
use axum::http::StatusCode;
use reqwest::Client;
use serde::Deserialize;
use shared::rules_model::{PortInfo, RuleSetRaw};
use std::{collections::HashSet, mem, time::Duration};
use tokio::{
    sync::Semaphore,
    time::{sleep, timeout, Instant},
};
use tracing::log::{debug, info};
use tracing::warn;

const REQ_TIMEOUT: Duration = Duration::from_secs(10);

pub fn spawn_all_state_sync(state: ControllerState, http: Client, cfg: crate::config::Config) {
    for svc_id in state.service_ids() {
        let state = state.clone();
        let http = http.clone();
        let interval = Duration::from_secs(cfg.controller.sync_interval_secs);
        let concurrency = cfg.controller.sync_concurrency;

        tokio::spawn(async move {
            state_sync_loop(svc_id, state, http, interval, concurrency).await;
        });
    }
}

async fn state_sync_loop(
    svc_id: String,
    state: ControllerState,
    http: Client,
    interval: Duration,
    concurrency: usize,
) {
    info!(
        "[state_sync_loop(svc_id={})] starting periodic sync",
        svc_id
    );

    let sem = std::sync::Arc::new(Semaphore::new(concurrency.max(1)));
    let mut next_at = Instant::now();

    let mut sync_iter = 0;

    loop {
        let now = Instant::now();
        if now < next_at {
            sleep(Duration::from_millis(200)).await;
            continue;
        }

        // snapshot service creds/URL without holding lock across await
        let (base_url, token) = {
            let g = state.read();
            match g.services.get(&svc_id) {
                Some(svc) => (svc.cfg.base_url.clone(), svc.cfg.token.clone()),
                None => return, // service removed → terminate task
            }
        };

        sync_iter += 1;
        debug!(
            "[state_sync_loop(svc_id={})] sync iteration {} for service {}",
            svc_id, sync_iter, base_url
        );

        // 1) Discover all ports and their PortInfo
        match fetch_ports_info(&http, &base_url, &token).await {
            Ok(port_infos) => {
                // Update state PortInfo and collect which ports exist now
                let discovered: HashSet<u16> = port_infos.iter().map(|pi| pi.public).collect();

                let mut ports: Vec<u16> = discovered.iter().copied().collect();
                ports.sort_unstable();

                // 1) Debug-print the Vec:
                debug!(
                    "[state_sync_loop(svc_id={})] discovered ports: {:?}. Checking hashes",
                    svc_id, ports
                );

                // Pre-write: update infos and figure out which ports need ruleset refresh
                let to_refresh: Vec<(u16, String)> = {
                    let mut w = state.write();

                    let mut boot_map = mem::take(&mut w.boot_logs_since);

                    let svc = match w.services.get_mut(&svc_id) {
                        Some(s) => s,
                        None => return,
                    };

                    let mut to_refresh: Vec<(u16, String)> = Vec::new();

                    // Update info snapshots
                    for pi in &port_infos {
                        let p = svc
                            .ports
                            .entry(pi.public)
                            .or_insert_with(crate::state::Port::new);
                        p.info = Some(pi.clone());

                        // Initialize per-port log cursor if still unset:
                        if p.next_logs_since == 0 {
                            if let Some(next) = boot_map.remove(&(svc_id.clone(), pi.public)) {
                                p.next_logs_since = next;
                            }
                        }

                        // Decide if we need to refresh ruleset: compare version hint
                        let need_ruleset = match (&p.ruleset_version, &pi.version) {
                            (None, _) => true,
                            (Some(curr), newv) => curr != newv,
                        };

                        // Temporarily store a marker if needed; we return it below
                        if need_ruleset {
                            to_refresh.push((pi.public, pi.version.clone()))
                        }
                    }

                    w.boot_logs_since = boot_map;
                    to_refresh
                };

                if to_refresh.is_empty() {
                    debug!("[state_sync_loop(svc_id={})] no ports to refresh", svc_id);
                } else {
                    debug!(
                        "[state_sync_loop(svc_id={})] ports to refresh: {:?}",
                        svc_id, to_refresh
                    );
                }

                // 2) Fetch rulesets for ports that changed (bounded concurrency)
                let mut tasks = Vec::with_capacity(to_refresh.len());
                for (port, new_ver) in to_refresh {
                    let http = http.clone();
                    let base_url = base_url.clone();
                    let token = token.clone();
                    let sem = sem.clone();
                    let svc_id = svc_id.clone();
                    let state = state.clone();

                    tasks.push(tokio::spawn(async move {
                        let _permit = sem.acquire().await.unwrap();
                        match fetch_port_ruleset(&http, &base_url, &token, port).await {
                            Ok((rs, rs_ver)) => {
                                // rs_ver should match new_ver; we still trust the endpoint
                                let mut w = state.write();
                                if let Some(svc) = w.services.get_mut(&svc_id) {
                                    debug!(
                                        "[state_sync_loop(svc_id={}), port={}] updated config {}",
                                        svc_id,
                                        port,
                                        rs.clone()
                                    );

                                    let p = svc
                                        .ports
                                        .entry(port)
                                        .or_insert_with(crate::state::Port::new);
                                    p.ruleset = Some(rs);
                                    p.ruleset_version = Some(rs_ver);
                                }
                            }
                            Err(e) => {
                                tracing::warn!(%svc_id, %port, error=?e, "ruleset fetch failed");
                            }
                        }
                    }));
                }

                for t in tasks {
                    let _ = t.await;
                }

                // 3) (Optional) mark disappearances / set healthy + last_sync
                {
                    let mut w = state.write();
                    if let Some(svc) = w.services.get_mut(&svc_id) {
                        // If you want to remove stale ports, you can do it here by comparing svc.ports.keys() with `discovered`.
                        svc.healthy = true;
                        svc.last_sync_ns = now_ns();
                    }
                }
            }
            Err(e) => {
                tracing::warn!(%svc_id, error=?e, "sync ports discovery failed");
                let mut w = state.write();
                if let Some(svc) = w.services.get_mut(&svc_id) {
                    svc.healthy = false;
                    svc.last_sync_ns = now_ns();
                }
            }
        }

        next_at = Instant::now() + interval;
    }
}

// ─────────────────────────── HTTP fetch helpers ───────────────────────────

#[derive(Deserialize)]
struct PortsEnvelope {
    status: String,
    data: Vec<PortInfo>, // matches your filter: list of PortInfo
}

async fn fetch_ports_info(http: &Client, base: &str, token: &str) -> Result<Vec<PortInfo>> {
    let url = format!("{base}/ports");
    debug!("[fetch_ports_info()] request to {}?token={}", url, token);

    let fut = http.get(&url).query(&[("token", token)]).send();

    let resp = timeout(REQ_TIMEOUT, fut).await??;
    if resp.status() != StatusCode::OK {
        anyhow::bail!("GET /ports non-200: {}", resp.status());
    }
    let env: PortsEnvelope = resp.json().await.context("parse /ports JSON")?;
    if env.status != "ok" {
        anyhow::bail!("GET /ports status={}", env.status);
    }
    Ok(env.data)
}

#[derive(Deserialize)]
struct RulesetEnvelope {
    status: String,
    data: RulesetData,
}

#[derive(Deserialize)]
struct RulesetData {
    ruleset: RuleSetRaw,
    version: String,
}

async fn fetch_port_ruleset(
    http: &Client,
    base: &str,
    token: &str,
    port: u16,
) -> Result<(RuleSetRaw, String)> {
    let url = format!("{base}/ports/{port}/ruleset");
    debug!(
        "[fetch_port_ruleset({})] request to {}?token={}",
        port, url, token
    );

    let fut = http.get(&url).query(&[("token", token)]).send();
    let resp = timeout(REQ_TIMEOUT, fut).await??;
    if resp.status() != StatusCode::OK {
        anyhow::bail!("GET {url} non-200: {}", resp.status());
    }
    let env: RulesetEnvelope = resp.json().await.context("parse /ruleset JSON")?;
    if env.status != "ok" {
        anyhow::bail!("GET {url} status={}", env.status);
    }

    if env.data.ruleset.filters.is_empty() && env.data.ruleset.markers.is_empty() {
        warn!(
            "[fetch_port_ruleset({})] incoming port ruleset empty ({})",
            port, env.data.version
        );
    }

    Ok((env.data.ruleset, env.data.version))
}

fn now_ns() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    (dur.as_secs() as i64)
        .saturating_mul(1_000_000_000)
        .saturating_add(dur.subsec_nanos() as i64)
}
