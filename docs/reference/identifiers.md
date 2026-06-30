# Identifiers

When you work with sinkhole you deal with three kinds of identifier. They are easy to mix up, so here is what each one is and where you use it.

| Identifier | What it is | Where you use it |
|---|---|---|
| Address (host and port) | where something listens on the network | clients connect to a filter's data port; the controller reaches a filter at its control address |
| Service id | the name you give the controller for a managed filter | every controller API path, and the UI |
| Display name | a human-friendly label for a service | the UI only |

## Addresses

These are ordinary network addresses.

- Clients connect to a filter's public data ports (the `public` side of `LISTEN_MAP`), and the filter forwards to the service behind it.
- The controller reaches each filter at its control address (the filter's `CONTROL_BIND`, default port `7001`), and the web UI reaches the controller at its `BIND` (default port `7000`).

## Service id

When you register a filter with the controller you give it an `id`. That id is how you refer to the filter from then on: it appears in every controller API path (for example `PUT /services/edge/ports/8080/ruleset`), it keys the filter's records and cursors in the central database, and it is how the UI lists and selects filters.

You set it in the controller's `SINKHOLE_CONTROLLER_SERVICES`, along with the filter's control `url` and `token`:

```json
[{"id":"edge","url":"http://10.0.0.5:7001","token":"…","name":"Edge filter"}]
```

The id maps to the `url` and `token`, which is what actually tells the controller how to reach the filter. Because the id stays stable, a filter can change its address (move hosts) without losing its history.

## Display name

The optional `name` is just a label shown in the UI. You can set it when you register the filter and change it later through the API, along with a description, color, and icon. It never addresses anything, and it can change freely.

## Putting it together

For a filter guarding a service on `10.0.0.5`, published on port `1337` and registered as `edge`:

- clients connect to `10.0.0.5:1337`, the data address;
- the controller talks to `http://10.0.0.5:7001`, the control address;
- you operate it as `/services/edge/...`, the service id.

It is the same filter; you just refer to it differently depending on whether you are a client, the controller, or an operator.

See the [API reference](../pages/reference/swagger/index.md) and [Environment variables](../configuration/env.md).
