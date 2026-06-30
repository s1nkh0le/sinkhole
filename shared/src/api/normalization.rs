/// Parse the `order` query param into `asc: bool`. Defaults to **descending**
/// (newest first) when unspecified — the operator-facing record/event endpoints
/// almost always want the latest rows. (Note: the controller→filter sync path
/// passes `order=asc` explicitly and the filter has its own default, so this
/// default only governs the controller's frontend-facing endpoints.)
#[inline]
pub fn normalize_order(order: Option<&str>) -> Result<bool, String> {
    match order.map(|s| s.to_ascii_lowercase()) {
        None => Ok(false),
        Some(ref s) if s == "asc" => Ok(true),
        Some(ref s) if s == "desc" => Ok(false),
        Some(e) => Err(format!("order must be 'asc' or 'desc', got {}", e)),
    }
}

#[inline]
pub fn clamp_limit(opt: Option<usize>, max: usize) -> Result<usize, String> {
    let v = opt.unwrap_or(1000);
    if v == 0 {
        return Err(format!("limit must be > 0, got {}", v));
    }
    Ok(v.min(max))
}

#[inline]
pub fn validate_range(since: u64, until: u64) -> Result<(), String> {
    if until < since {
        Err(format!(
            "invalid since/until range: since={}, until={}",
            since, until
        ))
    } else {
        Ok(())
    }
}

/// Split a comma-separated query value (e.g. `?positive_markers=a,b,c`) into a
/// trimmed, de-duplicated, sorted list. Empty segments are dropped, so trailing
/// commas and `""` collapse to an empty list. Used for the marker filters: axum's
/// default `Query` (serde_urlencoded) cannot deserialize `Vec` from a query
/// string at all, so list params are carried as a single comma-joined string.
#[inline]
pub fn split_csv(s: Option<&str>) -> Vec<String> {
    let mut v: Vec<String> = match s {
        None => return Vec::new(),
        Some(raw) => raw
            .split(',')
            .map(str::trim)
            .filter(|seg| !seg.is_empty())
            .map(str::to_owned)
            .collect(),
    };
    v.sort_unstable();
    v.dedup();
    v
}

/// Parse a comma-separated `protocol` query value into canonical protocol strings
/// (e.g. `?protocol=raw-ascii,http/1.1`). Each token must be a known
/// [`Protocol`](crate::types::base_types::Protocol); an unknown one is a hard error
/// (returned to the caller as a 400). Returns deduped, sorted canonical strings.
#[inline]
pub fn parse_protocols(csv: Option<&str>) -> Result<Vec<String>, String> {
    use crate::types::base_types::Protocol;
    let mut out: Vec<String> = Vec::new();
    for tok in split_csv(csv) {
        let p: Protocol = tok.parse()?;
        out.push(p.as_str().to_string());
    }
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

#[inline]
pub fn u64_to_i64_sat(v: u64) -> i64 {
    if v > i64::MAX as u64 {
        i64::MAX
    } else {
        v as i64
    }
}

#[cfg(test)]
mod tests {
    use super::split_csv;

    #[test]
    fn split_csv_handles_none_and_empty() {
        assert!(split_csv(None).is_empty());
        assert!(split_csv(Some("")).is_empty());
        assert!(split_csv(Some(" , ,")).is_empty());
    }

    #[test]
    fn split_csv_trims_dedups_and_sorts() {
        assert_eq!(split_csv(Some("auth_ok")), vec!["auth_ok"]);
        assert_eq!(
            split_csv(Some(" b , a ,a, ,b")),
            vec!["a".to_string(), "b".to_string()]
        );
    }
}
