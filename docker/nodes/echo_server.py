#!/usr/bin/env python3
"""Configurable multi-port mock upstream for the sinkhole test harness.

Each port runs an independent HTTP(S) server whose *behaviour* is declared in
``ECHO_SPEC`` — a JSON array of per-port specs. This lets one container stand in
for a challenge's internal services, with a different behaviour per port, so the
filter (and the controller's per-port record sync) can be exercised against a
realistic mix.

Behaviours (``mode``):
  * ``echo``  — reflect method/path/headers/body back as JSON (the default).
                Unlike ``python -m http.server`` this also answers POST/PUT/…,
                so body forwarding and truncation can be asserted client-side.
  * ``delay`` — like ``echo`` but sleeps ``delay_ms`` before replying (the
                "slow upstream"): useful for timing matchers / idle detection.
                Keep ``delay_ms`` below the traffic client's read timeout or an
                allowed-but-slow reply reads as a drop.
  * ``fixed`` — ignore the request and always return a canned ``status`` +
                ``body`` (with optional ``content_type``): a stable target for
                whitelist/blacklist assertions.
  * ``raw``   — a plain (non-HTTP) TCP echo: every byte received is echoed back,
                for the whole connection lifetime. This is the stream-scope
                target — the filter can't HTTP-parse it, so it flows as raw
                chunks. Multi-round friendly (the socket stays open).
  * ``exec``  — spawn a subprocess per connection and bridge the socket to its
                stdin/stdout (inetd / ``socat EXEC:`` style). ``exec_cmd`` is the
                argv array, e.g. ``["python","/srv/menu_app.py"]`` — point it at a
                compiled binary just the same. Models an *interactive* service
                (a menu that drives request→reply→request over stdin/stdout): the
                stream-scope, server-speaks-first target for the filter's
                interactive/unframed path.

HTTP modes speak HTTP/1.1 with keep-alive, so a client can send many requests
over one TCP connection (multi-round) — exercising connection-id / reply-chain
tracking and session timing matchers.

ECHO_SPEC example (whitespace optional):
  [
    {"port": 80, "mode": "echo"},
    {"port": 81, "mode": "delay", "delay_ms": 1500},
    {"port": 82, "mode": "fixed", "status": 200, "body": "pong",
                 "content_type": "text/plain"},
    {"port": 83, "mode": "echo", "tls": true},
    {"port": 84, "mode": "exec", "exec_cmd": ["python", "/srv/menu_app.py"]}
  ]

TLS: a port with ``"tls": true`` terminates HTTPS itself, using the cert/key in
``ECHO_TLS_CERT`` / ``ECHO_TLS_KEY``. NOTE: sinkhole terminates client TLS at the
*filter* and speaks *plaintext* to its upstream, so a TLS echo port can only be
reached by a client hitting it directly (not through a filter). The harness's
"real" TLS path is filter-terminated (the filter's TLS_MAP). The echo TLS switch
exists for direct client tests only.

Back-compat: if ``ECHO_SPEC`` is unset, ``ECHO_PORTS`` (comma-separated, default
``80,81,82,83``) is treated as a list of plain ``echo`` ports.
"""
import json
import os
import socket
import socketserver
import ssl
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def _load_specs():
    """Return a list of normalized per-port behaviour dicts."""
    raw = os.environ.get("ECHO_SPEC", "").strip()
    if raw:
        specs = json.loads(raw)
        if not isinstance(specs, list):
            raise ValueError("ECHO_SPEC must be a JSON array of per-port specs")
        return [_normalize(s) for s in specs]

    # Back-compat: ECHO_PORTS -> a list of plain echo ports.
    ports = os.environ.get("ECHO_PORTS", "80,81,82,83")
    return [
        _normalize({"port": int(p.strip()), "mode": "echo"})
        for p in ports.split(",")
        if p.strip()
    ]


def _normalize(spec):
    mode = spec.get("mode", "echo")
    if mode not in ("echo", "delay", "fixed", "raw", "exec"):
        raise ValueError(f"unknown echo mode {mode!r} for port {spec.get('port')}")
    exec_cmd = spec.get("exec_cmd")
    if mode == "exec":
        if not isinstance(exec_cmd, list) or not exec_cmd:
            raise ValueError(
                f"exec mode on port {spec.get('port')} needs a non-empty "
                f'"exec_cmd" array, e.g. ["python","/srv/menu_app.py"]'
            )
    return {
        "port": int(spec["port"]),
        "mode": mode,
        "delay_ms": int(spec.get("delay_ms", 0)),
        "status": int(spec.get("status", 200)),
        "body": spec.get("body", ""),
        "content_type": spec.get("content_type", "text/plain"),
        "tls": bool(spec.get("tls", False)),
        "exec_cmd": exec_cmd,
    }


class BehaviorServer(ThreadingHTTPServer):
    """ThreadingHTTPServer that carries its port's behaviour spec."""

    daemon_threads = True

    def __init__(self, addr, handler, behavior):
        self.behavior = behavior
        super().__init__(addr, handler)


class EchoHandler(BaseHTTPRequestHandler):
    # HTTP/1.1 => persistent connections (keep-alive), so a client can pipeline
    # many requests over one socket. Every response below sends Content-Length,
    # which keep-alive framing requires.
    protocol_version = "HTTP/1.1"

    def _read_body(self):
        length = int(self.headers.get("Content-Length", "0") or "0")
        return self.rfile.read(length) if length else b""

    def _respond_echo(self, body):
        payload = json.dumps(
            {
                "upstream_port": self.server.server_address[1],
                "mode": self.server.behavior["mode"],
                "method": self.command,
                "path": self.path,
                "headers": dict(self.headers.items()),
                "body_len": len(body),
                "body": body.decode("utf-8", "replace"),
            }
        ).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(payload)

    def _respond_fixed(self):
        b = self.server.behavior
        body = b["body"].encode("utf-8") if isinstance(b["body"], str) else bytes(b["body"])
        self.send_response(b["status"])
        self.send_header("Content-Type", b["content_type"])
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(body)

    def _handle(self):
        behavior = self.server.behavior
        body = self._read_body()  # always drain the body so the socket stays sane
        if behavior["mode"] == "fixed":
            self._respond_fixed()
            return
        if behavior["mode"] == "delay" and behavior["delay_ms"] > 0:
            time.sleep(behavior["delay_ms"] / 1000.0)
        self._respond_echo(body)

    do_GET = _handle
    do_POST = _handle
    do_PUT = _handle
    do_DELETE = _handle
    do_PATCH = _handle
    do_HEAD = _handle

    def log_message(self, *_args):
        pass


class RawEchoHandler(socketserver.BaseRequestHandler):
    """Plain TCP echo: reflect every byte back until the peer closes."""

    def handle(self):
        while True:
            try:
                data = self.request.recv(65536)
            except OSError:
                return
            if not data:
                return
            try:
                self.request.sendall(data)
            except OSError:
                return


class RawEchoServer(socketserver.ThreadingTCPServer):
    daemon_threads = True
    allow_reuse_address = True


class ExecHandler(socketserver.BaseRequestHandler):
    """Bridge a TCP connection to a subprocess's stdin/stdout — the classic
    inetd / ``socat ... EXEC:`` pattern used by CTF pwn services. Per connection we
    spawn ``exec_cmd`` and pump bytes both ways (socket → child stdin, child stdout
    → socket) until either side closes. This is a *binary/interactive* upstream:
    the program drives a request→reply→request loop over stdin/stdout, so it
    exercises the filter's interactive (unframed) path end-to-end."""

    def handle(self):
        cmd = self.server.behavior["exec_cmd"]
        sock = self.request
        # bufsize=0 → unbuffered pipes; we read the child's stdout via os.read so a
        # prompt is delivered as soon as the program flushes it (no block-buffering).
        proc = subprocess.Popen(
            cmd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            bufsize=0,
        )

        def sock_to_child():
            try:
                while True:
                    data = sock.recv(65536)
                    if not data:
                        break
                    proc.stdin.write(data)
                    proc.stdin.flush()
            except OSError:
                pass
            finally:
                try:
                    proc.stdin.close()
                except OSError:
                    pass

        def child_to_sock():
            fd = proc.stdout.fileno()
            try:
                while True:
                    out = os.read(fd, 65536)
                    if not out:
                        break
                    sock.sendall(out)
            except OSError:
                pass

        t_in = threading.Thread(target=sock_to_child, daemon=True)
        t_out = threading.Thread(target=child_to_sock, daemon=True)
        t_in.start()
        t_out.start()
        proc.wait()
        t_out.join(timeout=1)
        try:
            sock.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        try:
            proc.kill()
        except OSError:
            pass


class ExecServer(socketserver.ThreadingTCPServer):
    daemon_threads = True
    allow_reuse_address = True

    def __init__(self, addr, handler, behavior):
        self.behavior = behavior
        super().__init__(addr, handler)


def _tls_context():
    cert = os.environ.get("ECHO_TLS_CERT")
    key = os.environ.get("ECHO_TLS_KEY")
    if not cert or not key:
        raise RuntimeError(
            "a port requests TLS but ECHO_TLS_CERT / ECHO_TLS_KEY are not set"
        )
    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    ctx.load_cert_chain(certfile=cert, keyfile=key)
    return ctx


def _serve(spec):
    if spec["mode"] == "raw":
        srv = RawEchoServer(("0.0.0.0", spec["port"]), RawEchoHandler)
        srv.serve_forever()
        return
    if spec["mode"] == "exec":
        srv = ExecServer(("0.0.0.0", spec["port"]), ExecHandler, spec)
        srv.serve_forever()
        return
    srv = BehaviorServer(("0.0.0.0", spec["port"]), EchoHandler, spec)
    if spec["tls"]:
        srv.socket = _tls_context().wrap_socket(srv.socket, server_side=True)
    srv.serve_forever()


if __name__ == "__main__":
    specs = _load_specs()
    threads = [threading.Thread(target=_serve, args=(s,), daemon=True) for s in specs]
    for t in threads:
        t.start()
    summary = ", ".join(
        f"{s['port']}:{s['mode']}{'(tls)' if s['tls'] else ''}" for s in specs
    )
    print(f"echo upstream listening — {summary}", flush=True)
    for t in threads:
        t.join()
