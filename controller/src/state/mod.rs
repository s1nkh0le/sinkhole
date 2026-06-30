//! In-memory cache of live filter state.
//!
//! `ctrl` holds the latest polled state per service; `health` tracks
//! reachability. The `api` layer serves reads from here so user requests don't
//! block on filter round-trips.

pub mod ctrl;
pub mod health;
