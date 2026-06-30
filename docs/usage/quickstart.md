# Quickstart

This brings the whole system up on one machine and walks through the operator path: deploy a ruleset through the controller, send a request, and watch it get dropped.

The local stack is `docker-compose.test.yaml`. It is self-contained: every token is baked into the file, so there is nothing to set up first.

!!! note "Test topology only"
    This stack runs everything on a single Docker network for convenience. In a real deployment the filters and the controller run on separate machines. See [Deployment](../operations/deployment.md).

## Prerequisites

- Docker, with the Compose plugin (`docker compose`).
- Optional: `just` and a Rust toolchain. `just` wraps the same Docker commands and is shown next to them below.

## 1. Bring up the stack

=== "Docker"

    ```bash
    docker compose -f docker-compose.test.yaml up --build
    ```

=== "just"

    ```bash
    just up
    ```

This starts a handful of mock services, one filter in front of each, and the controller (its API is on port `7000`). One of those services is `gadget`: an echo server you can reach through its filter on port `6001`.

When you are done, tear it down:

=== "Docker"

    ```bash
    docker compose -f docker-compose.test.yaml down -v
    ```

=== "just"

    ```bash
    just down
    ```

## 2. Deploy a ruleset

Everything goes through the controller. This blocks any request whose path starts with `/blocked`, on the `gadget` service, port `6001`.

!!! note
    This is the raw API call. In normal use the web UI builds and sends it for you, so you would not type this by hand. It is shown so you can see what the UI does under the hood, and so you can script the controller if you want to.

```bash
CTRL=http://localhost:7000
TOKEN=token-controller-897fe79a   # baked into docker-compose.test.yaml

curl -fsS -X PUT "$CTRL/services/gadget/ports/6001/ruleset" \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
        "comment": "demo",
        "markers": [],
        "filters": [
          { "id": "no-blocked", "mode": "blacklist", "direction": "inbound",
            "scope": "http.path",
            "matcher": { "type": "path_regex", "pattern": "^/blocked" } }
        ]
      }'
```

## 3. Send traffic through the filter

```bash
curl -i http://localhost:6001/          # allowed: the echo service replies 200
curl -i http://localhost:6001/blocked   # dropped: the connection closes with no reply
```

## 4. See the decision in the controller

The controller pulls records from each filter on a short interval. After a moment:

```bash
curl -fsS "$CTRL/services/gadget/ports/6001/records?decision=drop&limit=20" \
  -H "Authorization: Bearer $TOKEN"
```

You get the drop record for `/blocked`, including the filter that matched and the structured reason it was dropped.

## Next

- Put a filter in front of a real service: [Installing a filter](../operations/installing-a-filter.md).
- Write your own rules: [Rule model](../filtering/rules.md) and [Examples](../filtering/examples.md).
- Run it for real: [Deployment](../operations/deployment.md).
- Tune behavior: [Environment variables](../configuration/env.md).
