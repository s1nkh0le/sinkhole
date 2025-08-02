use serde_json::json;
use shared::rules_model::*;
use shared::utils::hashes::sha256_hex;

pub fn calc_version(raw: &RuleSetRaw) -> anyhow::Result<String> {
    // Make canonical: sort by id, strip comment
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
