use super::{MatchInput, MatchOutputCore, Matcher};
use crate::matcher::types::CmpOp;
use shared::types::base_types::{MatchStatus, Scope};

pub struct SizeMatcher {
    pub(crate) op: CmpOp,
    pub(crate) value: usize,
    pub field_name: Option<String>,
}

impl SizeMatcher {
    pub fn new(op: CmpOp, value: usize, field_name: Option<String>) -> Self {
        Self {
            op,
            value,
            field_name,
        }
    }

    #[inline]
    fn compare(&self, actual: usize) -> bool {
        self.op.eval_u64(actual as u64, self.value as u64)
    }
}

impl Matcher for SizeMatcher {
    fn evaluate(&self, inp: &MatchInput) -> MatchOutputCore {
        let len = match inp.scope {
            Scope::Stream => inp.raw.len(),
            Scope::HttpBody => inp.http.map(|h| h.body.len()).unwrap_or(0),
            Scope::HttpPath => inp
                .http
                .and_then(|h| h.path.as_ref())
                .map(|p| p.len())
                .unwrap_or(0),
            Scope::HttpMethod => inp
                .http
                .and_then(|h| h.method.as_ref())
                .map(|p| p.len())
                .unwrap_or(0),
            Scope::HttpHeader => {
                // if header scope, field_name must be provided
                if let Some(ref name) = self.field_name {
                    let val = inp.http.and_then(|h| h.header_value(name)).unwrap_or("");
                    val.len()
                } else {
                    0
                }
            }
            Scope::HttpStatus | Scope::Timing => 0,
        };

        let ok = self.compare(len);

        MatchOutputCore {
            status: if ok {
                MatchStatus::Satisfied
            } else {
                MatchStatus::NotSatisfied
            },
            input: len.to_string(),
            matched: None,
            numeric: Some(len as f64),
            comment: None,
            flags: vec![],
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matcher::test_support::{Scenario, http};
    use shared::types::base_types::CmpOp;

    fn size(op: CmpOp, value: usize, field: Option<&str>) -> SizeMatcher {
        SizeMatcher::new(op, value, field.map(str::to_owned))
    }

    #[test]
    fn body_length_compare() {
        let s = Scenario::with_http(http(Some("POST"), Some("/"), None, &[], b"0123456789")); // 10 bytes
        assert_eq!(
            size(CmpOp::Gt, 5, None)
                .evaluate(&s.input(Scope::HttpBody))
                .status,
            MatchStatus::Satisfied
        );
        assert_eq!(
            size(CmpOp::Lt, 5, None)
                .evaluate(&s.input(Scope::HttpBody))
                .status,
            MatchStatus::NotSatisfied
        );
        assert_eq!(
            size(CmpOp::Eq, 10, None)
                .evaluate(&s.input(Scope::HttpBody))
                .status,
            MatchStatus::Satisfied
        );
    }

    #[test]
    fn stream_length_compare() {
        let s = Scenario::with_raw(b"abcd");
        assert_eq!(
            size(CmpOp::Ge, 4, None)
                .evaluate(&s.input(Scope::Stream))
                .status,
            MatchStatus::Satisfied
        );
    }

    #[test]
    fn path_length_compare() {
        let s = Scenario::with_http(http(Some("GET"), Some("/abc"), None, &[], b"")); // 4
        assert_eq!(
            size(CmpOp::Eq, 4, None)
                .evaluate(&s.input(Scope::HttpPath))
                .status,
            MatchStatus::Satisfied
        );
    }

    #[test]
    fn header_length_uses_field_name() {
        let s = Scenario::with_http(http(
            Some("GET"),
            Some("/"),
            None,
            &[("X-Token", "abcdef")],
            b"",
        ));
        assert_eq!(
            size(CmpOp::Eq, 6, Some("x-token"))
                .evaluate(&s.input(Scope::HttpHeader))
                .status,
            MatchStatus::Satisfied
        );
    }

    #[test]
    fn missing_data_is_zero_length() {
        // body scope but no http message present -> len 0
        let s = Scenario::with_raw(b"ignored");
        assert_eq!(
            size(CmpOp::Eq, 0, None)
                .evaluate(&s.input(Scope::HttpBody))
                .status,
            MatchStatus::Satisfied
        );

        // header scope without field_name -> len 0
        let s = Scenario::with_http(http(
            Some("GET"),
            Some("/"),
            None,
            &[("X-Token", "abc")],
            b"",
        ));
        assert_eq!(
            size(CmpOp::Eq, 0, None)
                .evaluate(&s.input(Scope::HttpHeader))
                .status,
            MatchStatus::Satisfied
        );
    }
}
