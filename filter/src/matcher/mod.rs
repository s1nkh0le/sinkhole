//! Matchers: the leaf predicates a filter evaluates against a message.
//!
//! Each matcher resolves an input slice from the configured [`Scope`] (stream,
//! path, body, method, status, header) and returns a [`MatchStatus`]; a missing
//! field yields `Skipped` rather than a false match.

use crate::engine::dispatcher::MessageCtx;
use shared::types::base_types::{MatchStatus, Scope};

pub struct MatchInput<'a> {
    pub scope: Scope,
    pub http: Option<&'a crate::http::HttpMessage>,
    pub raw: &'a [u8],
    pub ctx: &'a MessageCtx<'a>,
}

#[derive(Clone, Debug)]
pub struct MatchOutputCore {
    pub status: MatchStatus,
    pub input: String,
    pub matched: Option<String>,
    pub numeric: Option<f64>,
    pub comment: Option<String>,
    pub flags: Vec<String>,
    pub error: Option<String>,
}

impl MatchOutputCore {
    pub fn skipped(msg: &str) -> Self {
        Self {
            status: MatchStatus::Skipped,
            input: String::new(),
            matched: None,
            numeric: None,
            comment: Some(msg.to_string()),
            flags: vec![],
            error: None,
        }
    }
}

pub trait Matcher: Send + Sync {
    fn evaluate(&self, inp: &MatchInput) -> MatchOutputCore;
}

pub mod always;
pub mod bytes_core;
pub mod bytes_matcher;
pub mod regex_core;
pub mod regex_matcher;
pub mod session_idle;
pub mod session_lifetime;
pub mod size;
pub mod types;

#[cfg(test)]
pub mod test_support;
