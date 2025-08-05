use crate::config::Config;
use crate::observability::sinks::sink_ecam;
use crate::observability::sinks::sink_ecam::{Origin, Severity};
use axum::body::Body;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use shared::api::err::ErrorEnvelope;
use shared::utils::hashes::sha256_hex;

const FILTER_TOKEN_FIELD: &'static str = "token";

/// Middleware that enforces `?ctrl_token=...` against the configured hash.
/// If no hash is configured, it lets the request pass (with a WARN logged at init time).
pub async fn require_ctrl_token(
    cfg: Config,
    req: Request<Body>,
    next: Next,
) -> Response {
    let expected = cfg.auth_hash.as_ref();
    if expected.is_none() {
        sink_ecam::record(
            Severity::Warn,
            Origin::Security,
            "Auth hash is not set, auth disabled | allowing control action",
            None,
        );
        return next.run(req).await;
    }
    let expected = expected.unwrap();

    // Extract ctrl_token from query string
    let token_opt = req.uri().query().and_then(|q| {
        form_urlencoded::parse(q.as_bytes())
            .find(|(k, _)| k == FILTER_TOKEN_FIELD)
            .map(|(_, v)| v.into_owned())
    });

    let Some(token) = token_opt else {
        sink_ecam::record(Severity::Error, Origin::Security, format!("authentication failed: no token provided"), None);
        return ErrorEnvelope::unauthorized("unauthorized", req.uri().path())
            .with_title("Authenticaton failure")
            .with_detail("No `ctrl_token` has been supplied. Request is dropped automatically.")
            .into_response();
    };

    let calc = sha256_hex(token.as_bytes());
    if &calc != expected {
        sink_ecam::record(Severity::Error, Origin::Security, format!("authentication failed: bad token '{}'", token), None);
        return ErrorEnvelope::unauthorized("unauthorized", req.uri().path())
            .with_title("Authenticaton failure")
            .with_detail("The supplied `ctrl_token` doesn't match the controller's allowed token. Request is dropped automatically.")
            .into_response();
    }

    next.run(req).await
}
