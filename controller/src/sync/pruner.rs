use std::time::Duration;
use tokio::task::JoinHandle;
use tokio::time::{Instant, sleep};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::db::commands::DbHandle;
use crate::db::insertion::prune_before;
use crate::fetch::fetch_prune::prune_filter_records;
use crate::state::ctrl::ControllerState;

const TICK: Duration = Duration::from_millis(500);

/// Spawn the single global pruner. It periodically:
///
/// 1. asks each filter to prune records older than `retention_filter` (those
///    records are already centralized), keeping filter DBs bounded;
/// 2. prunes the central controller DB beyond `retention_db` (if enabled).
///
/// Cancelling `cancel` stops the loop promptly.
pub fn spawn(state: ControllerState, db: DbHandle, cancel: CancellationToken) -> JoinHandle<()> {
    tokio::spawn(run(state, db, cancel))
}

async fn run(state: ControllerState, db: DbHandle, cancel: CancellationToken) {
    let cfg = state.cfg();
    let interval = cfg.prune_interval;
    let filter_retention_ns = cfg.retention_filter.as_nanos() as u64;
    let db_retention_ns = cfg.retention_db.as_nanos() as u64;

    // The pruner always runs: filter DBs are ephemeral staging and must be kept
    // bounded (pruned down to the confirmed cursor + the grace window). The central
    // DB is only pruned when `retention_db` is set.

    // First run after one interval (let initial sync settle).
    let mut next_at = Instant::now() + interval;

    loop {
        if cancel.is_cancelled() {
            break;
        }

        let now = Instant::now();
        if now < next_at {
            let wait = (next_at - now).min(TICK);
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = sleep(wait) => {}
            }
            continue;
        }

        let now_ns = shared::utils::time::now_ns();

        // ── 1. Prune filter DBs (always; they're ephemeral staging) ────────
        {
            let cutoff = now_ns.saturating_sub(filter_retention_ns);

            // Snapshot (base_url, token, [(port, cursor)]) per service.
            let services: Vec<(String, String, String, Vec<(u16, u64)>)> = {
                let g = state.read();
                g.services
                    .iter()
                    .map(|(id, svc)| {
                        let ports = svc
                            .ports
                            .iter()
                            .map(|(p, port)| (*p, port.cursors.next_records_since))
                            .collect::<Vec<_>>();
                        (
                            id.clone(),
                            svc.cfg.base_url.clone(),
                            svc.cfg.token.clone(),
                            ports,
                        )
                    })
                    .collect()
            };

            for (svc_id, base_url, token, ports) in services {
                for (port, cursor) in ports {
                    if cancel.is_cancelled() {
                        break;
                    }
                    // Never prune past the confirmed cursor (= max centralized ts
                    // + 1). With confirmed writes this is a hard guarantee: we only
                    // delete filter records that are durably in the central DB.
                    let safe_cutoff = cutoff.min(cursor.saturating_sub(1));
                    if safe_cutoff == 0 {
                        continue;
                    }
                    if let Err(e) =
                        prune_filter_records(&state.http, &base_url, &token, safe_cutoff).await
                    {
                        warn!(%svc_id, %port, error=%e, "filter prune failed");
                    } else {
                        debug!(%svc_id, %port, cutoff = safe_cutoff, "filter pruned");
                    }
                }
            }
        }

        // ── 2. Prune central DB ────────────────────────────────────────────
        if db_retention_ns > 0 {
            let cutoff = now_ns.saturating_sub(db_retention_ns) as i64;
            if cutoff > 0 {
                prune_before(&db, cutoff).await;
            }
        }

        next_at = Instant::now() + interval;
    }
}
