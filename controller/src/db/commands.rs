use crate::db::util::{write_ecam_batch, write_records_batch};
use rusqlite::Connection;
use shared::types::ecam_types::EcamEvent;
use shared::types::record_types::EvaluationRecord;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct DbHandle {
    pub tx: mpsc::Sender<CtrlDbCmd>,
    pub path: Arc<String>,
}

pub enum CtrlDbCmd {
    InsertEcam(
        Vec<EcamEvent>,
        String /* svc */
    ),
    InsertRecords(
        Vec<EvaluationRecord>,
        String, /* svc */
        u16,    /* port */
    ),
    Shutdown,
}

// Spawn once at start-up
pub fn init(path: &str) -> anyhow::Result<DbHandle> {
    let (tx, mut rx) = mpsc::channel::<CtrlDbCmd>(8_192);
    let path = Arc::new(path.to_string());

    let path_clone = path.clone();

    tokio::spawn(async move {
        const DB_OPEN_COMMAND: &'static str = "db open";
        let mut conn = Connection::open(&*path_clone).expect(DB_OPEN_COMMAND);

        // tables setup (idempotent)
        conn.execute_batch(include_str!("schema.sql")).unwrap();

        // main database command loop
        while let Some(cmd) = rx.recv().await {
            match cmd {
                CtrlDbCmd::InsertEcam(events, svc) => {
                    let tx = conn.transaction().unwrap();
                    {
                        write_ecam_batch(&tx, &events, &svc);
                    }
                    tx.commit().ok();
                }
                CtrlDbCmd::InsertRecords(recs, svc, port) => {
                    let tx = conn.transaction().unwrap();
                    {
                        // prepared statements live inside helper, dropped before commit
                        write_records_batch(&tx, &recs, &svc, port);
                    }
                    tx.commit().ok();
                }
                CtrlDbCmd::Shutdown => break,
            }
        }
    });

    Ok(DbHandle { tx, path })
}
