#!/usr/bin/env python3
"""Refuse a merge that increases the programme's required evidence gap.

`docs/design/data/standards.js` is the committed scorecard. This check reads it at a base git ref
and at the head and fails when the number of **required due-absent** artefacts grows. Optional
rows (`mandatory: false`) and rows outside the declared scope (`scheduled` / `excluded`) are
reported but never gated: they are visible work, not conformity debt.

A standard change that adds a required row is therefore refused unless the same change also
evidences it, or declares the row scheduled. That is the ratchet the v4 expansion (#920) lacked:
it added 160 required rows while the due-absent count rose from the instrument's own change.

Usage:
    python3 scripts/check-evidence-ratchet.py --base origin/main
    python3 scripts/check-evidence-ratchet.py --base origin/main --head docs/design/data/standards.js
    python3 scripts/check-evidence-ratchet.py --self-test

Exit: 0 no growth; 1 required due-absent grew; 2 unreadable or unparsable input.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_HEAD = ROOT / "docs" / "design" / "data" / "standards.js"
BASE_PATH = "docs/design/data/standards.js"


def load_text(text: str) -> dict[str, Any]:
    """Parse the `window.CONVEYOR_STANDARDS = {...};` wrapper without importing the pack."""
    start = text.find("{")
    end = text.rfind("}")
    if start < 0 or end <= start:
        raise ValueError("no JSON object in the standards record")
    document = json.loads(text[start : end + 1])
    if not isinstance(document, dict) or not isinstance(document.get("artefacts"), list):
        raise ValueError("the standards record carries no artefacts array")
    return document


def load_ref(ref: str) -> dict[str, Any]:
    proc = subprocess.run(
        ["git", "show", f"{ref}:{BASE_PATH}"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        raise ValueError(
            f"git show {ref}:{BASE_PATH} failed: {proc.stderr.strip() or proc.returncode}"
        )
    return load_text(proc.stdout)


def is_due(artefact: dict[str, Any]) -> bool:
    """A row without a due_state is a pre-v4 row; the whole record was due then."""
    return artefact.get("due_state", "due") == "due"


def is_required(artefact: dict[str, Any]) -> bool:
    return bool(artefact.get("mandatory", True))


def is_absent(artefact: dict[str, Any]) -> bool:
    return artefact.get("status") == "absent"


def counts(document: dict[str, Any]) -> dict[str, int]:
    due = [a for a in document.get("artefacts") or [] if is_due(a)]
    required = [a for a in due if is_required(a)]
    optional = [a for a in due if not is_required(a)]
    return {
        "required_due": len(required),
        "required_absent": sum(1 for a in required if is_absent(a)),
        "optional_due": len(optional),
        "optional_absent": sum(1 for a in optional if is_absent(a)),
    }


def absent_by_category(document: dict[str, Any], required: bool) -> dict[tuple[str, str], int]:
    rows: dict[tuple[str, str], int] = {}
    for artefact in document.get("artefacts") or []:
        if not is_due(artefact) or not is_absent(artefact):
            continue
        if is_required(artefact) != required:
            continue
        key = (str(artefact.get("standard") or "?"), str(artefact.get("category") or "?"))
        rows[key] = rows.get(key, 0) + 1
    return rows


def compare(base: dict[str, Any], head: dict[str, Any], report: bool = True) -> int:
    before = counts(base)
    after = counts(head)
    if report:
        print(
            "check-evidence-ratchet: required_absent "
            f"{before['required_absent']} -> {after['required_absent']}; "
            f"optional_absent {before['optional_absent']} -> {after['optional_absent']}"
        )
    if after["required_absent"] <= before["required_absent"]:
        return 0
    if not report:
        return 1
    old = absent_by_category(base, required=True)
    new = absent_by_category(head, required=True)
    grew = [
        (key, amount)
        for key, amount in new.items()
        if amount > old.get(key, 0)
    ]
    for (standard, category), amount in sorted(grew, key=lambda row: (-row[1], row[0])):
        print(f"  +{amount} {standard}/{category}", file=sys.stderr)
    print(
        "check-evidence-ratchet: required due-absent grew by "
        f"{after['required_absent'] - before['required_absent']}. Evidence the new rows in this "
        "change, or declare them `scheduled`; do not raise the denominator.",
        file=sys.stderr,
    )
    return 1


def make(rows: list[tuple[str, str, str, str, bool]]) -> dict[str, Any]:
    """A tiny record: (standard, category, due_state, status, mandatory)."""
    return {
        "artefacts": [
            {
                "standard": standard,
                "category": category,
                "due_state": due_state,
                "status": status,
                "mandatory": mandatory,
            }
            for standard, category, due_state, status, mandatory in rows
        ]
    }


def self_test() -> None:
    base = make(
        [
            ("MSP", "vision", "due", "absent", True),
            ("PMP", "brief", "due", "absent", False),
            ("PMP", "wbs", "due", "evidenced", True),
            ("TOGAF", "phase-a", "scheduled", "absent", True),
        ]
    )
    equal = make(
        [
            ("MSP", "vision", "due", "absent", True),
            ("PMP", "brief", "due", "absent", False),
            ("PMP", "wbs", "due", "evidenced", True),
            ("TOGAF", "phase-a", "scheduled", "absent", True),
        ]
    )
    growth = make(
        [
            ("MSP", "vision", "due", "absent", True),
            ("MSP", "blueprint", "due", "absent", True),
            ("PMP", "brief", "due", "absent", False),
            ("PMP", "wbs", "due", "evidenced", True),
            ("TOGAF", "phase-a", "scheduled", "absent", True),
        ]
    )
    optional_growth = make(
        [
            ("MSP", "vision", "due", "absent", True),
            ("PMP", "brief", "due", "absent", False),
            ("PMP", "inception", "due", "absent", False),
            ("PMP", "wbs", "due", "evidenced", True),
            ("TOGAF", "phase-a", "scheduled", "absent", True),
        ]
    )
    reduced = make(
        [
            ("MSP", "vision", "due", "evidenced", True),
            ("PMP", "brief", "due", "absent", False),
            ("PMP", "wbs", "due", "evidenced", True),
            ("TOGAF", "phase-a", "scheduled", "absent", True),
        ]
    )
    before = counts(base)
    assert before == {
        "required_due": 2,
        "required_absent": 1,
        "optional_due": 1,
        "optional_absent": 1,
    }, before
    assert compare(base, equal, report=False) == 0, "an equal count passes"
    assert compare(base, optional_growth, report=False) == 0, "optional growth never gates"
    assert compare(base, growth, report=False) == 1, "required growth fails"
    assert compare(base, reduced, report=False) == 0, "a reduction passes"
    assert compare(growth, base, report=False) == 0, "a reduction from the other direction passes"

    wrapped = 'window.CONVEYOR_STANDARDS = ' + json.dumps(base) + ";\n"
    assert load_text(wrapped) == base, "the wrapper round-trips"
    try:
        load_text("not a record")
    except ValueError:
        pass
    else:
        raise AssertionError("a missing object must not parse")
    print("check-evidence-ratchet: self-test ok")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", help="the git ref whose standards record is the floor")
    parser.add_argument("--head", type=Path, default=DEFAULT_HEAD, help="the record to check")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if not args.base:
        parser.error("--base is required")
    try:
        base = load_ref(args.base)
        head = load_text(args.head.read_text(encoding="utf-8"))
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"check-evidence-ratchet: {error}", file=sys.stderr)
        return 2
    return compare(base, head)


if __name__ == "__main__":
    sys.exit(main())
