use crate::matcher::types::CmpOp;
use crate::matcher::{MatchInput, MatchOutputCore, Matcher};
use shared::types::MatchStatus;
use shared::utils::time::now_ns;

pub struct SessionIdleMatcher {
    pub op: CmpOp,
    pub value: f64, // seconds
}

impl Matcher for SessionIdleMatcher {
    fn evaluate(&self, inp: &MatchInput) -> MatchOutputCore {
        let now = now_ns();
        let session_last_message = inp.ctx.conn_prev_msg_ns;

        let delta = (now - session_last_message) as f64 / 1e9;
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
                    "now: {}, session_start: {}, delta = {}, op: {}, threshold: {}",
                    now, session_last_message, delta, self.op, self.value
                )
                .into(),
            ),
            flags: vec![],
            error: None,
        }
    }
}
