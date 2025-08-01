use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use crate::config::ServiceCfg;
use shared::rules_model::{PortInfo, RuleSetRaw};

#[derive(Debug)]
pub struct Port {
    /// Latest config snapshot fetched from filter.
    pub info: Option<PortInfo>,
    /// Latest ruleset fetched from filter.
    pub ruleset: Option<RuleSetRaw>,
    /// Cursor for logs (next_since from last pull).
    pub next_logs_since: i64,
}

impl Port {
    pub fn new() -> Self {
        Self {
            info: None,
            ruleset: None,
            next_logs_since: 0,
        }
    }
}

#[derive(Debug)]
pub struct Service {
    pub cfg: ServiceCfg,
    pub ports: HashMap<u16, Port>,
    pub last_sync_ns: i64,
    pub healthy: bool,
    /// Global ECAM cursor (since last pulled).
    pub next_ecam_since: i64,
}

#[derive(Debug)]
pub struct Inner {
    pub services: HashMap<String, Service>,
}

#[derive(Debug, Clone)]
pub struct ControllerState {
    inner: Arc<RwLock<Inner>>,
}

impl ControllerState {
    pub fn new(cfg: crate::config::Config) -> anyhow::Result<Self> {
        let mut services = HashMap::new();
        for svc in cfg.services {
            services.insert(
                svc.id.clone(),
                Service {
                    cfg: svc,
                    ports: HashMap::new(),
                    last_sync_ns: 0,
                    healthy: false,
                    next_ecam_since: 0,
                },
            );
        }
        Ok(Self {
            inner: Arc::new(RwLock::new(Inner {
                services
            })),
        })
    }

    pub fn read(&self) -> parking_lot::RwLockReadGuard<'_, Inner> {
        self.inner.read()
    }

    pub fn write(&self) -> parking_lot::RwLockWriteGuard<'_, Inner> {
        self.inner.write()
    }
}
