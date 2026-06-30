//! Binary-safe matchers for the raw-stream and body scopes.
//!
//! Mirrors [`crate::matcher::regex_matcher`] but resolves the input as raw
//! bytes instead of UTF-8 text, so it can match arbitrary binary payloads.
//! Legal only on [`Scope::Stream`] and [`Scope::HttpBody`] (the scopes that
//! carry unparsed bytes); the rule legality table enforces this.

use super::{MatchInput, MatchOutputCore, Matcher, bytes_core::BytesRegexCore};
use shared::types::base_types::Scope;

pub struct BytesMatcher {
    core: BytesRegexCore,
    scope: Scope,
}

impl BytesMatcher {
    pub fn new(scope: Scope, core: BytesRegexCore) -> Self {
        Self { core, scope }
    }
}

impl Matcher for BytesMatcher {
    fn evaluate(&self, inp: &MatchInput) -> MatchOutputCore {
        if inp.scope != self.scope {
            return MatchOutputCore::skipped("scope mismatch");
        }
        match resolve_bytes(self.scope, inp) {
            Some(bytes) => self.core.eval(bytes),
            None => MatchOutputCore::skipped("no bytes for scope"),
        }
    }
}

/// Resolve the raw bytes a byte-matcher should run against for a given scope.
fn resolve_bytes<'a>(scope: Scope, inp: &'a MatchInput) -> Option<&'a [u8]> {
    match scope {
        Scope::Stream => Some(inp.raw),
        Scope::HttpBody => inp.http.map(|h| h.body.as_ref()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matcher::test_support::{Scenario, http};
    use shared::types::base_types::MatchStatus;

    #[test]
    fn stream_scope_matches_binary() {
        let s = Scenario::with_raw(&[0x41, 0xde, 0xad, 0xbe, 0xef]);
        let m = BytesMatcher::new(Scope::Stream, BytesRegexCore::from_hex("deadbeef").unwrap());
        assert_eq!(
            m.evaluate(&s.input(Scope::Stream)).status,
            MatchStatus::Satisfied
        );
    }

    #[test]
    fn body_scope_matches_binary() {
        let s = Scenario::with_http(http(
            Some("POST"),
            Some("/"),
            None,
            &[],
            &[0x90, 0x90, 0x90, 0x90],
        ));
        let m = BytesMatcher::new(Scope::HttpBody, BytesRegexCore::new("\\x90{4,}").unwrap());
        assert_eq!(
            m.evaluate(&s.input(Scope::HttpBody)).status,
            MatchStatus::Satisfied
        );
    }

    #[test]
    fn scope_mismatch_is_skipped() {
        let s = Scenario::with_raw(b"abc");
        let m = BytesMatcher::new(Scope::HttpBody, BytesRegexCore::new("a").unwrap());
        assert_eq!(
            m.evaluate(&s.input(Scope::Stream)).status,
            MatchStatus::Skipped
        );
    }
}
