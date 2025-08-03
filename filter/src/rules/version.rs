use serde_json::json;
use shared::rules_model::*;
use shared::utils::hashes::sha256_hex;

pub fn calc_version_config_only_no_ruleset(raw: &PortInfo) -> anyhow::Result<String> {
    let mut name = raw.name.clone();
    let mut hex_color = raw.hex_color.clone();
    let mut comment = raw.comment.clone();
    let mut public = raw.public.clone();
    let mut upstream = raw.upstream.clone();
    let mut enabled_scopes = raw.enabled_scopes.clone();
    // We don't include version, as that only depends on ruleset

    enabled_scopes.sort();

    let canon = json!({
        "name": name,
        "hex_color": hex_color,
        "comment": comment,
        "public": public,
        "upstream": upstream,
        "enabled_scopes": enabled_scopes,
    });

    let s = serde_json::to_string(&canon)?;
    Ok(sha256_hex(s.as_bytes()))
}

pub fn calc_version_raw_ruleset(raw: &RuleSetRaw) -> anyhow::Result<String> {
    let mut markers = raw.markers.clone();
    let mut filters = raw.filters.clone();

    markers.sort_by(|a, b| a.id.cmp(&b.id));
    filters.sort_by(|a, b| a.id.cmp(&b.id));

    let canon = json!({
        "markers": markers,
        "filters": filters
    });

    let s = serde_json::to_string(&canon)?;
    Ok(sha256_hex(s.as_bytes()))
}
