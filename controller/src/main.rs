use std::time::Duration;
use axum::Router;
use crate::api::routes_controller;
use tokio::net::TcpListener;
use tokio::signal;
use tracing_subscriber::{fmt, EnvFilter};

mod api;
mod config;
mod db;
mod state;
mod sync;
pub mod fetch;



#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── Logging ───────────────────────────────────────────────────────────
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,hyper_util=info,reqwest=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    // ── Config + state ────────────────────────────────────────────────────
    let cfg = config::load()?;
    let db = db::init(&cfg.controller.db_path)?;
    let state = state::ControllerState::new(cfg.clone(), db.clone())?;

    // One shared client (cheap to clone)
    let http = reqwest::Client::new();

    // ── Background tasks ──────────────────────────────────────────────────
    sync::sync_health::spawn(state.clone(), http.clone());
    sync::sync_state::spawn(state.clone(), http.clone(), Duration::from_secs(cfg.controller.sync_interval_secs));
    sync::sync_dbs::spawn(state.clone(), http.clone(), db.clone());

    // ── HTTP server ───────────────────────────────────────────────────────
    let app: Router = routes_controller::build_router(state.clone());

    // IMPORTANT: bind from parsed config, not raw env vars
    let bind = cfg.controller.bind;
    let listener = TcpListener::bind(bind).await?;
    tracing::info!("controller API listening on {}", bind);

    // Graceful shutdown hook
    let db_for_shutdown = db.clone();
    let shutdown = async move {
        // Unix: SIGTERM support; non-unix: just ctrl-c
        #[cfg(unix)]
        let term = async {
            use tokio::signal::unix::{signal, SignalKind};
            if let Ok(mut sigterm) = signal(SignalKind::terminate()) {
                sigterm.recv().await;
            }
        };
        #[cfg(not(unix))]
        let term = std::future::pending::<()>();

        tokio::select! {
            _ = signal::ctrl_c() => {},
            _ = term => {},
        }

        tracing::info!("shutdown signal received, shutting down…");
        let _ = db_for_shutdown.tx.send(db::Cmd::Shutdown).await;
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;

    Ok(())
}
