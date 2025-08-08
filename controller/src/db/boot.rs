use anyhow::Result;
use rusqlite::{Connection, Row};
use std::collections::HashMap;

/// Returns:
/// - ecam_since:  map service_id -> (max ts_ns + 1) or 0 if none.
/// - logs_since:  map (service_id, port) -> (max timestamp_ns + 1) or 0 if none.
// controller/db/boot.rs
pub fn read_boot_cursors(
    path: &str,
) -> Result<(HashMap<String, u64>, HashMap<(String, u16), u64>)> {
    let conn = Connection::open(path)?;

    // ── ECAM cursors ────────────────────────────────────────────────
    let mut ecam_since: HashMap<String, u64> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT service_id, MAX(ts_ns) AS max_ts
             FROM ecam
             GROUP BY service_id",
        )?;
        let rows = stmt.query_map([], |row: &Row| {
            // SQLite row -> (String, Option<i64>)
            let svc: String  = row.get(0)?;
            let max_ts: Option<i64> = row.get(1)?;
            Ok((svc, max_ts))
        })?;

        for r in rows {
            let (svc, max_ts) = r?;
            let next: u64 = match max_ts {
                Some(v) if v >= 0 => v.saturating_add(1)        as u64,
                _                        => 0,
            };
            ecam_since.insert(svc, next);
        }
    }

    // ── RECORD cursors ──────────────────────────────────────────────
    let mut logs_since: HashMap<(String, u16), u64> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT service_id, port, MAX(timestamp_ns) AS max_ts
             FROM records
             GROUP BY service_id, port",
        )?;
        let rows = stmt.query_map([], |row: &Row| {
            let svc:  String = row.get(0)?;
            let port_i: i64  = row.get(1)?;
            let max_ts: Option<i64> = row.get(2)?;
            Ok(((svc, port_i as u16), max_ts))
        })?;

        for r in rows {
            let ((svc, port), max_ts) = r?;
            let next: u64 = match max_ts {
                Some(v) if v >= 0 => v.saturating_add(1)        as u64,
                _                        => 0,
            };
            logs_since.insert((svc, port), next);
        }
    }

    Ok((ecam_since, logs_since))
}

