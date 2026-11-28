#!/usr/bin/env python3
"""One redaction rule set for everything this repository publishes.

The guard's hard set (`scripts/ci/no-private-refs.sh`) is the floor: an internal hostname, a
tailnet address, a Tailscale name, the deploy key/host names and the private GitOps path. The
estate's node names are the same kind of fact and are redacted with it, to role names that keep
the evidence's meaning:

    hp1 -> compute-1, hp2 -> ci-1, mf1 -> storage-1, worker3 -> registry-1,
    mini1 -> media-1, mini2 -> media-2, pgx -> gpu-1

The rule set is imported by the writers that put text into the public tree (`fetch-issues.sh`,
`redact-public-register.py`, `build-docs.py`, `rework-project.py`,
`publish-artefact-bodies.py`) so one replacement cannot drift from another. The Rust snapshot
renderer mirrors these patterns in `crates/pack/src/snapshot.rs::redact`; the shared fixture
`scripts/fixtures/private-refs.json` is read by both tests.

Usage:
    python3 scripts/private_refs.py --self-test
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any

__all__ = ["PATTERNS", "redact", "redact_text"]

# Order matters: the host suffix first, then the names that can sit inside a host label.
PATTERNS: tuple[tuple[re.Pattern[str], str], ...] = (
    (re.compile(r"\b([A-Za-z0-9][A-Za-z0-9_-]*)\.internal\b"), r"\1.example.com"),
    (re.compile(r"nonagenticai/gitops-production"), "gitops-production"),
    (re.compile(r"strato-vps"), "deploy-key"),
    (re.compile(r"strato-server"), "deploy-host"),
    (re.compile(r"\bts\.net\b"), "tailnet"),
    (
        re.compile(r"\b100\.(?:6[4-9]|[7-9]\d|1[01]\d|12[0-7])\.\d{1,3}\.\d{1,3}\b"),
        "192.0.2.10",
    ),
    (re.compile(r"\bhp1\b"), "compute-1"),
    (re.compile(r"\bhp2\b"), "ci-1"),
    (re.compile(r"\bmf1\b"), "storage-1"),
    (re.compile(r"\bworker3\b"), "registry-1"),
    (re.compile(r"\bmini1\b"), "media-1"),
    (re.compile(r"\bmini2\b"), "media-2"),
    (re.compile(r"\bpgx\b"), "gpu-1"),
)


def redact_text(text: str) -> str:
    """Apply every rule to one string."""
    for pattern, replacement in PATTERNS:
        text = pattern.sub(replacement, text)
    return text


def redact(value: Any) -> Any:
    """Apply the rules through a JSON-shaped value (strings, lists, objects)."""
    if isinstance(value, str):
        return redact_text(value)
    if isinstance(value, list):
        return [redact(item) for item in value]
    if isinstance(value, dict):
        return {key: redact(item) for key, item in value.items()}
    return value


def self_test() -> int:
    fixture = Path(__file__).with_name("fixtures") / "private-refs.json"
    cases = json.loads(fixture.read_text(encoding="utf-8"))
    failures = []
    for case in cases:
        actual = redact_text(case["input"])
        if actual != case["python"]:
            failures.append(f"{case['input']!r}: {actual!r} != {case['python']!r}")
    if failures:
        print("private_refs: FAIL", file=sys.stderr)
        for failure in failures:
            print(f"  {failure}", file=sys.stderr)
        return 1
    print(f"private_refs: OK ({len(cases)} cases)")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    parser.error("nothing to do: pass --self-test")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
