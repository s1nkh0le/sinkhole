use crate::{
    api::auth,
    config::Config,
    registry::PortRegistry,
    rules::{self, compile::CompiledRuleSet},
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tracing::{debug, error, info};

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

#[derive(Deserialize)]
struct TokenWrap {
    token: String,
}

// ---------------------------------------------------------
// GET /ruleset
// ---------------------------------------------------------
pub async fn get_ruleset(
    Path(port): Path<u16>,
    State((_cfg, registry)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
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
    State((cfg, registry)): State<SharedState>,
    Json(body): Json<rules::RuleSetRaw>,
) -> (StatusCode, Json<serde_json::Value>) {
    // ── auth ──────────────────────────────────────────────
    if let Err(e) = auth::check_token(None, &cfg.auth_hash) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"status":"error","errors":[e.to_string()]})),
        );
    }

    // ── locate port ───────────────────────────────────────
    let pr = match registry.get(&port) {
        Some(p) => p,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"status":"error","errors":["unknown port"]})),
            )
        }
    };

    // ── validate JSON vs port scopes ──────────────────────
    let errs = rules::validate::validate_ruleset(&body, &pr.enabled_scopes);
    if !errs.is_empty() {
        error!(?errs, "ruleset validation failed");
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"status":"error","errors": errs })),
        );
    }

    // ── calculate deterministic version ───────────────────
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

    // ── compile ───────────────────────────────────────────
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

    // ── atomically replace ────────────────────────────────
    pr.raw_ruleset.store(Arc::new(body));
    pr.compiled.store(Arc::new(compiled));

    (
        StatusCode::OK,
        Json(json!({"status":"ok","data":{"version":version}})),
    )
}

// ---------------------------------------------------------
// GET /stats
// ---------------------------------------------------------
pub async fn get_stats(
    Path(port): Path<u16>,
    State((_cfg, registry)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match registry.get(&port) {
        Some(pr) => {
            let snap = crate::observability::stats::capture(&pr);
            (
                StatusCode::OK,
                Json(json!({"status":"ok","data": snap })),
            )
        }
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
