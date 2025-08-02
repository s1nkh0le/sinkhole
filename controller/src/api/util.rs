use anyhow::Result;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use crate::state::{ControllerState, Service};
use reqwest::Client;
use serde_json::json;

#[derive(Deserialize)]
struct FilterPortsEnvelope {
    status: String,
    data: Vec<FilterPortRow>,
}
#[derive(Deserialize)]
struct FilterPortRow {
    public: u16,
    upstream: u16,
}

/// If the service has no ports recorded, fetch from filter `/ports` and populate `state`.
pub async fn ensure_ports_loaded(
    state: &ControllerState,
    svc_id: &str,
    http: &Client,
) -> Result<()> {
    // fast path: already known
    if state.read().services.get(svc_id).map_or(true, |svc| !svc.ports.is_empty()) {
        return Ok(());
    }

    let (base, token) = {
        let g = state.read();
        let svc = g.services.get(svc_id).expect("checked earlier");
        (svc.cfg.base_url.clone(), svc.cfg.token.clone())
    };

    let resp = http
        .get(format!("{base}/ports"))
        .query(&[("token", token.as_str())])
        .send()
        .await?
        .json::<FilterPortsEnvelope>()
        .await?;

    if resp.status != "ok" {
        anyhow::bail!("filter /ports returned non-ok");
    }

    // Insert any new ports
    let mut w = state.write();
    let svc = w.services.get_mut(svc_id).expect("present");
    for row in resp.data {
        svc.ports.entry(row.public).or_insert_with(|| crate::state::Port::new());
    }
    Ok(())
}


pub fn ensure_service_exists(
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

pub fn ensure_service_port_exists(
    state: &ControllerState,
    svc_id: &str,
    port: u16,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let guard = state.read();
    match guard.services.get(svc_id).and_then(|s: &Service| s.ports.get(&port)) {
        Some(_) => Ok(()),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"status":"error","errors":["unknown service/port"]})),
        )),
    }
}