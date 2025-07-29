
use anyhow::{anyhow, Result};
use tokio::{net::TcpListener, signal};
use tracing::{error, info};
use tracing_subscriber::{fmt, EnvFilter};

use crate::{
    proxy::run_all_ports,
};
use crate::db::actor::{init_db, DbCmd};
use crate::db::hydration::hydrate_registry;
use crate::observability::summary::dump_port_state;

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
mod db;

#[tokio::main]
async fn main() -> Result<()> {
    let log_filter = EnvFilter::new("trace");

    fmt()
        .with_env_filter(log_filter)
        .with_target(false)
        .init();

    info!("starting tfilter...");

    let cfg = config::load()?;
    info!(?cfg, "loaded environment configuration");

    let registry = registry::build_registry(&cfg)?;
    info!("registry built (ports: {})", cfg.ports.len());



    let db_path = std::env::var("TFILTER_DB_PATH").unwrap_or_else(|_| "tfilter.db".into());
    let db_handle = init_db(&db_path)?;
    let db_for_shutdown = db_handle.clone();
    info!(?cfg, "db connection initialized");

    observability::telemetry::init_db_sink(db_handle.clone());


    if cfg.hydrate_db {
        info!("hydrating from db");
        hydrate_registry(&db_path, &registry)?;
        for entry in registry.iter() {
            dump_port_state(entry.value());
        }
    } else {
        info!("DB hydration disabled by TFILTER_HYDRATE_DB=false");
    }



    // spawn proxies
    let proxy_cfg = cfg.clone();
    let reg_clone = registry.clone();
    let proxy_handle = tokio::spawn(async move {
        if let Err(e) = run_all_ports(proxy_cfg, reg_clone).await {
            error!(error=?e, "proxy listeners exited with error");
        }
    });

    // control API
    let app = api::routes::build_router(cfg.clone(), registry.clone(), db_handle.clone());
    let bind = cfg.ctrl_bind_addr();
    let listener = TcpListener::bind(bind)
        .await
        .map_err(|e| anyhow!("unable to bind control API on address {bind}: '{e}'"))?;
    info!("control API listening on {}", bind);



    // graceful shutdown
    let shutdown = async move {
        signal::ctrl_c().await.expect("install ctrl+c handler");
        info!("CTRL+C received, shutting down…");
        let _ = db_for_shutdown.tx.send(DbCmd::Shutdown).await;
    };

    // graceful shutdown
    let server = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown);

    // run server and wait
    info!("running!");

    server.await?;
    info!("control API server stopped");

    // ensure proxy task is done (it will exit when listeners stop accepting)
    let _ = proxy_handle.await;
    info!("tfilter exited successfully");
    Ok(())
}
