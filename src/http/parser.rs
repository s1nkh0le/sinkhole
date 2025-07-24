use anyhow::{Result, anyhow};
use bytes::Bytes;
use httparse::{Header, Request, Response};

#[derive(Debug, Clone)]
pub struct HttpMessage {
    pub method: Option<String>, // request
    pub path: Option<String>,   // request
    pub status: Option<u16>,    // response
    pub reason: Option<String>, // response
    pub headers: Vec<(String, String)>,
    pub body: Bytes, // possibly truncated outside
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum HttpPart {
    Request,
    Response,
}

/// ParsedHttp = header info + computed body length
#[derive(Debug, Clone)]
pub struct ParsedHttp {
    pub part: HttpPart,
    pub msg: HttpMessage,
}

/// Try to parse an HTTP/1.x request or response from `buf`.
/// Returns (ParsedHttp, total_bytes_consumed) on success.
/// If it doesn't look like HTTP, returns Ok(None).
pub fn parse_http(buf: &[u8]) -> Result<Option<(ParsedHttp, usize)>> {
    // very cheap check
    if !looks_like_http(buf) {
        return Ok(None);
    }

    // Try request first
    if let Some((parsed, used)) = parse_request(buf)? {
        return Ok(Some((parsed, used)));
    }
    // Try response
    if let Some((parsed, used)) = parse_response(buf)? {
        return Ok(Some((parsed, used)));
    }

    // Not HTTP after all
    Ok(None)
}

fn looks_like_http(buf: &[u8]) -> bool {
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

fn parse_request(buf: &[u8]) -> Result<Option<(ParsedHttp, usize)>> {
    let mut headers = [Header {
        name: "",
        value: &[],
    }; 64];
    let mut req = Request::new(&mut headers);

    let status = req.parse(buf).map_err(|e| anyhow!("httparse: {e}"))?;
    if status.is_partial() {
        return Ok(None);
    }
    let header_end = status.unwrap();

    let method = req.method.ok_or_else(|| anyhow!("no method"))?.to_string();
    let path = req.path.ok_or_else(|| anyhow!("no path"))?.to_string();

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
        // body not fully present
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
        truncated: false, // truncation handled by caller
    };

    Ok(Some((
        ParsedHttp {
            part: HttpPart::Request,
            msg,
        },
        total_needed,
    )))
}

fn parse_response(buf: &[u8]) -> Result<Option<(ParsedHttp, usize)>> {
    let mut headers = [Header {
        name: "",
        value: &[],
    }; 64];
    let mut resp = Response::new(&mut headers);

    let status = resp.parse(buf).map_err(|e| anyhow!("httparse: {e}"))?;
    if status.is_partial() {
        return Ok(None);
    }
    let header_end = status.unwrap();

    let code = resp.code.ok_or_else(|| anyhow!("no status code"))?;
    let reason = resp.reason.unwrap_or("").to_string();

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
    let total_needed = header_end + content_len;

    if buf.len() < total_needed {
        return Ok(None);
    }

    let body = Bytes::copy_from_slice(&buf[header_end..header_end + content_len]);

    let msg = HttpMessage {
        method: None,
        path: None,
        status: Some(code as u16),
        reason: Some(reason),
        headers: hdrs,
        body,
        truncated: false,
    };

    Ok(Some((
        ParsedHttp {
            part: HttpPart::Response,
            msg,
        },
        total_needed,
    )))
}

fn content_length(hdrs: &[(String, String)]) -> Option<usize> {
    for (k, v) in hdrs {
        if k == "content-length" {
            if let Ok(n) = v.trim().parse::<usize>() {
                return Some(n);
            }
        }
    }
    None
}
