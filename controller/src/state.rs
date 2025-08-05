use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use serde::Serialize;
use crate::config::{ControllerCfg, ServiceCfg};
use crate::db::boot::read_boot_cursors;
use crate::db::DbHandle;
use shared::types::ruleset_types::{PortInfo, RuleSetRaw};

#[derive(Debug)]
pub struct Port {
    /// Latest config snapshot fetched from filter.
    pub info: Option<PortInfo>,
    /// Latest ruleset fetched from filter.
    pub ruleset: Option<RuleSetRaw>,
    /// Hash of markers+filter
    pub ruleset_version: Option<String>,
    /// Cursor for logs (next_since from last pull).
    pub port_cursors: PortCursors,
}

#[derive(Debug)]
pub struct PortCursors {
    /// Cursor for logs (next_since from last pull).
    pub next_logs_since: i64,
}

impl Port {
    pub fn new() -> Self {
        Self {
            info: None,
            ruleset: None,
            ruleset_version: None,
            port_cursors: PortCursors {
                next_logs_since: 0,
            }
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
    pub next_ecam_since: i64,
}
#[derive(Debug, Serialize)]
pub struct ServiceMetadata {
    pub healthy: bool,
    pub last_state_sync_ns: i64,
}



#[derive(Debug)]
pub struct Inner {
    pub services: HashMap<String, Service>,
    pub db: DbHandle,
    pub boot_logs_since: HashMap<(String, u16), i64>,
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
                        healthy: false,
                    }
                },
            );
        }

        Ok(Self {
            cfg: Arc::new(cfg.controller),
            inner: Arc::new(RwLock::new(Inner {
                services,
                db,
                boot_logs_since: boot_logs_map,
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
