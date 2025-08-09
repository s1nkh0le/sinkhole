use crate::observability::sinks::sink_ecam;
use crate::observability::sinks::sink_ecam::{Origin, Severity};
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt, net::SocketAddr, str::FromStr};
use tracing::{debug, info, trace, warn};

const ENV_PREFIX: &str = "SINKHOLE_FILTER_";
const DEFAULT_CTRL_BIND: &str = "0.0.0.0:7001";
const DEFAULT_BODY_LIMIT: usize = 1_048_576; // 1MB
const DEFAULT_DB_PATH: &str = "sinkhole.db";

#[derive(Clone, Debug, Serialize)]
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
    /// Required: e.g. "127.0.0.1"
    pub upstream_host: String,

    /// Comma-separated "public:upstream" pairs. Example: "8080:80,8443:8000"
    #[serde(default)]
    pub listen_map: String,

    /// Preferred: full bind address for the filter control API (e.g. "0.0.0.0:7001")
    /// ENV: SINKHOLE_FILTER_CTRL_BIND
    #[serde(default)]
    pub ctrl_bind: Option<SocketAddr>,

    /// Legacy fallback: just the port (we bind to 0.0.0.0)
    /// ENV: SINKHOLE_FILTER_CTRL_PORT
    #[serde(default)]
    pub ctrl_port: Option<u16>,

    /// ENV: SINKHOLE_FILTER_DEBUG
    #[serde(default)]
    pub debug: bool,

    /// ENV: SINKHOLE_FILTER_AUTH_HASH (hex sha256; disables auth if not set)
    #[serde(default)]
    pub auth_hash: Option<String>,

    /// ENV: SINKHOLE_FILTER_BODY_LIMIT_BYTES
    #[serde(default)]
    pub body_limit_bytes: Option<usize>,

    /// ENV: SINKHOLE_FILTER_HYDRATE_DB
    #[serde(default)]
    pub hydrate_db: bool,

    /// ENV: SINKHOLE_FILTER_DB_PATH
    #[serde(default)]
    pub db_path: Option<String>,

    /// ENV: SINKHOLE_FILTER_SINKS_STDOUT
    #[serde(default)]
    pub sinks_stdout: Option<bool>,

    /// ENV: SINKHOLE_FILTER_SINKS_EMIT
    #[serde(default)]
    pub sinks_emit: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Config {
    pub upstream_host: String,
    pub ports: Vec<PortBinding>,
    pub ctrl_bind: SocketAddr,
    pub debug: bool,
    pub auth_hash: Option<String>,
    pub body_limit_bytes: usize,
    pub hydrate_db: bool,
    pub db_path: String,
    pub sinks_stdout: bool,
    pub sinks_emit: bool,
}

impl Config {
    #[inline]
    pub fn ctrl_bind_addr(&self) -> SocketAddr {
        self.ctrl_bind
    }
}

pub fn load() -> Result<Config> {
    // 1) Load raw env
    let raw: RawConfig = envy::prefixed(ENV_PREFIX)
        .from_env()
        .context("failed to parse filter environment")?;
    trace!(?raw, "raw filter config loaded");

    // 2) Normalize bind
    let ctrl_bind = match (raw.ctrl_bind, raw.ctrl_port) {
        (Some(bind), _) => bind,
        (None, Some(port)) => SocketAddr::from(([0, 0, 0, 0], port)),
        (None, None) => SocketAddr::from_str(DEFAULT_CTRL_BIND)
            .expect("DEFAULT_CTRL_BIND must parse"),
    };

    // 3) Parse listen_map
    let ports = parse_listen_map(&raw.listen_map)
        .with_context(|| format!("{}LISTEN_MAP invalid", ENV_PREFIX))?;

    // 4) Validate + accumulate errors
    let mut errs = Vec::new();

    let upstream_host = raw.upstream_host.trim().to_string();
    if upstream_host.is_empty() {
        errs.push(format!("{}UPSTREAM_HOST is empty", ENV_PREFIX));
    }

    // Ports sanity
    if ports.is_empty() {
        errs.push(format!("{}LISTEN_MAP is empty", ENV_PREFIX));
    }

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

    // Collide with control bind?
    let ctrl_port = ctrl_bind.port();
    if ports.iter().any(|p| p.public == ctrl_port) {
        errs.push(format!(
            "{}CTRL_BIND ({}) collides with a public port",
            ENV_PREFIX, ctrl_bind
        ));
    }

    // AUTH_HASH validity (optional)
    if let Some(ref h) = raw.auth_hash {
        let ok = h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit());
        if !ok {
            errs.push(format!(
                "{}AUTH_HASH must be 64 hex chars (sha256)",
                ENV_PREFIX
            ));
        }
    }

    // Body limit
    let body_limit_bytes = raw.body_limit_bytes.unwrap_or(DEFAULT_BODY_LIMIT);
    if body_limit_bytes == 0 {
        errs.push(format!("{}BODY_LIMIT_BYTES must be > 0", ENV_PREFIX));
    }

    // 5) Emit + fail if any errors
    if !errs.is_empty() {
        let joined = errs.join("; ");
        sink_ecam::record(
            Severity::Error,
            Origin::Configuration,
            format!("configuration errors: {}", joined),
            None,
        );
        return Err(anyhow!(joined));
    }

    // 6) Warnings / info
    if raw.auth_hash.is_none() {
        sink_ecam::record(
            Severity::Warn,
            Origin::Configuration,
            format!("AUTH disabled: {}AUTH_HASH is not set", ENV_PREFIX),
            None,
        );
    }
    if raw.ctrl_port.is_some() && raw.ctrl_bind.is_none() {
        warn!(
            "Using legacy {}CTRL_PORT={}; prefer {}CTRL_BIND=\"0.0.0.0:PORT\"",
            ENV_PREFIX, ctrl_port, ENV_PREFIX
        );
    }

    info!(
        ctrl_bind = %ctrl_bind,
        ports = ports.len(),
        debug = raw.debug,
        "filter configuration validated"
    );

    // 7) Final config
    Ok(Config {
        upstream_host,
        ports,
        ctrl_bind,
        debug: raw.debug,
        auth_hash: raw.auth_hash.map(|s| s.to_ascii_lowercase()),
        body_limit_bytes,
        hydrate_db: raw.hydrate_db,
        db_path: raw.db_path.unwrap_or_else(|| DEFAULT_DB_PATH.to_string()),
        sinks_stdout: raw.sinks_stdout.unwrap_or(true),
        sinks_emit: raw.sinks_emit.unwrap_or(true),
    })
}

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------
fn parse_listen_map(s: &str) -> Result<Vec<PortBinding>> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(Vec::new());
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
        let public = a
            .trim()
            .parse::<u16>()
            .with_context(|| format!("invalid public port '{}'", a))?;
        let upstream = b
            .trim()
            .parse::<u16>()
            .with_context(|| format!("invalid upstream port '{}'", b))?;
        out.push(PortBinding { public, upstream });
    }
    Ok(out)
}
