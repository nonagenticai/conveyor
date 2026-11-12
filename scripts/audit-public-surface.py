#!/usr/bin/env python3
"""audit-public-surface.py — what must not be public in this repository.

The repo is about to become public. This scanner answers one question: which tracked
files (and which historical commits, including fetched PR heads) carry something that
must not be published — a credential, a personal email, a private IP address, a home
directory, or a production log sample.

It is deliberately independent of gitleaks: gitleaks finds credential shapes, this
finds the rest. Run both.

The report is written OUTSIDE the repository by default (~/.claude/plans/), because a
report that lists the values it found is itself a leak. The terminal gets counts only.

Usage:
    python3 scripts/audit-public-surface.py                 # tracked tree
    python3 scripts/audit-public-surface.py --history       # every commit in --all
    python3 scripts/audit-public-surface.py --all           # tree + history
    python3 scripts/audit-public-surface.py --all --fail-on-findings
    python3 scripts/audit-public-surface.py --report /tmp/audit.md

Exit codes: 0 no findings (or findings without --fail-on-findings), 1 findings with
--fail-on-findings, 2 usage/git error.
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_REPORT = Path.home() / ".claude" / "plans" / "conveyor-public-audit.md"
BINARY_SUFFIXES = {
    ".woff2", ".woff", ".png", ".jpg", ".jpeg", ".gif", ".ico", ".pdf",
    ".zip", ".gz", ".tgz", ".bz2", ".xz", ".mp4", ".webm", ".ttf", ".otf",
}

# Documentation-only address ranges (RFC 5737) plus loopback are not private data.
ALLOWED_IPS = {"0.0.0.0", "127.0.0.1", "255.255.255.255"}
# Deliberate test data: these paths carry fixture values, not estate facts. Findings
# here are reported as `synthetic-*` and never fail the gate. The two webhook files
# are the SSRF address-policy tests: they MUST use private addresses to prove that a
# private target is allowed, and those addresses are examples, not estate hosts.
# `.gitleaks.toml` carries literal fake token shapes in its allowlist examples.
SYNTHETIC_PATH = re.compile(
    r"^(?:crates/(?:actions|domain|api)/tests/fixtures/benchmark/|seed/benchmark/|\.claude/hooks/tests/"
    r"|crates/actions/src/board/webhooks\.rs$|crates/api/tests/webhook_shape\.rs$|\.gitleaks\.toml$)"
)
# Third-party and example homes that appear in quoted code. They identify nobody here.
SYNTHETIC_HOME = re.compile(
    r"^(?:~/\.(?:cargo|claude|ssh|aws)(?:/|$)|/home/(?:tom|mulawa|workflows|user|alice|bob)(?:/|$)"
    r"|/Users/(?:wsykala|macbookpro|user|alice|bob)(?:/|$))"
)
# Documented synthetic domains; `.test`, `.invalid` and `.example` are reserved.
SYNTHETIC_DOMAIN = re.compile(
    r"(?:^|\.)(?:example|invalid|test|localhost)$"
    r"|^(?:acme\.dev|acme\.example|b\.co|request\.com|some\.host|other\.host|complex\.url\.com"
    r"|pytest\.fixture|pytest\.mark\.sphinx|admin\.register|dev\.invalid|broken\.example|y\.example)$",
    re.IGNORECASE,
)
IPV4 = re.compile(r"(?:(?<![\d.])(?:\d{1,3}\.){3}\d{1,3}(?![\d.]))")
PRIVATE_IP = re.compile(
    r"^(?:10\.|192\.168\.|172\.(?:1[6-9]|2\d|3[01])\.|100\.(?:6[4-9]|[7-9]\d|1[01]\d|12[0-7])\.)"
)
EMAIL = re.compile(r"[A-Za-z0-9._%+\-]+@([A-Za-z0-9.\-]+\.[A-Za-z]{2,})")
HOME = re.compile(r"(?:/Users/[A-Za-z0-9._-]+|/home/[A-Za-z0-9._-]+|~/(?:\.ssh|\.aws|\.cargo|\.claude)|[A-Za-z]:\\Users\\[^\\\s]+)")
TOKEN = re.compile(
    r"(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|sk-[A-Za-z0-9]{20,}"
    r"|AKIA[0-9A-Z]{16}|xox[baprs]-[A-Za-z0-9\-]{10,}|npm_[A-Za-z0-9]{20,}"
    r"|-----BEGIN [A-Z ]*PRIVATE KEY-----"
    r"|(?i:bearer)\s+eyJ[A-Za-z0-9_\-]{20,}\.[A-Za-z0-9_\-]{20,}\.[A-Za-z0-9_\-]{20,})"
)
# A test may send a truncated/fake JWT to prove it is refused; only a full JWT is a leak.
SENSITIVE_NAME = re.compile(
    r"(?:^|/)(?:\.env(?:\..*)?|\.npmrc|\.netrc|id_rsa|id_ed25519|kubeconfig|.*\.(?:pem|key|pfx|p12))$",
    re.IGNORECASE,
)


@dataclass(frozen=True)
class Finding:
    klass: str
    where: str
    text: str

    def redacted(self) -> str:
        value = self.text
        if len(value) <= 6:
            return "*" * len(value)
        return value[:3] + "…" + value[-2:]


def git(*args: str) -> str:
    return subprocess.check_output(["git", "-C", str(ROOT), *args], text=True, errors="replace")


def allow_email(match: re.Match[str]) -> bool:
    domain = match.group(1).lower()
    return (
        domain.endswith("example.com")
        or domain.endswith("example.org")
        or domain.endswith("example.net")
        or domain.endswith("users.noreply.github.com")
        or domain == "example.invalid"
    )


def allow_ip(value: str) -> bool:
    if value in ALLOWED_IPS:
        return True
    parts = [int(p) for p in value.split(".")]
    if any(p > 255 for p in parts):
        return True  # a version, not an address
    if parts[0] == 192 and parts[1] == 0 and parts[2] == 2:  # 192.0.2.0/24
        return True
    if parts[0] == 198 and parts[1] == 51 and parts[2] == 100:  # 198.51.100.0/24
        return True
    if parts[0] == 203 and parts[1] == 0 and parts[2] == 113:  # 203.0.113.0/24
        return True
    return False


def in_url_authority(text: str, start: int) -> bool:
    """True when the match sits in `scheme://user:pass@host` userinfo, not an address."""
    window = text[max(0, start - 160):start]
    cut = max(window.rfind(ch) for ch in (' ', '\t', '\n', '"', "'", '`', '<'))
    return "://" in window[cut + 1:]


def scan_text(where: str, text: str, synthetic: bool = False) -> list[Finding]:
    prefix = "synthetic-" if synthetic else ""
    out: list[Finding] = []
    for m in TOKEN.finditer(text):
        out.append(Finding(prefix + "secret", where, m.group(0)))
    for m in EMAIL.finditer(text):
        if allow_email(m) or SYNTHETIC_DOMAIN.search(m.group(1)) or in_url_authority(text, m.start()):
            continue
        out.append(Finding(prefix + "email", where, m.group(0)))
    for m in IPV4.finditer(text):
        value = m.group(0)
        if allow_ip(value):
            continue
        if PRIVATE_IP.match(value + "."):
            out.append(Finding(prefix + "private-ip", where, value))
    for m in HOME.finditer(text):
        if SYNTHETIC_HOME.match(m.group(0)):
            continue
        out.append(Finding(prefix + "home-path", where, m.group(0)))
    return out


def scan_tree() -> list[Finding]:
    findings: list[Finding] = []
    for rel in git("ls-files").splitlines():
        path = ROOT / rel
        if not path.is_file():
            continue
        if SENSITIVE_NAME.search(rel):
            findings.append(Finding("sensitive-name", rel, rel))
        if path.suffix.lower() in BINARY_SUFFIXES:
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        for finding in scan_text(rel, text, synthetic=bool(SYNTHETIC_PATH.match(rel))):
            findings.append(finding)
    return findings


def scan_history() -> list[Finding]:
    findings: list[Finding] = []
    stream = git("log", "--all", "-p", "--no-color", "--format=%x00%H %ad %s", "--date=short")
    commit = "unknown"
    path = "?"
    for line in stream.splitlines():
        if line.startswith("\x00"):
            commit = line[1:]
            continue
        if line.startswith("+++ b/"):
            path = line[6:]
            continue
        if line.startswith("+") and not line.startswith("+++"):
            for finding in scan_text(f"commit {commit} {path}", line[1:], synthetic=bool(SYNTHETIC_PATH.match(path))):
                findings.append(finding)
    return findings


def has_pr_refs() -> bool:
    refs = git("for-each-ref", "--format=%(refname)", "refs/remotes/origin/pr")
    return bool(refs.strip())


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--history", action="store_true", help="also scan every commit in --all")
    parser.add_argument("--all", action="store_true", help="tree + history")
    parser.add_argument("--report", type=Path, default=DEFAULT_REPORT)
    parser.add_argument("--fail-on-findings", action="store_true")
    args = parser.parse_args()

    if not (ROOT / ".git").exists() and not (ROOT.parent / ".git").exists():
        print("audit: not a git checkout", file=sys.stderr)
        return 2

    findings = scan_tree()
    if args.history or args.all:
        findings += scan_history()

    counts: dict[str, int] = {}
    for finding in findings:
        counts[finding.klass] = counts.get(finding.klass, 0) + 1

    args.report.parent.mkdir(parents=True, exist_ok=True)
    with args.report.open("w", encoding="utf-8") as handle:
        handle.write("# Public-surface audit (private report — do not commit)\n\n")
        handle.write(f"Repo: `{ROOT}`\n\n")
        handle.write(f"PR refs fetched: {'yes' if has_pr_refs() else 'no (run the pre-flip fetch)'}\n\n")
        handle.write("| class | count |\n|---|---|\n")
        for klass in sorted(counts):
            handle.write(f"| {klass} | {counts[klass]} |\n")
        handle.write("\n## Findings\n\n")
        for finding in sorted(findings, key=lambda f: (f.klass, f.where)):
            handle.write(f"- **{finding.klass}** · `{finding.where}` · `{finding.redacted()}`\n")

    print(f"audit: {len(findings)} finding(s); report at {args.report}")
    for klass in sorted(counts):
        print(f"  {klass}: {counts[klass]}")

    real = [f for f in findings if not f.klass.startswith("synthetic-")]
    return 1 if real and args.fail_on_findings else 0


if __name__ == "__main__":
    raise SystemExit(main())
