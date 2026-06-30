# sinkhole — reproducible dev tasks. Run `just` to list targets.
# Requires: cargo; docker compose (stack targets); python3 (traffic/cert/autoconfig);
# npm + cross + Docker (release builds).

set shell := ["bash", "-uc"]

# List available recipes.
default:
    @just --list

# Build the whole workspace (debug).
build:
    cargo build --workspace

# Build the whole workspace (release).
build-release:
    cargo build --workspace --release

# Run all tests across the workspace.
test:
    cargo test --workspace

# Type-check without producing binaries.
check:
    cargo check --workspace

# Format the whole workspace.
fmt:
    cargo fmt --all

# Lint the whole workspace.
clippy:
    cargo clippy --workspace --all-targets

# Cross-compile the static musl release into dist/<version>/ (npm + cross + Docker).
release:
    bash scripts/build_release.sh

# sha256 of a plaintext token, for the *_AUTH_TOKEN_HASH env vars.
token-hash token:
    @printf '%s' "{{token}}" | sha256sum | cut -d' ' -f1

# Generate the test stack's throwaway TLS cert into docker/certs/ (run before `just up`).
cert:
    python3 utilities/sinkhole-gen-cert.py --dir docker/certs --generate-key --force --yes

# Bring the full TEST stack up: 4 challenge filters + echoes + controller + UI (run `just cert` first).
up:
    docker compose -f docker-compose.test.yaml up --build

# Tear down the test stack and remove its volumes.
down:
    docker compose -f docker-compose.test.yaml down -v

# Interactive traffic client (e.g. `just traffic --target gadget-echo --mode random`).
traffic *ARGS:
    python3 docker/traffic/traffic_client.py {{ARGS}}

# Parallel stress client (e.g. `just stress --services all --concurrency 32`).
stress *ARGS:
    python3 docker/traffic/stress_client.py {{ARGS}}

# Reroute a guarded service's published port through a filter (`just autoconfig --help`).
autoconfig *ARGS:
    python3 utilities/sinkhole-autoconfig.py {{ARGS}}

# Refresh the committed OpenAPI specs from the code (override dirs: `just regen-openapi /tmp/specs`).
regen-openapi *DIRS:
    bash scripts/regen_openapi.sh {{DIRS}}
