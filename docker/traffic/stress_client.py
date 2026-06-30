#!/usr/bin/env python3
"""sinkhole stress client — parallel load against one or many services.

Where ``traffic_client.py`` is for *looking* at single decisions, this is for
*pressure*: many concurrent connections firing large requests at all (or a
subset of) the stack's services at once, to shake out the proxy hot path and the
DB-actor batching under load.

Examples:
    # hammer every stack service for 15s, 32 conns each, 256 KiB bodies
    python docker/stress_client.py --services all --concurrency 32 --duration 15

    # just two services, fixed request count, 1 MiB bodies, 10 rounds/conn
    python docker/stress_client.py --services gadget-echo,sea-echo \
        --requests 5000 --size 1048576 --rounds 10

    # see what 'all' expands to
    python docker/stress_client.py --list

It reuses the channel + payload primitives from ``traffic_common``; the default
payload is a large ``PUT /upload`` (override with --payload). Allow/drop is
detected the same way (EOF == drop). stdlib-only.
"""
from __future__ import annotations

import argparse
import sys
import threading
import time
from dataclasses import dataclass, field

import traffic_common as tc


@dataclass
class Counters:
    allow: int = 0
    drop: int = 0
    timeout: int = 0
    error: int = 0
    rounds: int = 0
    latency_sum: float = 0.0
    latency_max: float = 0.0
    lock: threading.Lock = field(default_factory=threading.Lock)

    def record(self, verdict: tc.Verdict, dt: float) -> None:
        with self.lock:
            self.rounds += 1
            self.latency_sum += dt
            self.latency_max = max(self.latency_max, dt)
            if verdict == tc.Verdict.ALLOW:
                self.allow += 1
            elif verdict == tc.Verdict.DROP:
                self.drop += 1
            elif verdict == tc.Verdict.TIMEOUT:
                self.timeout += 1
            else:
                self.error += 1


def _worker(target: tc.Target, payload: tc.Payload, rounds: int, timeout: float,
            deadline: float | None, budget: list[int], budget_lock: threading.Lock,
            counters: Counters, stop: threading.Event, flush_threshold: int) -> None:
    """One worker: repeatedly open a connection and fire `rounds` payloads on it,
    until the time deadline passes, the request budget runs out, or stop is set."""
    while not stop.is_set():
        if deadline is not None and time.monotonic() >= deadline:
            return
        if budget is not None:
            with budget_lock:
                if budget[0] <= 0:
                    return
                budget[0] -= rounds
        ch = tc.Channel(target, timeout=timeout, flush_threshold=flush_threshold)
        try:
            ch.open()
        except OSError:
            counters.record(tc.Verdict.ERROR, 0.0)
            time.sleep(0.05)
            continue
        try:
            for _ in range(rounds):
                if stop.is_set():
                    break
                t0 = time.monotonic()
                r = ch.exchange(payload)
                counters.record(r.verdict, time.monotonic() - t0)
                if r.verdict in (tc.Verdict.DROP, tc.Verdict.ERROR):
                    break  # socket is dead; reconnect on the next outer loop
        finally:
            ch.close()


def _run_for_target(target: tc.Target, payload: tc.Payload, args) -> Counters:
    counters = Counters()
    stop = threading.Event()
    deadline = time.monotonic() + args.duration if args.duration else None
    budget = [args.requests] if args.requests else None
    budget_lock = threading.Lock()

    threads = [
        threading.Thread(
            target=_worker,
            args=(target, payload, args.rounds, args.timeout, deadline,
                  budget, budget_lock, counters, stop, args.flush_threshold),
            daemon=True,
        )
        for _ in range(args.concurrency)
    ]
    for t in threads:
        t.start()
    try:
        for t in threads:
            t.join()
    except KeyboardInterrupt:
        stop.set()
        for t in threads:
            t.join(timeout=2.0)
    return counters


def _report(target: tc.Target, c: Counters, elapsed: float) -> None:
    rps = c.rounds / elapsed if elapsed > 0 else 0.0
    avg = (c.latency_sum / c.rounds * 1000) if c.rounds else 0.0
    print(
        f"  {target.name:<16} {c.rounds:>7} rounds  {rps:>8.1f}/s  "
        f"avg {avg:>7.1f}ms  max {c.latency_max * 1000:>7.1f}ms  "
        f"\033[32m{c.allow}A\033[0m/\033[31m{c.drop}D\033[0m/"
        f"\033[33m{c.timeout}T\033[0m/\033[35m{c.error}E\033[0m"
    )


def _resolve_targets(spec: str) -> list[tc.Target]:
    if spec == "all":
        # Only the throughput-friendly echo ports — skip the deliberate `delay`
        # ports so latency stats aren't dominated by the mock upstream's sleep.
        return [t for t in tc.STACK_TARGETS if "delay" not in t.name and not t.tls]
    out: list[tc.Target] = []
    for name in spec.split(","):
        name = name.strip()
        if not name:
            continue
        t = tc.target_by_name(name)
        if t is None:
            print(f"unknown service '{name}' (see --list)", file=sys.stderr)
            continue
        out.append(t)
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description="sinkhole parallel stress client")
    ap.add_argument("--list", action="store_true", help="list stack services")
    ap.add_argument("--services", default="all",
                    help="'all' or comma-list of target names (see --list)")
    ap.add_argument("--concurrency", type=int, default=16, help="connections per service")
    ap.add_argument("--rounds", type=int, default=1, help="requests per connection")
    ap.add_argument("--duration", type=float, default=10.0, help="seconds (0 = use --requests)")
    ap.add_argument("--requests", type=int, default=0, help="total requests/service (overrides duration)")
    ap.add_argument("--size", type=int, default=262144, help="body size in bytes for the default payload")
    ap.add_argument("--payload", help="catalog payload key (overrides the default large PUT)")
    ap.add_argument("--timeout", type=float, default=tc.DEFAULT_TIMEOUT)
    ap.add_argument("--flush-threshold", type=int, default=tc.DEFAULT_FLUSH_THRESHOLD,
                    help="pad raw payloads past this many bytes (match filter BODY_LIMIT_BYTES)")
    args = ap.parse_args()

    if args.list:
        for t in tc.STACK_TARGETS:
            print(f"  {t.name:<16} {t.host}:{t.port:<6} {t.kind:<5} {t.desc}")
        return 0

    if args.requests:
        args.duration = 0.0

    targets = _resolve_targets(args.services)
    if not targets:
        print("no valid targets", file=sys.stderr)
        return 2

    if args.payload:
        payload = tc.payload_by_key(args.payload)
        if payload is None:
            print(f"unknown payload '{args.payload}' (see traffic_client --list)", file=sys.stderr)
            return 2
    else:
        payload = tc.Payload("stress", f"PUT /upload {args.size}B", "http", "load",
                             method="PUT", path="/upload", body=b"X" * args.size,
                             headers={"Content-Type": "application/octet-stream"})

    plan = "duration %.0fs" % args.duration if args.duration else f"{args.requests} req/svc"
    print(f"stress: {len(targets)} service(s), {args.concurrency} conns each, "
          f"{args.rounds} round(s)/conn, {plan}, payload='{payload.label}'")

    # Run services in parallel too (one thread-pool per target, all live at once).
    results: dict[str, tuple[Counters, float]] = {}
    threads: list[threading.Thread] = []
    lock = threading.Lock()

    def runner(t: tc.Target) -> None:
        t0 = time.monotonic()
        c = _run_for_target(t, payload, args)
        with lock:
            results[t.name] = (c, time.monotonic() - t0)

    for t in targets:
        th = threading.Thread(target=runner, args=(t,), daemon=True)
        th.start()
        threads.append(th)
    try:
        for th in threads:
            th.join()
    except KeyboardInterrupt:
        print("\ninterrupted — partial results:")

    print("\nresults:")
    tot = Counters()
    max_elapsed = 0.0
    for t in targets:
        if t.name not in results:
            continue
        c, elapsed = results[t.name]
        _report(t, c, elapsed)
        tot.allow += c.allow; tot.drop += c.drop
        tot.timeout += c.timeout; tot.error += c.error; tot.rounds += c.rounds
        max_elapsed = max(max_elapsed, elapsed)
    agg_rps = tot.rounds / max_elapsed if max_elapsed > 0 else 0.0
    print(f"\n  TOTAL {tot.rounds} rounds in {max_elapsed:.1f}s ({agg_rps:.0f}/s) — "
          f"{tot.allow} allow, {tot.drop} drop, {tot.timeout} timeout, {tot.error} error")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        print()
        sys.exit(130)
