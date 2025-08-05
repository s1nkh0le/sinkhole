use crate::matcher::{MatchInput, MatchOutputCore, Matcher};
use regex::Regex;
use shared::types::base_types::MatchStatus;

pub struct StatusRegexMatcher {
    re: Regex,
    case_sensitive: bool,
}

impl StatusRegexMatcher {
    pub fn new(pat: &str, case_sensitive: bool) -> anyhow::Result<Self> {
        let re = if case_sensitive {
            Regex::new(pat)?
        } else {
            Regex::new(&format!("(?i){}", pat))?
        };
        Ok(Self { re, case_sensitive })
    }
}

impl Matcher for StatusRegexMatcher {
    fn evaluate(&self, inp: &MatchInput) -> MatchOutputCore {
        let status_str = inp
            .http
            .and_then(|h| h.status)
            .map(|s| s.to_string())
            .unwrap_or_default();

        let m = self.re.find(&status_str);

        MatchOutputCore {
            status: if m.is_some() {
                MatchStatus::Satisfied
            } else {
                MatchStatus::NotSatisfied
            },
            input: status_str.clone(),
            matched: m.map(|m| m.as_str().to_string()),
            numeric: None,
            comment: None,
            flags: if self.case_sensitive {
                vec![]
            } else {
                vec!["case_insensitive".into()]
            },
            error: None,
        }
    }
}
