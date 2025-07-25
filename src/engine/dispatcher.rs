//! Two‑phase evaluation (markers → filters) for a single message.

use std::collections::HashSet;
use std::sync::Arc;

use base64::Engine;
use tracing::debug;
use crate::{
    matcher::{MatchInput, MatchOutputCore},
    observability::{
        eval_record::{
            DropReason, EvaluationDetail, EvaluationRecord, HttpInfo, MatchOutput, StatsSnapshot,
        },
    },
    rules::compile::{CompiledFilter, CompiledMarker, CompiledRuleSet, MatcherMeta},
    types::{Decision, Direction, MatchStatus, Mode, Scope},
    utils::now_ns,
};

use crate::registry::PortRuntime;

pub struct MessageCtx<'a> {
    pub conn_id: u64,
    pub seq: u64,
    pub inbound: bool,
    pub public_port: u16,
    pub upstream_port: u16,

    pub raw: &'a [u8],
    pub http: Option<&'a crate::http::HttpMessage>,

    pub src_ip: &'a str,
    pub src_port: u16,
}

/// Main entry: evaluate all rules, return (Decision, EvaluationRecord)
pub fn evaluate(
    compiled: &Arc<CompiledRuleSet>,
    port_rt: &Arc<PortRuntime>,
    msg: &MessageCtx,
    debug: bool,
) -> (Decision, EvaluationRecord) {
    // ----------------------------------------------------------------
    // Phase 1 – markers
    // ----------------------------------------------------------------
    let dir = if msg.inbound {
        Direction::Inbound
    } else {
        Direction::Outbound
    };

    let mut details = Vec::<EvaluationDetail>::new();
    let mut verified: HashSet<&str> = HashSet::new();

    for m in &compiled.markers {
        if !m.enabled || !m.direction.matches(dir) {
            continue;
        }

        let input = build_match_input(msg, m.scope);
        let out = m.matcher.evaluate(&input);

        match out.status {
            MatchStatus::Satisfied => {
                verified.insert(&m.id);
            }
            MatchStatus::NotSatisfied => {},
            MatchStatus::Skipped => {},
            MatchStatus::Error => {},
        }

        details.push(build_detail(
            "marker",
            m,
            &out,
            m.meta.clone(),
            None,
            None,
            None,
        ));

        if debug {
            // keep evaluating all; nothing to short‑circuit for markers
        }
    }

    // ----------------------------------------------------------------
    // Phase 2 – filters
    // ----------------------------------------------------------------
    let mut blacklist_hit = false;
    let mut wl_all_total = 0u64;
    let mut wl_all_hit = 0u64;
    let mut wl_any_total = 0u64;
    let mut wl_any_hit = 0u64;

    'filters: for f in &compiled.filters {
        if !f.enabled || !f.direction.matches(dir) {
            continue;
        }

        // gating
        let gated = f
            .needed_all
            .iter()
            .all(|id| verified.contains(id.as_str()))
            && (f.needed_any.is_empty()
            || f.needed_any.iter().any(|id| verified.contains(id.as_str())));


        if !gated {
            details.push(build_detail(
                "filter",
                f,
                &MatchOutputCore::skipped("gated"),
                f.meta.clone(),
                Some(f.mode),
                Some(false),
                None,
            ));
            continue;
        }

        // increment whitelist totals when gated
        match f.mode {
            Mode::WhitelistAll => wl_all_total += 1,
            Mode::WhitelistAny => wl_any_total += 1,
            _ => {}
        }

        let input = build_match_input(msg, f.scope);
        let out = f.matcher.evaluate(&input);

        match out.status {
            MatchStatus::Satisfied => {
                match f.mode {
                    Mode::Blacklist => blacklist_hit = true,
                    Mode::WhitelistAll => wl_all_hit += 1,
                    Mode::WhitelistAny => wl_any_hit += 1,
                }
            }
            MatchStatus::NotSatisfied => {}
            MatchStatus::Skipped => {},
            MatchStatus::Error => {},
        }

        // decision effect field
        let decision_eff = match f.mode {
            Mode::Blacklist if out.status == MatchStatus::Satisfied => Some("blacklist_drop"),
            Mode::WhitelistAll if out.status == MatchStatus::Satisfied => {
                Some("whitelist_all_satisfied")
            }
            Mode::WhitelistAny if out.status == MatchStatus::Satisfied => {
                Some("whitelist_any_satisfied")
            }
            _ => None,
        };

        details.push(build_detail(
            "filter",
            f,
            &out,
            f.meta.clone(),
            Some(f.mode),
            Some(true),
            decision_eff,
        ));

        // early exit if blacklist hit and not in debug
        if blacklist_hit && !debug {
            break 'filters;
        }
    }

    // ----------------------------------------------------------------
    // Final decision + drop reasons
    // ----------------------------------------------------------------
    let mut drop_reasons = Vec::<DropReason>::new();
    let decision = if blacklist_hit {
        drop_reasons.push(DropReason {
            kind: "blacklist".into(),
            detail: None,
        });
        Decision::Drop
    } else if wl_all_total > 0 && wl_all_hit != wl_all_total {
        drop_reasons.push(DropReason {
            kind: "whitelist_all_miss".into(),
            detail: Some(format!("hit {}/{}", wl_all_hit, wl_all_total)),
        });
        Decision::Drop
    } else if wl_any_total > 0 && wl_any_hit == 0 {
        drop_reasons.push(DropReason {
            kind: "whitelist_any_miss".into(),
            detail: Some("no whitelist_any hit".into()),
        });
        Decision::Drop
    } else {
        Decision::Allow
    };

    // ----------------------------------------------------------------
    // Build EvaluationRecord
    // ----------------------------------------------------------------
    let http_info = msg.http.map(|h| HttpInfo {
        method: h.method.clone(),
        path: h.path.clone(),
        status: h.status,
        reason: h.reason.clone(),
        headers: h.headers.clone(),
        body_len: h.body.len(),
        truncated: h.truncated,
    });


    let record = EvaluationRecord {
        timestamp_ns: now_ns(),
        conn_id: msg.conn_id,
        message_seq: msg.seq,
        inbound: msg.inbound,
        public_port: msg.public_port,
        upstream_port: msg.upstream_port,
        decision,
        drop_reasons,
        http: http_info,
        raw_b64: base64::engine::general_purpose::STANDARD.encode(msg.raw),
        details,
        src_ip: msg.src_ip.to_owned(),
        src_port: msg.src_port,
        dst_ip: "0.0.0.0".into(),
        dst_port: msg.public_port,
    };

    (decision, record)
}

// ----------------------------------------------------------------
fn build_match_input<'a>(msg: &'a MessageCtx, scope: Scope) -> MatchInput<'a> {
    MatchInput {
        scope,
        inbound: msg.inbound,
        http: msg.http,
        raw: msg.raw,
    }
}

fn build_detail(
    kind: &str,
    rule: &impl HasId,                 // trait object workaround
    core: &MatchOutputCore,
    meta: MatcherMeta,
    mode_opt: Option<Mode>,
    gated_opt: Option<bool>,
    decision_eff: Option<&str>,
) -> EvaluationDetail {
    EvaluationDetail {
        kind: kind.into(),
        id: rule.id().to_owned(),
        name: rule.name().clone(),
        mode: mode_opt.map(|m| format!("{:?}", m).to_lowercase()),
        gated: gated_opt,
        decision_effect: decision_eff.map(str::to_owned),
        match_output: MatchOutput {
            scope: meta_target_to_scope(&meta.target),
            target: meta.target,
            condition: meta.condition,
            input: core.input.clone(),
            matched: core.matched.clone(),
            numeric: core.numeric,
            status: core.status,
            comment: core.comment.clone(),
            comparator: meta.comparator,
            flags: core.flags.clone(),
            error: core.error.clone(),
        },
    }
}

trait HasId {
    fn id(&self) -> &String;
    fn name(&self) -> &Option<String>;
}
impl HasId for CompiledMarker {
    fn id(&self) -> &String { &self.id }
    fn name(&self) -> &Option<String> { &self.name }
}
impl HasId for CompiledFilter {
    fn id(&self) -> &String { &self.id }
    fn name(&self) -> &Option<String> { &self.name }
}

fn meta_target_to_scope(t: &str) -> Scope {
    match t {
        "meta" => Scope::Meta,
        "stream" => Scope::Stream,
        "body" => Scope::HttpBody,
        "path" => Scope::HttpPath,
        s if s.starts_with("header") => Scope::HttpHeader,
        _ => Scope::Stream,
    }
}
