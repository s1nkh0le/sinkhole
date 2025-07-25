use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use crate::observability::eval_record::StatsSnapshot;

#[derive(Default)]
pub struct RuleCounters {
    pub hits: AtomicU64,
    pub misses: AtomicU64,
    pub skipped: AtomicU64,
    pub errors: AtomicU64,
}

impl RuleCounters {
    #[inline] pub fn inc_hits(&self)    { self.hits.fetch_add(1, Ordering::Relaxed); }
    #[inline] pub fn inc_misses(&self)  { self.misses.fetch_add(1, Ordering::Relaxed); }
    #[inline] pub fn inc_skipped(&self) { self.skipped.fetch_add(1, Ordering::Relaxed); }
    #[inline] pub fn inc_errors(&self)  { self.errors.fetch_add(1, Ordering::Relaxed); }
}

pub type CountersMap = DashMap<String, Arc<RuleCounters>>;

pub fn get_or_init(map: &CountersMap, id: &str) -> Arc<RuleCounters> {
    map.entry(id.to_owned())
        .or_insert_with(|| Arc::new(RuleCounters::default()))
        .clone()
}

pub fn capture(port: &crate::registry::PortRuntime) -> StatsSnapshot {
    StatsSnapshot {
        blacklist_hits: 0, // fill later if you keep a global
        whitelist_all_total: 0,
        whitelist_all_hit: 0,
        whitelist_any_total: 0,
        whitelist_any_hit: 0,
    }
}