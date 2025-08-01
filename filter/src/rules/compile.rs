use std::sync::Arc;

use super::version::calc_version;
use crate::matcher::Matcher;
use crate::matcher::size::SizeMatcher;
use shared::types::{Mode, RuleDirection, Scope};
use anyhow::{anyhow, bail, Result};
use shared::rules_model::{FilterDef, MarkerDef, MatcherDef, RuleSetRaw};
use crate::matcher::always::AlwaysMatcher;
use crate::matcher::method_regex::MethodRegexMatcher;
use crate::matcher::session_idle::SessionIdleMatcher;
use crate::matcher::session_lifetime::SessionLifetimeMatcher;
use crate::matcher::status_regex::StatusRegexMatcher;
use crate::matcher::types::CmpOp;

pub struct CompiledMarker {
    pub id: String,
    pub name: Option<String>,
    pub direction: RuleDirection,
    pub scope: Scope,
    pub matcher: Arc<dyn Matcher>,
    pub enabled: bool,
    pub meta: MatcherMeta,
}

pub struct CompiledFilter {
    pub id: String,
    pub name: Option<String>,
    pub direction: RuleDirection,
    pub scope: Scope,
    pub matcher: Arc<dyn Matcher>,
    pub mode: Mode,
    pub needed_all: Vec<String>,
    pub needed_any: Vec<String>,
    pub enabled: bool,
    pub meta: MatcherMeta,
}

#[derive(Clone)]
pub struct MatcherMeta {
    pub comparator: String, // "regex" | "size"
    pub target: String,     // e.g. "header:user-agent", "body", "stream", "path", "meta"
    pub condition: String,  // human‑readable: `regex /foo/` or `size > 1000`
}

pub struct CompiledRuleSet {
    pub version: String,
    pub markers: Vec<CompiledMarker>,
    pub filters: Vec<CompiledFilter>,
}

// --------------------------------------------------
// Public entry
// --------------------------------------------------
pub fn compile_ruleset(raw: &RuleSetRaw) -> Result<CompiledRuleSet> {
    let version = calc_version(raw)?;

    let markers = raw
        .markers
        .iter()
        .map(|m| compile_marker(m))
        .collect::<Result<Vec<_>>>()?;

    let filters = raw
        .filters
        .iter()
        .map(|f| compile_filter(f))
        .collect::<Result<Vec<_>>>()?;

    Ok(CompiledRuleSet {
        version,
        markers,
        filters,
    })
}

// --------------------------------------------------
// helpers
// --------------------------------------------------
fn compile_marker(m: &MarkerDef) -> Result<CompiledMarker> {
    let (matcher, meta) = compile_matcher(&m.scope, &m.matcher)?;
    Ok(CompiledMarker {
        id: m.id.clone(),
        name: m.name.clone(),
        direction: m.direction,
        scope: m.scope,
        matcher,
        meta,
        enabled: m.enabled,
    })
}

fn compile_filter(f: &FilterDef) -> Result<CompiledFilter> {
    let (matcher, meta) = compile_matcher(&f.scope, &f.matcher)?;
    Ok(CompiledFilter {
        id: f.id.clone(),
        name: f.name.clone(),
        direction: f.direction,
        scope: f.scope,
        matcher,
        meta,
        mode: f.mode,
        needed_all: f.needed_markers_all.clone(),
        needed_any: f.needed_markers_any.clone(),
        enabled: f.enabled,
    })
}


fn compile_matcher(scope: &Scope, def: &MatcherDef) -> Result<(Arc<dyn Matcher>, MatcherMeta)> {
    use MatcherDef::*;
    match def {
        StreamRegex { pattern, .. } => Ok((
            Arc::new(crate::matcher::stream_regex::StreamRegex::new(
                pattern, false,
            )?),
            MatcherMeta {
                comparator: "regex".into(),
                target: "stream".into(),
                condition: format!("regex /{}/", pattern),
            },
        )),
        HeaderRegex {
            header_name,
            pattern,
            ..
        } => Ok((
            Arc::new(crate::matcher::header_regex::HeaderRegex::new(
                header_name,
                pattern,
                false,
            )?),
            MatcherMeta {
                comparator: "regex".into(),
                target: format!("header:{}", header_name.to_ascii_lowercase()),
                condition: format!("regex /{}/", pattern),
            },
        )),
        StatusRegex { pattern, case_sensitive } => {
            let m = StatusRegexMatcher::new(pattern, *case_sensitive)
                .map_err(|e| anyhow!("invalid status regex: {}", e))?;
            Ok((
                Arc::new(m),
                MatcherMeta {
                    comparator: format!("regex /{}/", pattern),
                    target: "status".into(),
                    condition: format!("regex /{}/", pattern),
                },
            ))
        }
        Size { op, value, field_name } => {
            let cmp = parse_op(op)?; // your existing parser
            let m = SizeMatcher {
                op: cmp,
                value: *value,
                field_name: field_name.clone(),
            };

            let target = match scope {
                Scope::HttpHeader => format!("headers[{}]", field_name.as_deref().unwrap_or("?")),
                Scope::HttpPath   => "path".into(),
                Scope::HttpBody   => "body".into(),
                Scope::HttpMethod => "method".into(),
                Scope::HttpStatus => "status".into(), // can't happen
                Scope::Stream     => "stream".into(),
                Scope::Timing     => "timing".into(), // can't happen
                Scope::Meta       => "meta".into(),
            };

            Ok((
                Arc::new(m),
                MatcherMeta {
                    comparator: format!("size {} {}", op, value),
                    target,
                    condition: format!("size {} {}", op, value),
                },
            ))
        }
        PathRegex { pattern, .. } => Ok((
            Arc::new(crate::matcher::path_regex::PathRegex::new(
                pattern,
                false,
            )?),
            MatcherMeta {
                comparator: "regex".into(),
                target: "path".into(),
                condition: format!("regex /{}/", pattern),
            },
        )),
        BodyRegex { pattern, .. } => Ok((
            Arc::new(crate::matcher::body_regex::BodyRegex::new(
                pattern,
                false,
            )?),
            MatcherMeta {
                comparator: "regex".into(),
                target: "body".into(),
                condition: format!("regex /{}/", pattern),
            },
        )),
        MethodRegex { pattern, case_sensitive } => {
            let m = MethodRegexMatcher::new(pattern, *case_sensitive)
                .map_err(|e| anyhow!("invalid regex for method: {}", e))?;

            Ok((
                Arc::new(m),
                MatcherMeta {
                    comparator: format!("regex /{}/", pattern),
                    target: "method".into(),
                    condition: format!("regex /{}/", pattern),
                },
            ))
        }
        SessionLifetime { op, value } => {
            let cmp = parse_op(op)?;
            ensure_timing_scope(*scope, "session_lifetime")?;
            Ok((
                Arc::new(SessionLifetimeMatcher { op: cmp, value: *value }),
                MatcherMeta {
                    comparator: format!("timing {} {}s", op, value),
                    target: "conn_lifetime".into(),
                    condition: format!("timing {} {}s", op, value),
                },
            ))
        }
        SessionIdle { op, value } => {
            let cmp = parse_op(op)?;
            ensure_timing_scope(*scope, "session_idle")?;
            Ok((
                Arc::new(SessionIdleMatcher { op: cmp, value: *value }),
                MatcherMeta {
                    comparator: format!("idle {} {}s", op, value),
                    target: "conn_idle".into(),
                    condition: format!("idle {} {}s", op, value),
                },
            ))
        }
        Always { value } => {
            Ok((
                Arc::new(AlwaysMatcher::new(*value)),
                MatcherMeta {
                    comparator: "const".into(),
                    target: "meta".into(),
                    condition: format!("always {}", value),
                },
            ))
        }
    }
}

fn ensure_scope(actual: &Scope, expected: Scope, mname: &str) -> Result<()> {
    if *actual != expected {
        Err(anyhow!(
            "matcher '{}' only allowed with scope {:?}",
            mname,
            expected
        ))
    } else {
        Ok(())
    }
}

fn ensure_timing_scope(scope: Scope, ctx: &str) -> Result<()> {
    if scope != Scope::Timing {
        bail!("{} matcher must use scope 'timing'", ctx);
    }
    Ok(())
}

fn parse_op(s: &str) -> Result<CmpOp> {
    use CmpOp::*;
    Ok(match s {
        ">" => Gt,
        ">=" => Ge,
        "<" => Lt,
        "<=" => Le,
        "==" => Eq,
        "!=" => Ne,
        _ => return Err(anyhow!("invalid comparator op '{}'", s)),
    })
}
