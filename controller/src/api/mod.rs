//! Controller user/web-UI-facing HTTP API (axum).
//!
//! The operator control plane: read endpoints serve cached state and the
//! central DB, while write endpoints (services, ruleset) relay to the affected
//! filters. Handlers live in the `api_*` submodules.

pub mod api_ecam;
pub mod api_health;
mod api_ports;
mod api_records;
pub mod api_records_grouped;
mod api_ruleset;
pub mod api_service;
mod api_service_meta;
pub mod api_state;
mod fetchers;
pub mod middleware_auth;
pub mod routes_controller;
pub mod util;
