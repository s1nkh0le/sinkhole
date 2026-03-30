use super::{MatchOutputCore, MatchStatus};
use regex::Regex;

pub struct RegexCore {
    regex: Regex,
    pub original: String,
    case_sensitive: bool,
}

impl RegexCore {
    /// `case_sensitive` mirrors the model field: false (default) means case‑insensitive matching.
    pub fn new(pattern: &str, case_sensitive: bool) -> anyhow::Result<Self> {
        let regex = regex::RegexBuilder::new(pattern)
            .case_insensitive(!case_sensitive)
            .build()?;
        Ok(Self {
            regex,
            original: pattern.to_owned(),
            case_sensitive,
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
                flags: if self.case_sensitive {
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
    if s.len() <= max {
        return s.to_owned();
    }
    // Find a char boundary at or before `max` to avoid splitting a multi-byte character.
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}
