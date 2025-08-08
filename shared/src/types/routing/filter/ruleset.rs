use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use crate::types::ruleset_types::RuleSetRaw;


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
    pub validation_errors: Option<String>
}
impl crate::api::ok::sealed::Sealed for FilterReplySuccessValidationRuleset {}
impl crate::api::ok::OkData for FilterReplySuccessValidationRuleset {}

