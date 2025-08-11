use serde::{Serialize, Deserialize};
use utoipa::ToSchema;
use std::collections::HashMap;
use rand::{rng, Rng};
use shared::utils::time::now_ns;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, Eq, PartialEq, Hash)]
pub enum HealthLayer {
    Probe,
    StateSync,
    EcamPull,
    RecordsPull,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FaultInfo {
    pub failures: u32,
    pub backoff_until_ns: u64,
    pub last_error: Option<String>,
    pub last_attempt_ns: u64,
    pub last_success_ns: u64,
}

impl Default for FaultInfo {
    fn default() -> Self {
        Self {
            failures: 0,
            backoff_until_ns: 0,   // pollable immediately
            last_error: None,
            last_attempt_ns: 0,
            last_success_ns: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum Health {
    Healthy {
        last_sync_ns: u64
    },
    Degraded { faults: HashMap<HealthLayer, FaultInfo>, last_sync_ns: u64 },
}

impl Default for Health {
    fn default() -> Self {
        Health::Degraded {
            faults: HashMap::new(),
            last_sync_ns: 0,
        }
    }
}

// internal policy definition
#[derive(Debug, Clone, Copy)]
enum Backoff { Const { interval_ns: u64 }, Exp { max_exp: u32 } }

impl Health {
    // defaults
    const PROBE_CONST_INTERVAL_NS: u64 = 5 * 1_000_000_000; // 5s
    const HEAVY_EXP_MAX_EXP: u32 = 5; // capped at 2^5 = 32s

    fn backoff_for(layer: HealthLayer) -> Backoff {
        match layer {
            HealthLayer::Probe      => Backoff::Const { interval_ns: Self::PROBE_CONST_INTERVAL_NS },
            HealthLayer::StateSync  => Backoff::Exp   { max_exp: Self::HEAVY_EXP_MAX_EXP },
            HealthLayer::EcamPull   => Backoff::Exp   { max_exp: Self::HEAVY_EXP_MAX_EXP },
            HealthLayer::RecordsPull=> Backoff::Exp   { max_exp: Self::HEAVY_EXP_MAX_EXP },
        }
    }

    pub fn is_healthy(&self) -> bool { matches!(self, Health::Healthy { .. }) }


    pub fn last_sync_ns(&self) -> u64 {
        match self {
            Health::Healthy { last_sync_ns, .. } => *last_sync_ns,
            Health::Degraded { last_sync_ns, .. } => *last_sync_ns
        }
    }


    pub fn can_poll(&self, layer: HealthLayer) -> bool {
        match self {
            Health::Healthy { .. } => true,
            Health::Degraded { faults, .. } => {
                let now = now_ns();
                faults.get(&layer).map(|fi| now >= fi.backoff_until_ns).unwrap_or(true)
            }
        }
    }

    pub fn next_allowed_ns(&self, layer: HealthLayer) -> u64 {
        match self {
            Health::Healthy { .. } => 0,
            Health::Degraded { faults, .. } => faults.get(&layer).map(|f| f.backoff_until_ns).unwrap_or(0),
        }
    }

    /// Pure (non-mutating) helpers
    pub fn record_success(self, layer: HealthLayer) -> Self {
        let now = now_ns();
        match self {
            Health::Healthy { .. } => Health::Healthy { last_sync_ns: now },
            Health::Degraded { mut faults, .. } => {
                if let Some(fi) = faults.get_mut(&layer) { fi.last_success_ns = now; }
                faults.remove(&layer);
                if faults.is_empty() {
                    Health::Healthy { last_sync_ns: now }
                } else {
                    Health::Degraded { faults, last_sync_ns: now }
                }
            }
        }
    }

    pub fn record_failure(self, layer: HealthLayer, last_error: Option<String>) -> Self {
        let now = now_ns();

        // normalize to get an editable map + current last_sync_ns
        let (mut faults, last_sync_ns) = match self {
            Health::Healthy { last_sync_ns } => (HashMap::new(), last_sync_ns),
            Health::Degraded { faults, last_sync_ns } => (faults, last_sync_ns),
        };

        let mut fi = faults.remove(&layer).unwrap_or(FaultInfo {
            failures: 0,
            backoff_until_ns: 0,
            last_error: None,
            last_attempt_ns: 0,
            last_success_ns: last_sync_ns,
        });

        fi.failures = fi.failures.saturating_add(1);
        fi.last_error = last_error;
        fi.last_attempt_ns = now;
        fi.backoff_until_ns = match Self::backoff_for(layer) {
            Backoff::Const { interval_ns } => now.saturating_add(interval_ns),
            Backoff::Exp { max_exp }       => now.saturating_add(exp_backoff_with_jitter_ns(fi.failures, max_exp)),
        };

        faults.insert(layer, fi);
        Health::Degraded { faults, last_sync_ns }
    }


    pub fn degraded_with_layers(layers: &[HealthLayer]) -> Self {
        let mut faults = HashMap::new();
        for &layer in layers {
            faults.insert(layer, FaultInfo::default());
        }
        Health::Degraded {
            faults,
            last_sync_ns: 0,
        }
    }

    /// Mutating convenience (ergonomic at call sites)
    pub fn mark_success(&mut self, layer: HealthLayer) {
        let new = std::mem::take(self).record_success(layer);
        *self = new;
    }
    pub fn mark_failure(&mut self, layer: HealthLayer, last_error: Option<String>) {
        let new = std::mem::take(self).record_failure(layer, last_error);
        *self = new;
    }
}

fn exp_backoff_with_jitter_ns(failures: u32, max_exp: u32) -> u64 {
    let secs  = 2u64.saturating_pow(failures.min(max_exp));
    let base  = secs * 1_000_000_000;
    let delta = (base as f64 * 0.10) as u64;
    let jitter = rng().random_range(0..=delta);
    base.saturating_sub(delta / 2).saturating_add(jitter)
}
