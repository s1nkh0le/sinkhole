pub mod aggregate;
pub mod boot;
pub mod insertion;
pub mod query;

use anyhow::Result;
use rusqlite::{params, Connection};
use shared::eval_record::EvaluationRecord;
use shared::sink_model::EcamEvent;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct DbHandle {
    pub tx: mpsc::Sender<Cmd>,
    pub path: Arc<String>,
}

pub enum Cmd {
    InsertEcam(Vec<EcamEvent>, String /* svc */),
    InsertLogs(
        Vec<EvaluationRecord>,
        String, /* svc */
        u16,    /* port */
    ),
    Shutdown,
}

// Spawn once at start-up
pub fn init(path: &str) -> Result<DbHandle> {
    let (tx, mut rx) = mpsc::channel::<Cmd>(64);
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
                Cmd::InsertLogs(recs, svc, port) => {
                    let tx = conn.transaction().unwrap();
                    for r in recs {
                        let _ = tx.execute(
                            "INSERT OR IGNORE INTO records(timestamp_ns,service_id,port,json) \
                             VALUES(?,?,?,?)",
                            params![
                                r.timestamp_ns as i64,
                                svc,
                                port,
                                serde_json::to_string(&r).unwrap()
                            ],
                        );
                    }
                    tx.commit().ok();
                }
                Cmd::Shutdown => break,
            }
        }
    });

    Ok(DbHandle { tx, path })
}
