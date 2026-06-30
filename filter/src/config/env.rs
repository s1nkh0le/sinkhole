use crate::observability::sinks::sink_ecam;
use crate::observability::sinks::sink_ecam::{Origin, Severity};
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use shared::utils::hashes::sha256_hex;
use std::{collections::HashSet, fmt, net::SocketAddr, str::FromStr};
use tracing::{info, trace};

const ENV_PREFIX: &str = "SINKHOLE_FILTER_";
const DEFAULT_CONTROL_BIND: &str = "0.0.0.0:7001";
const DEFAULT_BODY_LIMIT: usize = 1_048_576; // 1MB
const DEFAULT_IDLE_FLUSH_MS: u64 = 50; // raw/non-HTTP read-idle flush window
const DEFAULT_DB_PATH: &str = "sinkhole-data/sinkhole-filter.db";

#[derive(Clone, Debug, Serialize)]
pub struct PortBinding {
    pub public: u16,
    pub upstream: u16,
    /// Per-port upstream host override. `None` = route this port to the global
    /// `UPSTREAM_HOST`; `Some(h)` = route it to `h` instead, so **one** filter can
    /// guard several upstream services on one box
    /// (`LISTEN_MAP="8080:web:80,1338:flightman:1338"`). Not serialized: internal
    /// topology, like `tls`.
    #[serde(skip)]
    pub upstream_host: Option<String>,
    /// TLS material for this port. `Some` = TLS service end-to-end: terminate the
    /// client handshake with this cert/key **and** re-encrypt to the (TLS)
    /// upstream. `None` = plain TCP both ways. Not serialized: file paths are not
    /// part of the public port view (see `tls_enabled`).
    #[serde(skip)]
    pub tls: Option<TlsFiles>,
}
impl PortBinding {
    /// Whether this port is a TLS service (terminate + re-encrypt).
    pub fn tls_enabled(&self) -> bool {
        self.tls.is_some()
    }
}

/// Paths to a PEM certificate chain and private key for TLS termination.
#[derive(Clone, Debug, Serialize)]
pub struct TlsFiles {
    pub cert_path: String,
    pub key_path: String,
}
impl fmt::Display for PortBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.upstream_host {
            Some(h) => write!(f, "{}->{}:{}", self.public, h, self.upstream),
            None => write!(f, "{}->{}", self.public, self.upstream),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct RawConfig {
    /// Default upstream host, e.g. "127.0.0.1". May be absent only if every
    /// `LISTEN_MAP` entry carries its own `public:host:upstream` (validated below).
    #[serde(default)]
    pub upstream_host: String,

    /// Comma-separated listen entries. Each is either `public:upstream` (routes
    /// to the global `UPSTREAM_HOST`) or `public:host:upstream` (routes that port
    /// to its own upstream host, so one filter can guard several services).
    /// Example: "8080:80,8443:8000" or "8080:web:80,1338:flightman:1338".
    #[serde(default)]
    pub listen_map: String,

    /// Optional per-port TLS config. Comma-separated
    /// "public:cert_path:key_path" entries. The public port must also appear in
    /// `listen_map`. Presence marks the port a TLS service end-to-end: the filter
    /// terminates client TLS with this cert/key and re-encrypts to the (TLS)
    /// upstream. Example:
    /// "8443:/certs/site.pem:/certs/site.key"
    /// ENV: SINKHOLE_FILTER_TLS_MAP
    #[serde(default)]
    pub tls_map: String,

    /// Full bind address for the filter control API (e.g. "0.0.0.0:7001").
    /// ENV: SINKHOLE_FILTER_CONTROL_BIND
    #[serde(default)]
    pub control_bind: Option<SocketAddr>,

    /// ENV: SINKHOLE_FILTER_DEBUG
    #[serde(default)]
    pub debug: bool,

    /// Control-API token, precomputed sha256 hex. Takes precedence over the
    /// plaintext form. Disables auth if neither is set.
    /// ENV: SINKHOLE_FILTER_AUTH_TOKEN_HASH
    #[serde(default)]
    pub auth_token_hash: Option<String>,

    /// Control-API token in plaintext, hashed at load if no hash is provided.
    /// ENV: SINKHOLE_FILTER_AUTH_TOKEN
    #[serde(default)]
    pub auth_token: Option<String>,

    /// ENV: SINKHOLE_FILTER_BODY_LIMIT_BYTES
    #[serde(default)]
    pub body_limit_bytes: Option<usize>,

    /// Read-idle flush window in milliseconds for unframed (non-HTTP) traffic.
    /// When the proxy holds un-forwarded bytes that aren't a complete HTTP message
    /// and no new bytes arrive within this window, it flushes them as a raw stream
    /// chunk — so interactive/line-based protocols aren't buffered until EOF or the
    /// body limit. `0` disables the idle flush (pure HTTP/bulk behavior).
    /// ENV: SINKHOLE_FILTER_IDLE_FLUSH_MS — defaults to 50.
    #[serde(default)]
    pub idle_flush_ms: Option<u64>,

    /// ENV: SINKHOLE_FILTER_CONN_IDLE_TIMEOUT_SECS — close a connection that has
    /// seen no bytes in either direction for this long. Default 120; `0` disables it.
    #[serde(default)]
    pub conn_idle_timeout_secs: Option<u64>,

    /// ENV: SINKHOLE_FILTER_CONN_MAX_LIFETIME_SECS — close a connection once it has
    /// been open this long, regardless of activity. Default 600; `0` disables it.
    #[serde(default)]
    pub conn_max_lifetime_secs: Option<u64>,

    /// ENV: SINKHOLE_FILTER_MAX_CONNECTIONS — cap on simultaneous open connections
    /// across the whole filter; new ones beyond it are dropped. Default 10000;
    /// `0` disables the cap.
    #[serde(default)]
    pub max_connections: Option<usize>,

    /// ENV: SINKHOLE_FILTER_DB_HYDRATE — defaults to `true` (opt-out). Restoring
    /// the last ruleset on boot is core to the no-interruption design.
    #[serde(default)]
    pub db_hydrate: Option<bool>,

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
    pub control_bind: SocketAddr,
    pub debug: bool,
    /// sha256(token) hex. Never serialized (the boot config log would otherwise
    /// leak it into stdout/aggregation); auth status is logged separately.
    #[serde(skip_serializing)]
    pub token_hash: Option<String>,
    pub body_limit_bytes: usize,
    pub idle_flush_ms: u64,
    /// Close a connection idle (no bytes either way) for this many seconds. 0 = off.
    pub conn_idle_timeout_secs: u64,
    /// Close a connection once it has lived this many seconds. 0 = off.
    pub conn_max_lifetime_secs: u64,
    /// Max simultaneous connections across the filter; new ones dropped. 0 = off.
    pub max_connections: usize,
    pub db_hydrate: bool,
    pub db_path: String,
    pub sinks_stdout: bool,
    pub sinks_emit: bool,
}

impl Config {
    #[inline]
    pub fn control_bind_addr(&self) -> SocketAddr {
        self.control_bind
    }
}

pub fn load() -> Result<Config> {
    // 1) Load raw env
    let raw: RawConfig = envy::prefixed(ENV_PREFIX)
        .from_env()
        .context("failed to parse filter environment")?;
    trace!(?raw, "raw filter config loaded");

    // 2) Normalize bind
    let control_bind = raw.control_bind.unwrap_or_else(|| {
        SocketAddr::from_str(DEFAULT_CONTROL_BIND).expect("DEFAULT_CONTROL_BIND must parse")
    });

    // 3) Parse listen_map
    let mut ports = parse_listen_map(&raw.listen_map)
        .with_context(|| format!("{}LISTEN_MAP invalid", ENV_PREFIX))?;

    // 3b) Parse TLS map and attach cert/key to the matching ports.
    let tls_map =
        parse_tls_map(&raw.tls_map).with_context(|| format!("{}TLS_MAP invalid", ENV_PREFIX))?;

    // 4) Validate + accumulate errors
    let mut errs = Vec::new();

    let upstream_host = raw.upstream_host.trim().to_string();
    // The global UPSTREAM_HOST may be empty only if every listen port carries its
    // own per-port host (`public:host:upstream`); otherwise the host-less ports
    // would have nowhere to forward.
    if upstream_host.is_empty() {
        let hostless: Vec<u16> = ports
            .iter()
            .filter(|p| p.upstream_host.is_none())
            .map(|p| p.public)
            .collect();
        if !hostless.is_empty() {
            errs.push(format!(
                "{}UPSTREAM_HOST is empty and these ports have no per-port host: {:?}",
                ENV_PREFIX, hostless
            ));
        }
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
    let control_port = control_bind.port();
    if ports.iter().any(|p| p.public == control_port) {
        errs.push(format!(
            "{}CONTROL_BIND ({}) collides with a public port",
            ENV_PREFIX, control_bind
        ));
    }

    // API token: a precomputed hash takes precedence; otherwise hash the
    // plaintext token. Either way the result must be a valid sha256 hex string.
    let token_hash = match (raw.auth_token_hash.as_ref(), raw.auth_token.as_ref()) {
        (Some(h), _) => Some(h.trim().to_ascii_lowercase()),
        (None, Some(tok)) => Some(sha256_hex(tok.as_bytes())),
        (None, None) => None,
    };
    if let Some(ref h) = token_hash {
        let ok = h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit());
        if !ok {
            errs.push(format!(
                "{}AUTH_TOKEN_HASH must be 64 hex chars (sha256)",
                ENV_PREFIX
            ));
        }
    }

    // Body limit
    let body_limit_bytes = raw.body_limit_bytes.unwrap_or(DEFAULT_BODY_LIMIT);
    if body_limit_bytes == 0 {
        errs.push(format!("{}BODY_LIMIT_BYTES must be > 0", ENV_PREFIX));
    }

    // Idle-flush window (0 = disabled).
    let idle_flush_ms = raw.idle_flush_ms.unwrap_or(DEFAULT_IDLE_FLUSH_MS);

    // TLS: each entry must reference a known public port and readable files.
    let known_ports: HashSet<u16> = ports.iter().map(|p| p.public).collect();
    for (public, files) in &tls_map {
        if !known_ports.contains(public) {
            errs.push(format!(
                "{}TLS_MAP references public port {} not present in LISTEN_MAP",
                ENV_PREFIX, public
            ));
        }
        if !std::path::Path::new(&files.cert_path).is_file() {
            errs.push(format!(
                "{}TLS_MAP cert file not found for port {}: '{}'",
                ENV_PREFIX, public, files.cert_path
            ));
        }
        if !std::path::Path::new(&files.key_path).is_file() {
            errs.push(format!(
                "{}TLS_MAP key file not found for port {}: '{}'",
                ENV_PREFIX, public, files.key_path
            ));
        }
    }
    if errs.is_empty() {
        for p in &mut ports {
            if let Some(files) = tls_map.get(&p.public) {
                p.tls = Some(files.clone());
            }
        }
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
    if token_hash.is_none() {
        sink_ecam::record(
            Severity::Warn,
            Origin::Configuration,
            format!("AUTH disabled: {}AUTH_TOKEN[_HASH] is not set", ENV_PREFIX),
            None,
        );
    }

    info!(
        control_bind = %control_bind,
        ports = ports.len(),
        debug = raw.debug,
        "filter configuration validated"
    );

    // 7) Final config
    // DB path: an explicit DB_PATH wins; otherwise default into the data dir.
    let db_path = raw.db_path.unwrap_or_else(|| DEFAULT_DB_PATH.to_string());
    shared::utils::fs::ensure_parent_dir(&db_path).with_context(|| {
        format!("could not create parent dir for {ENV_PREFIX}DB_PATH='{db_path}'")
    })?;

    Ok(Config {
        upstream_host,
        ports,
        control_bind,
        debug: raw.debug,
        token_hash,
        body_limit_bytes,
        idle_flush_ms,
        // Connection guards, on by default (set a value to 0 to disable that guard).
        // Idle reclaims dead/slowloris connections; lifetime bounds the worst case.
        conn_idle_timeout_secs: raw.conn_idle_timeout_secs.unwrap_or(120),
        conn_max_lifetime_secs: raw.conn_max_lifetime_secs.unwrap_or(600),
        max_connections: raw.max_connections.unwrap_or(10_000),
        // Hydration is opt-out: restoring the last ruleset on boot is essential.
        db_hydrate: raw.db_hydrate.unwrap_or(true),
        db_path,
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
    for entry in s.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        // Two forms:
        //   public:upstream            -> route to the global UPSTREAM_HOST
        //   public:host:upstream       -> route this port to `host` (per-port)
        // A per-port host is any hostname / IPv4 (no IPv6 literal — it would
        // collide with the ':' separator).
        let segs: Vec<&str> = entry.split(':').map(str::trim).collect();
        let (pub_s, host, up_s) = match segs.as_slice() {
            [a, b] => (*a, None, *b),
            [a, h, b] => {
                if h.is_empty() {
                    return Err(anyhow!("empty upstream host in '{}'", entry));
                }
                (*a, Some((*h).to_string()), *b)
            }
            _ => {
                return Err(anyhow!(
                    "invalid LISTEN_MAP entry '{}' (expected public:upstream or public:host:upstream)",
                    entry
                ));
            }
        };
        let public = pub_s
            .parse::<u16>()
            .with_context(|| format!("invalid public port '{}'", pub_s))?;
        let upstream = up_s
            .parse::<u16>()
            .with_context(|| format!("invalid upstream port '{}'", up_s))?;
        out.push(PortBinding {
            public,
            upstream,
            upstream_host: host,
            tls: None,
        });
    }
    Ok(out)
}

/// Parse `SINKHOLE_FILTER_TLS_MAP`: comma-separated `public:cert_path:key_path`
/// entries. Returns a map from public port to its [`TlsFiles`].
fn parse_tls_map(s: &str) -> Result<std::collections::HashMap<u16, TlsFiles>> {
    let s = s.trim();
    let mut out = std::collections::HashMap::new();
    if s.is_empty() {
        return Ok(out);
    }
    for entry in s.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let mut parts = entry.splitn(3, ':');
        let port = parts.next().unwrap_or("").trim();
        let cert = parts.next().map(str::trim).unwrap_or("");
        let key = parts.next().map(str::trim).unwrap_or("");
        if cert.is_empty() || key.is_empty() {
            return Err(anyhow!(
                "invalid TLS entry '{}' (expected public:cert_path:key_path)",
                entry
            ));
        }
        let public = port
            .parse::<u16>()
            .with_context(|| format!("invalid TLS public port '{}'", port))?;
        if out
            .insert(
                public,
                TlsFiles {
                    cert_path: cert.to_string(),
                    key_path: key.to_string(),
                },
            )
            .is_some()
        {
            return Err(anyhow!("duplicate TLS entry for public port {}", public));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tls_map_empty_is_ok() {
        assert!(parse_tls_map("").unwrap().is_empty());
        assert!(parse_tls_map("   ").unwrap().is_empty());
    }

    #[test]
    fn tls_map_parses_entries() {
        let map = parse_tls_map("8443:/c/site.pem:/c/site.key, 9443:/c/a.pem:/c/a.key").unwrap();
        assert_eq!(map.len(), 2);
        let f = map.get(&8443).unwrap();
        assert_eq!(f.cert_path, "/c/site.pem");
        assert_eq!(f.key_path, "/c/site.key");
    }

    #[test]
    fn tls_map_rejects_missing_key() {
        assert!(parse_tls_map("8443:/c/site.pem").is_err());
    }

    #[test]
    fn tls_map_rejects_bad_port() {
        assert!(parse_tls_map("notaport:/c/site.pem:/c/site.key").is_err());
    }

    #[test]
    fn tls_map_rejects_duplicate_port() {
        assert!(parse_tls_map("8443:/c/a.pem:/c/a.key,8443:/c/b.pem:/c/b.key").is_err());
    }

    #[test]
    fn listen_map_two_segment_uses_global_host() {
        let ports = parse_listen_map("8080:80, 8443:8000").unwrap();
        assert_eq!(ports.len(), 2);
        assert_eq!((ports[0].public, ports[0].upstream), (8080, 80));
        assert!(ports[0].upstream_host.is_none());
        assert!(ports[1].upstream_host.is_none());
    }

    #[test]
    fn listen_map_three_segment_sets_per_port_host() {
        let ports = parse_listen_map("8080:web:80, 1338:flightman:1338").unwrap();
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].upstream_host.as_deref(), Some("web"));
        assert_eq!((ports[0].public, ports[0].upstream), (8080, 80));
        assert_eq!(ports[1].upstream_host.as_deref(), Some("flightman"));
        assert_eq!(ports[1].upstream, 1338);
    }

    #[test]
    fn listen_map_mixed_forms() {
        let ports = parse_listen_map("80:81, 1338:flightman:1338").unwrap();
        assert!(ports[0].upstream_host.is_none());
        assert_eq!(ports[1].upstream_host.as_deref(), Some("flightman"));
    }

    #[test]
    fn listen_map_rejects_bad_entries() {
        assert!(parse_listen_map("8080").is_err()); // no colon
        assert!(parse_listen_map("8080::80").is_err()); // empty host
        assert!(parse_listen_map("8080:web:80:extra").is_err()); // too many segments
        assert!(parse_listen_map("nope:web:80").is_err()); // bad public port
        assert!(parse_listen_map("8080:web:nope").is_err()); // bad upstream port
    }
}
