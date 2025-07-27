use crate::matcher::{MatchInput, MatchOutputCore, Matcher};
use crate::types::MatchStatus;
use regex::Regex;

pub struct MethodRegexMatcher {
    re: Regex,
    case_sensitive: bool,
}

impl MethodRegexMatcher {
    pub fn new(pattern: &str, case_sensitive: bool) -> anyhow::Result<Self> {
        let re = if case_sensitive {
            Regex::new(pattern)?
        } else {
            Regex::new(&format!("(?i){}", pattern))?
        };
        Ok(Self { re, case_sensitive })
    }
}

impl Matcher for MethodRegexMatcher {
    fn evaluate(&self, inp: &MatchInput) -> MatchOutputCore {
        let method = inp.http.and_then(|h| h.method.as_deref()).unwrap_or("");
        let m = self.re.find(method);

        MatchOutputCore {
            status: if m.is_some() { MatchStatus::Satisfied } else { MatchStatus::NotSatisfied },
            input: method.to_string(),
            matched: m.map(|m| m.as_str().to_string()),
            numeric: None,
            comment: None,
            flags: if self.case_sensitive { vec![] } else { vec!["case_insensitive".into()] },
            error: None,
        }
    }
}
