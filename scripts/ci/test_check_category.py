#!/usr/bin/env python3
"""Tests for `scripts/generate-standards.py --check-category`.

Standalone: no database, no network, no Rust build. Each case prints one PASS line;
the process exits 0 only when every case passed.
"""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "generate-standards.py"
FIX = ROOT / "tests" / "fixtures" / "check-category"
SNAPSHOT = FIX / "snapshot"
CATEGORY = "fixture-artefact"

CASES: list = []


def case(fn):
    CASES.append(fn)
    return fn


def run(standard: Path, key: str, snapshot: Path, outputs: Path):
    return subprocess.run(
        [
            sys.executable, str(SCRIPT),
            "--check-category", key,
            "--standard", str(standard),
            "--snapshot", str(snapshot),
            "--outputs-dir", str(outputs),
        ],
        capture_output=True, text=True,
    )


def sandbox(tmp: Path) -> tuple[Path, Path]:
    snapshot = tmp / "snapshot"
    shutil.copytree(SNAPSHOT, snapshot)
    outputs = tmp / "out"
    outputs.mkdir()
    return snapshot, outputs


def documents(snapshot: Path) -> list[dict]:
    return json.loads((snapshot / "artefacts.json").read_text(encoding="utf-8"))["documents"]


def write_documents(snapshot: Path, rows: list[dict]) -> None:
    (snapshot / "artefacts.json").write_text(
        json.dumps({"documents": rows}), encoding="utf-8",
    )


@case
def positive_is_clean_and_writes_nothing() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        snapshot, outputs = sandbox(Path(tmp))
        before = sorted(path.name for path in snapshot.iterdir())
        proc = run(FIX / "standard.json", CATEGORY, snapshot, outputs)
        assert proc.returncode == 0, (proc.returncode, proc.stdout, proc.stderr)
        assert "ok (1 instance(s))" in proc.stdout, proc.stdout
        assert proc.stderr == "", proc.stderr
        assert not list(outputs.iterdir()), "the check wrote into the outputs directory"
        assert sorted(path.name for path in snapshot.iterdir()) == before


@case
def missing_document_exits_1() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        snapshot, outputs = sandbox(Path(tmp))
        write_documents(snapshot, [])
        proc = run(FIX / "standard.json", CATEGORY, snapshot, outputs)
        assert proc.returncode == 1, (proc.returncode, proc.stdout, proc.stderr)
        assert "no published fixture-artefact document" in proc.stdout, proc.stdout


@case
def stale_hash_exits_1() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        snapshot, outputs = sandbox(Path(tmp))
        rows = documents(snapshot)
        rows[0]["meta"] = {"sha256": "0" * 64}
        write_documents(snapshot, rows)
        proc = run(FIX / "standard.json", CATEGORY, snapshot, outputs)
        assert proc.returncode == 1, (proc.returncode, proc.stdout, proc.stderr)
        assert "stale document doc-1" in proc.stdout, proc.stdout
        assert "differs from the recorded" in proc.stdout, proc.stdout


@case
def unknown_category_exits_2() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        snapshot, outputs = sandbox(Path(tmp))
        proc = run(FIX / "standard.json", "no-such-category", snapshot, outputs)
        assert proc.returncode == 2, (proc.returncode, proc.stdout, proc.stderr)
        assert "unknown category" in proc.stdout, proc.stdout


@case
def untriggered_on_demand_exits_0_with_exact_line() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        snapshot, outputs = sandbox(Path(tmp))
        proc = run(FIX / "standard.json", "fixture-ondemand", snapshot, outputs)
        assert proc.returncode == 0, (proc.returncode, proc.stdout, proc.stderr)
        assert proc.stdout == "on-demand untriggered: fixture-ondemand\n", proc.stdout


@case
def composition_refusal_exits_1() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        snapshot, outputs = sandbox(Path(tmp))
        proc = run(FIX / "refusal-standard.json", CATEGORY, snapshot, outputs)
        assert proc.returncode == 1, (proc.returncode, proc.stdout, proc.stderr)
        assert "composition refused (MISSING_SOURCE)" in proc.stdout, proc.stdout


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
