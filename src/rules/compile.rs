use crate::matcher::header_regex::HeaderRegex;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use crate::matcher::Matcher;
use crate::matcher::size::{CmpOp, SizeMatcher};
use crate::types::{Mode, RuleDirection, Scope};
use super::{
    model::{FilterDef, MatcherDef, MarkerDef, RuleSetRaw},
    version::calc_version,
};

pub struct CompiledMarker {
    pub id: String,
    pub name: Option<String>,
    pub direction: RuleDirection,
    pub scope: Scope,
    pub matcher: Arc<dyn Matcher>,
    pub enabled: bool,
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
    Ok(CompiledMarker {
        id: m.id.clone(),
        name: m.name.clone(),
        direction: m.direction,
        scope: m.scope,
        matcher: compile_matcher(&m.scope, &m.matcher)?,
        enabled: m.enabled,
    })
}

fn compile_filter(f: &FilterDef) -> Result<CompiledFilter> {
    Ok(CompiledFilter {
        id: f.id.clone(),
        name: f.name.clone(),
        direction: f.direction,
        scope: f.scope,
        matcher: compile_matcher(&f.scope, &f.matcher)?,
        mode: f.mode,
        needed_all: f.needed_markers_all.clone(),
        needed_any: f.needed_markers_any.clone(),
        enabled: f.enabled,
    })
}

fn compile_matcher(scope: &Scope, def: &MatcherDef) -> Result<Arc<dyn Matcher>> {
    use MatcherDef::*;
    match def {
        StreamRegex { pattern, case_sensitive } => {
            ensure_scope(scope, Scope::Stream, "stream_regex")?;
            Ok(Arc::new(crate::matcher::stream_regex::StreamRegex::new(pattern, *case_sensitive)?))
        }
        PathRegex { pattern, case_sensitive } => {
            ensure_scope(scope, Scope::HttpPath, "path_regex")?;
            Ok(Arc::new(crate::matcher::path_regex::PathRegex::new(pattern, *case_sensitive)?))
        }
        BodyRegex { pattern, case_sensitive } => {
            ensure_scope(scope, Scope::HttpBody, "body_regex")?;
            Ok(Arc::new(crate::matcher::body_regex::BodyRegex::new(pattern, *case_sensitive)?))
        }
        HeaderRegex {
            header_name,
            pattern,
            case_sensitive,
        } => {
            ensure_scope(scope, Scope::HttpHeader, "header_regex")?;
            Ok(Arc::new(crate::matcher::header_regex::HeaderRegex::new(
                header_name,
                pattern,
                *case_sensitive,
            )?))
        }
        Size { op, value } => {
            let cmp = parse_op(op)?;
            Ok(Arc::new(SizeMatcher::new(cmp, *value)))
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
