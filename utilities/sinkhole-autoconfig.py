#!/usr/bin/env python3
"""sinkhole-autoconfig — drop a sinkhole filter into a service's docker-compose.

Given a service's docker-compose file, this rewrites it so that **every public
(host-published) TCP port is routed through a single sinkhole filter** instead of
hitting the service container directly:

  * each guarded service loses its host `ports:` and becomes internal-only
    (`expose:`), reachable only on the compose network;
  * one `sinkhole-filter-<name>` service is added that republishes those public
    ports and forwards each to its original upstream container
    (`LISTEN_MAP=public:host:upstream,…`), inspecting every message;
  * the filter references the SCP'd `sinkhole` executable (bind-mounted) and keeps
    its DB in a local `./sinkhole-data` directory.

The original compose is backed up to `<file>.bak` and rewritten in place.

stdlib + pyyaml only. One-shot, but it shows the before/after port plan and asks
for confirmation before writing (use --yes to skip).
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import secrets
import sys
from pathlib import Path

try:
    import yaml
except ImportError:
    sys.stderr.write(
        "error: PyYAML is required (pip install pyyaml). If this box has no pip,\n"
        "       copy the directory to a machine that has it, run the tool there,\n"
        "       and copy the patched compose back.\n"
    )
    sys.exit(2)


class _IndentDumper(yaml.SafeDumper):
    """Indent block sequence items *under* their mapping key, e.g.

        volumes:
          - a:b

    instead of pyyaml's default flat alignment (dash level with the key). Matches
    the conventional docker-compose style.
    """

    def increase_indent(self, flow=False, indentless=False):
        return super().increase_indent(flow, False)

# ── colors ──────────────────────────────────────────────────────────────────
_TTY = sys.stdout.isatty()


def _c(code: str, s: str) -> str:
    return f"\033[{code}m{s}\033[0m" if _TTY else s


def bold(s):   return _c("1", s)
def dim(s):    return _c("2", s)
def red(s):    return _c("31", s)
def green(s):  return _c("32", s)
def yellow(s): return _c("33", s)
def blue(s):   return _c("36", s)
def mag(s):    return _c("35", s)


def info(s):  print(f"{blue('▸')} {s}")
def warn(s):  print(f"{yellow('!')} {s}")
def err(s):   print(f"{red('✗')} {s}", file=sys.stderr)
def good(s):  print(f"{green('✓')} {s}")


# ── compose / executable discovery ──────────────────────────────────────────
COMPOSE_NAMES = ["docker-compose.yml", "docker-compose.yaml", "compose.yml", "compose.yaml"]
EXE_NAMES = ["sinkhole-filter", "filter", "sinkhole"]


def find_compose(start: Path) -> Path | None:
    for n in COMPOSE_NAMES:
        p = start / n
        if p.is_file():
            return p
    return None


def find_executable(start: Path) -> Path | None:
    """Search 'sinkhole-y' locations: ./sinkhole, ../sinkhole, /sinkhole, and the
    cwd itself, up a couple of levels."""
    candidates: list[Path] = []
    for base in [start, start.parent, start.parent.parent, Path("/sinkhole"), Path("/opt/sinkhole")]:
        candidates.append(base / "sinkhole")
        candidates.append(base)
    seen = set()
    for d in candidates:
        d = d.resolve()
        if d in seen or not d.is_dir():
            continue
        seen.add(d)
        for name in EXE_NAMES:
            p = d / name
            if p.is_file() and os.access(p, os.X_OK):
                return p
    return None


# ── port parsing ────────────────────────────────────────────────────────────
class Publish:
    """One host-published TCP port: host `ext` -> container `target` on service."""
    def __init__(self, service: str, ext: int, target: int):
        self.service = service
        self.ext = ext
        self.target = target

    def __repr__(self):
        return f"{self.ext}->{self.service}:{self.target}"


def _expand_range(a: str, b: str):
    """Expand 'EXT' or 'EXT-EXT2' against 'INT'/'INT-INT2' into (ext,int) pairs."""
    if "-" in a and "-" in b:
        a0, a1 = (int(x) for x in a.split("-", 1))
        b0, b1 = (int(x) for x in b.split("-", 1))
        if (a1 - a0) != (b1 - b0):
            raise ValueError(f"port range length mismatch: {a} vs {b}")
        return list(zip(range(a0, a1 + 1), range(b0, b1 + 1)))
    return [(int(a), int(b))]


def parse_port_entry(service: str, entry) -> tuple[list[Publish], list[str]]:
    """Return (publishes, warnings) for one `ports:` entry. Non-TCP / non-host /
    unsupported forms are skipped with a warning rather than guessed."""
    warns: list[str] = []
    # long form: {target, published, protocol, host_ip, mode}
    if isinstance(entry, dict):
        proto = str(entry.get("protocol", "tcp")).lower()
        if proto != "tcp":
            return [], [f"{service}: skipped non-TCP port {entry}"]
        published, target = entry.get("published"), entry.get("target")
        if published is None or target is None:
            return [], [f"{service}: skipped port without published/target {entry}"]
        return [Publish(service, int(published), int(target))], warns

    s = str(entry).strip()
    proto = "tcp"
    if "/" in s:
        s, proto = s.rsplit("/", 1)
        if proto.lower() != "tcp":
            return [], [f"{service}: skipped non-TCP port {entry!r}"]
    parts = s.split(":")
    if len(parts) == 1:
        return [], [f"{service}: skipped {entry!r} (container-only / ephemeral host port — no fixed public port)"]
    if len(parts) == 2:
        a, b = parts
    elif len(parts) == 3:  # IP:EXT:INT — drop the IP binding, republish on all ifaces
        _, a, b = parts
        warns.append(f"{service}: {entry!r} was bound to a specific host IP; the filter republishes on all interfaces")
    else:
        return [], [f"{service}: skipped malformed port {entry!r}"]
    try:
        pubs = [Publish(service, e, t) for e, t in _expand_range(a, b)]
    except ValueError as ex:
        return [], [f"{service}: skipped {entry!r} ({ex})"]
    return pubs, warns


def analyze(compose: dict) -> tuple[list[Publish], list[str]]:
    """Collect every host-published TCP port across all services."""
    pubs: list[Publish] = []
    warns: list[str] = []
    for name, svc in (compose.get("services") or {}).items():
        if not isinstance(svc, dict):
            continue
        for entry in svc.get("ports") or []:
            p, w = parse_port_entry(name, entry)
            pubs.extend(p)
            warns.extend(w)
    return pubs, warns


# ── patching ────────────────────────────────────────────────────────────────
IN_CONTAINER_EXE = "/sinkhole/sinkhole-filter"


def build_patch(compose: dict, pubs: list[Publish], *, filter_name: str,
                exe_host_path: str, base_image: str, control_port: int,
                control_host_port: int, body_limit: int, token_hash: str | None,
                debug: bool, data_dir: str) -> dict:
    """Return a NEW compose dict with services made internal + filter added."""
    out = json.loads(json.dumps(compose))  # deep copy
    services = out.setdefault("services", {})

    guarded_services = sorted({p.service for p in pubs})
    taken_ext = {p.ext for p in pubs}

    # 1) make each guarded service internal-only: drop the taken host ports, expose targets
    for sname in guarded_services:
        svc = services[sname]
        kept = []
        for entry in svc.get("ports") or []:
            parsed, _ = parse_port_entry(sname, entry)
            if parsed and all(pp.ext in taken_ext for pp in parsed):
                continue  # taken over by the filter
            kept.append(entry)
        if kept:
            svc["ports"] = kept
        else:
            svc.pop("ports", None)
        targets = sorted({str(p.target) for p in pubs if p.service == sname})
        exposed = {str(x) for x in (svc.get("expose") or [])}
        svc["expose"] = sorted(exposed | set(targets), key=lambda x: int(x))

    # 2) build the filter service
    listen_map = ",".join(f"{p.ext}:{p.service}:{p.target}" for p in sorted(pubs, key=lambda p: p.ext))
    env = {
        "SINKHOLE_FILTER_LISTEN_MAP": listen_map,
        "SINKHOLE_FILTER_CONTROL_BIND": f"0.0.0.0:{control_port}",
        "SINKHOLE_FILTER_DB_PATH": f"/sinkhole-data/{filter_name}.db",
        "SINKHOLE_FILTER_BODY_LIMIT_BYTES": str(body_limit),
    }
    if debug:
        env["SINKHOLE_FILTER_DEBUG"] = "true"
    else:
        env["SINKHOLE_FILTER_AUTH_TOKEN_HASH"] = token_hash  # type: ignore[assignment]

    port_pubs = [f"{p.ext}:{p.ext}" for p in sorted(pubs, key=lambda p: p.ext)]
    port_pubs.append(f"{control_host_port}:{control_port}")

    services[f"sinkhole-filter-{filter_name}"] = {
        "image": base_image,
        "restart": "unless-stopped",
        "depends_on": guarded_services,
        "entrypoint": [IN_CONTAINER_EXE],
        "environment": env,
        "volumes": [
            f"{exe_host_path}:{IN_CONTAINER_EXE}:ro",
            f"{data_dir}:/sinkhole-data",
        ],
        "ports": port_pubs,
    }
    return out


# ── rendering ───────────────────────────────────────────────────────────────
def render_plan(filter_name: str, pubs: list[Publish], control_host_port: int,
                control_port: int):
    fbox = green("▣ filter")
    col = 30  # before-column width (measured on plain text, then colored)

    def row(plain_before: str, colored_before: str, colored_after: str):
        pad = " " * max(2, col - len(plain_before))
        print(f"  {colored_before}{pad}{colored_after}")

    print()
    print(bold(f"  {filter_name} — sinkhole port plan"))
    print(dim("  " + "─" * 64))
    row("before", bold("before"), bold("after"))
    for p in sorted(pubs, key=lambda p: p.ext):
        pb = f":{p.ext} -> {p.service}:{p.target}"
        cb = f"{blue(':'+str(p.ext))} {dim('→')} {p.service}:{p.target}"
        ca = f"{blue(':'+str(p.ext))} {dim('→')} {fbox} {dim('→')} {mag(p.service+':'+str(p.target))}"
        row(pb, cb, ca)
    # control API row (no 'before' equivalent)
    ca = f"{blue(':'+str(control_host_port))} {dim('→')} {fbox} {dim('(control API)')}"
    row("(new)", dim("(new)"), ca)
    print(dim("  " + "─" * 64))
    n_svc = len({p.service for p in pubs})
    print(dim(f"  1 filter, {len(pubs)} guarded port(s) across {n_svc} upstream(s), now internal-only"))
    print()


# ── main ────────────────────────────────────────────────────────────────────
def main() -> int:
    ap = argparse.ArgumentParser(description="Route a docker-compose service's public ports through a sinkhole filter.")
    ap.add_argument("-f", "--file", help="compose file (default: autodetect in --dir)")
    ap.add_argument("-d", "--dir", default=".", help="service directory (default: cwd)")
    ap.add_argument("-n", "--name", help="filter/challenge name (default: directory name)")
    ap.add_argument("-e", "--exec", dest="exe", help="path to the sinkhole executable (default: autodetect)")
    ap.add_argument("--control-port", type=int, default=7001, help="filter control port (container, default 7001)")
    ap.add_argument("--control-host-port", type=int, help="host port for the control API (default: same as --control-port)")
    ap.add_argument("--base-image", default="debian:bookworm-slim", help="base image to run the binary in")
    ap.add_argument("--body-limit", type=int, default=1048576, help="BODY_LIMIT_BYTES (default 1 MiB)")
    ap.add_argument("--data-dir", default="./sinkhole-data", help="host dir for the filter DB (default ./sinkhole-data)")
    ap.add_argument("--token", help="filter auth token (default: generate one)")
    ap.add_argument("--debug", action="store_true", help="run the filter in DEBUG (no auth) — testing only")
    ap.add_argument("-y", "--yes", action="store_true", help="apply without confirmation")
    args = ap.parse_args()

    sdir = Path(args.dir).resolve()
    if not sdir.is_dir():
        err(f"not a directory: {sdir}"); return 2

    compose_path = Path(args.file).resolve() if args.file else find_compose(sdir)
    if not compose_path or not compose_path.is_file():
        err(f"no compose file found in {sdir} (looked for {', '.join(COMPOSE_NAMES)})"); return 2

    name = args.name or sdir.name
    info(f"service name:   {bold(name)}")
    info(f"compose file:   {compose_path}")

    # executable
    exe = Path(args.exe).resolve() if args.exe else find_executable(sdir)
    if not exe or not exe.is_file():
        err("could not find the sinkhole executable; pass --exec <path> "
            "(searched ./sinkhole, ../sinkhole, /sinkhole, /opt/sinkhole)")
        return 2
    info(f"executable:     {exe}")

    # parse
    with open(compose_path) as fh:
        compose = yaml.safe_load(fh) or {}
    if "services" not in compose:
        err("compose has no `services:` block"); return 2

    pubs, warns = analyze(compose)
    for w in warns:
        warn(w)
    if not pubs:
        err("no host-published TCP ports found — nothing to guard"); return 1

    # reject duplicate public ports (prod ports are unique; ambiguous to route)
    seen: dict[int, str] = {}
    dups = []
    for p in pubs:
        if p.ext in seen and seen[p.ext] != p.service:
            dups.append(p.ext)
        seen[p.ext] = p.service
    if dups:
        err(f"duplicate public port(s) {sorted(set(dups))} across services — refusing to patch "
            "(production ports must be unique). Resolve the collision and re-run.")
        return 1

    control_host_port = args.control_host_port or args.control_port
    if control_host_port in {p.ext for p in pubs}:
        err(f"control host port {control_host_port} collides with a guarded public port; pass --control-host-port")
        return 1

    # auth
    token = None
    token_hash = None
    if not args.debug:
        token = args.token or secrets.token_hex(16)
        token_hash = hashlib.sha256(token.encode()).hexdigest()

    # show plan
    render_plan(name, pubs, control_host_port, args.control_port)
    if args.debug:
        warn("DEBUG mode: filter auth is DISABLED (testing only)")
    else:
        info(f"auth token (give this to the controller): {bold(token)}")

    if not args.yes:
        try:
            ans = input(f"  {bold('Apply this rewrite?')} {dim('[y/N]')} ").strip().lower()
        except EOFError:
            ans = "n"
        if ans not in {"y", "yes"}:
            warn("aborted; no changes written"); return 0

    # patch + write (back up first)
    patched = build_patch(
        compose, pubs, filter_name=name, exe_host_path=str(exe),
        base_image=args.base_image, control_port=args.control_port,
        control_host_port=control_host_port, body_limit=args.body_limit,
        token_hash=token_hash, debug=args.debug, data_dir=args.data_dir,
    )
    bak = compose_path.with_suffix(compose_path.suffix + ".bak")
    if not bak.exists():
        bak.write_bytes(compose_path.read_bytes())
        good(f"backed up original -> {bak.name}")
    else:
        warn(f"{bak.name} already exists; leaving it untouched")
    Path(sdir / args.data_dir).mkdir(parents=True, exist_ok=True)
    with open(compose_path, "w") as fh:
        yaml.dump(patched, fh, Dumper=_IndentDumper, sort_keys=False,
                  default_flow_style=False, width=120, indent=2)
    good(f"patched {compose_path.name} — {len(pubs)} port(s) now routed through sinkhole-filter-{name}")

    # controller wiring hint
    if not args.debug:
        entry = {"id": name, "url": f"http://<this-box>:{control_host_port}", "token": token, "name": name}
        print(dim("\n  controller SERVICES entry (collect these for the orchestrator):"))
        print("  " + json.dumps(entry))
    print(dim("\n  next: docker compose up -d --build\n"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
