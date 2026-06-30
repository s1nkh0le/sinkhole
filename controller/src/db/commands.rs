use crate::config::env::ServiceCfg;
use crate::db::util::{
    delete_service, prune_before, update_service_meta, upsert_service, write_ecam_batch,
    write_records_batch,
};
use crate::state::ctrl::ServiceMetadata;
use rusqlite::Connection;
use shared::types::ecam_types::EcamEvent;
use shared::types::record_types::EvaluationRecord;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

/// Reply channel for a write that the caller waits on before advancing its
/// cursor (confirmed / at-least-once delivery).
pub type WriteAck = oneshot::Sender<anyhow::Result<()>>;

#[derive(Debug, Clone)]
pub struct DbHandle {
    pub tx: mpsc::Sender<CtrlDbCmd>,
    pub path: Arc<String>,
}

pub enum CtrlDbCmd {
    InsertEcam(Vec<EcamEvent>, String /* svc */, WriteAck),
    InsertRecords(
        Vec<EvaluationRecord>,
        String, /* svc */
        u16,    /* port */
        WriteAck,
    ),
    /// Delete records/ecam older than this absolute ns timestamp.
    PruneBefore {
        cutoff_ns: i64,
    },
    /// Insert/refresh a tracked service (connection config + metadata).
    UpsertService {
        cfg: ServiceCfg,
        meta: ServiceMetadata,
        added_at_ns: i64,
    },
    /// Update only a service's display metadata.
    UpdateServiceMeta {
        id: String,
        meta: ServiceMetadata,
    },
    /// Remove a tracked service row.
    DeleteService {
        id: String,
    },
    Shutdown,
}

// Spawn once at start-up
pub fn init(path: &str) -> anyhow::Result<DbHandle> {
    let (tx, mut rx) = mpsc::channel::<CtrlDbCmd>(8_192);
    let path = Arc::new(path.to_string());

    // Open the owned connection and apply the schema synchronously, before this
    // function returns, so the tables exist before any read-path connection opens.
    // (Applying it inside the spawned task races a fresh-DB boot: a startup read
    // could open the DB first and hit `no such table`.) The connection then moves
    // into the actor task below.
    let mut conn = Connection::open(&*path)?;
    conn.execute_batch(include_str!("schema.sql"))?;

    tokio::spawn(async move {
        // main database command loop
        while let Some(cmd) = rx.recv().await {
            match cmd {
                CtrlDbCmd::InsertEcam(events, svc, ack) => {
                    // Confirmed write: commit, then report the result so the ECAM
                    // sync loop advances its cursor only on success.
                    let res: anyhow::Result<()> = (|| {
                        let tx = conn.transaction()?;
                        write_ecam_batch(&tx, &events, &svc);
                        tx.commit()?;
                        Ok(())
                    })();
                    let _ = ack.send(res);
                }
                CtrlDbCmd::InsertRecords(recs, svc, port, ack) => {
                    // Confirmed write: commit, then report the result so the
                    // records sync loop advances its cursor only on success
                    // (at-least-once; a failed write is retried from the same cursor).
                    let res: anyhow::Result<()> = (|| {
                        let tx = conn.transaction()?;
                        // prepared statements live inside helper, dropped before commit
                        write_records_batch(&tx, &recs, &svc, port);
                        tx.commit()?;
                        Ok(())
                    })();
                    let _ = ack.send(res);
                }
                CtrlDbCmd::PruneBefore { cutoff_ns } => {
                    let tx = conn.transaction().unwrap();
                    let removed = prune_before(&tx, cutoff_ns);
                    if tx.commit().is_ok() && removed > 0 {
                        tracing::info!(removed, cutoff_ns, "central DB pruned");
                    }
                }
                CtrlDbCmd::UpsertService {
                    cfg,
                    meta,
                    added_at_ns,
                } => {
                    upsert_service(&conn, &cfg, &meta, added_at_ns);
                }
                CtrlDbCmd::UpdateServiceMeta { id, meta } => {
                    update_service_meta(&conn, &id, &meta);
                }
                CtrlDbCmd::DeleteService { id } => {
                    delete_service(&conn, &id);
                }
                CtrlDbCmd::Shutdown => break,
            }
        }
    });

    Ok(DbHandle { tx, path })
}
