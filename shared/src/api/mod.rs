use std::fmt::format;
use axum::http::StatusCode;
use axum::Json;
use serde_json::Value;

pub mod ok;
pub mod err;


type ApiResponse = (StatusCode, Json<Value>);

pub fn normalize_order(order: Option<&str>) -> Result<bool, String> {
    match order.map(|s| s.to_ascii_lowercase()) {
        None => Ok(true),
        Some(ref s) if s == "asc"  => Ok(true),
        Some(ref s) if s == "desc" => Ok(false),
        Some(e) => Err(format!("order must be 'asc' or 'desc', got {}", e).into()),
    }
}

pub fn clamp_limit(opt: Option<usize>, max: usize) -> Result<usize, String> {
    let v = opt.unwrap_or(1000);
    if v == 0 { return Err(format!("limit must be > 0, got {}", v).into()); }
    Ok(v.min(max))
}

pub fn validate_range(since: i64, until: i64) -> Result<(), String> {
    if since < 0 || until < 0 || until < since {
        Err(format!("invalid since/until range: since={}, until={}", since, until).into())
    } else {
        Ok(())
    }
}