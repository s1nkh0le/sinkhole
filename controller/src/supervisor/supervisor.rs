use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use std::time::Duration;
use std::{collections::HashMap, sync::Arc};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::info;
use crate::config::env::ServiceCfg;
use crate::{
    state::ctrl::{ControllerState, Service, ServiceCursors},
    state::health::Health,
};
use crate::db::commands::DbHandle;

/// A running set of tasks for one service (health, state, ecam, records)
struct ServiceTasks {
    cancel: CancellationToken,
    handles: Vec<JoinHandle<()>>,
}

impl ServiceTasks {
    fn new(cancel: CancellationToken, handles: Vec<JoinHandle<()>>) -> Self {
        Self { cancel, handles }
    }

    fn cancel(&self) {
        self.cancel.cancel();
    }

    async fn join(self) {
        for h in self.handles {
            // Best effort: we don't propagate panics here; log if needed
            let _ = h.await;
        }
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

        info!("[Supervisor::remove_service({})] Service emoved", service_id);
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
                Duration::from_secs(5),
                Duration::from_secs(5),
                Duration::from_secs(5),
                Duration::from_secs(cfg.sync_interval_secs),
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

        info!("[Supervisor::reconfigure_service({})] Service reconfigured", cfg.id);

        // Restart tasks
        self.remove_service(&cfg.id).await?;
        self.spawn_for_service(&cfg.id).await
    }
}
