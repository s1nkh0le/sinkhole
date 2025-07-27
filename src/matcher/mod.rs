use crate::types::{MatchStatus, Scope};

pub struct MatchInput<'a> {
    pub scope: Scope,
    pub inbound: bool,
    pub http: Option<&'a crate::http::HttpMessage>,
    pub raw: &'a [u8],
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

pub mod regex_core;
pub mod stream_regex;
pub mod path_regex;
pub mod body_regex;
pub mod header_regex;
pub mod size;
pub mod always;
