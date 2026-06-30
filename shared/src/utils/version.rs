use crate::types::ruleset_types::*;
use fnv_rs::{Fnv64, FnvHasher};
use serde_json::{Value, json};

/// Calculates hash of `RuleSetRaw`, by hashing its `markers` and `filters`
#[inline]
pub fn calc_version_ruleset(raw: &RuleSetRaw) -> String {
    let mut markers = raw.markers.clone();
    let mut filters = raw.filters.clone();

    markers.sort_by(|a, b| a.id.cmp(&b.id));
    filters.sort_by(|a, b| a.id.cmp(&b.id));

    let canon = json!({
        "markers": markers,
        "filters": filters
    });

    calc_hash_impl(canon)
}

/// Calculates hash of `PortInfo`'s data, excluding version fields, by hashing its `name`, `comment`, `hex_color`, `public`, `upstream`, and `enabled_scopes`
#[inline]
pub fn calc_version_info(raw: &PortInfo) -> String {
    let name = raw.clone().name.unwrap_or("no_name".to_string());
    let comment = raw.clone().comment.unwrap_or("no_comment".to_string());
    let hex_color = raw.clone().hex_color.unwrap_or("no_hex_color".to_string());
    let public = raw.clone().public;
    let upstream = raw.clone().upstream;
    let mut scopes = raw.clone().enabled_scopes;

    scopes.sort_by_key(|a| a.to_string());

    let canon = json!({
        "name": name,
        "comment": comment,
        "hex_color": hex_color,
        "public": public,
        "upstream": upstream,
        "scopes": scopes
    });

    calc_hash_impl(canon)
}

/// Calculates the combined hash of the ruleset and port info by copying the hashes from `PortInfo`
#[inline]
pub fn calc_version_combined_copyover(version_info: &str, version_ruleset: &str) -> String {
    let canon = json!({
        "version_portinfo": version_info,
        "version_ruleset": version_ruleset,
    });

    calc_hash_impl(canon)
}

/// Hashes everything, and then hashes the hashes. What a comment.
#[inline]
pub fn calc_version_combined_compute(port: &PortInfo, ruleset: &RuleSetRaw) -> String {
    let version_portinfo = calc_version_info(port);
    let version_ruleset = calc_version_ruleset(ruleset);

    let canon = json!({
        "version_portinfo": version_portinfo,
        "version_ruleset": version_ruleset,
    });

    calc_hash_impl(canon)
}

/// Quick hashing for string (and non cryptographycally secure) version computation
#[inline]
fn calc_hash_impl(value: Value) -> String {
    let s = serde_json::to_string(&value).unwrap_or("error_calc_hash_impl".to_string());
    let result = Fnv64::hash(s.as_bytes());

    result.as_hex()
}
