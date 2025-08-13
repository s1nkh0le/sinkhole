use anyhow::{anyhow, Result};
use rusqlite::{params, Connection};
use shared::types::record_types::EvaluationRecord;
use shared::types::ecam_types::EcamEvent;
use std::path::Path;
use crate::db::util::open;

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
    sql.push_str(if asc {
        " ORDER BY ts_ns ASC"
    } else {
        " ORDER BY ts_ns DESC"
    });
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


pub fn load_records_agg_advanced(
    db_path: &str,
    service_id: Option<&str>,
    port: Option<u16>,
    since: i64,
    until: i64,
    limit: usize,
    is_asc: bool,
    positive_markers: &[String],
    negative_markers: &[String],
    is_inbound: Option<bool>,
    decision: Option<&str>,
    conn_id: Option<i64>,
    method: Option<&str>,
    path_like: Option<&str>,
    status_eq: Option<i64>,
    status_min: Option<i64>,
    status_max: Option<i64>,
    len: Option<i64>,
    len_min: Option<i64>,
    len_max: Option<i64>,
    drop_kind: Option<&str>,
    drop_item_type: Option<&str>,
    drop_value: Option<&str>,
) -> Result<(Vec<EvaluationRecord>, i64)> {
    let conn = open(db_path)?;

    let need_to_join_dropreason =
        drop_kind.is_some() || drop_item_type.is_some() || drop_value.is_some();

    let pos_markers: Vec<&str> = {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        positive_markers
            .iter()
            .map(|s| s.as_str())
            .filter(|m| seen.insert((*m).to_owned()))
            .collect()
    };
    let need_positive_markers_join = !pos_markers.is_empty();
    let need_negative_markers_subquery = !negative_markers.is_empty();

    let need_distinct = need_to_join_dropreason || need_positive_markers_join;

    let mut sql = String::new();
    sql.push_str("SELECT ");
    if need_distinct { sql.push_str("DISTINCT "); }
    sql.push_str("r.timestamp_ns, r.json FROM records r");

    if need_to_join_dropreason {
        sql.push_str(
            " JOIN drop_reason_items d
              ON d.timestamp_ns = r.timestamp_ns
             AND d.service_id   = r.service_id
             AND d.port         = r.port",
        );
    }

    for (i, _) in pos_markers.iter().enumerate() {
        use std::fmt::Write as _;
        let _ = write!(
            &mut sql,
            " JOIN record_markers m{idx}
                ON m{idx}.timestamp_ns = r.timestamp_ns
               AND m{idx}.service_id   = r.service_id
               AND m{idx}.port         = r.port
               AND m{idx}.marker_id    = ?",
            idx = i
        );
    }

    sql.push_str(" WHERE r.timestamp_ns > ? AND r.timestamp_ns <= ?");

    let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    for m in &pos_markers {
        binds.push(Box::new((*m).to_string()));
    }
    binds.push(Box::new(since));
    binds.push(Box::new(until));

    if let Some(svc) = service_id {
        sql.push_str(" AND r.service_id = ?");
        binds.push(Box::new(svc.to_string()));
    }
    if let Some(p) = port {
        sql.push_str(" AND r.port = ?");
        binds.push(Box::new(p as i64));
    }
    if let Some(b) = is_inbound {
        sql.push_str(" AND r.inbound = ?");
        binds.push(Box::new(if b { 1i64 } else { 0i64 }));
    }
    if let Some(dec) = decision {
        sql.push_str(" AND r.decision = ?");
        binds.push(Box::new(dec.to_string()));
    } else if need_to_join_dropreason {
        // Optional hint for the planner (join already implies this)
        sql.push_str(" AND r.decision = 'drop'");
    }

    if let Some(cid) = conn_id {
        sql.push_str(" AND r.conn_id = ?");
        binds.push(Box::new(cid));
    }
    if let Some(m) = method {
        sql.push_str(" AND r.http_method = ?");
        binds.push(Box::new(m.to_string()));
    }
    if let Some(pl) = path_like {
        sql.push_str(" AND r.http_path LIKE ?");
        binds.push(Box::new(pl.to_string()));
    }
    if let Some(eq) = status_eq {
        sql.push_str(" AND r.http_status = ?");
        binds.push(Box::new(eq));
    } else {
        if let Some(mi) = status_min {
            sql.push_str(" AND r.http_status >= ?");
            binds.push(Box::new(mi));
        }
        if let Some(ma) = status_max {
            sql.push_str(" AND r.http_status <= ?");
            binds.push(Box::new(ma));
        }
    }

    if let Some(len_exact) = len {
        sql.push_str(" AND r.http_length = ?");
        binds.push(Box::new(len_exact));
    } else {
        if let Some(mi) = len_min {
            sql.push_str(" AND r.http_length >= ?");
            binds.push(Box::new(mi));
        }
        if let Some(ma) = len_max {
            sql.push_str(" AND r.http_length <= ?");
            binds.push(Box::new(ma));
        }
    }

    if need_negative_markers_subquery {
        use std::fmt::Write as _;
        sql.push_str(
            " AND NOT EXISTS (
                SELECT 1 FROM record_markers mneg
                 WHERE mneg.timestamp_ns = r.timestamp_ns
                   AND mneg.service_id   = r.service_id
                   AND mneg.port         = r.port
                   AND mneg.marker_id IN (",
        );
        let placeholders = std::iter::repeat("?")
            .take(negative_markers.len())
            .collect::<Vec<_>>()
            .join(", ");
        let _ = write!(&mut sql, "{placeholders}");
        sql.push_str("))");

        for m in negative_markers {
            binds.push(Box::new(m.clone()));
        }
    }

    if need_to_join_dropreason {
        if let Some(k) = drop_kind {
            sql.push_str(" AND d.reason_kind = ?");
            binds.push(Box::new(k.to_string()));
        }
        if let Some(t) = drop_item_type {
            sql.push_str(" AND d.item_type = ?");
            binds.push(Box::new(t.to_string()));
        }
        if let Some(v) = drop_value {
            sql.push_str(" AND d.value = ?");
            binds.push(Box::new(v.to_string()));
        }
    }

    sql.push_str(if is_asc { " ORDER BY r.timestamp_ns ASC" } else { " ORDER BY r.timestamp_ns DESC" });
    sql.push_str(" LIMIT ?");
    binds.push(Box::new(limit as i64));

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(rusqlite::params_from_iter(binds.iter()))?;

    let mut out: Vec<EvaluationRecord> = Vec::new();
    let mut max_ts = since;

    while let Some(row) = rows.next()? {
        let ts: i64 = row.get(0)?;
        let js: String = row.get(1)?;
        let rec: EvaluationRecord = serde_json::from_str(&js)
            .map_err(|e| anyhow::anyhow!("bad record json: {e}"))?;
        if ts > max_ts { max_ts = ts; }
        out.push(rec);
    }

    let next_since = if out.is_empty() { since } else { max_ts + 1 };
    Ok((out, next_since))
}