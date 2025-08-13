use std::path::Path;
use rusqlite::{params, Connection};
use shared::types::base_types::Decision;
use shared::types::ecam_types::EcamEvent;
use shared::types::record_types::{DropReason, EvaluationRecord};
use rusqlite::Transaction;


pub fn open(path: &str) -> anyhow::Result<Connection> {
    let conn = Connection::open(Path::new(path))?;
    Ok(conn)
}

/// Slim row emitted for every atomic drop reason item we want to index.
struct DropReasonItemRow {
    reason_kind: &'static str, // "blacklist" | "whitelist_all_miss" | "whitelist_any_miss"
    item_type:   &'static str, // "filter_id" | "missing_id" | "hit_id"
    value:       String,
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
                        item_type:   "filter_id",
                        value:       id.clone(),
                    });
                }
            }
            DropReason::WhitelistAllMiss { missing_ids, .. } => {
                for id in missing_ids {
                    out.push(DropReasonItemRow {
                        reason_kind: "whitelist_all_miss",
                        item_type:   "missing_id",
                        value:       id.clone(),
                    });
                }
            }
            DropReason::WhitelistAnyMiss { hit_ids, .. } => {
                for id in hit_ids {
                    out.push(DropReasonItemRow {
                        reason_kind: "whitelist_any_miss",
                        item_type:   "hit_id",
                        value:       id.clone(),
                    });
                }
            }
        }
    }
    out
}

/// Insert a batch of ECAM events with a single transaction.
pub fn write_ecam_batch(tx: &Transaction, events: &[EcamEvent], svc: &str) {
    // tiny & simple: no need to pre-prepare, but we can
    let _ = tx.execute_batch("BEGIN");
    for ev in events {
        let _ = tx.execute(
            "INSERT OR IGNORE INTO ecam(ts_ns, service_id, json) VALUES(?,?,?)",
            params![ev.ts_ns as i64, svc, serde_json::to_string(ev).unwrap()],
        );
    }
    let _ = tx.execute_batch("END");
}

/// Insert a batch of EvaluationRecord rows + link tables (drop reasons, markers)
pub fn write_records_batch(tx: &Transaction, recs: &[EvaluationRecord], svc: &str, port: u16) {
    // Prepare once per batch
    let mut stmt_rec = tx
        .prepare(
            "INSERT OR IGNORE INTO records(
                timestamp_ns, service_id, port,
                inbound, decision, conn_id,
                http_method, http_path, http_status, http_length,
                json
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .unwrap();

    let mut stmt_dri = tx
        .prepare(
            "INSERT INTO drop_reason_items(
                timestamp_ns, service_id, port, reason_kind, item_type, value
             ) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .unwrap();

    let mut stmt_mark = tx
        .prepare(
            "INSERT OR IGNORE INTO record_markers(
                timestamp_ns, service_id, port, marker_id
             ) VALUES (?, ?, ?, ?)",
        )
        .unwrap();

    for r in recs {
        // derive fields
        let inbound_i   = if r.inbound { 1i64 } else { 0i64 };
        let decision_s  = match r.decision { Decision::Allow => "allow", Decision::Drop => "drop" };
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
            http_method,
            http_path,
            http_status,
            http_length,
            json
        ]);

        // drop reasons (only for drops)
        if matches!(r.decision, Decision::Drop) {
            for item in expand_drop_reasons(&r.drop_reasons) {
                let _ = stmt_dri.execute(params![
                    r.timestamp_ns as i64,
                    svc,
                    port as i64,
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
                m
            ]);
        }
    }
}
