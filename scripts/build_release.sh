#!/usr/bin/env bash
# Build the release artifacts for the current version into dist/<version>/:
#   - statically-linked musl binaries (filter + controller) for linux x86_64 and aarch64
#   - SHA256SUMS (combined) plus a per-binary <name>.sha256
#
# The web UI is embedded into the controller binary at compile time (rust-embed
# over frontend/dist), so the frontend is built FIRST, before the binaries, and
# there is no separate frontend artifact.
#
# Why musl + static: rusqlite is bundled (compiles SQLite in) and rustls uses ring,
# so a musl build is a single self-contained file that runs on any Linux host with
# no glibc-version surprises. That self-containment (plus tokio/hyper/axum/reqwest
# and the embedded web UI) is also why the binaries are tens of MB — it's mostly
# code, not symbols.
#
# Symbols: by default we KEEP them, so a crash backtrace shows function names. The
# release profile already omits DWARF line info, so symbols cost only a few MB and
# stripping them saves little on a binary this size. Set STRIP=1 for the smallest
# artifact (no symbol table).
#
# Requires:
#   - npm    — to build the web UI that gets embedded into the controller.
#   - cross  (cargo install cross --locked) + Docker — handles the per-arch musl
#     and bundled-SQLite C toolchains so this works for aarch64 without a local
#     cross C compiler.
#
# Run from anywhere in the repo: scripts/build_release.sh
set -euo pipefail
cd "$(dirname "$0")/.."

# All crates share one version; read it from a crate manifest.
VERSION="$(grep -m1 '^version = ' controller/Cargo.toml | cut -d'"' -f2)"
OUT="dist/${VERSION}"
echo "Building sinkhole v${VERSION} -> ${OUT}/"

if ! command -v cross >/dev/null 2>&1; then
  echo "error: 'cross' not found. Install it with:" >&2
  echo "         cargo install cross --locked" >&2
  echo "       (cross + Docker provide the musl + bundled-SQLite toolchains per arch.)" >&2
  exit 1
fi
if ! command -v npm >/dev/null 2>&1; then
  echo "error: 'npm' not found; it is required to build the web UI embedded in the controller." >&2
  exit 1
fi

# 1) Build the web UI first so rust-embed bakes it into the controller binary.
echo "── building web UI (embedded into the controller) ──"
( cd frontend && npm ci && npm run build )

# Optional symbol stripping (STRIP=1). Done at link by rustc, so it's arch-agnostic
# and also covers the cross-compiled aarch64 binary without an aarch64 strip tool.
# Default keeps symbols (useful backtraces); see the header note.
case "${STRIP:-}" in
  1|true|yes)
    echo "stripping symbols (STRIP=${STRIP})"
    export RUSTFLAGS="${RUSTFLAGS:-} -C strip=symbols"
    ;;
  *)
    echo "keeping symbols (set STRIP=1 to strip)"
    ;;
esac

# Each arch builds into its OWN target dir (set in the loop below). Two reasons:
# stay clear of the dev `target/` (host-glibc build artifacts), and never share
# host build-script/proc-macro binaries between the two cross images — they carry
# different glibc versions, so a tool compiled in one container fails with
# `GLIBC_x.y not found` when reused by the other.

# target triple -> short arch label used in the artifact file names
TARGETS=(
  "x86_64-unknown-linux-musl:x86_64"
  "aarch64-unknown-linux-musl:aarch64"
)

rm -rf "${OUT}"
mkdir -p "${OUT}"

# 2) Cross-compile both binaries per arch (the controller embeds the UI built above).
for entry in "${TARGETS[@]}"; do
  triple="${entry%%:*}"
  arch="${entry##*:}"
  echo "── building ${arch} (${triple}) ──"
  export CARGO_TARGET_DIR="target/release-cross/${arch}"
  # --locked: build exactly the committed Cargo.lock, so releases are reproducible.
  cross build --locked --release --target "${triple}" --bin filter --bin controller
  for bin in filter controller; do
    cp "${CARGO_TARGET_DIR}/${triple}/release/${bin}" "${OUT}/sinkhole-${bin}-linux-${arch}"
  done
done

# 3) Checksums: a combined SHA256SUMS (verify all with `sha256sum -c SHA256SUMS`)
# plus a per-artifact `<name>.sha256` for convenience on the release page.
echo "── checksums ──"
( cd "${OUT}"
  sha256sum sinkhole-* > SHA256SUMS
  for f in sinkhole-*; do sha256sum "${f}" > "${f}.sha256"; done
)

echo
echo "Release artifacts for v${VERSION}:"
ls -lh "${OUT}"
echo
echo "SHA256SUMS:"
cat "${OUT}/SHA256SUMS"
