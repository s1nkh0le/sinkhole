#!/usr/bin/env bash
# ───────────────────────────────────────────────────────
# Launch the Sinkhole WAF Web UI
# Usage:  ./webui.sh [port]       (default: 8888)
# ───────────────────────────────────────────────────────
set -euo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
WEBUI_DIR="${DIR}/webui"
PORT="${1:-8888}"

if [ ! -f "${WEBUI_DIR}/index.html" ]; then
  echo "[!] webui/index.html not found at ${WEBUI_DIR}"
  exit 1
fi

echo ""
echo "  ⬡  SINKHOLE WAF — Web UI"
echo "  ─────────────────────────"
echo "  Serving on:  http://localhost:${PORT}"
echo "  Webui dir:   ${WEBUI_DIR}"
echo "  Press Ctrl+C to stop"
echo ""

cd "${WEBUI_DIR}"
python3 -m http.server "${PORT}" --bind 127.0.0.1
