# API reference

sinkhole exposes two HTTP APIs, shown here from the committed OpenAPI specs (generated from the code):

- [Controller API](openapi-ctrl.md): the operator and UI-facing API. This is the one you normally use; it relays edits to filters and serves the aggregated data.
- [Filter API](openapi-filter.md): the per-filter control API the controller calls. Useful for understanding the data flow or debugging a single filter.

Each running process also serves Swagger UI at `/docs` and the raw spec at `/openapi.json`.

## Conventions

- Auth: protected endpoints need `Authorization: Bearer <token>`, the controller token for the controller API and the filter token for the filter API. See [Authentication](../../../configuration/auth.md).
- Success responses are wrapped as `{"status":"ok","data":{...}}`.
- Errors are wrapped as `{"status":"error","code":<http>,"title":"...","detail":"..."}`.
- List endpoints for records and events return a `next_since` cursor to page forward. See [Data and persistence](../../../architecture/persistence.md).
