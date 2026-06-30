//! Observability: structured sinks for evaluation records and ECAM events.
//!
//! `sinks` fan out records/events to stdout and the in-memory emit buffer the
//! controller polls; `summary` provides rollups. Sink toggles are runtime-
//! configurable (see `sinks::TARGETS`).

pub mod sinks;
pub mod summary;
