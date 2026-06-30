//! Test-only builders for matcher unit tests.
//!
//! Constructing a [`MatchInput`] by hand is verbose because of the borrows in
//! [`MessageCtx`]. [`Scenario`] owns the message data and hands out a borrowed
//! [`MatchInput`] on demand, so each test is a couple of lines.

use crate::engine::dispatcher::MessageCtx;
use crate::http::HttpMessage;
use crate::matcher::MatchInput;
use bytes::Bytes;
use shared::types::base_types::Scope;

/// Build an [`HttpMessage`] for tests. All parts are optional; pass `None`/empty
/// for the ones a given scope does not care about.
pub fn http(
    method: Option<&str>,
    path: Option<&str>,
    status: Option<u16>,
    headers: &[(&str, &str)],
    body: &[u8],
) -> HttpMessage {
    HttpMessage {
        method: method.map(str::to_owned),
        path: path.map(str::to_owned),
        status,
        reason: None,
        headers: headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        body: Bytes::copy_from_slice(body),
        truncated: false,
        version: Some(1),
    }
}

/// Owns the message data for one matcher evaluation and lends a [`MatchInput`].
pub struct Scenario {
    http: Option<HttpMessage>,
    raw: Vec<u8>,
    ctx: MessageCtx<'static>,
}

impl Scenario {
    /// A scenario carrying a parsed HTTP message (raw mirrors nothing useful).
    pub fn with_http(msg: HttpMessage) -> Self {
        Self {
            http: Some(msg),
            raw: Vec::new(),
            ctx: base_ctx(),
        }
    }

    /// A scenario carrying only a raw (non-HTTP) byte stream.
    pub fn with_raw(raw: &[u8]) -> Self {
        Self {
            http: None,
            raw: raw.to_vec(),
            ctx: base_ctx(),
        }
    }

    /// Borrow a [`MatchInput`] for `scope`.
    pub fn input(&self, scope: Scope) -> MatchInput<'_> {
        MatchInput {
            scope,
            http: self.http.as_ref(),
            raw: &self.raw,
            ctx: &self.ctx,
        }
    }
}

/// A `MessageCtx` whose borrowed fields are static dummies; the matchers under
/// test read `inp.raw`/`inp.http` (owned by [`Scenario`]) and the timing fields
/// here, never `ctx.raw`/`ctx.src_ip`.
fn base_ctx() -> MessageCtx<'static> {
    MessageCtx {
        conn_id: 1,
        seq: 0,
        inbound: true,
        public_port: 80,
        upstream_port: 80,
        raw: &[],
        raw_capture_limit: usize::MAX,
        http: None,
        protocol: shared::types::base_types::Protocol::RawBinary,
        src_ip: "127.0.0.1",
        src_port: 1234,
        dst_ip: "127.0.0.1",
        dst_port: 80,
        conn_start_ns: 0,
        conn_prev_msg_ns: 0,
        msg_now_ns: 0,
    }
}
