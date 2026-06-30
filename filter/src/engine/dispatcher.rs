//! Two‑phase evaluation (markers → filters) for a single message.

use std::collections::HashSet;
use std::sync::Arc;

use crate::{
    matcher::{MatchInput, MatchOutputCore},
    rules::compile::{CompiledFilter, CompiledMarker, CompiledRuleSet, MatcherMeta},
};
use base64::Engine;
use shared::types::base_types::{Decision, Direction, MatchStatus, Mode, Scope};
use shared::types::record_types::{
    DropReason, EvaluationDetail, EvaluationRecord, HttpInfo, MatchOutput,
};
use shared::utils::time::now_ns;

pub struct MessageCtx<'a> {
    /// Monotonic per-connection id from a process-local counter. Unique within a
    /// filter process; the controller DB disambiguates across filters with the
    /// (svc, port, conn) key. A sequential id keeps connections orderable and
    /// readable in logs, which a UUID would not.
    pub conn_id: u64,
    /// 0-based index of this message within the connection, in the given
    /// direction. Bumped once per evaluated chunk.
    pub seq: u64,
    /// `true` for client→service traffic, `false` for service→client. Selects
    /// which rule `Direction` applies and which endpoint is `src` vs `dst`.
    pub inbound: bool,
    /// Public port the client connected to on the filter (the listener port).
    pub public_port: u16,
    /// Upstream port the filter forwards to — i.e. the guarded service's port.
    pub upstream_port: u16,

    /// The chunk being evaluated, exactly as it appeared on the wire. Matchers
    /// see this in full; only the `raw_b64` *captured* in the record is capped.
    pub raw: &'a [u8],
    /// Max bytes of `raw` to base64-capture into the `EvaluationRecord`
    /// (`BODY_LIMIT_BYTES`). Matching is unaffected — this only bounds the
    /// per-record encode + storage cost for large frames.
    pub raw_capture_limit: usize,
    /// Parsed HTTP view of `raw`, when this chunk was a complete HTTP message.
    /// `None` for raw-stream / chunked-transfer chunks (HTTP-scoped rules skip).
    pub http: Option<&'a crate::http::HttpMessage>,
    /// Wire protocol classified for this chunk by the proxy (HTTP/1.x from the
    /// parsed version, HTTP/2 from the connection preface, else raw ascii/binary).
    pub protocol: shared::types::base_types::Protocol,

    /// IP/port of the endpoint that *sent* this chunk: the client on inbound,
    /// the upstream service on outbound.
    pub src_ip: &'a str,
    pub src_port: u16,
    /// IP/port of the endpoint that *receives* this chunk: the upstream service
    /// on inbound, the client on outbound. On inbound this is the real resolved
    /// service address (useful for filtering/debugging).
    pub dst_ip: &'a str,
    pub dst_port: u16,

    /// Wall-clock time (ns) the connection was accepted.
    pub conn_start_ns: u64,
    /// Wall-clock time (ns) of the previous evaluated message on this
    /// connection, for idle-gap timing matchers.
    pub conn_prev_msg_ns: u64,
    /// Wall-clock timestamp captured once for this message. Timing matchers read
    /// this instead of calling the global clock, so they are pure functions of
    /// the context and can be tested deterministically.
    pub msg_now_ns: u64,
}

/// Main entry: evaluate all rules, return (Decision, EvaluationRecord).
///
/// Pure function of `(compiled, ctx)` — it reads no shared/live state, which
/// keeps the proxy hot path lock-free and makes the decision logic directly
/// unit-testable.
pub fn evaluate(compiled: &Arc<CompiledRuleSet>, ctx: &MessageCtx) -> (Decision, EvaluationRecord) {
    // ----------------------------------------------------------------
    // Phase 1 – markers
    // ----------------------------------------------------------------
    let dir = if ctx.inbound {
        Direction::Inbound
    } else {
        Direction::Outbound
    };

    let mut details = Vec::<EvaluationDetail>::new();
    let mut verified_markers: HashSet<&str> = HashSet::new();

    for m in &compiled.markers {
        if !m.enabled || !m.direction.matches(dir) {
            continue;
        }

        let input = build_match_input(ctx, m.scope);
        let out = m.matcher.evaluate(&input);

        match out.status {
            MatchStatus::Satisfied => {
                verified_markers.insert(&m.id);
            }
            MatchStatus::NotSatisfied => {}
            MatchStatus::Skipped => {}
            MatchStatus::Error => {}
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
    }

    // ----------------------------------------------------------------
    // Phase 2 – filters
    // ----------------------------------------------------------------
    let mut blacklist_hits: Vec<String> = Vec::new();

    let mut wl_all_req: Vec<String> = Vec::new(); // gated whitelist-all filters
    let mut wl_all_hit: Vec<String> = Vec::new();

    let mut wl_any_req: Vec<String> = Vec::new(); // gated whitelist-any filters
    let mut wl_any_hit: Vec<String> = Vec::new();

    'filters: for f in &compiled.filters {
        if !f.enabled || !f.direction.matches(dir) {
            continue;
        }

        // gating ------------------------------------------------------------
        let gated = f
            .needed_all
            .iter()
            .all(|id| verified_markers.contains(id.as_str()))
            && (f.needed_any.is_empty()
                || f.needed_any
                    .iter()
                    .any(|id| verified_markers.contains(id.as_str())));

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

        // track whitelist groups only when gated
        match f.mode {
            Mode::WhitelistAll => wl_all_req.push(f.id.clone()),
            Mode::WhitelistAny => wl_any_req.push(f.id.clone()),
            _ => {}
        }

        // evaluate matcher ---------------------------------------------------
        let input = build_match_input(ctx, f.scope);
        let out = f.matcher.evaluate(&input);

        // account hits
        if out.status == MatchStatus::Satisfied {
            match f.mode {
                Mode::Blacklist => blacklist_hits.push(f.id.clone()),
                Mode::WhitelistAll => wl_all_hit.push(f.id.clone()),
                Mode::WhitelistAny => wl_any_hit.push(f.id.clone()),
            }
        }

        // decision_effect for telemetry
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

        // Stop at the first blacklist hit: the connection is already doomed, and
        // the remaining filters can't change a Drop into an Allow.
        if !blacklist_hits.is_empty() {
            break 'filters;
        }
    }

    // ----------------------------------------------------------------
    // Final decision + drop reasons
    // ----------------------------------------------------------------
    let mut drop_reasons = Vec::<DropReason>::new();

    let decision = if !blacklist_hits.is_empty() {
        drop_reasons.push(DropReason::Blacklist {
            filter_ids: blacklist_hits.clone(),
        });
        Decision::Drop
    } else {
        // whitelist-all miss?
        if !wl_all_req.is_empty() && wl_all_hit.len() != wl_all_req.len() {
            let missing: Vec<String> = wl_all_req
                .iter()
                .filter(|id| !wl_all_hit.contains(*id))
                .cloned()
                .collect();
            drop_reasons.push(DropReason::WhitelistAllMiss {
                required: wl_all_req.len(),
                hit: wl_all_hit.len(),
                missing_ids: missing,
            });
            Decision::Drop
        // whitelist-any miss?
        } else if !wl_any_req.is_empty() && wl_any_hit.is_empty() {
            drop_reasons.push(DropReason::WhitelistAnyMiss {
                group_size: wl_any_req.len(),
                hit_ids: wl_any_hit.clone(),
            });
            Decision::Drop
        } else {
            Decision::Allow
        }
    };

    // ----------------------------------------------------------------
    // Build EvaluationRecord
    // ----------------------------------------------------------------
    let http_info = ctx.http.map(|h| HttpInfo {
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
        conn_id: ctx.conn_id,
        message_seq: ctx.seq,
        inbound: ctx.inbound,
        public_port: ctx.public_port,
        upstream_port: ctx.upstream_port,
        protocol: ctx.protocol,
        markers: verified_markers.iter().map(|s| s.to_string()).collect(),
        decision,
        drop_reasons,
        http: http_info,
        raw_b64: base64::engine::general_purpose::STANDARD
            .encode(&ctx.raw[..ctx.raw.len().min(ctx.raw_capture_limit)]),
        details,
        src_ip: ctx.src_ip.to_owned(),
        src_port: ctx.src_port,
        dst_ip: ctx.dst_ip.to_owned(),
        dst_port: ctx.dst_port,
    };

    (decision, record)
}

// ----------------------------------------------------------------
fn build_match_input<'a>(msg: &'a MessageCtx, scope: Scope) -> MatchInput<'a> {
    MatchInput {
        scope,
        http: msg.http,
        raw: msg.raw,
        ctx: msg,
    }
}

fn build_detail(
    kind: &str,
    rule: &impl HasId, // trait object workaround
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
    fn id(&self) -> &String {
        &self.id
    }
    fn name(&self) -> &Option<String> {
        &self.name
    }
}
impl HasId for CompiledFilter {
    fn id(&self) -> &String {
        &self.id
    }
    fn name(&self) -> &Option<String> {
        &self.name
    }
}

fn meta_target_to_scope(t: &str) -> Scope {
    match t {
        "stream" => Scope::Stream,
        "body" => Scope::HttpBody,
        "status" => Scope::HttpStatus,
        "path" => Scope::HttpPath,
        "method" => Scope::HttpMethod,
        "timing" => Scope::Timing,
        s if s.starts_with("header") => Scope::HttpHeader,
        _ => Scope::Stream,
    }
}

#[cfg(test)]
mod tests {
    //! Decision-logic tests for [`evaluate`]. Rules use the `always` matcher so a
    //! rule's hit/miss is set directly, isolating the marker-gating and
    //! blacklist/whitelist resolution from any matcher behaviour.
    use super::*;
    use crate::rules::compile::compile_ruleset;
    use shared::types::base_types::{Mode, RuleDirection};
    use shared::types::ruleset_types::{FilterDef, MarkerDef, MatcherDef, RuleSetRaw};

    // ── builders ────────────────────────────────────────────────────────────

    /// A marker backed by `always`, satisfied iff `hit`.
    fn marker(id: &str, hit: bool, direction: RuleDirection) -> MarkerDef {
        MarkerDef {
            id: id.into(),
            name: None,
            direction,
            scope: Scope::Stream,
            matcher: MatcherDef::Always { value: hit },
            enabled: true,
        }
    }

    /// A filter backed by `always`, satisfied iff `hit`, with the given gates.
    fn filter(
        id: &str,
        mode: Mode,
        hit: bool,
        needed_all: &[&str],
        needed_any: &[&str],
        direction: RuleDirection,
    ) -> FilterDef {
        FilterDef {
            id: id.into(),
            name: None,
            direction,
            scope: Scope::Stream,
            matcher: MatcherDef::Always { value: hit },
            mode,
            needed_markers_all: needed_all.iter().map(|s| s.to_string()).collect(),
            needed_markers_any: needed_any.iter().map(|s| s.to_string()).collect(),
            enabled: true,
        }
    }

    fn ruleset(markers: Vec<MarkerDef>, filters: Vec<FilterDef>) -> Arc<CompiledRuleSet> {
        Arc::new(
            compile_ruleset(&RuleSetRaw {
                markers,
                filters,
                commit_message: None,
            })
            .expect("compile test ruleset"),
        )
    }

    fn ctx(inbound: bool) -> MessageCtx<'static> {
        MessageCtx {
            conn_id: 1,
            seq: 0,
            inbound,
            public_port: 8080,
            upstream_port: 80,
            raw: b"",
            raw_capture_limit: usize::MAX,
            http: None,
            protocol: shared::types::base_types::Protocol::RawBinary,
            src_ip: "127.0.0.1",
            src_port: 1234,
            dst_ip: "127.0.0.1",
            dst_port: 80,
            conn_start_ns: 0,
            conn_prev_msg_ns: 0,
            msg_now_ns: 0,
        }
    }

    /// Evaluate a ruleset against a single (inbound/outbound) message.
    fn decide(cr: &Arc<CompiledRuleSet>, inbound: bool) -> (Decision, EvaluationRecord) {
        evaluate(cr, &ctx(inbound))
    }

    const BOTH: RuleDirection = RuleDirection::Both;

    // ── empty / baseline ──────────────────────────────────────────────────────

    #[test]
    fn empty_ruleset_allows() {
        let cr = ruleset(vec![], vec![]);
        let (d, rec) = decide(&cr, true);
        assert_eq!(d, Decision::Allow);
        assert!(rec.drop_reasons.is_empty());
    }

    // ── blacklist ─────────────────────────────────────────────────────────────

    #[test]
    fn blacklist_hit_drops() {
        let cr = ruleset(
            vec![],
            vec![filter("bl", Mode::Blacklist, true, &[], &[], BOTH)],
        );
        let (d, rec) = decide(&cr, true);
        assert_eq!(d, Decision::Drop);
        match rec.drop_reasons.as_slice() {
            [DropReason::Blacklist { filter_ids }] => {
                assert_eq!(filter_ids, &["bl".to_string()]);
            }
            other => panic!("expected one Blacklist reason, got {other:?}"),
        }
    }

    #[test]
    fn blacklist_miss_allows() {
        let cr = ruleset(
            vec![],
            vec![filter("bl", Mode::Blacklist, false, &[], &[], BOTH)],
        );
        assert_eq!(decide(&cr, true).0, Decision::Allow);
    }

    #[test]
    fn blacklist_short_circuits_remaining_filters() {
        let cr = ruleset(
            vec![],
            vec![
                filter("bl1", Mode::Blacklist, true, &[], &[], BOTH),
                filter("bl2", Mode::Blacklist, true, &[], &[], BOTH),
            ],
        );
        let (d, rec) = decide(&cr, true);
        assert_eq!(d, Decision::Drop);
        // Only the first blacklist hit is recorded; bl2 is never evaluated.
        match rec.drop_reasons.as_slice() {
            [DropReason::Blacklist { filter_ids }] => assert_eq!(filter_ids, &["bl1".to_string()]),
            other => panic!("expected single Blacklist reason, got {other:?}"),
        }
        assert!(rec.details.iter().any(|d| d.id == "bl1"));
        assert!(!rec.details.iter().any(|d| d.id == "bl2"));
    }

    #[test]
    fn blacklist_takes_precedence_over_whitelist_miss() {
        let cr = ruleset(
            vec![],
            vec![
                filter("bl", Mode::Blacklist, true, &[], &[], BOTH),
                filter("w", Mode::WhitelistAll, false, &[], &[], BOTH),
            ],
        );
        let (d, rec) = decide(&cr, true);
        assert_eq!(d, Decision::Drop);
        assert!(matches!(
            rec.drop_reasons.as_slice(),
            [DropReason::Blacklist { .. }]
        ));
    }

    // ── whitelist-all ──────────────────────────────────────────────────────────

    #[test]
    fn whitelist_all_satisfied_allows() {
        let cr = ruleset(
            vec![],
            vec![
                filter("w1", Mode::WhitelistAll, true, &[], &[], BOTH),
                filter("w2", Mode::WhitelistAll, true, &[], &[], BOTH),
            ],
        );
        assert_eq!(decide(&cr, true).0, Decision::Allow);
    }

    #[test]
    fn whitelist_all_partial_miss_drops() {
        let cr = ruleset(
            vec![],
            vec![
                filter("w1", Mode::WhitelistAll, true, &[], &[], BOTH),
                filter("w2", Mode::WhitelistAll, false, &[], &[], BOTH),
            ],
        );
        let (d, rec) = decide(&cr, true);
        assert_eq!(d, Decision::Drop);
        match rec.drop_reasons.as_slice() {
            [
                DropReason::WhitelistAllMiss {
                    required,
                    hit,
                    missing_ids,
                },
            ] => {
                assert_eq!(*required, 2);
                assert_eq!(*hit, 1);
                assert_eq!(missing_ids, &["w2".to_string()]);
            }
            other => panic!("expected WhitelistAllMiss, got {other:?}"),
        }
    }

    // ── whitelist-any ──────────────────────────────────────────────────────────

    #[test]
    fn whitelist_any_one_hit_allows() {
        let cr = ruleset(
            vec![],
            vec![
                filter("a1", Mode::WhitelistAny, false, &[], &[], BOTH),
                filter("a2", Mode::WhitelistAny, true, &[], &[], BOTH),
            ],
        );
        assert_eq!(decide(&cr, true).0, Decision::Allow);
    }

    #[test]
    fn whitelist_any_all_miss_drops() {
        let cr = ruleset(
            vec![],
            vec![
                filter("a1", Mode::WhitelistAny, false, &[], &[], BOTH),
                filter("a2", Mode::WhitelistAny, false, &[], &[], BOTH),
            ],
        );
        let (d, rec) = decide(&cr, true);
        assert_eq!(d, Decision::Drop);
        assert!(matches!(
            rec.drop_reasons.as_slice(),
            [DropReason::WhitelistAnyMiss { group_size, hit_ids }]
                if *group_size == 2 && hit_ids.is_empty()
        ));
    }

    // ── marker gating ──────────────────────────────────────────────────────────

    #[test]
    fn gated_filter_skipped_when_marker_absent() {
        // The blacklist would hit, but its `needed_all` marker is unsatisfied.
        let cr = ruleset(
            vec![marker("m", false, BOTH)],
            vec![filter("bl", Mode::Blacklist, true, &["m"], &[], BOTH)],
        );
        let (d, rec) = decide(&cr, true);
        assert_eq!(d, Decision::Allow);
        let bl = rec.details.iter().find(|d| d.id == "bl").expect("detail");
        assert_eq!(bl.gated, Some(false));
    }

    #[test]
    fn gated_filter_runs_when_marker_present() {
        let cr = ruleset(
            vec![marker("m", true, BOTH)],
            vec![filter("bl", Mode::Blacklist, true, &["m"], &[], BOTH)],
        );
        let (d, rec) = decide(&cr, true);
        assert_eq!(d, Decision::Drop);
        assert!(rec.markers.contains(&"m".to_string()));
    }

    #[test]
    fn needed_any_requires_at_least_one_marker() {
        let markers = || vec![marker("m1", false, BOTH), marker("m2", true, BOTH)];
        // One of the two `needed_any` markers is satisfied → filter runs.
        let cr = ruleset(
            markers(),
            vec![filter(
                "bl",
                Mode::Blacklist,
                true,
                &[],
                &["m1", "m2"],
                BOTH,
            )],
        );
        assert_eq!(decide(&cr, true).0, Decision::Drop);

        // Neither satisfied → filter gated out.
        let cr2 = ruleset(
            vec![marker("m1", false, BOTH), marker("m2", false, BOTH)],
            vec![filter(
                "bl",
                Mode::Blacklist,
                true,
                &[],
                &["m1", "m2"],
                BOTH,
            )],
        );
        assert_eq!(decide(&cr2, true).0, Decision::Allow);
    }

    #[test]
    fn disabled_marker_does_not_satisfy_gate() {
        let mut m = marker("m", true, BOTH);
        m.enabled = false;
        let cr = ruleset(
            vec![m],
            vec![filter("bl", Mode::Blacklist, true, &["m"], &[], BOTH)],
        );
        assert_eq!(decide(&cr, true).0, Decision::Allow);
    }

    #[test]
    fn record_lists_only_satisfied_markers() {
        let cr = ruleset(
            vec![marker("yes", true, BOTH), marker("no", false, BOTH)],
            vec![],
        );
        let (_, rec) = decide(&cr, true);
        assert!(rec.markers.contains(&"yes".to_string()));
        assert!(!rec.markers.contains(&"no".to_string()));
    }

    // ── direction & enabled ────────────────────────────────────────────────────

    #[test]
    fn direction_mismatch_skips_filter() {
        let cr = ruleset(
            vec![],
            vec![filter(
                "bl",
                Mode::Blacklist,
                true,
                &[],
                &[],
                RuleDirection::Inbound,
            )],
        );
        // Outbound traffic: inbound-only filter is skipped → Allow.
        assert_eq!(decide(&cr, false).0, Decision::Allow);
        // Inbound traffic: filter fires → Drop.
        assert_eq!(decide(&cr, true).0, Decision::Drop);
    }

    #[test]
    fn disabled_filter_is_skipped() {
        let mut f = filter("bl", Mode::Blacklist, true, &[], &[], BOTH);
        f.enabled = false;
        let cr = ruleset(vec![], vec![f]);
        assert_eq!(decide(&cr, true).0, Decision::Allow);
    }
}
