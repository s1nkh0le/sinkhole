//! Central SQLite store.
//!
//! `boot` opens the database; `insertion` and `query` are the write/
//! read paths for mirrored records and ecam events. Writes go through a single
//! actor so the sync loops never contend on the connection.

pub mod boot;
pub mod commands;
pub mod insertion;
pub mod query;
pub mod query_records_grouped;
pub mod util;
