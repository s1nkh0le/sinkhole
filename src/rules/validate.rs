use super::model::*;
use crate::types::Scope;
use std::collections::{HashMap, HashSet};

// ---------------------------------------------
// Validation entry
// ---------------------------------------------
pub fn validate_ruleset(rs: &RuleSetRaw, enabled_scopes: &HashSet<Scope>) -> Vec<String> {
    let mut errs = Vec::new();
    let mut ids: HashSet<String> = HashSet::new();

    // ------------- id helpers -------------
    fn check_id(id: &str, ids: &mut HashSet<String>, errs: &mut Vec<String>) {
        if id.len() > 64
            || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            errs.push(format!("invalid id '{}'", id));
        }
        if !ids.insert(id.to_owned()) {
            errs.push(format!("duplicate id '{}'", id));
        }
    }

    // ------------- matcher⇄scope table -------------
    fn matcher_scope_ok(scope: Scope, m: &MatcherDef) -> bool {
        use MatcherDef::*;
        match (scope, m) {
            (Scope::Stream, StreamRegex { .. }) => true,
            (Scope::HttpPath, PathRegex { .. }) => true,
            (Scope::HttpBody, BodyRegex { .. }) => true,
            (Scope::HttpHeader, HeaderRegex { .. }) => true,
            // size is universal
            (_, Size { .. }) => true,
            // always is ok with every scope
            (_, Always { .. }) => true,   // allow on any scope
            _ => false,
        }
    }

    // ------------- iterate markers -------------
    for m in &rs.markers {
        check_id(&m.id, &mut ids, &mut errs);

        // scope allowed on port?
        if !enabled_scopes.contains(&m.scope) {
            errs.push(format!(
                "markers[id={}].scope '{:?}' disabled for port",
                m.id, m.scope
            ));
        }

        // matcher vs scope
        if !matcher_scope_ok(m.scope, &m.matcher) {
            errs.push(format!(
                "markers[id={}].matcher type doesn't match scope {:?}",
                m.id, m.scope
            ));
        }
    }

    // ------------- iterate filters -------------
    for f in &rs.filters {
        check_id(&f.id, &mut ids, &mut errs);

        if !enabled_scopes.contains(&f.scope) {
            errs.push(format!(
                "filters[id={}].scope '{:?}' disabled for port",
                f.id, f.scope
            ));
        }

        if !matcher_scope_ok(f.scope, &f.matcher) {
            errs.push(format!(
                "filters[id={}].matcher type doesn't match scope {:?}",
                f.id, f.scope
            ));
        }

        // gating marker ids must exist
        for r in f.needed_markers_all.iter().chain(&f.needed_markers_any) {
            if !rs.markers.iter().any(|m| &m.id == r) {
                errs.push(format!("filter[id={}].needed marker '{}' unknown", f.id, r));
            }
        }
    }

    errs
}
