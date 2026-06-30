use super::{MatchOutputCore, MatchStatus};
use regex::bytes::Regex;

/// Byte-oriented regex engine shared by all binary-safe matchers.
///
/// Compiled with Unicode mode disabled, so `.` matches any single byte and
/// `\xNN` is a literal byte. This lets patterns match arbitrary binary payloads
/// (for example shellcode signatures) that the string engine cannot represent.
pub struct BytesRegexCore {
    regex: Regex,
}

impl BytesRegexCore {
    /// Compile a byte-regex pattern (non-Unicode semantics).
    pub fn new(pattern: &str) -> anyhow::Result<Self> {
        let regex = regex::bytes::RegexBuilder::new(pattern)
            .unicode(false)
            .build()?;
        Ok(Self { regex })
    }

    /// Compile a literal hex byte signature, e.g. `"de ad be ef"`, into an
    /// anchored-free byte pattern that matches that exact sequence anywhere.
    pub fn from_hex(hex: &str) -> anyhow::Result<Self> {
        let cleaned: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
        if cleaned.is_empty() || !cleaned.len().is_multiple_of(2) {
            anyhow::bail!("hex signature must be non-empty with an even number of hex digits");
        }
        let mut pattern = String::with_capacity(cleaned.len() * 2);
        for i in (0..cleaned.len()).step_by(2) {
            let byte = u8::from_str_radix(&cleaned[i..i + 2], 16)
                .map_err(|_| anyhow::anyhow!("invalid hex digit in signature"))?;
            pattern.push_str(&format!("\\x{:02x}", byte));
        }
        Self::new(&pattern)
    }

    /// Run the byte-regex against `data`.
    pub fn eval(&self, data: &[u8]) -> MatchOutputCore {
        if let Some(mat) = self.regex.find(data) {
            MatchOutputCore {
                status: MatchStatus::Satisfied,
                input: hex_preview(data, 64),
                matched: Some(hex_preview(mat.as_bytes(), 64)),
                numeric: None,
                comment: Some(format!("matched @{}", mat.start())),
                flags: vec!["bytes".to_owned()],
                error: None,
            }
        } else {
            MatchOutputCore {
                status: MatchStatus::NotSatisfied,
                input: hex_preview(data, 64),
                matched: None,
                numeric: None,
                comment: None,
                flags: vec!["bytes".to_owned()],
                error: None,
            }
        }
    }
}

/// Render up to `max` bytes as space-separated hex, with an ellipsis if longer.
fn hex_preview(data: &[u8], max: usize) -> String {
    let shown = data.len().min(max);
    let mut out = String::with_capacity(shown * 3 + 1);
    for (i, b) in data[..shown].iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&format!("{:02x}", b));
    }
    if data.len() > max {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_literal_bytes_including_non_utf8() {
        let core = BytesRegexCore::new("\\xde\\xad\\xbe\\xef").unwrap();
        let data = [0x00, 0xde, 0xad, 0xbe, 0xef, 0xff];
        assert_eq!(core.eval(&data).status, MatchStatus::Satisfied);
        assert_eq!(core.eval(b"deadbeef").status, MatchStatus::NotSatisfied);
    }

    #[test]
    fn matches_null_byte_impossible_in_string_regex() {
        let core = BytesRegexCore::new("\\x00").unwrap();
        assert_eq!(
            core.eval(&[0x41, 0x00, 0x42]).status,
            MatchStatus::Satisfied
        );
    }

    #[test]
    fn nop_sled_quantifier() {
        let core = BytesRegexCore::new("\\x90{4,}").unwrap();
        assert_eq!(
            core.eval(&[0x90, 0x90, 0x90, 0x90, 0x90]).status,
            MatchStatus::Satisfied
        );
        assert_eq!(core.eval(&[0x90, 0x90]).status, MatchStatus::NotSatisfied);
    }

    #[test]
    fn from_hex_builds_literal_signature() {
        let core = BytesRegexCore::from_hex("de ad be ef").unwrap();
        assert_eq!(
            core.eval(&[0x01, 0xde, 0xad, 0xbe, 0xef]).status,
            MatchStatus::Satisfied
        );
        assert_eq!(core.eval(&[0xde, 0xad]).status, MatchStatus::NotSatisfied);
    }

    #[test]
    fn from_hex_rejects_bad_input() {
        assert!(BytesRegexCore::from_hex("abc").is_err());
        assert!(BytesRegexCore::from_hex("").is_err());
        assert!(BytesRegexCore::from_hex("zz").is_err());
    }
}
