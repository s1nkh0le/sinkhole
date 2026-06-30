use super::{MatchOutputCore, MatchStatus};
use regex::Regex;

pub struct RegexCore {
    regex: Regex,
    case_sensitive: bool,
}

impl RegexCore {
    pub fn new(pattern: &str, case_sensitive: bool) -> anyhow::Result<Self> {
        let regex = regex::RegexBuilder::new(pattern)
            .case_insensitive(!case_sensitive) // default false ⇒ case-insensitive
            .build()?;
        Ok(Self {
            regex,
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
    if s.len() > max {
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &s[..end])
    } else {
        s.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::types::base_types::MatchStatus;

    #[test]
    fn case_insensitive_by_default() {
        let core = RegexCore::new("hello", false).unwrap();
        assert_eq!(core.eval("HELLO world").status, MatchStatus::Satisfied);
        assert!(
            core.eval("HELLO")
                .flags
                .contains(&"case_insensitive".to_string())
        );
    }

    #[test]
    fn case_sensitive_when_requested() {
        let core = RegexCore::new("hello", true).unwrap();
        assert_eq!(core.eval("HELLO world").status, MatchStatus::NotSatisfied);
        assert_eq!(core.eval("hello world").status, MatchStatus::Satisfied);
        assert!(
            core.eval("hello")
                .flags
                .contains(&"case_sensitive".to_string())
        );
    }

    #[test]
    fn invalid_pattern_errors() {
        assert!(RegexCore::new("(unclosed", false).is_err());
    }

    #[test]
    fn anchors_are_respected() {
        let core = RegexCore::new("^/admin$", true).unwrap();
        assert_eq!(core.eval("/admin").status, MatchStatus::Satisfied);
        assert_eq!(core.eval("/admin/panel").status, MatchStatus::NotSatisfied);
    }

    #[test]
    fn truncation_is_multibyte_safe() {
        // 200 'é' chars (2 bytes each) exceeds the 128-byte cap; must not panic
        // on a char boundary and must produce the ellipsis.
        let text = "é".repeat(200);
        let core = RegexCore::new("nomatch", false).unwrap();
        let out = core.eval(&text);
        assert!(out.input.ends_with('…'));
    }
}
