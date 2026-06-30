//! Outbound HTTP client: the controller's calls to individual filters.
//!
//! One `fetch_*` module per filter resource (state, records, ecam, health,
//! ports, ruleset). These are the low-level requests the `sync` loops and the
//! `api` relay handlers build on.

pub mod error;
pub mod extraction;
pub mod fetch_ecam;
pub mod fetch_prune;
pub mod fetch_records;
pub mod fetch_state;
