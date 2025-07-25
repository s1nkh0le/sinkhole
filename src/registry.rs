use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use anyhow::Result;
use arc_swap::ArcSwap;
use dashmap::DashMap;

use crate::{
    config::{Config, PortBinding},
    rules::{
        compile::{compile_ruleset, CompiledRuleSet},
        model::RuleSetRaw,
    },
    types::Scope,
};

/// In‑memory runtime object for one public port.
pub struct PortRuntime {
    pub binding: PortBinding,
    pub enabled_scopes: HashSet<Scope>,

    /// Raw JSON ruleset.
    pub raw_ruleset: ArcSwap<RuleSetRaw>,

    /// Compiled ruleset for the engine.
    pub compiled: ArcSwap<CompiledRuleSet>,
}

/// Key = public port
pub type PortRegistry = DashMap<u16, Arc<PortRuntime>>;

pub fn build_registry(cfg: &Config) -> Result<PortRegistry> {
    let reg = PortRegistry::new();

    for pb in &cfg.ports {
        let empty_raw = Arc::new(RuleSetRaw {
            comment: None,
            markers: vec![],
            filters: vec![],
        });
        let empty_compiled = Arc::new(compile_ruleset(&empty_raw)?);

        let pr = Arc::new(PortRuntime {
            binding: pb.clone(),
            enabled_scopes: default_scopes(),
            raw_ruleset: ArcSwap::from(empty_raw),
            compiled: ArcSwap::from(empty_compiled),
        });

        reg.insert(pb.public, pr);
    }

    Ok(reg)
}

fn default_scopes() -> HashSet<Scope> {
    use Scope::*;
    [Meta, Stream, HttpHeader, HttpPath, HttpBody]
        .into_iter()
        .collect()
}
