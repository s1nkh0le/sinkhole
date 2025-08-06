# 1. System-wide utilities

### 1.1 `GET /health`

Simple liveness probe.

- Authentication: none
- Response: `{"status":"ok"}`


### 1.2 `GET /version`

Returns the running software revision.

- Authentication: none
- Response: `{"status":"ok","data":{"version":"1.0.0"}}`

---

# 2. Central event monitor (ECAM)

### 2.1 `GET /ecam`

Fetches operational events (warnings, errors).

- Authentication: required
- URL parameters: none
- Query parameters: 
  - `since` 
    - Unit: [ns]
    - Default: `0`
- Response: `{"status":"ok", "data": [<EcamEvent>, ...], "next_since": 1754004005155452001}`


`EcamEvent` object

```json
{
  "ts_ns":      <i64>,
  "severity":   "warn" | "error", 
  "origin":     "Network" | "Configuration" | "Security" | ...,
  "msg":        "<text>",
  "data":       <json> | null
}
```

References:
- `EcamEvent`: `src/observability/sinks/sink_ecam.rs`
- `Severity`: `src/observability/sinks/sink_ecam.rs`
- `Origin`: `src/observability/sinks/sink_ecam.rs`


Notes:
- Events are ordered ascending; `next_since` is always `max(ts_ns) + 1`.

---

# 3. Port registry

### 3.1 `GET /ports`

List all configured public ports.

- Authentication: required
- Response: `{"status":"ok", "data":[<PortInfo>, ...]}`

`PortInfo`

```json
{
  "public":           8080,
  "upstream":         80,
  "name":             "blocca",
  "comment":          "commento a caso",
  "hex_color":        "#FF8800",
  "enabled_scopes":   ["stream","http.body", "meta", ...],
  "version":          "96ff2df5b203ddba9f62dd495143d01d33965cb244a3f0cf3f8d6a391e06cb8c"
}
```

References:
- `PortInfo`: `src/api/config.rs`
- `Scope`: `src/types.rs`

Notes:
- `public` is the port exposed to the internet
- `upstream` is the protected, internal service port
- `enabled_scopes` can not be empty
- `version` is the hash of the whole port config+ruleset, so it can be used to check for changes
- `name`, `comment`, and `hex_color` are optional

---

### 3.2 `GET /ports/{port}/config`

Retrieve per-port metadata.

- Authentication: required
- URL parameters: `port`
- Response: `{"status":"ok", "data":<PortInfo>}`


### 3.3 `PUT /ports/{port}/config`

Update metadata / enabled scopes.


- Authentication: required
- URL parameters: `port`
- Body: `{"name":<str?>, "comment":<str?>, "hex_color":<str?>, "enabled_scopes":[<Scope>, ...] }`
- Response: `{"status":"ok"}`


---

# 4. Ruleset management

### 4.1 `GET /ports/{port}/ruleset`

Return the **raw** rule JSON currently effective.

- Authentication: required
- URL parameters: `port`

Response:
```
{
  "data": {
    "ruleset": {
      "comment": null,
      "filters": [],
      "markers": []
    },
    "version": "96ff2df5b203ddba9f62dd495143d01d33965cb244a3f0cf3f8d6a391e06cb8c"
  },
  "status": "ok"
}
```


### 4.2 `PUT /ports/{port}/ruleset`

Upload & replace the rule set for a port.

- Authentication: required
- URL parameters: `port`
Body: 
```
{
  "comment": "Commento inutile",
  "markers": [<MarkerDef>, ...],
  "filters": [<FilterDef>, ...]
}

```

- Response: `{"status":"ok", "data":{"version":version}}`

References:
- `MarkerDef`: `src/rules/model.rs`
- `FilterDef`: `src/rules/model.rs`


Notes:
- `markers` and `filters` are the fules that will replace the active rules in that port configuration


# 5. Runtime statistics

### 5.1 `GET /ports/{port}/stats`

- Authentication: required
- URL parameters: `port`
- Response: `{"status":"ok"}`

---

# 6. Log retrieval

### 6.1 `GET /ports/{port}/records`

Query stored `EvaluationRecord`s.


- Authentication: required
- URL parameters: `port`
- Query parameters:
  - `until`
    - Type: `Option<i64>`
    - Unit: [ns]
    - Default: `i64::MAX`
  - `since`
    - Type: `Option<i64>`
    - Unit: [ns]
    - Default: `0`
  - `limit`
    - Type: `Option<i64>`
    - Default: `1_000`
    - Min: `1`
    - Max: `10_000`
  - `order`
    - Type: `Option<String>`
    - Default: `asc`
    - Possible values: `asc`, `desc`
  - `decision`
    - Type: `Option<String>`
    - Default: `both`
    - Possible values: `drop`, `allow`
  - `dir`
    - Type: `Option<String>`
    - Default: `in` and `out`
    - Possible values: `in`, `out`
  - `reason`
    - Type: `Option<String>`
    - Default: `blacklist` and `whitelist_all_miss` and `whitelist_any_miss`
    - Possible values: `blacklist`, `whitelist_all_miss`, `whitelist_any_miss`


Response

```json
{
  "status":"ok",
  "data":{
    "records":[<EvaluationRecord>, ...],
    "next_since": 1753964700000000001
  }
}
```

Notes:
- `next_since` is the maximum timestamp for `EvaluationRecord`s retrieved by this query, plus 1 (ns); it can be used as the `since` parameter in a subsequent query with the same parameters to obtain only newer records


---

# 7. Database maintenance

### 7.1 `GET /db/health`

Quick check that SQLite is reachable.

### 7.2 `GET /db/stats`

Return current row counts & on-disk size.

### 7.3 `GET /db/clear`

**Dangerous**: deletes *all* tables, recreates schema.

---

# 8. Misc

## Environment flags impacting the API

| Variable               | Default   | Effect                                                |
|------------------------|-----------|-------------------------------------------------------|
| `SINKHOLE_AUTH_HASH`   | *(unset)* | Enables token authentication when set.                |
| `SINKHOLE_SINK_STDOUT` | `true`    | When `false`, suppresses sink emission to stdout.     |
| `SINKHOLE_SINK_EMIT`   | `true`    | When `false`, disables internal fan-out to DB / ECAM. |

---
