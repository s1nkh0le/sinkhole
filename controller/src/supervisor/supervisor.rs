use crate::config::env::ServiceCfg;
use crate::db::commands::DbHandle;
use crate::{
    state::ctrl::{ControllerState, Service, ServiceCursors},
    state::health::Health,
};
use anyhow::{Result, anyhow};
use parking_lot::Mutex;
use std::time::Duration;
use std::{collections::HashMap, sync::Arc};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::info;

/// A running set of tasks for one service (health, state, ecam, records)
struct ServiceTasks {
    cancel: CancellationToken,
    handles: Vec<JoinHandle<()>>,
}

impl ServiceTasks {
    fn new(cancel: CancellationToken, handles: Vec<JoinHandle<()>>) -> Self {
        Self { cancel, handles }
    }
}

/// Supervises lifecycle of service tasks and dynamic add/remove.
#[derive(Clone)]
pub struct Supervisor {
    state: ControllerState,
    db: DbHandle,
    tasks: Arc<Mutex<HashMap<String, ServiceTasks>>>,
}

impl Supervisor {
    pub fn new(state: ControllerState, db: DbHandle) -> Self {
        Self {
            state,
            db,
            tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Spawn task groups for all known services at startup.
    pub async fn spawn_all_existing(&self) {
        for id in self.state.service_ids() {
            // Ignore errors here; they’ll be visible on API when adding
            let _ = self.spawn_for_service(&id).await;
        }
    }

    /// Add a new service at runtime (mutates state and starts tasks).
    pub async fn add_service(&self, cfg: ServiceCfg) -> Result<()> {
        // Atomically: check + insert
        {
            let mut w = self.state.write();
            if w.services.contains_key(&cfg.id) {
                anyhow::bail!("service '{}' already exists", cfg.id);
            }

            w.services.insert(
                cfg.id.clone(),
                Service {
                    cfg: cfg.clone(),
                    ports: Default::default(),
                    service_cursors: ServiceCursors { next_ecam_since: 0 },
                    health: Health::Degraded {
                        faults: Default::default(),
                        last_sync_ns: 0,
                    },
                    metadata: Default::default(),
                },
            );
        }

        // Resume from persisted state if this service was known before (e.g. a
        // controller restart + re-add via API): refresh boot cursors so we don't
        // re-pull the filter's entire history, and restore display metadata.
        self.refresh_persisted_state(&cfg.id);

        // Spawn tasks (idempotent per service_id)
        if let Err(e) = self.spawn_for_service(&cfg.id).await {
            // Roll back state on failure
            let mut w = self.state.write();
            w.services.remove(&cfg.id);
            return Err(e);
        }

        info!("[Supervisor::add_service({})] New service added", cfg.id);
        Ok(())
    }

    /// Pull this service's persisted cursors + metadata from the DB into live
    /// state. Used when a service is added at runtime so it resumes instead of
    /// re-pulling the filter's whole history. Best-effort; logs on failure.
    fn refresh_persisted_state(&self, service_id: &str) {
        let path = self.db.path.as_str();

        if let Ok(bc) = crate::db::boot::read_boot_cursors(path) {
            let mut w = self.state.write();
            if let Some(v) = bc.ecam_cursors.get(service_id).copied() {
                w.boot_cursors
                    .ecam_cursors
                    .insert(service_id.to_string(), v);
                if let Some(svc) = w.services.get_mut(service_id)
                    && svc.service_cursors.next_ecam_since == 0
                {
                    svc.service_cursors.next_ecam_since = v;
                }
            }
            for ((svc, port), v) in bc.records_cursors.iter() {
                if svc == service_id {
                    w.boot_cursors
                        .records_cursors
                        .insert((svc.clone(), *port), *v);
                }
            }
        } else {
            tracing::warn!(%service_id, "could not refresh boot cursors for added service");
        }

        // Restore persisted display metadata for this service, if any row exists.
        if let Ok(services) = crate::db::util::read_all_services(path)
            && let Some(ps) = services.into_iter().find(|s| s.cfg.id == service_id)
        {
            let mut w = self.state.write();
            if let Some(svc) = w.services.get_mut(service_id) {
                svc.metadata = ps.meta;
            }
        }
    }

    /// Stop background tasks for `service_id` (if any). Idempotent.
    /// Does **not** mutate ControllerState (the handler should remove the service from state after this).
    pub async fn remove_service(&self, service_id: &str) -> Result<()> {
        // 1) Take ownership of tasks under a short critical section.
        //    Do not await while holding the MutexGuard.
        let (handles, cancel) = {
            let mut task_map = self.tasks.lock();
            match task_map.remove(service_id) {
                Some(ServiceTasks { cancel, handles }) => (handles, cancel),
                None => return Ok(()), // nothing to stop: idempotent
            }
        };

        // 2) Signal shutdown first, then await all tasks.
        cancel.cancel();

        // Optional: give tasks a bounded time to finish.
        for (idx, h) in handles.into_iter().enumerate() {
            match tokio::time::timeout(Duration::from_secs(10), h).await {
                Ok(Ok(())) => { /* task exited cleanly */ }
                Ok(Err(join_err)) => {
                    // task panicked or was aborted
                    tracing::warn!(%service_id, task_index = idx, error=?join_err, "task join error");
                }
                Err(_elapsed) => {
                    // timed out waiting; tasks should honor CancellationToken quickly
                    tracing::warn!(%service_id, task_index = idx, "task did not stop within timeout");
                }
            }
        }

        info!(
            "[Supervisor::remove_service({})] Service removed",
            service_id
        );
        Ok(())
    }

    /// Ensure we have a task group for `service_id`. Idempotent.
    pub async fn spawn_for_service(&self, service_id: &str) -> Result<()> {
        // Don’t double-spawn
        if self.tasks.lock().contains_key(service_id) {
            return Ok(());
        }

        // Snapshot configs for spawning
        let (http, health_interval, ecam_interval, rec_interval, state_interval) = {
            let cfg = self.state.cfg();
            (
                &self.state.http,
                cfg.sync_health_interval,
                cfg.sync_ecam_interval,
                cfg.sync_records_interval,
                cfg.sync_config_interval,
            )
        };

        let cancel = CancellationToken::new();

        // Each sync module must expose a per-service spawner that accepts a CancellationToken.
        let h1 = crate::sync::sync_health::spawn_for_service(
            service_id.to_string(),
            self.state.clone(),
            http.clone(),
            health_interval,
            cancel.clone(),
        );

        let h2 = crate::sync::sync_state::spawn_for_service(
            service_id.to_string(),
            self.state.clone(),
            http.clone(),
            state_interval,
            cancel.clone(),
        );

        let h3 = crate::sync::sync_ecam::spawn_for_service(
            service_id.to_string(),
            self.state.clone(),
            http.clone(),
            self.db.clone(),
            ecam_interval,
            cancel.clone(),
        );

        let h4 = crate::sync::sync_records::spawn_for_service(
            service_id.to_string(),
            self.state.clone(),
            http.clone(),
            self.db.clone(),
            rec_interval,
            cancel.clone(),
        );

        self.tasks.lock().insert(
            service_id.to_string(),
            ServiceTasks::new(cancel, vec![h1, h2, h3, h4]),
        );

        Ok(())
    }

    /// For reconfiguring an existing service (e.g., change URL/token):
    /// update state and restart its tasks atomically.
    pub async fn reconfigure_service(&self, cfg: ServiceCfg) -> Result<()> {
        // Update state
        {
            let mut w = self.state.write();
            let Some(svc) = w.services.get_mut(&cfg.id) else {
                return Err(anyhow!("service '{}' not found", cfg.id));
            };
            svc.cfg = cfg.clone();
        }

        info!(
            "[Supervisor::reconfigure_service({})] Service reconfigured",
            cfg.id
        );

        // Restart tasks
        self.remove_service(&cfg.id).await?;
        self.spawn_for_service(&cfg.id).await
    }
}
