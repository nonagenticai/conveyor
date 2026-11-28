#!/usr/bin/env python3
"""Build the one rework register from a snapshot, for every reader that needs it.

The register is derived, never stored: the conformance audit, the fresh standards findings
and the checked-in finding ledgers are merged by `audit-programme-conformance.py
--rework-out`. `verify-project.py` already built it per receipt; the rework factory needs
the same document, against its own post-write snapshot, to know which of a project's rework
cards the audit now reports as cleared. One implementation, so the receipt and the card
discharge can never disagree about what is open.

Usage:
    python3 scripts/rework_register.py --snapshot /tmp/snap/data --out /tmp/rework.json
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
AUDIT = ROOT / "scripts" / "audit-programme-conformance.py"
GENERATOR = ROOT / "scripts" / "generate-standards.py"
V1_LEDGER = ROOT / "docs" / "evidence" / "v1-conformance-findings.json"
DEEP_REVIEWS = (
    ROOT / "docs" / "evidence" / "record-w3" / "deep-review-findings.json",
    ROOT / "docs" / "evidence" / "record-w4" / "deep-review-findings.json",
)


class RegisterError(RuntimeError):
    """The register could not be derived from the snapshot."""


def _run(command: list[str], label: str) -> None:
    completed = subprocess.run(command, capture_output=True, text=True, check=False)
    if completed.returncode != 0:
        detail = completed.stderr.strip() or completed.stdout.strip()
        raise RegisterError(f"{label}: {detail[:300]}")


def build(snapshot_dir: Path, outputs_dir: Path | None = None) -> dict[str, Any]:
    """The rework register for one snapshot.

    `outputs_dir` is the rendered page directory beside the snapshot (`<root>/docs`); the
    standards generator reads it to tell a published document from one with no page.
    """
    snapshot = Path(snapshot_dir).resolve()
    if not (snapshot / "programme.json").is_file():
        raise RegisterError(f"{snapshot} is not a snapshot directory")
    outputs = Path(outputs_dir).resolve() if outputs_dir else snapshot.parent / "docs"
    work = Path(tempfile.mkdtemp(prefix="rework-register-"))
    try:
        conformance = work / "conformance"
        conformance.mkdir()
        _run(
            [
                sys.executable,
                str(AUDIT),
                "--snapshot",
                str(snapshot),
                "--out",
                str(conformance),
                "--snapshot-label",
                "docs/design/data",
                "--report",
            ],
            "the conformance audit failed",
        )
        standards = work / "standards-findings.json"
        _run(
            [
                sys.executable,
                str(GENERATOR),
                "--snapshot",
                str(snapshot),
                "--outputs-dir",
                str(outputs),
                "--gaps",
                str(conformance / "conformance-gap-register.json"),
                "--findings-out",
                str(standards),
            ],
            "the standards generator failed",
        )
        register_path = work / "rework.json"
        register_out = work / "register-conformance"
        register_out.mkdir()
        command = [
            sys.executable,
            str(AUDIT),
            "--snapshot",
            str(snapshot),
            "--out",
            str(register_out),
            "--snapshot-label",
            "docs/design/data",
            "--path-label",
            f"{standards}=docs/evidence/standards-findings.json",
            "--report",
            "--rework-out",
            str(register_path),
            "--findings",
            str(V1_LEDGER),
            "--findings",
            str(standards),
        ]
        for ledger in DEEP_REVIEWS:
            command += ["--deep-review", str(ledger)]
        _run(command, "the rework register failed")
        return json.loads(register_path.read_text(encoding="utf-8"))
    finally:
        shutil.rmtree(work, ignore_errors=True)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--snapshot", type=Path, required=True)
    parser.add_argument("--outputs-dir", type=Path, default=None)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        register = build(args.snapshot, args.outputs_dir)
    except RegisterError as error:
        print(f"rework_register: {error}", file=sys.stderr)
        return 1
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(
        json.dumps(register, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    summary = register.get("summary") or {}
    print(
        f"rework_register: {summary.get('packages', 0)} package(s), "
        f"{summary.get('open', 0)} open"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
