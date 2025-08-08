use crate::{
    config::{Config, PortBinding},
    rules::compile::{compile_ruleset, CompiledRuleSet},
};
use anyhow::Result;
use arc_swap::{ArcSwap, ArcSwapOption};
use dashmap::DashMap;
use shared::types::base_types::Scope;
use shared::types::ruleset_types::{PortInfo, RuleSetRaw};
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use shared::types::snapshot_types::PortSnapshot;
use shared::utils::version::{calc_version_combined_copyover, calc_version_info, calc_version_ruleset};

/// In‑memory runtime object for one public port.
pub struct PortRuntime {
    pub binding: PortBinding,
    pub enabled_scopes: RwLock<HashSet<Scope>>,

    /// Raw JSON ruleset.
    pub raw_ruleset: ArcSwap<RuleSetRaw>,

    /// Compiled ruleset for the engine.
    pub compiled: ArcSwap<CompiledRuleSet>,

    pub name: ArcSwapOption<String>,
    pub comment: ArcSwapOption<String>,
    pub hex_color: ArcSwapOption<String>,
}


impl PortRuntime {
    pub fn to_portinfo(&self) -> PortInfo {
        // -------- ArcSwapOption fields -----------------------------------
        // `load_full()` gives an `Option<Arc<T>>`.  `as_deref()` turns that into
        // `Option<&T>`, and `cloned()` makes an owned `Option<T>`.
        let name      = self.name.load_full().as_deref().cloned();
        let comment   = self.comment.load_full().as_deref().cloned();
        let hex_color = self.hex_color.load_full().as_deref().cloned();

        // -------- RwLock field -------------------------------------------
        let enabled_scopes = {
            let guard = self
                .enabled_scopes
                .read()
                .expect("PortRuntime.enabled_scopes lock poisoned");
            guard.iter().cloned().collect::<Vec<_>>()
        };

        // -------- Assemble result ----------------------------------------
        PortInfo {
            public:   self.binding.public,
            upstream: self.binding.upstream,
            name,
            comment,
            hex_color,
            enabled_scopes,
        }
    }

    /// Return a read-only clone of the *current* raw ruleset.
    pub fn to_rulesetraw(&self) -> RuleSetRaw {
        // `load_full()` gives an `Arc<RuleSetRaw>`; deref and clone to hand out
        // an owned value that is independent of the registry.
        (*self.raw_ruleset.load_full()).clone()
    }
    
    /// Return a read-only clone of the *current* raw ruleset.
    pub fn to_snapshot(&self) -> PortSnapshot {
        let info = self.to_portinfo();
        let ruleset = self.to_rulesetraw();
        let ver_info = calc_version_info(&info);
        let ver_ruleset = calc_version_ruleset(&ruleset);
        let ver_combined = calc_version_combined_copyover(&ver_info, &ver_ruleset);
        
        PortSnapshot {
            info,
            ruleset,
            ver_info,
            ver_ruleset,
            ver_combined
        }
    }
}

pub type PortRegistry = DashMap<u16, Arc<PortRuntime>>;

pub fn build_registry(cfg: &Config) -> Result<PortRegistry> {
    let reg = PortRegistry::new();

    for port_binding in &cfg.ports {
        let empty_raw = Arc::new(RuleSetRaw {
            comment: None,
            markers: vec![],
            filters: vec![],
        });
        let empty_compiled = Arc::new(compile_ruleset(&empty_raw)?);

        let port_runtime = Arc::new(PortRuntime {
            binding: port_binding.clone(),
            enabled_scopes: RwLock::new(default_scopes()),
            raw_ruleset: ArcSwap::from(empty_raw),
            compiled: ArcSwap::from(empty_compiled),
            name: ArcSwapOption::from(None),
            comment: ArcSwapOption::from(None),
            hex_color: ArcSwapOption::from(None),
        });

        reg.insert(port_binding.public, port_runtime);
    }

    Ok(reg)
}

fn default_scopes() -> HashSet<Scope> {
    use Scope::*;
    [
        Meta, Stream, HttpHeader, HttpPath, HttpBody, HttpMethod, HttpStatus, Timing,
    ]
    .into_iter()
    .collect()
}
