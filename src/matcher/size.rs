use super::{MatchInput, MatchOutputCore, Matcher};
use crate::types::{MatchStatus, Scope};

#[derive(Clone, Copy)]
pub enum CmpOp { Lt, Le, Eq, Ne, Ge, Gt }

pub struct SizeMatcher {
    op: CmpOp,
    value: usize,
}

impl SizeMatcher {
    pub fn new(op: CmpOp, value: usize) -> Self { Self { op, value } }

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
            Scope::HttpHeader => 0, // header size is handled in header_size matcher (future)
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
