//! Rule engine: turns a decoded message into a decision.
//!
//! The dispatcher walks the active ruleset for a port, evaluates each filter's
//! matchers against the message, applies marker gating and blacklist/whitelist
//! logic, and emits an evaluation record.

pub mod dispatcher;
