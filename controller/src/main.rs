//! sinkhole controller: the orchestrator.
//!
//! A single-instance service that manages a fleet of `filter` instances. It
//! periodically polls each filter for state, logs, ECAM events, records and
//! health, mirrors everything into a central SQLite DB (pruning filter-side
//! data once centralized), and exposes a user and web-UI facing API. All
//! operator edits go through the controller, which relays them to the affected
//! filters; clients never talk to filters directly.
//!
//! Module map: `fetch` (HTTP calls to filters), `sync` (periodic poll loops),
//! `state` (in-memory cache of live filter state), `supervisor` (per-service
//! task lifecycle), `db` (central store and record actor), `api` (control
//! plane).

// These signatures, return types and the supervisor module layout would need a
// structural refactor (param structs, boxed errors, module rename) to satisfy
// the following lints.
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]
#![allow(clippy::result_large_err)]
#![allow(clippy::module_inception)]

use crate::api::routes_controller;
use crate::db::commands::CtrlDbCmd;
use crate::state::ctrl::ControllerState;
use crate::supervisor::supervisor::Supervisor;
use config::env;
use std::sync::Arc;
use supervisor::app_state::AppState;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

mod api;
pub mod config;
mod db;
pub mod fetch;
pub mod state;
pub mod supervisor;
mod sync;

#[tokio::main]
async fn main() {
    // Keep fatal errors graceful: log a clean, backtrace-free message (e.g. a
    // port already in use) instead of dumping an anyhow trace.
    let code = match run().await {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!("{e:#}");
            1
        }
    };

    // Hard-exit rather than returning (which would drop the tokio runtime). The
    // Ctrl-D watcher in wait_for_shutdown parks an *uninterruptible* blocking stdin
    // read; when shutdown is triggered by anything else (Ctrl-C / SIGTERM) that
    // read stays parked, and dropping the runtime would block on it forever. We've
    // already shut down gracefully by here, so exiting the process is correct.
    std::process::exit(code);
}

async fn run() -> anyhow::Result<()> {
    // ── Logging
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,hyper_util=info,reqwest=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    // ── Config + state
    let cfg = env::load()?;
    let db = db::commands::init(&cfg.controller.db_path)?;
    let http = reqwest::Client::new();
    let state = ControllerState::new(cfg.clone(), db.clone(), http.clone())?;

    // ── Supervisor & background tasks
    let supervisor = Arc::new(Supervisor::new(state.clone(), db.clone()));
    supervisor.spawn_all_existing().await;

    // ── Global pruner (filter DB prune + central DB retention)
    let prune_cancel = tokio_util::sync::CancellationToken::new();
    let pruner_handle = crate::sync::pruner::spawn(state.clone(), db.clone(), prune_cancel.clone());

    // ── HTTP server
    let app_state = AppState {
        ctrl: state.clone(),
        supervisor: supervisor.clone(),
    };
    let app = routes_controller::build_router(app_state);

    let bind = cfg.controller.bind;
    let listener = TcpListener::bind(bind).await.map_err(|e| match e.kind() {
        std::io::ErrorKind::AddrInUse => {
            anyhow::anyhow!(
                "address {bind} is already in use; is another controller already running?"
            )
        }
        _ => anyhow::anyhow!("cannot bind controller API to {bind}: {e}"),
    })?;
    tracing::info!("controller API listening on {}", bind);

    // graceful shutdown on Ctrl-C / SIGTERM / Ctrl-D (see wait_for_shutdown)
    let db_for_shutdown = db.clone();
    let shutdown = async move {
        wait_for_shutdown().await;
        tracing::info!("shutdown signal received, shutting down...");
        let _ = db_for_shutdown.tx.send(CtrlDbCmd::Shutdown).await;
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;

    // stop the pruner before exiting
    prune_cancel.cancel();
    let _ = pruner_handle.await;

    Ok(())
}

/// Resolve when the process should shut down: Ctrl-C (SIGINT), SIGTERM (unix), or
/// EOF on stdin (Ctrl-D). Stdin EOF is only watched when stdin is a terminal, so a
/// detached/daemonized process (stdin redirected from /dev/null) is not torn down
/// the instant it starts.
async fn wait_for_shutdown() {
    use std::io::IsTerminal;

    let ctrl_d = async {
        if std::io::stdin().is_terminal() {
            use tokio::io::AsyncReadExt;
            let mut stdin = tokio::io::stdin();
            let mut buf = [0u8; 64];
            loop {
                match stdin.read(&mut buf).await {
                    Ok(0) | Err(_) => break, // EOF (Ctrl-D) or closed
                    Ok(_) => {}              // ignore anything typed before EOF
                }
            }
        } else {
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let term = async {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term => {}
        _ = ctrl_d => {}
    }
}
