use crate::http::request::try_parse_request;
use crate::http::response::try_parse_response;
use anyhow::Result;
use bytes::Bytes;
use shared::types::base_types::Protocol;

#[derive(Debug, Clone)]
pub struct HttpMessage {
    pub method: Option<String>, // request
    pub path: Option<String>,   // request
    pub status: Option<u16>,    // response
    pub reason: Option<String>, // response
    pub headers: Vec<(String, String)>,
    pub body: Bytes, // possibly truncated outside
    pub truncated: bool,
    /// HTTP/1 minor version from httparse: `0` = HTTP/1.0, `1` = HTTP/1.1.
    pub version: Option<u8>,
}

impl HttpMessage {
    pub fn header_value(&self, name: &str) -> Option<&str> {
        let needle = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(&needle))
            .map(|(_, v)| v.as_str())
    }
}

/// Parsed HTTP message plus its computed body length. Request vs response is
/// already encoded in `HttpMessage` itself (method/path vs status/reason).
#[derive(Debug, Clone)]
pub struct ParsedHttp {
    pub msg: HttpMessage,
}

/// Outcome of attempting to parse one HTTP message from a buffer. The middle state
/// is what lets the proxy distinguish "a committed HTTP message whose body is still
/// arriving" (keep buffering, never raw/idle-flush mid-message) from "not HTTP /
/// head still arriving" (eligible for the raw fast-path and idle-flush).
#[derive(Debug)]
pub enum HttpParse {
    /// A complete message and the number of bytes it consumed.
    Complete(ParsedHttp, usize),
    /// A valid request/response head parsed, but the `Content-Length` body hasn't
    /// fully arrived yet — a committed HTTP message in progress.
    AwaitingBody,
    /// Not (yet) recognizable as a complete-or-committed HTTP message: a partial
    /// head, a malformed line, or simply non-HTTP bytes.
    Incomplete,
}

/// Try to parse an HTTP/1.x request or response from `buf`.
/// Returns `(ParsedHttp, total_bytes_consumed)` on success, or `Ok(None)` when
/// the buffer is not (yet) a complete HTTP message — including the partial,
/// non-HTTP, and malformed cases. A parse error from either underlying parser is
/// treated as "not this kind of message" rather than a hard failure, so the
/// caller can fall back to raw-stream handling / wait for more bytes.
///
/// # Known limitations
/// This is a pragmatic HTTP/1.x parser, not a conformant one:
/// - Body framing uses `Content-Length` only; `Transfer-Encoding: chunked` is
///   not decoded (a chunked body is left in the raw stream).
/// - [`looks_like_http`] sniffing is loose (any uppercase first byte is treated
///   as a possible request line), so non-HTTP traffic can be tentatively parsed.
/// - HTTP/2 and request pipelining are not handled.
///
/// Byte-oriented matchers (`Scope::Stream`) still see the unparsed bytes, so
/// these limitations affect HTTP-field matchers, not raw-stream matching.
pub fn parse_http(buf: &[u8], inbound: bool, max_body: usize) -> HttpParse {
    if !looks_like_http(buf) {
        return HttpParse::Incomplete;
    }

    // Try the parser matching the traffic direction first, then the other as a
    // fallback (so the response parser choking on a request line never aborts
    // parsing). A hard `Err` from a parser is treated as "not this kind" —
    // `Incomplete` — so we fall through to the other parser. `Complete` and
    // `AwaitingBody` are both decisive and returned immediately.
    let attempt = |res: Result<HttpParse>| -> Option<HttpParse> {
        match res {
            Ok(HttpParse::Incomplete) | Err(_) => None,
            Ok(decisive) => Some(decisive),
        }
    };

    let (first, second): (Result<HttpParse>, Result<HttpParse>) = if inbound {
        (
            try_parse_request(buf, max_body),
            try_parse_response(buf, max_body),
        )
    } else {
        (
            try_parse_response(buf, max_body),
            try_parse_request(buf, max_body),
        )
    };

    attempt(first)
        .or_else(|| attempt(second))
        .unwrap_or(HttpParse::Incomplete)
}

pub fn looks_like_http(buf: &[u8]) -> bool {
    // naive: starts with [A-Z]{3,10} space "/" or "HTTP/"
    if buf.len() < 4 {
        return false;
    }
    // request line typical: "GET /", "POST /"
    if buf[0].is_ascii_uppercase() {
        return true;
    }
    // response line: "HTTP/1.1 200"
    if buf.starts_with(b"HTTP/") {
        return true;
    }
    false
}

/// HTTP/2 client connection preface. A connection that opens with this is HTTP/2;
/// we **tag** it but do not parse h2 frames (HPACK is stateful — out of scope).
pub const H2_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n";

/// Map a parsed HTTP/1 message to its protocol tag (httparse minor version:
/// `0` = 1.0, else treat as 1.1).
#[inline]
pub fn http_protocol(version: Option<u8>) -> Protocol {
    match version {
        Some(0) => Protocol::Http10,
        _ => Protocol::Http11,
    }
}

/// Bytes treated as "text" for raw ascii/binary classification: printable ASCII
/// plus tab / newline / carriage-return.
#[inline]
fn is_text_byte(b: u8) -> bool {
    matches!(b, 0x09 | 0x0A | 0x0D | 0x20..=0x7E)
}

/// Classify an unframed (non-HTTP) chunk as `raw-ascii` vs `raw-binary`.
///
/// We sample only the first [`RAW_CLASSIFY_SAMPLE`] bytes — a protocol reveals its
/// nature early and this bounds the cost on large chunks — and call it ascii when
/// **≥80%** of the sample is text. The 80% (not 100%) leaves headroom for the stray
/// control byte a CLI/TUI prompt or an exploit payload carries, while genuine
/// binary protocols fall through to `raw-binary`.
pub fn classify_raw(buf: &[u8]) -> Protocol {
    const RAW_CLASSIFY_SAMPLE: usize = 1024;
    if buf.is_empty() {
        return Protocol::RawBinary;
    }
    let n = buf.len().min(RAW_CLASSIFY_SAMPLE);
    let text = buf[..n].iter().filter(|&&b| is_text_byte(b)).count();
    if text * 100 >= n * 80 {
        Protocol::RawAscii
    } else {
        Protocol::RawBinary
    }
}

pub fn content_length(hdrs: &[(String, String)]) -> Option<usize> {
    for (k, v) in hdrs {
        if k == "content-length"
            && let Ok(n) = v.trim().parse::<usize>()
        {
            return Some(n);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    fn msg(headers: &[(&str, &str)]) -> HttpMessage {
        HttpMessage {
            method: Some("GET".into()),
            path: Some("/".into()),
            status: None,
            reason: None,
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            body: Bytes::new(),
            truncated: false,
            version: Some(1),
        }
    }

    /// Parse and require a `Complete` message, returning `(ParsedHttp, used)`.
    fn complete(buf: &[u8], inbound: bool, max_body: usize) -> (ParsedHttp, usize) {
        match parse_http(buf, inbound, max_body) {
            HttpParse::Complete(p, used) => (p, used),
            other => panic!("expected Complete, got {other:?}"),
        }
    }

    #[test]
    fn absurd_content_length_does_not_panic() {
        // Regression: a hostile `Content-Length` (usize::MAX) must not overflow
        // `header_end + content_len` and walk the body slice out of bounds. It
        // should be treated as a committed message still awaiting its (impossible)
        // body, never a panic.
        let buf = format!(
            "POST /u HTTP/1.1\r\nContent-Length: {}\r\n\r\nx",
            usize::MAX
        );
        assert!(matches!(
            parse_http(buf.as_bytes(), true, 1024),
            HttpParse::AwaitingBody
        ));
        // Also the saturating boundary just below the type max.
        let buf2 = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\nx", u64::MAX);
        assert!(matches!(
            parse_http(buf2.as_bytes(), false, 1024),
            HttpParse::AwaitingBody | HttpParse::Incomplete
        ));
    }

    #[test]
    fn header_value_is_case_insensitive() {
        let m = msg(&[("Content-Type", "application/json")]);
        assert_eq!(m.header_value("content-type"), Some("application/json"));
        assert_eq!(m.header_value("CONTENT-TYPE"), Some("application/json"));
    }

    #[test]
    fn header_value_absent_returns_none() {
        let m = msg(&[("Host", "example.com")]);
        assert_eq!(m.header_value("authorization"), None);
    }

    // ── looks_like_http (sniffing) ────────────────────────────────────────────

    #[test]
    fn sniffing_rejects_short_and_lowercase() {
        assert!(!looks_like_http(b"GET")); // < 4 bytes
        assert!(!looks_like_http(b"hello world")); // lowercase first byte
    }

    #[test]
    fn sniffing_accepts_request_and_response_lines() {
        assert!(looks_like_http(b"GET /"));
        assert!(looks_like_http(b"HTTP/1.1 200 OK"));
    }

    #[test]
    fn non_http_buffer_is_incomplete() {
        assert!(matches!(
            parse_http(b"\x00\x01\x02\x03 binary junk", true, 1024),
            HttpParse::Incomplete
        ));
    }

    // ── request framing ────────────────────────────────────────────────────────

    #[test]
    fn parses_simple_get_request() {
        let buf = b"GET /hello HTTP/1.1\r\nHost: x\r\n\r\n";
        let (parsed, used) = complete(buf, true, 1024);
        assert_eq!(used, buf.len());
        assert_eq!(parsed.msg.method.as_deref(), Some("GET"));
        assert_eq!(parsed.msg.path.as_deref(), Some("/hello"));
        assert_eq!(parsed.msg.status, None);
        assert_eq!(parsed.msg.version, Some(1));
        assert!(parsed.msg.body.is_empty());
        assert!(!parsed.msg.truncated);
    }

    #[test]
    fn captures_http_version() {
        let (p10, _) = complete(b"GET / HTTP/1.0\r\n\r\n", true, 64);
        assert_eq!(http_protocol(p10.msg.version), Protocol::Http10);
        let (p11, _) = complete(b"GET / HTTP/1.1\r\n\r\n", true, 64);
        assert_eq!(http_protocol(p11.msg.version), Protocol::Http11);
    }

    #[test]
    fn parses_request_with_full_content_length_body() {
        let buf = b"POST /p HTTP/1.1\r\nContent-Length: 5\r\n\r\nhello";
        let (parsed, used) = complete(buf, true, 1024);
        assert_eq!(used, buf.len());
        assert_eq!(&parsed.msg.body[..], b"hello");
        assert!(!parsed.msg.truncated);
    }

    #[test]
    fn incomplete_body_awaits_body() {
        // Content-Length promises 5 bytes, only 2 present → committed, awaiting body.
        let buf = b"POST /p HTTP/1.1\r\nContent-Length: 5\r\n\r\nhe";
        assert!(matches!(
            parse_http(buf, true, 1024),
            HttpParse::AwaitingBody
        ));
    }

    #[test]
    fn partial_headers_are_incomplete() {
        // No terminating CRLF CRLF yet → head not parsed.
        let buf = b"GET / HTTP/1.1\r\nHost: x";
        assert!(matches!(parse_http(buf, true, 1024), HttpParse::Incomplete));
    }

    #[test]
    fn oversized_body_is_truncated_but_fully_consumed() {
        // CL = 10, max_body = 4: body captured is capped, truncated flag set, but
        // `used` still spans the whole message so forwarding stays byte-exact.
        let buf = b"POST /p HTTP/1.1\r\nContent-Length: 10\r\n\r\n0123456789";
        let (parsed, used) = complete(buf, true, 4);
        assert_eq!(used, buf.len());
        assert!(parsed.msg.truncated);
        assert_eq!(&parsed.msg.body[..], b"0123");
    }

    #[test]
    fn pipelined_requests_consume_only_the_first() {
        let buf = b"GET /a HTTP/1.1\r\n\r\nGET /b HTTP/1.1\r\n\r\n";
        let (parsed, used) = complete(buf, true, 1024);
        assert_eq!(parsed.msg.path.as_deref(), Some("/a"));
        assert!(
            used < buf.len(),
            "only the first message should be consumed"
        );
    }

    // ── response framing ────────────────────────────────────────────────────────

    #[test]
    fn parses_response_with_status_reason_and_body() {
        let buf = b"HTTP/1.1 404 Not Found\r\nContent-Length: 2\r\n\r\nno";
        let (parsed, used) = complete(buf, false, 1024);
        assert_eq!(used, buf.len());
        assert_eq!(parsed.msg.status, Some(404));
        assert_eq!(parsed.msg.reason.as_deref(), Some("Not Found"));
        assert_eq!(parsed.msg.method, None);
        assert_eq!(&parsed.msg.body[..], b"no");
    }

    // ── protocol classification ──────────────────────────────────────────────────

    #[test]
    fn classify_raw_ascii_vs_binary() {
        assert_eq!(
            classify_raw(b"MENU> choose an option\n"),
            Protocol::RawAscii
        );
        // mostly-text with a stray control byte still counts as ascii (80% rule)
        assert_eq!(classify_raw(b"login: admin\x01\n"), Protocol::RawAscii);
        // mostly non-printable → binary
        assert_eq!(
            classify_raw(&[0u8, 1, 2, 3, 4, 5, 0xff, 0xfe, b'a', b'b']),
            Protocol::RawBinary
        );
    }

    #[test]
    fn h2_preface_is_recognizable() {
        let conn = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n\x00\x00";
        assert!(conn.starts_with(H2_PREFACE));
        // ...and it is NOT a parseable HTTP/1 message
        assert!(matches!(parse_http(conn, true, 64), HttpParse::Incomplete));
    }

    // ── content_length helper ─────────────────────────────────────────────────

    #[test]
    fn content_length_parses_trims_and_rejects_non_numeric() {
        assert_eq!(
            content_length(&[("content-length".into(), "5".into())]),
            Some(5)
        );
        assert_eq!(
            content_length(&[("content-length".into(), "  7  ".into())]),
            Some(7)
        );
        assert_eq!(
            content_length(&[("content-length".into(), "abc".into())]),
            None
        );
        assert_eq!(content_length(&[("host".into(), "x".into())]), None);
    }
}
