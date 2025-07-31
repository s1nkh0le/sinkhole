use once_cell::sync::Lazy;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::error;
use tracing::warn;
use crate::observability::sinks;
use crate::utils::now_ns;

const MAX_ECAM_EVENTS: usize = 2_000;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub enum Origin {
    Global,
    Port(u16),
    Matcher { port: u16, filter_id: String },
    Db,
    Telemetry,
}

#[derive(Debug, Clone, Serialize)]
pub struct EcamEvent {
    pub ts_ns: i64,
    pub severity: Severity,
    pub origin: Origin,
    pub msg: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct EcamSink {
    inner: Arc<RwLock<VecDeque<EcamEvent>>>,
}

pub static ECAM_SINK: Lazy<EcamSink> = Lazy::new(EcamSink::new);

impl EcamSink {
    fn new() -> Self {
        Self { inner: Arc::new(RwLock::new(VecDeque::new())) }
    }

    fn push(&self, ev: EcamEvent) {
        let mut guard = self.inner.write().unwrap();
        if guard.len() == MAX_ECAM_EVENTS {
            guard.pop_front(); // evict oldest
        }
        guard.push_back(ev);
    }

    /// Copy out all events newer than `after_ns` (inclusive).
    /// If `after_ns == 0` you get the full buffer (capped).
    pub fn snapshot(&self, after_ns: i64) -> Vec<EcamEvent> {
        let guard = self.inner.read().unwrap();
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
    let log_stdout = sinks::TARGETS.sinks_stdout.load(Ordering::Relaxed);;
    let push_sink = sinks::TARGETS.sinks_emit.load(Ordering::Relaxed);;
    
    if (!log_stdout && !push_sink) {
        return;
    }
    
    let ev = EcamEvent {
        ts_ns: now_ns() as i64,
        severity,
        origin,
        msg: msg.into(),
        data,
    };

    if (log_stdout) {
        match ev.severity {
            Severity::Warn => warn!(?ev.origin, "{}", ev.msg),
            Severity::Error => error!(?ev.origin, "{}", ev.msg),
        }
    }

    if (push_sink) {
        ECAM_SINK.push(ev);
    }
}
