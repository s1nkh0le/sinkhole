use crate::api::auth;
use crate::api::routes::SharedState;
use crate::db::actor::DbCmd;
use crate::observability::sinks::sink_ecam;
use crate::observability::sinks::sink_ecam::{Origin, Severity};
use crate::observability::summary::dump_port_state;
use crate::rules;
use crate::rules::CompiledRuleSet;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{error, info};

///     API: `GET /ports/{port}/ruleset`
///     method: `get_ruleset`
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

    match registry.get(&port) {
        Some(pr) => {
            let rs_arc = pr.raw_ruleset.load();
            let version = rules::calc_version(&rs_arc).unwrap_or_default();

            (
                StatusCode::OK,
                Json(json!({"status":"ok","data":{"version":version,"ruleset": rs_arc.as_ref() }})),
            )
        }
        None => {
            sink_ecam::record(
                Severity::Warn,
                Origin::API,
                format!("[GET /ports/{port}/ruleset, get_ruleset()]: port not found"),
                None,
            );

            (
                StatusCode::NOT_FOUND,
                Json(json!({"status":"error","errors":["unknown port"]})),
            )
        }
    }
}

///     API: `PUT /ports/{port}/ruleset`
///     method: `put_ruleset`
pub async fn put_ruleset(
    Path(port): Path<u16>,
    Query(params): Query<HashMap<String, String>>,
    State((cfg, registry, db)): State<SharedState>,
    Json(body): Json<rules::RuleSetRaw>,
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
        sink_ecam::record(
            Severity::Warn,
            Origin::API,
            format!(
                "[PUT /ports/{port}/ruleset, put_ruleset()]: rejecting rules | ruleset validation failed, error=s{}", errs.join(", ")
            ),
            None,
        );

        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"status":"error","errors": errs })),
        );
    }

    // version + compile
    let version = match rules::calc_version(&body) {
        Ok(v) => v,
        Err(e) => {
            sink_ecam::record(
                Severity::Warn,
                Origin::API,
                format!(
                    "[PUT /ports/{port}/ruleset, put_ruleset()]: rejecting rules | version calc failed, error={e}"
                ),
                None,
            );

            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"status":"error","errors":[e.to_string()]})),
            );
        }
    };

    let compiled: CompiledRuleSet = match rules::compile::compile_ruleset(&body) {
        Ok(c) => c,
        Err(e) => {
            sink_ecam::record(
                Severity::Warn,
                Origin::API,
                format!(
                    "[PUT /ports/{port}/ruleset, put_ruleset()]: rejecting rules | compilation failed, error={e}"
                ),
                None,
            );

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

    let raw_str = serde_json::to_string(&body).unwrap();
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
