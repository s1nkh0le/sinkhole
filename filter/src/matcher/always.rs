use super::{MatchInput, MatchOutputCore, Matcher};
use shared::types::MatchStatus;

pub struct AlwaysMatcher {
    value: bool,
}

impl AlwaysMatcher {
    pub fn new(value: bool) -> Self {
        Self { value }
    }
}

impl Matcher for AlwaysMatcher {
    fn evaluate(&self, _inp: &MatchInput) -> MatchOutputCore {
        MatchOutputCore {
            status: if self.value {
                MatchStatus::Satisfied
            } else {
                MatchStatus::NotSatisfied
            },
            input: String::new(),
            matched: None,
            numeric: None,
            comment: None,
            flags: vec![],
            error: None,
        }
    }
}
