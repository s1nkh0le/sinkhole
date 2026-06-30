# Examples

Rulesets you can copy and adapt. Send one to a port with `PUT /services/{service}/ports/{port}/ruleset` on the controller, using the service name you registered (see the [Quickstart](../usage/quickstart.md)). The [Rule model](rules.md) and [Evaluation semantics](evaluation.md) explain the pieces.

## Block a path

Drop any request to `/admin` or below.

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

## Block a byte signature in the body

Drop requests whose body contains a known byte sequence anywhere. This works on binary traffic, not just text.

```json
{
  "markers": [],
  "filters": [
    {
      "id": "sig-deadbeef",
      "mode": "blacklist",
      "direction": "inbound",
      "scope": "http.body",
      "matcher": { "type": "bytes_hex", "value": "deadbeef" }
    }
  ]
}
```

## Require a method and a path prefix

Allow only `GET` and `HEAD` requests under `/public`, and drop everything else. Both checks are whitelist-all, so both must pass.

```json
{
  "markers": [],
  "filters": [
    {
      "id": "only-get-head",
      "mode": "whitelist_all",
      "direction": "inbound",
      "scope": "http.method",
      "matcher": { "type": "method_regex", "pattern": "^(GET|HEAD)$" }
    },
    {
      "id": "only-public",
      "mode": "whitelist_all",
      "direction": "inbound",
      "scope": "http.path",
      "matcher": { "type": "path_regex", "pattern": "^/public/" }
    }
  ]
}
```

## Allow if any of several conditions hold

Pass a request if it carries an API key header, or if the path is `/health`. These are whitelist-any, so one of them is enough.

```json
{
  "markers": [],
  "filters": [
    {
      "id": "has-api-key",
      "mode": "whitelist_any",
      "direction": "inbound",
      "scope": "http.header",
      "matcher": { "type": "header_regex", "header_name": "x-api-key", "pattern": ".+" }
    },
    {
      "id": "is-health",
      "mode": "whitelist_any",
      "direction": "inbound",
      "scope": "http.path",
      "matcher": { "type": "path_regex", "pattern": "^/health$" }
    }
  ]
}
```

## Only scan large bodies

Use a marker to check something once, here whether the body is over 1 MiB, and run the expensive scan only when it holds.

```json
{
  "markers": [
    {
      "id": "big-body",
      "direction": "inbound",
      "scope": "http.body",
      "matcher": { "type": "size", "op": ">", "value": 1048576 }
    }
  ],
  "filters": [
    {
      "id": "scan-large",
      "mode": "blacklist",
      "direction": "inbound",
      "scope": "http.body",
      "needed_markers_all": ["big-body"],
      "matcher": { "type": "body_regex", "pattern": "(?i)<script" }
    }
  ]
}
```

## Drop on a response status

Catch the service returning `5xx` responses. Status rules apply to responses only.

```json
{
  "markers": [],
  "filters": [
    {
      "id": "no-5xx",
      "mode": "blacklist",
      "direction": "outbound",
      "scope": "http.status",
      "matcher": { "type": "status_regex", "pattern": "^5\\d\\d$" }
    }
  ]
}
```

## Drop idle connections

Timing rules must use `direction: both`.

```json
{
  "markers": [],
  "filters": [
    {
      "id": "too-idle",
      "mode": "blacklist",
      "direction": "both",
      "scope": "timing",
      "matcher": { "type": "session.idle", "op": ">", "value": 30.0 }
    }
  ]
}
```
