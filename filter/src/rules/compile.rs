use std::sync::Arc;

use crate::matcher::Matcher;
use crate::matcher::always::AlwaysMatcher;
use crate::matcher::bytes_core::BytesRegexCore;
use crate::matcher::bytes_matcher::BytesMatcher;
use crate::matcher::regex_matcher::RegexMatcher;
use crate::matcher::session_idle::SessionIdleMatcher;
use crate::matcher::session_lifetime::SessionLifetimeMatcher;
use crate::matcher::size::SizeMatcher;
use crate::rules::legality::check_rule;
use anyhow::{Result, anyhow};
use shared::types::base_types::{Mode, RuleDirection, Scope};
use shared::types::ruleset_types::{FilterDef, MarkerDef, MatcherDef, RuleSetRaw};
use shared::utils::version::calc_version_ruleset;

#[derive(Clone)]
pub struct CompiledMarker {
    pub id: String,
    pub name: Option<String>,
    pub direction: RuleDirection,
    pub scope: Scope,
    pub matcher: Arc<dyn Matcher>,
    pub enabled: bool,
    pub meta: MatcherMeta,
}

#[derive(Clone)]
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

#[derive(Debug, Clone)]
pub struct MatcherMeta {
    pub comparator: String, // "regex" | "size"
    pub target: String,     // e.g. "header:user-agent", "body", "stream", "path", "always"
    pub condition: String,  // human‑readable: `regex /foo/` or `size > 1000`
}

#[derive(Clone)]
pub struct CompiledRuleSet {
    pub version: String,
    pub markers: Vec<CompiledMarker>,
    pub filters: Vec<CompiledFilter>,
}

// --------------------------------------------------
// Public entry
// --------------------------------------------------
pub fn compile_ruleset(raw: &RuleSetRaw) -> Result<CompiledRuleSet> {
    let version_ruleset = calc_version_ruleset(raw);

    let markers = raw
        .markers
        .iter()
        .map(compile_marker)
        .collect::<Result<Vec<_>>>()?;

    let filters = raw
        .filters
        .iter()
        .map(compile_filter)
        .collect::<Result<Vec<_>>>()?;

    Ok(CompiledRuleSet {
        version: version_ruleset,
        markers,
        filters,
    })
}

// --------------------------------------------------
// helpers
// --------------------------------------------------
fn compile_marker(m: &MarkerDef) -> Result<CompiledMarker> {
    let (matcher, meta) = compile_matcher(&m.scope, &m.matcher, m.direction)?;
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
    let (matcher, meta) = compile_matcher(&f.scope, &f.matcher, f.direction)?;
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

fn compile_matcher(
    scope: &Scope,
    def: &MatcherDef,
    direction: RuleDirection,
) -> Result<(Arc<dyn Matcher>, MatcherMeta)> {
    // Legality is decided in exactly one place (`rules::legality`); compilation
    // refuses anything that wouldn't pass validation, and never panics.
    check_rule(*scope, def, direction).map_err(|e| anyhow!(e))?;

    use MatcherDef::*;
    match def {
        StreamRegex {
            pattern,
            case_sensitive,
        } => build_regex(*scope, pattern, *case_sensitive, None, "stream".into()),
        HeaderRegex {
            header_name,
            pattern,
            case_sensitive,
        } => build_regex(
            *scope,
            pattern,
            *case_sensitive,
            Some(header_name),
            format!("header:{}", header_name.to_ascii_lowercase()),
        ),
        StatusRegex {
            pattern,
            case_sensitive,
        } => build_regex(*scope, pattern, *case_sensitive, None, "status".into()),
        Size {
            op,
            value,
            field_name,
        } => {
            let cmp = *op;
            let m = SizeMatcher::new(cmp, *value, field_name.clone());

            let target = match scope {
                Scope::HttpHeader => format!("headers[{}]", field_name.as_deref().unwrap_or("?")),
                Scope::HttpPath => "path".into(),
                Scope::HttpBody => "body".into(),
                Scope::Stream => "stream".into(),
                // Guaranteed unreachable by `check_rule`; keep compile total & panic-free.
                other => {
                    return Err(anyhow!(
                        "size matcher is not allowed with scope {:?}",
                        other
                    ));
                }
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
        PathRegex {
            pattern,
            case_sensitive,
        } => build_regex(*scope, pattern, *case_sensitive, None, "path".into()),
        BodyRegex {
            pattern,
            case_sensitive,
        } => build_regex(*scope, pattern, *case_sensitive, None, "body".into()),
        StreamBytes { pattern } => build_bytes(
            *scope,
            BytesRegexCore::new(pattern).map_err(|e| anyhow!("invalid byte-regex: {}", e)),
            format!("bytes /{}/", pattern),
            "stream".into(),
        ),
        BodyBytes { pattern } => build_bytes(
            *scope,
            BytesRegexCore::new(pattern).map_err(|e| anyhow!("invalid byte-regex: {}", e)),
            format!("bytes /{}/", pattern),
            "body".into(),
        ),
        BytesHex { value } => build_bytes(
            *scope,
            BytesRegexCore::from_hex(value).map_err(|e| anyhow!("invalid hex signature: {}", e)),
            format!("hex {}", value),
            match scope {
                Scope::HttpBody => "body".into(),
                _ => "stream".into(),
            },
        ),
        MethodRegex {
            pattern,
            case_sensitive,
        } => build_regex(*scope, pattern, *case_sensitive, None, "method".into()),
        SessionLifetime { op, value } => {
            let cmp = *op;
            Ok((
                Arc::new(SessionLifetimeMatcher {
                    op: cmp,
                    value: *value,
                }),
                MatcherMeta {
                    comparator: format!("timing {} {}s", op, value),
                    target: "conn_lifetime".into(),
                    condition: format!("timing {} {}s", op, value),
                },
            ))
        }
        SessionIdle { op, value } => {
            let cmp = *op;
            Ok((
                Arc::new(SessionIdleMatcher {
                    op: cmp,
                    value: *value,
                }),
                MatcherMeta {
                    comparator: format!("idle {} {}s", op, value),
                    target: "conn_idle".into(),
                    condition: format!("idle {} {}s", op, value),
                },
            ))
        }
        Always { value } => Ok((
            Arc::new(AlwaysMatcher::new(*value)),
            MatcherMeta {
                comparator: "const".into(),
                target: "always".into(),
                condition: format!("always {}", value),
            },
        )),
    }
}

/// Build a [`BytesMatcher`] + telemetry metadata for a byte-regex / hex scope.
fn build_bytes(
    scope: Scope,
    core: Result<BytesRegexCore>,
    condition: String,
    target: String,
) -> Result<(Arc<dyn Matcher>, MatcherMeta)> {
    let core = core?;
    Ok((
        Arc::new(BytesMatcher::new(scope, core)),
        MatcherMeta {
            comparator: "bytes".into(),
            target,
            condition,
        },
    ))
}

/// Build a unified [`RegexMatcher`] + its telemetry metadata for any regex scope.
fn build_regex(
    scope: Scope,
    pattern: &str,
    case_sensitive: bool,
    header_name: Option<&str>,
    target: String,
) -> Result<(Arc<dyn Matcher>, MatcherMeta)> {
    let m = RegexMatcher::new(scope, pattern, case_sensitive, header_name)
        .map_err(|e| anyhow!("invalid regex /{}/: {}", pattern, e))?;
    Ok((
        Arc::new(m),
        MatcherMeta {
            comparator: "regex".into(),
            target,
            condition: format!("regex /{}/", pattern),
        },
    ))
}
