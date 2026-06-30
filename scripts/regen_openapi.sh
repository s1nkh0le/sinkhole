#!/usr/bin/env bash
# Regenerate the OpenAPI specs by booting both binaries and fetching /openapi.json,
# writing openapi_filter.json + openapi_ctrl.json into each output directory. Keeps
# the committed specs (and the frontend's generated types) in sync with the code.
#
# Usage:
#   scripts/regen_openapi.sh [OUTPUT_DIR ...]
#
#   # default: the two committed locations (docs site + frontend codegen source)
#   scripts/regen_openapi.sh
#   # or one/more custom dirs:
#   scripts/regen_openapi.sh /tmp/specs
set -euo pipefail
cd "$(dirname "$0")/.."

# Output dirs: the given args, or the two committed locations by default.
if [ "$#" -gt 0 ]; then
    OUT_DIRS=( "$@" )
else
    OUT_DIRS=( "docs/pages/reference/swagger" "docs/assets/openapi" )
fi

FILTER_PORT=17101
CTRL_PORT=17100
TMP_DB_DIR="$(mktemp -d)"
FILTER_PID=""
CTRL_PID=""

cleanup() {
    [ -n "$FILTER_PID" ] && kill "$FILTER_PID" 2>/dev/null || true
    [ -n "$CTRL_PID" ] && kill "$CTRL_PID" 2>/dev/null || true
    rm -rf "$TMP_DB_DIR"
}
trap cleanup EXIT

# Block until an endpoint answers, so we never race a slow boot (or hang forever).
wait_up() {
    local url="$1" tries=80
    until curl -fsS -o /dev/null "$url" 2>/dev/null; do
        tries=$((tries - 1))
        if [ "$tries" -le 0 ]; then
            echo "error: $url did not come up in time" >&2
            return 1
        fi
        sleep 0.25
    done
}

echo "building filter + controller (debug)…"
cargo build --bin filter --bin controller

# Boot both in DEBUG (auth disabled) on throwaway ports + DBs. The controller does
# not serve its UI here — we only need the API surface.
SINKHOLE_FILTER_UPSTREAM_HOST=127.0.0.1 \
SINKHOLE_FILTER_LISTEN_MAP="18080:80" \
SINKHOLE_FILTER_CONTROL_BIND="127.0.0.1:${FILTER_PORT}" \
SINKHOLE_FILTER_DEBUG=true \
SINKHOLE_FILTER_DB_PATH="${TMP_DB_DIR}/filter.db" \
./target/debug/filter >/dev/null 2>&1 &
FILTER_PID=$!

SINKHOLE_CONTROLLER_BIND="127.0.0.1:${CTRL_PORT}" \
SINKHOLE_CONTROLLER_DEBUG=true \
SINKHOLE_CONTROLLER_SERVE_FRONTEND=false \
SINKHOLE_CONTROLLER_DB_PATH="${TMP_DB_DIR}/ctrl.db" \
./target/debug/controller >/dev/null 2>&1 &
CTRL_PID=$!

wait_up "http://127.0.0.1:${FILTER_PORT}/health"
wait_up "http://127.0.0.1:${CTRL_PORT}/health"

for dst in "${OUT_DIRS[@]}"; do
    mkdir -p "$dst"
    curl -fsS "http://127.0.0.1:${FILTER_PORT}/openapi.json" -o "${dst}/openapi_filter.json"
    curl -fsS "http://127.0.0.1:${CTRL_PORT}/openapi.json"   -o "${dst}/openapi_ctrl.json"
    echo "  wrote ${dst}/openapi_filter.json, ${dst}/openapi_ctrl.json"
done

echo "OpenAPI specs regenerated."
