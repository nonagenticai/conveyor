#!/usr/bin/env python3
"""The product-rework chain: a finding either becomes an executed work package or a waiver.

`governance/product-rework.json` is the register the product conformance audit writes and the
programme reads. This checker is the chain's gate:

  * every finding cites evidence that resolves, or is `unprovable` — a prose claim is not evidence;
  * a P0 finding is closed only by an executed work package (a commit, a named test and a demo
    credit) or by an accepted waiver with an owner and an unexpired date;
  * a package's WSJF is the four integer inputs; a computed `score` in the file is refused;
  * an executed package's findings exist, and its test is a real path::test or a command.

Usage:
    python3 scripts/check-product-rework.py
    python3 scripts/check-product-rework.py --self-test
"""

from __future__ import annotations

import argparse
import datetime
import json
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
REGISTER = ROOT / "governance" / "product-rework.json"
SEED = ROOT / "seed" / "programme.json"

SEVERITIES = {"P0", "P1", "P2"}
DISPOSITIONS = {"execute", "defer", "accept", "correct-the-claim", "unprovable"}
FINDING_STATUSES = {"draft", "verified", "adjudicated", "packaged", "closed"}
PACKAGE_STATUSES = {"planned", "executing", "executed", "waived", "deferred"}
KINDS = {"code_rework", "instrument_rework", "record_rework", "doc_correction"}
WSJF_FIELDS = ("business_value", "time_criticality", "risk_reduction", "job_size")
URL = re.compile(r"^https?://")


def problems(register: dict[str, Any], root: Path) -> list[str]:
    found: list[str] = []
    findings = register.get("findings")
    packages = register.get("work_packages")
    if not isinstance(findings, list) or not isinstance(packages, list):
        return ["the register needs `findings` and `work_packages` arrays"]

    by_id: dict[str, dict[str, Any]] = {}
    for finding in findings:
        fid = str(finding.get("id") or "")
        if not fid:
            found.append("a finding has no id")
            continue
        if fid in by_id:
            found.append(f"duplicate finding id {fid}")
        by_id[fid] = finding
        for field in ("slice", "project", "spec_clause", "claim", "observed", "verifier"):
            if not str(finding.get(field) or "").strip():
                found.append(f"{fid}: missing {field}")
        if finding.get("severity") not in SEVERITIES:
            found.append(f"{fid}: severity {finding.get('severity')!r}")
        if finding.get("disposition") not in DISPOSITIONS:
            found.append(f"{fid}: disposition {finding.get('disposition')!r}")
        if finding.get("status") not in FINDING_STATUSES:
            found.append(f"{fid}: status {finding.get('status')!r}")
        evidence = finding.get("evidence")
        if not isinstance(evidence, list) or not evidence:
            found.append(f"{fid}: evidence must be a non-empty array")
            evidence = []
        for item in evidence:
            text = str(item)
            if URL.match(text) or text.startswith(("probe:", "snapshot:", "commit:")):
                continue
            path = text.split("::")[0].split(":")[0]
            if not path or not (root / path).exists():
                found.append(f"{fid}: evidence does not resolve: {text!r}")
        if finding.get("disposition") == "accept":
            waiver = finding.get("waiver") or {}
            if not str(waiver.get("owner") or "").strip():
                found.append(f"{fid}: an accepted finding needs a waiver owner")
            if not str(waiver.get("reason") or "").strip():
                found.append(f"{fid}: an accepted finding needs a waiver reason")
            try:
                expiry = datetime.date.fromisoformat(str(waiver.get("expiry")))
            except ValueError:
                expiry = None
            if expiry is None or expiry < datetime.date.today():
                found.append(f"{fid}: an accepted finding needs an unexpired waiver date")

    covered: dict[str, list[dict[str, Any]]] = {}
    package_ids: set[str] = set()
    for package in packages:
        pid = str(package.get("id") or "")
        if not pid:
            found.append("a work package has no id")
            continue
        if pid in package_ids:
            found.append(f"duplicate work package id {pid}")
        package_ids.add(pid)
        if package.get("kind") not in KINDS:
            found.append(f"{pid}: kind {package.get('kind')!r}")
        if package.get("status") not in PACKAGE_STATUSES:
            found.append(f"{pid}: status {package.get('status')!r}")
        for field in ("project", "title", "definition_of_done", "verify"):
            if not str(package.get(field) or "").strip():
                found.append(f"{pid}: missing {field}")
        acceptance = package.get("acceptance")
        if not isinstance(acceptance, list) or not acceptance:
            found.append(f"{pid}: acceptance must name at least one scenario")
        wsjf = package.get("wsjf")
        if not isinstance(wsjf, dict):
            found.append(f"{pid}: wsjf is required")
        else:
            if "score" in wsjf:
                found.append(f"{pid}: a computed WSJF score must never be stored")
            for field in WSJF_FIELDS:
                if not isinstance(wsjf.get(field), int) or wsjf.get(field) < 0:
                    found.append(f"{pid}: wsjf.{field} must be a non-negative integer")
            if isinstance(wsjf.get("job_size"), int) and wsjf["job_size"] < 1:
                found.append(f"{pid}: wsjf.job_size must be at least 1")
        ids = package.get("finding_ids")
        if not isinstance(ids, list) or not ids:
            found.append(f"{pid}: finding_ids must name at least one finding")
            ids = []
        for fid in ids:
            if fid not in by_id:
                found.append(f"{pid}: unknown finding {fid}")
            else:
                covered.setdefault(str(fid), []).append(package)
        paths = package.get("affected_paths") or []
        for path in paths:
            if not (root / str(path).split("::")[0]).exists():
                found.append(f"{pid}: affected path does not resolve: {path!r}")
        if package.get("status") == "executed":
            evidence = package.get("evidence") or {}
            for field in ("commit", "test", "demo_credit"):
                if not str(evidence.get(field) or "").strip():
                    found.append(f"{pid}: an executed package needs evidence.{field}")
            test = str(evidence.get("test") or "")
            if test:
                path = test.split("::")[0].split(" ")[0]
                if not (root / path).exists():
                    found.append(f"{pid}: evidence.test does not resolve: {test!r}")
        if package.get("status") in ("waived", "deferred"):
            for fid in ids:
                finding = by_id.get(fid) or {}
                if finding.get("disposition") not in ("accept", "defer"):
                    found.append(f"{pid}: {package['status']} but {fid} has no accept/defer disposition")

    for fid, finding in by_id.items():
        if finding.get("severity") != "P0":
            continue
        if finding.get("status") == "closed":
            continue
        executed = [
            package
            for package in covered.get(fid, [])
            if package.get("status") == "executed"
        ]
        waived = finding.get("disposition") == "accept" and finding.get("waiver")
        if not executed and not waived:
            found.append(f"{fid}: an open P0 has no executed work package and no accepted waiver")
    return found


def load_standard_projects(root: Path) -> set[str]:
    seed = json.loads((root / "seed" / "programme.json").read_text(encoding="utf-8"))
    projects = seed.get("projects") or []
    return {row.get("key") for row in projects if isinstance(row, dict)}


def check(register: dict[str, Any], root: Path) -> list[str]:
    found = problems(register, root)
    known = load_standard_projects(root)
    for finding in register.get("findings") or []:
        if finding.get("project") and finding["project"] not in known:
            found.append(f"{finding.get('id')}: project {finding['project']!r} is not in the seed")
    for package in register.get("work_packages") or []:
        if package.get("project") and package["project"] not in known:
            found.append(f"{package.get('id')}: project {package['project']!r} is not in the seed")
    return found


def self_test() -> None:
    good = {
        "version": 1,
        "findings": [
            {
                "id": "F-T4-1",
                "slice": "T4",
                "project": "T4",
                "spec_clause": "SPEC_V2 §12 T4 gate",
                "claim": "a repeat signature escalates",
                "observed": "the second interruption opens a second card",
                "evidence": ["crates/actions/src/board/ledger.rs:1", "probe: coach.repeat -> two cards"],
                "severity": "P0",
                "disposition": "execute",
                "verifier": "re-ran the probe; two rows",
                "status": "verified",
            }
        ],
        "work_packages": [
            {
                "id": "WP-T4-1",
                "finding_ids": ["F-T4-1"],
                "project": "T4",
                "tranche": 3,
                "kind": "code_rework",
                "title": "escalate a repeat signature instead of opening a second card",
                "definition_of_done": "one card per signature, an escalation row on the repeat",
                "acceptance": ["Given a repeated signature, when the coach runs, then exactly one card exists"],
                "verify": "cargo test -p conveyor-actions --test coach",
                "affected_paths": ["crates/actions/src/board/ledger.rs"],
                "wsjf": {
                    "business_value": 8,
                    "time_criticality": 5,
                    "risk_reduction": 8,
                    "job_size": 3,
                },
                "status": "executed",
                "evidence": {
                    "commit": "abc1234",
                    "test": "crates/actions/src/board/ledger.rs::a_repeat_signature_escalates",
                    "demo_credit": "01a1ff00-0000-7000-8000-000000000000",
                },
            }
        ],
    }
    root = ROOT
    found = problems(good, root)
    if found:
        raise SystemExit(f"self-test: the valid fixture failed: {found}")

    broken = json.loads(json.dumps(good))
    broken["findings"][0]["status"] = "verified"
    broken["work_packages"][0]["status"] = "planned"
    broken["work_packages"][0]["wsjf"]["score"] = 7.0
    broken["findings"][0]["evidence"].append("no/such/path.rs:1")
    problems_found = problems(broken, root)
    for needle in (
        "open P0 has no executed work package",
        "computed WSJF score",
        "does not resolve",
    ):
        if not any(needle in problem for problem in problems_found):
            raise SystemExit(
                f"self-test: the mutant did not trip {needle!r}: {problems_found}"
            )

    no_test = json.loads(json.dumps(good))
    no_test["work_packages"][0]["evidence"]["test"] = ""
    if not any("evidence.test" in problem for problem in problems(no_test, root)):
        raise SystemExit("self-test: an executed package without a test passed")

    waived = json.loads(json.dumps(good))
    waived["work_packages"] = []
    waived["findings"][0].update(
        {
            "status": "adjudicated",
            "disposition": "accept",
            "waiver": {
                "owner": "programme owner",
                "reason": "no live surface for this clause in the current estate",
                "expiry": "2099-01-01",
            },
        }
    )
    if problems(waived, root):
        raise SystemExit(f"self-test: an accepted waiver failed: {problems(waived, root)}")
    waived["findings"][0]["waiver"]["expiry"] = "2000-01-01"
    if not any("unexpired" in problem for problem in problems(waived, root)):
        raise SystemExit("self-test: an expired waiver passed")

    duplicate = json.loads(json.dumps(good))
    duplicate["findings"].append(json.loads(json.dumps(good["findings"][0])))
    if not any("duplicate finding id" in problem for problem in problems(duplicate, root)):
        raise SystemExit("self-test: a duplicate finding id passed")

    print("check-product-rework: self-test ok")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--register", type=Path, default=REGISTER)
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return
    if not args.register.exists():
        raise SystemExit(f"no register at {args.register}")
    register = json.loads(args.register.read_text(encoding="utf-8"))
    found = check(register, ROOT)
    if found:
        for problem in found:
            print(f"FAIL {problem}", file=sys.stderr)
        sys.exit(1)
    findings = register.get("findings") or []
    packages = register.get("work_packages") or []
    executed = [package for package in packages if package.get("status") == "executed"]
    print(
        f"check-product-rework: OK — {len(findings)} finding(s), {len(packages)} package(s), "
        f"{len(executed)} executed"
    )


if __name__ == "__main__":
    main()
