use crate::db::commands::{DbHandle, FilterDbCmd};
use crate::observability::sinks;
use once_cell::sync::OnceCell;
use shared::types::record_types::EvaluationRecord;

static DB_SINK: OnceCell<DbHandle> = OnceCell::new();

pub fn init_db_sink(handle: DbHandle) {
    DB_SINK.set(handle).ok(); // ignore double-set
}

fn push_to_db(rec: &EvaluationRecord) {
    if let Some(db) = DB_SINK.get() {
        let _ = db.tx.try_send(FilterDbCmd::InsertRecord(rec.clone()));
    }
}

pub fn record(rec: &EvaluationRecord) {
    use std::sync::atomic::Ordering::Relaxed;
    // Per-message records are high-volume traffic spam: only echo them to stdout
    // when stdout sinks are on *and* we're in debug mode. ECAM operational events
    // (sink_ecam) still follow `sinks_stdout` alone. Either way they go to the DB.
    let log_stdout =
        sinks::TARGETS.sinks_stdout.load(Relaxed) && sinks::TARGETS.debug.load(Relaxed);
    let push_db = sinks::TARGETS.sinks_emit.load(Relaxed);

    if log_stdout {
        // Best-effort stdout echo; never let a serialization hiccup panic the proxy.
        if let Ok(json) = serde_json::to_string(rec) {
            println!("{json}");
        }
    }

    if push_db {
        push_to_db(rec);
    }
}
