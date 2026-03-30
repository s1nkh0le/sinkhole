use crate::config::Config;
use crate::db::actor::DbHandle;
use std::sync::atomic::{AtomicBool, Ordering};

pub mod sink_ecam;
pub mod sink_telemetry;

pub(crate) struct TelemetryTargets {
    sinks_stdout: AtomicBool,
    sinks_emit: AtomicBool,
}

pub(crate) static TARGETS: TelemetryTargets = TelemetryTargets {
    sinks_stdout: AtomicBool::new(true),
    sinks_emit: AtomicBool::new(true),
};

pub fn init_all(cfg: &Config, db_handle: DbHandle) {
    TARGETS.sinks_stdout.store(cfg.sinks_stdout, Ordering::Relaxed);
    TARGETS.sinks_emit.store(cfg.sinks_emit, Ordering::Relaxed);

    // Telemetry
    sink_telemetry::init_db_sink(db_handle);

    // Ecam
    // ...
}
