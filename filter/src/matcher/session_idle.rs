use crate::matcher::types::CmpOp;
use crate::matcher::{MatchInput, MatchOutputCore, Matcher};
use shared::types::base_types::MatchStatus;

pub struct SessionIdleMatcher {
    pub op: CmpOp,
    pub value: f64, // seconds
}

impl Matcher for SessionIdleMatcher {
    fn evaluate(&self, inp: &MatchInput) -> MatchOutputCore {
        let now = inp.ctx.msg_now_ns;
        let session_last_message = inp.ctx.conn_prev_msg_ns;

        let delta = now.saturating_sub(session_last_message) as f64 / 1e9;
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
            comment: Some(format!(
                "now: {}, session_start: {}, delta = {}, op: {}, threshold: {}",
                now, session_last_message, delta, self.op, self.value
            )),
            flags: vec![],
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dispatcher::MessageCtx;
    use shared::types::base_types::Scope;

    fn ctx_with(prev_msg_ns: u64, now_ns: u64) -> MessageCtx<'static> {
        MessageCtx {
            conn_id: 1,
            seq: 0,
            inbound: true,
            public_port: 80,
            upstream_port: 80,
            raw: &[],
            raw_capture_limit: usize::MAX,
            http: None,
            protocol: shared::types::base_types::Protocol::RawBinary,
            src_ip: "127.0.0.1",
            src_port: 1234,
            dst_ip: "127.0.0.1",
            dst_port: 80,
            conn_start_ns: 0,
            conn_prev_msg_ns: prev_msg_ns,
            msg_now_ns: now_ns,
        }
    }

    #[test]
    fn idle_threshold_uses_msg_now_ns() {
        // 3s since last message; threshold > 2s -> satisfied.
        let ctx = ctx_with(1_000_000_000, 4_000_000_000);
        let inp = MatchInput {
            scope: Scope::Timing,
            http: None,
            raw: &[],
            ctx: &ctx,
        };
        let m = SessionIdleMatcher {
            op: CmpOp::Gt,
            value: 2.0,
        };
        assert_eq!(m.evaluate(&inp).status, MatchStatus::Satisfied);

        // 0.5s since last message; threshold > 2s -> not satisfied.
        let ctx = ctx_with(1_000_000_000, 1_500_000_000);
        let inp = MatchInput {
            scope: Scope::Timing,
            http: None,
            raw: &[],
            ctx: &ctx,
        };
        assert_eq!(m.evaluate(&inp).status, MatchStatus::NotSatisfied);
    }
}
