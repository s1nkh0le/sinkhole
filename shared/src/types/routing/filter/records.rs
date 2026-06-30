use crate::types::record_types::EvaluationRecord;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Format for querying `EvaluationRecord`s from the filter.
#[derive(Deserialize, Debug)]
pub struct FilterRequestGetQueryRecords {
    pub since: Option<u64>,
    pub until: Option<u64>,
    pub limit: Option<usize>,
    /// "asc" | "desc"
    pub order: Option<String>,
    /// "allow" | "drop"
    pub decision: Option<String>,
    /// "in" | "out"
    pub dir: Option<String>,
    /// "blacklist" | "whitelist_all_miss" | "whitelist_any_miss"
    pub reason: Option<String>,
}

/// Selected `EvaluationRecord`s by query, and next cursor.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, IntoParams)]
pub struct FilterReplyRecordsAndCursor {
    pub records: Vec<EvaluationRecord>,
    /// Cursor, representing the max timestamp of the resulting `EvaluationRecord`s + 1ns.
    /// Can be used by the next query to fetch all records produced from this instant onward.
    pub next_since: u64,
}
impl crate::api::ok::sealed::Sealed for FilterReplyRecordsAndCursor {}
impl crate::api::ok::OkData for FilterReplyRecordsAndCursor {}
