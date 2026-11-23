#!/usr/bin/env python3
"""Tests for `scripts/publish-artefact-bodies.py`.

The publisher's own `--self-test` builds the fixtures (a tiny standard, a two-project
snapshot, a stub transport) and asserts the readiness classification, the exact
publication payloads, the skip, the ambiguity refusal and the journal shape.  This file
runs that self-test and checks the CLI contract around it.  No database, no network, no
Rust build.
"""

from __future__ import annotations

import importlib.util
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "publish-artefact-bodies.py"

CASES: list = []


def case(fn):
    CASES.append(fn)
    return fn


def run(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(SCRIPT), *args], capture_output=True, text=True,
    )


@case
def self_test_passes() -> None:
    proc = run("--self-test")
    assert proc.returncode == 0, (proc.stdout, proc.stderr)
    assert "FAIL" not in proc.stdout, proc.stdout
    assert proc.stdout.count("PASS self-test") >= 8, proc.stdout


@case
def mode_and_selection_are_required() -> None:
    proc = run("--snapshot", "/nonexistent", "--outputs-dir", "/nonexistent")
    assert proc.returncode == 2, (proc.returncode, proc.stderr)
    combined = proc.stdout + proc.stderr
    assert "error" in combined.lower(), combined[-400:]


@case
def apply_requires_a_bodies_directory() -> None:
    proc = run(
        "--snapshot", "/nonexistent", "--outputs-dir", "/nonexistent",
        "--all-projects", "--apply",
    )
    assert proc.returncode == 2, (proc.returncode, proc.stderr)
    assert "--bodies-out" in proc.stderr, proc.stderr[-400:]


@case
def default_journal_is_the_w4_directory() -> None:
    spec = importlib.util.spec_from_file_location("publish_artefact_bodies", SCRIPT)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    assert module.DEFAULT_JOURNAL == ROOT / "docs" / "evidence" / "record-w4" / "replay.jsonl"
    assert "record-w3" not in str(module.DEFAULT_JOURNAL)


@case
def help_lists_the_contract_flags() -> None:
    proc = run("--help")
    assert proc.returncode == 0, proc.stderr
    for flag in (
        "--standard", "--snapshot", "--outputs-dir", "--category", "--project",
        "--all-projects", "--all", "--check", "--dry-run", "--apply", "--local-bin",
        "--actor-id", "--journal", "--bodies-out", "--self-test",
    ):
        assert flag in proc.stdout, flag


def main() -> int:
    failures = 0
    for fn in CASES:
        try:
            fn()
        except Exception as exc:  # noqa: BLE001 - report every failure, then exit 1.
            failures += 1
            print(f"FAIL {fn.__name__}: {exc!r}")
        else:
            print(f"PASS {fn.__name__}")
    print(f"{len(CASES)} cases, {failures} failed")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
