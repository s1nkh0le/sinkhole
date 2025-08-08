use crate::types::snapshot_types::PortSnapshot;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Full serialized fetch of the controller
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FilterStateOkData {
    pub ports:     Vec<PortSnapshot>,
}
impl crate::api::ok::sealed::Sealed for FilterStateOkData {}
impl crate::api::ok::OkData for FilterStateOkData {}