use crate::db::actor::{DbCmd, init_db};
use crate::db::hydration::hydrate_registry;
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
mod tls;

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

    // Build TLS state if enabled
    let tls_state: Option<std::sync::Arc<proxy::TlsState>> = if cfg.tls_enabled {
        let cert = cfg.tls_cert_path.as_deref().expect("TLS_CERT_PATH validated");
        let key = cfg.tls_key_path.as_deref().expect("TLS_KEY_PATH validated");
        let acceptor = tls::build_acceptor(cert, key)?;
        let connector = if cfg.tls_upstream {
            Some(tls::build_connector(cfg.tls_skip_verify)?)
        } else {
            None
        };
        info!("TLS enabled: accepting client TLS, upstream_tls={}, skip_verify={}", cfg.tls_upstream, cfg.tls_skip_verify);
        Some(std::sync::Arc::new(proxy::TlsState {
            acceptor,
            upstream_connector: connector,
            upstream_host: cfg.upstream_host.clone(),
        }))
    } else {
        None
    };

    // spawn proxies
    let proxy_cfg = cfg.clone();
    let reg_clone = registry.clone();
    let proxy_handle = tokio::spawn(async move {
        if let Err(e) = run_all_ports(proxy_cfg, reg_clone, tls_state).await {
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
