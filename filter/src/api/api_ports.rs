use crate::api::middleware_auth;
use crate::api::routes_filter::SharedState;
use crate::db::actor::DbCmd;
use crate::observability::sinks::sink_ecam;
use crate::observability::sinks::sink_ecam::{Origin, Severity};
use axum::Json;
use axum::extract::{OriginalUri, Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use shared::rules_model::PortInfo;
use shared::types::Scope;
use shared::utils::time::now_ns;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct ConfigUpdate {
    name: Option<String>,
    comment: Option<String>,
    hex_color: Option<String>,
    enabled_scopes: Option<Vec<Scope>>,
}

#[utoipa::path(
    get,
    path = "/ports",
    params(
        ("token" = String, description = "Authentication token"),
    ),
    responses(
        (status = 200, description = "OK"),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "filter"
)]
pub async fn get_port_list(
    OriginalUri(_uri): OriginalUri,
    State((_cfg, registry, _db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
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
            version: compiled.version.clone(),
        });
    }

    (StatusCode::OK, Json(json!({"status":"ok", "data":list})))
}


#[utoipa::path(
    get,
    path = "/ports/{port}/config",
    params(
        ("token" = String, description = "Authentication token"),
        ("port" = u16, Path, description = "Public port"),
    ),
    responses(
        (status = 200, description = "OK"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Unknown port"),
    ),
    tag = "filter"
)]
pub async fn get_config(
    OriginalUri(uri): OriginalUri,
    Path(port): Path<u16>,
    State((_cfg, registry, _db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match registry.get(&port) {
        Some(pr) => {
            let name = pr.name.load().as_deref().map(ToOwned::to_owned);
            let comment = pr.comment.load().as_deref().map(ToOwned::to_owned);
            let hex_color = pr.hex_color.load().as_deref().map(ToOwned::to_owned);
            let scopes = pr
                .enabled_scopes
                .read()
                .unwrap()
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            let compiled = pr.compiled.load();

            let info = PortInfo {
                public: pr.binding.public,
                upstream: pr.binding.upstream,
                name,
                comment,
                hex_color,
                enabled_scopes: scopes,
                version: compiled.version.clone(),
            };

            (StatusCode::OK, Json(json!({"status":"ok","data":info})))
        }
        None => {
            sink_ecam::record(
                Severity::Warn,
                Origin::API,
                format!("[get_config(), GET {}]: port not found", uri.path()),
                None,
            );

            (
                StatusCode::NOT_FOUND,
                Json(json!({"status":"error","errors":["unknown port"]})),
            )
        }
    }
}



#[utoipa::path(
    put,
    path = "/ports/{port}/config",
    params(
        ("token" = String, description = "Authentication token"),
        ("port" = u16, Path, description = "Public port"),
    ),
    responses(
        (status = 200, description = "OK"),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Unknown port"),
    ),
    tag = "filter"
)]
pub async fn put_config(
    OriginalUri(uri): OriginalUri,
    Path(port): Path<u16>,
    State((_cfg, registry, db)): State<SharedState>,
    Json(body): Json<ConfigUpdate>,
) -> (StatusCode, Json<serde_json::Value>) {
    // ── port lookup ─────────────────────────────────────
    let pr = match registry.get(&port) {
        Some(p) => p,
        None => {
            sink_ecam::record(Severity::Warn, Origin::API, format!("[put_config(), PUT {}]: rejecting config | port not found", uri.path()), None, );
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"status":"error","errors":["unknown port"]})),
            );
        }
    };

    // ── validations (only for provided fields) ──────────
    let mut errs: Vec<String> = Vec::new();

    if let Some(ref scopes) = body.enabled_scopes {
        if scopes.is_empty() {
            errs.push("enabled_scopes cannot be empty".to_string());
        }
    }

    if let Some(ref hex) = body.hex_color {
        if !is_valid_hex_color(hex) {
            errs.push("hex_color must be in the form #RRGGBB".to_string());
        }
    }

    if !errs.is_empty() {
        sink_ecam::record(Severity::Warn, Origin::API, format!("[put_config(), PUT {}]: validation failed: {}", uri.path(), errs.join(", ")), None);
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"status":"error","errors": errs})),
        );
    }

    // ── apply updates (only provided fields) ─────────────
    if let Some(ref n) = body.name {
        pr.name.store(Some(Arc::new(n.clone())));
    }
    if let Some(ref c) = body.comment {
        pr.comment.store(Some(Arc::new(c.clone())));
    }
    if let Some(ref hc) = body.hex_color {
        pr.hex_color.store(Some(Arc::new(hc.clone())));
    }
    if let Some(ref scopes) = body.enabled_scopes {
        let mut guard = pr.enabled_scopes.write().unwrap();
        guard.clear();
        guard.extend(scopes.iter().cloned());
        info!(port, scopes=?scopes, "enabled scopes updated");
    }

    // ── build *effective* snapshot (post-update) ────────
    let name = pr.name.load().as_deref().map(ToOwned::to_owned);
    let comment = pr.comment.load().as_deref().map(ToOwned::to_owned);
    let hex_color = pr.hex_color.load().as_deref().map(ToOwned::to_owned);
    let scopes = pr
        .enabled_scopes
        .read()
        .unwrap()
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    let compiled = pr.compiled.load();

    let info = PortInfo {
        public: pr.binding.public,
        upstream: pr.binding.upstream,
        name,
        comment,
        hex_color,
        enabled_scopes: scopes.clone(),
        version: compiled.version.clone(),
    };

    // ── persist effective config to DB ──────────────────
    let cfg_json = json!({
        "name": info.name,
        "comment": info.comment,
        "hex_color": info.hex_color,
        "enabled_scopes": info.enabled_scopes,
    })
    .to_string();

    let _ = db.tx.try_send(DbCmd::SaveConfig {
        port,
        json: cfg_json,
        ts_ns: now_ns() as i64,
    });

    info!("[put_config(), PUT {}]: configuration updated", uri.path());

    // ── respond with typed snapshot ──────────────────────
    (StatusCode::OK, Json(json!({"status":"ok","data": info})))
}

/// strict #RRGGBB (6 hex digits) validator
fn is_valid_hex_color(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 7 || b[0] != b'#' {
        return false;
    }
    b[1..].iter().all(|c| c.is_ascii_hexdigit())
}
