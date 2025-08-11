use axum::body::Body;
use axum::http::header::AUTHORIZATION;
use axum::http::Request;

const BEARER: &'static str = "Bearer ";

// Extract "Authorization: Bearer <token>"
pub fn bearer_token_from_header(req: &Request<Body>) -> Option<String> {
    let val = req.headers().get(AUTHORIZATION)?;
    let s = val.to_str().ok()?.trim();
    let pref = BEARER;
    if s.len() >= pref.len() && s[..pref.len()].eq_ignore_ascii_case(pref) {
        Some(s[pref.len()..].trim().to_string())
    } else {
        None
    }
}

// Temporary fallback: ?ctrl_token=...
pub fn token_from_query(req: &Request<Body>, token_field: &'static str) -> Option<String> {
    req.uri().query().and_then(|q| {
        form_urlencoded::parse(q.as_bytes())
            .find(|(k, _)| k == token_field)
            .map(|(_, v)| v.into_owned())
    })
}