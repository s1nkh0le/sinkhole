use anyhow::{anyhow, Result};
use rusqlite::{params_from_iter, ToSql};
use crate::api::api_records_grouped::RecordGroup;
use shared::types::record_types::EvaluationRecord;
use shared::types::ecam_types::EcamEvent;
use std::path::Path;
use crate::db::util::open;

/// Returns groups (service_id, port, conn_id) with their records, and a cursor (next_since).
/// `limit_groups` is the number of groups to return (not records).
pub fn load_records_grouped_agg_advanced(
    db_path: &str,
    service_id: Option<&str>,
    port: Option<u16>,
    since: i64,
    until: i64,
    limit_groups: usize,
    asc: bool,

    positive_markers: &[String],
    negative_markers: &[String],
    inbound: Option<bool>,
    decision: Option<&str>,     // "allow" | "drop"
    conn_id: Option<i64>,

    method: Option<&str>,
    path_like: Option<&str>,    // already wrapped with %...%
    status_eq: Option<i64>,
    status_min: Option<i64>,
    status_max: Option<i64>,

    len: Option<i64>,
    len_min: Option<i64>,
    len_max: Option<i64>,

    drop_kind: Option<&str>,        // e.g. "blacklist", "whitelist_all_miss", "whitelist_any_miss"
    drop_item_type: Option<&str>,   // e.g. "filter_id", "missing_id", "hit_id"
    drop_value: Option<&str>,
) -> Result<(Vec<RecordGroup>, i64)>
where
    RecordGroup: serde::de::DeserializeOwned + Clone, // adapt if you place types elsewhere
{
    let conn = open(db_path)?;

    // ——— Build phase 1: find group keys (svc, port, conn) matching filters
    let need_drop_join = drop_kind.is_some() || drop_item_type.is_some() || drop_value.is_some();

    // De-dup positive markers, we’ll join once per id
    let pos_markers: Vec<&str> = {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        positive_markers
            .iter()
            .map(|s| s.as_str())
            .filter(|m| seen.insert((*m).to_owned()))
            .collect()
    };
    let need_pos = !pos_markers.is_empty();
    let need_neg = !negative_markers.is_empty();

    // NOTE: we order groups by their first seen timestamp (MIN(r.timestamp_ns))
    let mut sql = String::new();
    sql.push_str("WITH matches AS (");
    sql.push_str(
        "SELECT r.service_id, r.port, r.conn_id, \
                MIN(r.timestamp_ns) AS first_ts, \
                MAX(r.timestamp_ns) AS last_ts \
         FROM records r"
    );

    if need_drop_join {
        sql.push_str(
            " JOIN drop_reason_items d
                ON d.timestamp_ns = r.timestamp_ns
               AND d.service_id   = r.service_id
               AND d.port         = r.port"
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

    // (binds for phase 1)
    let mut binds: Vec<Box<dyn ToSql>> = Vec::new();
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
    if let Some(b) = inbound {
        sql.push_str(" AND r.inbound = ?");
        binds.push(Box::new(if b { 1i64 } else { 0i64 }));
    }
    if let Some(dec) = decision {
        sql.push_str(" AND r.decision = ?");
        binds.push(Box::new(dec.to_string()));
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
    if let Some(exact) = len {
        sql.push_str(" AND r.http_length = ?");
        binds.push(Box::new(exact));
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

    if need_neg {
        use std::fmt::Write as _;
        sql.push_str(
            " AND NOT EXISTS (
                SELECT 1 FROM record_markers mneg
                 WHERE mneg.timestamp_ns = r.timestamp_ns
                   AND mneg.service_id   = r.service_id
                   AND mneg.port         = r.port
                   AND mneg.marker_id IN ("
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

    if need_drop_join {
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

    sql.push_str(" GROUP BY r.service_id, r.port, r.conn_id");
    sql.push_str(if asc { " ORDER BY first_ts ASC" } else { " ORDER BY first_ts DESC" });
    sql.push_str(" LIMIT ?");
    binds.push(Box::new(limit_groups as i64));
    sql.push_str(") SELECT service_id, port, conn_id, first_ts, last_ts FROM matches");

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(binds.iter()))?;

    #[derive(Clone)]
    struct Key {
        service_id: String,
        port: u16,
        conn_id: i64,
        first_ts: i64,
        last_ts: i64,
    }

    let mut keys: Vec<Key> = Vec::new();
    let mut max_first = since;

    while let Some(row) = rows.next()? {
        let svc: String = row.get(0)?;
        let port_i: i64 = row.get(1)?;
        let conn: i64 = row.get(2)?;
        let first_ts: i64 = row.get(3)?;
        let last_ts: i64 = row.get(4)?;

        keys.push(Key {
            service_id: svc,
            port: port_i as u16,
            conn_id: conn,
            first_ts,
            last_ts,
        });

        if first_ts > max_first {
            max_first = first_ts;
        }
    }

    if keys.is_empty() {
        return Ok((Vec::new(), since));
    }

    // ——— Phase 2: fetch all records for the selected groups
    // Build a big OR list: (svc=?, port=?, conn=?) OR ...
    let mut sql2 = String::from(
        "SELECT r.timestamp_ns, r.json, r.service_id, r.port, r.conn_id \
         FROM records r WHERE "
    );
    let mut binds2: Vec<Box<dyn ToSql>> = Vec::new();
    for (i, k) in keys.iter().enumerate() {
        if i > 0 { sql2.push_str(" OR "); }
        sql2.push_str("(r.service_id = ? AND r.port = ? AND r.conn_id = ?)");
        binds2.push(Box::new(k.service_id.clone()));
        binds2.push(Box::new(k.port as i64));
        binds2.push(Box::new(k.conn_id));
    }
    sql2.push_str(" ORDER BY r.timestamp_ns ASC");

    let mut stmt2 = conn.prepare(&sql2)?;
    let mut rows2 = stmt2.query(params_from_iter(binds2.iter()))?;

    use std::collections::BTreeMap;
    let mut buckets: BTreeMap<(String, u16, i64), Vec<EvaluationRecord>> = BTreeMap::new();

    while let Some(row) = rows2.next()? {
        let _ts: i64 = row.get(0)?;
        let js: String = row.get(1)?;
        let svc: String = row.get(2)?;
        let port_i: i64 = row.get(3)?;
        let conn: i64 = row.get(4)?;
        let rec: EvaluationRecord = serde_json::from_str(&js)
            .map_err(|e| anyhow!("bad record json: {e}"))?;

        buckets
            .entry((svc, port_i as u16, conn))
            .or_default()
            .push(rec);
    }

    // Assemble API-friendly groups
    let mut groups: Vec<RecordGroup> = Vec::with_capacity(keys.len());
    for k in keys {
        if let Some(recs) = buckets.remove(&(k.service_id.clone(), k.port, k.conn_id)) {
            groups.push(RecordGroup {
                service_id: k.service_id,
                port: k.port,
                conn_id: k.conn_id,
                first_ts_ns: k.first_ts as u64,
                last_ts_ns:  k.last_ts as u64,
                records: recs,
            });
        } else {
            // no records loaded for this key (shouldn’t happen)
            groups.push(RecordGroup {
                service_id: k.service_id,
                port: k.port,
                conn_id: k.conn_id,
                first_ts_ns: k.first_ts as u64,
                last_ts_ns:  k.last_ts as u64,
                records: Vec::new(),
            });
        }
    }

    // Cursor: use the last group’s first_ts (matches ordering)
    let next_since = if groups.is_empty() { since } else { (max_first + 1) };

    Ok((groups, next_since))
}
