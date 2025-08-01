use anyhow::anyhow;
use tokio::{net::TcpListener, signal};
use tracing_subscriber::{fmt, EnvFilter};

mod config;
mod state;
mod pull;
mod api;
mod db;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    fmt().with_env_filter(EnvFilter::from_default_env()).init();

    let cfg = config::load()?;
    tracing::info!(services = cfg.services.len(), "configuration loaded");

    let controller_state = state::ControllerState::new(cfg.clone())?;

    pull::spawn_all(&cfg, controller_state.clone());

    // Build router via api module
    let app = api::build_router(controller_state);

    // Bind address from env or default
    let bind = std::env::var("CONTROLLER_BIND")
        .unwrap_or_else(|_| "0.0.0.0:7000".to_string());

    let listener = TcpListener::bind(&bind)
        .await
        .map_err(|e| anyhow!("unable to bind controller API on {bind}: '{e}'"))?;

    tracing::info!("controller API listening on {}", bind);

    // Graceful shutdown
    let shutdown = async {
        signal::ctrl_c().await.expect("install ctrl+c handler");
        tracing::info!("CTRL+C received, shutting down…");
        // TODO: flush DB, signal pull tasks to stop, etc.
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;

    Ok(())
}
