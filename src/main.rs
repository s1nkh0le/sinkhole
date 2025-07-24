use anyhow::Result;
use tokio::net::TcpListener;
use tracing_subscriber::{fmt, EnvFilter};

mod types;
mod utils;
mod config;
mod api;
mod listener;
mod proxy;
mod observability;
mod http;

#[tokio::main]
async fn main() -> Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let cfg = config::load()?;
    tracing::info!(?cfg, "loaded configuration");

    // spawn proxies
    let proxy_cfg = cfg.clone();
    tokio::spawn(async move {
        if let Err(e) = listener::run_all_ports(proxy_cfg).await {
            tracing::error!(error=?e, "proxy listeners exited with error");
        }
    });

    // control API
    let app = api::routes::build_router(cfg.clone());
    let bind = cfg.ctrl_bind_addr();
    let listener = TcpListener::bind(bind).await?;
    tracing::info!("control API listening on {}", bind);
    axum::serve(listener, app).await?;

    Ok(())
}
