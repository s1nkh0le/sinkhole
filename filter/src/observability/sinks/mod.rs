use crate::config::Config;
use crate::db::commands::DbHandle;
use std::sync::atomic::{AtomicBool, Ordering};

pub mod sink_ecam;
pub mod sink_telemetry;

/// Process-wide toggles for the telemetry/ECAM sinks, read on every record by
/// `sink_telemetry::record` and `sink_ecam::record`. Stored as atomics so they
/// can be set once at start-up (and, in principle, flipped at runtime) without
/// locking. Must be a `static`, not a `const`: a `const` is copied at each use
/// site, so writes to it would silently no-op.
pub(crate) struct TelemetryTargets {
    pub(crate) sinks_stdout: AtomicBool,
    pub(crate) sinks_emit: AtomicBool,
    /// Whether the process is in debug mode. Per-message *evaluation records* are
    /// only echoed to stdout when this is set (they are high-volume traffic spam
    /// in production); ECAM operational events ignore it and follow `sinks_stdout`.
    pub(crate) debug: AtomicBool,
}

pub(crate) static TARGETS: TelemetryTargets = TelemetryTargets {
    sinks_stdout: AtomicBool::new(true),
    sinks_emit: AtomicBool::new(true),
    debug: AtomicBool::new(false),
};

pub fn init_all(cfg: &Config, db_handle: DbHandle) {
    TARGETS
        .sinks_stdout
        .store(cfg.sinks_stdout, Ordering::Relaxed);
    TARGETS.sinks_emit.store(cfg.sinks_emit, Ordering::Relaxed);
    TARGETS.debug.store(cfg.debug, Ordering::Relaxed);

    sink_telemetry::init_db_sink(db_handle);
}
