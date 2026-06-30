use crate::http::HttpMessage;
use crate::http::parser::{HttpParse, ParsedHttp, content_length};
use anyhow::{Result, anyhow};
use bytes::Bytes;
use httparse::{Header, Request};

pub fn try_parse_request(buf: &[u8], max_body: usize) -> Result<HttpParse> {
    let mut headers = [Header {
        name: "",
        value: &[],
    }; 64];
    let mut req = Request::new(&mut headers);

    let status = match req.parse(buf) {
        Ok(s) => s,
        Err(e) => return Err(anyhow!("httparse request error: {e}")),
    };
    if status.is_partial() {
        return Ok(HttpParse::Incomplete);
    }
    let header_end = status.unwrap();

    // If these are missing it's not a real request -> not HTTP (Incomplete).
    let method = match req.method {
        Some(m) => m.to_string(),
        None => return Ok(HttpParse::Incomplete),
    };
    let path = match req.path {
        Some(p) => p.to_string(),
        None => return Ok(HttpParse::Incomplete),
    };
    let version = req.version;

    let hdrs = req
        .headers
        .iter()
        .map(|h| {
            (
                h.name.to_ascii_lowercase(),
                String::from_utf8_lossy(h.value).into_owned(),
            )
        })
        .collect::<Vec<_>>();

    let content_len = content_length(&hdrs).unwrap_or(0);
    // `saturating_add`: a hostile/absurd `Content-Length` (up to usize::MAX) must
    // not overflow `total_needed`. On overflow it saturates to usize::MAX, so the
    // check below always takes the `AwaitingBody` branch and the body slice (only
    // reached when `buf.len() >= total_needed`) can never go out of bounds. The
    // oversized "body" is then bounded by the pump's raw-flush threshold.
    let total_needed = header_end.saturating_add(content_len);
    if buf.len() < total_needed {
        // Head is valid and complete; we're committed to this message, just waiting
        // for the rest of the body.
        return Ok(HttpParse::AwaitingBody);
    }

    // Cap the body we capture for inspection at `max_body`. Forwarding still uses
    // `total_needed` (byte-exact), so the proxy stays transparent.
    let captured = content_len.min(max_body);
    let truncated = content_len > max_body;
    let body = Bytes::copy_from_slice(&buf[header_end..header_end + captured]);

    let msg = HttpMessage {
        method: Some(method),
        path: Some(path),
        status: None,
        reason: None,
        headers: hdrs,
        body,
        truncated,
        version,
    };

    Ok(HttpParse::Complete(ParsedHttp { msg }, total_needed))
}
