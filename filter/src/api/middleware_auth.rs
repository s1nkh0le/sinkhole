use crate::config::Config;
use crate::observability::sinks::sink_ecam;
use crate::observability::sinks::sink_ecam::{Origin, Severity};
use axum::body::Body;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use shared::api::auth::decision::{AuthOutcome, auth_rejection, decide_bearer_auth};

/// Auth via "Authorization: Bearer <token>". The decision + rejection envelopes
/// are shared with the controller (`shared::api::auth::decision`); only the
/// logging is filter-specific (ECAM security events).
pub async fn require_token(cfg: Config, req: Request<Body>, next: Next) -> Response {
    let outcome = decide_bearer_auth(cfg.token_hash.as_deref(), cfg.debug, &req);

    match outcome {
        AuthOutcome::DebugBypass => sink_ecam::record(
            Severity::Warn,
            Origin::Security,
            "Debug on => Auth disabled (no SINKHOLE_FILTER_AUTH_TOKEN[_HASH]); allowing request"
                .to_string(),
            None,
        ),
        AuthOutcome::Misconfigured => sink_ecam::record(
            Severity::Error,
            Origin::Security,
            "Auth misconfigured: not debug and no SINKHOLE_FILTER_AUTH_TOKEN[_HASH] set in non-debug mode; refusing request"
                .to_string(),
            None,
        ),
        AuthOutcome::MissingToken => sink_ecam::record(
            Severity::Error,
            Origin::Security,
            "Missing token".to_string(),
            None,
        ),
        AuthOutcome::BadToken => sink_ecam::record(
            Severity::Error,
            Origin::Security,
            "Authentication failure: wrong token".to_string(),
            None,
        ),
        AuthOutcome::Authorized => {}
    }

    if let Some(rejection) = auth_rejection(outcome, req.uri().path()) {
        return rejection;
    }

    next.run(req).await
}
