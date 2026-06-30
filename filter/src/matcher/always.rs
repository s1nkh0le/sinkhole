use super::{MatchInput, MatchOutputCore, Matcher};
use shared::types::base_types::MatchStatus;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matcher::test_support::Scenario;
    use shared::types::base_types::Scope;

    #[test]
    fn always_true_satisfies() {
        let s = Scenario::with_raw(b"");
        assert_eq!(
            AlwaysMatcher::new(true)
                .evaluate(&s.input(Scope::Stream))
                .status,
            MatchStatus::Satisfied
        );
    }

    #[test]
    fn always_false_does_not_satisfy() {
        let s = Scenario::with_raw(b"");
        assert_eq!(
            AlwaysMatcher::new(false)
                .evaluate(&s.input(Scope::Stream))
                .status,
            MatchStatus::NotSatisfied
        );
    }
}
