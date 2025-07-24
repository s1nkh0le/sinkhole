use super::model::*;
use crate::types::Scope;
use std::collections::{HashMap, HashSet};

/// Validate ruleset against spec & enabled scopes.
/// Returns Vec<String> of all errors; empty vec == OK.
pub fn validate_ruleset(rs: &RuleSetRaw, enabled_scopes: &HashSet<Scope>) -> Vec<String> {
    let mut errs = Vec::new();
    let mut ids  = HashSet::new();

    fn check_id(id: &str, ids: &mut HashSet<String>, errs: &mut Vec<String>) {
        if id.len() > 64 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            errs.push(format!("invalid id '{}'", id));
        }
        if !ids.insert(id.to_owned()) {
            errs.push(format!("duplicate id '{}'", id));
        }
    }

    for m in &rs.markers {
        check_id(&m.id, &mut ids, &mut errs);
        if !enabled_scopes.contains(&m.scope) {
            errs.push(format!("markers[id={}].scope '{:?}' disabled", m.id, m.scope));
        }
    }
    for f in &rs.filters {
        check_id(&f.id, &mut ids, &mut errs);
        if !enabled_scopes.contains(&f.scope) {
            errs.push(format!("filters[id={}].scope '{:?}' disabled", f.id, f.scope));
        }
        for r in f.needed_markers_all.iter().chain(&f.needed_markers_any) {
            if !rs.markers.iter().any(|m| &m.id == r) {
                errs.push(format!("filter[id={}].needed marker '{}' unknown", f.id, r));
            }
        }
    }
    errs
}
