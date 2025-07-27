use std::collections::HashMap;
use std::sync::Arc;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::info;
use crate::api::auth;
use crate::api::routes::SharedState;
use crate::db::DbCmd;
use crate::types::Scope;

#[derive(Deserialize)]
pub struct ConfigUpdate {
    name: Option<String>,
    comment: Option<String>,
    hex_color: Option<String>,
    enabled_scopes: Option<Vec<Scope>>,
}

#[derive(Serialize)]
pub struct PortInfo {
    public: u16,
    upstream: u16,
    name: Option<String>,
    comment: Option<String>,
    hex_color: Option<String>,
    enabled_scopes: Vec<Scope>,
    version: Option<String>,
}



// ---------------------------------------------------------
// GET /ports  (list)
// ---------------------------------------------------------
pub async fn list_ports(
    Query(params): Query<HashMap<String, String>>,
    State((cfg, registry, db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(e) = auth::check_token(params.get("token").map(String::as_str), &cfg) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"status":"error","errors":[e.to_string()]})),
        );
    }

    let mut list = Vec::new();
    for entry in registry.iter() {
        let pr = entry.value(); // Arc<PortRuntime>
        let name_opt = pr.name.load().as_deref().map(ToOwned::to_owned);
        let comment_opt = pr.comment.load().as_deref().map(ToOwned::to_owned);
        let hex_color = pr.hex_color.load().as_deref().map(ToOwned::to_owned);
        let scopes_vec = pr
            .enabled_scopes
            .read()
            .unwrap()
            .iter()
            .cloned()
            .collect::<Vec<_>>();

        let compiled = pr.compiled.load();

        list.push(PortInfo {
            public: pr.binding.public,
            upstream: pr.binding.upstream,
            name: name_opt,
            comment: comment_opt,
            hex_color: hex_color,
            enabled_scopes: scopes_vec,
            version: Some(compiled.version.clone()),
        });
    }

    (StatusCode::OK, Json(json!({"status":"ok","data":list})))
}

// ---------------------------------------------------------
// GET /ports/{port}/config
// ---------------------------------------------------------
pub async fn get_config(
    Path(port): Path<u16>,
    Query(params): Query<HashMap<String, String>>,
    State((cfg, registry, _db)): State<SharedState>,   // _db unused here
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(e) = auth::check_token(params.get("token").map(String::as_str), &cfg) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"status":"error","errors":[e.to_string()]})),
        );
    }

    match registry.get(&port) {
        Some(pr) => {
            let name      = pr.name.load().as_deref().map(ToOwned::to_owned);
            let comment   = pr.comment.load().as_deref().map(ToOwned::to_owned);
            let hex_color = pr.hex_color.load().as_deref().map(ToOwned::to_owned);
            let scopes    = pr.enabled_scopes.read().unwrap().iter().cloned().collect::<Vec<_>>();
            let compiled  = pr.compiled.load();

            let info = PortInfo {
                public: pr.binding.public,
                upstream: pr.binding.upstream,
                name,
                comment,
                hex_color,
                enabled_scopes: scopes,
                version: Some(compiled.version.clone()),
            };

            (StatusCode::OK, Json(json!({"status":"ok","data":info})))
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"status":"error","errors":["unknown port"]})),
        ),
    }
}

// ---------------------------------------------------------
// PUT /ports/{port}/config
// ---------------------------------------------------------
pub async fn put_config(
    Path(port): Path<u16>,
    Query(params): Query<HashMap<String, String>>,
    State((cfg, registry, db)): State<SharedState>,
    Json(body): Json<ConfigUpdate>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(e) = auth::check_token(params.get("token").map(String::as_str), &cfg) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"status":"error","errors":[e.to_string()]})),
        );
    }

    let pr = match registry.get(&port) {
        Some(p) => p,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"status":"error","errors":["unknown port"]})),
            );
        }
    };

    // --- enabled_scopes validation ---------------------------------------
    if let Some(ref scopes) = body.enabled_scopes {
        if scopes.is_empty() {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"status":"error","errors":["enabled_scopes cannot be empty"]})),
            );
        }
        let mut guard = pr.enabled_scopes.write().unwrap();
        guard.clear();
        guard.extend(scopes.iter().cloned());
        info!(port, scopes=?scopes, "enabled scopes updated");
    }

    // --- name/comment -----------------------------------------------------
    if let Some(ref n) = body.name {
        pr.name.store(Some(Arc::new(n.clone())));
    }
    if let Some(ref c) = body.comment {
        pr.comment.store(Some(Arc::new(c.clone())));
    }

    let cfg_json = json!({
        "name": body.name,
        "comment": body.comment,
        "hex_color": body.hex_color,
        "enabled_scopes": body.enabled_scopes
    }) .to_string();

    let _ = db.tx.try_send(DbCmd::SaveConfig {
        port,
        json: cfg_json,
        ts_ns: crate::utils::now_ns() as i64,
    });

    info!(port, "port config updated");

    (StatusCode::OK, Json(json!({"status":"ok"})))
}