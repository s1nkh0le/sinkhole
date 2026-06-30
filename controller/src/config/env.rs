use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use shared::utils::hashes::sha256_hex;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::time::Duration;
use tracing::{info, warn};

/// Per-controller configuration (global).
#[derive(Debug, Clone)]
pub struct ControllerCfg {
    pub debug: bool,
    /// Address the controller HTTP server binds to (e.g. "0.0.0.0:7000").
    pub bind: SocketAddr,
    /// Path to the controller cache DB.
    pub db_path: String,
    /// Allowed CORS origin(s) for a *separate* browser UI, comma-separated. `None`
    /// means permissive (`Any`) in debug, or same-origin-only when serving the
    /// bundled UI; in non-debug without the bundled UI it is required (see `load`).
    pub cors_origin: Option<String>,
    /// Background sync cadence for service health.
    pub sync_health_interval: Duration,
    /// Background sync cadence for ECAM events.
    pub sync_ecam_interval: Duration,
    /// Background sync cadence for evaluation records.
    pub sync_records_interval: Duration,
    /// Background sync cadence for ports/config/ruleset snapshots.
    pub sync_config_interval: Duration,
    /// max in-flight port fetches per service
    pub sync_concurrency: usize,
    /// how often the pruner runs
    pub prune_interval: Duration,
    /// Grace window of already-centralized data kept on the *filter* DBs on top of
    /// the confirmed cursor. 0 = prune up to the cursor (lean staging).
    pub retention_filter: Duration,
    /// retention applied to the *central* controller DB. 0 = keep forever.
    pub retention_db: Duration,
    /// control token hash
    pub token_hash: Option<String>,
    /// Hydrate the tracked service fleet from the DB on boot, and persist
    /// runtime fleet changes to it. Opt-out (default `true`). When off, the
    /// `services` table is dormant and the fleet is env-only (ephemeral adds).
    pub db_hydrate: bool,
    /// Serve the web UI (embedded in the binary at build time) under `/ui`, same
    /// origin as the API so the browser needs no CORS. Default `true`; set to
    /// `false` to host the frontend separately (e.g. via Vite) and set `CORS_ORIGIN`.
    pub serve_frontend: bool,
}

/// One upstream filter "service" the controller talks to.
///
/// Identity/connection only — the display **name** is not stored here; it lives in
/// the per-service metadata (`metadata.name`), seeded from the config `name` at
/// creation. See [`Config::service_names`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceCfg {
    /// Stable identifier (must be unique). Used in API paths and DB keys.
    pub id: String,
    /// Base URL of the filter (no trailing slash; e.g. "http://127.0.0.1:7001").
    pub base_url: String,
    /// Bearer/token to authenticate against the filter.
    pub token: String,
}

/// Full configuration loaded at startup.
#[derive(Debug, Clone)]
pub struct Config {
    pub controller: ControllerCfg,
    pub services: Vec<ServiceCfg>,
    /// Optional display names supplied with the services (id -> name), used once
    /// at boot to seed each service's `metadata.name` when it has none.
    pub service_names: HashMap<String, String>,
}

impl Config {
    pub fn service_ids(&self) -> Vec<String> {
        self.services.iter().map(|s| s.id.clone()).collect()
    }
}

const ENV_PREFIX: &str = "SINKHOLE_CONTROLLER_";

#[derive(Debug, Deserialize)]
struct RawCtrl {
    /// SINKHOLE_CONTROLLER_DEBUG (bool)
    #[serde(default)]
    debug: bool,

    /// SINKHOLE_CONTROLLER_BIND (e.g. "0.0.0.0:7000")
    #[serde(default)]
    bind: Option<String>,

    /// SINKHOLE_CONTROLLER_DB_PATH (e.g. "ctrl.db")
    #[serde(default)]
    db_path: Option<String>,

    /// SINKHOLE_CONTROLLER_CORS_ORIGIN (comma-separated origins)
    #[serde(default)]
    cors_origin: Option<String>,

    /// SINKHOLE_CONTROLLER_DB_HYDRATE (bool) — defaults to `true` (opt-out).
    #[serde(default)]
    db_hydrate: Option<bool>,

    /// SINKHOLE_CONTROLLER_SERVE_FRONTEND (bool) — serve the embedded web UI under
    /// `/ui`. Defaults to `true`.
    #[serde(default)]
    serve_frontend: Option<bool>,

    /// SINKHOLE_CONTROLLER_SYNC_HEALTH_INTERVAL_MS (u64, milliseconds)
    #[serde(default)]
    sync_health_interval_ms: Option<u64>,

    /// SINKHOLE_CONTROLLER_SYNC_ECAM_INTERVAL_MS (u64, milliseconds)
    #[serde(default)]
    sync_ecam_interval_ms: Option<u64>,

    /// SINKHOLE_CONTROLLER_SYNC_RECORDS_INTERVAL_MS (u64, milliseconds)
    #[serde(default)]
    sync_records_interval_ms: Option<u64>,

    /// SINKHOLE_CONTROLLER_SYNC_CONFIG_INTERVAL_MS (u64, milliseconds)
    #[serde(default)]
    sync_config_interval_ms: Option<u64>,

    /// SINKHOLE_CONTROLLER_SYNC_CONCURRENCY (usize)
    #[serde(default)]
    sync_concurrency: Option<usize>,

    /// SINKHOLE_CONTROLLER_PRUNE_INTERVAL_SECS (u64)
    #[serde(default)]
    prune_interval_secs: Option<u64>,

    /// SINKHOLE_CONTROLLER_RETENTION_FILTER_SECS (u64); grace window of already-
    /// centralized data kept on filters. 0 = prune up to the confirmed cursor.
    #[serde(default)]
    retention_filter_secs: Option<u64>,

    /// SINKHOLE_CONTROLLER_RETENTION_DB_SECS (u64); 0 keeps central DB forever
    #[serde(default)]
    retention_db_secs: Option<u64>,

    /// SINKHOLE_CONTROLLER_AUTH_TOKEN_HASH (sha256 hex) – takes precedence
    #[serde(default)]
    auth_token_hash: Option<String>,

    /// SINKHOLE_CONTROLLER_AUTH_TOKEN (plain) – hashed if hash not provided
    #[serde(default)]
    auth_token: Option<String>,
}

const DEFAULT_BINDING: &str = "0.0.0.0:7000";
const DEFAULT_DB_PATH: &str = "sinkhole-data/sinkhole-ctrl.db";

pub fn load() -> Result<Config> {
    // 1) Parse globals with envy
    let raw: RawCtrl = envy::prefixed(ENV_PREFIX)
        .from_env()
        .context("Failed to parse controller environment variables")?;

    // 2) Normalize globals
    let bind_str = raw.bind.unwrap_or_else(|| DEFAULT_BINDING.to_string());
    let bind: SocketAddr = bind_str
        .parse()
        .with_context(|| format!("invalid {}BIND='{}'", ENV_PREFIX, bind_str))?;

    let db_path = raw.db_path.unwrap_or_else(|| DEFAULT_DB_PATH.to_string());
    shared::utils::fs::ensure_parent_dir(&db_path).with_context(|| {
        format!(
            "could not create parent dir for {}DB_PATH='{}'",
            ENV_PREFIX, db_path
        )
    })?;
    // Sync cadences are configured in milliseconds (defaults preserve the prior
    // 2s/5s/3s/5s behaviour). Records defaults to 3s — snappy enough for a live
    // UI without hammering filters; lower it for near-real-time centralization.
    let sync_health_interval = Duration::from_millis(raw.sync_health_interval_ms.unwrap_or(2_000));
    let sync_ecam_interval = Duration::from_millis(raw.sync_ecam_interval_ms.unwrap_or(5_000));
    let sync_records_interval =
        Duration::from_millis(raw.sync_records_interval_ms.unwrap_or(3_000));
    let sync_config_interval = Duration::from_millis(raw.sync_config_interval_ms.unwrap_or(5_000));
    let sync_concurrency = raw.sync_concurrency.unwrap_or(4).max(1);
    let prune_interval = Duration::from_secs(raw.prune_interval_secs.unwrap_or(600));
    // Filter DBs are ephemeral staging: the controller always prunes them down to
    // what it has confirmed-centralized, keeping only this grace window of already-
    // centralized data on top. Default 10 min; 0 is safe (prune up to the confirmed
    // cursor) thanks to confirmed writes. The cursor bound means un-centralized
    // records are never pruned regardless of this value.
    let retention_filter = Duration::from_secs(raw.retention_filter_secs.unwrap_or(600));
    let retention_db = Duration::from_secs(raw.retention_db_secs.unwrap_or(0));

    let token_hash = match (raw.auth_token_hash, raw.auth_token) {
        (Some(h), _) => Some(h),
        (None, Some(tok)) => Some(sha256_hex(tok.as_bytes())),
        (None, None) => None,
    };

    // Fail closed: in non-debug mode an auth token MUST be configured.
    if token_hash.is_none() {
        if raw.debug {
            warn!(
                "controller auth is DISABLED ({}AUTH_TOKEN[_HASH] not set; allowed because DEBUG=true)",
                ENV_PREFIX
            );
        } else {
            bail!(
                "{ENV_PREFIX}AUTH_TOKEN[_HASH] is required in non-debug mode (refusing to start with auth disabled)"
            );
        }
    }

    // The bundled web UI is served same-origin (see SERVE_FRONTEND), so when it is
    // enabled CORS does not apply to it and no origin needs to be configured.
    let serve_frontend = raw.serve_frontend.unwrap_or(true);

    // CORS: a permissive (`Any`) policy is only acceptable in debug. In non-debug an
    // explicit origin must be configured, otherwise refuse to start — UNLESS we serve
    // the bundled UI, which is same-origin (cross-origin is then simply denied, see
    // build_cors_layer_same_origin_only). So the requirement only bites when a
    // separate, cross-origin frontend is the only browser client.
    let cors_origin = raw
        .cors_origin
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if cors_origin.is_none() && !raw.debug && !serve_frontend {
        bail!(
            "{ENV_PREFIX}CORS_ORIGIN is required in non-debug mode when not serving the bundled UI \
             (set {ENV_PREFIX}SERVE_FRONTEND=true to serve the same-origin UI, or DEBUG=true for permissive CORS)"
        );
    }

    let controller = ControllerCfg {
        debug: raw.debug,
        bind,
        db_path,
        cors_origin,
        sync_health_interval,
        sync_ecam_interval,
        sync_records_interval,
        sync_config_interval,
        sync_concurrency,
        prune_interval,
        retention_filter,
        retention_db,
        token_hash,
        db_hydrate: raw.db_hydrate.unwrap_or(true),
        serve_frontend,
    };

    // 3) Parse services (JSON array in SINKHOLE_CONTROLLER_SERVICES)
    let (services, service_names) = load_services_from_env()?;
    if services.is_empty() {
        // Not fatal: a controller may start with no services and have them added
        // later via the supervisor API (`PUT /services/new`).
        warn!("[load()]: No services provided via env");
    }

    // 4) Validate services
    validate_services(&services)?;

    info!(
        services = services.len(),
        bind = %controller.bind,
        "controller config loaded"
    );

    Ok(Config {
        controller,
        services,
        service_names,
    })
}

// ---------------------------------------------------------------------
// Services parsing
// ---------------------------------------------------------------------

/// One service entry as written in the `SINKHOLE_CONTROLLER_SERVICES` JSON array.
#[derive(Debug, Deserialize)]
struct RawServiceJson {
    /// Stable, mandatory identifier.
    id: String,
    /// Filter control base URL.
    url: String,
    /// Bearer token for that filter.
    token: String,
    /// Optional display name (seeds `metadata.name`).
    #[serde(default)]
    name: Option<String>,
}

/// Parse the services from `SINKHOLE_CONTROLLER_SERVICES`, a JSON array, e.g.:
///
/// ```json
/// [
///   {"id":"edge","url":"http://10.0.0.5:7001","token":"…","name":"Edge filter"},
///   {"id":"api","url":"http://10.0.0.6:7001","token":"…"}
/// ]
/// ```
///
/// Returns the connection configs plus a map `id -> name` for metadata seeding.
fn load_services_from_env() -> Result<(Vec<ServiceCfg>, HashMap<String, String>)> {
    let raw = match std::env::var(format!("{ENV_PREFIX}SERVICES")) {
        Ok(s) if !s.trim().is_empty() => s,
        _ => return Ok((Vec::new(), HashMap::new())),
    };

    let parsed: Vec<RawServiceJson> = serde_json::from_str(&raw).with_context(|| {
        format!("{ENV_PREFIX}SERVICES must be a JSON array of {{id, url, token, name?}}")
    })?;

    let mut services = Vec::with_capacity(parsed.len());
    let mut names = HashMap::new();
    for s in parsed {
        if let Some(name) = s.name.and_then(|n| {
            let t = n.trim().to_string();
            (!t.is_empty()).then_some(t)
        }) {
            names.insert(s.id.clone(), name);
        }
        services.push(ServiceCfg {
            id: s.id,
            base_url: normalize_base_url(&s.url),
            token: s.token,
        });
    }
    Ok((services, names))
}

fn normalize_base_url(u: &str) -> String {
    let mut s = u.trim().to_string();
    while s.ends_with('/') {
        s.pop();
    }
    s
}

fn validate_services(services: &[ServiceCfg]) -> Result<()> {
    let mut seen = HashSet::new();
    for s in services {
        if s.id.trim().is_empty() {
            bail!(
                "service id must not be empty (service with base_url={})",
                s.base_url
            );
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
    Ok(())
}
