use super::{MatchInput, MatchOutputCore, Matcher, regex_core::RegexCore};
use shared::types::base_types::Scope;

pub struct HeaderRegex {
    header_name: String,
    core: RegexCore,
}

impl HeaderRegex {
    pub fn new(header_name: &str, pattern: &str, case_sensitive: bool) -> anyhow::Result<Self> {
        Ok(Self {
            header_name: header_name.to_ascii_lowercase(),
            core: RegexCore::new(pattern, case_sensitive)?,
        })
    }
}

impl Matcher for HeaderRegex {
    fn evaluate(&self, inp: &MatchInput) -> MatchOutputCore {
        if inp.scope != Scope::HttpHeader {
            return MatchOutputCore::skipped("scope mismatch");
        }
        let header_val_opt = inp.http.and_then(|h| {
            h.headers
                .iter()
                .find(|(k, _)| k == &self.header_name)
                .map(|(_, v)| v)
        });

        match header_val_opt {
            Some(val) => self.core.eval(val),
            None => MatchOutputCore::skipped("header not present"),
        }
    }
}
