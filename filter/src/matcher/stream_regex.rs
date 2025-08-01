use super::{regex_core::RegexCore, MatchInput, MatchOutputCore, Matcher};
use shared::types::Scope;
use std::str;

pub struct StreamRegex {
    core: RegexCore,
}

impl StreamRegex {
    pub fn new(pattern: &str, case_sensitive: bool) -> anyhow::Result<Self> {
        Ok(Self {
            core: RegexCore::new(pattern, case_sensitive)?,
        })
    }
}

impl Matcher for StreamRegex {
    fn evaluate(&self, inp: &MatchInput) -> MatchOutputCore {
        if inp.scope != Scope::Stream {
            return MatchOutputCore::skipped("scope mismatch");
        }
        let txt = str::from_utf8(inp.raw).unwrap_or_default();
        self.core.eval(txt)
    }
}