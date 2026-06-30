use crate::http::HttpMessage;
use crate::http::parser::{HttpParse, ParsedHttp, content_length};
use anyhow::anyhow;
use bytes::Bytes;
use httparse::{Header, Response};

pub fn try_parse_response(buf: &[u8], max_body: usize) -> anyhow::Result<HttpParse> {
    let mut headers = [Header {
        name: "",
        value: &[],
    }; 64];
    let mut resp = Response::new(&mut headers);

    let status = match resp.parse(buf) {
        Ok(s) => s,
        Err(e) => return Err(anyhow!("httparse response error: {e}")),
    };
    if status.is_partial() {
        return Ok(HttpParse::Incomplete);
    }
    let header_end = status.unwrap();

    let code = match resp.code {
        Some(c) => c,
        None => return Ok(HttpParse::Incomplete),
    };
    let reason = resp.reason.unwrap_or("").to_string();
    let version = resp.version;

    let hdrs = resp
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
    // `saturating_add`: see `request.rs` — a hostile `Content-Length` must not
    // overflow `total_needed` and walk the body slice out of bounds.
    let total_needed = header_end.saturating_add(content_len);
    if buf.len() < total_needed {
        return Ok(HttpParse::AwaitingBody);
    }

    // Cap the body we capture for inspection at `max_body`; forwarding stays exact.
    let captured = content_len.min(max_body);
    let truncated = content_len > max_body;
    let body = Bytes::copy_from_slice(&buf[header_end..header_end + captured]);

    let msg = HttpMessage {
        method: None,
        path: None,
        status: Some(code),
        reason: Some(reason),
        headers: hdrs,
        body,
        truncated,
        version,
    };

    Ok(HttpParse::Complete(ParsedHttp { msg }, total_needed))
}
