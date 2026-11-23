#!/usr/bin/env python3
"""Tests for `scripts/compose-artefact-bodies.py`.

Standalone: no database, no network, no Rust build. Each case prints one PASS line;
the process exits 0 only when every case passed.
"""

from __future__ import annotations

import hashlib
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "compose-artefact-bodies.py"
FIX = ROOT / "tests" / "fixtures" / "compose"
SNAPSHOT = FIX / "snapshot"
CATEGORY = "fixture-artefact"

CASES: list = []


def case(fn):
    CASES.append(fn)
    return fn


def run(standard: Path, snapshot: Path = SNAPSHOT, out: Path | None = None):
    command = [
        sys.executable, str(SCRIPT),
        "--standard", str(standard),
        "--snapshot", str(snapshot),
        "--category", CATEGORY,
        "--subject-kind", "project",
        "--subject", "PX",
    ]
    if out is not None:
        command += ["--out", str(out)]
    return subprocess.run(command, capture_output=True, text=True)


def facts(body: str) -> list[dict]:
    blocks = re.findall(r"```json\n(.*?)\n```", body, re.DOTALL)
    return [json.loads(block) for block in blocks]


def assert_refusal(standard: Path, code: str, snapshot: Path = SNAPSHOT) -> None:
    with tempfile.TemporaryDirectory() as tmp:
        out = Path(tmp) / "summary.json"
        proc = run(standard, snapshot=snapshot, out=out)
        assert proc.returncode == 1, (proc.returncode, proc.stdout, proc.stderr)
        error = json.loads(proc.stderr)
        assert error["code"] == code, error
        assert proc.stdout.strip() == "", proc.stdout
        assert not out.exists(), f"{code} wrote a body to --out"


@case
def positive_composes_body() -> None:
    proc = run(FIX / "positive-standard.json")
    assert proc.returncode == 0, proc.stderr
    summary = json.loads(proc.stdout)
    body = summary["body"]
    assert summary["schema"] == "artefact-body/v1"
    assert summary["standard"] == "FIXTURE"
    assert summary["spec"] == "SPEC-fixture"
    assert summary["title"] == "Fixture Artefact — Compose Fixture Project"
    assert body.startswith("# Fixture Artefact — Compose Fixture Project\n"), body[:80]
    assert body.index("## Scope") < body.index("## Evidence")
    assert "The people and systems the work touches." in body
    assert "The recorded proof for each acceptance claim." in body
    assert [heading["heading"] for heading in summary["headings"]] == ["Scope", "Evidence"]
    assert [heading["required"] for heading in summary["headings"]] == [True, True]
    assert [source["count"] for source in summary["headings"][0]["sources"]] == [1]
    assert [source["count"] for source in summary["headings"][1]["sources"]] == [2]
    composed = facts(body)
    assert [fact["source"] for fact in composed] == [
        "programme.json#/projects", "msp.json#/items",
    ]
    assert [fact["count"] for fact in composed] == [1, 2]
    assert [row["id"] for row in composed[1]["rows"]] == ["ev-1", "ev-2"]
    assert summary["body_sha256"] == hashlib.sha256(body.encode("utf-8")).hexdigest()
    assert len(summary["skeleton_sha256"]) == 64


@case
def out_writes_summary_file() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        out = Path(tmp) / "summary.json"
        proc = run(FIX / "positive-standard.json", out=out)
        assert proc.returncode == 0, proc.stderr
        assert proc.stdout.strip() == "", proc.stdout
        summary = json.loads(out.read_text(encoding="utf-8"))
        assert summary["schema"] == "artefact-body/v1"
        assert summary["body_sha256"] == hashlib.sha256(
            summary["body"].encode("utf-8")
        ).hexdigest()


@case
def missing_source_is_refused() -> None:
    assert_refusal(FIX / "missing-source-standard.json", "MISSING_SOURCE")


@case
def empty_pointer_literal_is_refused() -> None:
    assert_refusal(FIX / "empty-pointer-standard.json", "EMPTY_POINTER")


@case
def empty_pointer_zero_rows_is_refused() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        snapshot = Path(tmp) / "snapshot"
        shutil.copytree(SNAPSHOT, snapshot)
        (snapshot / "msp.json").write_text(json.dumps({"items": []}), encoding="utf-8")
        assert_refusal(FIX / "positive-standard.json", "EMPTY_POINTER", snapshot=snapshot)


@case
def missing_heading_is_refused() -> None:
    assert_refusal(FIX / "missing-heading-standard.json", "MISSING_HEADING")


@case
def placeholder_is_refused() -> None:
    assert_refusal(FIX / "placeholder-standard.json", "PLACEHOLDER")


@case
def no_evidence_is_refused() -> None:
    # Every source is optional and resolves to zero rows. The skeleton's intent paragraphs are
    # not evidence: a heading-only body must never become a published artefact.
    assert_refusal(FIX / "no-evidence-standard.json", "NO_EVIDENCE")


@case
def self_test_passes() -> None:
    proc = subprocess.run(
        [sys.executable, str(SCRIPT), "--self-test"], capture_output=True, text=True,
    )
    assert proc.returncode == 0, proc.stderr
    assert proc.stdout.count("PASS self-test") >= 1, proc.stdout


def main() -> int:
    failures = 0
    for fn in CASES:
        try:
            fn()
        except Exception as exc:  # noqa: BLE001 - the runner reports every failure, then exits 1.
            failures += 1
            print(f"FAIL {fn.__name__}: {exc!r}")
        else:
            print(f"PASS {fn.__name__}")
    print(f"{len(CASES)} cases, {failures} failed")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
