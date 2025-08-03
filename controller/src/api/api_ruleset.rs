// ──────────────────────────────────────────────────────────────
// Ruleset (proxy GET/PUT /ports/{port}/ruleset on filter)
// ──────────────────────────────────────────────────────────────

use crate::api::util::ensure_service_port_exists;
use crate::state::ControllerState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use reqwest::Client;
use serde_json::json;
use shared::rules_model::RuleSetRaw;

pub async fn get_ruleset(
    Path((svc, port)): Path<(String, u16)>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(resp) = ensure_service_port_exists(&state, &svc, port) {
        return resp;
    }

    if let Some(rs) = state
        .read()
        .services
        .get(&svc)
        .and_then(|s| s.ports.get(&port))
        .and_then(|p| p.ruleset.clone())
    {
        return (StatusCode::OK, Json(json!({"status":"ok","data": rs})));
    }

    let (base, token) = {
        let g = state.read();
        let s = g.services.get(&svc).unwrap();
        (s.cfg.base_url.clone(), s.cfg.token.clone())
    };

    let url = format!("{base}/ports/{port}/ruleset");
    let resp = Client::new()
        .get(&url)
        .query(&[("token", token.as_str())])
        .send()
        .await;

    let Ok(resp) = resp else {
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({"status":"error","errors":["filter unreachable"]})),
        );
    };

    if !resp.status().is_success() {
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({"status":"error","errors":[format!("filter returned {}", resp.status())]})),
        );
    }

    let env = resp
        .json::<serde_json::Value>()
        .await
        .unwrap_or(json!({"status":"error"}));
    if env.get("status") != Some(&json!("ok")) {
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({"status":"error","errors":["filter returned non-ok"]})),
        );
    }

    let rs: RuleSetRaw = serde_json::from_value(env.get("data").cloned().unwrap_or(json!({})))
        .map_err(|e| e.to_string())
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"status":"error","errors":[e]})),
            )
        })
        .unwrap();

    state
        .write()
        .services
        .get_mut(&svc)
        .and_then(|s| s.ports.get_mut(&port))
        .map(|p| p.ruleset = Some(rs.clone()));

    (StatusCode::OK, Json(json!({"status":"ok","data": rs})))
}

pub async fn put_ruleset(
    Path((svc, port)): Path<(String, u16)>,
    State(state): State<ControllerState>,
    Json(body): Json<RuleSetRaw>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(resp) = ensure_service_port_exists(&state, &svc, port) {
        return resp;
    }

    let (base, token) = {
        let g = state.read();
        let s = g.services.get(&svc).unwrap();
        (s.cfg.base_url.clone(), s.cfg.token.clone())
    };

    let url = format!("{base}/ports/{port}/ruleset");
    let resp = Client::new()
        .put(&url)
        .query(&[("token", token.as_str())])
        .json(&body)
        .send()
        .await;

    let Ok(resp) = resp else {
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({"status":"error","errors":["filter unreachable"]})),
        );
    };

    // Bubble up filter validation errors
    let payload = resp
        .json::<serde_json::Value>()
        .await
        .unwrap_or(json!({"status":"error"}));
    if payload.get("status") != Some(&json!("ok")) {
        return (StatusCode::BAD_REQUEST, Json(payload));
    }

    // cache on success
    state
        .write()
        .services
        .get_mut(&svc)
        .and_then(|s| s.ports.get_mut(&port))
        .map(|p| p.ruleset = Some(body));

    (StatusCode::OK, Json(payload))
}
