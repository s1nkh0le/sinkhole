use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

/// Pruning request parameters
#[derive(Deserialize, Debug, ToSchema, IntoParams)]
pub struct FilterRequestDeleteQueryPruneRecords {
    /// Delete all database records before this timestamp, keep all records with tiemstamp greater than this
    pub prune_threshold_ts: u64,
}
