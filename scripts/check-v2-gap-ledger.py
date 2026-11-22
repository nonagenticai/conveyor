#!/usr/bin/env python3
"""Verify `docs/evidence/v2-gap-ledger.json` against its three sources.

The ledger unifies: absent artefacts from the regenerated 795-artefact record, stale
`conformance-baseline.json` entries with no live gap, and the open v1 code findings. This
checker recomputes each source set and proves:

  * the row counts and key sets equal the sources (no extra, no missing row);
  * every row carries an owner (possibly `unassigned`) and a fix card;
  * no key is duplicated;
  * every open v1 finding id appears exactly once as a `ledger:` row.

Defaults are the lane's retained inputs under `/tmp`; pass the flags to check another run.

    python3 scripts/check-v2-gap-ledger.py
    python3 scripts/check-v2-gap-ledger.py --self-test

Exit 0 = all proofs hold; exit 1 = at least one proof failed.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import re
import subprocess
import sys
import tempfile
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_LEDGER = ROOT / "docs/evidence/v2-gap-ledger.json"
DEFAULT_FINDINGS = ROOT / "docs/evidence/v1-conformance-findings.json"
DEFAULT_BASELINE = ROOT / "docs/evidence/conformance-baseline.json"
DEFAULT_GAPS = Path("/tmp/conf-0750/conformance-gap-register.json")
DEFAULT_SNAPSHOT = Path("/tmp/snap-526-0750/data")
DEFAULT_REGISTER = Path("/tmp/conf-0750/rework.json")
KINDS = ("artefact", "baseline", "ledger")
STATUS = {"artefact": "absent", "baseline": "stale", "ledger": "open"}
DEPLOY = re.compile(
    r"\b(authoring\.intake|authoring\.draft|authoring\.review|authoring\.publish|"
    r"authoring\.assets|00(?:62|63|64|65|66))\b"
)


def read(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def regenerate(snapshot: Path, register: Path, out: Path) -> dict:
    """Rebuild the 795-artefact record and its findings with the unchanged generator."""
    out.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            sys.executable,
            str(ROOT / "scripts/generate-standards.py"),
            "--snapshot", str(snapshot),
            "--register", str(register),
            "--data-out", str(out),
            "--findings-out", str(out / "findings.json"),
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    return {
        "standards": read(out / "standards.json"),
        "findings": read(out / "findings.json")["findings"],
        "register": read(register),
        "baseline": read(DEFAULT_BASELINE),
        "gaps": read(DEFAULT_GAPS)["gaps"],
        "v1": read(DEFAULT_FINDINGS)["findings"],
    }


def expected(source: dict) -> dict:
    absent = [a for a in source["standards"]["artefacts"] if a["status"] == "absent"]
    gap_keys = {(g["rule"], g.get("subject")) for g in source["gaps"]}
    stale = [
        e for e in source["baseline"]["entries"]
        if (e["rule"], e["subject"]) not in gap_keys
    ]
    open_findings = [f for f in source["v1"] if not f.get("resolved_by")]
    return {
        "absent": {a["id"]: a for a in absent},
        "stale": {(e["rule"], e["subject"]): e for e in stale},
        "open": {f["id"]: f for f in open_findings},
        "register_keys": {p["key"] for p in source["register"]["packages"]},
    }


def row_text(row: dict, source: dict, expected_sets: dict) -> str:
    if row["kind"] == "ledger":
        finding = expected_sets["open"].get(row["fix_card"], {})
        return " ".join(str(finding.get(k, "")) for k in
                        ("title", "problem", "acceptance", "failure_scenario"))
    if row["kind"] == "artefact":
        artefact = expected_sets["absent"].get(row["key"].split(":", 1)[1], {})
        generated = next((f for f in source["findings"] if f["id"] == artefact.get("id")), {})
        return " ".join(
            [str(artefact.get("category", ""))]
            + [str(generated.get(k, "")) for k in
               ("title", "problem", "acceptance", "failure_scenario")]
        )
    return ""


def check(ledger: dict, source: dict) -> list[str]:
    problems: list[str] = []
    exp = expected(source)
    if ledger.get("schema") != "v2-gap-ledger/v1":
        problems.append(f"schema is {ledger.get('schema')!r}, expected v2-gap-ledger/v1")
    if ledger.get("lane_pin") != "ffc13a0f":
        problems.append(f"lane_pin is {ledger.get('lane_pin')!r}, expected ffc13a0f")
    rows = ledger.get("rows")
    if not isinstance(rows, list) or not rows:
        return problems + ["rows is missing or empty"]

    keys = [str(row.get("key") or "") for row in rows]
    duplicates = sorted(key for key, count in Counter(keys).items() if count > 1)
    if duplicates:
        problems.append(f"duplicate keys: {duplicates[:5]}")
    for row in rows:
        kind = row.get("kind")
        if kind not in KINDS:
            problems.append(f"{row.get('key')}: kind {kind!r} is not one of {KINDS}")
            continue
        for field in ("key", "standard", "spec", "subject", "project", "title",
                      "status", "severity", "owner", "fix_card", "target_phase"):
            if not str(row.get(field) or "").strip():
                problems.append(f"{row.get('key')}: field {field!r} is empty")
        if row.get("status") != STATUS[kind]:
            problems.append(f"{row.get('key')}: status {row.get('status')!r} != {STATUS[kind]!r}")
        if not isinstance(row.get("deploy_bound"), bool):
            problems.append(f"{row.get('key')}: deploy_bound is not a boolean")
        elif row["deploy_bound"] != bool(DEPLOY.search(row_text(row, source, exp))):
            problems.append(f"{row.get('key')}: deploy_bound disagrees with its source text")

    got = {kind: {str(r["key"]) for r in rows if r.get("kind") == kind} for kind in KINDS}
    want = {
        "artefact": {f"artefact:{key}" for key in exp["absent"]},
        "baseline": {f"baseline:{rule}:{subject}" for rule, subject in exp["stale"]},
        "ledger": {f"ledger:{key}" for key in exp["open"]},
    }
    for kind in KINDS:
        if got[kind] != want[kind]:
            missing = sorted(want[kind] - got[kind])[:3]
            extra = sorted(got[kind] - want[kind])[:3]
            problems.append(
                f"{kind}: key set mismatch (missing {missing}, extra {extra}, "
                f"want {len(want[kind])}, got {len(got[kind])})"
            )

    open_keys = [row["key"] for row in rows if row.get("kind") == "ledger"]
    if len(open_keys) != len(exp["open"]):
        problems.append(f"open findings: {len(open_keys)} rows, source has {len(exp['open'])}")
    if len(set(open_keys)) != len(open_keys):
        problems.append("open findings: a ledger row is repeated")
    for identifier in exp["open"]:
        if open_keys.count(f"ledger:{identifier}") != 1:
            problems.append(f"open finding {identifier} does not appear exactly once")

    for row in rows:
        if row.get("owner") != "unassigned" and not str(row.get("owner") or "").strip():
            problems.append(f"{row.get('key')}: owner is empty and not 'unassigned'")
        if row.get("kind") == "ledger":
            finding = exp["open"].get(row.get("fix_card"))
            if finding is None:
                problems.append(f"{row.get('key')}: fix_card is not an open finding id")
            else:
                if row.get("owner") != finding["owner_project"]:
                    problems.append(f"{row.get('key')}: owner != finding owner_project")
                if row.get("severity") != finding["severity"]:
                    problems.append(f"{row.get('key')}: severity != finding severity")
        elif row.get("kind") == "baseline":
            if row.get("fix_card") != row.get("key"):
                problems.append(f"{row.get('key')}: baseline fix_card is not its key")
        else:
            artefact = exp["absent"].get(str(row.get("key")).split(":", 1)[1])
            if artefact is None:
                problems.append(f"{row.get('key')}: not an absent artefact in the record")
                continue
            published = str(artefact.get("package_key") or "")
            if not (
                row.get("fix_card") in exp["register_keys"]
                or row.get("fix_card") in {published, artefact["id"]}
            ):
                problems.append(f"{row.get('key')}: fix_card is neither a package key nor finding id")

    counts = ledger.get("counts") or {}
    if counts.get("rows") != len(rows):
        problems.append(f"counts.rows {counts.get('rows')} != {len(rows)}")
    if counts.get("by_kind") != {kind: len(want[kind]) for kind in KINDS}:
        problems.append(f"counts.by_kind {counts.get('by_kind')} != source counts")
    return problems


def self_test(ledger: dict, source: dict) -> list[str]:
    """Prove the checker refuses each defect the card names."""
    failures: list[str] = []
    mutations = [
        ("duplicate key", lambda doc: doc["rows"].append(copy.deepcopy(doc["rows"][0]))),
        ("missing owner", lambda doc: doc["rows"][0].update(owner="")),
        ("missing fix card", lambda doc: doc["rows"][0].update(fix_card="")),
        ("missing open finding", lambda doc: doc["rows"].pop(
            next(i for i, row in enumerate(doc["rows"]) if row["kind"] == "ledger"))),
        ("missing artefact row", lambda doc: doc["rows"].pop(
            next(i for i, row in enumerate(doc["rows"]) if row["kind"] == "artefact"))),
    ]
    for name, mutate in mutations:
        broken = copy.deepcopy(ledger)
        mutate(broken)
        if not check(broken, source):
            failures.append(f"negative control not refused: {name}")
        else:
            print(f"  self-test refused: {name}")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ledger", type=Path, default=DEFAULT_LEDGER)
    parser.add_argument("--snapshot", type=Path, default=DEFAULT_SNAPSHOT)
    parser.add_argument("--register", type=Path, default=DEFAULT_REGISTER)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    ledger = read(args.ledger)
    with tempfile.TemporaryDirectory(prefix="v2-gap-ledger-") as temporary:
        source = regenerate(args.snapshot, args.register, Path(temporary))
        problems = check(ledger, source)
        if args.self_test:
            problems += self_test(ledger, source)

    if problems:
        print("v2-gap-ledger: FAIL", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1
    rows = ledger["rows"]
    severity = Counter(row["severity"] for row in rows)
    owners = Counter(row["owner"] for row in rows)
    deploy = Counter("true" if row["deploy_bound"] else "false" for row in rows)
    print("v2-gap-ledger: PASS")
    print(f"  rows {len(rows)} = artefact {owners_total(rows, 'artefact')} + "
          f"baseline {owners_total(rows, 'baseline')} + ledger {owners_total(rows, 'ledger')}")
    print("  severity " + " · ".join(f"{name} {severity[name]}" for name in sorted(severity)))
    print(f"  deploy_bound false {deploy['false']} · true {deploy['true']}")
    print(f"  owners non-empty {len(rows)} (unassigned {owners['unassigned']})")
    print(f"  fix cards non-empty {sum(1 for row in rows if row['fix_card'])}")
    print(f"  duplicate keys 0 · open ledger ids {owners_total(rows, 'ledger')}, each exactly once")
    if args.self_test:
        print("  self-test: PASS (5 negative controls refused)")
    return 0


def owners_total(rows: list[dict], kind: str) -> int:
    return sum(1 for row in rows if row["kind"] == kind)


if __name__ == "__main__":
    raise SystemExit(main())
