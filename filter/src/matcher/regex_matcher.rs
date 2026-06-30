//! A single regex matcher for every regex-based scope.
//!
//! Replaces the six near-identical wrappers (stream/path/body/header/method/
//! status). The scope decides which slice of the message the regex runs against
//! via [`resolve_input`]; everything else is shared [`RegexCore`] behaviour.

use super::{MatchInput, MatchOutputCore, Matcher, regex_core::RegexCore};
use shared::types::base_types::Scope;
use std::borrow::Cow;
use std::str;

pub struct RegexMatcher {
    core: RegexCore,
    scope: Scope,
    /// Lower-cased header name; only set (and only used) for `Scope::HttpHeader`.
    header_name: Option<String>,
}

impl RegexMatcher {
    pub fn new(
        scope: Scope,
        pattern: &str,
        case_sensitive: bool,
        header_name: Option<&str>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            core: RegexCore::new(pattern, case_sensitive)?,
            scope,
            header_name: header_name.map(|h| h.to_ascii_lowercase()),
        })
    }
}

impl Matcher for RegexMatcher {
    fn evaluate(&self, inp: &MatchInput) -> MatchOutputCore {
        if inp.scope != self.scope {
            return MatchOutputCore::skipped("scope mismatch");
        }
        match resolve_input(self.scope, inp, self.header_name.as_deref()) {
            Ok(text) => self.core.eval(&text),
            Err(reason) => MatchOutputCore::skipped(reason),
        }
    }
}

/// Resolve the text a regex should run against for a given scope.
///
/// Returns `Err(reason)` when the message does not carry the relevant field
/// (the caller turns that into a `Skipped` result, which — like `NotSatisfied`
/// — is a non-hit for the decision engine).
fn resolve_input<'a>(
    scope: Scope,
    inp: &'a MatchInput,
    header_name: Option<&str>,
) -> Result<Cow<'a, str>, &'static str> {
    match scope {
        Scope::Stream => match str::from_utf8(inp.raw) {
            Ok(s) => Ok(Cow::Borrowed(s)),
            Err(_) => Err("stream is not valid UTF-8; use a byte matcher"),
        },
        Scope::HttpPath => inp
            .http
            .and_then(|h| h.path.as_deref())
            .map(Cow::Borrowed)
            .ok_or("no http path"),
        Scope::HttpBody => match inp.http {
            Some(h) => match str::from_utf8(&h.body) {
                Ok(s) => Ok(Cow::Borrowed(s)),
                Err(_) => Err("body is not valid UTF-8; use a byte matcher"),
            },
            None => Err("no http body"),
        },
        Scope::HttpMethod => inp
            .http
            .and_then(|h| h.method.as_deref())
            .map(Cow::Borrowed)
            .ok_or("no http method"),
        Scope::HttpStatus => match inp.http.and_then(|h| h.status) {
            Some(s) => Ok(Cow::Owned(s.to_string())),
            None => Err("no http status"),
        },
        Scope::HttpHeader => {
            let name = header_name.ok_or("header scope requires header_name")?;
            inp.http
                .and_then(|h| h.header_value(name))
                .map(Cow::Borrowed)
                .ok_or("header not present")
        }
        Scope::Timing => Err("regex is not supported on the timing scope"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matcher::Matcher;
    use crate::matcher::test_support::{Scenario, http};
    use shared::types::base_types::MatchStatus;

    fn rx(scope: Scope, pattern: &str, header: Option<&str>) -> RegexMatcher {
        RegexMatcher::new(scope, pattern, false, header).unwrap()
    }

    #[test]
    fn stream_scope_hit_and_miss() {
        let s = Scenario::with_raw(b"GET /flag HTTP/1.1\r\n\r\n");
        assert_eq!(
            rx(Scope::Stream, "/flag", None)
                .evaluate(&s.input(Scope::Stream))
                .status,
            MatchStatus::Satisfied
        );
        assert_eq!(
            rx(Scope::Stream, "/nope", None)
                .evaluate(&s.input(Scope::Stream))
                .status,
            MatchStatus::NotSatisfied
        );
    }

    #[test]
    fn stream_invalid_utf8_is_skipped() {
        // Non-UTF-8 bytes must yield Skipped, not a silent empty-string miss, so
        // operators know to reach for a byte matcher instead.
        let s = Scenario::with_raw(&[0xff, 0xfe, 0x00, 0x90]);
        assert_eq!(
            rx(Scope::Stream, "anything", None)
                .evaluate(&s.input(Scope::Stream))
                .status,
            MatchStatus::Skipped
        );
    }

    #[test]
    fn path_scope() {
        let s = Scenario::with_http(http(Some("GET"), Some("/admin/panel"), None, &[], b""));
        assert_eq!(
            rx(Scope::HttpPath, "^/admin", None)
                .evaluate(&s.input(Scope::HttpPath))
                .status,
            MatchStatus::Satisfied
        );
        assert_eq!(
            rx(Scope::HttpPath, "^/public", None)
                .evaluate(&s.input(Scope::HttpPath))
                .status,
            MatchStatus::NotSatisfied
        );
    }

    #[test]
    fn body_scope() {
        let s = Scenario::with_http(http(
            Some("POST"),
            Some("/"),
            None,
            &[],
            b"union select password",
        ));
        assert_eq!(
            rx(Scope::HttpBody, "union\\s+select", None)
                .evaluate(&s.input(Scope::HttpBody))
                .status,
            MatchStatus::Satisfied
        );
    }

    #[test]
    fn method_scope() {
        let s = Scenario::with_http(http(Some("DELETE"), Some("/"), None, &[], b""));
        assert_eq!(
            rx(Scope::HttpMethod, "DELETE", None)
                .evaluate(&s.input(Scope::HttpMethod))
                .status,
            MatchStatus::Satisfied
        );
    }

    #[test]
    fn status_scope_renders_code_as_text() {
        let s = Scenario::with_http(http(None, None, Some(503), &[], b""));
        assert_eq!(
            rx(Scope::HttpStatus, "503", None)
                .evaluate(&s.input(Scope::HttpStatus))
                .status,
            MatchStatus::Satisfied
        );
        assert_eq!(
            rx(Scope::HttpStatus, "200", None)
                .evaluate(&s.input(Scope::HttpStatus))
                .status,
            MatchStatus::NotSatisfied
        );
    }

    #[test]
    fn header_scope_uses_named_header() {
        let s = Scenario::with_http(http(
            Some("GET"),
            Some("/"),
            None,
            &[("User-Agent", "sqlmap/1.0")],
            b"",
        ));
        // header lookup is case-insensitive on the name
        assert_eq!(
            rx(Scope::HttpHeader, "sqlmap", Some("user-agent"))
                .evaluate(&s.input(Scope::HttpHeader))
                .status,
            MatchStatus::Satisfied
        );
    }

    #[test]
    fn missing_field_is_skipped_not_a_hit() {
        // path scope but the message has no path
        let s = Scenario::with_http(http(None, None, Some(200), &[], b""));
        assert_eq!(
            rx(Scope::HttpPath, ".*", None)
                .evaluate(&s.input(Scope::HttpPath))
                .status,
            MatchStatus::Skipped
        );

        // header scope but the named header is absent
        let s = Scenario::with_http(http(Some("GET"), Some("/"), None, &[], b""));
        assert_eq!(
            rx(Scope::HttpHeader, ".*", Some("x-absent"))
                .evaluate(&s.input(Scope::HttpHeader))
                .status,
            MatchStatus::Skipped
        );
    }

    #[test]
    fn scope_mismatch_is_skipped() {
        let s = Scenario::with_http(http(Some("GET"), Some("/admin"), None, &[], b""));
        // matcher bound to path scope, evaluated against a body-scope input
        let m = rx(Scope::HttpPath, "/admin", None);
        assert_eq!(
            m.evaluate(&s.input(Scope::HttpBody)).status,
            MatchStatus::Skipped
        );
    }

    #[test]
    fn case_insensitive_default() {
        let s = Scenario::with_http(http(Some("GET"), Some("/ADMIN"), None, &[], b""));
        assert_eq!(
            rx(Scope::HttpPath, "admin", None)
                .evaluate(&s.input(Scope::HttpPath))
                .status,
            MatchStatus::Satisfied
        );
    }
}
