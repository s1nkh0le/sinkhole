use crate::config::Config;
use crate::db::commands::DbHandle;
use std::sync::atomic::AtomicBool;

pub mod sink_ecam;
pub mod sink_telemetry;

struct TelemetryTargets {
    sinks_stdout: AtomicBool,
    sinks_emit: AtomicBool,
}

const TARGETS: TelemetryTargets = TelemetryTargets {
    sinks_stdout: AtomicBool::new(true),
    sinks_emit: AtomicBool::new(true),
};

pub fn init_all(cfg: &Config, db_handle: DbHandle) {
    // fixme
    TARGETS.sinks_stdout = AtomicBool::new(cfg.sinks_stdout);
    TARGETS.sinks_emit = AtomicBool::new(cfg.sinks_emit);

    // Telemetry
    sink_telemetry::init_db_sink(db_handle);

    // Ecam
    // ...
}
