//! sinkhole shared: the common wire model.
//!
//! Types and helpers shared by the `filter` and `controller` crates so both
//! sides serialize the exact same JSON: the rule model (`types::ruleset_types`,
//! `types::base_types`), evaluation records and ECAM events, the per-endpoint
//! request/response envelopes under `types::routing`, and small utilities for
//! hashing, versioning, time and bearer-token auth (`utils`, `api`).

pub mod api;
pub mod types;
pub mod utils;
