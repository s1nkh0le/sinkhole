use crate::types::ruleset_types::RuleSetRaw;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// List of ruleset information, versioned: (`PortInfo`, `ver_info`)
///
/// Returned by:
/// - GET /ports/{port}/ruleset
/// - PUT /ports/{port}/ruleset
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FilterReplySuccessRuleset {
    pub ruleset: RuleSetRaw,
    pub ver_ruleset: String,
}
impl crate::api::ok::sealed::Sealed for FilterReplySuccessRuleset {}
impl crate::api::ok::OkData for FilterReplySuccessRuleset {}

/// Validation information
///
/// Returned by:
/// - PUT /ports/{port}/ruleset/validate
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FilterReplySuccessValidationRuleset {
    pub ruleset: RuleSetRaw,
    pub validated: bool,
    pub validation_errors: Option<String>,
}
impl crate::api::ok::sealed::Sealed for FilterReplySuccessValidationRuleset {}
impl crate::api::ok::OkData for FilterReplySuccessValidationRuleset {}

/// One historical ruleset revision for a port.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RulesetHistoryEntry {
    /// Content-hash version of this revision (markers + filters only, so a
    /// rollback to identical content shares the version of the revision it
    /// restores; a commit-message-only change keeps the same version too).
    pub version: String,
    /// Wall-clock time this revision was published, in nanoseconds.
    pub updated_at_ns: i64,
    /// The full ruleset for this revision; its commit message is
    /// `ruleset.commit_message`. Rollback is a plain re-publish of this value.
    pub ruleset: RuleSetRaw,
}

/// Full ruleset publish history for a port, newest first.
///
/// Returned by:
/// - GET /ports/{port}/ruleset/history
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FilterReplyRulesetHistory {
    pub entries: Vec<RulesetHistoryEntry>,
}
impl crate::api::ok::sealed::Sealed for FilterReplyRulesetHistory {}
impl crate::api::ok::OkData for FilterReplyRulesetHistory {}
