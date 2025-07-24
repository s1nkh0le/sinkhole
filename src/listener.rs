// src/listener.rs
use crate::{config::Config, proxy::run_proxy_for_port};
use anyhow::Result;
use tokio::task;

pub async fn run_all_ports(cfg: Config) -> Result<()> {
    for pb in cfg.ports.clone() {
        let cfg_cloned = cfg.clone();
        task::spawn(async move {
            if let Err(e) = run_proxy_for_port(cfg_cloned, pb.clone()).await {
                tracing::error!(public_port = pb.public, error=?e, "proxy failed");
            }
        });
    }
    Ok(())
}
