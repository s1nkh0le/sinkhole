use crate::matcher::types::CmpOp;
use crate::matcher::{MatchInput, MatchOutputCore, Matcher};
use shared::types::base_types::MatchStatus;
use shared::utils::time::ns_to_hms;

pub struct SessionLifetimeMatcher {
    pub op: CmpOp,
    pub value: f64, // seconds
}

impl Matcher for SessionLifetimeMatcher {
    fn evaluate(&self, inp: &MatchInput) -> MatchOutputCore {
        let now = inp.ctx.msg_now_ns;
        let session_start = inp.ctx.conn_start_ns;

        let delta = now.saturating_sub(session_start) as f64 / 1e9;
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
                "now_ns(): {}, conn_start_ns: {}; delta = {}, op: {}, threshold: {}",
                ns_to_hms(now),
                ns_to_hms(session_start),
                delta,
                self.op,
                self.value
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

    fn ctx_with(start_ns: u64, now_ns: u64) -> MessageCtx<'static> {
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
            conn_start_ns: start_ns,
            conn_prev_msg_ns: start_ns,
            msg_now_ns: now_ns,
        }
    }

    #[test]
    fn lifetime_threshold_uses_msg_now_ns() {
        // 10s lifetime; threshold >= 5s -> satisfied.
        let ctx = ctx_with(0, 10_000_000_000);
        let inp = MatchInput {
            scope: Scope::Timing,
            http: None,
            raw: &[],
            ctx: &ctx,
        };
        let m = SessionLifetimeMatcher {
            op: CmpOp::Ge,
            value: 5.0,
        };
        assert_eq!(m.evaluate(&inp).status, MatchStatus::Satisfied);

        // 1s lifetime; threshold >= 5s -> not satisfied.
        let ctx = ctx_with(0, 1_000_000_000);
        let inp = MatchInput {
            scope: Scope::Timing,
            http: None,
            raw: &[],
            ctx: &ctx,
        };
        assert_eq!(m.evaluate(&inp).status, MatchStatus::NotSatisfied);
    }
}
