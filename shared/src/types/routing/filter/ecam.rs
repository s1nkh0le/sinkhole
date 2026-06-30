use crate::types::ecam_types::EcamEvent;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Format for querying the ECAM records
#[derive(Debug, Serialize, Deserialize, IntoParams)]
pub struct FilterRequestGetQueryEcamQuery {
    pub since: Option<u64>,
}

/// Selected ECAM events by query, and next cursor
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, IntoParams)]
pub struct FilterReplyEcamAndCursor {
    pub events: Vec<EcamEvent>,
    /// Cursor, representing the max timestamp of the resulting `EcamEvent`s + 1ns.
    /// Can be used by the next query to fetch all events produced from this instant onward.
    pub next_since: u64,
}
impl crate::api::ok::sealed::Sealed for FilterReplyEcamAndCursor {}
impl crate::api::ok::OkData for FilterReplyEcamAndCursor {}
