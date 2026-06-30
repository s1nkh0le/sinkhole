use axum::body::Body;
use axum::http::Request;
use axum::http::header::AUTHORIZATION;

const BEARER: &str = "Bearer ";

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
