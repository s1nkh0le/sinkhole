use crate::types::ruleset_types::{PortInfo, RuleSetRaw};
use crate::utils::version::{
    calc_version_combined_copyover, calc_version_info, calc_version_ruleset,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Complete, transferrable port information (includes `PortInfo`, `RuleSetRaw` and version).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PortSnapshot {
    /// Port info
    pub info: PortInfo,
    /// Port ruleset
    pub ruleset: RuleSetRaw,

    /// Version for `PortInfo` (hash of all its fields)
    pub ver_info: String,
    /// Version for `RuleSetRaw` (hash of all its fields)
    pub ver_ruleset: String,
    /// Combined hash for full `PortSnapshot` (`PortInfo` + `RuleSetRaw`)
    pub ver_combined: String,
}

impl PortSnapshot {
    pub fn compute_versions(&mut self) {
        self.ver_info = calc_version_info(&self.info);
        self.ver_ruleset = calc_version_ruleset(&self.ruleset);
        self.ver_combined = calc_version_combined_copyover(&self.ver_info, &self.ver_ruleset);
    }
}
