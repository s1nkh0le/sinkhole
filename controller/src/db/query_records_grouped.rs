use crate::api::api_records_grouped::RecordGroup;
use crate::db::util::open;
use anyhow::{Result, anyhow};
use rusqlite::{ToSql, params_from_iter};
use shared::types::record_types::EvaluationRecord;
use std::collections::HashMap;
use std::fmt::Write as _;

/// Result of a grouped query: the groups (newest-or-oldest first per `asc`) plus
/// a direction-aware cursor `(next_since, next_until)`:
///
///   * page *forward* in time (asc browse)  → pass `next_since` back as `since`
///   * page *backward* in time (desc browse) → pass `next_until` back as `until`
///
/// On an empty result both carry the requested bounds unchanged.
pub struct GroupedPage {
    pub groups: Vec<RecordGroup>,
    pub next_since: i64,
    pub next_until: i64,
}

/// Returns connection groups `(service_id, port, conn_id)` with their records.
///
/// `limit_groups` bounds the number of **groups** (connections), not records.
/// `record_limit_per_group`, when set, caps how many (earliest) records each
/// group returns — a safety valve against a single chatty connection dominating
/// the payload; `None` returns the whole connection.
///
/// One statement: a `matches` CTE selects the qualifying group keys (applying all
/// record-level filters), then we JOIN `records` back on the key — which is a
/// prefix of the table's primary key, so it's an index seek. (This replaces the
/// old two-phase approach that round-tripped the keys to Rust and rebuilt them
/// into a giant `OR` list.)
#[allow(clippy::too_many_arguments)]
pub fn load_records_grouped_agg_advanced(
    db_path: &str,
    service_id: Option<&str>,
    port: Option<u16>,
    since: i64,
    until: i64,
    limit_groups: usize,
    asc: bool,
    record_limit_per_group: Option<i64>,

    positive_markers: &[String],
    negative_markers: &[String],
    inbound: Option<bool>,
    decision: Option<&str>, // "allow" | "drop"
    conn_id: Option<i64>,
    conn_id_min: Option<i64>,
    conn_id_max: Option<i64>,

    method: Option<&str>,
    path_like: Option<&str>, // already wrapped with %...%
    status_eq: Option<i64>,
    status_min: Option<i64>,
    status_max: Option<i64>,

    len: Option<i64>,
    len_min: Option<i64>,
    len_max: Option<i64>,
    protocols: &[String],

    // message-count-per-connection filter (HAVING on the matched records in a group)
    msg_count: Option<i64>,
    msg_count_min: Option<i64>,
    msg_count_max: Option<i64>,

    drop_kind: Option<&str>, // e.g. "blacklist", "whitelist_all_miss", "whitelist_any_miss"
    drop_item_type: Option<&str>, // e.g. "filter_id", "missing_id", "hit_id"
    drop_value: Option<&str>,
) -> Result<GroupedPage> {
    let conn = open(db_path)?;

    let need_drop_join = drop_kind.is_some() || drop_item_type.is_some() || drop_value.is_some();

    // De-dup positive markers; one JOIN per distinct id.
    let pos_markers: Vec<&str> = {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        positive_markers
            .iter()
            .map(|s| s.as_str())
            .filter(|m| seen.insert((*m).to_owned()))
            .collect()
    };
    let need_neg = !negative_markers.is_empty();
    let dir = if asc { "ASC" } else { "DESC" };

    // ─── matches CTE: which group keys qualify, ordered by first-seen ──────────
    let mut sql = String::new();
    let mut binds: Vec<Box<dyn ToSql>> = Vec::new();

    sql.push_str(
        "WITH matches AS (\
           SELECT r.service_id, r.port, r.conn_id, \
                  MIN(r.timestamp_ns) AS first_ts, \
                  MAX(r.timestamp_ns) AS last_ts \
           FROM records r",
    );

    if need_drop_join {
        sql.push_str(
            " JOIN record_drop_reasons d
                ON d.service_id   = r.service_id
               AND d.port         = r.port
               AND d.conn_id      = r.conn_id
               AND d.inbound      = r.inbound
               AND d.message_seq  = r.message_seq",
        );
    }
    for (i, _) in pos_markers.iter().enumerate() {
        let _ = write!(
            &mut sql,
            " JOIN record_markers m{idx}
                ON m{idx}.service_id   = r.service_id
               AND m{idx}.port         = r.port
               AND m{idx}.conn_id      = r.conn_id
               AND m{idx}.inbound      = r.inbound
               AND m{idx}.message_seq  = r.message_seq
               AND m{idx}.marker_id    = ?",
            idx = i
        );
    }
    for m in &pos_markers {
        binds.push(Box::new((*m).to_string()));
    }

    sql.push_str(" WHERE r.timestamp_ns > ? AND r.timestamp_ns <= ?");
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
    } else {
        if let Some(mi) = conn_id_min {
            sql.push_str(" AND r.conn_id >= ?");
            binds.push(Box::new(mi));
        }
        if let Some(ma) = conn_id_max {
            sql.push_str(" AND r.conn_id <= ?");
            binds.push(Box::new(ma));
        }
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

    if !protocols.is_empty() {
        let placeholders = std::iter::repeat_n("?", protocols.len())
            .collect::<Vec<_>>()
            .join(", ");
        let _ = write!(&mut sql, " AND r.protocol IN ({placeholders})");
        for p in protocols {
            binds.push(Box::new(p.clone()));
        }
    }

    if need_neg {
        sql.push_str(
            " AND NOT EXISTS (
                SELECT 1 FROM record_markers mneg
                 WHERE mneg.service_id   = r.service_id
                   AND mneg.port         = r.port
                   AND mneg.conn_id      = r.conn_id
                   AND mneg.inbound      = r.inbound
                   AND mneg.message_seq  = r.message_seq
                   AND mneg.marker_id IN (",
        );
        let placeholders = std::iter::repeat_n("?", negative_markers.len())
            .collect::<Vec<_>>()
            .join(", ");
        sql.push_str(&placeholders);
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

    // message-count-per-connection filter: HAVING on the count of records that
    // matched the (record-level) predicates in each group. With no other record
    // filters this is just the connection's message count.
    if let Some(exact) = msg_count {
        sql.push_str(" HAVING COUNT(*) = ?");
        binds.push(Box::new(exact));
    } else if msg_count_min.is_some() || msg_count_max.is_some() {
        let mut clauses: Vec<&str> = Vec::new();
        if msg_count_min.is_some() {
            clauses.push("COUNT(*) >= ?");
        }
        if msg_count_max.is_some() {
            clauses.push("COUNT(*) <= ?");
        }
        let _ = write!(&mut sql, " HAVING {}", clauses.join(" AND "));
        if let Some(mi) = msg_count_min {
            binds.push(Box::new(mi));
        }
        if let Some(ma) = msg_count_max {
            binds.push(Box::new(ma));
        }
    }

    let _ = write!(&mut sql, " ORDER BY first_ts {dir} LIMIT ?)");
    binds.push(Box::new(limit_groups as i64));

    // ─── join records back to the matched groups (index seek on the PK prefix) ─
    // Records within a group are always returned oldest→newest; groups keep the
    // CTE's first_ts ordering. With a per-group cap we rank rows and keep the
    // earliest N.
    if let Some(cap) = record_limit_per_group {
        sql.push_str(
            " SELECT timestamp_ns, json, service_id, port, conn_id, first_ts, last_ts FROM (\
               SELECT r.timestamp_ns AS timestamp_ns, r.json AS json, \
                      m.service_id AS service_id, m.port AS port, m.conn_id AS conn_id, \
                      m.first_ts AS first_ts, m.last_ts AS last_ts, \
                      ROW_NUMBER() OVER (\
                        PARTITION BY m.service_id, m.port, m.conn_id \
                        ORDER BY r.timestamp_ns ASC) AS rn \
               FROM matches m \
               JOIN records r \
                 ON r.service_id = m.service_id AND r.port = m.port AND r.conn_id = m.conn_id) \
             WHERE rn <= ?",
        );
        binds.push(Box::new(cap));
        let _ = write!(
            &mut sql,
            " ORDER BY first_ts {dir}, service_id, port, conn_id, timestamp_ns ASC"
        );
    } else {
        sql.push_str(
            " SELECT r.timestamp_ns, r.json, m.service_id, m.port, m.conn_id, \
                     m.first_ts, m.last_ts \
              FROM matches m \
              JOIN records r \
                ON r.service_id = m.service_id AND r.port = m.port AND r.conn_id = m.conn_id",
        );
        let _ = write!(
            &mut sql,
            " ORDER BY m.first_ts {dir}, m.service_id, m.port, m.conn_id, r.timestamp_ns ASC"
        );
    }

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(binds.iter()))?;

    // Assemble groups in encounter order (== first_ts order from the query).
    let mut groups: Vec<RecordGroup> = Vec::new();
    let mut index: HashMap<(String, u16, i64), usize> = HashMap::new();
    let mut min_first = i64::MAX;
    let mut max_first = i64::MIN;

    while let Some(row) = rows.next()? {
        // col 0 (timestamp_ns) drives SQL ordering; not needed Rust-side.
        let js: String = row.get(1)?;
        let svc: String = row.get(2)?;
        let port_i: i64 = row.get(3)?;
        let conn_i: i64 = row.get(4)?;
        let first_ts: i64 = row.get(5)?;
        let last_ts: i64 = row.get(6)?;

        let rec: EvaluationRecord =
            serde_json::from_str(&js).map_err(|e| anyhow!("bad record json: {e}"))?;

        let key = (svc.clone(), port_i as u16, conn_i);
        let gi = *index.entry(key).or_insert_with(|| {
            min_first = min_first.min(first_ts);
            max_first = max_first.max(first_ts);
            groups.push(RecordGroup {
                service_id: svc,
                port: port_i as u16,
                conn_id: conn_i,
                first_ts_ns: first_ts as u64,
                last_ts_ns: last_ts as u64,
                records: Vec::new(),
            });
            groups.len() - 1
        });
        groups[gi].records.push(rec);
    }

    let (next_since, next_until) = if groups.is_empty() {
        (since, until)
    } else {
        (max_first + 1, (min_first - 1).max(0))
    };

    Ok(GroupedPage {
        groups,
        next_since,
        next_until,
    })
}
