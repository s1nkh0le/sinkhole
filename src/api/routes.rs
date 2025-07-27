use std::{collections::HashMap, sync::Arc};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, error, info};

use crate::db::{DbCmd, DbHandle};
use crate::types::Scope;
use crate::{
    api::auth,
    config::Config,
    registry::PortRegistry,
    rules::{self, compile::CompiledRuleSet},
};

#[derive(Deserialize)]
struct ConfigUpdate {
    name: Option<String>,
    comment: Option<String>,
    hex_color: Option<String>,
    enabled_scopes: Option<Vec<Scope>>,
}

#[derive(Serialize)]
struct PortInfo {
    public: u16,
    upstream: u16,
    name: Option<String>,
    comment: Option<String>,
    hex_color: Option<String>,
    enabled_scopes: Vec<Scope>,
    version: Option<String>,
}

type SharedState = (Config, PortRegistry, DbHandle);

pub fn build_router(cfg: Config, registry: PortRegistry, db_handle: DbHandle) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/version", get(version))
        .route("/ports", get(list_ports))
        .route("/ports/{port}/config", get(get_config).put(put_config))
        .route("/ports/{port}/stats", get(get_stats))
        .route("/ports/{port}/ruleset", get(get_ruleset).put(put_ruleset))
        .route("/ports/{port}/logs", get(get_logs))
        .with_state((cfg, registry, db_handle))
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

// ---------------------------------------------------------
// GET /ruleset
// ---------------------------------------------------------
pub async fn get_ruleset(
    Path(port): Path<u16>,
    Query(params): Query<HashMap<String, String>>,
    State((cfg, registry, db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(e) = auth::check_token(params.get("token").map(String::as_str), &cfg) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"status":"error","errors":[e.to_string()]})),
        );
    }

    if let Some(pr) = registry.get(&port) {
        let rs_arc = pr.raw_ruleset.load();
        let version = rules::calc_version(&rs_arc).unwrap_or_default();
        (
            StatusCode::OK,
            Json(json!({"status":"ok","data":{"version":version,"ruleset": rs_arc.as_ref() }})),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(json!({"status":"error","errors":["unknown port"]})),
        )
    }
}

// ---------------------------------------------------------
// PUT /ruleset
// ---------------------------------------------------------
pub async fn put_ruleset(
    Path(port): Path<u16>,
    Query(params): Query<HashMap<String, String>>,
    State((cfg, registry, db)): State<SharedState>,
    Json(body): Json<crate::rules::RuleSetRaw>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(e) = auth::check_token(params.get("token").map(String::as_str), &cfg) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"status":"error","errors":[e.to_string()]})),
        );
    }

    // locate port
    let pr = match registry.get(&port) {
        Some(p) => p,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"status":"error","errors":["unknown port"]})),
            );
        }
    };

    // validate
    let scopes_guard = pr.enabled_scopes.read().unwrap();
    let errs = rules::validate::validate_ruleset(&body, &*scopes_guard);
    drop(scopes_guard);

    if !errs.is_empty() {
        error!(?errs, "ruleset validation failed");
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"status":"error","errors": errs })),
        );
    }

    // version + compile
    let version = match rules::calc_version(&body) {
        Ok(v) => v,
        Err(e) => {
            error!(?e, "version calc failed");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"status":"error","errors":[e.to_string()]})),
            );
        }
    };

    let compiled: CompiledRuleSet = match rules::compile::compile_ruleset(&body) {
        Ok(c) => c,
        Err(e) => {
            error!(?e, "compilation failed");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"status":"error","errors":[e.to_string()]})),
            );
        }
    };

    info!(
        port,
        version,
        markers = compiled.markers.len(),
        filters = compiled.filters.len(),
        "ruleset accepted"
    );

    let raw_str    = serde_json::to_string(&body).unwrap();
    let version_db = version.clone();

    pr.raw_ruleset.store(Arc::new(body));
    pr.compiled.store(Arc::new(compiled));

    let _ = db.tx.try_send(DbCmd::SaveRuleset {
        port,
        version: version_db,
        json: raw_str,
        ts_ns: crate::utils::now_ns() as i64,
    });

    (
        StatusCode::OK,
        Json(json!({"status":"ok","data":{"version":version}})),
    )
}

// ---------------------------------------------------------
// GET /stats  (currently stub)
// ---------------------------------------------------------
pub async fn get_stats(
    Path(port): Path<u16>,
    Query(params): Query<HashMap<String, String>>,
    State((cfg, registry, db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(e) = auth::check_token(params.get("token").map(String::as_str), &cfg) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"status":"error","errors":[e.to_string()]})),
        );
    }

    match registry.get(&port) {
        Some(_) => (
            StatusCode::OK,
            Json(json!({"status":"ok","data":"stats disabled"})),
        ),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"status":"error","errors":["unknown port"]})),
        ),
    }
}

#[derive(Deserialize)]
struct LogsQuery {
    since: Option<i64>,
    limit: Option<usize>,
    token: Option<String>,
}

pub async fn get_logs(
    Path(port): Path<u16>,
    Query(q): Query<LogsQuery>,
    State((cfg, registry, db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(e) = auth::check_token(q.token.as_deref(), &cfg) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"status":"error","errors":[e.to_string()]})),
        );
    }

    // ensure port exists
    if registry.get(&port).is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"status":"error","errors":["unknown port"]})),
        );
    }

    let since = q.since.unwrap_or(0);
    let limit = q.limit.unwrap_or(1000).min(10_000);

    match crate::db::fetch_records_since(db.path.clone(), port, since, limit).await {
        Ok((records, last_ts)) => {
            (
                StatusCode::OK,
                Json(json!({
                    "status":"ok",
                    "data": {
                        "records": records,
                        "next_since": last_ts
                    }
                })),
            )
        }
        Err(e) => {
            tracing::error!(?e, port, "fetch_logs failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"status":"error","errors":[e.to_string()]})),
            )
        }
    }
}


// ---------------------------------------------------------
async fn health() -> Json<serde_json::Value> {
    Json(json!({"status":"ok"}))
}

async fn version() -> Json<serde_json::Value> {
    Json(json!({"status":"ok","data":{"version":"0.0.1"}}))
}
