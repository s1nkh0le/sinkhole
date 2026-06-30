use crate::config::env::ServiceCfg;
use crate::db::commands::{CtrlDbCmd, DbHandle};
use crate::state::ctrl::ServiceMetadata;
use anyhow::anyhow;
use shared::types::ecam_types::EcamEvent;
use shared::types::record_types::EvaluationRecord;
use tokio::sync::oneshot;

// Confirmed-write helpers: they resolve only once the DB actor has committed (or
// failed) the batch, so callers can gate their cursor on the result.
pub async fn insert_ecam(db: &DbHandle, svc: &str, v: &[EcamEvent]) -> anyhow::Result<()> {
    let (ack, rx) = oneshot::channel();
    db.tx
        .send(CtrlDbCmd::InsertEcam(v.to_vec(), svc.into(), ack))
        .await
        .map_err(|_| anyhow!("db actor channel closed"))?;
    rx.await
        .map_err(|_| anyhow!("db actor dropped the write ack"))?
}
pub async fn insert_records(
    db: &DbHandle,
    svc: &str,
    port: u16,
    v: &[EvaluationRecord],
) -> anyhow::Result<()> {
    let (ack, rx) = oneshot::channel();
    db.tx
        .send(CtrlDbCmd::InsertRecords(v.to_vec(), svc.into(), port, ack))
        .await
        .map_err(|_| anyhow!("db actor channel closed"))?;
    rx.await
        .map_err(|_| anyhow!("db actor dropped the write ack"))?
}

pub async fn prune_before(db: &DbHandle, cutoff_ns: i64) {
    let _ = db.tx.send(CtrlDbCmd::PruneBefore { cutoff_ns }).await;
}

/// Insert/refresh a tracked service (connection config + metadata).
pub async fn persist_service(db: &DbHandle, cfg: &ServiceCfg, meta: &ServiceMetadata) {
    let _ = db
        .tx
        .send(CtrlDbCmd::UpsertService {
            cfg: cfg.clone(),
            meta: meta.clone(),
            added_at_ns: shared::utils::time::now_ns() as i64,
        })
        .await;
}

/// Update only a service's display metadata (the row must already exist).
pub async fn persist_service_meta(db: &DbHandle, svc: &str, meta: &ServiceMetadata) {
    let _ = db
        .tx
        .send(CtrlDbCmd::UpdateServiceMeta {
            id: svc.to_string(),
            meta: meta.clone(),
        })
        .await;
}

/// Remove a tracked service row.
pub async fn delete_service_row(db: &DbHandle, id: &str) {
    let _ = db
        .tx
        .send(CtrlDbCmd::DeleteService { id: id.to_string() })
        .await;
}
