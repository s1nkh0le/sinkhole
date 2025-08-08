use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use rand::{rng, Rng};
use serde::Serialize;
use crate::config::{ControllerCfg, ServiceCfg};
use crate::db::boot::read_boot_cursors;
use crate::db::DbHandle;
use shared::types::snapshot_types::PortSnapshot;

#[derive(Debug)]
pub struct Port {
    /// Latest snapshot fetched from the filter (`/fetch`).
    pub snap: Option<PortSnapshot>,

    /// Cursor for records pulls.
    pub cursors: PortCursors,
}

#[derive(Debug)]
pub struct PortCursors {
    pub next_records_since: u64,
}

impl Port {
    pub fn new() -> Self {
        Self {
            snap: None,
            cursors: PortCursors { next_records_since: 0 },
        }
    }
}

#[derive(Debug)]
pub struct Service {
    pub cfg: ServiceCfg,
    pub ports: HashMap<u16, Port>,
    pub service_cursors: ServiceCursors,
    pub metadata: ServiceMetadata,
}

#[derive(Debug, Serialize)]
pub struct ServiceCursors {
    /// Global ECAM cursor (since last pulled).
    pub next_ecam_since: u64,
}

impl Service {
    /// Merge a freshly fetched snapshot vector into this service.
    ///
    /// – Creates new Port entries if needed
    /// – Updates existing ones if `ver_combined` changed
    /// – Removes stale ports that disappeared from the filter
    pub fn merge_snapshots(&mut self,
                           snaps: Vec<PortSnapshot>,
                           boot_logs_map: &mut HashMap<(String,u16),u64>,
                           svc_id: &str)
    {
        let mut seen = std::collections::HashSet::new();

        for snap in snaps {
            let port = snap.info.public;
            seen.insert(port);

            let p = self.ports.entry(port).or_insert_with(Port::new);

            // initialise record cursor from boot map if this is first time
            if p.cursors.next_records_since == 0 {
                if let Some(next) = boot_logs_map.remove(&(svc_id.to_string(), port)) {
                    p.cursors.next_records_since = next;
                }
            }

            let need_update = match &p.snap {
                None => true,
                Some(cur) => cur.ver_combined != snap.ver_combined,
            };
            if need_update {
                p.snap = Some(snap);
            }
        }

        // Remove ports no longer present
        self.ports.retain(|port, _| seen.contains(port));
    }
}






/// Health status of one filter instance
#[derive(Debug, Clone, Copy, Serialize, Eq, PartialEq)]
pub enum Health {
    Healthy,
    Unreachable {
        /// # consecutive failed probes
        failures: u32,
        /// do not probe again before this deadline (ns)
        backoff_until_ns: u64,
    },
}

impl Health {
    /// Fixed retry cadence every time the filter is unreachable
    pub const RETRY_NS: u64 = 10 * 1_000_000_000; // 10 s

    /// On success, we’re healthy again.
    pub fn record_success() -> Self {
        Health::Healthy
    }

    /// Constant-cadence failure (no back-off) – used by probes.
    pub fn record_failure(now_ns: u64, _prev: Health) -> Self {
        Health::Unreachable {
            failures: 0,
            backoff_until_ns: now_ns + Self::RETRY_NS,
        }
    }

    /// Exponential back-off with ±10 % jitter.
    pub fn record_failure_backoff(now_ns: u64, prev: Health) -> Self {
        fn next_backoff_ns(failures: u32) -> u64 {
            let secs = 2u64.saturating_pow(failures.min(8));       // 1,2,4, ... 256 s
            secs * 1_000_000_000
        }
        fn jitter(ns: u64) -> u64 {
            let delta = (ns as f64 * 0.10) as u64;                 // ±10 %
            let mut rng = rng(); // pre-initialized rng
            let offset = rng.random_range(0..=delta);
            ns - delta / 2 + offset
        }

        let new_failures = match prev {
            Health::Unreachable { failures, .. } => failures + 1,
            Health::Healthy => 1,
        };
        let backoff = jitter(next_backoff_ns(new_failures));

        Health::Unreachable {
            failures: new_failures,
            backoff_until_ns: now_ns + backoff,
        }
    }

    /// Are we allowed to poll right now?
    pub fn can_poll(&self, now_ns: u64) -> bool {
        match self {
            Health::Healthy => true,
            Health::Unreachable { backoff_until_ns, .. } => now_ns >= *backoff_until_ns,
        }
    }
}


#[derive(Debug, Serialize)]
pub struct ServiceMetadata {
    pub health: Health,
    pub last_state_sync_ns: u64,
    pub last_error: Option<String>,
}

#[derive(Debug)]
pub struct Inner {
    pub services: HashMap<String, Service>,
    pub db: DbHandle,
    pub boot_records_since: HashMap<(String, u16), u64>,
}

#[derive(Debug, Clone)]
pub struct ControllerState {
    cfg: Arc<ControllerCfg>,
    inner: Arc<RwLock<Inner>>,
}

impl ControllerState {
    pub fn new(cfg: crate::config::Config, db: DbHandle) -> anyhow::Result<Self> {
        // 1) Read boot cursors from DB
        let (boot_cursor_map, boot_logs_map) = read_boot_cursors(&db.path)?;

        let mut services = HashMap::new();
        for svc in cfg.services {
            let boot_ecam_since = boot_cursor_map.get(&svc.id).copied().unwrap_or(0);

            services.insert(
                svc.id.clone(),
                Service {
                    cfg: svc,
                    ports: HashMap::new(),
                    service_cursors: ServiceCursors {
                        next_ecam_since: boot_ecam_since
                    },
                    metadata: ServiceMetadata {
                        last_state_sync_ns: 0,
                        health: Health::Unreachable {
                            failures: 0,
                            backoff_until_ns: 0,
                        },
                        last_error: None,
                    }
                },
            );
        }

        Ok(Self {
            cfg: Arc::new(cfg.controller),
            inner: Arc::new(RwLock::new(Inner {
                services,
                db,
                boot_records_since: boot_logs_map,
            })),
        })
    }

    pub fn cfg(&self) -> Arc<ControllerCfg> {
        self.cfg.clone()
    }

    pub fn db(&self) -> DbHandle {
        self.inner.read().db.clone()
    }

    pub fn service_ids(&self) -> Vec<String> {
        self.read().services.keys().cloned().collect()
    }

    pub fn read(&self) -> parking_lot::RwLockReadGuard<'_, Inner> {
        self.inner.read()
    }

    pub fn write(&self) -> parking_lot::RwLockWriteGuard<'_, Inner> {
        self.inner.write()
    }
}
