//! Periodic poll loops, one per filter resource.
//!
//! Each `sync_*` task fetches from filters on an interval and mirrors the data
//! into the central DB / in-memory state; record and ecam syncs also advance
//! per-service cursors and trigger filter-side pruning once data is centralized.

pub mod pruner;
pub mod sync_ecam;
pub mod sync_health;
pub mod sync_records;
pub mod sync_state;
