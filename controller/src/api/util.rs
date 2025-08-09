use anyhow::Result;

use crate::fetch::fetch_state::fetch_state;
use crate::state::ctrl::ControllerState;
use reqwest::Client;

pub async fn ensure_service_or_fetch_filter_state(
    state: &ControllerState,
    service_id: &str,
    http:      &Client,
) -> Result<()> {
    // Fast-path: already cached
    if state.read().services
        .get(service_id)
        .map(|s| !s.ports.is_empty())
        .unwrap_or(false)
    {
        return Ok(());
    }

    // One-shot pull from filter ----------------------------------------
    let (base_url, token) = {
        let g = state.read();
        let svc = g.services.get(service_id)
            .ok_or_else(|| anyhow::anyhow!("unknown service"))?;
        (svc.cfg.base_url.clone(), svc.cfg.token.clone())
    };

    let snaps = fetch_state(http, &base_url, &token, std::time::Duration::from_secs(5)).await?;

    // Merge into state
    {
        let mut w = state.write();
        let svc = w.services.get_mut(service_id)
            .ok_or_else(|| anyhow::anyhow!("unknown service"))?;

        let mut dummy = std::collections::HashMap::new();
        svc.merge_snapshots(snaps, &mut dummy, service_id);
    }

    Ok(())
}

pub fn ensure_port_exists(
    state: &ControllerState,
    service_id: &str,
    port: u16,
) -> bool {
    state.read()
        .services
        .get(service_id)
        .and_then(|s| s.ports.get(&port))
        .map(|p| p.snap.is_some())
        .unwrap_or(false)
}

pub fn ensure_service_exists(state: &ControllerState, service_id: &str) -> bool {
    state.read().services.contains_key(service_id)
}