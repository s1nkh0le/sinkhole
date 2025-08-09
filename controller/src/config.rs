use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use shared::utils::hashes::sha256_hex;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::time::Duration;
use tracing::{warn, info};

/// Per-controller configuration (global).
#[derive(Debug, Clone)]
pub struct ControllerCfg {
    pub debug: bool,
    /// Address the controller HTTP server binds to (e.g. "0.0.0.0:7000").
    pub bind: SocketAddr,
    /// Path to the controller cache DB.
    pub db_path: String,
    /// Background pull interval for logs/ecam.
    pub pull_interval: Duration,
    /// how often we refresh ports/config/ruleset
    pub sync_interval_secs: u64,
    /// max in-flight port fetches per service
    pub sync_concurrency: usize,
    /// control token hash
    pub ctrl_token_hash: Option<String>,
}

/// One upstream filter "service" the controller talks to.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub fn service_ids(&self) -> Vec<String> {
        self.services.iter().map(|s| s.id.clone()).collect()
    }
}

const ENV_PREFIX: &str = "SINKHOLE_CTRL_";

#[derive(Debug, Deserialize)]
struct RawCtrl {
    /// SINKHOLE_CTRL_DEBUG (bool)
    #[serde(default)]
    debug: bool,

    /// SINKHOLE_CTRL_BIND (e.g. "0.0.0.0:7000")
    #[serde(default)]
    bind: Option<String>,

    /// SINKHOLE_CTRL_DB_PATH (e.g. "ctrl.db")
    #[serde(default)]
    db_path: Option<String>,

    /// SINKHOLE_CTRL_PULL_INTERVAL_SECS (u64)
    #[serde(default)]
    pull_interval_secs: Option<u64>,

    /// SINKHOLE_CTRL_SYNC_INTERVAL_SECS (u64)
    #[serde(default)]
    sync_interval_secs: Option<u64>,

    /// SINKHOLE_CTRL_SYNC_CONCURRENCY (usize)
    #[serde(default)]
    sync_concurrency: Option<usize>,

    /// SINKHOLE_CTRL_API_TOKEN_HASH (sha256 hex) – takes precedence
    #[serde(default)]
    api_token_hash: Option<String>,

    /// SINKHOLE_CTRL_API_TOKEN (plain) – hashed if hash not provided
    #[serde(default)]
    api_token: Option<String>,
}

pub fn load() -> Result<Config> {
    // 1) Parse globals with envy
    let raw: RawCtrl = envy::prefixed(ENV_PREFIX)
        .from_env()
        .context("Failed to parse controller environment variables")?;

    // 2) Normalize globals
    let bind_str = raw.bind.unwrap_or_else(|| "0.0.0.0:7000".to_string());
    let bind: SocketAddr = bind_str
        .parse()
        .with_context(|| format!("invalid {}BIND='{}'", ENV_PREFIX, bind_str))?;

    let db_path = raw.db_path.unwrap_or_else(|| "ctrl.db".to_string());
    let pull_interval = Duration::from_secs(raw.pull_interval_secs.unwrap_or(2));
    let sync_interval_secs = raw.sync_interval_secs.unwrap_or(15);
    let sync_concurrency = raw.sync_concurrency.unwrap_or(4);

    let ctrl_token_hash = match (raw.api_token_hash, raw.api_token) {
        (Some(h), _) => Some(h),
        (None, Some(tok)) => Some(sha256_hex(tok.as_bytes())),
        (None, None) => None,
    };

    if ctrl_token_hash.is_none() {
        warn!("controller auth is DISABLED ({}API_TOKEN[_HASH] not set)", ENV_PREFIX);
    }

    let controller = ControllerCfg {
        debug: raw.debug,
        bind,
        db_path,
        pull_interval,
        sync_interval_secs,
        sync_concurrency,
        ctrl_token_hash,
    };

    // 3) Parse services (supports two patterns)
    let services = load_services_from_env()?;
    if services.is_empty() {
        bail!(
            "no services found: set {}SERVICE_0_URL and {}SERVICE_0_TOKEN \
             or use {}SERVICES=\"id1,id2\" + {}SERVICE_id_URL/TOKEN",
            ENV_PREFIX, ENV_PREFIX, ENV_PREFIX, ENV_PREFIX
        );
    }

    // 4) Validate services
    validate_services(&services)?;

    info!(
        services = services.len(),
        bind = %controller.bind,
        "controller config loaded"
    );

    Ok(Config { controller, services })
}

// ---------------------------------------------------------------------
// Services parsing
// ---------------------------------------------------------------------

fn load_services_from_env() -> Result<Vec<ServiceCfg>> {
    // Prefer named list if present, otherwise fall back to indexed.
    if let Some(list) = std::env::var_os(format!("{ENV_PREFIX}SERVICES")) {
        return load_named_services(list.to_string_lossy().as_ref());
    }
    load_indexed_services()
}

/// Pattern A: SINKHOLE_CTRL_SERVICES="edge,api"
/// Then each:
///   SINKHOLE_CTRL_SERVICE_edge_URL
///   SINKHOLE_CTRL_SERVICE_edge_TOKEN
///   SINKHOLE_CTRL_SERVICE_edge_NAME (optional)
fn load_named_services(list: &str) -> Result<Vec<ServiceCfg>> {
    let mut out = Vec::new();
    for raw_id in list.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let url_key   = format!("{ENV_PREFIX}SERVICE_{}_URL", raw_id);
        let token_key = format!("{ENV_PREFIX}SERVICE_{}_TOKEN", raw_id);
        let name_key  = format!("{ENV_PREFIX}SERVICE_{}_NAME", raw_id);

        let url = std::env::var(&url_key).with_context(|| format!("missing {}", url_key))?;
        let token = std::env::var(&token_key).with_context(|| format!("missing {}", token_key))?;
        let name = std::env::var(&name_key).ok();

        out.push(ServiceCfg {
            id: raw_id.to_string(),
            base_url: normalize_base_url(&url),
            token,
            name,
        });
    }
    Ok(out)
}

/// Pattern B: Indexed
///   SINKHOLE_CTRL_SERVICE_0_URL
///   SINKHOLE_CTRL_SERVICE_0_TOKEN
///   SINKHOLE_CTRL_SERVICE_0_ID (optional, default "svc0")
///   SINKHOLE_CTRL_SERVICE_0_NAME (optional)
fn load_indexed_services() -> Result<Vec<ServiceCfg>> {
    let mut out = Vec::new();
    for idx in 0.. {
        let url_key   = format!("{ENV_PREFIX}SERVICE_{}_URL", idx);
        let token_key = format!("{ENV_PREFIX}SERVICE_{}_TOKEN", idx);
        let id_key    = format!("{ENV_PREFIX}SERVICE_{}_ID", idx);
        let name_key  = format!("{ENV_PREFIX}SERVICE_{}_NAME", idx);

        let Some(url_os) = std::env::var_os(&url_key) else {
            // stop at the first gap to keep it simple
            break;
        };
        let url = url_os.into_string().map_err(|_| anyhow!("invalid UTF-8 in {}", url_key))?;
        let token = std::env::var(&token_key)
            .with_context(|| format!("missing {}", token_key))?;
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
    let mut s = u.trim().to_string();
    while s.ends_with('/') { s.pop(); }
    s
}

fn validate_services(services: &[ServiceCfg]) -> Result<()> {
    let mut seen = HashSet::new();
    for s in services {
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
                s.id, s.base_url
            );
        }
    }
    Ok(())
}
