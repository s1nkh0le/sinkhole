#!/usr/bin/env python3
"""Shared primitives for the sinkhole traffic tools (stdlib-only).

This module is the common core behind two front-ends:

  * ``traffic_client.py`` — an interactive/CLI client for *manually* poking a
    filter's rules in real time (pick a target, fire a payload, watch the
    allow/drop verdict, repeat/vary).
  * ``stress_client.py``  — a parallel load generator that hammers one or many
    services at once.

It is deliberately protocol-light: sinkhole only guards a *TCP channel*, and on
that channel anything can flow (HTTP/1.1, raw ASCII, binary, an HTTP/2 preface,
…). So the building blocks here are "open a channel, push some bytes, read what
comes back, decide allow vs drop" — not a real HTTP client.

Allow/drop detection
--------------------
A filter ``Drop`` *kills the connection* (the proxy pump stops and the socket
dies). So the strong, reliable signal is **EOF / connection-close before a
complete reply == DROP**; a complete reply == ALLOW. A read timeout is only the
weak fallback (reported as ``timeout``, treated as a probable drop). Keep the
echo server's ``delay`` modes comfortably below ``DEFAULT_TIMEOUT`` or a slow but
allowed response will be misread as a drop.

stdlib-only: no third-party deps, matching ``echo_server.py``.
"""
from __future__ import annotations

import json
import os
import random
import socket
import ssl
import sys
from dataclasses import dataclass, field
from enum import Enum

# ── ANSI styling (auto-disabled when not a TTY or NO_COLOR is set) ────────────
_USE_COLOR = sys.stdout.isatty() and os.environ.get("NO_COLOR") is None


def _sgr(code: str) -> str:
    return code if _USE_COLOR else ""


RESET = _sgr("\033[0m")
BOLD = _sgr("\033[1m")
DIM = _sgr("\033[2m")
RED = _sgr("\033[31m")
GRN = _sgr("\033[32m")
YEL = _sgr("\033[33m")
BLU = _sgr("\033[34m")
MAG = _sgr("\033[35m")
CYN = _sgr("\033[36m")
GRY = _sgr("\033[90m")


def col(s: str, *codes: str) -> str:
    return f"{''.join(codes)}{s}{RESET}" if codes and _USE_COLOR else s

# Read budget for one reply. Must exceed the largest echo `delay_ms` in the
# stack (today: 1500ms on gadget:6002, 800ms on sea:8002) or an allowed-but-slow
# response gets misclassified as a drop.
DEFAULT_TIMEOUT = 4.0

# Raw (non-HTTP) traffic is only flushed + evaluated by the filter as a "raw
# stream chunk" once the buffered bytes exceed its body limit — otherwise it sits
# buffered until EOF, and an EOF tears the whole connection down before any echo
# returns (proxy.rs cancels the sibling pump). So to get a *readable* verdict we
# pad raw payloads past this threshold, forcing a mid-connection flush that keeps
# the socket alive. Defaults to the test stack's BODY_LIMIT_BYTES (1 MiB); raise
# it via --flush-threshold if your filter's body limit is higher.
DEFAULT_FLUSH_THRESHOLD = 1_048_576

# Default self-signed cert for the test stack's filter-terminated TLS port (7002).
# Resolved relative to this file (docker/traffic/) so it works from any CWD.
DEFAULT_CACERT = os.path.normpath(
    os.path.join(os.path.dirname(__file__), "..", "certs", "cert.pem")
)


# ─────────────────────────────────────────────────────────────────────────────
#  Verdict
# ─────────────────────────────────────────────────────────────────────────────
class Verdict(str, Enum):
    ALLOW = "allow"      # got a complete, well-formed reply
    DROP = "drop"        # connection closed before/within the reply (filter Drop)
    TIMEOUT = "timeout"  # no reply within the read budget (probable drop)
    ERROR = "error"      # couldn't even connect / transport failure

    @property
    def glyph(self) -> str:
        return {
            Verdict.ALLOW: col("✓ allow", BOLD, GRN),
            Verdict.DROP: col("✗ drop ", BOLD, RED),
            Verdict.TIMEOUT: col("… t/o  ", BOLD, YEL),
            Verdict.ERROR: col("! err  ", BOLD, MAG),
        }[self]


@dataclass
class RoundResult:
    verdict: Verdict
    status: int | None  # HTTP status code if a reply was parsed
    note: str           # short human note (e.g. "204 bytes", "EOF", reason)


# ─────────────────────────────────────────────────────────────────────────────
#  Targets
# ─────────────────────────────────────────────────────────────────────────────
@dataclass
class Target:
    name: str
    host: str
    port: int
    kind: str = "http"          # "http" | "raw" | "tls" — a hint for `random` mode
    tls: bool = False
    cacert: str | None = None
    desc: str = ""

    @property
    def label(self) -> str:
        flag = " (tls)" if self.tls else ""
        return f"{self.name}{flag} → {self.host}:{self.port}  {self.desc}"


# Predefined targets for the docker-compose.test.yaml stack, paired to the echo
# capability behind each filter port (see the compose file's ECHO_SPEC blocks).
STACK_TARGETS: list[Target] = [
    Target("gadget-echo",    "localhost", 6001, "http", desc="echo / general HTTP scopes"),
    Target("gadget-delay",   "localhost", 6002, "http", desc="delay 1500ms / timing"),
    Target("gadget-fixed",   "localhost", 6003, "http", desc="fixed 200 / status-whitelist"),
    Target("weather-echo",   "localhost", 7001, "http", desc="echo"),
    Target("weather-tls",    "localhost", 7002, "tls", tls=True, cacert=DEFAULT_CACERT,
           desc="filter-terminated TLS end-to-end"),
    Target("sea-echo",       "localhost", 8001, "http", desc="echo"),
    Target("sea-delay",      "localhost", 8002, "http", desc="delay 800ms / timing"),
    Target("sea-fixed403",   "localhost", 8003, "http", desc="fixed 403 / status-blacklist"),
    Target("sea-raw",        "localhost", 8004, "raw",  desc="raw TCP echo / stream scope"),
    Target("dutyfree-echo",  "localhost", 9001, "http", desc="echo"),
    Target("dutyfree-echo2", "localhost", 9002, "http", desc="echo"),
]


def target_by_name(name: str) -> Target | None:
    for t in STACK_TARGETS:
        if t.name == name:
            return t
    return None


# ─────────────────────────────────────────────────────────────────────────────
#  Payloads — a tagged catalog, each tagged with the engine capability it pokes
# ─────────────────────────────────────────────────────────────────────────────
@dataclass
class Payload:
    key: str
    label: str
    kind: str          # "http" | "raw"
    capability: str    # which scope / matcher this is meant to exercise
    # HTTP fields
    method: str = "GET"
    path: str = "/"
    headers: dict[str, str] = field(default_factory=dict)
    body: bytes = b""
    # raw field
    raw: bytes = b""


def _oversized_body(n: int = 1_100_000) -> bytes:
    # Above the stack's default BODY_LIMIT_BYTES (1 MiB) → forces the filter's
    # raw-stream-chunk path instead of an assembled HttpMessage.
    return b"A" * n


def _large_body(n: int = 65_536) -> bytes:
    return b"B" * n


# The catalog. `random` mode samples from here (filtered by target kind);
# the interactive menu lets you fire any of these at any target.
CATALOG: list[Payload] = [
    # ── HTTP: method / path scopes ──────────────────────────────────────────
    Payload("get_root", "GET / (benign baseline)", "http", "http.method/path",
            method="GET", path="/"),
    Payload("get_admin", "GET /admin (suspicious path)", "http", "http.path",
            method="GET", path="/admin"),
    Payload("get_traversal", "GET /../../etc/passwd (path traversal)", "http", "http.path",
            method="GET", path="/../../etc/passwd"),
    Payload("get_blocked", "GET /blocked (sample-ruleset blacklist hit)", "http", "http.path",
            method="GET", path="/blocked"),
    Payload("sqli_query", "GET /search?q=' OR '1'='1 (SQLi-ish query)", "http", "http.path",
            method="GET", path="/search?q=%27%20OR%20%271%27%3D%271"),
    Payload("xss_path", "GET /<script>alert(1)</script> (XSS-ish)", "http", "http.path",
            method="GET", path="/%3Cscript%3Ealert(1)%3C/script%3E"),
    Payload("delete_resource", "DELETE /resource/123", "http", "http.method",
            method="DELETE", path="/resource/123"),
    Payload("patch_resource", "PATCH /resource/123", "http", "http.method",
            method="PATCH", path="/resource/123", body=b'{"op":"noop"}',
            headers={"Content-Type": "application/json"}),
    # ── HTTP: header scope ──────────────────────────────────────────────────
    Payload("bad_useragent", "GET / with User-Agent: sqlmap/1.0", "http", "http.header",
            method="GET", path="/", headers={"User-Agent": "sqlmap/1.0"}),
    Payload("header_injection", "GET / with odd X-Forwarded-For", "http", "http.header",
            method="GET", path="/", headers={"X-Forwarded-For": "127.0.0.1, evil"}),
    # ── HTTP: body scope / size matcher / raw-chunk path ────────────────────
    Payload("post_json", "POST /api JSON body", "http", "http.body",
            method="POST", path="/api", body=b'{"user":"alice","action":"buy"}',
            headers={"Content-Type": "application/json"}),
    Payload("post_form", "POST /login form creds", "http", "http.body",
            method="POST", path="/login", body=b"username=admin&password=' OR 1=1--",
            headers={"Content-Type": "application/x-www-form-urlencoded"}),
    Payload("put_large", "PUT /upload 64 KiB body (size matcher)", "http", "http.body/size",
            method="PUT", path="/upload", body=_large_body(),
            headers={"Content-Type": "application/octet-stream"}),
    Payload("put_oversized", "PUT /upload 1.1 MiB body (> body limit → raw chunk)", "http",
            "stream (raw chunk)", method="PUT", path="/upload", body=_oversized_body(),
            headers={"Content-Type": "application/octet-stream"}),
    # ── raw / non-HTTP: stream scope ────────────────────────────────────────
    Payload("raw_text", "raw ASCII line 'PING'", "raw", "stream",
            raw=b"PING sinkhole\n"),
    Payload("raw_binary", "raw binary blob w/ deadbeef signature", "raw", "stream (bytes_hex)",
            raw=bytes([0x00, 0x01, 0x02]) + bytes.fromhex("deadbeef") + bytes(range(8))),
    Payload("raw_garbage", "raw random 256 bytes", "raw", "stream",
            raw=bytes(random.randint(0, 255) for _ in range(256))),
    Payload("h2_preface", "HTTP/2 client connection preface (degrade-to-raw)", "raw",
            "stream (h2c)", raw=b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n"),
]


def payload_by_key(key: str) -> Payload | None:
    for p in CATALOG:
        if p.key == key:
            return p
    return None


# Named scenarios: ordered payload keys to fire as one multi-round sequence.
SCENARIOS: dict[str, list[str]] = {
    "probe":  ["get_root", "get_admin", "get_traversal", "bad_useragent", "sqli_query"],
    "mixed":  ["get_root", "post_json", "get_blocked", "put_large", "delete_resource"],
    "burst":  ["get_root"] * 20,
    "binary": ["raw_text", "raw_binary", "h2_preface"],
}


def random_payloads(target: Target, n: int) -> list[Payload]:
    """Pick `n` random payloads appropriate for the target's channel kind."""
    if target.kind == "raw":
        pool = [p for p in CATALOG if p.kind == "raw"]
    else:
        pool = [p for p in CATALOG if p.kind == "http"]
    return [random.choice(pool) for _ in range(n)]


# ─────────────────────────────────────────────────────────────────────────────
#  Wire encoding + the channel
# ─────────────────────────────────────────────────────────────────────────────
def build_http(target: Target, p: Payload) -> bytes:
    """Serialize an HTTP/1.1 request with keep-alive (so multi-round reuses the
    socket). Caller-supplied headers override the defaults."""
    hdrs: dict[str, str] = {
        "Host": f"{target.host}:{target.port}",
        "User-Agent": "sinkhole-traffic/1.0",
        "Accept": "*/*",
        "Connection": "keep-alive",
    }
    hdrs.update(p.headers)
    if p.body:
        hdrs["Content-Length"] = str(len(p.body))
    head = f"{p.method} {p.path} HTTP/1.1\r\n"
    head += "".join(f"{k}: {v}\r\n" for k, v in hdrs.items())
    head += "\r\n"
    return head.encode("latin-1") + p.body


def _tls_context(cacert: str | None) -> ssl.SSLContext:
    if cacert:
        ctx = ssl.create_default_context(cafile=cacert)
        return ctx
    ctx = ssl.create_default_context()
    ctx.check_hostname = False
    ctx.verify_mode = ssl.CERT_NONE
    return ctx


class Channel:
    """One TCP (optionally TLS) connection to a target, reusable for N rounds."""

    def __init__(self, target: Target, timeout: float = DEFAULT_TIMEOUT,
                 flush_threshold: int = DEFAULT_FLUSH_THRESHOLD):
        self.target = target
        self.timeout = timeout
        self.flush_threshold = flush_threshold
        self.sock: socket.socket | None = None

    def open(self) -> None:
        raw = socket.create_connection((self.target.host, self.target.port), timeout=self.timeout)
        if self.target.tls:
            ctx = _tls_context(self.target.cacert)
            # SAN on the test cert is "localhost"; pass host as SNI/verify name.
            raw = ctx.wrap_socket(raw, server_hostname=self.target.host)
        raw.settimeout(self.timeout)
        self.sock = raw

    def close(self) -> None:
        if self.sock is not None:
            try:
                self.sock.close()
            finally:
                self.sock = None

    def __enter__(self) -> "Channel":
        self.open()
        return self

    def __exit__(self, *_exc) -> None:
        self.close()

    # ── one exchange ──────────────────────────────────────────────────────────
    def exchange(self, p: Payload) -> RoundResult:
        """Send one payload and classify the reply. The socket stays open for
        reuse (HTTP keep-alive / raw stays mid-connection); on DROP it's dead.

        RAW payloads are padded past `flush_threshold` so the filter flushes them
        as a stream chunk mid-connection (see DEFAULT_FLUSH_THRESHOLD) — the only
        way to read an allow/drop verdict for non-HTTP traffic."""
        assert self.sock is not None, "channel not open"
        if p.kind == "http":
            data = build_http(self.target, p)
            try:
                self.sock.sendall(data)
            except (BrokenPipeError, ConnectionResetError, OSError) as e:
                return RoundResult(Verdict.DROP, None, f"send failed: {e}")
            return self._read_http()

        # raw: pad to force a mid-connection flush, then expect a byte-exact echo.
        data = p.raw
        if len(data) <= self.flush_threshold:
            data = data + b"." * (self.flush_threshold + 16 - len(data))
        try:
            self.sock.sendall(data)
        except (BrokenPipeError, ConnectionResetError, OSError) as e:
            return RoundResult(Verdict.DROP, None, f"send failed: {e}")
        return self._read_raw(len(data))

    def _read_http(self) -> RoundResult:
        assert self.sock is not None
        buf = b""
        try:
            while True:
                chunk = self.sock.recv(65536)
                if not chunk:
                    # EOF before a complete reply → the filter dropped us.
                    if not buf:
                        return RoundResult(Verdict.DROP, None, "EOF, no reply")
                    return RoundResult(Verdict.DROP, None, "EOF mid-reply")
                buf += chunk
                done, status, total = _http_complete(buf)
                if done:
                    return RoundResult(Verdict.ALLOW, status, f"{total} bytes")
        except socket.timeout:
            if buf:
                return RoundResult(Verdict.TIMEOUT, None, f"partial {len(buf)}B, no end")
            return RoundResult(Verdict.TIMEOUT, None, "no reply in time")
        except (ConnectionResetError, OSError) as e:
            return RoundResult(Verdict.DROP, None, f"reset: {e}")

    def _read_raw(self, expected: int) -> RoundResult:
        # An allowed raw chunk is byte-exactly echoed back; a clean close with
        # nothing (or a partial then close) means the filter dropped it.
        assert self.sock is not None
        buf = b""
        try:
            while True:
                chunk = self.sock.recv(65536)
                if not chunk:
                    if not buf:
                        return RoundResult(Verdict.DROP, None, "EOF, no echo")
                    return RoundResult(Verdict.DROP, None,
                                       f"EOF after partial echo {len(buf)}/{expected}")
                buf += chunk
                if len(buf) >= expected:
                    return RoundResult(Verdict.ALLOW, None, f"{len(buf)} bytes echoed")
        except socket.timeout:
            if buf:
                return RoundResult(Verdict.TIMEOUT, None, f"partial echo {len(buf)}/{expected}")
            return RoundResult(Verdict.TIMEOUT, None, "no echo in time")
        except (ConnectionResetError, OSError) as e:
            return RoundResult(Verdict.DROP, None, f"reset: {e}")


def _http_complete(buf: bytes) -> tuple[bool, int | None, int]:
    """Is `buf` a complete HTTP response? Returns (done, status, total_len)."""
    sep = buf.find(b"\r\n\r\n")
    if sep == -1:
        return (False, None, len(buf))
    head = buf[:sep].decode("latin-1", "replace")
    lines = head.split("\r\n")
    status: int | None = None
    parts = lines[0].split(" ")
    if len(parts) >= 2 and parts[1].isdigit():
        status = int(parts[1])
    content_length: int | None = None
    for line in lines[1:]:
        if ":" in line:
            k, v = line.split(":", 1)
            if k.strip().lower() == "content-length" and v.strip().isdigit():
                content_length = int(v.strip())
    body_start = sep + 4
    if content_length is None:
        # No declared length: treat headers-complete as done (HEAD/empty body).
        return (True, status, len(buf))
    have = len(buf) - body_start
    return (have >= content_length, status, body_start + content_length)


# ─────────────────────────────────────────────────────────────────────────────
#  Convenience: run a sequence of payloads over a single channel (multi-round)
# ─────────────────────────────────────────────────────────────────────────────
def run_sequence(target: Target, payloads: list[Payload], timeout: float = DEFAULT_TIMEOUT,
                 interval_ms: int = 0, on_result=None, reuse: bool = False,
                 flush_threshold: int = DEFAULT_FLUSH_THRESHOLD) -> list[RoundResult]:
    """Fire `payloads` in order, reporting each via `on_result(i, payload, result)`.

    reuse=True  → all rounds share ONE connection (a session / keep-alive multi-
                  round test); stops at the first drop, since that kills the socket.
    reuse=False → each payload gets a FRESH connection (independent probes); a
                  drop on one doesn't stop the rest — every payload gets a verdict.
    """
    import time
    results: list[RoundResult] = []
    ch: Channel | None = None
    for i, p in enumerate(payloads):
        if ch is None or not reuse:
            if ch is not None:
                ch.close()
            ch = Channel(target, timeout=timeout, flush_threshold=flush_threshold)
            try:
                ch.open()
            except OSError as e:
                r = RoundResult(Verdict.ERROR, None, f"connect failed: {e}")
                results.append(r)
                if on_result:
                    on_result(i, p, r)
                ch = None
                if reuse:
                    break
                continue

        r = ch.exchange(p)
        results.append(r)
        if on_result:
            on_result(i, p, r)
        if reuse and r.verdict in (Verdict.DROP, Verdict.ERROR):
            break
        if interval_ms and i < len(payloads) - 1:
            time.sleep(interval_ms / 1000.0)
    if ch is not None:
        ch.close()
    return results


def fmt_summary(results: list[RoundResult]) -> str:
    n = len(results)
    allow = sum(1 for r in results if r.verdict == Verdict.ALLOW)
    drop = sum(1 for r in results if r.verdict == Verdict.DROP)
    to = sum(1 for r in results if r.verdict == Verdict.TIMEOUT)
    err = sum(1 for r in results if r.verdict == Verdict.ERROR)
    return f"{n} rounds → {allow} allow, {drop} drop, {to} timeout, {err} error"


def parse_kv_headers(items: list[str]) -> dict[str, str]:
    """Parse 'Name: value' / 'Name=value' strings into a header dict."""
    out: dict[str, str] = {}
    for it in items:
        if ":" in it:
            k, v = it.split(":", 1)
        elif "=" in it:
            k, v = it.split("=", 1)
        else:
            continue
        out[k.strip()] = v.strip()
    return out


def as_json_target(t: Target) -> str:
    return json.dumps({"name": t.name, "host": t.host, "port": t.port, "tls": t.tls})
