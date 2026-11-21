#!/usr/bin/env python3
"""Refuse a rework content file before it can half-run a project.

`scripts/rework-project.py` performs live governance writes in a fixed order. A content file that
is structurally wrong fails part-way through — the runner resumes, but the project's transcript
then mixes a refused act with the real ones. This check runs first, offline, and refuses:

  * a missing or mistyped content key;
  * an artefact set that is not the adopted profile's seven A–D artefacts;
  * an output body missing a heading its category requires (`seed/output-standard.json`);
  * a WSJF input triple that is not the one the batch's PI plan recorded;
  * an id that appears nowhere in the snapshot the facts were read from (a fabricated reference).

Usage:
    python3 scripts/rework-content-check.py --project S02 \
        --content /tmp/rework-content-S02.json --snapshot /tmp/snap-baseline/data \
        --wsjf 8,8,8,13 --closes false
    python3 scripts/rework-content-check.py --self-test
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DELEGATION = "01a1ff1f-e4ba-72dc-9c1a-13049933cf14"
UUID = re.compile(r"\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b")
REQUIRED_ARTEFACTS = [
    ("a", "psa", "matrix"),
    ("a", "psa", "catalog"),
    ("a", "psa", "catalog"),
    ("a", "psa", "diagram"),
    ("b", "hld", "diagram"),
    ("c", "hld", "catalog"),
    ("d", "hld", "catalog"),
]
LEVELS = {"irrelevant", "consistent", "compliant", "conformant", "fully_conformant", "non_conformant"}


def fail(message: str) -> "NoReturn":  # noqa: F821 - a deliberate abort
    print(f"rework-content-check: {message}", file=sys.stderr)
    raise SystemExit(2)


def headings(body: str) -> set[str]:
    return {
        line.lstrip("#").strip()
        for line in body.splitlines()
        if line.startswith("## ")
    }


def all_ids(value: Any, found: set[str]) -> None:
    if isinstance(value, str):
        found.update(UUID.findall(value))
    elif isinstance(value, dict):
        for item in value.values():
            all_ids(item, found)
    elif isinstance(value, list):
        for item in value:
            all_ids(item, found)


def check(content: dict, template_standard: dict, snapshot_ids: set[str],
          wsjf: tuple[int, int, int, int], closes: bool) -> list[str]:
    problems: list[str] = []

    def expect(condition: bool, message: str) -> None:
        if not condition:
            problems.append(message)

    expect(isinstance(content.get("charter"), str) and len(content["charter"]) > 200,
           "charter must be a markdown string of substance")
    soaw = content.get("soaw") or {}
    expect(isinstance(soaw.get("title"), str) and isinstance(soaw.get("body"), dict),
           "soaw must be {title, body: object}")

    artefacts = content.get("artifacts") or []
    expect(len(artefacts) == 7, f"artifacts must be exactly 7, got {len(artefacts)}")
    pairs = [(a.get("phase_code"), a.get("plateau"), a.get("artifact_kind")) for a in artefacts]
    expect(pairs == REQUIRED_ARTEFACTS, f"artefact phase/plateau/kind set is {pairs}")
    expect(all(isinstance(a.get("title"), str) and a["title"].strip() for a in artefacts),
           "every artefact needs a title")
    expect(all(isinstance(a.get("entries"), list) and a["entries"] for a in artefacts),
           "every artefact needs at least one entry")

    contract = content.get("delivery_contract") or {}
    expect(isinstance(contract.get("title"), str) and isinstance(contract.get("body"), dict),
           "delivery_contract must be {title, body: object}")

    wbs = content.get("wbs_deliverable") or {}
    got = tuple(wbs.get(key) for key in
                ("business_value", "time_criticality", "risk_reduction", "job_size"))
    expect(got == wsjf, f"WSJF inputs are {got}, the PI plan records {wsjf}")
    expect(isinstance(wbs.get("title"), str) and wbs["title"].strip(), "wbs title is required")

    review = content.get("review") or {}
    expect(review.get("level") in LEVELS, f"review level {review.get('level')!r} is not one of the six")
    findings = review.get("findings")
    expect(isinstance(findings, (str, list, dict)) and bool(findings), "review findings are empty")
    expect(isinstance(review.get("evidence"), dict), "review evidence must be an object")

    templates = template_standard.get("templates") or {}
    for key in ("project_brief_output", "evidence_output"):
        if key not in content:
            continue
        category = key.replace("_output", "").replace("_", "-")
        required = (templates.get(category) or {}).get("required") or []
        missing = [name for name in required if name not in headings(content[key] or "")]
        expect(not missing, f"{key} is missing required sections: {', '.join(missing)}")

    if closes:
        expect(isinstance(content.get("closure"), str) and len(content["closure"]) > 100,
               "closure must be a markdown string of substance")
        expect(isinstance(content.get("closure_review"), str) and len(content["closure_review"]) > 100,
               "closure_review must be a markdown string of substance")
        required = (templates.get("closure-review") or {}).get("required") or []
        missing = [name for name in required if name not in headings(content["closure_review"])]
        expect(not missing, f"closure_review is missing required sections: {', '.join(missing)}")
    else:
        expect("closure" not in content and "closure_review" not in content,
               "a project that stays active must carry no closure keys")

    ids: set[str] = set()
    all_ids(content, ids)
    fabricated = sorted(ids - snapshot_ids - {DELEGATION})
    expect(not fabricated, f"ids that appear nowhere in the snapshot: {', '.join(fabricated)}")
    return problems


def load_snapshot_ids(snapshot: Path) -> set[str]:
    ids: set[str] = set()

    def walk(value: Any) -> None:
        if isinstance(value, dict):
            for key, item in value.items():
                if key == "id" and isinstance(item, str):
                    ids.add(item)
                walk(item)
        elif isinstance(value, list):
            for item in value:
                walk(item)

    for path in sorted(snapshot.glob("*.json")):
        walk(json.loads(path.read_text(encoding="utf-8")))
    outputs = snapshot.parent / "docs" / "outputs.js"
    if outputs.exists():
        text = outputs.read_text(encoding="utf-8")
        walk(json.loads(text[text.find("{") : text.rfind("}") + 1]))
    return ids


def self_test() -> None:
    standard = json.loads((ROOT / "seed" / "output-standard.json").read_text(encoding="utf-8"))
    templates = standard["templates"]

    def body_for(category: str) -> str:
        required = (templates.get(category) or {}).get("required") or ["Body"]
        return "\n\n".join(f"## {name}\n\ntext" for name in required)

    good = {
        "charter": "# Charter\n\n" + "A grounded charter statement. " * 12,
        "soaw": {"title": "SoAW", "body": {"assignment": "x", "retrospective": True}},
        "artifacts": [
            {"title": f"Artefact {index}", "phase_code": phase, "plateau": plateau,
             "artifact_kind": kind, "entries": [{"axis_a": "a", "axis_b": "b", "value": "v"}]}
            for index, (phase, plateau, kind) in enumerate(REQUIRED_ARTEFACTS)
        ],
        "delivery_contract": {"title": "Contract", "body": {"compliance": "manifest"}},
        "wbs_deliverable": {"title": "Deliverable", "business_value": 5,
                            "time_criticality": 5, "risk_reduction": 3, "job_size": 12},
        "review": {"level": "compliant", "findings": [{"code": "X-1", "note": "n"}],
                   "evidence": {"manifest": "acceptance/programme/X.toml"}},
        "project_brief_output": body_for("project-brief"),
        "evidence_output": body_for("evidence"),
        "closure_review": body_for("closure-review") + "\n\n" + "review text " * 20,
        "closure": "# Closure\n\n" + "closure text " * 20,
    }
    ids: set[str] = set()
    all_ids(good, ids)
    wsjf = (5, 5, 3, 12)
    problems = check(good, standard, ids, wsjf, True)
    if problems:
        raise SystemExit(f"self-test: the valid fixture failed: {problems}")

    broken = json.loads(json.dumps(good))
    broken["artifacts"] = broken["artifacts"][:6]
    broken["wbs_deliverable"]["job_size"] = 1
    broken["project_brief_output"] = "# no required headings"
    broken["review"]["level"] = "probably-fine"
    broken["contract"] = "00000000-0000-0000-0000-000000000000"
    problems = check(broken, standard, ids, wsjf, True)
    for needle in ("exactly 7", "WSJF", "missing required sections", "not one of the six",
                   "nowhere in the snapshot"):
        if not any(needle in problem for problem in problems):
            raise SystemExit(f"self-test: the mutant did not trip {needle!r}: {problems}")

    active = json.loads(json.dumps(good))
    del active["closure"], active["closure_review"]
    if check(active, standard, ids, wsjf, False):
        raise SystemExit("self-test: an active project without closure keys failed")
    print("rework-content-check: self-test ok")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project")
    parser.add_argument("--content", type=Path)
    parser.add_argument("--snapshot", type=Path)
    parser.add_argument("--wsjf", default="", help="BV,TC,RR,Size the PI plan records")
    parser.add_argument("--closes", choices=("true", "false"), default="true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return
    if not (args.project and args.content and args.snapshot and args.wsjf):
        parser.error("--project, --content, --snapshot and --wsjf are required")
    try:
        wsjf = tuple(int(part) for part in args.wsjf.split(","))
    except ValueError:
        parser.error("--wsjf must be four comma-separated integers")
    if len(wsjf) != 4:
        parser.error("--wsjf must be four comma-separated integers")
    standard = json.loads((ROOT / "seed" / "output-standard.json").read_text(encoding="utf-8"))
    content = json.loads(args.content.read_text(encoding="utf-8"))
    problems = check(content, standard, load_snapshot_ids(args.snapshot), wsjf,
                     args.closes == "true")
    if problems:
        for problem in problems:
            print(f"FAIL {problem}", file=sys.stderr)
        raise SystemExit(1)
    print(f"rework-content-check: {args.project} content is consistent with the facts")


if __name__ == "__main__":
    main()
