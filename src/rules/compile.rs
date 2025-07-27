use std::sync::Arc;

use super::{
    model::{FilterDef, MarkerDef, MatcherDef, RuleSetRaw},
    version::calc_version,
};
use crate::matcher::Matcher;
use crate::matcher::size::{CmpOp, SizeMatcher};
use crate::types::{Mode, RuleDirection, Scope};
use anyhow::{Result, anyhow};
use crate::matcher::always::AlwaysMatcher;

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
        Size { op, value } => Ok((
            Arc::new(SizeMatcher::new(parse_op(op)?, *value)),
            MatcherMeta {
                comparator: "size".into(),
                target: match scope {
                    Scope::Meta => "meta".into(),
                    Scope::Stream => "stream".into(),
                    Scope::HttpBody => "body".into(),
                    Scope::HttpPath => "path".into(),
                    Scope::HttpHeader => "header".into(),
                },
                condition: format!("size {} {}", op, value),
            },
        )),
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
