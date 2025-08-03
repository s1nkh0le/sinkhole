use crate::db;
use crate::state::ControllerState;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize, Debug)]
pub struct RecordsQuery {
    pub service_id: Option<String>,
    pub port: Option<u16>,
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub limit: Option<usize>,
    pub order: Option<String>,
}

/// GET /records?service_id=&port=&since=&until=&limit=&order=
pub async fn get_records(
    Query(q): Query<RecordsQuery>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<serde_json::Value>) {
    // defaults
    let since = q.since.unwrap_or(0);
    let until = q.until.unwrap_or(i64::MAX);
    let limit = q.limit.unwrap_or(1000).min(10_000);
    let asc = q
        .order
        .as_deref()
        .map(|s| s.eq_ignore_ascii_case("asc"))
        .unwrap_or(true);

    // validate service if provided
    if let Some(ref svc_id) = q.service_id {
        if !state.read().services.contains_key(svc_id) {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"status":"error","errors":["unknown service_id"]})),
            );
        }
        // if a port is provided, ensure controller knows it (best-effort)
        if let Some(p) = q.port {
            let guard = state.read();
            if let Some(svc_ref) = guard.services.get(svc_id) {
                if !svc_ref.ports.contains_key(&p) {
                    return (
                        StatusCode::NOT_FOUND,
                        Json(json!({"status":"error","errors":["unknown port for service"]})),
                    );
                }
            }
        }
    }

    // query DB
    let db = state.db();
    let svc = q.service_id.clone();
    let port = q.port;
    let join_res = tokio::task::spawn_blocking(move || {
        db::query::load_records_agg(&db.path, svc.as_deref(), port, since, until, limit, asc)
    })
    .await;

    match join_res {
        Ok(Ok((records, next_since))) => (
            StatusCode::OK,
            Json(json!({"status":"ok","data":{"records": records, "next_since": next_since}})),
        ),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"status":"error","errors":[e.to_string()]})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"status":"error","errors":[format!("join error: {e}")]})),
        ),
    }
}
