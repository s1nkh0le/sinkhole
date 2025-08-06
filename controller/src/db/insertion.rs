use crate::db::{Cmd, DbHandle};
use shared::types::record_types::EvaluationRecord;
use shared::types::ecam_types::EcamEvent;

// async helpers used by puller
pub async fn insert_ecam(db: &DbHandle, svc: &str, v: &[EcamEvent]) {
    let _ = db.tx.send(Cmd::InsertEcam(v.to_vec(), svc.into())).await;
}
pub async fn insert_records(db: &DbHandle, svc: &str, port: u16, v: &[EvaluationRecord]) {
    let _ = db
        .tx
        .send(Cmd::InsertRecords(v.to_vec(), svc.into(), port))
        .await;
}
