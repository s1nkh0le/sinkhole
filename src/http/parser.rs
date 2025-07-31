use anyhow::{Result};
use bytes::Bytes;
use crate::http::request::try_parse_request;
use crate::http::response::try_parse_response;

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

impl HttpMessage {
    pub fn header_value(&self, name: &str) -> Option<&str> {
        let needle = name.to_ascii_lowercase();
        self.headers.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(&needle))
            .map(|(_, v)| v.as_str())
    }
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
// src/http/parser.rs
pub fn parse_http(buf: &[u8], inbound: bool) -> Result<Option<(ParsedHttp, usize)>> {
    if !looks_like_http(buf) {
        return Ok(None);
    }

    if inbound {
        // client → server: request first
        if let Some(r) = try_parse_request(buf)? { return Ok(Some(r)); }
        if let Some(r) = try_parse_response(buf)? { return Ok(Some(r)); }
    } else {
        // server → client: response first
        if let Some(r) = try_parse_response(buf)? { return Ok(Some(r)); }
        if let Some(r) = try_parse_request(buf)? { return Ok(Some(r)); }
    }

    Ok(None)
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

pub fn content_length(hdrs: &[(String, String)]) -> Option<usize> {
    for (k, v) in hdrs {
        if k == "content-length" {
            if let Ok(n) = v.trim().parse::<usize>() {
                return Some(n);
            }
        }
    }
    None
}
