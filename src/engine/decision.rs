use crate::types::Decision;

#[derive(Default)]
pub struct WhitelistState {
    pub all_total: u64,
    pub all_hit:   u64,
    pub any_total: u64,
    pub any_hit:   u64,
}

pub enum EarlyExit {
    None,
    Drop(String), // reason
}

/// After iterating filters ‑ decide.
pub fn final_decision(
    blacklist_hit: bool,
    wl: &WhitelistState,
    drop_reasons: &mut Vec<String>,
) -> Decision {
    if blacklist_hit {
        drop_reasons.push("blacklist".into());
        Decision::Drop
    } else if wl.all_total > 0 && wl.all_hit != wl.all_total {
        drop_reasons.push("whitelist_all_miss".into());
        Decision::Drop
    } else if wl.any_total > 0 && wl.any_hit == 0 {
        drop_reasons.push("whitelist_any_miss".into());
        Decision::Drop
    } else {
        Decision::Allow
    }
}
