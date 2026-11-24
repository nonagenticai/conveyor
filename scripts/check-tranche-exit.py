#!/usr/bin/env python3
"""Refuse a tranche `continue` while any member is red, stale or missing.

A tranche review records a decision over a *set* of projects; the audit's
`tranche_review_without_authorisation` checks the review's own link, but nothing executable
reads the members' receipts. This is that check: it resolves the tranche's membership from
the live snapshot (`meta.tranche`), reads each member's latest acceptance receipt, and exits 0
only when every member is green **at the snapshot's head sha**.

A receipt is fresh only when its `snapshot.git_sha` equals the snapshot manifest's `git_sha`;
a green receipt from an older head proves nothing about this one. A member with no receipt is
not green.

Usage:
    python3 scripts/check-tranche-exit.py --tranche 4 --snapshot /tmp/snap/data
    python3 scripts/check-tranche-exit.py --tranche 4 --snapshot /tmp/snap/data --json-out /tmp/exit.json
    python3 scripts/check-tranche-exit.py --self-test

Exit: 0 every member green and fresh; 1 any red/stale/missing member; 2 usage or unreadable
input. The review that consumes this runs only on exit 0; otherwise it records `pause`, which
authorises nothing.
"""

from __future__ import annotations

import argparse
import json
import sys
import tempfile
from pathlib import Path
from typing import Any, NoReturn

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_EVIDENCE = ROOT / "docs" / "evidence" / "rework"


def fail(message: str, code: int = 2) -> "NoReturn":
    print(f"check-tranche-exit: {message}", file=sys.stderr)
    raise SystemExit(code)


def load_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        fail(f"missing {path}")
    except json.JSONDecodeError as error:
        fail(f"{path} is not JSON: {error}")


def members(snapshot: Path, tranche: int) -> list[dict[str, Any]]:
    programme = load_json(snapshot / "programme.json")
    found = [
        project
        for project in programme.get("projects") or []
        if str((project.get("meta") or {}).get("tranche")) == str(tranche)
    ]
    return sorted(found, key=lambda project: project.get("key") or "")


def judge(snapshot: Path, evidence_dir: Path, tranche: int) -> dict[str, Any]:
    manifest = load_json(snapshot / "manifest.json")
    head = str(manifest.get("git_sha") or "")
    rows: list[dict[str, Any]] = []
    for project in members(snapshot, tranche):
        key = str(project.get("key") or "?")
        path = evidence_dir / f"{key}.json"
        row: dict[str, Any] = {"key": key, "status": project.get("status")}
        if not path.exists():
            row.update(state="missing", reason=f"no receipt at {path}")
            rows.append(row)
            continue
        receipt = load_json(path)
        sha = str((receipt.get("snapshot") or {}).get("git_sha") or "")
        verdict = receipt.get("verdict")
        row["receipt_sha"] = sha
        row["verdict"] = verdict
        if sha != head:
            row.update(
                state="stale",
                reason=f"receipt is at {sha or 'no sha'}, the snapshot is at {head}",
            )
        elif verdict != "green":
            row.update(state="red", reason=f"verdict {verdict!r}")
        else:
            row.update(state="green", reason=f"{len(receipt.get('checks') or [])} checks")
        rows.append(row)
    bad = [row for row in rows if row["state"] != "green"]
    return {
        "schema": "tranche-exit/v1",
        "tranche": tranche,
        "snapshot_sha": head,
        "members": rows,
        "ready": not bad and bool(rows),
        "blocking": [
            f"{row['key']}: {row['state']} — {row['reason']}" for row in bad
        ]
        or ([] if rows else [f"tranche {tranche} has no members in the snapshot"]),
    }


def print_report(report: dict[str, Any]) -> None:
    print(f"tranche {report['tranche']} at {report['snapshot_sha'] or 'no sha'}")
    for row in report["members"]:
        print(f"  {row['key']:<10} {row['state']:<8} {row['reason']}")
    if report["ready"]:
        print(f"tranche {report['tranche']} exit criteria hold: every member is green and fresh")
    else:
        print(f"tranche {report['tranche']} is NOT ready to continue:")
        for line in report["blocking"]:
            print(f"  - {line}")


def self_test() -> int:
    failures: list[str] = []
    work = Path(tempfile.mkdtemp(prefix="tranche-exit-selftest-"))
    snapshot = work / "snapshot"
    snapshot.mkdir()
    (snapshot / "manifest.json").write_text(json.dumps({"git_sha": "head-1"}))
    (snapshot / "programme.json").write_text(
        json.dumps(
            {
                "projects": [
                    {"key": "A", "status": "closed", "meta": {"tranche": 4}},
                    {"key": "B", "status": "active", "meta": {"tranche": 4}},
                    {"key": "C", "status": "closed", "meta": {"tranche": 5}},
                ]
            }
        )
    )
    evidence = work / "evidence"
    evidence.mkdir()

    def receipt(key: str, verdict: str = "green", sha: str = "head-1") -> None:
        (evidence / f"{key}.json").write_text(
            json.dumps(
                {
                    "project": key,
                    "verdict": verdict,
                    "snapshot": {"git_sha": sha},
                    "checks": [{"id": f"{key}-project", "result": "pass"}],
                }
            )
        )

    # Every member green and fresh -> ready.
    receipt("A")
    receipt("B")
    report = judge(snapshot, evidence, 4)
    if not report["ready"]:
        failures.append(f"green tranche refused: {report['blocking']}")
    if [row["key"] for row in report["members"]] != ["A", "B"]:
        failures.append(f"membership resolved wrong: {report['members']}")

    # One red member -> refused, and the red member is named.
    receipt("B", verdict="red")
    report = judge(snapshot, evidence, 4)
    if report["ready"]:
        failures.append("a red member did not refuse the tranche")
    if not any("B: red" in line for line in report["blocking"]):
        failures.append(f"the red member is not named: {report['blocking']}")

    # A green receipt from an older head is stale -> refused.
    receipt("B")
    receipt("A", sha="head-0")
    report = judge(snapshot, evidence, 4)
    if report["ready"]:
        failures.append("a stale receipt did not refuse the tranche")
    if not any("A: stale" in line for line in report["blocking"]):
        failures.append(f"the stale member is not named: {report['blocking']}")

    # A missing receipt is not green.
    (evidence / "A.json").unlink()
    report = judge(snapshot, evidence, 4)
    if report["ready"] or not any("A: missing" in line for line in report["blocking"]):
        failures.append(f"a missing receipt was not refused: {report['blocking']}")

    # A tranche with no members is not ready.
    report = judge(snapshot, evidence, 9)
    if report["ready"] or not report["blocking"]:
        failures.append("an empty tranche reported ready")

    if failures:
        for failure in failures:
            print(f"SELFTEST FAIL: {failure}", file=sys.stderr)
        return 1
    print(f"check-tranche-exit: self-test OK — {4} negative controls")
    return 0


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tranche", type=int)
    parser.add_argument("--snapshot")
    parser.add_argument("--evidence-dir", type=Path, default=DEFAULT_EVIDENCE)
    parser.add_argument("--json-out", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        raise SystemExit(self_test())
    if args.tranche is None or not args.snapshot:
        fail("--tranche and --snapshot are required (or --self-test)")
    report = judge(Path(args.snapshot), args.evidence_dir, args.tranche)
    if args.json_out:
        args.json_out.write_text(json.dumps(report, indent=1) + "\n", encoding="utf-8")
    print_report(report)
    raise SystemExit(0 if report["ready"] else 1)


if __name__ == "__main__":
    main()
