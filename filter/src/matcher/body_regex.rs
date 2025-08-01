use super::{regex_core::RegexCore, MatchInput, MatchOutputCore, Matcher};
use shared::types::Scope;

pub struct BodyRegex {
    core: RegexCore,
}

impl BodyRegex {
    pub fn new(pattern: &str, case: bool) -> anyhow::Result<Self> {
        Ok(Self {
            core: RegexCore::new(pattern, case)?,
        })
    }
}

impl Matcher for BodyRegex {
    fn evaluate(&self, inp: &MatchInput) -> MatchOutputCore {
        if inp.scope != Scope::HttpBody {
            return MatchOutputCore::skipped("scope mismatch");
        }

        match inp.http {
            Some(http) => {
                let txt = str::from_utf8(&http.body).unwrap_or_default();
                self.core.eval(txt)
            }
            None => MatchOutputCore::skipped("no http body"),
        }
    }
}
