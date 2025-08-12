use std::sync::Arc;
use std::time::Duration;
use axum::Router;
use crate::api::routes_controller;
use tokio::net::TcpListener;
use tokio::signal;
use tracing_subscriber::{fmt, EnvFilter};
use config::env;
use state::ctrl;
use supervisor::app_state::AppState;
use crate::db::commands::CtrlDbCmd;
use crate::state::ctrl::ControllerState;
use crate::supervisor::supervisor::Supervisor;

mod api;
mod db;
mod sync;
pub mod fetch;
pub mod config;
pub mod state;
pub mod supervisor;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── Logging
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("trace,hyper_util=info,reqwest=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    // ── Config + state
    let cfg   = env::load()?;
    let db    = db::commands::init(&cfg.controller.db_path)?;
    let http  = reqwest::Client::new();
    let state = ControllerState::new(cfg.clone(), db.clone(), http.clone())?;

    // ── Supervisor & background tasks
    let supervisor = Arc::new(Supervisor::new(state.clone(), db.clone()));
    supervisor.spawn_all_existing().await;

    // ── HTTP server
    let app_state = AppState { ctrl: state.clone(), supervisor: supervisor.clone() };
    let app = routes_controller::build_router(app_state);

    let bind = cfg.controller.bind;
    let listener = TcpListener::bind(bind).await?;
    tracing::info!("controller API listening on {}", bind);

    // Graceful shutdown
    let db_for_shutdown = db.clone();
    let shutdown = async move {
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
            _ = tokio::signal::ctrl_c() => {},
            _ = term => {},
        }

        tracing::info!("shutdown signal received, shutting down…");
        let _ = db_for_shutdown.tx.send(CtrlDbCmd::Shutdown).await;
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;

    Ok(())
}
