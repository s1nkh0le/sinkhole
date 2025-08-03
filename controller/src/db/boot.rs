use std::collections::HashMap;
use rusqlite::{Connection, Row};
use anyhow::Result;

/// Returns:
/// - ecam_since:  map service_id -> (max ts_ns + 1) or 0 if none.
/// - logs_since:  map (service_id, port) -> (max timestamp_ns + 1) or 0 if none.
pub fn read_boot_cursors(path: &str) -> Result<(HashMap<String, i64>, HashMap<(String, u16), i64>)> {
    let conn = Connection::open(path)?;

    // ECAM: max per service
    let mut ecam_since: HashMap<String, i64> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT service_id, MAX(ts_ns) AS max_ts
             FROM ecam
             GROUP BY service_id"
        )?;
        let rows = stmt.query_map([], |row: &Row| {
            let svc: String = row.get(0)?;
            let max_ts: Option<i64> = row.get(1)?;
            Ok((svc, max_ts.unwrap_or(-1)))
        })?;

        for r in rows {
            let (svc, max_ts) = r?;
            let next = if max_ts < 0 { 0 } else { max_ts.saturating_add(1) };
            ecam_since.insert(svc, next);
        }
    }

    // RECORDS: max per (service, port)
    let mut logs_since: HashMap<(String, u16), i64> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT service_id, port, MAX(timestamp_ns) AS max_ts
             FROM records
             GROUP BY service_id, port"
        )?;
        let rows = stmt.query_map([], |row: &Row| {
            let svc: String = row.get(0)?;
            let port_i: i64 = row.get(1)?; // SQLite INTEGER -> i64
            let port = u16::try_from(port_i).unwrap_or(0);
            let max_ts: Option<i64> = row.get(2)?;
            Ok(((svc, port), max_ts.unwrap_or(-1)))
        })?;

        for r in rows {
            let ((svc, port), max_ts) = r?;
            let next = if max_ts < 0 { 0 } else { max_ts.saturating_add(1) };
            logs_since.insert((svc, port), next);
        }
    }

    Ok((ecam_since, logs_since))
}