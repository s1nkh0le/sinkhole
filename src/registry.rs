use crate::config::PortBinding;
use arc_swap::ArcSwap;
use dashmap::DashMap;
use std::sync::Arc;

// #[derive(Clone)]
pub struct PortRuntime {
    pub binding: PortBinding,
    pub enabled_scopes: std::collections::HashSet<crate::types::Scope>,
    pub ruleset: ArcSwap<crate::rules::RuleSetRaw>, // compiled later
}

pub type PortRegistry = DashMap<u16, Arc<PortRuntime>>;

pub fn build_registry(cfg: &crate::config::Config) -> PortRegistry {
    let reg = PortRegistry::new();
    for pb in &cfg.ports {
        let pr = Arc::new(PortRuntime {
            binding: pb.clone(),
            enabled_scopes: std::collections::HashSet::from_iter([
                crate::types::Scope::Meta,
                crate::types::Scope::Stream,
                crate::types::Scope::HttpHeader,
                crate::types::Scope::HttpPath,
                crate::types::Scope::HttpBody,
            ]),
            ruleset: ArcSwap::from_pointee(crate::rules::RuleSetRaw {
                comment: None,
                markers: vec![],
                filters: vec![],
            }),
        });
        reg.insert(pb.public, pr);
    }
    reg
}
