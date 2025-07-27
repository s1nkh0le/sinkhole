use once_cell::sync::OnceCell;
use crate::db::actor::{DbCmd, DbHandle};
use crate::observability::eval_record::EvaluationRecord;

static DB_SINK: OnceCell<DbHandle> = OnceCell::new();

pub fn init_db_sink(handle: DbHandle) {
    DB_SINK.set(handle).ok(); // ignore if double-set
}

pub fn emit_record(rec: &EvaluationRecord) {
    println!("{}", serde_json::to_string(rec).unwrap());
    if let Some(db) = DB_SINK.get() {
        let _ = db.tx.try_send(DbCmd::InsertRecord(rec.clone()));
    }
}
