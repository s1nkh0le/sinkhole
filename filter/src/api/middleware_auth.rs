use crate::config::Config;
use crate::observability::sinks::sink_ecam;
use crate::observability::sinks::sink_ecam::{Origin, Severity};
use axum::body::Body;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tracing::{error, info};
use shared::api::auth::bearer::{bearer_token_from_header, token_from_query};
use shared::api::err::ErrorEnvelope;
use shared::utils::hashes::sha256_hex;

const FILTER_TOKEN_FIELD: &str = "token";

/// Header-first auth: "Authorization: Bearer <token>".
/// Temporarily accepts `?token=...` (deprecated) with a WARN.
pub async fn require_token(cfg: Config, req: Request<Body>, next: Next) -> Response {
    let expected = match cfg.auth_hash.as_ref() {
        Some(h) => h,
        None => {
            if cfg.debug {
                sink_ecam::record(
                    Severity::Warn,
                    Origin::Security,
                    "Auth disabled (no AUTH_HASH) — allowing request".to_string(),
                    None,
                );
                return next.run(req).await;
            } else {
                panic!("Filter auth disabled in release mode (no AUTH_HASH) — aborting");
            }
        }
    };

    let hdr_tok = bearer_token_from_header(&req);
    let q_tok   = token_from_query(&req, FILTER_TOKEN_FIELD);

    if let (Some(h), Some(q)) = (&hdr_tok, &q_tok) {
        if h != q {
            sink_ecam::record(
                Severity::Error,
                Origin::Security,
                "Conflicting token in header and query".to_string(),
                None,
            );
            return ErrorEnvelope::unauthorized("unauthorized", req.uri().path())
                .with_title("Authentication failure")
                .with_detail("Conflicting tokens in header and query")
                .into_response();
        }
    }

    let token = if let Some(t) = hdr_tok.clone() {
        t
    } else if let Some(t) = q_tok.clone() {
        sink_ecam::record(
            Severity::Warn,
            Origin::Security,
            "Query token is DEPRECATED; use Authorization: Bearer".to_string(),
            None,
        );
        t
    } else {
        sink_ecam::record(
            Severity::Error,
            Origin::Security,
            "Missing token".to_string(),
            None,
        );
        return ErrorEnvelope::unauthorized("unauthorized", req.uri().path())
            .with_title("Authentication failure")
            .with_detail("Missing Authorization: Bearer (or deprecated ?token)")
            .into_response();
    };



    let calc = sha256_hex(token.trim().as_bytes());
    if &calc != expected {
        sink_ecam::record(
            Severity::Error,
            Origin::Security,
            "Authentication failure: wrong token".to_string(),
            None,
        );
        return ErrorEnvelope::unauthorized("unauthorized", req.uri().path())
            .with_title("Authentication failure")
            .with_detail("Bad token")
            .into_response();
        error!("auth_middleware: failed: supplied_token={} (b={:?}, q={:?}), correct_token={}", token, hdr_tok.clone(), q_tok.clone(), expected);

    } else {
        info!("auth_middleware: passed: supplied_token={} (b={:?}, q={:?}), correct_token={}", token, hdr_tok.clone(), q_tok.clone(), expected);

    }

    next.run(req).await
}