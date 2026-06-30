//! sinkhole filter: the data plane.
//!
//! A transparent TCP proxy that sits in front of a guarded service. It accepts
//! client connections, forwards them to the upstream, and on every parsed
//! message runs the compiled rule engine (markers then filters) to reach an
//! Allow or Drop decision. Decisions and diagnostics are logged to a local
//! SQLite DB and exposed, together with live port state, through a small
//! control API that the controller polls and edits. One filter instance guards
//! one host.
//!
//! Module map: `proxy` (accept and forward loop), `http` (incremental parser),
//! `engine` (per-message evaluation), `matcher` and `rules` (rule model and
//! compile), `registry` (live per-port runtime), `db` (record actor),
//! `observability` (telemetry and ECAM sinks), `api` (control plane).

// Widening these function signatures and boxing these variants is a structural
// refactor of the proxy hot path and DB command API, not a code cleanup.
#![allow(clippy::too_many_arguments)]
#![allow(clippy::large_enum_variant)]

use crate::db::commands::{FilterDbCmd, init_db};
use crate::db::query_hydration::hydrate_registry;
use crate::observability::summary::dump_port_state;
use crate::proxy::{bind_all_ports, run_all_ports};
use anyhow::{Result, anyhow};
use observability::sinks;
use tokio::net::TcpListener;
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, fmt};

mod api;
mod config;
mod db;
mod engine;
mod http;
mod matcher;
mod observability;
mod proxy;
mod registry;
mod rules;
mod tls;

#[tokio::main]
async fn main() {
    // Keep fatal errors graceful: log a clean, backtrace-free message (e.g. a
    // port already in use) instead of dumping an anyhow trace.
    let code = match run().await {
        Ok(()) => 0,
        Err(e) => {
            error!("{e:#}");
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

async fn run() -> Result<()> {
    let log_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    fmt().with_env_filter(log_filter).with_target(false).init();

    info!("starting sinkhole filter...");

    // Config (from env)
    let cfg = config::load()?;
    info!("effective env config: {}", serde_json::to_string(&cfg)?);

    let registry = registry::build_registry(&cfg)?;
    info!(
        "registry built, port mappings: [{}]",
        cfg.ports
            .iter()
            .map(|b| b.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );

    // Database
    let db_handle = init_db(&cfg.db_path)?;
    let db_for_shutdown = db_handle.clone();
    info!("db connection established");

    if cfg.db_hydrate {
        info!("hydrating from db...");
        hydrate_registry(&cfg.db_path, &registry)?;
        for entry in registry.iter() {
            dump_port_state(entry.value());
        }
    } else {
        info!("DB hydration is disabled");
    }

    // Init sinks
    sinks::init_all(&cfg, db_handle.clone());

    // Bind all public proxy ports up front, so a port clash fails fast with one
    // clean error (graceful exit) rather than a half-up filter proxying nothing.
    let bound = bind_all_ports(&cfg).await?;

    // spawn proxies (serving the pre-bound listeners)
    let proxy_cfg = cfg.clone();
    let reg_clone = registry.clone();
    let conn_alloc = std::sync::Arc::new(crate::proxy::ConnIdAllocator::new(db_handle.clone()));
    let proxy_cancel = tokio_util::sync::CancellationToken::new();
    let proxy_cancel_for_task = proxy_cancel.clone();
    let proxy_handle = tokio::spawn(async move {
        if let Err(e) = run_all_ports(
            proxy_cfg,
            reg_clone,
            proxy_cancel_for_task,
            conn_alloc,
            bound,
        )
        .await
        {
            error!(error=?e, "proxy listeners exited with error");
        }
    });

    // control API
    let app = api::routes_filter::build_router(cfg.clone(), registry.clone(), db_handle.clone());
    let bind = cfg.control_bind_addr();
    let listener = TcpListener::bind(bind).await.map_err(|e| match e.kind() {
        std::io::ErrorKind::AddrInUse => {
            anyhow!(
                "control API address {bind} is already in use; is another filter already running?"
            )
        }
        _ => anyhow!("unable to bind control API on address {bind}: {e}"),
    })?;
    info!("control API listening on {}", bind);

    // graceful shutdown on Ctrl-C / SIGTERM / Ctrl-D (see wait_for_shutdown)
    let shutdown = async move {
        wait_for_shutdown().await;
        info!("shutdown signal received, shutting down...");
        let _ = db_for_shutdown.tx.send(FilterDbCmd::Shutdown).await;
    };

    // server
    let server = axum::serve(listener, app).with_graceful_shutdown(shutdown);

    // run server and wait
    info!("running!");

    server.await?;
    info!("control API server stopped");

    // cancel proxy listeners and wait for them to drain
    proxy_cancel.cancel();
    let _ = proxy_handle.await;
    info!("filter exited successfully");
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
