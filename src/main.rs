use anyhow::Result;
use tokio::net::TcpListener;
use tracing_subscriber::{fmt, EnvFilter};
use crate::listener::run_all_ports;

mod types;
mod utils;
mod config;
mod api;
mod listener;
mod proxy;
mod observability;
mod http;
mod rules;
mod registry;
mod matcher;
mod engine;

#[tokio::main]
async fn main() -> Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();


    let cfg = config::load()?;
    let registry = registry::build_registry(&cfg)?;

    let proxy_cfg = cfg.clone();
    let reg_clone = registry.clone();
    tokio::spawn(async move {
        if let Err(e) = run_all_ports(proxy_cfg, reg_clone).await {
            tracing::error!(error=?e, "proxy listeners exited with error");
        }
    });

    // control API
    let app = api::routes::build_router(cfg.clone(), registry.clone());
    let bind = cfg.ctrl_bind_addr();
    let listener = TcpListener::bind(bind).await?;
    axum::serve(listener, app).await?;


    Ok(())
}
