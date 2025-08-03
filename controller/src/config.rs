use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::time::Duration;

/// Per-controller configuration (global).
#[derive(Debug, Clone)]
pub struct ControllerCfg {
    /// Address the controller HTTP server binds to (e.g. "0.0.0.0:7000").
    pub bind: SocketAddr,
    /// Path to the controller cache DB.
    pub db_path: String,
    /// Background pull interval for logs/ecam.
    pub pull_interval: Duration,

    pub sync_interval_secs: u64, // how often we refresh ports/config/ruleset
    pub sync_concurrency: usize, // max in-flight port fetches per service
}

/// One upstream filter "service" the controller talks to.
#[derive(Debug, Clone, Deserialize)]
pub struct ServiceCfg {
    /// Stable identifier (must be unique). Used in API paths and DB keys.
    pub id: String,
    /// Base URL of the filter (no trailing slash; e.g. "http://127.0.0.1:7001").
    pub base_url: String,
    /// Bearer/token to authenticate against the filter.
    pub token: String,
    /// Optional human-friendly display name.
    pub name: Option<String>,
}

/// Full configuration loaded at startup.
#[derive(Debug, Clone)]
pub struct Config {
    pub controller: ControllerCfg,
    pub services: Vec<ServiceCfg>,
}

impl Config {
    /// Convenience: list of service IDs.
    pub fn service_ids(&self) -> Vec<String> {
        self.services.iter().map(|s| s.id.clone()).collect()
    }
}

pub fn load() -> Result<Config> {
    // ── Controller (globals) ───────────────────────────────────────────────
    // Prefer CTRL_* keys; accept old names for compatibility.
    let bind_str = std::env::var("CTRL_BIND")
        .or_else(|_| std::env::var("CONTROLLER_BIND"))
        .unwrap_or_else(|_| "0.0.0.0:7000".to_string());

    let bind: SocketAddr = bind_str
        .parse()
        .with_context(|| format!("invalid CTRL_BIND/CONTROLLER_BIND: '{bind_str}'"))?;

    let db_path = std::env::var("CTRL_DB_PATH")
        .unwrap_or_else(|_| "ctrl.db".to_string());

    let pull_secs = std::env::var("CTRL_PULL_INTERVAL_SECS")
        .or_else(|_| std::env::var("PULL_INTERVAL_SECS"))
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(2);
    let pull_interval = Duration::from_secs(pull_secs);

    // NEW: env for sync task
    let sync_interval_secs = std::env::var("SYNC_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(15);

    let sync_concurrency = std::env::var("SYNC_CONCURRENCY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);

    let controller = ControllerCfg {
        bind,
        db_path,
        pull_interval,
        sync_interval_secs,
        sync_concurrency
    };

    // ── Services (upstreams) ───────────────────────────────────────────────
    // Preferred: SERVICE_{N}_URL / SERVICE_{N}_TOKEN / SERVICE_{N}_ID / SERVICE_{N}_NAME
    // Legacy fallback: FILTER_{N}_URL / FILTER_{N}_TOKEN (warn).
    let services = load_services_from_env()?;

    if services.is_empty() {
        bail!("no services found: set SERVICE_0_URL and SERVICE_0_TOKEN (or legacy FILTER_0_URL/TOKEN)");
    }

    // Validate uniqueness of IDs
    let mut seen = HashSet::new();
    for s in &services {
        if s.id.trim().is_empty() {
            bail!("service id must not be empty (service with base_url={})", s.base_url);
        }
        if !seen.insert(s.id.clone()) {
            bail!("duplicate service id '{}'", s.id);
        }
        if s.token.trim().is_empty() {
            bail!("empty token for service '{}'", s.id);
        }
        if !s.base_url.starts_with("http://") && !s.base_url.starts_with("https://") {
            bail!(
                "service '{}' base_url must start with http:// or https:// (got '{}')",
                s.id,
                s.base_url
            );
        }
    }

    Ok(Config { controller, services })
}

// Scan SERVICE_{N}_* first; if none found, scan legacy FILTER_{N}_* (warn).
fn load_services_from_env() -> Result<Vec<ServiceCfg>> {
    let services = scan_indexed_services("SERVICE")?;
    Ok(services)
}

fn scan_indexed_services(prefix: &str) -> Result<Vec<ServiceCfg>> {
    let mut out = Vec::new();

    // Two patterns supported:
    //  A) Indexed:    {PREFIX}_{N}_URL, _TOKEN, optional _ID, _NAME
    //  B) Named list: {PREFIX}S="foo,bar"; then {PREFIX}_foo_URL/TOKEN, {PREFIX}_bar_URL/TOKEN,...
    //
    // We implement (A) now; (B) can be added later if needed.

    for idx in 0.. {
        let url_key   = format!("{prefix}_{idx}_URL");
        let token_key = format!("{prefix}_{idx}_TOKEN");
        let id_key    = format!("{prefix}_{idx}_ID");
        let name_key  = format!("{prefix}_{idx}_NAME");

        let Some(url_os) = std::env::var_os(&url_key) else {
            // stop at first gap (contiguous block)
            break;
        };
        let url = url_os.into_string().map_err(|_| anyhow!("invalid UTF-8 in {}", url_key))?;

        let token = std::env::var(&token_key)
            .with_context(|| format!("missing {token_key} for {prefix}_{idx}"))?;

        let id = std::env::var(&id_key).unwrap_or_else(|_| format!("svc{idx}"));
        let name = std::env::var(&name_key).ok();

        out.push(ServiceCfg {
            id,
            base_url: normalize_base_url(&url),
            token,
            name,
        });
    }

    Ok(out)
}

fn normalize_base_url(u: &str) -> String {
    // strip trailing slashes for consistent joining
    let mut s = u.trim().to_string();
    while s.ends_with('/') {
        s.pop();
    }
    s
}
