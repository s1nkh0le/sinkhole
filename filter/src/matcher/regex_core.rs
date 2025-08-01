use super::{MatchOutputCore, MatchStatus};
use regex::Regex;

pub struct RegexCore {
    regex: Regex,
    pub original: String,
    case_insensitive: bool,
}

impl RegexCore {
    pub fn new(pattern: &str, case_insensitive: bool) -> anyhow::Result<Self> {
        let regex = regex::RegexBuilder::new(pattern)
            .case_insensitive(!case_insensitive) // default false ⇒ insensitive
            .build()?;
        Ok(Self {
            regex,
            original: pattern.to_owned(),
            case_insensitive,
        })
    }

    pub fn eval(&self, text: &str) -> MatchOutputCore {
        if let Some(mat) = self.regex.find(text) {
            MatchOutputCore {
                status: MatchStatus::Satisfied,
                input: truncate(text, 128),
                matched: Some(mat.as_str().to_string()),
                numeric: None,
                comment: Some(format!("matched @{}", mat.start())),
                flags: if self.case_insensitive {
                    vec!["case_sensitive"]
                } else {
                    vec!["case_insensitive"]
                }
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
                error: None,
            }
        } else {
            MatchOutputCore {
                status: MatchStatus::NotSatisfied,
                input: truncate(text, 128),
                matched: None,
                numeric: None,
                comment: None,
                flags: vec![],
                error: None,
            }
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}…", &s[..max])
    } else {
        s.to_owned()
    }
}
