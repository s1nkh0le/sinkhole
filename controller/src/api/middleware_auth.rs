use crate::config::env::ControllerCfg;
use axum::http::Request;
use axum::{body::Body, middleware::Next, response::Response};
use shared::api::auth::decision::{AuthOutcome, auth_rejection, decide_bearer_auth};
use std::sync::Arc;
use tracing::log::warn;

/// Auth via "Authorization: Bearer <token>". The decision + rejection envelopes
/// are shared with the filter (`shared::api::auth::decision`); only the logging
/// is controller-specific (`tracing`).
pub async fn require_ctrl_token(
    cfg: Arc<ControllerCfg>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let outcome = decide_bearer_auth(cfg.token_hash.as_deref(), cfg.debug, &req);

    match outcome {
        AuthOutcome::DebugBypass => {
            warn!(
                "[require_ctrl_token()] Auth disabled (no SINKHOLE_CONTROLLER_AUTH_TOKEN[_HASH]) — allowing request"
            );
        }
        AuthOutcome::Misconfigured => {
            tracing::error!(
                "[require_ctrl_token()] auth misconfigured: no SINKHOLE_CONTROLLER_AUTH_TOKEN[_HASH] set in non-debug mode — refusing request"
            );
        }
        _ => {}
    }

    if let Some(rejection) = auth_rejection(outcome, req.uri().path()) {
        return rejection;
    }

    next.run(req).await
}
