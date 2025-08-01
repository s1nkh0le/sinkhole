use crate::matcher::types::CmpOp;
use super::{MatchInput, MatchOutputCore, Matcher};
use shared::types::{MatchStatus, Scope};

pub struct SizeMatcher {
    pub(crate) op: CmpOp,
    pub(crate) value: usize,
    pub field_name: Option<String>,
}

impl SizeMatcher {
    pub fn new(op: CmpOp, value: usize, field_name: Option<String>) -> Self { Self { op, value, field_name } }

    #[inline]
    fn compare(&self, actual: usize) -> bool {
        self.op.eval_u64(actual as u64, self.value as u64)
    }
}

impl Matcher for SizeMatcher {
    fn evaluate(&self, inp: &MatchInput) -> MatchOutputCore {
        let len = match inp.scope {
            Scope::Meta | Scope::Stream => inp.raw.len(),
            Scope::HttpBody => inp.http.map(|h| h.body.len()).unwrap_or(0),
            Scope::HttpPath => inp.http.and_then(|h| h.path.as_ref()).map(|p| p.len()).unwrap_or(0),
            Scope::HttpMethod => inp.http.and_then(|h| h.method.as_ref()).map(|p| p.len()).unwrap_or(0),
            Scope::HttpHeader => {
                // if header scope, field_name must be provided
                if let Some(ref name) = self.field_name {
                    let val = inp
                        .http
                        .and_then(|h| h.header_value(name))
                        .unwrap_or("");
                    val.len()
                } else {
                    0
                }
            }
            Scope::HttpStatus => panic!(),
            Scope::Timing => panic!(),
        };

        let ok = self.compare(len);

        MatchOutputCore {
            status: if ok { MatchStatus::Satisfied } else { MatchStatus::NotSatisfied },
            input: len.to_string(),
            matched: None,
            numeric: Some(len as f64),
            comment: None,
            flags: vec![],
            error: None,
        }
    }
}
