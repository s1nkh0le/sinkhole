use crate::registry::PortRuntime;
use crate::rules::compile::CompiledRuleSet;
use std::sync::Arc;
use tracing::debug;
use shared::rules_model::{MatcherDef, RuleSetRaw};

/// Emit a concise snapshot of port state at DEBUG level.
pub fn dump_port_state(rt: &Arc<PortRuntime>) {
    // ---- guards ---------------------------------------------------------
    let compiled_guard = rt.compiled.load();
    let compiled: &CompiledRuleSet = &*compiled_guard;

    let raw_guard = rt.raw_ruleset.load();
    let raw: &RuleSetRaw = &*raw_guard;

    // ---- meta -----------------------------------------------------------
    let name_guard = rt.name.load();
    let name = name_guard
        .as_ref()
        .map(|s| s.as_str())
        .unwrap_or("-");

    let scopes = rt
        .enabled_scopes
        .read()
        .expect("PortRuntime.enabled_scopes lock poisoned");

    let scope_list = scopes.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(", ");

    debug!(
        port = rt.binding.public,
        upstream = rt.binding.upstream,
        name = name,
        enabled_scopes = scope_list,
        version = compiled.version,
        markers = compiled.markers.len(),
        filters = compiled.filters.len(),
        "PortSnapshot",
    );

    // ---- markers --------------------------------------------------------
    for m in &raw.markers {
        debug!(
            port = rt.binding.public,
            id = %m.id,
            dir = ?m.direction,
            scope = ?m.scope,
            enabled = m.enabled,
            matcher = %matcher_desc(&m.matcher),
            "-> Marker",
        );
    }

    // ---- filters --------------------------------------------------------
    for f in &raw.filters {
        debug!(
            port = rt.binding.public,
            dir = ?f.direction,
            scope = ?f.scope,
            mode = ?f.mode,
            matcher = %matcher_desc(&f.matcher),
            "-> Filter(id={}, enabled={}): ", f.id, f.enabled,
        );
    }
}

/// stringify MatcherDef for debug
fn matcher_desc(m: &MatcherDef) -> String {
    use MatcherDef::*;
    match m {
        StreamRegex { pattern, .. }  => format!("stream_regex /{}/", pattern),
        PathRegex   { pattern, .. }  => format!("path_regex /{}/",   pattern),
        BodyRegex   { pattern, .. }  => format!("body_regex /{}/",   pattern),
        HeaderRegex { header_name, pattern, .. } =>
            format!("header_regex {}=/{} /", header_name, pattern),
        MethodRegex { pattern, .. }  => format!("method_regex /{}/", pattern),
        StatusRegex { pattern, .. }  => format!("status_regex /{}/", pattern),
        SessionLifetime { value, op, .. }  => format!("session_lifetime /{} {}/", op, value),
        SessionIdle { value, op, .. }  => format!("session_idle /{} {}/", op, value),
        Size { op, value, field_name } => {
            if let Some(h) = field_name {
                format!("size {} {} (header:{})", op, value, h)
            } else {
                format!("size {} {}", op, value)
            }
        }
        Always { value } => format!("always = {}", value),
    }
}
