
use anyhow::{anyhow, Result};
use tokio::{net::TcpListener, signal};
use tracing::{debug, error, info};
use tracing_subscriber::{fmt, EnvFilter};

use crate::{
    proxy::run_all_ports,
};

mod types;
mod utils;
mod config;
mod api;
mod proxy;
mod observability;
mod http;
mod rules;
mod registry;
mod matcher;
mod engine;

#[tokio::main]
async fn main() -> Result<()> {
    let log_filter = EnvFilter::new("trace");

    fmt()
        .with_env_filter(log_filter)
        .with_target(false)
        .init();

    info!("starting tfilter...");

    let cfg = config::load()?;
    info!(?cfg, "loaded configuration");

    let registry = registry::build_registry(&cfg)?;
    info!("registry built (ports: {})", cfg.ports.len());

    // spawn proxies
    let proxy_cfg = cfg.clone();
    let reg_clone = registry.clone();
    let proxy_handle = tokio::spawn(async move {
        if let Err(e) = run_all_ports(proxy_cfg, reg_clone).await {
            error!(error=?e, "proxy listeners exited with error");
        }
    });

    // control API
    let app = api::routes::build_router(cfg.clone(), registry.clone());
    let bind = cfg.ctrl_bind_addr();
    let listener = TcpListener::bind(bind)
        .await
        .map_err(|e| anyhow!("unable to bind control API on address {bind}: '{e}'"))?;
    info!("control API listening on {}", bind);

    // graceful shutdown
    let server = axum::serve(listener, app)
        .with_graceful_shutdown(async {
            signal::ctrl_c().await.expect("install ctrl+c handler");
            info!("CTRL+C received, shutting down…");
        });

    // run server and wait
    server.await?;
    info!("control API server stopped");

    // ensure proxy task is done (it will exit when listeners stop accepting)
    let _ = proxy_handle.await;

    info!("tfilter exited successfully");
    Ok(())
}
