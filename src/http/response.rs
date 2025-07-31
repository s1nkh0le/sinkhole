use crate::http::parser::{content_length, HttpPart, ParsedHttp};
use crate::http::HttpMessage;
use anyhow::anyhow;
use bytes::Bytes;
use httparse::{Header, Response};

pub fn try_parse_response(buf: &[u8]) -> anyhow::Result<Option<(ParsedHttp, usize)>> {
    let mut headers = [Header { name: "", value: &[] }; 64];
    let mut resp = Response::new(&mut headers);

    let status = match resp.parse(buf) {
        Ok(s) => s,
        Err(e) => return Err(anyhow!("httparse response error: {e}")),
    };
    if status.is_partial() {
        return Ok(None);
    }
    let header_end = status.unwrap();

    let code = match resp.code {
        Some(c) => c,
        None => return Ok(None),
    };
    let reason = resp.reason.unwrap_or("").to_string();

    let hdrs = resp.headers.iter().map(|h| {
        (
            h.name.to_ascii_lowercase(),
            String::from_utf8_lossy(h.value).into_owned(),
        )
    }).collect::<Vec<_>>();

    let content_len = content_length(&hdrs).unwrap_or(0);
    let total_needed = header_end + content_len;
    if buf.len() < total_needed {
        return Ok(None);
    }

    let body = Bytes::copy_from_slice(&buf[header_end..header_end + content_len]);

    let msg = HttpMessage {
        method: None,
        path: None,
        status: Some(code),
        reason: Some(reason),
        headers: hdrs,
        body,
        truncated: false,
    };

    Ok(Some((ParsedHttp { part: HttpPart::Response, msg }, total_needed)))
}