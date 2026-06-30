#!/usr/bin/env python3
"""sinkhole traffic client — manual, real-time rule poking.

Open a channel to a guarded port, fire a payload, and read the verdict the
filter renders (allow vs drop, inferred from whether the connection survives —
see ``traffic_common`` for the detection model). Built for the loop you actually
do when writing rules: *fire → look → tweak → fire again*.

Two ways to drive it:

  Interactive (default — what you'll use most):
      python docker/traffic_client.py
    Pick a target, pick a payload (or a random/scenario mode), fire, then
    repeat / vary a parameter / switch target without restarting.

  One-shot CLI (scriptable):
      python docker/traffic_client.py --target gadget-echo --payload get_blocked
      python docker/traffic_client.py --target sea-echo --mode random --rounds 5
      python docker/traffic_client.py --target gadget-echo --mode scenario --scenario probe
      python docker/traffic_client.py --host 1.2.3.4 --port 6001 --method GET --path /x
      python docker/traffic_client.py --list

Multi-round (`--rounds N`) reuses ONE TCP connection (HTTP keep-alive / raw
loop), so it exercises connection-id / reply-chain tracking and session timing
matchers. `--interval-ms` spaces the rounds (drives session.idle).

stdlib-only.
"""
from __future__ import annotations

import argparse
import sys
from dataclasses import dataclass

import traffic_common as tc


# ─────────────────────────────────────────────────────────────────────────────
#  Reporting
# ─────────────────────────────────────────────────────────────────────────────
def _kind_tag(kind: str) -> str:
    return tc.col(f"{kind:<4}", tc.CYN if kind == "http" else tc.MAG)


def _print_round(i: int, p: tc.Payload | None, r: tc.RoundResult) -> None:
    label = p.label if p else "(no payload)"
    status = tc.col(f"[{r.status}]", tc.DIM) if r.status is not None else "     "
    num = tc.col(f"{i + 1:>3}", tc.GRY)
    print(f"  {num}  {r.verdict.glyph} {status}  {label}  {tc.col('— ' + r.note, tc.DIM)}")


def _print_summary(results: list[tc.RoundResult]) -> None:
    n = len(results)
    a = sum(1 for x in results if x.verdict == tc.Verdict.ALLOW)
    d = sum(1 for x in results if x.verdict == tc.Verdict.DROP)
    t = sum(1 for x in results if x.verdict == tc.Verdict.TIMEOUT)
    e = sum(1 for x in results if x.verdict == tc.Verdict.ERROR)
    print(f"  {tc.col(f'{n} rounds', tc.BOLD)} → "
          f"{tc.col(f'{a} allow', tc.GRN)}, {tc.col(f'{d} drop', tc.RED)}, "
          f"{tc.col(f'{t} timeout', tc.YEL)}, {tc.col(f'{e} error', tc.MAG)}\n")


@dataclass
class Session:
    target: tc.Target
    payloads: list[tc.Payload]
    rounds: int
    interval: int
    timeout: float
    flush_threshold: int

    @property
    def payload_label(self) -> str:
        if len(self.payloads) == 1:
            p = self.payloads[0]
            return f"{p.label}  {tc.col(f'({p.capability})', tc.DIM)}"
        return tc.col(f"sequence of {len(self.payloads)}", tc.BOLD)


def _fire(s: Session) -> None:
    # One payload repeated N rounds == a single keep-alive session (reuse=True);
    # a multi-payload sequence == independent probes, each on its own connection.
    if len(s.payloads) > 1:
        seq, reuse = s.payloads, False
    else:
        seq, reuse = s.payloads * max(1, s.rounds), True
    arrow = tc.col("▶", tc.CYN)
    print(f"\n{arrow} {tc.col(s.target.name, tc.BOLD)} "
          f"{tc.col(f'{s.target.host}:{s.target.port}', tc.DIM)}  "
          f"{tc.col(f'×{len(seq)}', tc.DIM)}")
    results = tc.run_sequence(s.target, seq, timeout=s.timeout, interval_ms=s.interval,
                              on_result=_print_round, reuse=reuse,
                              flush_threshold=s.flush_threshold)
    _print_summary(results)


# ─────────────────────────────────────────────────────────────────────────────
#  Listing
# ─────────────────────────────────────────────────────────────────────────────
def _list() -> None:
    print(tc.col("\nTargets", tc.BOLD), tc.col("(docker-compose.test.yaml stack)", tc.DIM))
    for t in tc.STACK_TARGETS:
        print(f"  {tc.col(f'{t.name:<16}', tc.BOLD)} "
              f"{t.host}:{t.port:<6} {_kind_tag(t.kind)}  {tc.col(t.desc, tc.DIM)}")
    print(tc.col("\nPayloads", tc.BOLD))
    for p in tc.CATALOG:
        print(f"  {_kind_tag(p.kind)} {tc.col(f'{p.key:<16}', tc.CYN)} "
              f"{tc.col(f'{p.capability:<22}', tc.DIM)} {p.label}")
    print(tc.col("\nScenarios", tc.BOLD))
    for name, keys in tc.SCENARIOS.items():
        print(f"  {tc.col(f'{name:<10}', tc.BOLD)} {tc.col(f'{len(keys)} rounds:', tc.DIM)} "
              f"{', '.join(dict.fromkeys(keys))}")
    print()


# ─────────────────────────────────────────────────────────────────────────────
#  Interactive menu
# ─────────────────────────────────────────────────────────────────────────────
def _ask(prompt: str, default: str = "") -> str:
    suffix = tc.col(f" [{default}]", tc.DIM) if default else ""
    try:
        ans = input(f"{tc.col('·', tc.CYN)} {prompt}{suffix}: ").strip()
    except EOFError:
        print()
        return default
    return ans or default


def _header(s: Session) -> None:
    rule = tc.col("─" * 16, tc.GRY)
    tls = tc.col(" tls", tc.MAG) if s.target.tls else ""
    print(f"\n{tc.col('═══', tc.CYN)} {tc.col('sinkhole traffic', tc.BOLD, tc.CYN)} "
          f"{tc.col('═══', tc.CYN)}")
    print(f" {tc.col('target ', tc.GRY)} {tc.col(s.target.name, tc.BOLD)}{tls} "
          f"{tc.col(f'→ {s.target.host}:{s.target.port}', tc.DIM)}")
    print(f" {tc.col('payload', tc.GRY)} {s.payload_label}")
    print(f" {tc.col('config ', tc.GRY)} rounds {tc.col(str(s.rounds), tc.BOLD)}   "
          f"interval {tc.col(f'{s.interval}ms', tc.BOLD)}   "
          f"timeout {tc.col(f'{s.timeout}s', tc.BOLD)}")
    del rule


def _menu(*items: tuple[str, str]) -> None:
    parts = [f"{tc.col(f'[{k}]', tc.CYN, tc.BOLD)} {label}" for k, label in items]
    print("  " + "   ".join(parts))


def _choose_target() -> tc.Target | None:
    print(tc.col("\nTargets:", tc.BOLD))
    for i, t in enumerate(tc.STACK_TARGETS):
        tls = tc.col(" tls", tc.MAG) if t.tls else ""
        print(f"  {tc.col(f'{i:>2}', tc.CYN)}  {tc.col(f'{t.name:<16}', tc.BOLD)}"
              f"{tls} {tc.col(f'{t.host}:{t.port}', tc.DIM)}  {tc.col(t.desc, tc.DIM)}")
    print(f"  {tc.col(' c', tc.CYN)}  custom host:port")
    sel = _ask("target (number / c / blank=cancel)")
    if not sel:
        return None
    if sel == "c":
        host = _ask("host", "localhost")
        port = int(_ask("port", "6001"))
        tls = _ask("tls? y/N", "n").lower().startswith("y")
        cacert = tc.DEFAULT_CACERT if tls else None
        return tc.Target("custom", host, port, "tls" if tls else "http", tls=tls, cacert=cacert)
    try:
        return tc.STACK_TARGETS[int(sel)]
    except (ValueError, IndexError):
        print(tc.col("  ? not a valid choice", tc.YEL))
        return _choose_target()


def _choose_specific() -> tc.Payload | None:
    """Catalog picker, grouped by capability for less clutter."""
    last_cap = None
    for i, p in enumerate(tc.CATALOG):
        if p.capability != last_cap:
            print(tc.col(f"  {p.capability}", tc.DIM))
            last_cap = p.capability
        print(f"    {tc.col(f'{i:>2}', tc.CYN)}  {_kind_tag(p.kind)} {p.label}")
    sel = _ask("payload # (blank=cancel)")
    if not sel:
        return None
    try:
        return tc.CATALOG[int(sel)]
    except (ValueError, IndexError):
        print(tc.col("  ? not a valid choice", tc.YEL))
        return _choose_specific()


def _choose_payloads(target: tc.Target) -> tuple[list[tc.Payload], int] | None:
    """Returns (payloads, rounds) or None if cancelled."""
    print(tc.col("\nPayload source:", tc.BOLD))
    _menu(("p", "specific payload"), ("r", "random mix"),
          ("s", "scenario"), ("v", "custom request"))
    src = _ask("source", "p")

    if src == "r":
        n = int(_ask("rounds", "5"))
        return (tc.random_payloads(target, n), n)

    if src == "s":
        names = list(tc.SCENARIOS)
        for i, name in enumerate(names):
            keys = tc.SCENARIOS[name]
            print(f"  {tc.col(f'{i:>2}', tc.CYN)}  {tc.col(f'{name:<10}', tc.BOLD)} "
                  f"{tc.col(f'{len(keys)} rounds', tc.DIM)}")
        idx = int(_ask("scenario #", "0"))
        keys = tc.SCENARIOS[names[idx]]
        return ([tc.payload_by_key(k) for k in keys], len(keys))

    if src == "v":
        return ([_build_custom()], int(_ask("rounds", "1")))

    p = _choose_specific()
    if p is None:
        return None
    rounds = int(_ask("rounds (same connection)", "1"))
    return ([p], rounds)


def _build_custom(base: tc.Payload | None = None) -> tc.Payload:
    kind = _ask("kind http/raw", base.kind if base else "http")
    if kind == "raw":
        text = _ask("raw bytes (text)", (base.raw.decode("latin-1") if base else "PING\\n"))
        return tc.Payload("custom", "custom raw", "raw", "stream",
                          raw=text.encode("latin-1").replace(b"\\n", b"\n").replace(b"\\r", b"\r"))
    method = _ask("method", base.method if base else "GET")
    path = _ask("path", base.path if base else "/")
    hdr_raw = _ask("headers (comma 'K: V', blank=none)",
                   ", ".join(f"{k}: {v}" for k, v in (base.headers.items() if base else [])))
    body_txt = _ask("body (text, blank=none)", (base.body.decode("latin-1") if base else ""))
    headers = tc.parse_kv_headers([h for h in hdr_raw.split(",") if h.strip()])
    return tc.Payload("custom", f"custom {method} {path}", "http", "custom",
                      method=method, path=path, headers=headers, body=body_txt.encode("latin-1"))


def interactive(timeout: float, flush_threshold: int = tc.DEFAULT_FLUSH_THRESHOLD) -> None:
    print(tc.col("sinkhole traffic client", tc.BOLD, tc.CYN),
          tc.col("— interactive (Ctrl-D / blank to back out)", tc.DIM))
    target = _choose_target()
    if target is None:
        return
    chosen = _choose_payloads(target)
    if chosen is None:
        return
    payloads, rounds = chosen
    interval = int(_ask("interval between rounds (ms)", "0"))
    s = Session(target, payloads, rounds, interval, timeout, flush_threshold)

    while True:
        _header(s)
        _menu(("⏎/r", "fire"), ("n", "new payload"), ("v", "vary"),
              ("t", "target"), ("o", "options"), ("q", "quit"))
        cmd = _ask("›", "r").lower()
        if cmd in ("q",):
            return
        if cmd in ("r", ""):
            _fire(s)
        elif cmd == "v":
            base = s.payloads[0] if s.payloads else None
            s.payloads = [_build_custom(base)]
            s.rounds = int(_ask("rounds", str(s.rounds)))
        elif cmd == "n":
            chosen = _choose_payloads(s.target)
            if chosen is not None:
                s.payloads, s.rounds = chosen
        elif cmd == "t":
            nt = _choose_target()
            if nt is not None:
                s.target = nt
                chosen = _choose_payloads(nt)
                if chosen is not None:
                    s.payloads, s.rounds = chosen
        elif cmd == "o":
            s.rounds = int(_ask("rounds", str(s.rounds)))
            s.interval = int(_ask("interval (ms)", str(s.interval)))
            s.timeout = float(_ask("timeout (s)", str(s.timeout)))
        else:
            print(tc.col("  ? unknown command", tc.YEL))


# ─────────────────────────────────────────────────────────────────────────────
#  CLI
# ─────────────────────────────────────────────────────────────────────────────
def _resolve_target(args) -> tc.Target | None:
    if args.target:
        t = tc.target_by_name(args.target)
        if t is None:
            print(f"unknown target '{args.target}' (see --list)", file=sys.stderr)
            return None
        return t
    if args.host:
        tls = args.tls
        return tc.Target("cli", args.host, args.port, "tls" if tls else "http",
                         tls=tls, cacert=(args.cacert or tc.DEFAULT_CACERT) if tls else None)
    return None


def main() -> int:
    ap = argparse.ArgumentParser(description="sinkhole manual traffic client")
    ap.add_argument("--list", action="store_true", help="list targets/payloads/scenarios")
    ap.add_argument("-i", "--interactive", action="store_true", help="force interactive menu")
    # target selection
    ap.add_argument("--target", help="predefined stack target name (see --list)")
    ap.add_argument("--host")
    ap.add_argument("--port", type=int, default=6001)
    ap.add_argument("--tls", action="store_true")
    ap.add_argument("--cacert", help=f"CA cert for TLS verify (default {tc.DEFAULT_CACERT})")
    # what to send
    ap.add_argument("--mode", choices=["payload", "random", "scenario"], default="payload")
    ap.add_argument("--payload", help="payload key (mode=payload)")
    ap.add_argument("--scenario", help="scenario name (mode=scenario)")
    # ad-hoc request (overrides catalog)
    ap.add_argument("--method")
    ap.add_argument("--path", default="/")
    ap.add_argument("--header", action="append", default=[], help="'K: V' (repeatable)")
    ap.add_argument("--data", help="request body (text)")
    ap.add_argument("--raw", help="send raw bytes instead of HTTP (text, \\n honored)")
    # connection control
    ap.add_argument("--rounds", type=int, default=1, help="rounds on one connection")
    ap.add_argument("--interval-ms", type=int, default=0)
    ap.add_argument("--timeout", type=float, default=tc.DEFAULT_TIMEOUT)
    ap.add_argument("--flush-threshold", type=int, default=tc.DEFAULT_FLUSH_THRESHOLD,
                    help="pad raw payloads past this many bytes to force a filter "
                         "flush (match the filter's BODY_LIMIT_BYTES)")
    args = ap.parse_args()

    if args.list:
        _list()
        return 0

    target = _resolve_target(args)
    if args.interactive or (target is None and not (args.method or args.raw)):
        interactive(args.timeout, args.flush_threshold)
        return 0
    if target is None:
        target = tc.Target("cli", args.host or "localhost", args.port,
                            "tls" if args.tls else "http", tls=args.tls,
                            cacert=args.cacert if args.tls else None)

    # build the payload list
    if args.raw is not None:
        raw = args.raw.encode("latin-1").replace(b"\\n", b"\n").replace(b"\\r", b"\r")
        payloads = [tc.Payload("cli-raw", "cli raw", "raw", "stream", raw=raw)]
    elif args.method:
        payloads = [tc.Payload("cli", f"{args.method} {args.path}", "http", "custom",
                               method=args.method, path=args.path,
                               headers=tc.parse_kv_headers(args.header),
                               body=(args.data or "").encode("latin-1"))]
    elif args.mode == "random":
        payloads = tc.random_payloads(target, args.rounds)
    elif args.mode == "scenario":
        name = args.scenario or "probe"
        if name not in tc.SCENARIOS:
            print(f"unknown scenario '{name}' (see --list)", file=sys.stderr)
            return 2
        payloads = [tc.payload_by_key(k) for k in tc.SCENARIOS[name]]
    else:
        key = args.payload or "get_root"
        p = tc.payload_by_key(key)
        if p is None:
            print(f"unknown payload '{key}' (see --list)", file=sys.stderr)
            return 2
        payloads = [p]

    _fire(Session(target, payloads, args.rounds, args.interval_ms, args.timeout,
                  args.flush_threshold))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        print()
        sys.exit(130)
