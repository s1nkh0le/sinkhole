use crate::config::ControllerCfg;
use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use shared::utils::hashes::sha256_hex;
use std::sync::Arc;
use tracing::log::warn;
use url::form_urlencoded;

/// Middleware that enforces `?ctrl_token=...` against the configured hash.
/// If no hash is configured, it lets the request pass (with a WARN logged at init time).
pub async fn require_ctrl_token(
    cfg: Arc<ControllerCfg>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let expected = cfg.ctrl_token_hash.as_ref();
    if expected.is_none() {
        // Auth disabled -> pass through
        warn!("Auth disabled");
        return next.run(req).await;
    }
    let expected = expected.unwrap();

    // Extract ctrl_token from query string
    let token_opt = req.uri().query().and_then(|q| {
        form_urlencoded::parse(q.as_bytes())
            .find(|(k, _)| k == "ctrl_token")
            .map(|(_, v)| v.into_owned())
    });

    let Some(token) = token_opt else {
        return unauthorized("missing ctrl_token");
    };

    let calc = sha256_hex(token.as_bytes());
    if &calc != expected {
        return unauthorized("unauthorized");
    }

    next.run(req).await
}

#[inline]
fn unauthorized(msg: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "status":"error",
            "errors":[msg]
        })),
    )
        .into_response()
}
