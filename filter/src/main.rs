use crate::db::actor::{DbCmd, init_db};
use crate::db::query_hydration::hydrate_registry;
use crate::observability::summary::dump_port_state;
use crate::proxy::run_all_ports;
use anyhow::{Result, anyhow};
use observability::sinks;
use tokio::{net::TcpListener, signal};
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

#[tokio::main]
async fn main() -> Result<()> {
    let log_filter = EnvFilter::new("trace");

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

    if cfg.hydrate_db {
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

    // spawn proxies
    let proxy_cfg = cfg.clone();
    let reg_clone = registry.clone();
    let proxy_handle = tokio::spawn(async move {
        if let Err(e) = run_all_ports(proxy_cfg, reg_clone).await {
            error!(error=?e, "proxy listeners exited with error");
        }
    });

    // control API
    let app = api::routes_filter::build_router(cfg.clone(), registry.clone(), db_handle.clone());
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

    // server
    let server = axum::serve(listener, app).with_graceful_shutdown(shutdown);

    // run server and wait
    info!("running!");

    server.await?;
    info!("control API server stopped");

    // ensure proxy task is done (it will exit when listeners stop accepting)
    let _ = proxy_handle.await;
    info!("filter exited successfully");
    Ok(())
}
