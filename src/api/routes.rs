use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde_json::json;
use tracing::{debug, error, info};

use crate::{
    api::auth,
    config::Config,
    registry::PortRegistry,
    rules::{self, compile::CompiledRuleSet},
};

pub fn build_router(cfg: Config, registry: PortRegistry) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/version", get(version))
        .route("/ports/{port}/stats", get(get_stats))
        .route(
            "/ports/{port}/ruleset",
            get(get_ruleset).put(put_ruleset),
        )
        .with_state((cfg, registry))
}

type SharedState = (Config, PortRegistry);

// ---------------------------------------------------------
// GET /ruleset
// ---------------------------------------------------------
pub async fn get_ruleset(
    Path(port): Path<u16>,
    Query(params): Query<HashMap<String, String>>,
    State((_cfg, registry)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(e) = auth::check_token(params.get("token").map(String::as_str)) {
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
    State((_cfg, registry)): State<SharedState>,
    Json(body): Json<crate::rules::RuleSetRaw>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(e) = auth::check_token(params.get("token").map(String::as_str)) {
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
            )
        }
    };

    // validate
    let errs = rules::validate::validate_ruleset(&body, &pr.enabled_scopes);
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

    pr.raw_ruleset.store(Arc::new(body));
    pr.compiled.store(Arc::new(compiled));

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
    State((_cfg, registry)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(e) = auth::check_token(params.get("token").map(String::as_str)) {
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

// ---------------------------------------------------------
async fn health() -> Json<serde_json::Value> {
    Json(json!({"status":"ok"}))
}

async fn version() -> Json<serde_json::Value> {
    Json(json!({"status":"ok","data":{"version":"0.0.1"}}))
}
