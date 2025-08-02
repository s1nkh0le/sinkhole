use anyhow::{anyhow, Result};
use rusqlite::{params, Connection};
use serde_json::Value;
use shared::eval_record::EvaluationRecord;
use shared::sink_model::EcamEvent;
use std::sync::Arc;

/// Load ECAM events (ascending), starting after `since` (exclusive).
/// Returns (events, next_since).
pub fn load_ecam(db_path: &Arc<String>, svc: &str, since: i64, limit: usize) -> Result<(Vec<EcamEvent>, i64)> {
    let conn = Connection::open(&**db_path)?;
    let mut stmt = conn.prepare(
        "SELECT ts_ns, json
         FROM ecam
         WHERE service_id = ?1 AND ts_ns > ?2
         ORDER BY ts_ns ASC
         LIMIT ?3",
    )?;

    let mut rows = stmt.query(params![svc, since, limit as i64])?;
    let mut out: Vec<EcamEvent> = Vec::new();
    let mut last_ts = since;

    while let Some(row) = rows.next()? {
        let ts: i64 = row.get(0)?;
        let js: String = row.get(1)?;
        // We trust the JSON was inserted by our controller (shared model).
        let mut ev: EcamEvent = serde_json::from_str(&js)
            .map_err(|e| anyhow!("invalid ecam json in db: {}", e))?;
        // Defensive: ensure ts from DB is authoritative
        ev.ts_ns = ts;
        last_ts = ts;
        out.push(ev);
    }

    let next_since = if last_ts == since { since } else { last_ts + 1 };
    Ok((out, next_since))
}

/// Load evaluation records (ascending), starting after `since` (exclusive).
/// Returns (records, next_since).
pub fn load_records(
    db_path: &Arc<String>,
    svc: &str,
    port: u16,
    since: i64,
    limit: usize,
) -> Result<(Vec<EvaluationRecord>, i64)> {
    let conn = Connection::open(&**db_path)?;
    let mut stmt = conn.prepare(
        "SELECT timestamp_ns, json
         FROM records
         WHERE service_id = ?1 AND port = ?2 AND timestamp_ns > ?3
         ORDER BY timestamp_ns ASC
         LIMIT ?4",
    )?;

    let mut rows = stmt.query(params![svc, port as i64, since, limit as i64])?;
    let mut out: Vec<EvaluationRecord> = Vec::new();
    let mut last_ts = since;

    while let Some(row) = rows.next()? {
        let ts: i64 = row.get(0)?;
        let js: String = row.get(1)?;
        let mut r: EvaluationRecord = serde_json::from_str(&js)
            .map_err(|e| anyhow!("invalid record json in db: {}", e))?;
        // Defensive: DB timestamp is authoritative for cursoring
        r.timestamp_ns = ts as u128;
        last_ts = ts;
        out.push(r);
    }

    let next_since = if last_ts == since { since } else { last_ts + 1 };
    Ok((out, next_since))
}
