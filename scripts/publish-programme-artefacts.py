#!/usr/bin/env python3
"""Compose and publish the programme-level standards artefacts from the live record.

The standard declares, for every category, the headings an artefact must carry and the record
source of each heading (generate-standards.py resolves them).  This tool turns those resolved
sources into a facts pack, validates an authored body against the required headings, and — via
`programme-record.py --local-bin` — composes and publishes the document through the product's
own actions, journaling every write for the post-merge replay into production.

Modes:
  --facts-dir DIR    write one <category>.facts.json per selected category (read-only)
  --dry-run          validate the bodies in --bodies and write a manifest (no writes)
  --apply            compose + publish every valid body through the actions

Every published write is append-only in docs/evidence/record-w3/journal.jsonl; the manifest maps
category -> {document_id, body_sha256, version}. A re-run with an unchanged body is a no-op.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GEN_PATH = ROOT / "scripts/generate-standards.py"
RECORD = ROOT / "scripts/programme-record.py"
STANDARD_PATH = ROOT / "seed/output-standard.json"
EVIDENCE_ROOT = ROOT / "docs/evidence/record-w3"
JOURNAL = EVIDENCE_ROOT / "journal.jsonl"
MANIFEST = EVIDENCE_ROOT / "published.json"
DEFAULT_ACTOR = "01a1efd4-c3e5-7b74-b32b-1b15683a8c4e"

# The programme-level set this slice owns (plan v6 W3). inspect-adapt and safe-art are absent on
# purpose: their mandatory derived record (kata candidates, shared_config) does not exist yet, so
# publishing them would be an explicit absence, not evidence.
DEFAULT_CATEGORIES = [
    "mandate",
    "programme-brief",
    "programme-strategies",
    "refined-vision",
    "programme-baselines",
    "programme-risk-register",
    "benefits-registry",
    "blueprint",
    "business-case",
    "requirements-catalog",
    "adr-register",
    "safe-cadence",
    "safe-portfolio",
]

TITLES = {
    "mandate": "Programme Mandate — conveyor buildout",
    "programme-brief": "Programme Brief — conveyor buildout",
    "programme-strategies": "Programme Strategies — conveyor buildout",
    "refined-vision": "Refined Vision — conveyor buildout",
    "programme-baselines": "Programme Baselines — management, governance and boundary",
    "programme-risk-register": "Programme Risk Register — conveyor buildout",
    "benefits-registry": "Benefits Registry — conveyor buildout",
    "blueprint": "Programme Blueprint — conveyor buildout",
    "business-case": "Programme Business Case — conveyor buildout",
    "requirements-catalog": "Requirements Catalog — conveyor buildout",
    "adr-register": "Architecture Decision Register — conveyor buildout",
    "safe-cadence": "SAFe Cadence — PI planning, demo, retro and refinement",
    "safe-portfolio": "SAFe Portfolio — conveyor buildout",
    "inspect-adapt": "Inspect and Adapt — conveyor buildout",
    "safe-art": "SAFe ART — conveyor buildout",
}


def fail(message: str) -> "NoReturn":
    sys.stderr.write(f"publish-programme-artefacts: {message}\n")
    raise SystemExit(2)


def load_generator():
    spec = importlib.util.spec_from_file_location("generate_standards", GEN_PATH)
    if spec is None or spec.loader is None:
        fail(f"cannot load {GEN_PATH}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_snapshot(directory: Path) -> dict:
    files = {
        path.stem: json.loads(path.read_text(encoding="utf-8"))
        for path in sorted(directory.glob("*.json"))
    }
    if "manifest" not in files:
        fail(f"{directory} is not a product snapshot: manifest.json is missing")
    return files


def programme_projects(snapshot: dict) -> dict[str, set[str]]:
    out: dict[str, set[str]] = {}
    for project in (snapshot.get("programme") or {}).get("projects") or []:
        out.setdefault(str(project.get("programme_id")), set()).add(str(project.get("id")))
    return out


def definition_project(snapshot: dict) -> str:
    projects = (snapshot.get("programme") or {}).get("projects") or []
    p0 = next((p for p in projects if str(p.get("key")) == "P0"), None)
    if p0 is None:
        fail("the snapshot carries no P0 programme-definition project")
    return str(p0["id"])


def build_instances(gen, std: dict, snapshot: dict, outputs: Path) -> dict:
    instances, _ = gen.build_instances(std, snapshot, outputs)
    return {(i["category"], str(i["subject"]["kind"])): i for i in instances}


def facts_for(gen, instance: dict, template: dict, snapshot: dict, outputs: Path,
              programme_projects_map: dict[str, set[str]]) -> dict:
    """The full record behind every heading — not the pack's 12-row excerpt."""
    headings = []
    wanted = list(template.get("required") or [])
    wanted += [h for h in (template.get("optional") or [])
               if h in (template.get("sources") or {})]
    for heading in wanted:
        section = gen.resolve_section(
            heading, template, instance["category"], instance["subject"], snapshot,
            outputs, programme_projects_map, register=None, item_limit=None,
        )
        items: list[dict] = []
        sources: list[dict] = []
        for source in section["sources"]:
            if source.get("count", 0) <= 0:
                continue
            sources.append({
                "kind": source.get("kind"),
                "file": source.get("file"),
                "pointer": source.get("pointer"),
                "category": source.get("category"),
                "count": source.get("count"),
            })
            items.extend(source.get("rows") or source.get("items") or [])
        headings.append({
            "heading": section["heading"],
            "status": section["status"],
            "sources": sources,
            "items": items,
        })
    return {
        "category": instance["category"],
        "standard": instance["standard"],
        "spec": instance["spec"],
        "subject": instance["subject"],
        "template_skeleton": template.get("skeleton") or "",
        "headings": headings,
    }


def required_headings(template: dict) -> list[str]:
    return list(template.get("required") or [])


def normalise(value: str) -> str:
    return " ".join(value.split()).strip().lower()


def validate_body(body: str, template: dict, category: str) -> list[str]:
    problems: list[str] = []
    headings = {
        normalise(line.lstrip("#").strip())
        for line in body.splitlines()
        if line.startswith("## ") or line.startswith("### ")
    }
    for heading in required_headings(template):
        if normalise(heading) not in headings:
            problems.append(f"missing heading: {heading}")
    if re.search(r"\b(TODO|TBD|PLACEHOLDER|FIXME|LOREM IPSUM)\b", body):
        problems.append("body contains a placeholder marker")
    if "$subject" in body or "$programme_id" in body:
        problems.append("body contains an unsubstituted template token")
    if len(body.encode("utf-8")) > 256 * 1024:
        problems.append("body exceeds the 256 KiB compose cap")
    if not body.strip().startswith("# "):
        problems.append("body must start with a level-one title")
    return problems


def body_sha(body: str) -> str:
    return hashlib.sha256(body.encode("utf-8")).hexdigest()


def read_manifest() -> dict:
    if MANIFEST.exists():
        return json.loads(MANIFEST.read_text(encoding="utf-8"))
    return {"schema": "programme-artefacts/v1", "documents": {}}


def write_manifest(manifest: dict) -> None:
    MANIFEST.parent.mkdir(parents=True, exist_ok=True)
    MANIFEST.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")


def append_journal(line: dict) -> None:
    JOURNAL.parent.mkdir(parents=True, exist_ok=True)
    if not JOURNAL.exists():
        JOURNAL.write_text(json.dumps({
            "schema": "conveyor-record-journal/v1",
            "staging_copy": "127.0.0.1:55443",
            "lane_head": os.environ.get("LANE_HEAD", ""),
        }) + "\n", encoding="utf-8")
    with JOURNAL.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(line, sort_keys=True) + "\n")


def record(action: str, payload: dict, *, read_action: str, read_input: dict | None,
           if_match: int | None, read_id_from_output: str | None, evidence: Path,
           actor: str, local_bin: str, quiet: bool = True) -> dict:
    command = [
        sys.executable, str(RECORD),
        "--action", action,
        "--input", json.dumps(payload),
        "--read-action", read_action,
        "--evidence", str(evidence),
        "--actor-id", actor,
        "--local-bin", local_bin,
    ]
    if read_input is not None:
        command += ["--read-input", json.dumps(read_input)]
    if if_match is not None:
        command += ["--if-match", str(if_match)]
    if read_id_from_output:
        command += ["--read-id-from-output", read_id_from_output]
    if quiet:
        command += ["--quiet"]
    completed = subprocess.run(command, capture_output=True, text=True)
    if completed.returncode != 0:
        sys.stderr.write(completed.stderr)
        sys.stderr.write(completed.stdout)
        fail(f"{action}: programme-record exited {completed.returncode}")
    try:
        return json.loads(completed.stdout)
    except ValueError:
        fail(f"{action}: transport output is not JSON: {completed.stdout[:400]!r}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--snapshot", type=Path, required=True)
    parser.add_argument("--outputs-dir", type=Path, required=True)
    parser.add_argument("--standard", type=Path, default=STANDARD_PATH)
    parser.add_argument("--category", action="append", default=[])
    parser.add_argument("--all-programme", action="store_true")
    parser.add_argument("--facts-dir", type=Path)
    parser.add_argument("--bodies", type=Path)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--local-bin", default=os.environ.get("PROGRAMME_RECORD_BIN", ""))
    parser.add_argument("--actor-id", default=os.environ.get("PROGRAMME_ACTOR", DEFAULT_ACTOR))
    args = parser.parse_args()

    gen = load_generator()
    std = json.loads(args.standard.read_text(encoding="utf-8"))
    snapshot = load_snapshot(args.snapshot)
    instances = build_instances(gen, std, snapshot, args.outputs_dir)
    p0 = definition_project(snapshot)

    categories = args.category or (
        sorted(std.get("categories") or []) if args.all_programme else DEFAULT_CATEGORIES
    )
    selected = []
    for category in categories:
        template = std["templates"].get(category)
        if template is None:
            fail(f"{category}: not in the standard")
        instance = instances.get((category, "programme"))
        if instance is None:
            fail(f"{category}: no programme instance (subject kind is not programme)")
        selected.append((category, template, instance))

    if args.facts_dir:
        args.facts_dir.mkdir(parents=True, exist_ok=True)
        projects_map = programme_projects(snapshot)
        for category, template, instance in selected:
            facts = facts_for(gen, instance, template, snapshot, args.outputs_dir, projects_map)
            facts["definition_project_id"] = p0
            (args.facts_dir / f"{category}.facts.json").write_text(
                json.dumps(facts, indent=2) + "\n", encoding="utf-8"
            )
        print(f"facts: {len(selected)} categories -> {args.facts_dir}")
        if not (args.dry_run or args.apply):
            return 0

    if not (args.dry_run or args.apply):
        parser.error("one of --facts-dir, --dry-run or --apply is required")
    if args.bodies is None:
        parser.error("--dry-run/--apply require --bodies DIR")

    manifest = read_manifest()
    plan: list[dict] = []
    failed = False
    for category, template, instance in selected:
        body_path = args.bodies / f"{category}.md"
        if not body_path.exists():
            plan.append({"category": category, "status": "missing-body"})
            failed = True
            continue
        body = body_path.read_text(encoding="utf-8")
        problems = validate_body(body, template, category)
        if problems:
            plan.append({"category": category, "status": "invalid", "problems": problems})
            failed = True
            continue
        digest = body_sha(body)
        previous = (manifest.get("documents") or {}).get(category) or {}
        if previous.get("body_sha256") == digest:
            plan.append({
                "category": category, "status": "unchanged",
                "document_id": previous.get("document_id"),
            })
            continue
        plan.append({"category": category, "status": "ready", "body_sha256": digest,
                     "body_bytes": len(body.encode("utf-8"))})

    if args.dry_run or not args.apply:
        print(json.dumps({"plan": plan, "dry_run": True}, indent=2))
        return 1 if failed else 0

    if failed:
        print(json.dumps({"plan": plan, "applied": [], "refused": True}, indent=2))
        return 1
    if not args.local_bin:
        fail("--apply needs --local-bin (or PROGRAMME_RECORD_BIN)")

    today = datetime.now(timezone.utc).date().isoformat()
    applied: list[dict] = []
    for item in plan:
        if item["status"] != "ready":
            applied.append(item)
            continue
        category = item["category"]
        body_path = args.bodies / f"{category}.md"
        body = body_path.read_text(encoding="utf-8")
        evidence = args.facts_dir / f"{category}.facts.json" if args.facts_dir else body_path
        if not evidence.exists():
            fail(f"{category}: write evidence {evidence} does not exist before the action")
        title = TITLES.get(category, category.replace("-", " ").title())
        compose = {
            "title": f"{title} (retrospective, {today})",
            "body": body,
            "project_id": p0,
            "category": category,
            "publishable": True,
            "meta": {"w3_natural_key": f"programme-artefact:{category}:{item['body_sha256'][:16]}"},
        }
        created = record(
            "documents.compose", compose,
            read_action="documents.get", read_input=None,
            if_match=None, read_id_from_output="id",
            evidence=evidence, actor=args.actor_id, local_bin=args.local_bin,
        )
        document_id = str(created.get("id") or "")
        if not document_id:
            fail(f"{category}: documents.compose returned no id: {created}")
        published = record(
            "documents.publish", {"id": document_id, "publish": True},
            read_action="documents.get", read_input={"id": document_id},
            if_match=int(created.get("version") or 1), read_id_from_output=None,
            evidence=evidence, actor=args.actor_id, local_bin=args.local_bin,
        )
        manifest.setdefault("documents", {})[category] = {
            "document_id": document_id,
            "body_sha256": item["body_sha256"],
            "version": int(published.get("version") or created.get("version") or 1),
            "title": compose["title"],
            "applied_at": datetime.now(timezone.utc).isoformat(),
        }
        write_manifest(manifest)
        evidence_ref = (
            str(evidence.relative_to(ROOT))
            if str(evidence).startswith(str(ROOT) + os.sep) else str(evidence)
        )
        append_journal({
            "action": "documents.compose",
            "actor": args.actor_id,
            "input": evidence_ref,
            "natural_key": compose["meta"]["w3_natural_key"],
            "output_id": document_id,
            "readback": f"documents.get:{document_id}",
        })
        append_journal({
            "action": "documents.publish",
            "actor": args.actor_id,
            "input": f"document:{document_id}",
            "natural_key": compose["meta"]["w3_natural_key"],
            "output_id": document_id,
            "readback": f"documents.get:{document_id}:version={published.get('version')}",
        })
        applied.append({"category": category, "status": "published",
                        "document_id": document_id})
        print(f"published {category}: {document_id}")
    print(json.dumps({"plan": plan, "applied": applied}, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
