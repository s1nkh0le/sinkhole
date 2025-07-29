use std::collections::HashMap;
use std::sync::Arc;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::json;
use tracing::{error, info};
use crate::api::auth;
use crate::api::routes::SharedState;
use crate::db::actor::DbCmd;
use crate::observability::summary::{dump_port_state};
use crate::rules;
use crate::rules::CompiledRuleSet;

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

    // Print ruleset log
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

    // Log port state
    dump_port_state(&*pr);

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
