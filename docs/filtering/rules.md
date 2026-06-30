# Rule model

A ruleset is a small JSON document you send to one port of one filter. It lists the rules the filter runs on every message through that port. You write it, the controller hands it to the filter, and the filter starts using it straight away.

There are two kinds of rule, and it helps to keep them apart:

- A **marker** recognizes something about a message, for example "this is a POST" or "this path looks like an admin page", and just remembers the answer. It never blocks anything by itself.
- A **filter** decides whether to allow or block, and can be told to apply only when certain markers matched.

Here is a complete ruleset that blocks any request whose path starts with `/admin`:

```json
{
  "comment": "block admin",
  "markers": [],
  "filters": [
    {
      "id": "no-admin",
      "mode": "blacklist",
      "direction": "inbound",
      "scope": "http.path",
      "matcher": { "type": "path_regex", "pattern": "^/admin" }
    }
  ]
}
```

This one needs no markers. It has a single filter: `blacklist` means a match drops the message, `direction: inbound` means it only looks at client-to-service traffic, `scope: http.path` means it looks at the request path, and the matcher is a regular expression on that path. An empty ruleset (no markers and no filters) allows everything.

The rest of this page is the reference for every field, scope, and matcher. For how a ruleset is applied to a message at runtime see [Evaluation semantics](evaluation.md), and for ready-made rulesets see [Examples](examples.md). Defined terms are collected in the [Glossary](../reference/glossary.md).

## The document

| Field | Type | Required | Notes |
|---|---|---|---|
| `markers` | array | yes | May be empty. |
| `filters` | array | yes | May be empty (an empty ruleset allows everything). |
| `comment` | string | no | Free-form description. |

## Markers

A marker checks one thing about a message and stores the yes/no answer under its `id` for filters to use.

| Field | Type | Required | Notes |
|---|---|---|---|
| `id` | string | yes | Unique across all markers and filters. |
| `name` | string | no | Label for the UI only. |
| `enabled` | boolean | no | Defaults to `true`. |
| `scope` | scope | yes | Which part of the message to look at (see below). |
| `direction` | direction | no | Defaults to `"both"`. |
| `matcher` | matcher | yes | How to test that part (see below). |

## Filters

A filter is a rule that can change the verdict. It has every field a marker has, plus:

| Field | Type | Required | Notes |
|---|---|---|---|
| `mode` | mode | yes | How a match affects the verdict (see below). |
| `needed_markers_all` | array of ids | no | The filter runs only if every listed marker matched. |
| `needed_markers_any` | array of ids | no | The filter runs only if at least one listed marker matched. |

### Modes

| Mode | When the matcher matches | When it does not |
|---|---|---|
| `blacklist` | drop the message immediately | keep evaluating the other rules |
| `whitelist_all` | counts as one required hit | the message is dropped, a required hit is missing |
| `whitelist_any` | satisfies its group | dropped only if no whitelist-any filter in the group matched |

### Directions

`inbound` is client to service, `outbound` is service to client, and `both` is either. A rule only runs on messages whose direction matches.

## Scopes and the matchers they allow

A **scope** is the part of a message a rule looks at. Each scope only allows certain matchers and, in one case, certain directions. Validation and compilation use this same table, so a ruleset that validates will always compile.

| Scope | Allowed matchers | Allowed directions |
|---|---|---|
| `stream` | `stream_regex`, `stream_bytes`, `bytes_hex`, `size`, `always` | any |
| `http.path` | `path_regex`, `size`, `always` | any |
| `http.body` | `body_regex`, `body_bytes`, `bytes_hex`, `size`, `always` | any |
| `http.header` | `header_regex`, `size`, `always` | any |
| `http.method` | `method_regex`, `always` | any |
| `http.status` | `status_regex`, `always` | `outbound` or `both` (never `inbound`) |
| `timing` | `session.lifetime`, `session.idle`, `always` | `both` only |

Any other combination fails validation. In particular: `size` is not allowed on `http.status`, `http.method`, or `timing`; the byte matchers exist only where raw bytes do (`stream` and `http.body`); and timing matchers must run in both directions.

## Matchers

A matcher object has a `"type"` field that selects the variant; the other keys are its payload.

| `type` | Payload | What it does |
|---|---|---|
| `stream_regex` | `pattern`, `case_sensitive?` | Regex over the raw stream as text. |
| `path_regex` | `pattern`, `case_sensitive?` | Regex over the request path. |
| `body_regex` | `pattern`, `case_sensitive?` | Regex over the body as text. |
| `header_regex` | `header_name`, `pattern`, `case_sensitive?` | Regex over a header value; the name is matched case-insensitively. |
| `method_regex` | `pattern`, `case_sensitive?` | Regex over the HTTP method. |
| `status_regex` | `pattern`, `case_sensitive?` | Regex over the status code as text; responses only. |
| `stream_bytes` | `pattern` | Binary-safe byte regex over the raw stream (`.` is any byte, `\xNN` a literal byte). |
| `body_bytes` | `pattern` | Binary-safe byte regex over the body. |
| `bytes_hex` | `value` | A literal hex signature found anywhere in the scope's bytes, for example `"deadbeef"` (whitespace allowed). Only on `stream` or `http.body`. |
| `size` | `op`, `value`, `field_name?` | Compares the size of the scope. `field_name` only applies to `http.header`. |
| `session.lifetime` | `op`, `value` (seconds) | How long the connection has been open. `timing` scope, both directions. |
| `session.idle` | `op`, `value` (seconds) | How long since the last message. Same constraints. |
| `always` | `value` (bool) | `true` always matches, `false` never. Works with any scope. |

`op` is one of `<`, `<=`, `==`, `!=`, `>=`, `>`. `case_sensitive` defaults to `false`. Regexes use the Rust `regex` grammar, which is linear-time and has no look-around or back-references; inline `(?i)` works. Use the byte matchers for binary traffic, where `.` and `\xNN` are byte-oriented.

A few matchers on their own:

```json
{ "type": "path_regex", "pattern": "^/admin", "case_sensitive": false }
```
```json
{ "type": "size", "op": "<=", "value": 256 }
```
```json
{ "type": "bytes_hex", "value": "deadbeef" }
```
```json
{ "type": "session.idle", "op": ">", "value": 2.45 }
```

## Validation

A ruleset is checked before it can take effect, and all problems are reported together rather than one at a time. The checks are:

1. The shapes must match the tables above; an unknown key is an error.
2. Every `id` must be unique across markers and filters.
3. The combination of scope, matcher, and direction must be allowed by the table above.
4. Every id in `needed_markers_all` or `needed_markers_any` must exist and must name a marker.
5. A rule may not use a scope the port has disabled (a port can turn specific scopes off).

You can check a ruleset without deploying it through the `.../ruleset/validate` endpoint (see the [API reference](../pages/reference/swagger/index.md)).
