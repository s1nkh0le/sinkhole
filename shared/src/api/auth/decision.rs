//! Shared bearer-auth decision for both control-API middlewares.
//!
//! The filter and controller previously duplicated the exact same auth logic
//! (expected-hash present? debug bypass? token present? token valid?) and the
//! exact same 401/503 rejection envelopes — only the *logging* differed (the
//! filter emits ECAM events, the controller uses `tracing`). This module owns the
//! decision and the rejection responses; each side keeps only its logging.

use axum::body::Body;
use axum::http::Request;
use axum::response::{IntoResponse, Response};

use crate::api::auth::bearer::bearer_token_from_header;
use crate::api::err::ErrorEnvelope;
use crate::utils::hashes::verify_sha256_hex;

/// Outcome of evaluating a request against the bearer-auth policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthOutcome {
    /// A valid token was presented — proceed.
    Authorized,
    /// No token is configured but the service is in debug mode — proceed
    /// (the caller should log a warning that auth is disabled).
    DebugBypass,
    /// No token is configured and the service is not in debug mode — refuse
    /// (fail-closed, 503). The caller should log an error.
    Misconfigured,
    /// No `Authorization: Bearer` header was present — refuse (401).
    MissingToken,
    /// A bearer token was present but did not match — refuse (401).
    BadToken,
}

impl AuthOutcome {
    /// `true` for the two outcomes that let the request through.
    pub fn passes(self) -> bool {
        matches!(self, AuthOutcome::Authorized | AuthOutcome::DebugBypass)
    }
}

/// Evaluate the bearer-auth policy. Pure: no logging, no side effects — the
/// caller maps the outcome to its own logging + [`auth_rejection`].
///
/// `token_hash` is the configured `sha256(token)` hex (the `AUTH_TOKEN_HASH`),
/// `debug` enables the no-token bypass.
pub fn decide_bearer_auth(
    token_hash: Option<&str>,
    debug: bool,
    req: &Request<Body>,
) -> AuthOutcome {
    let Some(expected) = token_hash else {
        return if debug {
            AuthOutcome::DebugBypass
        } else {
            AuthOutcome::Misconfigured
        };
    };

    let Some(token) = bearer_token_from_header(req) else {
        return AuthOutcome::MissingToken;
    };

    if verify_sha256_hex(token.trim(), expected) {
        AuthOutcome::Authorized
    } else {
        AuthOutcome::BadToken
    }
}

/// Build the HTTP rejection for a non-passing outcome, or `None` for the two
/// passing outcomes ([`AuthOutcome::Authorized`] / [`AuthOutcome::DebugBypass`]),
/// which the caller proceeds on. `path` is the request path for the envelope's
/// `instance` field.
pub fn auth_rejection(outcome: AuthOutcome, path: &str) -> Option<Response> {
    match outcome {
        AuthOutcome::Authorized | AuthOutcome::DebugBypass => None,
        AuthOutcome::Misconfigured => Some(
            ErrorEnvelope::service_unavailable("auth_not_configured", path)
                .with_title("Service misconfigured")
                .with_detail("Authentication is not configured (no AUTH_TOKEN[_HASH])")
                .into_response(),
        ),
        AuthOutcome::MissingToken => Some(
            ErrorEnvelope::unauthorized("unauthorized", path)
                .with_title("Authentication failure")
                .with_detail("Missing Authorization: Bearer")
                .into_response(),
        ),
        AuthOutcome::BadToken => Some(
            ErrorEnvelope::unauthorized("unauthorized", path)
                .with_title("Authentication failure")
                .with_detail("Bad token")
                .into_response(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::hashes::sha256_hex;

    fn req(auth: Option<&str>) -> Request<Body> {
        let mut b = Request::builder().uri("/x");
        if let Some(v) = auth {
            b = b.header("authorization", v);
        }
        b.body(Body::empty()).unwrap()
    }

    #[test]
    fn no_hash_debug_bypasses_else_misconfigured() {
        assert_eq!(
            decide_bearer_auth(None, true, &req(None)),
            AuthOutcome::DebugBypass
        );
        assert_eq!(
            decide_bearer_auth(None, false, &req(None)),
            AuthOutcome::Misconfigured
        );
    }

    #[test]
    fn token_presence_and_validity() {
        let hash = sha256_hex(b"s3cret");
        // Configured but no header.
        assert_eq!(
            decide_bearer_auth(Some(&hash), false, &req(None)),
            AuthOutcome::MissingToken
        );
        // Wrong token.
        assert_eq!(
            decide_bearer_auth(Some(&hash), false, &req(Some("Bearer nope"))),
            AuthOutcome::BadToken
        );
        // Correct token, case-insensitive scheme.
        assert_eq!(
            decide_bearer_auth(Some(&hash), false, &req(Some("bearer s3cret"))),
            AuthOutcome::Authorized
        );
    }

    #[test]
    fn rejection_maps_to_responses() {
        use axum::http::StatusCode;
        assert!(auth_rejection(AuthOutcome::Authorized, "/x").is_none());
        assert!(auth_rejection(AuthOutcome::DebugBypass, "/x").is_none());
        assert_eq!(
            auth_rejection(AuthOutcome::Misconfigured, "/x")
                .unwrap()
                .status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            auth_rejection(AuthOutcome::MissingToken, "/x")
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            auth_rejection(AuthOutcome::BadToken, "/x")
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }
}
