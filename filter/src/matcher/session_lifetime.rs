use crate::matcher::types::CmpOp;
use crate::matcher::{MatchInput, MatchOutputCore, Matcher};
use shared::types::MatchStatus;
use shared::utils::time::{now_ns, ns_to_hms};

pub struct SessionLifetimeMatcher {
    pub op: CmpOp,
    pub value: f64, // seconds
}

impl Matcher for SessionLifetimeMatcher {
    fn evaluate(&self, inp: &MatchInput) -> MatchOutputCore {
        let now = now_ns();
        let session_start = inp.ctx.conn_start_ns;

        let delta = (now - session_start) as f64 / 1e9;
        let ok = self.op.eval_f64(delta, self.value);

        MatchOutputCore {
            status: if ok {
                MatchStatus::Satisfied
            } else {
                MatchStatus::NotSatisfied
            },
            input: format!("{:.6}", delta),
            matched: None,
            numeric: Some(delta),
            comment: Some(
                format!(
                    "now_ns(): {}, conn_start_ns: {}; delta = {}, op: {}, threshold: {}",
                    ns_to_hms(now),
                    ns_to_hms(session_start),
                    delta,
                    self.op,
                    self.value
                )
                .into(),
            ),
            flags: vec![],
            error: None,
        }
    }
}
