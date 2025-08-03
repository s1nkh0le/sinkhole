use crate::api::routes;
use tokio::net::TcpListener;
use tracing_subscriber::{fmt, EnvFilter};

mod api;
mod config;
mod db;
mod observability;
mod puller;
mod state;
mod sync;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let log_filter = EnvFilter::new("trace,hyper_util=info,reqwest=info");
    fmt().with_env_filter(log_filter).init();

    let cfg = config::load()?;
    let db = db::init(&cfg.controller.db_path)?;
    let state = state::ControllerState::new(cfg.clone(), db.clone())?;

    sync::spawn_all_state_sync(state.clone(), reqwest::Client::new(), cfg.clone());
    puller::spawn_all_pull_tasks(state.clone(), reqwest::Client::new(), db.clone());

    let app = routes::build_router(state.clone());

    let bind = std::env::var("CONTROLLER_BIND").unwrap_or_else(|_| "0.0.0.0:7000".into());
    let listener = TcpListener::bind(&bind).await?;
    tracing::info!("controller API listening on {bind}");

    let db_for_shutdown = db.clone();
    let shutdown = async move {
        // SIGTERM on Unix
        #[cfg(unix)]
        let term = async {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigterm = signal(SignalKind::terminate()).expect("install SIGTERM handler");
            sigterm.recv().await;
        };

        #[cfg(not(unix))]
        let term = std::future::pending::<()>();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
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
