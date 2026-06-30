use crate::observability::sinks;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
pub(crate) use shared::types::ecam_types::{EcamEvent, Origin, Severity};
use shared::utils::time::now_ns;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tracing::error;
use tracing::warn;

const MAX_ECAM_EVENTS: usize = 2_000;

#[derive(Debug, Clone)]
pub struct EcamSink {
    inner: Arc<RwLock<VecDeque<EcamEvent>>>,
}

pub static ECAM_SINK: Lazy<EcamSink> = Lazy::new(EcamSink::new);

impl EcamSink {
    fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(VecDeque::new())),
        }
    }

    fn push(&self, ev: EcamEvent) {
        let mut guard = self.inner.write();
        if guard.len() == MAX_ECAM_EVENTS {
            guard.pop_front(); // evict oldest
        }
        guard.push_back(ev);
    }

    /// Copy out all events newer than `after_ns` (inclusive).
    /// If `after_ns == 0` you get the full buffer (capped).
    pub fn snapshot(&self, after_ns: u64) -> Vec<EcamEvent> {
        let guard = self.inner.read();
        guard
            .iter()
            .filter(|e| e.ts_ns >= after_ns)
            .cloned()
            .collect()
    }
}

pub fn record(
    severity: Severity,
    origin: Origin,
    msg: impl Into<String>,
    data: Option<serde_json::Value>,
) {
    let log_stdout = sinks::TARGETS.sinks_stdout.load(Ordering::Relaxed);
    let push_sink = sinks::TARGETS.sinks_emit.load(Ordering::Relaxed);

    if !log_stdout && !push_sink {
        return;
    }

    let ev = EcamEvent {
        ts_ns: now_ns(),
        severity,
        origin,
        msg: msg.into(),
        data,
    };

    if log_stdout {
        match ev.severity {
            Severity::Warn => warn!("{}", ev.msg),
            Severity::Error => error!("{}", ev.msg),
        }
    }

    if push_sink {
        ECAM_SINK.push(ev);
    }
}
