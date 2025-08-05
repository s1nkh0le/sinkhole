use crate::config::ControllerCfg;
use axum::{
    body::Body,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use shared::utils::hashes::sha256_hex;
use std::sync::Arc;
use axum::extract::OriginalUri;
use axum::http::Request;
use tracing::log::warn;
use url::form_urlencoded;
use shared::api::err::ErrorEnvelope;

const CTRL_TOKEN_FIELD: &'static str = "ctrl_token";

/// Middleware that enforces `?ctrl_token=...` against the configured hash.
/// If no hash is configured, it lets the request pass (with a WARN logged at init time).
pub async fn require_ctrl_token(
    cfg: Arc<ControllerCfg>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let expected = cfg.ctrl_token_hash.as_ref();
    if expected.is_none() {
        warn!("[require_ctrl_token()] Auth disabled, allowing controller traffic");
        return next.run(req).await;
    }
    let expected = expected.unwrap();

    // Extract ctrl_token from query string
    let token_opt = req.uri().query().and_then(|q| {
        form_urlencoded::parse(q.as_bytes())
            .find(|(k, _)| k == CTRL_TOKEN_FIELD)
            .map(|(_, v)| v.into_owned())
    });

    let Some(token) = token_opt else {
        return ErrorEnvelope::unauthorized("unauthorized", req.uri().path())
            .with_title("Authenticaton failure")
            .with_detail("No `ctrl_token` has been supplied. Request is dropped automatically.")
            .into_response();
    };

    let calc = sha256_hex(token.as_bytes());
    if &calc != expected {
        return ErrorEnvelope::unauthorized("unauthorized", req.uri().path())
            .with_title("Authenticaton failure")
            .with_detail("The supplied `ctrl_token` doesn't match the controller's allowed token. Request is dropped automatically.")
            .into_response();
    }

    next.run(req).await
}