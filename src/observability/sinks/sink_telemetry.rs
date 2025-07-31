use crate::db::actor::{DbCmd, DbHandle};
use crate::observability::eval_record::EvaluationRecord;
use crate::observability::sinks;
use once_cell::sync::OnceCell;

static DB_SINK: OnceCell<DbHandle> = OnceCell::new();

pub fn init_db_sink(handle: DbHandle) {
    DB_SINK.set(handle).ok();          // ignore double-set
}

fn push_to_db(rec: &EvaluationRecord) {
    if let Some(db) = DB_SINK.get() {
        let _ = db.tx.try_send(DbCmd::InsertRecord(rec.clone()));
    }
}

pub fn record(rec: &EvaluationRecord) {
    let log_stdout = sinks::TARGETS.sinks_stdout.load(std::sync::atomic::Ordering::Relaxed);
    let push_db = sinks::TARGETS.sinks_emit.load(std::sync::atomic::Ordering::Relaxed);;

    if (log_stdout) {
        println!("{}", serde_json::to_string(rec).unwrap());
    }

    if (push_db) {
        push_to_db(rec);
    }
}
