use crate::rules::legality::check_rule;
use shared::types::base_types::Scope;
use shared::types::ruleset_types::*;
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

        // matcher⇄scope⇄direction legality (single source of truth)
        if let Err(reason) = check_rule(m.scope, &m.matcher, m.direction) {
            errs.push(format!("markers[id={}]: {}", m.id, reason));
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

        if let Err(reason) = check_rule(f.scope, &f.matcher, f.direction) {
            errs.push(format!("filters[id={}]: {}", f.id, reason));
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

fn validate_size_field(scope: Scope, field_name: Option<&str>, ctx: &str, errs: &mut Vec<String>) {
    match scope {
        Scope::HttpHeader => {
            if field_name.is_none() || field_name.unwrap().trim().is_empty() {
                errs.push(format!(
                    "{ctx}.matcher.field_name: required for size on http.header"
                ));
            }
        }
        _ => {
            if field_name.is_some() {
                errs.push(format!(
                    "{ctx}.matcher.field_name: only allowed for scope=http.header"
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::types::base_types::{CmpOp, Mode, RuleDirection};
    use shared::types::ruleset_types::{FilterDef, MarkerDef};

    fn all_scopes() -> HashSet<Scope> {
        [
            Scope::Stream,
            Scope::HttpHeader,
            Scope::HttpPath,
            Scope::HttpBody,
            Scope::HttpMethod,
            Scope::HttpStatus,
            Scope::Timing,
        ]
        .into_iter()
        .collect()
    }

    fn marker(id: &str, scope: Scope, dir: RuleDirection, matcher: MatcherDef) -> MarkerDef {
        MarkerDef {
            id: id.into(),
            name: None,
            direction: dir,
            scope,
            matcher,
            enabled: true,
        }
    }

    #[test]
    fn accepts_valid_ruleset() {
        let rs = RuleSetRaw {
            markers: vec![marker(
                "m1",
                Scope::HttpPath,
                RuleDirection::Inbound,
                MatcherDef::PathRegex {
                    pattern: "/admin".into(),
                    case_sensitive: false,
                },
            )],
            filters: vec![FilterDef {
                id: "f1".into(),
                name: None,
                direction: RuleDirection::Inbound,
                scope: Scope::HttpPath,
                matcher: MatcherDef::PathRegex {
                    pattern: "/x".into(),
                    case_sensitive: false,
                },
                mode: Mode::Blacklist,
                needed_markers_all: vec!["m1".into()],
                needed_markers_any: vec![],
                enabled: true,
            }],
            commit_message: None,
        };
        assert!(validate_ruleset(&rs, &all_scopes()).is_empty());
    }

    #[test]
    fn rejects_scope_matcher_mismatch() {
        let rs = RuleSetRaw {
            markers: vec![marker(
                "m1",
                Scope::HttpBody,
                RuleDirection::Inbound,
                MatcherDef::PathRegex {
                    pattern: "x".into(),
                    case_sensitive: false,
                },
            )],
            filters: vec![],
            commit_message: None,
        };
        assert!(!validate_ruleset(&rs, &all_scopes()).is_empty());
    }

    #[test]
    fn rejects_size_on_header_without_field_name() {
        let rs = RuleSetRaw {
            markers: vec![marker(
                "m1",
                Scope::HttpHeader,
                RuleDirection::Inbound,
                MatcherDef::Size {
                    op: CmpOp::Gt,
                    value: 10,
                    field_name: None,
                },
            )],
            filters: vec![],
            commit_message: None,
        };
        assert!(!validate_ruleset(&rs, &all_scopes()).is_empty());
    }

    #[test]
    fn rejects_unknown_gating_marker() {
        let rs = RuleSetRaw {
            markers: vec![],
            filters: vec![FilterDef {
                id: "f1".into(),
                name: None,
                direction: RuleDirection::Inbound,
                scope: Scope::Stream,
                matcher: MatcherDef::StreamRegex {
                    pattern: "x".into(),
                    case_sensitive: false,
                },
                mode: Mode::Blacklist,
                needed_markers_all: vec!["ghost".into()],
                needed_markers_any: vec![],
                enabled: true,
            }],
            commit_message: None,
        };
        assert!(!validate_ruleset(&rs, &all_scopes()).is_empty());
    }
}
