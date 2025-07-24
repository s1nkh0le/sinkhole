use super::{regex_core::RegexCore, MatchInput, MatchOutputCore, Matcher};
use crate::types::Scope;

pub struct PathRegex {
    core: RegexCore,
}

impl PathRegex {
    pub fn new(pattern: &str, case: bool) -> anyhow::Result<Self> {
        Ok(Self {
            core: RegexCore::new(pattern, case)?,
        })
    }
}

impl Matcher for PathRegex {
    fn evaluate(&self, inp: &MatchInput) -> MatchOutputCore {
        if inp.scope != Scope::HttpPath {
            return MatchOutputCore::skipped("scope mismatch");
        }
        match inp.http.and_then(|h| h.path.as_ref()) {
            Some(path) => self.core.eval(path),
            None => MatchOutputCore::skipped("no http path"),
        }
    }
}
