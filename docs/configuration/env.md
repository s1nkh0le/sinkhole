# Environment variables

Both binaries are configured entirely through environment variables. The filter uses the `SINKHOLE_FILTER_` prefix and the controller uses the `SINKHOLE_CONTROLLER_` prefix. Booleans accept `true` or `false`.

The authoritative defaults live in the source (`filter/src/config/env.rs` and `controller/src/config/env.rs`). Related pages:

- The token and CORS model: [Authentication](auth.md).
- TLS on a data port: [TLS termination](tls.md).
- What hydration, retention, and the sync cadences do: [Data and persistence](../architecture/persistence.md).

---

## Filter (`SINKHOLE_FILTER_*`)

A filter reads these once at startup and does not look at them again while it runs. In practice you always set `LISTEN_MAP`, and `UPSTREAM_HOST` unless every listen entry names its own host. Everything else has a working default, which is what lets a filter come up and start forwarding with no further configuration.

| Variable | Type | Default | Required | Meaning |
|---|---|---|---|---|
| `SINKHOLE_FILTER_UPSTREAM_HOST` | string | (none) | cond. | Default host traffic is forwarded to (e.g. `service`, `127.0.0.1`). Required unless every `LISTEN_MAP` entry specifies its own `public:host:upstream`. |
| `SINKHOLE_FILTER_LISTEN_MAP` | string | `""` | yes | Comma-separated listen entries. Each is `public:upstream` (routes to `UPSTREAM_HOST`) or `public:host:upstream` (per-port upstream host, so one filter can guard several services). For example `8080:80,8443:8000` or `8080:web:80,1338:pwn:1338`. Must be non-empty. |
| `SINKHOLE_FILTER_TLS_MAP` | string | `""` | no | Comma-separated `public:cert:key` entries marking a port as a TLS service (terminate client TLS, re-encrypt to the service). See [TLS termination](tls.md). |
| `SINKHOLE_FILTER_CONTROL_BIND` | socket addr | `0.0.0.0:7001` | no | Bind address for the control API the controller talks to. |
| `SINKHOLE_FILTER_DEBUG` | bool | `false` | no | Verbose behaviour; allows running with auth disabled (see [Authentication](auth.md)). |
| `SINKHOLE_FILTER_AUTH_TOKEN` | string | (none) | no\* | Control-API token in plaintext; hashed at load if no hash is given. |
| `SINKHOLE_FILTER_AUTH_TOKEN_HASH` | hex sha256 | (none) | no\* | Precomputed sha256 of the control-API token. Takes precedence over `AUTH_TOKEN`. If neither is set, auth is disabled (warned), and in non-debug mode requests get a `503`. |
| `SINKHOLE_FILTER_BODY_LIMIT_BYTES` | usize | `1048576` | no | How much of a body is captured for inspection and stored in a record. Forwarding stays byte-exact; bodies above this are marked truncated. Must be `> 0`. |
| `SINKHOLE_FILTER_IDLE_FLUSH_MS` | u64 (ms) | `50` | no | Quiet window for unframed (non-HTTP) traffic. When the filter holds bytes that are not yet a complete HTTP message and the sender pauses this long, it flushes them as a raw chunk, so interactive protocols are not buffered until EOF. `0` restores the old buffer-until-complete behaviour. |
| `SINKHOLE_FILTER_CONN_IDLE_TIMEOUT_SECS` | u64 (s) | `120` | no | Close a connection that has seen no bytes in either direction for this long. Bounds slowloris and dead, half-open connections without cutting active sessions. `0` disables it. |
| `SINKHOLE_FILTER_CONN_MAX_LIFETIME_SECS` | u64 (s) | `600` | no | Close a connection once it has been open this long, regardless of activity. A hard upper bound; raise it for legitimately long sessions, or set `0` to disable. |
| `SINKHOLE_FILTER_MAX_CONNECTIONS` | usize | `10000` | no | Cap on simultaneous open connections across the whole filter. At the cap, a new connection is accepted and then immediately dropped (not queued). Sized to bound a flood without affecting realistic load; `0` disables the cap. |
| `SINKHOLE_FILTER_DB_HYDRATE` | bool | `true` | no | Restore the per-port ruleset and config from the local database at boot. Opt-out; keep it on, since it is how a filter re-enforces its rules after a restart. See [hydration](../architecture/persistence.md#hydration-filter-boot). |
| `SINKHOLE_FILTER_DB_PATH` | string | `sinkhole-data/sinkhole-filter.db` | no | Path to the filter's local SQLite database. |
| `SINKHOLE_FILTER_SINKS_STDOUT` | bool | `true` | no | Echo records to stdout (records also require `DEBUG`; ECAM events follow this flag alone). |
| `SINKHOLE_FILTER_SINKS_EMIT` | bool | `true` | no | Write records to the database at all. |

\* Without `AUTH_TOKEN` or `AUTH_TOKEN_HASH` the control API is unauthenticated. Set one in production. See [Authentication](auth.md).

Two constraints are checked at startup: `CONTROL_BIND` must not collide with any public port in `LISTEN_MAP`, and public ports must be unique and non-zero.

---

## Controller (`SINKHOLE_CONTROLLER_*`)

The controller also reads these once at startup. Outside debug mode it fails closed: it refuses to start without an auth token. It also refuses to start without a CORS origin **unless it is serving the bundled web UI** (`SERVE_FRONTEND`, on by default). That UI is same-origin, so it needs no CORS, and cross-origin requests are simply denied. You only need `CORS_ORIGIN` when a *separate* frontend on another origin is the browser client. Everything else has a default, and the sync cadences are the main thing you would tune for a given fleet.

### Global settings

| Variable | Type | Default | Required | Meaning |
|---|---|---|---|---|
| `SINKHOLE_CONTROLLER_DEBUG` | bool | `false` | no | Verbose behaviour. Also allows permissive CORS and auth-disabled startup. |
| `SINKHOLE_CONTROLLER_BIND` | socket addr | `0.0.0.0:7000` | no | Bind address for the controller's API. |
| `SINKHOLE_CONTROLLER_DB_PATH` | string | `sinkhole-data/sinkhole-ctrl.db` | no | Path to the central SQLite database. |
| `SINKHOLE_CONTROLLER_DB_HYDRATE` | bool | `true` | no | Persist the tracked fleet and restore it on boot, so services added at runtime survive a restart. Opt-out; when off, the fleet is whatever the environment declares. See [fleet hydration](../architecture/persistence.md#controller-fleet-hydration). |
| `SINKHOLE_CONTROLLER_CORS_ORIGIN` | string | (none) | cond. | Allowed origin(s) of a *separate* browser frontend, comma-separated. Unset means permissive with `DEBUG=true`, or same-origin-only (cross-origin denied) when serving the bundled UI. In non-debug without the bundled UI the controller refuses to start, so it is only required when you turn `SERVE_FRONTEND` off in non-debug. See [Authentication](auth.md). |
| `SINKHOLE_CONTROLLER_SERVE_FRONTEND` | bool | `true` | no | Serve the web UI (embedded in the controller binary at build time) under `/ui`, same origin as the API so the browser needs no CORS; `/` then redirects to `/ui/`. On by default. Set to `false` to host the frontend separately (e.g. via Vite), in which case set `CORS_ORIGIN`. |
| `SINKHOLE_CONTROLLER_AUTH_TOKEN_HASH` | hex sha256 | (none) | no\* | Expected controller token as a sha256 hash. Takes precedence over `AUTH_TOKEN`. |
| `SINKHOLE_CONTROLLER_AUTH_TOKEN` | string | (none) | no\* | Plain controller token; hashed at startup if no hash is given. |
| `SINKHOLE_CONTROLLER_SYNC_HEALTH_INTERVAL_MS` | u64 | `2000` | no | How often to poll each filter's health. |
| `SINKHOLE_CONTROLLER_SYNC_ECAM_INTERVAL_MS` | u64 | `5000` | no | How often to pull ECAM events. |
| `SINKHOLE_CONTROLLER_SYNC_RECORDS_INTERVAL_MS` | u64 | `3000` | no | How often to pull evaluation records. |
| `SINKHOLE_CONTROLLER_SYNC_CONFIG_INTERVAL_MS` | u64 | `5000` | no | How often to refresh ports, config, and ruleset snapshots. |
| `SINKHOLE_CONTROLLER_SYNC_CONCURRENCY` | usize | `4` | no | Max in-flight per-port record fetches per service (at least 1). |
| `SINKHOLE_CONTROLLER_PRUNE_INTERVAL_SECS` | u64 | `600` | no | How often the [pruner](../architecture/persistence.md#pruning-and-retention) runs. |
| `SINKHOLE_CONTROLLER_RETENTION_FILTER_SECS` | u64 | `600` | no | How long records already copied to the controller are kept on the filter (default 10 minutes). `0` prunes them as soon as they are centralized. |
| `SINKHOLE_CONTROLLER_RETENTION_DB_SECS` | u64 | `0` | no | Retention for the central database. `0` keeps everything. |

\* In non-debug mode a token is required: if neither `AUTH_TOKEN_HASH` nor `AUTH_TOKEN` is set, the controller refuses to start. With `DEBUG=true`, missing auth is allowed and warned. See [Authentication](auth.md).

### Defining the filters to manage

The managed fleet is a JSON array in one variable, `SINKHOLE_CONTROLLER_SERVICES`:

```bash
SINKHOLE_CONTROLLER_SERVICES='[
  {"id":"edge","url":"http://10.0.0.5:7001","token":"<filter plaintext token>","name":"Edge filter"},
  {"id":"api","url":"http://10.0.0.6:7001","token":"<filter plaintext token>"}
]'
```

| Field | Required | Meaning |
|---|---|---|
| `id` | yes | A stable, unique name you use to refer to this filter in the controller. |
| `url` | yes | Base URL of the filter's control API (must be `http://` or `https://`). |
| `token` | yes | The filter's plaintext token (its `SINKHOLE_FILTER_AUTH_TOKEN`), used to authenticate to it. |
| `name` | no | Display name; seeds the service's metadata name, editable later through the API. |

Filters can also be added and removed at runtime through the controller API, so this variable is just the starting fleet. See [Controller](../architecture/controller.md).

---

## Minimal examples

Filter:

```bash
SINKHOLE_FILTER_UPSTREAM_HOST=service \
SINKHOLE_FILTER_LISTEN_MAP="8080:80" \
SINKHOLE_FILTER_CONTROL_BIND="0.0.0.0:7001" \
SINKHOLE_FILTER_AUTH_TOKEN_HASH="$(printf '%s' "$TOKEN" | sha256sum | cut -d' ' -f1)" \
./filter
```

Controller (bundled UI, the default; open `http://<host>:7000`, which redirects to `/ui/`):

```bash
SINKHOLE_CONTROLLER_BIND="0.0.0.0:7000" \
SINKHOLE_CONTROLLER_AUTH_TOKEN="$CTRL_TOKEN" \
SINKHOLE_CONTROLLER_SERVICES='[{"id":"edge","url":"http://127.0.0.1:7001","token":"'"$TOKEN"'"}]' \
./controller
```

Controller with a *separate* frontend instead (disable the bundled UI, allow its origin):

```bash
SINKHOLE_CONTROLLER_BIND="0.0.0.0:7000" \
SINKHOLE_CONTROLLER_AUTH_TOKEN="$CTRL_TOKEN" \
SINKHOLE_CONTROLLER_SERVE_FRONTEND="false" \
SINKHOLE_CONTROLLER_CORS_ORIGIN="https://admin.example.com" \
SINKHOLE_CONTROLLER_SERVICES='[{"id":"edge","url":"http://127.0.0.1:7001","token":"'"$TOKEN"'"}]' \
./controller
```
