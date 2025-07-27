use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use tracing::error;
use crate::api::auth;
use crate::api::routes::SharedState;
use crate::db;

#[derive(Deserialize, Debug)]
pub struct LogsQuery {
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub limit: Option<usize>,
    pub order: Option<String>,          // "asc" | "desc"
    pub decision: Option<String>,       // "allow" | "drop"
    pub dir: Option<String>,            // "in" | "out"
    pub reason: Option<String>,         // "blacklist" | "whitelist_all_miss" | "whitelist_any_miss"
    pub token: Option<String>,
}

/// GET /ports/{port}/logs
pub async fn get_logs(
    Path(port): Path<u16>,
    Query(q): Query<LogsQuery>,
    State((cfg, registry, db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    // auth
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

    // ---- sanity checks -------------------------------------------------
    let mut errs = Vec::new();

    let decision = q.decision.as_deref().map(str::to_ascii_lowercase);
    if let Some(ref d) = decision {
        if d != "allow" && d != "drop" {
            errs.push("decision must be 'allow' or 'drop'".to_string());
        }
    }

    let dir = q.dir.as_deref().map(str::to_ascii_lowercase);
    if let Some(ref d) = dir {
        if d != "in" && d != "out" {
            errs.push("dir must be 'in' or 'out'".to_string());
        }
    }

    let reason = q.reason.as_deref().map(str::to_ascii_lowercase);
    if let Some(ref r) = reason {
        const ALLOWED: &[&str] = &["blacklist","whitelist_all_miss","whitelist_any_miss"];
        if !ALLOWED.contains(&r.as_str()) {
            errs.push(format!("reason must be one of {:?}", ALLOWED));
        }
    }

    let order_raw = q.order.as_deref().unwrap_or("asc");
    let asc = match order_raw.to_ascii_lowercase().as_str() {
        "asc"  => true,
        "desc" => false,
        _ => {
            errs.push("order must be 'asc' or 'desc'".to_string());
            true
        }
    };

    let limit = q.limit.unwrap_or(1000);
    if limit == 0 {
        errs.push("limit must be > 0".to_string());
    }
    let limit = limit.min(10_000);

    let since = q.since.unwrap_or(0);
    let until = q.until.unwrap_or(i64::MAX);
    if since < 0 || until < 0 || until < since {
        errs.push("invalid since/until range".to_string());
    }

    if !errs.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"status":"error","errors": errs})),
        );
    }

    // fetch
    match db::fetch_records_filtered(
        db.path.clone(),
        port,
        since,
        until,
        limit,
        asc,
        q.decision.as_deref(),
        q.dir.as_deref(),
        q.reason.as_deref(),
    ).await {
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
            error!(?e, port, "fetch_records_filtered failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"status":"error","errors":[e.to_string()]})),
            )
        }
    }
}