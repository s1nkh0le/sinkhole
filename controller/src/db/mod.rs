pub mod boot;
pub mod insertion;
pub mod query;

use anyhow::Result;
use rusqlite::{params, Connection};
use shared::types::base_types::Decision;
use shared::types::ecam_types::EcamEvent;
use shared::types::record_types::{DropReason, EvaluationRecord};
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct DbHandle {
    pub tx: mpsc::Sender<Cmd>,
    pub path: Arc<String>,
}

pub enum Cmd {
    InsertEcam(Vec<EcamEvent>, String /* svc */),
    InsertRecords(
        Vec<EvaluationRecord>,
        String, /* svc */
        u16,    /* port */
    ),
    Shutdown,
}

// Spawn once at start-up
pub fn init(path: &str) -> Result<DbHandle> {
    let (tx, mut rx) = mpsc::channel::<Cmd>(8_192);
    let path = Arc::new(path.to_string());

    let path_clone = path.clone();
    tokio::spawn(async move {
        let mut conn = Connection::open(&*path_clone).expect("db open");

        // tables
        conn.execute_batch(include_str!("schema.sql")).unwrap();

        // main loop
        while let Some(cmd) = rx.recv().await {
            match cmd {
                Cmd::InsertEcam(events, svc) => {
                    let tx = conn.transaction().unwrap();
                    for ev in events {
                        let _ = tx.execute(
                            "INSERT OR IGNORE INTO ecam(ts_ns,service_id,json) VALUES(?,?,?)",
                            params![ev.ts_ns, svc, serde_json::to_string(&ev).unwrap()],
                        );
                    }
                    tx.commit().ok();
                }
                Cmd::InsertRecords(recs, svc, port) => {
                    let tx = conn.transaction().unwrap();

                    {
                        // 1) Prepare statements once per batch
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
                                    timestamp_ns, service_id, port,
                                    reason_kind, item_type, value
                                ) VALUES (?, ?, ?, ?, ?, ?)",
                            )
                            .unwrap();

                        for r in recs {
                            // ---- derive fields ------------------------------------------------
                            let inbound_i = if r.inbound { 1 } else { 0 };
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

                            // (Optional) strip raw payload from the stored JSON to save space:
                            // let mut slim = r.clone();
                            // slim.raw_b64.clear();
                            // let json = serde_json::to_string(&slim).unwrap();
                            let json = serde_json::to_string(&r).unwrap();

                            // ---- insert parent row -------------------------------------------
                            let _ = stmt_rec.execute(params![
                                r.timestamp_ns as i64,
                                &svc,
                                port as i64,
                                inbound_i,
                                decision_s,
                                r.conn_id as i64,
                                http_method, // Option<&str> is fine
                                http_path,
                                http_status, // Option<i64>
                                http_length, // Option<i64>
                                json
                            ]);

                            // ---- explode drop reasons into link rows --------------------------
                            if decision_s == "drop" {
                                for dr in &r.drop_reasons {
                                    match dr {
                                        DropReason::Blacklist { filter_ids } => {
                                            for id in filter_ids {
                                                let _ = stmt_dri.execute(params![
                                                    r.timestamp_ns as i64,
                                                    &svc,
                                                    port as i64,
                                                    "blacklist",
                                                    "filter_id",
                                                    id
                                                ]);
                                            }
                                        }
                                        DropReason::WhitelistAllMiss { missing_ids, .. } => {
                                            for id in missing_ids {
                                                let _ = stmt_dri.execute(params![
                                                    r.timestamp_ns as i64,
                                                    &svc,
                                                    port as i64,
                                                    "whitelist_all_miss",
                                                    "missing_id",
                                                    id
                                                ]);
                                            }
                                        }
                                        DropReason::WhitelistAnyMiss { hit_ids, .. } => {
                                            for id in hit_ids {
                                                let _ = stmt_dri.execute(params![
                                                    r.timestamp_ns as i64,
                                                    &svc,
                                                    port as i64,
                                                    "whitelist_any_miss",
                                                    "hit_id",
                                                    id
                                                ]);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    tx.commit().ok();
                }
                Cmd::Shutdown => break,
            }
        }
    });

    Ok(DbHandle { tx, path })
}
