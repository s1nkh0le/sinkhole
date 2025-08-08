use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use crate::types::base_types::Scope;
use crate::types::ruleset_types::PortInfo;



/** REQUESTS **/

/// Format for pushing a configuration update to the filter.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, IntoParams)]
pub struct FilterRequestPutBodyConfigUpdate {
    pub name: Option<String>,
    pub comment: Option<String>,
    pub hex_color: Option<String>,
    pub enabled_scopes: Option<Vec<Scope>>,
}





/** RESPONSES **/

/// List of port information, versioned: (`PortInfo`, `ver_info`)
///
/// Returned by:
/// - GET /ports
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FilterReplySuccessPortAndVersionVec {
    /// List of `PortInfo`-version pairs
    pub content: Vec<FilterReplySuccessPortAndVersion>
}
impl crate::api::ok::sealed::Sealed for FilterReplySuccessPortAndVersionVec {}
impl crate::api::ok::OkData for FilterReplySuccessPortAndVersionVec {}


/// Single port information with fresh `PortInfo`-version hash.
///
/// Returned by:
/// - GET /ports/{port}/config
/// - PUT /ports/{port}/config
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FilterReplySuccessPortAndVersion {
    /// Single port info
    pub info: PortInfo,
    /// Matching port version hash
    pub ver_info: String
}
impl crate::api::ok::sealed::Sealed for FilterReplySuccessPortAndVersion {}
impl crate::api::ok::OkData for FilterReplySuccessPortAndVersion {}
