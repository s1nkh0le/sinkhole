//! Filter control-plane HTTP API (axum) the controller calls.
//!
//! Bearer-authenticated endpoints for ports, ruleset, records, ECAM, db and
//! health. Handlers live in the `api_*` submodules; `middleware_auth` enforces
//! the shared token.

mod api_db;
mod api_ecam;
mod api_health;
mod api_ports;
mod api_records;
mod api_ruleset;
mod api_state;
mod api_stats;
pub mod middleware_auth;
pub mod routes_filter;
