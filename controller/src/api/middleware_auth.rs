use crate::config::env::ControllerCfg;
use axum::{
    body::Body,
    middleware::Next,
    response::{IntoResponse, Response},
};
use shared::utils::hashes::sha256_hex;
use std::sync::Arc;
use axum::http::Request;
use tracing::log::warn;
use shared::api::auth::bearer::{bearer_token_from_header, token_from_query};
use shared::api::err::ErrorEnvelope;

const CTRL_TOKEN_FIELD: &str = "ctrl_token";


/// Header-first auth: "Authorization: Bearer <token>".
/// Temporarily accepts `?ctrl_token=...` (deprecated) with a WARN.
pub async fn require_ctrl_token(
    cfg: Arc<ControllerCfg>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let expected = match cfg.token_hash.as_ref() {
        Some(h) => h,
        None => {
            if cfg.debug {
                warn!("[require_ctrl_token()] Auth disabled (no CTRL token) — allowing request");
                return next.run(req).await;
            } else {
                panic!("Controller auth disabled in release mode (no CTRL token hash) — aborting");
            }
        }
    };

    let hdr_tok = bearer_token_from_header(&req);
    let q_tok   = token_from_query(&req, CTRL_TOKEN_FIELD);

    // If both present and disagree → reject (avoid ambiguity).
    if let (Some(h), Some(q)) = (&hdr_tok, &q_tok) {
        if h != q {
            return ErrorEnvelope::unauthorized("unauthorized", req.uri().path())
                .with_title("Authentication failure")
                .with_detail("Conflicting tokens in header and query")
                .into_response();
        }
    }

    // Prefer header, fallback to query (deprecated)
    let token = if let Some(t) = hdr_tok {
        t
    } else if let Some(t) = q_tok {
        warn!("[require_ctrl_token()] query token is DEPRECATED; use Authorization: Bearer");
        t
    } else {
        return ErrorEnvelope::unauthorized("unauthorized", req.uri().path())
            .with_title("Authentication failure")
            .with_detail("Missing Authorization: Bearer (or deprecated ?ctrl_token)")
            .into_response();
    };

    let calc = sha256_hex(token.trim().as_bytes());
    if &calc != expected {
        return ErrorEnvelope::unauthorized("unauthorized", req.uri().path())
            .with_title("Authentication failure")
            .with_detail("Bad token")
            .into_response();
    }

    next.run(req).await
}