use crate::{config::Config, proxy::run_proxy_for_port, registry::PortRegistry};
use anyhow::Result;
use tokio::task;

/// Spawn a proxy task for every entry in LISTEN_MAP.
pub async fn run_all_ports(cfg: Config, registry: PortRegistry) -> Result<()> {
    for pb in cfg.ports.clone() {
        let cfg_cloned = cfg.clone();
        let registry_cloned = registry.clone();
        let pb_cloned = pb.clone();
        let public_port = pb.public;       // <- cache before move

        task::spawn(async move {
            if let Err(e) =
                run_proxy_for_port(cfg_cloned, registry_cloned, pb_cloned).await
            {
                tracing::error!(
                    error=?e,
                    "proxy for port {} failed",
                    public_port
                );
            }
        });
    }
    Ok(())
}
