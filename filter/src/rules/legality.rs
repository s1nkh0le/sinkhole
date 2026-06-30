//! Single source of truth for matcher legality.
//!
//! Both [`crate::rules::validate`] (dry-run validation surfaced to the user) and
//! [`crate::rules::compile`] (which turns a [`RuleSetRaw`] into executable
//! matchers) consult the functions here, so the two can never disagree about
//! which `(scope, matcher, direction)` combinations are legal.

use shared::types::base_types::{RuleDirection, Scope};
use shared::types::ruleset_types::MatcherDef;

/// Coarse classification of a [`MatcherDef`] independent of its parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatcherKind {
    StreamRegex,
    PathRegex,
    BodyRegex,
    HeaderRegex,
    MethodRegex,
    StatusRegex,
    StreamBytes,
    BodyBytes,
    BytesHex,
    Size,
    SessionLifetime,
    SessionIdle,
    Always,
}

impl MatcherKind {
    pub fn of(def: &MatcherDef) -> Self {
        match def {
            MatcherDef::StreamRegex { .. } => MatcherKind::StreamRegex,
            MatcherDef::PathRegex { .. } => MatcherKind::PathRegex,
            MatcherDef::BodyRegex { .. } => MatcherKind::BodyRegex,
            MatcherDef::HeaderRegex { .. } => MatcherKind::HeaderRegex,
            MatcherDef::MethodRegex { .. } => MatcherKind::MethodRegex,
            MatcherDef::StatusRegex { .. } => MatcherKind::StatusRegex,
            MatcherDef::StreamBytes { .. } => MatcherKind::StreamBytes,
            MatcherDef::BodyBytes { .. } => MatcherKind::BodyBytes,
            MatcherDef::BytesHex { .. } => MatcherKind::BytesHex,
            MatcherDef::Size { .. } => MatcherKind::Size,
            MatcherDef::SessionLifetime { .. } => MatcherKind::SessionLifetime,
            MatcherDef::SessionIdle { .. } => MatcherKind::SessionIdle,
            MatcherDef::Always { .. } => MatcherKind::Always,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            MatcherKind::StreamRegex => "stream_regex",
            MatcherKind::PathRegex => "path_regex",
            MatcherKind::BodyRegex => "body_regex",
            MatcherKind::HeaderRegex => "header_regex",
            MatcherKind::MethodRegex => "method_regex",
            MatcherKind::StatusRegex => "status_regex",
            MatcherKind::StreamBytes => "stream_bytes",
            MatcherKind::BodyBytes => "body_bytes",
            MatcherKind::BytesHex => "bytes_hex",
            MatcherKind::Size => "size",
            MatcherKind::SessionLifetime => "session.lifetime",
            MatcherKind::SessionIdle => "session.idle",
            MatcherKind::Always => "always",
        }
    }
}

/// Is this matcher kind legal in this scope? (Direction-independent.)
///
/// This is the canonical `(Scope × Matcher)` table.
pub fn scope_matcher_ok(scope: Scope, kind: MatcherKind) -> bool {
    use MatcherKind::*;
    match (scope, kind) {
        // Each regex matcher is bound to exactly one scope.
        (Scope::Stream, StreamRegex) => true,
        (Scope::HttpPath, PathRegex) => true,
        (Scope::HttpBody, BodyRegex) => true,
        (Scope::HttpHeader, HeaderRegex) => true,
        (Scope::HttpMethod, MethodRegex) => true,
        (Scope::HttpStatus, StatusRegex) => true,

        // Byte matchers are binary-safe and only meaningful where unparsed bytes
        // exist: the raw stream and the message body.
        (Scope::Stream, StreamBytes) => true,
        (Scope::HttpBody, BodyBytes) => true,
        (Scope::Stream, BytesHex) | (Scope::HttpBody, BytesHex) => true,

        // Timing matchers live only in the timing scope.
        (Scope::Timing, SessionLifetime) => true,
        (Scope::Timing, SessionIdle) => true,

        // `size` is universal, except where there is nothing meaningful to size.
        (Scope::HttpStatus, Size) | (Scope::HttpMethod, Size) | (Scope::Timing, Size) => false,
        (_, Size) => true,

        // `always` is scope-agnostic.
        (_, Always) => true,

        _ => false,
    }
}

/// Full legality check for a `(scope, matcher, direction)` triple.
///
/// Returns `Ok(())` if the combination is valid, or a human-readable reason if
/// not. Used by both validation and compilation so neither can drift.
pub fn check_rule(scope: Scope, def: &MatcherDef, direction: RuleDirection) -> Result<(), String> {
    let kind = MatcherKind::of(def);

    if !scope_matcher_ok(scope, kind) {
        return Err(format!(
            "matcher '{}' is not allowed with scope '{}'",
            kind.label(),
            scope
        ));
    }

    // Status only exists on responses.
    if scope == Scope::HttpStatus && direction == RuleDirection::Inbound {
        return Err(format!(
            "scope '{}' is not allowed with direction 'inbound'",
            Scope::HttpStatus
        ));
    }

    // Timing matchers observe the whole connection, so they must run in both
    // directions.
    if matches!(
        kind,
        MatcherKind::SessionLifetime | MatcherKind::SessionIdle
    ) && direction != RuleDirection::Both
    {
        return Err(format!(
            "matcher '{}' requires direction 'both'",
            kind.label()
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::types::base_types::CmpOp;

    fn size() -> MatcherDef {
        MatcherDef::Size {
            op: CmpOp::Gt,
            value: 10,
            field_name: None,
        }
    }
    fn stream() -> MatcherDef {
        MatcherDef::StreamRegex {
            pattern: "x".into(),
            case_sensitive: false,
        }
    }
    fn idle() -> MatcherDef {
        MatcherDef::SessionIdle {
            op: CmpOp::Gt,
            value: 1.0,
        }
    }
    fn status() -> MatcherDef {
        MatcherDef::StatusRegex {
            pattern: "5..".into(),
            case_sensitive: false,
        }
    }

    #[test]
    fn regex_bound_to_its_scope() {
        assert!(check_rule(Scope::Stream, &stream(), RuleDirection::Both).is_ok());
        assert!(check_rule(Scope::HttpBody, &stream(), RuleDirection::Both).is_err());
    }

    #[test]
    fn size_universal_except_status_method_timing() {
        for s in [
            Scope::Stream,
            Scope::HttpPath,
            Scope::HttpBody,
            Scope::HttpHeader,
        ] {
            assert!(
                check_rule(s, &size(), RuleDirection::Both).is_ok(),
                "size should be legal on {s:?}"
            );
        }
        for s in [Scope::HttpStatus, Scope::HttpMethod, Scope::Timing] {
            assert!(
                check_rule(s, &size(), RuleDirection::Both).is_err(),
                "size should be illegal on {s:?}"
            );
        }
    }

    #[test]
    fn timing_requires_direction_both() {
        assert!(check_rule(Scope::Timing, &idle(), RuleDirection::Both).is_ok());
        assert!(check_rule(Scope::Timing, &idle(), RuleDirection::Inbound).is_err());
        assert!(check_rule(Scope::Timing, &idle(), RuleDirection::Outbound).is_err());
    }

    #[test]
    fn status_forbids_inbound() {
        assert!(check_rule(Scope::HttpStatus, &status(), RuleDirection::Outbound).is_ok());
        assert!(check_rule(Scope::HttpStatus, &status(), RuleDirection::Inbound).is_err());
    }

    #[test]
    fn always_legal_anywhere() {
        let always = MatcherDef::Always { value: true };
        for s in [
            Scope::Stream,
            Scope::HttpStatus,
            Scope::Timing,
            Scope::HttpHeader,
        ] {
            assert!(check_rule(s, &always, RuleDirection::Both).is_ok());
        }
    }

    #[test]
    fn byte_matchers_only_on_stream_and_body() {
        let sb = MatcherDef::StreamBytes {
            pattern: "\\x90".into(),
        };
        let bb = MatcherDef::BodyBytes {
            pattern: "\\x90".into(),
        };
        let hx = MatcherDef::BytesHex {
            value: "deadbeef".into(),
        };
        assert!(check_rule(Scope::Stream, &sb, RuleDirection::Both).is_ok());
        assert!(check_rule(Scope::HttpBody, &bb, RuleDirection::Both).is_ok());
        assert!(check_rule(Scope::Stream, &hx, RuleDirection::Both).is_ok());
        assert!(check_rule(Scope::HttpBody, &hx, RuleDirection::Both).is_ok());

        // Wrong scope or wrong byte-kind/scope pairing is rejected.
        assert!(check_rule(Scope::HttpBody, &sb, RuleDirection::Both).is_err());
        assert!(check_rule(Scope::Stream, &bb, RuleDirection::Both).is_err());
        assert!(check_rule(Scope::HttpPath, &hx, RuleDirection::Both).is_err());
        assert!(check_rule(Scope::HttpStatus, &sb, RuleDirection::Both).is_err());
    }
}
