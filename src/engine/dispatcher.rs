use crate::{
    observability::stats,
    rules::compile::{CompiledFilter, CompiledMarker, CompiledRuleSet},
    types::{Decision, Direction, MatchStatus, Scope},
    utils::now_ns,
};
use crate::matcher::{MatchInput, MatchOutputCore};
use super::decision::{final_decision, WhitelistState};
use crate::observability::eval_record::{EvaluationDetail, EvaluationRecord, MatchOutput};
use crate::registry::PortRuntime;
use std::sync::Arc;
use crate::types::Mode;
use base64::Engine;

pub struct MessageCtx<'a> {
    pub conn_id: u64,
    pub seq: u64,
    pub inbound: bool,
    pub public_port: u16,
    pub upstream_port: u16,
    pub raw: &'a [u8],
    pub http: Option<&'a crate::http::HttpMessage>,
    pub scope: Scope, // stream or http.xxx depending on rule
    pub src_ip: &'a str,
    pub src_port: u16,
}

pub fn evaluate(
    compiled: &Arc<CompiledRuleSet>,
    port_rt: &Arc<PortRuntime>,
    msg: &MessageCtx,
    debug: bool,
) -> (Decision, EvaluationRecord) {
    let mut details = Vec::<EvaluationDetail>::new();
    let mut verified = std::collections::HashSet::new();

    // ---- Phase 1 markers ----
    for m in &compiled.markers {
        if !m.enabled || !m.direction.matches(if msg.inbound {Direction::Inbound} else {Direction::Outbound}) {
            continue;
        }
        let counters = stats::get_or_init(&port_rt.marker_counters, &m.id);
        let mi = build_input(msg, m.scope);

        let out = m.matcher.evaluate(&mi);
        match out.status {
            MatchStatus::Satisfied => { verified.insert(&m.id); counters.inc_hits(); }
            MatchStatus::NotSatisfied => counters.inc_misses(),
            MatchStatus::Skipped => counters.inc_skipped(),
            MatchStatus::Error => counters.inc_errors(),
        }
        details.push(build_detail("marker", &m.id, out));
    }

    // ---- Phase 2 filters ----
    let mut wl = WhitelistState::default();
    let mut blacklist_hit = false;

    for f in &compiled.filters {
        if !f.enabled || !f.direction.matches(if msg.inbound {Direction::Inbound} else {Direction::Outbound}) {
            continue;
        }
        // gating
        let gated = f.needed_all.iter().all(|id| verified.contains(id))
            && (f.needed_any.is_empty()
            || f.needed_any.iter().any(|id| verified.contains(id)));

        let counters = stats::get_or_init(&port_rt.filter_counters, &f.id);

        if !gated {
            counters.inc_skipped();
            details.push(build_detail("filter", &f.id, MatchOutputCore::skipped("gated")));
            continue;
        }

        let mi = build_input(msg, f.scope);
        let out = f.matcher.evaluate(&mi);

        match out.status {
            MatchStatus::Satisfied => {
                counters.inc_hits();
                match f.mode {
                    Mode::Blacklist => blacklist_hit = true,
                    Mode::WhitelistAll => wl.all_hit += 1,
                    Mode::WhitelistAny => wl.any_hit += 1,
                }
            }
            MatchStatus::NotSatisfied => {
                counters.inc_misses();
                if let Mode::WhitelistAll = f.mode {
                    // miss in whitelist‑all counts later
                }
            }
            MatchStatus::Skipped => counters.inc_skipped(),
            MatchStatus::Error => counters.inc_errors(),
        }

        match f.mode {
            Mode::WhitelistAll => wl.all_total += 1,
            Mode::WhitelistAny => wl.any_total += 1,
            _ => {}
        }

        // early exit when blacklist
        if blacklist_hit && !debug {
            details.push(build_detail("filter", &f.id, out));
            break;
        }

        details.push(build_detail("filter", &f.id, out));
    }

    // ---- Final decision ----
    let mut drop_reasons = Vec::new();
    let decision = final_decision(blacklist_hit, &wl, &mut drop_reasons);

    // ---- Build record ----
    let record = EvaluationRecord {
        timestamp_ns: now_ns(),
        conn_id: msg.conn_id,
        message_seq: msg.seq,
        inbound: msg.inbound,
        public_port: msg.public_port,
        upstream_port: msg.upstream_port,
        decision,
        drop_reasons: drop_reasons
            .into_iter()
            .map(|k| crate::observability::eval_record::DropReason {
                kind: k,
                detail: None,
            })
            .collect(),
        http: msg.http.map(|h| crate::observability::eval_record::HttpInfo {
            method: h.method.clone(),
            path: h.path.clone(),
            status: h.status,
            reason: h.reason.clone(),
            headers: h.headers.clone(),
            body_len: h.body.len(),
            truncated: h.truncated,
        }),
        raw_b64: base64::engine::general_purpose::STANDARD.encode(msg.raw),
        details,
        stats_snapshot: None,
        src_ip: msg.src_ip.to_owned(),
        src_port: msg.src_port,
        dst_ip: "0.0.0.0".into(),
        dst_port: msg.public_port,
    };

    (decision, record)
}

// helpers -------------------------------------------------
fn build_input<'a>(msg: &'a MessageCtx, scope: Scope) -> MatchInput<'a> {
    MatchInput {
        scope,
        inbound: msg.inbound,
        http: msg.http,
        raw: msg.raw,
    }
}

fn build_detail(kind: &str, id: &str, core: MatchOutputCore) -> EvaluationDetail {
    EvaluationDetail {
        kind: kind.into(),
        id: id.into(),
        name: None,
        mode: None,
        gated: None,
        decision_effect: None,
        match_output: MatchOutput {
            scope: Scope::Stream, // placeholder
            target: String::new(),
            condition: String::new(),
            input: core.input,
            matched: core.matched,
            numeric: core.numeric,
            status: core.status,
            comment: core.comment,
            comparator: String::new(),
            flags: core.flags,
            error: core.error,
        },
    }
}
