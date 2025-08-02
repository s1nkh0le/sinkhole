use crate::http::HttpMessage;
use crate::http::parser::{HttpPart, ParsedHttp, content_length};
use anyhow::{Result, anyhow};
use bytes::Bytes;
use httparse::{Header, Request};

pub fn try_parse_request(buf: &[u8]) -> Result<Option<(ParsedHttp, usize)>> {
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
        return Ok(None);
    }
    let header_end = status.unwrap();

    // If these are missing it's not a real request -> return None instead of Err
    let method = match req.method {
        Some(m) => m.to_string(),
        None => return Ok(None),
    };
    let path = match req.path {
        Some(p) => p.to_string(),
        None => return Ok(None),
    };

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
    let total_needed = header_end + content_len;
    if buf.len() < total_needed {
        return Ok(None);
    }

    let body = Bytes::copy_from_slice(&buf[header_end..header_end + content_len]);

    let msg = HttpMessage {
        method: Some(method),
        path: Some(path),
        status: None,
        reason: None,
        headers: hdrs,
        body,
        truncated: false,
    };

    Ok(Some((
        ParsedHttp {
            part: HttpPart::Request,
            msg,
        },
        total_needed,
    )))
}
