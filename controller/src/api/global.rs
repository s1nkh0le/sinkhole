use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tracing::warn;
use crate::{
    state::ControllerState,
};
use shared::eval_record::{EvaluationRecord, DropReason};
use shared::types::Decision;
use crate::db::query::{load_ecam_agg, load_records_agg};

#[derive(Deserialize, Debug)]
pub struct EcamQuery {
    pub service_id: Option<String>,
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub limit: Option<usize>,
    pub order: Option<String>, // "asc" | "desc"
}

#[derive(Deserialize, Debug)]
pub struct RecordsAggQuery {
    pub service_id: Option<String>,
    pub port: Option<u16>,
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub limit: Option<usize>,
    pub order: Option<String>, // "asc" | "desc"
    pub decision: Option<String>, // "allow" | "drop"
    pub dir: Option<String>,      // "in" | "out"
    pub reason: Option<String>,   // "blacklist" | "whitelist_all_miss" | "whitelist_any_miss"
}

/// GET /ecam (aggregate)
pub async fn get_ecam_global(
    State(state): State<ControllerState>,
    Query(q): Query<EcamQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    // todo check valid port
    // basic params & defaults
    let since = q.since.unwrap_or(0);
    let until = q.until.unwrap_or(i64::MAX);

    let order_raw = q.order.as_deref().unwrap_or("asc");
    let asc = match order_raw.to_ascii_lowercase().as_str() {
        "asc" => true,
        "desc" => false,
        _ => true,
    };

    let limit = q.limit.unwrap_or(1000).min(10_000);

    // optional: validate service_id if present (404 on unknown)
    if let Some(ref svc) = q.service_id {
        if !state.read().services.contains_key(svc) {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"status":"error","errors":["unknown service_id"]})),
            );
        }
    }

    let db = state.db();
    let svc_opt = q.service_id.clone();

    if svc_opt.is_some() {
        warn!("[get_ecam_global(), GET /ecam] passed svc={} to global aggregation API", svc_opt.clone().unwrap());
    }

    let res = tokio::task::spawn_blocking(move || {
        load_ecam_agg(&db.path, svc_opt.as_deref(), since, until, limit, asc)
    })
        .await;

    match res {
        Ok(Ok((events, next_since))) => (
            StatusCode::OK,
            Json(json!({
                "status":"ok",
                "data": events,
                "next_since": next_since
            })),
        ),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"status":"error","errors":[e.to_string()]})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"status":"error","errors":[e.to_string()]})),
        ),
    }
}

/// helper: decision filter
fn decision_matches(rec: &EvaluationRecord, want: &str) -> bool {
    match want {
        "allow" => matches!(rec.decision, Decision::Allow),
        "drop"  => matches!(rec.decision, Decision::Drop),
        _ => true,
    }
}

/// helper: direction filter
fn dir_matches(rec: &EvaluationRecord, want: &str) -> bool {
    match want {
        "in"  => rec.inbound,
        "out" => !rec.inbound,
        _ => true,
    }
}

/// helper: reason filter (based on drop_reasons)
fn reason_matches(rec: &EvaluationRecord, want: &str) -> bool {
    match want {
        "blacklist" =>
            rec.drop_reasons.iter().any(|r| matches!(r, DropReason::Blacklist { .. })),
        "whitelist_all_miss" =>
            rec.drop_reasons.iter().any(|r| matches!(r, DropReason::WhitelistAllMiss { .. })),
        "whitelist_any_miss" =>
            rec.drop_reasons.iter().any(|r| matches!(r, DropReason::WhitelistAnyMiss { .. })),
        _ => true,
    }
}

/// GET /records (aggregate)
pub async fn get_records_global(
    State(state): State<ControllerState>,
    Query(q): Query<RecordsAggQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    let since = q.since.unwrap_or(0);
    let until = q.until.unwrap_or(i64::MAX);

    let order_raw = q.order.as_deref().unwrap_or("asc");
    let asc = match order_raw.to_ascii_lowercase().as_str() {
        "asc" => true,
        "desc" => false,
        _ => true,
    };

    let limit = q.limit.unwrap_or(1000).min(10_000);

    // todo: validate service_id and port if present
    if let Some(ref svc) = q.service_id {
        if !state.read().services.contains_key(svc) {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"status":"error","errors":["unknown service_id"]})),
            );
        }
    }

    let db = state.db();
    let svc_opt = q.service_id.clone();
    let port_opt = q.port;

    if port_opt.is_some() {
        warn!("[get_records_global(), GET /records] passed port={} to global aggregation API", port_opt.clone().unwrap());
    }

    // Load (SQL: service, port, times, order, limit)
    let res = tokio::task::spawn_blocking(move || {
        load_records_agg(&db.path, svc_opt.as_deref(), port_opt, since, until, limit, asc)
    })
        .await;

    match res {
        Ok(Ok((mut records, next_since))) => {
            // apply in-memory filters for decision/dir/reason
            if let Some(ref d) = q.decision {
                let want = d.to_ascii_lowercase();
                records.retain(|r| decision_matches(r, &want));
            }
            if let Some(ref d) = q.dir {
                let want = d.to_ascii_lowercase();
                records.retain(|r| dir_matches(r, &want));
            }
            if let Some(ref r) = q.reason {
                let want = r.to_ascii_lowercase();
                records.retain(|rec| reason_matches(rec, &want));
            }

            (
                StatusCode::OK,
                Json(json!({
                    "status":"ok",
                    "data": {
                        "records": records,
                        "next_since": next_since
                    }
                })),
            )
        }
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"status":"error","errors":[e.to_string()]})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"status":"error","errors":[e.to_string()]})),
        ),
    }
}
