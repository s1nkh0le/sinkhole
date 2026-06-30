#!/usr/bin/env python3
"""A tiny interactive menu service for the test stack.

Wired to a TCP socket by ``echo_server.py``'s ``exec`` mode (its stdin/stdout are
the socket). It models a classic CTF-style interactive program: it prints a banner
+ menu **first** (server-speaks-first), then loops request→reply→request over
stdin/stdout. This is the end-to-end target for the filter's interactive/unframed
path — small line-based messages must round-trip promptly instead of buffering
until EOF.

Point ``exec_cmd`` at a compiled binary instead and the wiring is identical; this
script is just a dependency-free stand-in (python:3.12-slim has no toolchain).
"""
import sys

BANNER = "=== sinkhole menu service ===\n"
MENU = (
    "1) ping\n"
    "2) echo <text>\n"
    "3) add <a> <b>\n"
    "4) quit\n"
    "> "
)


def emit(s: str) -> None:
    sys.stdout.write(s)
    sys.stdout.flush()


def main() -> None:
    emit(BANNER)
    emit(MENU)
    for line in sys.stdin:
        cmd = line.strip()
        if cmd == "1" or cmd == "ping":
            emit("pong\n")
        elif cmd.startswith("2") or cmd.startswith("echo"):
            # everything after the verb is echoed back
            arg = cmd.split(None, 1)[1] if len(cmd.split(None, 1)) > 1 else ""
            emit(f"echo: {arg}\n")
        elif cmd.startswith("3") or cmd.startswith("add"):
            parts = cmd.split()[1:]
            try:
                total = sum(int(p) for p in parts)
                emit(f"sum: {total}\n")
            except ValueError:
                emit("error: add expects integers\n")
        elif cmd == "4" or cmd == "quit":
            emit("bye\n")
            return
        elif cmd == "":
            pass
        else:
            emit(f"unknown command: {cmd!r}\n")
        emit(MENU)


if __name__ == "__main__":
    try:
        main()
    except (BrokenPipeError, KeyboardInterrupt):
        pass
