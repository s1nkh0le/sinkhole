## 0. Glossary — the moving parts

| *Term*                 | Definition                                                                                                                                         | Why it exists / what problem it solves                                                                  |
|------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------|---------------------------------------------------------------------------------------------------------|
| **`Message`**          | One complete unit of traffic examined by sinkhole: either a full HTTP request/response *or* a raw TCP chunk when we cannot recognise HTTP.         | It is the atomic “thing” we decide *Allow* or *Drop* for.                                               |
| **`Marker`**           | A light-weight rule that only answers **yes / no** and stores that answer under its `id` for later use.                                            | Lets you compute facts once and reuse them across many filters (e.g. “body bigger than 1 MiB”).         |
| **`Filter`**           | A rule that can change the connection’s fate.  Evaluated after markers.                                                                            | Carries the business logic: block IPs, allow authenticated paths, enforce timing, …                     |
| **`Scope`**            | Where the rule reads its input string from (`http.path`, `stream`, …).                                                                             | Keeps matchers simple and reusable (a `regex` doesn’t need to know if it is matching a path or a body). |
| **`Matcher`**          | The micro-engine that performs the actual test (regex, size, always-true, timing…).                                                                | Makes the DSL extensible — new matchers can be added without changing the outer JSON shape.             |
| **`Direction`**        | Rule direction: `"in"`, `"out"` or `"both"` correlates to client→server or server→client flow.                                                     | Prevents nonsense like testing a response status on incoming traffic.                                   |
| **`Mode`**             | Filter’s strategy: `blacklist`, `whitelist_all`, `whitelist_any`.                                                                                  | Describes *how* its truth value affects the final decision.                                             |
| **`Gating`**           | A filter can declare `needed_markers_all` or `needed_markers_any`; said filter will run *if and only if* those markers requirements are satisfied. | Express complex boolean logic (*A **and** (B **or** C)*).                                               |
| **`Decision`**         | The engine’s final verdict for a message: `Allow` or `Drop`.                                                                                       | Forward or discard bytes, and save the audit record.                                                    |
| **`DropReason`**       | Structured explanation attached when `Decision::Drop`.                                                                                             | Lets UIs show *why* something was blocked (`blacklist`, `whitelist_any_miss`, …).                       |
| **`EvaluationDetail`** | Per-marker / per-filter debug blob saved when the port is in `debug=true`.                                                                         | Exhaustive trace for troubleshooting (“regex skipped because header missing”, etc.).                    |

---

## 1  Top-level JSON document (`RuleSetRaw`)

| Field     | Type       | Required | Notes                               |
|-----------|------------|----------|-------------------------------------|
| `markers` | `Marker[]` | yes      |                                     |
| `filters` | `Filter[]` | yes      |                                     |
| `version` | `string`   | no       | Opaque string (hash, semver, etc.). |

---

## 2  Marker object (`Marker`)

| Field       | Type           | Required | Constraints                                                             |
|-------------|----------------|----------|-------------------------------------------------------------------------|
| `id`        | `string`       | yes      | ASCII `[A-Za-z0-9_-]`, max 64 chars, unique across *markers + filters*. |
| `name`      | `string`       | no       | UI label only.                                                          |
| `enabled`   | `boolean`      | no       | Defaults `true`.                                                        |
| `scope`     | **Scope**      | yes      | See § 5.                                                                |
| `direction` | **Direction**  | no       | Default `"both"`.                                                       |
| `matcher`   | **MatcherDef** | yes      | Variant object, § 6.                                                    |
| `meta`      | `object`       | no       | Opaque user data.                                                       |

Markers are usually used for one of four patterns:

| Pattern              | How it looks                   | What it enables                                                          |
|----------------------|--------------------------------|--------------------------------------------------------------------------|
| **One-shot fact**    | `size > 1_048_576`             | Later filters can say *“if body is large, then …”*.                      |
| **Cheap pre-filter** | header `"user-agent" =~ Bot`   | Avoids expensive regexes until we are sure it’s a bot.                   |
| **State flag**       | `always:true`                  | Book-keeping (“gate every whitelist to marker *client\_ok* after auth”). |
| **Debug probe**      | `path_regex` on a passive port | See where traffic *would* match without dropping.                        |

---

## 3  Filter object (`Filter`)

`Filter` inherits all fields of **Marker** and adds:

| Field                                     | Type       | Required | Constraints                                                    |
|-------------------------------------------|------------|----------|----------------------------------------------------------------|
| `mode`                                    | **Mode**   | yes      | § 4.                                                           |
| `needed_markers_all`                      | `string[]` | no       | All listed marker `id`s must be satisfied before matcher runs. |
| `needed_markers_any`                      | `string[]` | no       | At least one of these marker `id`s must be satisfied.          |
| *(All inherited fields retain identical)* |            |          |                                                                |

Examples:

| Mode               | When matcher is **satisfied** | When **not satisfied**                                                        | Typical use-case                                                    |
|--------------------|-------------------------------|-------------------------------------------------------------------------------|---------------------------------------------------------------------|
| **blacklist**      | drop **immediately**          | keep evaluating                                                               | IP bans, SQLi signatures                                            |
| **whitelist\_all** | noted as “OK”                 | Disregard rule current this message                                           | Hard allow-list (e.g. `(method ∈ {GET,HEAD}) ∧ (path /^\/public/)`) |
| **whitelist\_any** | recorded as “one hit”         | Keep evaluating<br/> if **none** hit, disregard rule for current this message | Soft allow (“either header A=foo *or* UA=bar”)                      |

---

## 4  Enumerations

| Enum          | Allowed JSON literals                                                                                               |
|---------------|---------------------------------------------------------------------------------------------------------------------|
| **Mode**      | `"blacklist"` \| `"whitelist_all"` \| `"whitelist_any"`                                                             |
| **Direction** | `"in"` \| `"out"` \| `"both"` (default)                                                                             |
| **Scope**     | `"stream"` \| `"http.path"` \| `"http.body"` \| `"http.header"` \| `"http.method"` \| `"http.status"` \| `"timing"` |
| **CmpOp**     | `"lt"` \| `"le"` \| `"eq"` \| `"ne"` \| `"ge"` \| `"gt"`                                                            |

---

## 5  Scope ↔ matcher compatibility matrix

> `src/rules/validate.rs > matcher_scope_ok()`

| Scope         | Permitted matcher variants                   | Permitted directions |
|---------------|----------------------------------------------|----------------------|
| `meta`?       | `size`, `always`                             | `both`               |
| `stream`      | `stream_regex`, `size`, `always`             | `both`               |
| `http.path`   | `path_regex`, `size`, `always`               | `both`               |
| `http.body`   | `body_regex`, `size`, `always`               | `both`               |
| `http.header` | `header_regex`, `size`, `always`             | `both`               |
| `http.method` | `method_regex`, `always`                     | `both`               |
| `http.status` | `status_check`, `always`                     | `outbound`           |
| `timing`      | `session_lifetime`, `session_idle`, `always` | `both`               |

Any other combination **fails validation**.


---

## 6  Matcher variant definitions (`MatcherDef`)

All matcher objects are single-key maps whose key identifies the variant;
each contains the payload listed below.

| Variant key        | Payload shape                                                                 | Extra rules                                                   |
|--------------------|-------------------------------------------------------------------------------|---------------------------------------------------------------|
| `stream_regex`     | `{ "regex": <string>, "case_insensitive": *bool?* }`                          |                                                               |
| `path_regex`       | `{ "regex": <string>, "case_insensitive": *bool?* }`                          |                                                               |
| `body_regex`       | `{ "regex": <string>, "case_insensitive": *bool?* }`                          |                                                               |
| `method_regex`     | `{ "regex": <string>, "case_insensitive": *bool?* }`                          |                                                               |
| `header_regex`     | `{ "regex": <string>, "header_name": <string>, "case_insensitive": *bool?* }` | `header_name` always compared ASCII-case-insensitive.         |
| `status_check`     | `{ "op": <CmpOp>, "value": *integer* }`                                       |                                                               |
| `size`             | `{ "op": <CmpOp>, "value": *integer*, "field_name": *string?* }`              | `field_name` allowed **only** with `http.header` scope.       |
| `session_lifetime` | `{ "op": <CmpOp>, "value_sec": *number* }`                                    | Scope **must** be `timing`, `direction` **must** be `"both"`. |
| `session_idle`     | `{ "op": <CmpOp>, "value_sec": *number* }`                                    | Same constraints.                                             |
| `always`           | `{ "value": true }` or `{ "value": false }`                                   | Boolean literal.                                              |

Regexes follow Rust `regex` crate grammar (no look-behind / look-ahead /
back-references).

### 6.1 Regex

* Uses Rust’s `regex` crate → **no** look-behind / look-ahead / back-refs.
* Case-insensitive flag available via `(?i)`.

```json
  {"path_regex":"(?i)^/admin"}
```

### 6.2 Size

```json
{
  "size": {
    "op": "<=",
    "value": 256
  }
}
```

### 6.3 Timing

```json
{
  "session_idle": {
    "op": ">",
    "value_sec": 2.45
  }
}
```

* `value_sec`'s unit is seconds.
* Internally compared as `f64`.

### 6.4 Always

```json
{
  "always": true
}
```

---

## 7  Validation rules (compile-time)

1. **Keys** must follow shapes in §§ 1–6; unknown keys → error.
2. `id` duplication or invalid chars → error.
3. `matcher` variant must be allowed for the rule’s `scope` (matrix above).
4. Every id listed in `needed_markers_all / needed_markers_any` must exist and refer to a
   marker.
5. At least one enabled filter in the ruleset.
6. Port configuration may disable specific scopes; if any rules touches disabled scopes, it's invalid
7. All failures are aggregated and returned as `{"status": "error", "errors": [...]}`.

---

## 8  Run-time evaluation semantics

1. **Marker phase**: evaluate every enabled marker (respecting `direction`).
   *Matcher output is stored under its `id`.*
2. **Filter phase**: walk filters in order.
    * If gating conditions fail, filter is *skipped*.
    * If mode `blacklist` **satisfies**, decision becomes `Drop`
      immediately (unless port `debug=true`).
    * `whitelist_*` results are collected for final evaluation.
3. **Decision synthesis**:
   *If no blacklist hit*:

    * `whitelist_all` → drop if **any** gated filter missed.
    * `whitelist_any` → drop if **zero** gated filters satisfied.
    * else `Allow`.
4. **Drop-reasons** emitted:

    * `"blacklist"` with field `filter_ids` if any blacklist satisfied.
    * `"whitelist_all_miss"` or `"whitelist_any_miss"` with human detail text if
      whitelist logic failed.
5. **EvaluationRecord** persisted (structure defined in the API reference).
6. If `Decision::Allow`, bytes are forwarded; otherwise connection half closes.

---

## 9  Debug detail object (`EvaluationDetail`)

| Field             | Type                                                                                       | Notes                                                |
|-------------------|--------------------------------------------------------------------------------------------|------------------------------------------------------|
| `kind`            | `"marker"` \| `"filter"`                                                                   |                                                      |
| `id`              | `string`                                                                                   | Rule id.                                             |
| `name`            | `string?`                                                                                  |                                                      |
| `mode`            | **Mode?**                                                                                  | Present for filters only.                            |
| `gated`           | `boolean?`                                                                                 | Whether filter was gated (`false` → skipped).        |
| `decision_effect` | `"blacklist_drop"` \| `"whitelist_all_satisfied"` \| `"whitelist_any_satisfied"` \| `null` | Applies to filters.                                  |
| `match_output`    | object                                                                                     | Raw matcher result (scope, condition, status, etc.). |

`EvaluationDetail` objects appear inside `EvaluationRecord.details[]` only when
the port’s `debug` flag is *true*.
