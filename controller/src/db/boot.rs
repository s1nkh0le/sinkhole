use anyhow::Result;
use rusqlite::{Connection, Row};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]
pub struct BootCursors {
    pub ecam_cursors: HashMap<String, u64>,
    pub records_cursors: HashMap<(String, u16), u64>
}

/// Fetches the map to use the latest timestamps from the db as cursor, in order to only fetch fresh data.
/// Produces a map for ECAM and a map for records.
///
/// Returns:
/// - ecam_since:  map (service_id) -> (max{ts_ns} + 1ns),                   or 0 if none.
/// - logs_since:  map (service_id, port) -> (max{timestamp_ns} + 1ns),      or 0 if none.
pub fn read_boot_cursors(
    path: &str,
) -> Result<BootCursors> {
    let conn = Connection::open(path)?;

    let mut ecam_cursors: HashMap<String, u64> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT service_id, MAX(ts_ns) AS max_ts
             FROM ecam
             GROUP BY service_id",
        )?;
        let rows = stmt.query_map([], |row: &Row| {
            // SQLite row -> (String, Option<i64>)
            let svc: String = row.get(0)?;
            let max_ts: Option<i64> = row.get(1)?;
            Ok((svc, max_ts))
        })?;

        for r in rows {
            let (svc, max_ts) = r?;
            let next: u64 = match max_ts {
                Some(v) if v >= 0 => v.saturating_add(1) as u64,
                _ => 0,
            };
            ecam_cursors.insert(svc, next);
        }
    }

    let mut records_cursors: HashMap<(String, u16), u64> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT service_id, port, MAX(timestamp_ns) AS max_ts
             FROM records
             GROUP BY service_id, port",
        )?;
        let rows = stmt.query_map([], |row: &Row| {
            let svc: String = row.get(0)?;
            let port_i: i64 = row.get(1)?;
            let max_ts: Option<i64> = row.get(2)?;
            Ok(((svc, port_i as u16), max_ts))
        })?;

        for r in rows {
            let ((svc, port), max_ts) = r?;
            let next: u64 = match max_ts {
                Some(v) if v >= 0 => v.saturating_add(1) as u64,
                _ => 0,
            };
            records_cursors.insert((svc, port), next);
        }
    }

    Ok(BootCursors { ecam_cursors, records_cursors })
}
