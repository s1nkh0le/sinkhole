use super::{MatchInput, MatchOutputCore, Matcher};
use crate::types::{MatchStatus, Scope};

#[derive(Clone, Copy)]
pub enum CmpOp { Lt, Le, Eq, Ne, Ge, Gt }

pub struct SizeMatcher {
    pub(crate) op: CmpOp,
    pub(crate) value: usize,
    pub field_name: Option<String>,
}

impl SizeMatcher {
    pub fn new(op: CmpOp, value: usize, field_name: Option<String>) -> Self { Self { op, value, field_name } }

    fn compare(&self, actual: usize) -> bool {
        match self.op {
            CmpOp::Lt => actual <  self.value,
            CmpOp::Le => actual <= self.value,
            CmpOp::Eq => actual == self.value,
            CmpOp::Ne => actual != self.value,
            CmpOp::Ge => actual >= self.value,
            CmpOp::Gt => actual >  self.value,
        }
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
                        .and_then(|h| h.header_value(name)) // implement header_value(name) -> Option<&str>
                        .unwrap_or("");
                    val.len()
                } else {
                    0
                }
            }
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
