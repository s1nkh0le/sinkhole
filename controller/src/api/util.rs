use anyhow::Result;
use axum::http::StatusCode;
use axum::Json;

use crate::fetch::fetch_state::fetch_state;
use crate::state::{ControllerState, Service};
use reqwest::Client;
use serde_json::json;

pub async fn ensure_snapshots(
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










pub fn __legacy_ensure_service_exists(
    state: &ControllerState,
    svc_id: &str,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if state.read().services.get(svc_id).is_some() {
        Ok(())
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(json!({"status":"error","errors":["unknown service"]})),
        ))
    }
}


pub fn __legacy_ensure_service_port_exists(
    state: &ControllerState,
    svc_id: &str,
    port: u16,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let guard = state.read();
    match guard
        .services
        .get(svc_id)
        .and_then(|s: &Service| s.ports.get(&port))
    {
        Some(_) => Ok(()),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"status":"error","errors":["unknown service/port"]})),
        )),
    }
}