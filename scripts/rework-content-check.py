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
# The architecture track's E–H set (owner tailoring 2026-11-24). A delivery pack carries the
# seven A–D artefacts; a programme/architecture pack carries these eight after them, in the
# order the standard seeds them (migration 0102).
ARCHITECTURE_ARTEFACTS = [
    ("e", "hld", "matrix"),
    ("e", "hld", "catalog"),
    ("e", "hld", "diagram"),
    ("e", "hld", "diagram"),
    ("f", "hld", "catalog"),
    ("g", "hld", "catalog"),
    ("h", "hld", "catalog"),
    ("h", "hld", "catalog"),
]
LEVELS = {"irrelevant", "consistent", "compliant", "conformant", "fully_conformant", "non_conformant"}
# Owner decision 2026-11-24: every artefact document is authored. A body that is a template
# skeleton, a bare source citation or a one-line placeholder is not authored prose, so the
# checker refuses it before the factory can publish it.
SUBSTANCE_MIN = 120
OBJECT_FIELDS = 3
OBJECT_MIN = 80
BOILERPLATE = (
    "the heading is evidenced by",
    "the facts carry the document reference",
    "no further fields",
    "carries no separate",
    "derived from the record or explicitly absent",
    "one standards artefact, derived",
)


def fail(message: str) -> "NoReturn":  # noqa: F821 - a deliberate abort
    print(f"rework-content-check: {message}", file=sys.stderr)
    raise SystemExit(2)


def headings(body: str) -> set[str]:
    return {
        line.lstrip("#").strip()
        for line in body.splitlines()
        if line.startswith("## ")
    }


def sections(body: str) -> dict[str, str]:
    """The markdown body's `## ` sections and the prose under each."""
    out: dict[str, list[str]] = {}
    current: str | None = None
    for line in body.splitlines():
        if line.startswith("## "):
            current = line[3:].strip()
            out[current] = []
        elif current is not None:
            out[current].append(line)
    return {name: "\n".join(lines).strip() for name, lines in out.items()}


def check_authored(body: str, required: list[str], label: str, problems: list[str]) -> None:
    """Refuse a body whose required sections are missing, thin or a source-citation stub."""
    found = sections(body)
    for name in required:
        prose = found.get(name, "")
        if len(prose) < SUBSTANCE_MIN:
            problems.append(
                f"{label} section {name!r} is not authored prose "
                f"(at least {SUBSTANCE_MIN} characters)"
            )
    lowered = body.lower()
    for phrase in BOILERPLATE:
        if phrase in lowered:
            problems.append(f"{label} carries a template stub: {phrase!r}")


def substantive_fields(body: dict, label: str, problems: list[str]) -> None:
    """An object body (SoAW, delivery contract) needs authored fields, not one-line pointers."""
    values = [value for value in body.values() if isinstance(value, str) and value.strip()]
    long_enough = [value for value in values if len(value.strip()) >= OBJECT_MIN]
    if len(values) < OBJECT_FIELDS or len(long_enough) < 2:
        problems.append(
            f"{label} needs at least {OBJECT_FIELDS} authored fields and two of "
            f"at least {OBJECT_MIN} characters"
        )
    for value in values:
        lowered = value.lower()
        for phrase in BOILERPLATE:
            if phrase in lowered:
                problems.append(f"{label} carries a template stub: {phrase!r}")


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
          wsjf: tuple[int, int, int, int], closes: bool,
          architecture: bool = False) -> list[str]:
    problems: list[str] = []

    def expect(condition: bool, message: str) -> None:
        if not condition:
            problems.append(message)

    expect(isinstance(content.get("charter"), str) and len(content["charter"]) > 200,
           "charter must be a markdown string of substance")
    if isinstance(content.get("charter"), str):
        for phrase in BOILERPLATE:
            if phrase in content["charter"].lower():
                problems.append(f"charter carries a template stub: {phrase!r}")
    soaw = content.get("soaw") or {}
    expect(isinstance(soaw.get("title"), str) and isinstance(soaw.get("body"), dict),
           "soaw must be {title, body: object}")
    if isinstance(soaw.get("body"), dict):
        substantive_fields(soaw["body"], "soaw body", problems)

    artefacts = content.get("artifacts") or []
    expected = REQUIRED_ARTEFACTS + (ARCHITECTURE_ARTEFACTS if architecture else [])
    expect(len(artefacts) == len(expected),
           f"artifacts must be exactly {len(expected)}, got {len(artefacts)}")
    pairs = [(a.get("phase_code"), a.get("plateau"), a.get("artifact_kind")) for a in artefacts]
    expect(pairs == expected, f"artefact phase/plateau/kind set is {pairs}")
    expect(all(isinstance(a.get("title"), str) and a["title"].strip() for a in artefacts),
           "every artefact needs a title")
    expect(all(isinstance(a.get("entries"), list) and a["entries"] for a in artefacts),
           "every artefact needs at least one entry")
    art_required = ((template_standard.get("templates") or {}).get("architecture-artifact")
                    or {}).get("required") or ["Metadata", "Entries", "Links", "Document"]
    for artefact in artefacts:
        label = f"artefact {artefact.get('title')!r}"
        body = artefact.get("body")
        expect(isinstance(body, str) and bool(body.strip()),
               f"{label} needs an authored body")
        if isinstance(body, str) and body.strip():
            check_authored(body, art_required, label, problems)

    contract = content.get("delivery_contract") or {}
    expect(isinstance(contract.get("title"), str) and isinstance(contract.get("body"), dict),
           "delivery_contract must be {title, body: object}")
    if isinstance(contract.get("body"), dict):
        substantive_fields(contract["body"], "delivery_contract body", problems)

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
        category = key.replace("_output", "").replace("_", "-")
        required = (templates.get(category) or {}).get("required") or []
        body = content.get(key)
        expect(isinstance(body, str) and bool(body.strip()), f"{key} must be authored")
        if isinstance(body, str) and body.strip():
            missing = [name for name in required if name not in headings(body)]
            expect(not missing, f"{key} is missing required sections: {', '.join(missing)}")
            check_authored(body, required, key, problems)

    if closes:
        closure = content.get("closure")
        closure_review = content.get("closure_review")
        expect(isinstance(closure, str) and len(closure) > 100,
               "closure must be a markdown string of substance")
        expect(isinstance(closure_review, str) and len(closure_review) > 100,
               "closure_review must be a markdown string of substance")
        if isinstance(closure, str):
            for phrase in BOILERPLATE:
                if phrase in closure.lower():
                    problems.append(f"closure carries a template stub: {phrase!r}")
        required = (templates.get("closure-review") or {}).get("required") or []
        body = closure_review if isinstance(closure_review, str) else ""
        missing = [name for name in required if name not in headings(body)]
        expect(not missing, f"closure_review is missing required sections: {', '.join(missing)}")
        if body.strip():
            check_authored(body, required, "closure_review", problems)
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


def project_architecture(snapshot: Path, project: str) -> bool:
    """The project's track from the snapshot: the architecture track carries E–H, delivery A–D.

    Same rule as the generator and the audit: `meta.adm_profile` wins when it names an
    architecture profile; otherwise `meta.track == "programme"`.
    """
    data = json.loads((snapshot / "programme.json").read_text(encoding="utf-8"))
    for row in data.get("projects") or []:
        if row.get("key") != project:
            continue
        meta = row.get("meta") or {}
        named = str(meta.get("adm_profile") or "").strip().lower()
        if named:
            return "architecture" in named or named in {"programme", "a-h", "ah"}
        return str(meta.get("track") or "").strip().lower() == "programme"
    fail(f"project {project!r} is not in the snapshot")


def self_test() -> None:
    standard = json.loads((ROOT / "seed" / "output-standard.json").read_text(encoding="utf-8"))
    templates = standard["templates"]

    def body_for(category: str) -> str:
        required = (templates.get(category) or {}).get("required") or ["Body"]
        return "\n\n".join(
            f"## {name}\n\n" + (f"Authored prose for {name}. " * 6) for name in required
        )

    good = {
        "charter": "# Charter\n\n" + "A grounded charter statement. " * 12,
        "soaw": {"title": "SoAW", "body": {
            "assignment": "Authored assignment statement. " * 4,
            "scope": "Authored scope statement. " * 4,
            "acceptance_criteria": "Authored acceptance criteria. " * 4,
        }},
        "artifacts": [
            {"title": f"Artefact {index}", "phase_code": phase, "plateau": plateau,
             "artifact_kind": kind, "entries": [{"axis_a": "a", "axis_b": "b", "value": "v"}],
             "body": body_for("architecture-artifact")}
            for index, (phase, plateau, kind) in enumerate(REQUIRED_ARTEFACTS)
        ],
        "delivery_contract": {"title": "Contract", "body": {
            "compliance": "Authored compliance method. " * 4,
            "acceptance": "Authored acceptance statement. " * 4,
            "approval": "Authored approval statement. " * 4,
        }},
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

    # The authored-prose gate: a thin artefact body and a source-citation stub are refused.
    thin = json.loads(json.dumps(good))
    thin["artifacts"][0]["body"] = "## Metadata\n\ntext"
    problems = check(thin, standard, ids, wsjf, True)
    if not any("authored prose" in problem for problem in problems):
        raise SystemExit("self-test: a thin artefact body was not refused")
    stub = json.loads(json.dumps(good))
    stub["project_brief_output"] = body_for("project-brief").replace(
        "Authored prose for Outcome.", "The heading is evidenced by a source.")
    problems = check(stub, standard, ids, wsjf, True)
    if not any("template stub" in problem for problem in problems):
        raise SystemExit("self-test: a source-citation stub was not refused")
    thin_soaw = json.loads(json.dumps(good))
    thin_soaw["soaw"]["body"] = {"assignment": "x"}
    problems = check(thin_soaw, standard, ids, wsjf, True)
    if not any("soaw body" in problem for problem in problems):
        raise SystemExit("self-test: a thin SoAW body was not refused")

    active = json.loads(json.dumps(good))
    del active["closure"], active["closure_review"]
    if check(active, standard, ids, wsjf, False):
        raise SystemExit("self-test: an active project without closure keys failed")

    # Closes said true but the content was authored for an active project: this must be a
    # reported refusal, not a KeyError from the closure-review heading check.
    closing_without_keys = json.loads(json.dumps(active))
    problems = check(closing_without_keys, standard, ids, wsjf, True)
    for needle in ("closure must be", "closure_review must be"):
        if not any(needle in problem for problem in problems):
            raise SystemExit(
                f"self-test: a closing content without {needle!r} did not refuse cleanly: {problems}"
            )
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
                     args.closes == "true",
                     architecture=project_architecture(args.snapshot, args.project))
    if problems:
        for problem in problems:
            print(f"FAIL {problem}", file=sys.stderr)
        raise SystemExit(1)
    print(f"rework-content-check: {args.project} content is consistent with the facts")


if __name__ == "__main__":
    main()
