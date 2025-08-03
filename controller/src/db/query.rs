use std::path::Path;
use anyhow::{anyhow, Result};
use rusqlite::{params, Connection};
use shared::eval_record::EvaluationRecord;
use shared::sink_model::EcamEvent;


fn open(path: &str) -> Result<Connection> {
    let conn = Connection::open(Path::new(path))?;
    Ok(conn)
}

/// Load ECAM events across services (optional filter by service_id)
pub fn load_ecam_agg(
    db_path: &str,
    service_id: Option<&str>,
    since: i64,
    until: i64,
    limit: usize,
    asc: bool,
) -> Result<(Vec<EcamEvent>, i64)> {
    let conn = open(db_path)?;
    let mut sql = String::from(
        "SELECT ts_ns, json
         FROM ecam
         WHERE ts_ns > ? AND ts_ns <= ?",
    );
    if service_id.is_some() {
        sql.push_str(" AND service_id = ?");
    }
    sql.push_str(if asc { " ORDER BY ts_ns ASC" } else { " ORDER BY ts_ns DESC" });
    sql.push_str(" LIMIT ?");

    let mut stmt = conn.prepare(&sql)?;

    let mut rows = if let Some(svc) = service_id {
        stmt.query(params![since, until, svc, limit as i64])?
    } else {
        stmt.query(params![since, until, limit as i64])?
    };

    let mut out: Vec<EcamEvent> = Vec::new();
    let mut max_ts = since;

    while let Some(row) = rows.next()? {
        let ts: i64 = row.get(0)?;
        let js: String = row.get(1)?;
        let ev: EcamEvent = serde_json::from_str(&js).map_err(|e| anyhow!("bad ecam json: {e}"))?;
        if ts > max_ts {
            max_ts = ts;
        }
        out.push(ev);
    }

    // next_since = (max + 1) or carry forward since when empty
    let next_since = if out.is_empty() { since } else { max_ts + 1 };
    // If DESC order was used for browse, the client can still use next_since semantics by requesting ASC later.
    Ok((out, next_since))
}

/// Load records across services/ports (optional filters)
pub fn load_records_agg(
    db_path: &str,
    service_id: Option<&str>,
    port: Option<u16>,
    since: i64,
    until: i64,
    limit: usize,
    asc: bool,
) -> Result<(Vec<EvaluationRecord>, i64)> {
    let conn = open(db_path)?;

    // Base
    let mut sql = String::from(
        "SELECT timestamp_ns, json
         FROM records
         WHERE timestamp_ns > ? AND timestamp_ns <= ?",
    );
    // Filters
    if service_id.is_some() {
        sql.push_str(" AND service_id = ?");
    }
    if port.is_some() {
        sql.push_str(" AND port = ?");
    }

    // Order & limit
    sql.push_str(if asc { " ORDER BY timestamp_ns ASC" } else { " ORDER BY timestamp_ns DESC" });
    sql.push_str(" LIMIT ?");

    // Bind
    let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    binds.push(Box::new(since));
    binds.push(Box::new(until));
    if let Some(svc) = service_id {
        binds.push(Box::new(svc.to_string()));
    }
    if let Some(p) = port {
        binds.push(Box::new(p as i64));
    }
    binds.push(Box::new(limit as i64));

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(rusqlite::params_from_iter(binds.iter()))?;

    let mut out: Vec<EvaluationRecord> = Vec::new();
    let mut max_ts = since;

    while let Some(row) = rows.next()? {
        let ts: i64 = row.get(0)?;
        let js: String = row.get(1)?;
        let rec: EvaluationRecord =
            serde_json::from_str(&js).map_err(|e| anyhow!("bad record json: {e}"))?;
        if ts > max_ts {
            max_ts = ts;
        }
        out.push(rec);
    }

    let next_since = if out.is_empty() { since } else { max_ts + 1 };
    Ok((out, next_since))
}

