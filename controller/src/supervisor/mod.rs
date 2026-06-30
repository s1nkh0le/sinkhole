//! Per-service task lifecycle.
//!
//! The supervisor owns the set of managed filters: it spawns and cancels the
//! `sync` tasks for each service and holds the shared app state they read from.

pub mod app_state;
pub mod supervisor;
