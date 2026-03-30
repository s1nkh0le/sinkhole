
# SINKHOLE

A high-performance Web Application Firewall (WAF) for Attack/Defense CTF competitions. Sits between attackers and your challenge services, intercepts traffic, evaluates rules in real-time, and drops malicious packets before they hit your services.

```
Attacker ──► [:8080] SINKHOLE FILTER [:80] ──► Challenge Service
                      │
                      ▼
              Rule Engine (markers → filters)
              ├── Blacklist   → DROP
              ├── Whitelist ALL → must pass every filter
              └── Whitelist ANY → must pass at least one
```

## Architecture

```
┌────────────────────────┐       ┌────────────────────────┐
│     CONTROLLER         │       │      FILTER (×N)       │
│  (aggregation hub)     │◄─────►│  (per-service proxy)   │
│  - Web UI backend      │ HTTP  │  - TCP proxy per port   │
│  - ECAM aggregation    │       │  - Rule engine          │
│  - Log collection      │       │  - TLS termination      │
│  - Ruleset sync        │       │  - SQLite logging       │
└────────────────────────┘       └────────────────────────┘
         ▲
         │ HTTP
┌────────────────────────┐
│       WEB UI           │
│  (static SPA)          │
│  - Filter management   │
│  - Real-time ECAM      │
│  - Traffic charts       │
│  - Rule editor          │
└────────────────────────┘
```

**Three components:**

| Component | Language | Purpose |
|-----------|----------|---------|
| `filter/` | Rust | Bidirectional TCP proxy + rule engine + control API |
| `controller/` | Rust | Centralized management, proxies to multiple filters |
| `webui/` | HTML/JS/CSS | Browser-based management dashboard |

## Quick Start

### 1. Configure & Run the Filter

```bash
# Set env vars (or use setup_env.sh)
export SINKHOLE_FILTER_UPSTREAM_HOST=127.0.0.1
export SINKHOLE_FILTER_LISTEN_MAP="8080:80,8443:443"
export SINKHOLE_FILTER_CTRL_PORT=7001
export SINKHOLE_FILTER_DEBUG=true
export SINKHOLE_FILTER_HYDRATE_DB=true
export SINKHOLE_FILTER_DB_PATH="sinkhole.db"

# Auth (optional but recommended)
AUTH_SECRET="my_secret_token"
export SINKHOLE_FILTER_AUTH_HASH=$(printf %s "$AUTH_SECRET" | sha256sum | awk '{print $1}')

cargo run --bin filter
```

This starts proxies on ports 8080 and 8443, forwarding to 80 and 443 on the upstream host. The control API listens on port 7001.

### 2. Configure & Run the Controller

```bash
export CTRL_BIND="0.0.0.0:7000"
export CTRL_DB_PATH="ctrl.db"
export CTRL_PULL_INTERVAL_SECS=2

# Register filter(s)
export SERVICE_0_URL="http://127.0.0.1:7001"
export SERVICE_0_TOKEN="my_secret_token"
export SERVICE_0_ID="filter_1"

cargo run --bin controller
```

### 3. Launch the Web UI

```bash
./webui.sh            # default: http://localhost:8888
./webui.sh 9090       # custom port
```

Open the printed URL in your browser. Enter the controller address (e.g. `http://localhost:7000`) in the sidebar and click **Connect**.

### 4. Docker

```bash
docker-compose up -d
```

The `docker-compose.yaml` sets up a filter + a sample challenge service. Ports are configured via environment variables.

## TLS Termination

Sinkhole can terminate TLS from clients, run the rule engine on decrypted plaintext, and optionally re-encrypt to the upstream service. This is essential when CTF challenges serve over HTTPS with self-signed certificates.

### How It Works

```
Client ──TLS──► SINKHOLE ──decrypt──► Rule Engine ──assess──► 
  └── re-encrypt (optional) ──TLS──► Challenge Service
```

1. Client connects with TLS to the filter's public port
2. Filter terminates TLS using the provided certificate + private key
3. Decrypted traffic flows through the rule engine (markers → filters)
4. If `TLS_UPSTREAM=true`, traffic is re-encrypted before forwarding to the challenge

### Configuration

```bash
# Enable TLS termination
export SINKHOLE_FILTER_TLS_ENABLED=true
export SINKHOLE_FILTER_TLS_CERT_PATH="/path/to/cert.pem"
export SINKHOLE_FILTER_TLS_KEY_PATH="/path/to/key.pem"

# Re-encrypt to upstream (when the challenge expects TLS)
export SINKHOLE_FILTER_TLS_UPSTREAM=true

# Skip upstream cert verification (default: true, useful for self-signed CTF certs)
export SINKHOLE_FILTER_TLS_SKIP_VERIFY=true
```

### Example: HTTPS Challenge

```bash
# Generate self-signed cert for testing
openssl req -x509 -newkey rsa:2048 -keyout key.pem -out cert.pem \
  -days 365 -nodes -subj '/CN=challenge.local'

# Run filter with TLS
export SINKHOLE_FILTER_TLS_ENABLED=true
export SINKHOLE_FILTER_TLS_CERT_PATH="cert.pem"
export SINKHOLE_FILTER_TLS_KEY_PATH="key.pem"
export SINKHOLE_FILTER_TLS_UPSTREAM=false   # challenge runs plain HTTP internally
export SINKHOLE_FILTER_UPSTREAM_HOST=127.0.0.1
export SINKHOLE_FILTER_LISTEN_MAP="8443:80"

cargo run --bin filter
# Now clients connect to https://your-ip:8443, traffic is decrypted, filtered, forwarded to :80
```

## Rule Engine

Rules are evaluated in two phases per TCP message:

### Phase 1: Markers

Markers are pattern matchers that tag traffic. They don't block anything directly — they mark connections for use by filters.

```json
{
  "id": "sql_injection_marker",
  "scope": "http.body",
  "direction": "inbound",
  "matcher": {
    "type": "body_regex",
    "pattern": "(?i)(union\\s+select|drop\\s+table|;\\s*--)",
    "case_sensitive": false
  }
}
```

### Phase 2: Filters

Filters make block/allow decisions. They can depend on markers via gating.

**Blacklist** — drop traffic matching the pattern:
```json
{
  "id": "block_sqli",
  "scope": "http.body",
  "direction": "inbound",
  "mode": "blacklist",
  "matcher": { "type": "body_regex", "pattern": "union\\s+select", "case_sensitive": false },
  "needed_markers_all": [],
  "needed_markers_any": []
}
```

**Whitelist ALL** — drop traffic that doesn't match ALL required filters:
```json
{
  "id": "require_valid_ua",
  "scope": "http.header",
  "direction": "inbound",
  "mode": "whitelist_all",
  "matcher": { "type": "header_regex", "header_name": "user-agent", "pattern": "^Mozilla", "case_sensitive": false },
  "needed_markers_all": [],
  "needed_markers_any": []
}
```

**Whitelist ANY** — drop traffic that doesn't match at least one filter in the group:
```json
{
  "id": "allow_known_paths",
  "scope": "http.path",
  "direction": "inbound",
  "mode": "whitelist_any",
  "matcher": { "type": "path_regex", "pattern": "^/(api|static|health)", "case_sensitive": true },
  "needed_markers_all": [],
  "needed_markers_any": []
}
```

### Matcher Types

| Matcher | Scope Required | Description |
|---------|---------------|-------------|
| `stream_regex` | `stream` | Regex on raw TCP bytes |
| `path_regex` | `http.path` | Regex on HTTP request path |
| `body_regex` | `http.body` | Regex on HTTP body |
| `header_regex` | `http.header` | Regex on a specific HTTP header |
| `method_regex` | `http.method` | Regex on HTTP method (GET, POST...) |
| `status_regex` | `http.status` | Regex on HTTP response status (outbound only) |
| `size` | `meta`, `stream`, `http.*` | Compare payload size (`>`, `>=`, `<`, `<=`, `==`, `!=`) |
| `session.lifetime` | `timing` | Connection age in seconds |
| `session.idle` | `timing` | Time since last message in seconds |
| `always` | any | Always matches (or never, if `value: false`) |

### Full Ruleset Example

```bash
curl -X PUT "http://localhost:7001/ports/8080/ruleset?token=my_secret_token" \
  -H "Content-Type: application/json" \
  -d '{
    "comment": "CTF round 1 defense",
    "markers": [
      {
        "id": "long_session",
        "scope": "timing",
        "direction": "both",
        "matcher": { "type": "session.lifetime", "op": ">", "value": 300 }
      }
    ],
    "filters": [
      {
        "id": "block_sqli",
        "scope": "http.body",
        "direction": "inbound",
        "mode": "blacklist",
        "matcher": {
          "type": "body_regex",
          "pattern": "(?i)(union\\s+select|;\\s*--|drop\\s+table)",
          "case_sensitive": false
        },
        "needed_markers_all": [],
        "needed_markers_any": []
      },
      {
        "id": "block_path_traversal",
        "scope": "http.path",
        "direction": "inbound",
        "mode": "blacklist",
        "matcher": {
          "type": "path_regex",
          "pattern": "\\.\\.[\\/]",
          "case_sensitive": true
        },
        "needed_markers_all": [],
        "needed_markers_any": []
      },
      {
        "id": "kill_long_sessions",
        "scope": "timing",
        "direction": "both",
        "mode": "blacklist",
        "matcher": { "type": "always", "value": true },
        "needed_markers_all": ["long_session"],
        "needed_markers_any": []
      }
    ]
  }'
```

## Web UI

The web UI is a static single-page application that talks to the **controller** API. No build step needed — just serve the `webui/` directory with any HTTP server.

### Features

- **Dashboard** — overview of all connected filters with health status, port counts, and live ECAM event stream
- **Filter Detail** — per-filter view with four tabs:
  - **Overview** — port cards with config (name, color, scopes), edit modal for each port
  - **Rules** — visual rule editor, separated by markers/filters and grouped by mode (blacklist, whitelist ALL, whitelist ANY). Add, edit, delete rules in-browser, then save to the backend
  - **ECAM** — live-streaming event monitor (errors, warnings) with auto-refresh every 3 seconds
  - **Traffic** — packet log table with decision/direction filters, plus charts (decisions over time, traffic by port)
- **TLS Configuration** — in the port config modal, toggle TLS, upload cert/key PEM files, configure upstream re-encryption and cert verification
- **Dark / Light Mode** — toggle in sidebar footer, persisted in localStorage

### Keyboard Shortcuts

- `Enter` in the controller URL field → connect
- All modals close on overlay click or Cancel

## Environment Reference

### Filter (`SINKHOLE_FILTER_` prefix)

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `UPSTREAM_HOST` | string | required | Challenge host address |
| `LISTEN_MAP` | string | `""` | Port mappings `"pub1:up1,pub2:up2"` |
| `CTRL_PORT` | u16 | `7000` | Control API port |
| `DEBUG` | bool | `false` | Include eval details in logs |
| `AUTH_HASH` | string | unset | SHA256 hex of auth token |
| `BODY_LIMIT_BYTES` | usize | `1048576` | Max request body (API) |
| `HYDRATE_DB` | bool | `false` | Restore rules from DB on start |
| `DB_PATH` | string | `sinkhole.db` | SQLite file path |
| `TLS_ENABLED` | bool | `false` | Terminate client TLS |
| `TLS_CERT_PATH` | string | unset | PEM certificate chain |
| `TLS_KEY_PATH` | string | unset | PEM private key |
| `TLS_UPSTREAM` | bool | `false` | Re-encrypt to upstream |
| `TLS_SKIP_VERIFY` | bool | `true` | Skip upstream cert check |

### Controller

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `CTRL_BIND` | string | `0.0.0.0:7000` | HTTP bind address |
| `CTRL_DB_PATH` | string | `ctrl.db` | SQLite path |
| `CTRL_PULL_INTERVAL_SECS` | u64 | `2` | Log/ECAM poll interval |
| `SYNC_INTERVAL_SECS` | u64 | `15` | Port discovery interval |
| `SERVICE_{N}_URL` | string | required | Filter base URL |
| `SERVICE_{N}_TOKEN` | string | required | Filter auth token |
| `SERVICE_{N}_ID` | string | `svc{N}` | Display ID |
| `SERVICE_{N}_NAME` | string | unset | Display name |

