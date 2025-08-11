use crate::config::env::{ControllerCfg, ServiceCfg};
use crate::db::boot::{read_boot_cursors, BootCursors};
use crate::db::DbHandle;
use crate::state::health::Health;
use parking_lot::RwLock;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use shared::types::snapshot_types::PortSnapshot;
use std::collections::HashMap;
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Port {
    /// Latest snapshot fetched from the filter (`/fetch`).
    pub snap: Option<PortSnapshot>,

    /// Cursor for records pulls.
    pub cursors: PortCursors,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortCursors {
    pub next_records_since: u64,
}

impl Port {
    pub fn new() -> Self {
        Self {
            snap: None,
            cursors: PortCursors {
                next_records_since: 0,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    pub cfg: ServiceCfg,
    pub ports: HashMap<u16, Port>,
    pub service_cursors: ServiceCursors,
    pub health: Health,
    pub metadata: ServiceMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
    pub fn merge_snapshots(
        &mut self,
        snaps: Vec<PortSnapshot>,
        boot_cursors: &mut BootCursors,
        svc_id: &str,
    ) {
        let mut seen = std::collections::HashSet::new();

        for snap in snaps {
            let port = snap.info.public;
            seen.insert(port);

            let p = self.ports.entry(port).or_insert_with(Port::new);

            // initialise record cursor from boot map if this is first time
            if p.cursors.next_records_since == 0 {
                // We remove the entry so that we don't re-apply it accidentally
                if let Some(next) = &boot_cursors.records_cursors.remove(&(svc_id.to_string(), port)) {
                    p.cursors.next_records_since = *next;
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


#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]
pub struct ServiceMetadata {
    pub name: Option<String>,
    pub description: Option<String>,
    pub hex_color: Option<String>,
    pub icon: Option<String>,
}

#[derive(Debug)]
pub struct Inner {
    pub services: HashMap<String, Service>,
    pub db: DbHandle,
    pub boot_cursors: BootCursors,
}

#[derive(Debug, Clone)]
pub struct ControllerState {
    pub cfg: Arc<ControllerCfg>,
    pub http: Client,
    pub inner: Arc<RwLock<Inner>>,
}

impl ControllerState {
    pub fn new(env_cfg: crate::env::Config, db: DbHandle, http: Client) -> anyhow::Result<Self> {
        // Services hashmap, initialized as empty
        let mut services = HashMap::new();

        // Reading boot cursors from the database, so that when a previously-existing service is added,
        // we can pick up the fetching process where we left off
        let boot_cursors: BootCursors = read_boot_cursors(&db.path)?;

        // We start booting services from the environment variables
        for svc in env_cfg.services {
            // If the service has an entry, we extract it to store insid
            let boot_ecam_since = boot_cursors.ecam_cursors.get(&svc.id).copied().unwrap_or(0);

            services.insert(
                svc.id.clone(),
                Service {
                    cfg: svc,
                    ports: HashMap::new(),
                    service_cursors: ServiceCursors {
                        next_ecam_since: boot_ecam_since,
                    },
                    health: Health::Degraded {
                        faults: HashMap::new(),
                        last_sync_ns: 0
                    },
                    metadata: ServiceMetadata::default(),
                },
            );
        }

        Ok(Self {
            cfg: Arc::new(env_cfg.controller),
            http,
            inner: Arc::new(RwLock::new(Inner { services, db, boot_cursors, })),
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
