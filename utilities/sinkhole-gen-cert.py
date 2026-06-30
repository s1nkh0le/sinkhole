#!/usr/bin/env python3
"""sinkhole-gen-cert — make a self-signed certificate for an existing private key.

A sinkhole TLS port terminates client TLS with a cert/key pair (`TLS_MAP`). In
production you usually already have the guarded service's **private key**; this
tool produces a matching self-signed certificate for it (with the right SANs) so
the filter can present it. If you have no key yet, `--generate-key` makes one too.

Wraps `openssl` (present on essentially every box) — stdlib only. Same look & feel
as sinkhole-autoconfig: shows the plan, confirms, then writes.
"""
from __future__ import annotations

import argparse
import ipaddress
import os
import shutil
import subprocess
import sys
from pathlib import Path

# ── colors (mirrors sinkhole-autoconfig) ─────────────────────────────────────
_TTY = sys.stdout.isatty()


def _c(code: str, s: str) -> str:
    return f"\033[{code}m{s}\033[0m" if _TTY else s


def bold(s):   return _c("1", s)
def dim(s):    return _c("2", s)
def red(s):    return _c("31", s)
def green(s):  return _c("32", s)
def yellow(s): return _c("33", s)
def blue(s):   return _c("36", s)


def info(s):  print(f"{blue('▸')} {s}")
def warn(s):  print(f"{yellow('!')} {s}")
def err(s):   print(f"{red('✗')} {s}", file=sys.stderr)
def good(s):  print(f"{green('✓')} {s}")


KEY_NAMES = ["privkey.pem", "key.pem", "server.key", "tls.key"]


def find_key(start: Path) -> Path | None:
    for n in KEY_NAMES:
        p = start / n
        if p.is_file():
            return p
    return None


def san_token(s: str) -> str:
    """Classify a SAN entry as IP:… or DNS:… for openssl."""
    s = s.strip()
    try:
        ipaddress.ip_address(s)
        return f"IP:{s}"
    except ValueError:
        return f"DNS:{s}"


def main() -> int:
    ap = argparse.ArgumentParser(description="Generate a self-signed certificate for an existing private key.")
    ap.add_argument("-k", "--key", help="private key PEM (default: autodetect in --dir)")
    ap.add_argument("-d", "--dir", default=".", help="directory to search / write in (default: cwd)")
    ap.add_argument("-o", "--out", help="output certificate path (default: cert.pem beside the key)")
    ap.add_argument("--cn", help="certificate CommonName (default: first --san, else 'sinkhole')")
    ap.add_argument("--san", default="localhost,127.0.0.1",
                    help="comma-separated SANs (DNS names / IPs). Default: localhost,127.0.0.1")
    ap.add_argument("--days", type=int, default=365, help="validity in days (default 365)")
    ap.add_argument("--generate-key", action="store_true", help="create an EC P-256 key if none is found")
    ap.add_argument("--force", action="store_true", help="overwrite an existing certificate")
    ap.add_argument("-y", "--yes", action="store_true", help="write without confirmation")
    args = ap.parse_args()

    if not shutil.which("openssl"):
        err("openssl not found on PATH"); return 2

    wdir = Path(args.dir).resolve()
    if not wdir.is_dir():
        err(f"not a directory: {wdir}"); return 2

    # locate / create the key
    key = Path(args.key).resolve() if args.key else find_key(wdir)
    if key is None:
        if args.generate_key:
            key = wdir / "privkey.pem"
            info(f"no key found; generating EC P-256 key -> {key}")
            r = subprocess.run(
                ["openssl", "genpkey", "-algorithm", "EC",
                 "-pkeyopt", "ec_paramgen_curve:P-256", "-out", str(key)],
                capture_output=True, text=True,
            )
            if r.returncode != 0:
                err(f"key generation failed:\n{r.stderr.strip()}"); return 1
            os.chmod(key, 0o600)
        else:
            err(f"no private key found in {wdir} (looked for {', '.join(KEY_NAMES)}); "
                "pass --key <path> or --generate-key")
            return 2
    if not key.is_file():
        err(f"key not found: {key}"); return 2

    out = Path(args.out).resolve() if args.out else key.with_name("cert.pem")
    sans = [s for s in (args.san or "").split(",") if s.strip()]
    if not sans:
        err("at least one --san is required"); return 2
    cn = args.cn or sans[0]
    san_str = ",".join(san_token(s) for s in sans)

    # plan
    print()
    print(bold("  sinkhole — certificate plan"))
    print(dim("  " + "─" * 56))
    info(f"private key:  {key}")
    info(f"certificate:  {out}")
    info(f"CommonName:   {bold(cn)}")
    info(f"SANs:         {', '.join(sans)}")
    info(f"valid for:    {args.days} days")
    print()

    if out.exists() and not args.force:
        err(f"{out} already exists (use --force to overwrite)"); return 1

    if not args.yes:
        try:
            ans = input(f"  {bold('Generate this certificate?')} {dim('[y/N]')} ").strip().lower()
        except EOFError:
            ans = "n"
        if ans not in {"y", "yes"}:
            warn("aborted; nothing written"); return 0

    cmd = [
        "openssl", "req", "-x509", "-new",
        "-key", str(key),
        "-out", str(out),
        "-days", str(args.days),
        "-subj", f"/CN={cn}",
        "-addext", f"subjectAltName={san_str}",
    ]
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        err(f"openssl failed:\n{r.stderr.strip()}"); return 1
    os.chmod(out, 0o644)
    good(f"wrote {out}")
    print(dim(f"\n  use it in the filter:  SINKHOLE_FILTER_TLS_MAP=\"<port>:{out}:{key}\"\n"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
