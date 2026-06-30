use axum::Json;
use axum::http::StatusCode;
use serde_json::Value;
use shared::api::err::ErrorEnvelope;
use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub enum CtrlErr {
    Timeout,
    Network(reqwest::Error),
    Http(u16),
    Auth,
    Json(anyhow::Error),
    CustomEnvelope(ErrorEnvelope),
    /// The service was removed (e.g. concurrently) between an existence check and
    /// the operation — surfaces as a clean 404 rather than an upstream error.
    ServiceGone,
}

impl CtrlErr {
    /// Map a filter-relay error to the controller's own `ErrorEnvelope`, with a
    /// faithful HTTP status. This is the single source of truth for how a relay
    /// failure surfaces to the operator/UI, so every relay handler behaves the
    /// same way:
    ///
    /// - the filter **responded with a structured error** (`CustomEnvelope`) →
    ///   propagate *its* status code, title, detail and machine id verbatim
    ///   (e.g. a `400` validation error stays a `400`, not a `502`);
    /// - the filter **could not be reached / misbehaved** (timeout, network,
    ///   bad payload, unexpected status, refused our token) → a gateway error
    ///   (`502`/`504`) that makes clear the fault is upstream, not the caller's.
    ///
    /// `instance` is the controller request path, recorded on the envelope.
    pub fn into_envelope(self, instance: &str) -> ErrorEnvelope {
        match self {
            CtrlErr::Timeout => ErrorEnvelope::new(StatusCode::GATEWAY_TIMEOUT, "Filter timeout")
                .with_error("filter_timeout")
                .with_instance(instance)
                .with_detail("the filter did not respond in time"),
            CtrlErr::Network(e) => ErrorEnvelope::bad_gateway("filter_unreachable", instance)
                .with_title("Filter unreachable")
                .with_detail(e.to_string()),
            CtrlErr::Auth => ErrorEnvelope::bad_gateway("filter_auth", instance)
                .with_title("Filter rejected controller credentials")
                .with_detail(
                    "the controller's token for this filter was refused (filter returned 401)",
                ),
            CtrlErr::Json(e) => ErrorEnvelope::bad_gateway("bad_filter_payload", instance)
                .with_title("Invalid filter response")
                .with_detail(e.to_string()),
            CtrlErr::Http(code) => ErrorEnvelope::bad_gateway("filter_http_error", instance)
                .with_title("Filter returned an unexpected status")
                .with_detail(format!("filter responded with HTTP {code}")),
            CtrlErr::ServiceGone => ErrorEnvelope::not_found("service_not_found", instance)
                .with_title("Unknown service")
                .with_detail("the service was removed before the request completed"),
            CtrlErr::CustomEnvelope(env) => {
                // Faithfully re-emit the filter's own structured error, re-rooting
                // the instance to this controller request.
                let code = StatusCode::from_u16(env.code).unwrap_or(StatusCode::BAD_GATEWAY);
                let mut out = ErrorEnvelope::new(code, env.title).with_instance(instance);
                if let Some(detail) = env.detail {
                    out = out.with_detail(detail);
                }
                if let Some(error) = env.error {
                    out = out.with_error(error);
                }
                out
            }
        }
    }

    /// Convenience: [`Self::into_envelope`] rendered as an axum response tuple.
    pub fn into_result(self, instance: &str) -> (StatusCode, Json<Value>) {
        self.into_envelope(instance).into_result()
    }
}

impl Display for CtrlErr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use CtrlErr::*;
        match self {
            Timeout => write!(f, "Request timed out"),
            Network(e) => write!(f, "Network error: {}", e),
            Http(c) => write!(f, "HTTP {}", c),
            Auth => write!(f, "Unauthorized"),
            Json(e) => write!(f, "JSON decode error: {}", e),
            ServiceGone => write!(f, "service removed"),
            CustomEnvelope(env) => write!(f, "ErrorEnvelope: {}", env), // assumes Display for ErrorEnvelope
        }
    }
}

impl std::error::Error for CtrlErr {}

impl From<reqwest::Error> for CtrlErr {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            CtrlErr::Timeout
        } else {
            CtrlErr::Network(e)
        }
    }
}
