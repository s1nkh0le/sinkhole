use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt, net::SocketAddr, str::FromStr};
use tracing::{debug, info, warn};

const DEFAULT_CTRL_PORT: u16 = 7000;
const DEFAULT_BODY_LIMIT: usize = 1_048_576; // 1MB

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PortBinding {
    pub public: u16,
    pub upstream: u16,
}

impl fmt::Display for PortBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}->{}", self.public, self.upstream)
    }
}

#[derive(Clone, Debug, Deserialize)]
struct RawConfig {
    /// TFILTER_UPSTREAM_HOST (required)
    pub upstream_host: String,

    /// TFILTER_LISTEN_MAP = "8080:80,8443:8000"
    #[serde(default)]
    pub listen_map: String,

    /// TFILTER_CTRL_PORT (default 7000)
    #[serde(default = "default_ctrl_port")]
    pub ctrl_port: u16,

    /// TFILTER_DEBUG = true/false
    #[serde(default)]
    pub debug: bool,

    /// TFILTER_AUTH_HASH = hex sha256
    #[serde(default)]
    pub auth_hash: Option<String>,

    /// TFILTER_BODY_LIMIT_BYTES
    #[serde(default)]
    pub body_limit_bytes: Option<usize>,

    #[serde(default)]
    pub hydrate_db: bool,

    #[serde(default = "default_db_path")]
    pub db_path: String,

    #[serde(default)]
    pub sinks_stdout: Option<bool>,
    #[serde(default)]
    pub sinks_emit:   Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Config {
    pub upstream_host: String,
    pub ports: Vec<PortBinding>,
    pub ctrl_port: u16,
    pub debug: bool,
    pub auth_hash: Option<String>,
    pub body_limit_bytes: usize,
    pub hydrate_db: bool,
    pub db_path: String,
    pub sinks_stdout: bool,
    pub sinks_emit: bool,
}

impl Config {
    pub fn ctrl_bind_addr(&self) -> SocketAddr {
        SocketAddr::from(([0, 0, 0, 0], self.ctrl_port))
    }
}

fn default_ctrl_port() -> u16 { DEFAULT_CTRL_PORT }
fn default_db_path() -> String { "tfilter.db".into() }

// ---------------------------------------------------------------------
// Public loader
// ---------------------------------------------------------------------
pub fn load() -> Result<Config> {
    // Load from env
    let raw: RawConfig = envy::prefixed("TFILTER_")
        .from_env()
        .context("Failed to parse environment variables")?;

    debug!(?raw, "raw config loaded");

    // Validate & normalize
    let mut errs = Vec::new();

    if raw.upstream_host.trim().is_empty() {
        errs.push("TFILTER_UPSTREAM_HOST is empty".to_string());
    }

    // listen_map parsing
    let ports = match parse_listen_map(&raw.listen_map) {
        Ok(v) => v,
        Err(e) => {
            errs.push(format!("TFILTER_LISTEN_MAP: {}", e));
            Vec::new()
        }
    };

    // Duplicates / collisions
    let mut seen_pub = HashSet::new();
    for p in &ports {
        if p.public == 0 {
            errs.push("public port cannot be 0".to_string());
        }
        if p.upstream == 0 {
            errs.push(format!("upstream port cannot be 0 (public {})", p.public));
        }
        if !seen_pub.insert(p.public) {
            errs.push(format!("duplicate public port {}", p.public));
        }
    }

    if ports.iter().any(|p| p.public == raw.ctrl_port) {
        errs.push(format!(
            "CTRL_PORT ({}) collides with a public port",
            raw.ctrl_port
        ));
    }

    // AUTH_HASH validation
    if let Some(ref h) = raw.auth_hash {
        let hex_ok = h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit());
        if !hex_ok {
            errs.push("TFILTER_AUTH_HASH must be 64 hex chars (SHA-256)".into());
        }
    }

    // BODY_LIMIT_BYTES
    let body_limit_bytes = raw.body_limit_bytes.unwrap_or(DEFAULT_BODY_LIMIT);
    if body_limit_bytes == 0 {
        errs.push("TFILTER_BODY_LIMIT_BYTES must be > 0".into());
    }

    if !errs.is_empty() {
        let joined = errs.join("; ");
        return Err(anyhow!(joined));
    }

    info!(
        ctrl_port = raw.ctrl_port,
        ports = ports.len(),
        debug = raw.debug,
        "configuration validated"
    );
    if raw.auth_hash.is_none() {
        warn!("AUTH disabled: TFILTER_AUTH_HASH not set");
    }

    Ok(Config {
        upstream_host: raw.upstream_host,
        ports,
        ctrl_port: raw.ctrl_port,
        debug: raw.debug,
        auth_hash: raw.auth_hash.map(|s| s.to_ascii_lowercase()),
        body_limit_bytes,
        hydrate_db: raw.hydrate_db,
        db_path: raw.db_path,
        sinks_stdout: raw.sinks_stdout.unwrap_or(true),
        sinks_emit: raw.sinks_emit.unwrap_or(true),
    })
}

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------
fn parse_listen_map(s: &str) -> Result<Vec<PortBinding>> {
    if s.trim().is_empty() {
        return Err(anyhow!("TFILTER_LISTEN_MAP is empty"));
    }
    let mut out = Vec::new();
    for pair in s.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let (a, b) = pair
            .split_once(':')
            .ok_or_else(|| anyhow!("invalid pair '{}'", pair))?;
        let public = u16::from_str(a.trim())
            .with_context(|| format!("invalid public port '{}'", a))?;
        let upstream = u16::from_str(b.trim())
            .with_context(|| format!("invalid upstream port '{}'", b))?;
        out.push(PortBinding { public, upstream });
    }
    if out.is_empty() {
        return Err(anyhow!("TFILTER_LISTEN_MAP produced no valid bindings"));
    }
    Ok(out)
}
