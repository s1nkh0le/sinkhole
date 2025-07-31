use super::model::*;
use crate::types::{RuleDirection, Scope};
use std::collections::HashSet;

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
            (Scope::HttpMethod, MethodRegex { .. }) => true,
            (Scope::HttpStatus, StatusRegex { .. }) => true,

            (Scope::Timing, SessionLifetime { .. }) => true,
            (Scope::Timing, SessionIdle { .. })     => true,

            // Size is universal, except when checking the status code
            (Scope::HttpStatus, Size { .. }) => false,
            (Scope::Timing, Size { .. }) => false,
            (_, Size { .. }) => true,

            // always is ok with any scope
            (_, Always { .. }) => true,
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

        // ---- forbid inbound status scope -------------------------------
        if m.scope == Scope::HttpStatus && m.direction == RuleDirection::Inbound {
            errs.push(format!(
                "markers[id={}].direction inbound not allowed with scope http.status",
                m.id
            ));
        }

        if matches!(m.matcher, MatcherDef::SessionLifetime { .. } | MatcherDef::SessionIdle { .. })
            && m.direction != RuleDirection::Both
        {
            errs.push(format!("markers[id={}]: timing scope requires direction='both'", m.id));
        }

        let ctx = format!("markers[id={}]", m.id);
        if let MatcherDef::Size { field_name, .. } = &m.matcher {
            validate_size_field(m.scope, field_name.as_deref(), &ctx, &mut errs);
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

        if f.scope == Scope::HttpStatus && f.direction == RuleDirection::Inbound {
            errs.push(format!(
                "filters[id={}].direction inbound not allowed with scope http.status",
                f.id
            ));
        }
        
        if matches!(f.matcher, MatcherDef::SessionLifetime { .. } | MatcherDef::SessionIdle { .. })
            && f.direction != RuleDirection::Both
        {
            errs.push(format!("filters[id={}]: timing scope requires direction='both'", f.id));
        }
        
        let ctx = format!("filters[id={}]", f.id);
        if let MatcherDef::Size { field_name, .. } = &f.matcher {
            validate_size_field(f.scope, field_name.as_deref(), &ctx, &mut errs);
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



fn validate_size_field(
    scope: Scope,
    field_name: Option<&str>,
    ctx: &str,
    errs: &mut Vec<String>,
) {
    match scope {
        Scope::HttpHeader => {
            if field_name.is_none() || field_name.unwrap().trim().is_empty() {
                errs.push(format!("{ctx}.matcher.field_name: required for size on http.header"));
            }
        }
        _ => {
            if field_name.is_some() {
                errs.push(format!("{ctx}.matcher.field_name: only allowed for scope=http.header"));
            }
        }
    }
}
