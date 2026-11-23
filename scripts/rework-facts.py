#!/usr/bin/env python3
"""Emit the facts a retrospective rework content file may cite, and nothing else.

The rework runner (`scripts/rework-project.py`) takes a content JSON authored for one project.
Authoring it from memory is how a governance record acquires a date, an id or a claim that the
record does not support.  This script reads a published snapshot (the product's own projection of
the live rows) and emits the project's facts: its row and meta, its tranche and the tranche review,
the mandatory A–D artefacts and which are missing, its contracts, reviews, baselines, published
outputs, its plan, and the state of the issues it names.

Read-only: it opens no connection and writes no row.  It never invents a value — a field the
snapshot does not carry is reported as `null` or as a missing entry.

Usage:
    python3 scripts/rework-facts.py --project S02 --snapshot ~/conveyor-evidence/<snap>/data
    python3 scripts/rework-facts.py --project S02 --snapshot docs/design/data --out /tmp/facts-S02.json
    python3 scripts/rework-facts.py --self-test
"""

from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SNAPSHOT = ROOT / "docs" / "design" / "data"
# The A–D set a delivery project must carry under the adopted profile (SPEC_V2 §8.1).
DELIVERY_PHASES = ("a", "b", "c", "d")
DELIVERY_CONTRACTS = ("statement_of_architecture_work", "delivery_contract")


def fail(message: str) -> "NoReturn":  # noqa: F821 - a deliberate abort
    print(f"rework-facts: {message}", file=sys.stderr)
    raise SystemExit(2)


def read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        fail(f"{path} is missing (is this a published snapshot?)")
    except json.JSONDecodeError as error:
        fail(f"{path} is not JSON: {error}")


def read_outputs(data_dir: Path) -> list[dict]:
    candidates = [data_dir.parent / "docs" / "outputs.js", data_dir / "outputs.js"]
    for path in candidates:
        if path.exists():
            text = path.read_text(encoding="utf-8")
            start, end = text.find("{"), text.rfind("}")
            if start < 0 or end < start:
                fail(f"{path} does not carry a JSON document")
            return json.loads(text[start : end + 1]).get("outputs", [])
    return []


def plan_facts(meta: dict) -> dict:
    path = meta.get("plan")
    if not path:
        return {"path": None, "exists": False, "title": None, "status": None, "headings": []}
    file = ROOT / path
    if not file.exists():
        return {"path": path, "exists": False, "title": None, "status": meta.get("plan_status"),
                "headings": []}
    headings: list[str] = []
    title = None
    for line in file.read_text(encoding="utf-8", errors="replace").splitlines()[:80]:
        if line.startswith("# ") and title is None:
            title = line[2:].strip()
        elif line.startswith(("## ", "### ")) and len(headings) < 12:
            headings.append(line.lstrip("# ").strip())
    return {
        "path": path,
        "exists": True,
        "title": title,
        "status": meta.get("plan_status"),
        "headings": headings,
    }


def facts(project_key: str, data_dir: Path) -> dict:
    programme = read_json(data_dir / "programme.json")
    architecture = read_json(data_dir / "architecture.json")
    msp = read_json(data_dir / "msp.json")

    project = next((p for p in programme.get("projects", []) if p.get("key") == project_key), None)
    if project is None:
        fail(f"{project_key} is not a project in {data_dir / 'programme.json'}")
    meta = project.get("meta") or {}
    project_id = project["id"]

    type_by_id = {row["id"]: row for row in architecture.get("standard", [])}
    profile = next((p for p in architecture.get("profiles", []) if p.get("status") == "adopted"), {})
    requirements = profile.get("artifact_requirements") or {}
    required = []
    for phase in DELIVERY_PHASES:
        for plateau, keys in (requirements.get(phase) or {}).items():
            required.extend({"phase": phase, "plateau": plateau, "key": key} for key in keys)

    artefacts = []
    for row in architecture.get("artifacts", []):
        if row.get("project_id") != project_id:
            continue
        kind = type_by_id.get(row.get("artifact_type_id"), {})
        artefacts.append({
            "id": row["id"],
            "title": row.get("title"),
            "key": kind.get("key"),
            "artifact_kind": kind.get("artifact_kind") or row.get("artifact_kind"),
            "phase_code": row.get("phase_code"),
            "plateau": row.get("plateau"),
            "status": row.get("status"),
            "document_id": row.get("document_id"),
        })
    present = {(a["key"], a["phase_code"]) for a in artefacts if a["status"] == "approved"}
    missing_artefacts = [
        item for item in required if (item["key"], item["phase"]) not in present
    ]

    contracts = [
        {
            "id": row["id"],
            "kind": row.get("kind"),
            "title": row.get("title"),
            "status": row.get("status"),
            "phase_code": row.get("phase_code"),
            "requested_by": row.get("requested_by"),
            "approved_by": row.get("approved_by"),
            "approval_id": row.get("approval_id"),
        }
        for row in architecture.get("contracts", [])
        if row.get("project_id") == project_id
    ]
    live_kinds = {c["kind"] for c in contracts if c["status"] == "live"}
    missing_contracts = [kind for kind in DELIVERY_CONTRACTS if kind not in live_kinds]

    reviews = [
        {
            "id": row["id"],
            "contract_id": row.get("contract_id"),
            "level": row.get("level"),
            "outcome": row.get("outcome"),
            "reviewed_at": row.get("reviewed_at"),
            "reviewed_by_name": row.get("reviewed_by_name"),
        }
        for row in architecture.get("compliance_reviews", [])
        if row.get("project_id") == project_id
    ]

    tranche_number = meta.get("tranche")
    tranches = msp.get("tranches", [])
    tranche = next((t for t in tranches if t.get("number") == tranche_number), None)
    next_tranche = next((t for t in tranches if t.get("number") == (tranche_number or 0) + 1), None)
    tranche_reviews = [
        {
            "id": row["id"],
            "tranche_id": row.get("tranche_id"),
            "decision": row.get("decision"),
            "reviewed_at": row.get("reviewed_at"),
            "reviewer_name": row.get("reviewer_name"),
        }
        for row in msp.get("tranche_reviews", [])
        if tranche and row.get("tranche_id") == tranche.get("id")
    ]

    baselines = [
        {"id": row["id"], "kind": row.get("kind"), "approved_by": row.get("approved_by"),
         "taken_at": row.get("taken_at")}
        for row in msp.get("project_baselines", [])
        if row.get("project_id") == project_id
    ]

    outputs = [
        {"id": row["id"], "category": row.get("category"), "title": row.get("title"),
         "published_at": row.get("published_at")}
        for row in read_outputs(data_dir)
        if row.get("project_id") == project_id
    ]
    published = {row["category"] for row in outputs}
    expected = list(meta.get("expected_outputs") or []) + list(meta.get("closure_outputs") or [])
    missing_outputs = [category for category in dict.fromkeys(expected) if category not in published]

    registry = {row["number"]: row for row in programme.get("issue_registry", {}).get("issues", [])}
    issues = []
    for number in meta.get("issues") or []:
        row = registry.get(number)
        issues.append({
            "number": number,
            "title": row.get("title") if row else None,
            "state": row.get("state") if row else None,
        })

    return {
        "schema": "rework-facts/v1",
        "generated_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "snapshot": str(data_dir),
        "project": {
            "key": project["key"],
            "id": project_id,
            "name": project.get("name"),
            "description": project.get("description"),
            "status": project.get("status"),
            "project_type_code": project.get("project_type_code"),
            "repo_url": project.get("repo_url"),
            "meta": meta,
        },
        "plan": plan_facts(meta),
        "tranche": {
            "number": tranche_number,
            "row": tranche,
            "reviews": tranche_reviews,
            "next": next_tranche,
        },
        "artefacts": {
            "required": required,
            "present": artefacts,
            "missing": missing_artefacts,
        },
        "contracts": {"present": contracts, "missing": missing_contracts},
        "reviews": reviews,
        "baselines": baselines,
        "outputs": {"published": outputs, "missing": missing_outputs},
        "issues": issues,
    }


def self_test() -> None:
    """The committed snapshot resolves a real project and refuses an unknown one."""
    sample = facts("S00", DEFAULT_SNAPSHOT)
    if sample["project"]["key"] != "S00" or not sample["project"]["id"]:
        raise SystemExit("self-test: S00 did not resolve")
    if len(sample["artefacts"]["required"]) != 7:
        raise SystemExit(
            f"self-test: the adopted profile names {len(sample['artefacts']['required'])} "
            "delivery artefacts, expected 7"
        )
    if sample["plan"]["path"] and not sample["plan"]["exists"]:
        raise SystemExit(f"self-test: the plan {sample['plan']['path']} does not exist")
    if not sample["outputs"]["published"]:
        raise SystemExit("self-test: S00 carries no published output, the snapshot is not the live shape")
    try:
        facts("NO-SUCH-PROJECT", DEFAULT_SNAPSHOT)
    except SystemExit as error:
        if error.code != 2:
            raise
    else:
        raise SystemExit("self-test: an unknown project was not refused")
    print("rework-facts: self-test ok")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project")
    parser.add_argument("--snapshot", type=Path, default=DEFAULT_SNAPSHOT)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return
    if not args.project:
        parser.error("--project is required")
    document = facts(args.project, args.snapshot)
    text = json.dumps(document, indent=1, sort_keys=True) + "\n"
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(text, encoding="utf-8")
        print(f"rework-facts: wrote {args.out}")
    else:
        sys.stdout.write(text)


if __name__ == "__main__":
    main()
