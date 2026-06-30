use crate::config::env::{ControllerCfg, ServiceCfg};
use crate::db::boot::{BootCursors, read_boot_cursors};
use crate::db::commands::DbHandle;
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

impl Default for Port {
    fn default() -> Self {
        Self::new()
    }
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

            let p = self.ports.entry(port).or_default();

            // initialise record cursor from boot map if this is first time
            if p.cursors.next_records_since == 0 {
                // We remove the entry so that we don't re-apply it accidentally
                if let Some(next) = &boot_cursors
                    .records_cursors
                    .remove(&(svc_id.to_string(), port))
                {
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

/// Build an in-memory `Service` in the initial (degraded, no ports) state.
fn mk_service(cfg: ServiceCfg, metadata: ServiceMetadata, next_ecam_since: u64) -> Service {
    Service {
        cfg,
        ports: HashMap::new(),
        service_cursors: ServiceCursors { next_ecam_since },
        health: Health::Degraded {
            faults: HashMap::new(),
            last_sync_ns: 0,
        },
        metadata,
    }
}

/// Reject a persisted service row that can't be a valid service. With hydration
/// on we'd rather refuse to start than serve a partial/inconsistent fleet.
fn validate_persisted_service(cfg: &ServiceCfg) -> anyhow::Result<()> {
    let hint = "fix the DB row or disable SINKHOLE_CONTROLLER_DB_HYDRATE";
    if cfg.id.trim().is_empty() {
        anyhow::bail!("persisted service has an empty id; fleet DB inconsistent ({hint})");
    }
    if cfg.token.trim().is_empty() {
        anyhow::bail!(
            "persisted service '{}' has an empty token; fleet DB inconsistent ({hint})",
            cfg.id
        );
    }
    if !cfg.base_url.starts_with("http://") && !cfg.base_url.starts_with("https://") {
        anyhow::bail!(
            "persisted service '{}' has an invalid base_url '{}'; fleet DB inconsistent ({hint})",
            cfg.id,
            cfg.base_url
        );
    }
    Ok(())
}

impl ControllerState {
    pub fn new(env_cfg: crate::env::Config, db: DbHandle, http: Client) -> anyhow::Result<Self> {
        // Cursors are derived from the records/ecam tables, independent of the
        // fleet hydration below, so record/event pulls always resume correctly.
        let boot_cursors: BootCursors = read_boot_cursors(&db.path)?;

        let crate::env::Config {
            controller,
            services: env_services,
            service_names,
        } = env_cfg;
        let hydrate = controller.db_hydrate;

        let next_ecam = |id: &str| boot_cursors.ecam_cursors.get(id).copied().unwrap_or(0);

        let mut services: HashMap<String, Service> = HashMap::new();

        // 1) Persisted fleet (DB) — only when hydration is enabled. Any read
        //    failure or invalid row is fatal: we verify the fleet before serving,
        //    rather than silently come up with a partial/inconsistent fleet.
        if hydrate {
            for ps in crate::db::util::read_all_services(&db.path)? {
                validate_persisted_service(&ps.cfg)?;
                let id = ps.cfg.id.clone();
                let next = next_ecam(&id);
                services.insert(id.clone(), mk_service(ps.cfg, ps.meta, next));
            }
        }

        // 2) Merge env-declared services on top.
        let mut newcomers: Vec<(ServiceCfg, ServiceMetadata)> = Vec::new();
        for cfg in env_services {
            if let Some(existing) = services.get(&cfg.id) {
                // Same id in env and DB: the stable connection data MUST agree.
                // We never silently pick one — diverging config is a fatal,
                // explicit boot error.
                if existing.cfg.base_url != cfg.base_url || existing.cfg.token != cfg.token {
                    // Report *which* fields diverge without printing the tokens
                    // themselves — boot errors land in logs/scrollback/CI output.
                    let url_diff = if existing.cfg.base_url != cfg.base_url {
                        format!(
                            "url differs (env={}, db={})",
                            cfg.base_url, existing.cfg.base_url
                        )
                    } else {
                        "url matches".to_string()
                    };
                    let tok_diff = if existing.cfg.token != cfg.token {
                        "token differs"
                    } else {
                        "token matches"
                    };
                    anyhow::bail!(
                        "service id '{id}' is declared in both the environment and the persisted \
                         DB with different connection data: {url_diff}; {tok_diff}. \
                         Edit SINKHOLE_CONTROLLER_SERVICES to match, or disable \
                         SINKHOLE_CONTROLLER_DB_HYDRATE.",
                        id = cfg.id,
                    );
                }
                // Matches → the DB entry (and its metadata) already wins; the env
                // `name` is intentionally ignored when the DB already has the service.
            } else {
                // New service from env: seed its display name from the env config.
                let mut metadata = ServiceMetadata::default();
                if let Some(name) = service_names.get(&cfg.id) {
                    metadata.name = Some(name.clone());
                }
                let next = next_ecam(&cfg.id);
                services.insert(
                    cfg.id.clone(),
                    mk_service(cfg.clone(), metadata.clone(), next),
                );
                if hydrate {
                    newcomers.push((cfg, metadata));
                }
            }
        }

        // 3) Persist env newcomers so the whole fleet lives in the DB and a
        //    subsequent restart brings everything back without re-declaring env.
        if hydrate && !newcomers.is_empty() {
            let conn = crate::db::util::open(&db.path)?;
            let now = shared::utils::time::now_ns() as i64;
            for (cfg, meta) in &newcomers {
                crate::db::util::upsert_service(&conn, cfg, meta, now);
            }
        }

        Ok(Self {
            cfg: Arc::new(controller),
            http,
            inner: Arc::new(RwLock::new(Inner {
                services,
                db,
                boot_cursors,
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
