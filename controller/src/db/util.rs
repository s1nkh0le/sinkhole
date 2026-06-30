use crate::config::env::ServiceCfg;
use crate::state::ctrl::ServiceMetadata;
use rusqlite::Transaction;
use rusqlite::{Connection, params};
use shared::types::base_types::Decision;
use shared::types::ecam_types::EcamEvent;
use shared::types::record_types::{DropReason, EvaluationRecord};
use std::path::Path;

pub fn open(path: &str) -> anyhow::Result<Connection> {
    let conn = Connection::open(Path::new(path))?;
    Ok(conn)
}

/// Slim row emitted for every atomic drop reason item we want to index.
struct DropReasonItemRow {
    reason_kind: &'static str, // "blacklist" | "whitelist_all_miss" | "whitelist_any_miss"
    item_type: &'static str,   // "filter_id" | "missing_id" | "hit_id"
    value: String,
}

/// Turn the enum into flat rows for the link table.
fn expand_drop_reasons(list: &[DropReason]) -> Vec<DropReasonItemRow> {
    let mut out = Vec::new();
    for dr in list {
        match dr {
            DropReason::Blacklist { filter_ids } => {
                for id in filter_ids {
                    out.push(DropReasonItemRow {
                        reason_kind: "blacklist",
                        item_type: "filter_id",
                        value: id.clone(),
                    });
                }
            }
            DropReason::WhitelistAllMiss { missing_ids, .. } => {
                for id in missing_ids {
                    out.push(DropReasonItemRow {
                        reason_kind: "whitelist_all_miss",
                        item_type: "missing_id",
                        value: id.clone(),
                    });
                }
            }
            DropReason::WhitelistAnyMiss { hit_ids, .. } => {
                for id in hit_ids {
                    out.push(DropReasonItemRow {
                        reason_kind: "whitelist_any_miss",
                        item_type: "hit_id",
                        value: id.clone(),
                    });
                }
            }
        }
    }
    out
}

/// Insert a batch of ECAM events. The caller owns the transaction (it calls
/// `conn.transaction()` then `tx.commit()`), so this must NOT issue its own
/// `BEGIN`/`END`: doing so committed the caller's transaction early and made the
/// outer `commit()` fail with "cannot commit - no transaction is active" (so ECAM
/// was never centralized).
pub fn write_ecam_batch(tx: &Transaction, events: &[EcamEvent], svc: &str) {
    for ev in events {
        let _ = tx.execute(
            "INSERT OR IGNORE INTO ecam(ts_ns, service_id, json) VALUES(?,?,?)",
            params![ev.ts_ns as i64, svc, serde_json::to_string(ev).unwrap()],
        );
    }
}

/// Insert a batch of EvaluationRecord rows + link tables (drop reasons, markers)
pub fn write_records_batch(tx: &Transaction, recs: &[EvaluationRecord], svc: &str, port: u16) {
    // Prepare once per batch
    let mut stmt_rec = tx
        .prepare(
            "INSERT OR IGNORE INTO records(
                timestamp_ns, service_id, port,
                inbound, decision, conn_id, message_seq,
                http_method, http_path, http_status, http_length, protocol,
                json
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .unwrap();

    let mut stmt_dri = tx
        .prepare(
            "INSERT OR IGNORE INTO record_drop_reasons(
                timestamp_ns, service_id, port, conn_id, inbound, message_seq,
                reason_kind, item_type, value
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .unwrap();

    let mut stmt_mark = tx
        .prepare(
            "INSERT OR IGNORE INTO record_markers(
                timestamp_ns, service_id, port, conn_id, inbound, message_seq, marker_id
             ) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .unwrap();

    for r in recs {
        // derive fields
        let inbound_i = if r.inbound { 1i64 } else { 0i64 };
        let decision_s = match r.decision {
            Decision::Allow => "allow",
            Decision::Drop => "drop",
        };
        let (http_method, http_path, http_status, http_length) = match &r.http {
            Some(h) => (
                h.method.as_deref(),
                h.path.as_deref(),
                h.status.map(|x| x as i64),
                Some(h.body_len as i64),
            ),
            None => (None, None, None, None),
        };

        // NOTE: keep your current storage behavior (full JSON):
        let json = serde_json::to_string(r).unwrap();

        // parent row
        let _ = stmt_rec.execute(params![
            r.timestamp_ns as i64,
            svc,
            port as i64,
            inbound_i,
            decision_s,
            r.conn_id as i64,
            r.message_seq as i64,
            http_method,
            http_path,
            http_status,
            http_length,
            r.protocol.as_str(),
            json
        ]);

        // drop reasons (only for drops)
        if matches!(r.decision, Decision::Drop) {
            for item in expand_drop_reasons(&r.drop_reasons) {
                let _ = stmt_dri.execute(params![
                    r.timestamp_ns as i64,
                    svc,
                    port as i64,
                    r.conn_id as i64,
                    inbound_i,
                    r.message_seq as i64,
                    item.reason_kind,
                    item.item_type,
                    item.value
                ]);
            }
        }

        // markers
        for m in &r.markers {
            let _ = stmt_mark.execute(params![
                r.timestamp_ns as i64,
                svc,
                port as i64,
                r.conn_id as i64,
                inbound_i,
                r.message_seq as i64,
                m
            ]);
        }
    }
}

/// Delete all records (and their link rows) and ecam events older than `cutoff_ns`.
/// Returns the number of `records` rows removed.
pub fn prune_before(tx: &Transaction, cutoff_ns: i64) -> usize {
    let _ = tx.execute(
        "DELETE FROM record_markers WHERE timestamp_ns < ?",
        params![cutoff_ns],
    );
    let _ = tx.execute(
        "DELETE FROM record_drop_reasons WHERE timestamp_ns < ?",
        params![cutoff_ns],
    );
    let removed = tx
        .execute(
            "DELETE FROM records WHERE timestamp_ns < ?",
            params![cutoff_ns],
        )
        .unwrap_or(0);
    let _ = tx.execute("DELETE FROM ecam WHERE ts_ns < ?", params![cutoff_ns]);
    removed
}

/// One persisted service: connection config + display metadata.
pub struct PersistedService {
    pub cfg: ServiceCfg,
    pub meta: ServiceMetadata,
}

/// Defensive `CREATE TABLE IF NOT EXISTS` for the `services` table, mirroring
/// `schema.sql`. Used by the sync readers/writers in case the async schema-setup
/// task hasn't run yet on a fresh DB.
const SERVICES_DDL: &str = "CREATE TABLE IF NOT EXISTS services (
    id          TEXT PRIMARY KEY,
    base_url    TEXT    NOT NULL,
    token       TEXT    NOT NULL,
    name        TEXT,
    description TEXT,
    hex_color   TEXT,
    icon        TEXT,
    added_at_ns INTEGER NOT NULL
);";

/// Insert a service or, if its id already exists, update everything **except**
/// `added_at_ns` (which records when it was first seen).
pub fn upsert_service(
    conn: &Connection,
    cfg: &ServiceCfg,
    meta: &ServiceMetadata,
    added_at_ns: i64,
) {
    let _ = conn.execute(
        "INSERT INTO services(id, base_url, token, name, description, hex_color, icon, added_at_ns)
         VALUES(?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
            base_url    = excluded.base_url,
            token       = excluded.token,
            name        = excluded.name,
            description = excluded.description,
            hex_color   = excluded.hex_color,
            icon        = excluded.icon",
        params![
            cfg.id,
            cfg.base_url,
            cfg.token,
            meta.name,
            meta.description,
            meta.hex_color,
            meta.icon,
            added_at_ns
        ],
    );
}

/// Update only the display-metadata columns of an existing service row.
pub fn update_service_meta(conn: &Connection, id: &str, meta: &ServiceMetadata) {
    let _ = conn.execute(
        "UPDATE services SET
            name        = ?,
            description = ?,
            hex_color   = ?,
            icon        = ?
         WHERE id = ?",
        params![meta.name, meta.description, meta.hex_color, meta.icon, id],
    );
}

/// Delete a service row by id.
pub fn delete_service(conn: &Connection, id: &str) {
    let _ = conn.execute("DELETE FROM services WHERE id = ?", params![id]);
}

/// Load the whole persisted fleet (connection config + metadata).
pub fn read_all_services(path: &str) -> anyhow::Result<Vec<PersistedService>> {
    let conn = Connection::open(Path::new(path))?;
    conn.execute_batch(SERVICES_DDL)?;

    let mut stmt = conn
        .prepare("SELECT id, base_url, token, name, description, hex_color, icon FROM services")?;
    let rows = stmt.query_map([], |row| {
        Ok(PersistedService {
            cfg: ServiceCfg {
                id: row.get(0)?,
                base_url: row.get(1)?,
                token: row.get(2)?,
            },
            meta: ServiceMetadata {
                name: row.get(3)?,
                description: row.get(4)?,
                hex_color: row.get(5)?,
                icon: row.get(6)?,
            },
        })
    })?;

    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}
