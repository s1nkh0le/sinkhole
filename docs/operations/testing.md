# Testing and development

Dev tasks live in the `justfile` (run `just` to list them). Cargo commands are workspace-wide.

## Build, test, lint

```bash
just build                    # cargo build --workspace
just test                     # cargo test --workspace
cargo test -p filter <name>   # run a single test (crate + name filter)
just check                    # cargo check --workspace
just fmt                      # cargo fmt --all
just clippy                   # cargo clippy --workspace --all-targets
```

CI (`.github/workflows/build_test.yaml`) runs two jobs:

- **lint**: `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings`. A warning fails the build.
- **build and test**: `cargo build` and `cargo test` on stable, beta, and nightly.

## The test stack

The test stack brings up a few mock services, a filter in front of each, and one controller, all on a single Docker network. It is self-contained, with tokens baked in.

```bash
docker compose -f docker-compose.test.yaml up --build   # or: just up
docker compose -f docker-compose.test.yaml down -v       # or: just down
```

See the [Quickstart](../usage/quickstart.md) for what it contains and how to drive it through the controller.

## Driving traffic

Two small Python clients (standard library only) under `docker/traffic/` exercise a running stack from the outside, the way a real client or checker would.

- `just traffic [ARGS]` runs `traffic_client.py`: send individual requests to poke rules live and watch each allow or drop decision.
- `just stress [ARGS]` runs `stress_client.py`: fire parallel load at one or many services at once, to see how the proxy and the database hold up under pressure.

Both talk to the public data ports, so they show what an attacker or the game's checker would see. To inspect what the filter decided, read the records back through the controller.

## OpenAPI specs

The committed specs in `docs/assets/openapi/` (and the Swagger reference pages), plus the frontend's generated types, are derived from the code. After changing any API request or response type, or any route:

```bash
just regen-openapi   # boots both binaries and writes fresh OpenAPI JSON into docs/
```

Keep the regenerated JSON with the code change so the API reference and the UI types stay in sync.

## Manual API exploration

Both binaries serve Swagger UI at `/docs` and the raw spec at `/openapi.json`, so you can exercise endpoints interactively (a bearer token is required for protected routes). A Postman or Hurl collection works equally well against the controller API.

## Where tests live

- Rule engine decisions: `filter/src/engine/dispatcher.rs`.
- HTTP parser edge cases: `filter/src/http/parser.rs`.
- Matchers and rule legality: `filter/src/matcher/` and `filter/src/rules/`.
- Token hashing and base types: `shared/src/`.
