use crate::api::auth;
use crate::config::Config;
use crate::registry::PortRegistry;
use crate::rules;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;



pub fn build_router(cfg: Config, registry: PortRegistry) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/version", get(version))
        .route("/ports/{port}/ruleset", get(get_ruleset).put(put_ruleset))
        .with_state((cfg, registry))
}


type SharedState = (Config, PortRegistry);

#[derive(Deserialize)]
struct TokenWrap {
    token: String,
}

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


pub async fn put_ruleset(
    Path(port): Path<u16>,
    State((cfg, registry)): State<SharedState>,
    Json(body): Json<rules::RuleSetRaw>,
) -> (StatusCode, Json<serde_json::Value>) {
    // token check (extract manually since we already parsed into RuleSetRaw)
    // here, assume query ?token=... (simpler for now)
    let token_opt = None; // extend later
    if let Err(e) = auth::check_token(token_opt, &cfg.auth_hash) {
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

    // validate
    let errs = rules::validate::validate_ruleset(&body, &pr.enabled_scopes);
    if !errs.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"status":"error","errors": errs })),
        );
    }

    let ver = match rules::calc_version(&body) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"status":"error","errors":[e.to_string()]})),
            );
        }
    };

    pr.raw_ruleset.store(Arc::new(body));

    (
        StatusCode::OK,
        Json(json!({"status":"ok","data":{"version":ver}})),
    )
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({"status":"ok"}))
}

async fn version() -> Json<serde_json::Value> {
    // In future: embed git commit via build.rs or env! macro.
    Json(json!({"status":"ok","data":{"version":"0.0.1"}}))
}
