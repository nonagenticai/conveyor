#!/usr/bin/env python3
"""Generate the MSP / TOGAF / SAFe artefact record from the published snapshot.

The output standard (`seed/output-standard.json`) declares, for every standards artefact:
its standard, level, subject kind, mandatory flag, SPEC clause, and for each required
section the source it derives from. This script instantiates every applicable artefact for
its owning subject and reports one of three honest statuses:

  evidenced  every required section resolves to a named row or published document
  partial    some required sections resolve, some do not
  absent     no required section resolves

An absent or partial mandatory artefact becomes a finding in the v1 findings schema, so the
rework register turns it into a work package. Nothing here invents prose: a section renders
the rows it resolved, or the fixed line naming the record that would satisfy it.

Usage:
    python3 scripts/generate-standards.py --check            # verify the committed record
    python3 scripts/generate-standards.py --data-out DIR --html-out DIR --findings-out FILE
    python3 scripts/generate-standards.py --self-test

Exit 0 = the record was generated (or the committed one is current); exit 1 = drift or a
defect. The publish pipeline runs pass 1 (findings) before the audit and pass 2 (data + HTML,
with the register) after the strict gate.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import html
import json
import re
import sys
import tempfile
from pathlib import Path
from typing import Any, Iterable

ROOT = Path(__file__).resolve().parents[1]
STANDARD_PATH = ROOT / "seed" / "output-standard.json"
PROGRAMME_SEED_PATH = ROOT / "seed" / "programme.json"
FINDINGS_PATH = ROOT / "docs" / "evidence" / "standards-findings.json"
DATA_PATH = ROOT / "docs" / "design" / "data" / "standards.json"
SCHEMA = "design-standards/v1"
FINDINGS_SCHEMA = "conveyor-v1-findings/v1"
PACKAGE_SCHEMA = "conveyor-rework/v1"

GATE_OBJECTS = {
    "business-case", "benefit-profile", "tranche-review", "statement-of-architecture-work",
    "compliance-review", "project-baseline", "change-control",
}
TOGAF_PROJECT_CATEGORIES = {
    "architecture-design", "architecture-analysis", "architecture-artifact",
    "statement-of-architecture-work", "compliance-review", "waiver",
    "delivery-contract", "statement-of-intent", "request-for-architecture-work",
    # Phase E's named artefacts (SPEC_V2 §8.1): a governed project's solution contract needs the
    # gap analysis, the candidates, and the project context and benefits diagrams. They join this
    # set so a project without them produces an honest absent artefact, not silence.
    "candidate-solutions", "project-context-diagram", "benefits-diagram", "gap-analysis",
}
# A data-gap rule already owns this defect. The artefact record still reports the honest status,
# but the package is the gap rule's: one defect, one package. A partial artefact is NOT covered,
# because a published document that lacks a required heading is a section-level finding the gap
# rules do not see.
COVERED_BY = {
    **{f"phase-{code}": "mandatory_artefacts" for code in ["preliminary", "a", "b", "c", "d", "e", "f", "g", "h"]},
    "architecture-artifact": "mandatory_artefacts",
    "project-wbs": "wbs_absent",
    "programme-baselines": "programme_baselines",
    "delivery-contract": "contract_missing",
    "statement-of-architecture-work": "contract_missing",
    "compliance-review": "compliance_review_missing",
}
ABSENT_ONLY_COVERED = {"project-brief": "unpublished_declared_output", "closure-review": "closed_without_closure_review"}
PHASE_ORDER = ["preliminary", "a", "b", "c", "d", "e", "f", "g", "h"]
HEADING = re.compile(r"<h([1-6])[^>]*>(.*?)</h\1>", re.IGNORECASE | re.DOTALL)
TAG = re.compile(r"<[^>]+>")


class GenerationError(Exception):
    pass


# --- the snapshot --------------------------------------------------------------------------

def load_gap_keys(path: Path) -> set[str]:
    if not path.is_file():
        return set()
    document = json.loads(path.read_text(encoding="utf-8"))
    return {
        f"{gap.get('rule')}:{gap.get('subject')}"
        for gap in document.get("gaps") or []
        if isinstance(gap, dict)
    }


def load_snapshot(directory: Path) -> dict[str, Any]:
    snapshot: dict[str, Any] = {}
    if not directory.is_dir():
        raise GenerationError(f"{directory} is not a directory")
    for path in sorted(directory.iterdir()):
        if path.suffix != ".json":
            continue
        try:
            snapshot[path.stem] = json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError as error:
            raise GenerationError(f"{path} is not JSON: {error}") from error
    return snapshot


def load_declared_phases(path: Path = PROGRAMME_SEED_PATH) -> dict[str, str]:
    """The ADM phase each project declares, from the checked-in programme seed.

    `seed/programme.json` is built from `seed/programme-definition.json`, which carries
    `{adm_phase, plateau}` per project. The live snapshot's project meta does not project
    those keys, so the generator reads the seed — a named record, not a guess. An empty map
    keeps the generator usable against a fixture or a snapshot-only tree.
    """
    if not path.is_file():
        return {}
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return {}
    declared: dict[str, str] = {}
    for project in document.get("projects") or []:
        if not isinstance(project, dict):
            continue
        phase = str((project.get("meta") or {}).get("adm_phase") or "").strip().lower()
        key = str(project.get("key") or "")
        if key and phase in PHASE_ORDER:
            declared[key] = phase
    return declared


def pointer(document: Any, path: str) -> Any:
    if path in ("", "/"):
        return document
    if not path.startswith("/"):
        raise GenerationError(f"not a JSON pointer: {path}")
    value = document
    for part in path[1:].split("/"):
        part = part.replace("~1", "/").replace("~0", "~")
        if isinstance(value, list):
            try:
                value = value[int(part)]
            except (ValueError, IndexError) as error:
                raise GenerationError(f"{path} does not resolve: {error}") from error
        elif isinstance(value, dict):
            if part not in value:
                return []
            value = value[part]
        else:
            return []
    return value


def pointer_problems(std: dict[str, Any], snapshot: dict[str, Any]) -> list[str]:
    """Every declared row source must resolve against the snapshot it will be read with.

    A wrong `file` or a mistyped pointer used to be indistinguishable from a real data gap: the
    section rendered `absent` forever and the catalogue never failed. This gate makes the pointer
    part of the contract. `--check` refuses on any problem; a generate run prints them as
    warnings so the record still describes the snapshot honestly.
    """
    problems: list[str] = []
    for category, template in sorted((std.get("templates") or {}).items()):
        for heading, sources in sorted((template.get("sources") or {}).items()):
            for source in sources:
                if source.get("kind") != "rows":
                    continue
                # A source the current snapshot projection does not carry yet (compliance
                # reviews, waivers, lessons, demo credits) is declared optional: the artefact is
                # honestly absent, and the pointer is not a typo. The flag is removed when the
                # projection lands, and `check-standards.cjs` still requires the package.
                if source.get("optional"):
                    continue
                file = str(source.get("file") or "")
                document = snapshot.get(Path(file).stem)
                if document is None:
                    problems.append(f"{category} / {heading}: no snapshot file {file!r}")
                    continue
                path = str(source.get("pointer") or "/")
                if path in ("", "/"):
                    continue
                if not path.startswith("/"):
                    problems.append(f"{category} / {heading}: {path!r} is not a JSON pointer")
                    continue
                value: Any = document
                broken = False
                for part in path[1:].split("/"):
                    part = part.replace("~1", "/").replace("~0", "~")
                    if isinstance(value, list):
                        try:
                            value = value[int(part)]
                        except (ValueError, IndexError):
                            problems.append(f"{category} / {heading}: {file}#{path} does not resolve")
                            broken = True
                            break
                    elif isinstance(value, dict):
                        if part not in value:
                            problems.append(
                                f"{category} / {heading}: {file}#{path} has no key {part!r}"
                            )
                            broken = True
                            break
                        value = value[part]
                    else:
                        problems.append(f"{category} / {heading}: {file}#{path} is not a container")
                        broken = True
                        break
                if broken:
                    continue
    return problems


def substitute(value: Any, subject: dict[str, Any], category: str, phase: str | None) -> Any:
    if isinstance(value, str):
        replacements = {
            "$subject_id": subject.get("id", ""),
            "$subject_key": subject.get("key", ""),
            "$programme_id": subject.get("programme_id", ""),
            "$project_id": subject.get("project_id") or subject.get("id", ""),
            "$project_key": subject.get("project_key", ""),
            "$category": category,
            "$phase": phase or subject.get("phase", ""),
        }
        for token, replacement in replacements.items():
            if value == token:
                return replacement
        return value
    if isinstance(value, list):
        return [substitute(item, subject, category, phase) for item in value]
    if isinstance(value, dict):
        return {key: substitute(item, subject, category, phase) for key, item in value.items()}
    return value


def matches(row: dict[str, Any], wanted: dict[str, Any]) -> bool:
    for key, value in wanted.items():
        if isinstance(value, list):
            if row.get(key) not in value:
                return False
        elif row.get(key) != value:
            return False
    return True


# --- subjects ------------------------------------------------------------------------------

def project_subjects(programme: dict[str, Any], projects: list[dict[str, Any]]) -> list[dict[str, Any]]:
    subjects: list[dict[str, Any]] = []
    for row in sorted(projects, key=lambda item: str(item.get("key") or item.get("id"))):
        subjects.append({
            "kind": "project",
            "id": str(row.get("id")),
            "key": str(row.get("key") or row.get("id")),
            "title": str(row.get("description") or row.get("key") or row.get("id")),
            "programme_id": str(row.get("programme_id") or programme.get("id")),
            "project_id": str(row.get("id")),
            "project_key": str(row.get("key") or row.get("id")),
            "status": str(row.get("status") or ""),
            "meta": row.get("meta") or {},
        })
    return subjects


# A test row is not a programme project. The marker is `meta.test_row = true`; the two known
# pre-marker e2e keys are named so the existing residue is excluded visibly rather than by a
# pattern that could swallow a real project. The audit carries the same predicate
# (`test_row_reason`); the excluded rows are listed in the record's summary, never silently
# dropped.
TEST_ROW_MARKER = "test_row"
LEGACY_TEST_PROJECT_KEYS = frozenset({
    "AAA-UI-PROOF-285-2026-11-19T07-3-2",
    "AAA-UI-PROOF-285-2026-11-19T07-4",
})


def test_row_reason(row: dict[str, Any]) -> str | None:
    """Why this project row is test residue, or None when it is a real project."""
    meta = row.get("meta")
    if isinstance(meta, dict) and meta.get(TEST_ROW_MARKER) is True:
        return f"meta.{TEST_ROW_MARKER} is true"
    key = str(row.get("key") or "")
    if key in LEGACY_TEST_PROJECT_KEYS:
        return "a known pre-marker e2e project key"
    return None


def build_subjects(
    snapshot: dict[str, Any],
    declared_phases: dict[str, str] | None = None,
) -> dict[str, list[dict[str, Any]]]:
    declared_phases = declared_phases or {}
    programme_doc = snapshot.get("programme") or {}
    programmes = programme_doc.get("programme") or []
    all_projects = programme_doc.get("projects") or []
    projects = [row for row in all_projects if not test_row_reason(row)]
    if not programmes:
        raise GenerationError("the snapshot carries no programme row")
    programme = programmes[0]
    programme_id = str(programme.get("id"))
    subjects: dict[str, list[dict[str, Any]]] = {
        "programme": [{
            "kind": "programme",
            "id": programme_id,
            "key": programme_id,
            "title": str(programme.get("name") or programme_id),
            "programme_id": programme_id,
            "meta": programme.get("meta") or {},
        }],
        "project": project_subjects(programme, projects),
        "tranche": [],
        "benefit": [],
        "capability": [],
        "phase": [],
        "artifact": [],
        "card": [],
    }
    subjects["capability"] = [{
        "kind": "capability",
        "id": programme_id,
        "key": programme_id,
        "title": f"Architecture capability — {programme.get('name') or programme_id}",
        "programme_id": programme_id,
        "meta": {},
    }]

    msp = snapshot.get("msp") or {}
    for row in sorted(msp.get("tranches") or [], key=lambda item: (item.get("number") or 0, str(item.get("id")))):
        subjects["tranche"].append({
            "kind": "tranche",
            "id": str(row.get("id")),
            "key": f"tranche-{row.get('number')}",
            "title": f"Tranche {row.get('number')}",
            "programme_id": str(row.get("programme_id") or programme_id),
            "meta": row,
        })
    for row in sorted(msp.get("benefit_profiles") or [], key=lambda item: str(item.get("id"))):
        title = str(row.get("description") or row.get("reference") or row.get("id"))
        subjects["benefit"].append({
            "kind": "benefit",
            "id": str(row.get("id")),
            "key": str(row.get("reference") or row.get("id")),
            "title": title,
            "programme_id": str(row.get("programme_id") or programme_id),
            "meta": row,
        })

    architecture = snapshot.get("architecture") or {}
    contracts = architecture.get("contracts") or []
    governed: dict[str, set[str]] = {}
    for contract in contracts:
        project_id = str(contract.get("project_id") or "")
        if project_id:
            governed.setdefault(project_id, set()).add("contract")
    profile_phases = PHASE_ORDER[:]
    for profile in architecture.get("profiles") or []:
        declared = [str(code).lower() for code in profile.get("phase_set") or []]
        if declared:
            profile_phases = [code for code in PHASE_ORDER if code in declared]
            break
    seen_phases: set[tuple[str, str]] = set()
    for subject in subjects["project"]:
        if subject["id"] not in governed:
            continue
        for phase in profile_phases:
            if phase == "preliminary":
                # The capability page owns Preliminary; a per-project preliminary subject has
                # no category to render (the same rule as the declared-phase loop below).
                continue
            seen_phases.add((subject["id"], phase))
            subjects["phase"].append({
                "kind": "phase",
                "id": f"{subject['id']}:{phase}",
                "key": f"{subject['key']}/{phase}",
                "title": f"{subject['key']} — {phase.upper()}",
                "programme_id": subject["programme_id"],
                "project_id": subject["id"],
                "project_key": subject["key"],
                "phase": phase,
                "meta": subject["meta"],
            })
    # Every project declares its ADM phase in the programme seed. A project without a contract
    # or a phase document is still governed by that phase, so it must produce a phase artefact
    # (absent when nothing was recorded) rather than no artefact at all. The gap was invisible:
    # only contracted projects were instanced, so an uncontracted project had no phase record
    # to be missing. `meta.adm_phase` is authoritative when the snapshot carries it; the seed
    # fallback is the checked-in definition.
    for subject in subjects["project"]:
        if str((subject.get("meta") or {}).get("measured_status") or "") == "archived":
            continue
        phase = str(
            (subject.get("meta") or {}).get("adm_phase")
            or declared_phases.get(subject["key"])
            or ""
        ).strip().lower()
        if phase not in PHASE_ORDER or (subject["id"], phase) in seen_phases:
            continue
        if phase == "preliminary":
            # Preliminary is the capability, not a per-project phase: its record is the
            # programme-level `phase-preliminary` page. Creating a phase subject for each
            # programme-definition project would produce a subject no category can render.
            continue
        seen_phases.add((subject["id"], phase))
        subjects["phase"].append({
            "kind": "phase",
            "id": f"{subject['id']}:{phase}",
            "key": f"{subject['key']}/{phase}",
            "title": f"{subject['key']} — {phase.upper()}",
            "programme_id": subject["programme_id"],
            "project_id": subject["id"],
            "project_key": subject["key"],
            "phase": phase,
            "meta": subject["meta"],
        })

    work = snapshot.get("work") or {}
    artefacts = snapshot.get("artefacts") or {}
    document_cards = {str(doc.get("card_id")) for doc in artefacts.get("documents") or [] if doc.get("card_id")}
    project_by_id = {subject["id"]: subject for subject in subjects["project"]}
    # One typed page per TOGAF architecture artefact (catalog, matrix or diagram). The old
    # `architecture-artifact` category summarised every artefact of a project on one page; a
    # reader could not see an individual artefact's entries, links or document. Each artefact
    # is now its own subject and gets its own page and package key.
    for row in sorted(architecture.get("artifacts") or [], key=lambda item: str(item.get("id"))):
        artifact_id = str(row.get("id") or "")
        if not artifact_id:
            continue
        project = project_by_id.get(str(row.get("project_id") or ""))
        subjects["artifact"].append({
            "kind": "artifact",
            "id": artifact_id,
            "key": artifact_id,
            "title": str(row.get("title") or row.get("artifact_kind") or artifact_id),
            "programme_id": programme_id,
            "project_id": str(row.get("project_id") or ""),
            "project_key": project["key"] if project else "",
            "phase": str(row.get("phase_code") or "").lower(),
            "meta": row,
        })
    for card in sorted(work.get("cards") or [], key=lambda item: str(item.get("id"))):
        card_id = str(card.get("id"))
        meta = card.get("meta") or {}
        if card_id not in document_cards and not meta.get("rework_key"):
            continue
        project = project_by_id.get(str(card.get("project_id") or ""))
        if project is None:
            continue
        subjects["card"].append({
            "kind": "card",
            "id": card_id,
            "key": str(card.get("key") or card_id),
            "title": str(card.get("title") or card_id),
            "programme_id": project["programme_id"],
            "project_id": project["id"],
            "project_key": project["key"],
            "meta": meta,
        })
    return subjects


# --- source resolution ---------------------------------------------------------------------

def documents_for(
    snapshot: dict[str, Any],
    subject: dict[str, Any],
    category: str,
    scope: str,
    programme_projects: dict[str, set[str]],
) -> list[dict[str, Any]]:
    documents = (snapshot.get("artefacts") or {}).get("documents") or []
    selected: list[dict[str, Any]] = []
    for doc in documents:
        if doc.get("category") != category:
            continue
        # #537 made publication a timestamp. Read it when present; fall back to the old flag for
        # a snapshot produced before that change so a rebase never drops every document.
        if "published_at" in doc:
            if not doc.get("published_at"):
                continue
        elif not doc.get("publishable", True):
            continue
        project_id = str(doc.get("project_id") or "")
        if scope == "subject":
            if project_id != subject.get("project_id"):
                continue
        else:
            if project_id not in programme_projects.get(subject.get("programme_id", ""), set()):
                continue
        selected.append(doc)
    return sorted(selected, key=lambda item: (str(item.get("created_at") or ""), str(item.get("id"))))


def document_headings(outputs_dir: Path, document_id: str) -> set[str] | None:
    page = outputs_dir / f"output-{document_id}.html"
    if not page.is_file():
        return None
    text = page.read_text(encoding="utf-8", errors="replace")
    found: set[str] = set()
    for _level, body in HEADING.findall(text):
        found.add(normalise(TAG.sub("", body)))
    return found


def normalise(text: str) -> str:
    return " ".join(html.unescape(text).split()).strip().lower()


def rows_for(
    snapshot: dict[str, Any],
    source: dict[str, Any],
    subject: dict[str, Any],
    category: str,
    phase: str | None,
) -> list[dict[str, Any]]:
    document = snapshot.get(Path(source["file"]).stem)
    if document is None:
        return []
    value = pointer(document, source.get("pointer", "/"))
    if isinstance(value, dict):
        value = [value]
    if not isinstance(value, list):
        return []
    wanted = substitute(source.get("match") or {}, subject, category, phase)
    return [row for row in value if isinstance(row, dict) and matches(row, wanted)]


def artifact_satisfies_requirement(artifact: dict[str, Any], key: str) -> bool:
    """Does this artefact satisfy a required artifact type?

    The snapshot projects `artifact_kind` (the type's code, e.g. `principles_catalog` or the
    generic `diagram`) and a UUID `artifact_type_id`, but not the type's `key`. A diagram whose
    title is "Principle derivation — …" is the `principle_derivation` requirement; matching only
    the kind would report two of the three Preliminary requirements missing while their artefacts
    exist. The title fallback requires every distinctive word of the key.
    """
    kind = str(artifact.get("artifact_kind") or "").lower()
    if kind == str(key).lower():
        return True
    if str(artifact.get("id") or "") == str(key):
        return True
    words = {word for word in re.split(r"[^a-z0-9]+", str(key).lower()) if len(word) > 3}
    title = {word for word in re.split(r"[^a-z0-9]+", str(artifact.get("title") or "").lower()) if len(word) > 3}
    return bool(words) and words <= title


def computed_rows(
    name: str,
    source: dict[str, Any],
    snapshot: dict[str, Any],
    subject: dict[str, Any],
    programme_projects: dict[str, set[str]],
) -> list[dict[str, Any]]:
    msp = snapshot.get("msp") or {}
    architecture = snapshot.get("architecture") or {}
    programme_doc = snapshot.get("programme") or {}
    if name == "programme_summary":
        projects = programme_doc.get("projects") or []
        programme_id = subject.get("programme_id")
        bound = [p for p in projects if str(p.get("programme_id")) == programme_id] if programme_id else projects
        return [{
            "kind": "summary",
            "projects": len(bound),
            "closed": sum(1 for p in bound if p.get("status") == "closed"),
            "active": sum(1 for p in bound if p.get("status") == "active"),
            "tranches": len([t for t in msp.get("tranches") or [] if t.get("programme_id") in (None, programme_id)]),
            "benefits": len([b for b in msp.get("benefit_profiles") or [] if b.get("programme_id") in (None, programme_id)]),
            "cadences": len(msp.get("cadences") or []),
            "baselines": len(msp.get("programme_baselines") or []),
            "register_items": len(msp.get("register_items") or []),
        }]
    if name == "benefit_map":
        return [
            {"kind": "link", **link}
            for link in msp.get("benefit_links") or []
        ]
    if name == "architecture_capability":
        rows: list[dict[str, Any]] = []
        profiles = architecture.get("profiles") or []
        if profiles:
            rows.append({"kind": "profile", **profiles[0]})
        return [
            *rows,
            *[{"kind": "standard", **row} for row in architecture.get("standard") or []],
            *[{"kind": "board", **row} for row in architecture.get("board") or []],
            *[{"kind": "maturity", **row} for row in architecture.get("maturity") or []],
            *[{"kind": "skill", **row} for row in architecture.get("skills") or []],
            *[{"kind": "contract", **row} for row in architecture.get("contracts") or []],
        ]
    if name == "phase_artefacts":
        phase = str(source.get("phase") or subject.get("phase") or "")
        profile = (architecture.get("profiles") or [{}])[0]
        wanted = (profile.get("artifact_requirements") or {}).get(phase) or {}
        names = {
            str(row.get("key")): str(row.get("name") or row.get("key"))
            for row in architecture.get("standard") or []
        }
        rows: list[dict[str, Any]] = []
        for plateau in sorted(wanted):
            for key in wanted[plateau] or []:
                evidence = [
                    artifact for artifact in architecture.get("artifacts") or []
                    if str(artifact.get("phase_code") or "").lower() == phase
                ]
                # A required type is evidenced by an artefact of that type in this phase. The
                # old count matched any artefact in the phase, so one artefact "satisfied"
                # every requirement. A requirement with zero evidence is not a row here: the
                # section is then absent and names the missing set, instead of rendering a
                # table of zeroes that reads as evidenced.
                matches = [
                    artifact for artifact in evidence
                    if artifact_satisfies_requirement(artifact, str(key))
                ]
                if not matches:
                    continue
                rows.append({
                    "kind": "required",
                    "plateau": plateau,
                    "key": key,
                    "name": names.get(str(key), str(key)),
                    "evidence": len(matches),
                })
        return rows
    if name == "requirements_catalog":
        return [
            artifact for artifact in architecture.get("artifacts") or []
            if "requirement" in json.dumps(artifact).lower()
        ]
    if name == "architecture_principles":
        return [
            artifact for artifact in architecture.get("artifacts") or []
            if str(artifact.get("artifact_kind") or "").lower().startswith("principle")
            or "principle" in str(artifact.get("title") or "").lower()
        ]
    if name == "architecture_profile":
        return [{"kind": "profile", **row} for row in architecture.get("profiles") or []]
    if name == "artifact_standard":
        return [{"kind": "standard", **row} for row in architecture.get("standard") or []]
    if name == "architecture_board":
        return [{"kind": "board", **row} for row in architecture.get("board") or []]
    if name == "architecture_governance":
        return [
            *[{"kind": "contract", **row} for row in architecture.get("contracts") or []],
            *[{"kind": "waiver", **row} for row in architecture.get("waivers") or []],
        ]
    if name == "architecture_maturity_skills":
        return [
            *[{"kind": "maturity", **row} for row in architecture.get("maturity") or []],
            *[{"kind": "skill", **row} for row in architecture.get("skills") or []],
        ]
    if name == "artifact_metadata":
        row = subject.get("meta") or {}
        return [{"kind": str(row.get("artifact_kind") or "artifact"), **row}]
    if name == "artifact_entries":
        entries = (subject.get("meta") or {}).get("entries") or []
        return [{"kind": "entry", **entry} for entry in entries if isinstance(entry, dict)]
    if name == "artifact_links":
        artifact_id = str(subject.get("id") or "")
        return [
            {"kind": "link", **link}
            for link in architecture.get("links") or []
            if artifact_id and artifact_id in {
                str(link.get("from_artifact_id") or ""),
                str(link.get("to_artifact_id") or ""),
            }
        ]
    if name == "project_artifact_links":
        owned = {
            str(artifact.get("id") or "")
            for artifact in architecture.get("artifacts") or []
            if str(artifact.get("project_id") or "") == str(subject.get("id") or "")
        }
        return [
            {"kind": "link", **link}
            for link in architecture.get("links") or []
            if str(link.get("from_artifact_id") or "") in owned
            or str(link.get("to_artifact_id") or "") in owned
        ]
    if name == "artifact_document":
        document_id = str((subject.get("meta") or {}).get("document_id") or "")
        if not document_id:
            return []
        return [
            document
            for document in (snapshot.get("artefacts") or {}).get("documents") or []
            if str(document.get("id") or "") == document_id
        ]
    raise GenerationError(f"unknown computed source {name!r}")


def resolve_documents(
    heading: str,
    template: dict[str, Any],
    subject: dict[str, Any],
    snapshot: dict[str, Any],
    outputs_dir: Path,
    programme_projects: dict[str, set[str]],
    category: str | None = None,
    scope: str | None = None,
) -> tuple[dict[str, Any], bool]:
    """Resolve one heading against the published documents. Returns (source, heading_found)."""
    wanted = category or str(template.get("document_category") or "")
    scope = scope or ("subject" if subject["kind"] in {"project", "card", "phase"} else "programme")
    docs = documents_for(snapshot, subject, wanted, scope, programme_projects)
    headings = {str(doc.get("id")): document_headings(outputs_dir, str(doc.get("id"))) for doc in docs}
    heading_key = normalise(heading)
    matched = [
        doc for doc in docs
        if headings[str(doc.get("id"))] is not None
        and heading_key in headings[str(doc.get("id"))]
    ]
    source = {
        "kind": "documents", "file": "artefacts.json", "pointer": "/documents",
        "category": wanted, "scope": scope,
        "count": len(matched),
        "items": [{"id": str(doc.get("id")), "title": str(doc.get("title") or "")}
                  for doc in matched[:12]],
    }
    return source, bool(matched)


def resolve_section(
    heading: str,
    template: dict[str, Any],
    category: str,
    subject: dict[str, Any],
    snapshot: dict[str, Any],
    outputs_dir: Path,
    programme_projects: dict[str, set[str]],
) -> dict[str, Any]:
    sources = template.get("sources") or {}
    declared = sources.get(heading)
    resolved: list[dict[str, Any]] = []
    missing_headings: list[str] = []
    if declared:
        for source in declared:
            kind = source["kind"]
            if kind == "rows":
                rows = rows_for(snapshot, source, subject, category, subject.get("phase"))
                resolved.append({
                    "kind": "rows", "file": source.get("file", ""),
                    "pointer": source.get("pointer", ""), "count": len(rows),
                    "items": [row_summary(row) for row in rows[:12]],
                })
            elif kind == "documents":
                wanted_category = str(substitute(
                    (source.get("match") or {}).get("category", "$category"),
                    subject, template.get("document_category") or category, subject.get("phase"),
                ))
                wanted_scope = str((source.get("match") or {}).get("scope") or
                                   ("subject" if subject["kind"] in {"project", "card", "phase"} else "programme"))
                document_source, heading_found = resolve_documents(
                    heading, template, subject, snapshot, outputs_dir, programme_projects,
                    wanted_category, wanted_scope,
                )
                if not heading_found:
                    missing_headings.append(heading)
                resolved.append(document_source)
            elif kind == "computed":
                rows = computed_rows(str(source.get("name")), source, snapshot, subject, programme_projects)
                resolved.append({
                    "kind": "computed", "name": str(source.get("name")),
                    "count": len(rows), "items": [row_summary(row) for row in rows[:12]],
                })
            elif kind == "register":
                resolved.append({"kind": "register", "count": 0, "items": []})

    # A document-backed artefact falls back to its published document when the row sources are
    # empty (or the template declares no row source for this heading). Without the fallback a
    # published compliance review would read as absent because its row table is empty.
    if template.get("document_category") and not [item for item in resolved if item["count"] > 0]:
        document_source, heading_found = resolve_documents(
            heading, template, subject, snapshot, outputs_dir, programme_projects,
        )
        if document_source["count"] > 0 or document_source.get("items"):
            resolved.append(document_source)
        if not heading_found:
            missing_headings.append(heading)

    evidence = [source for source in resolved if source["count"] > 0]
    status = "evidenced" if evidence else ("partial" if missing_headings else "absent")
    return {
        "heading": heading,
        "status": status,
        "sources": resolved,
        "missing": [] if evidence else [
            f"No row or document satisfies '{heading}'"
            + (f"; a document exists but lacks the heading" if missing_headings else "")
        ],
    }


def row_summary(row: dict[str, Any]) -> dict[str, Any]:
    title = (
        row.get("title") or row.get("name") or row.get("key") or row.get("description")
        or row.get("reference") or row.get("id") or row.get("kind") or ""
    )
    # A catalog entry carries its item name on `axis_b` (and a matrix cell its axes), and an
    # artefact link carries no title at all. Without these the artefact page would list UUIDs
    # where a reader needs the item and the relation.
    if row.get("from_artifact_id") or row.get("to_artifact_id"):
        title = (
            f"{row.get('kind') or 'link'}: {row.get('from_artifact_id') or '?'} → "
            f"{row.get('to_artifact_id') or '?'}"
        )
    elif isinstance(row.get("axis_b"), str) and row.get("axis_b"):
        title = row.get("axis_b")
    elif isinstance(row.get("axis_a"), str) and row.get("axis_a"):
        title = row.get("axis_a")
    return {
        "title": str(title),
        "status": str(row.get("status") or row.get("state") or ""),
        "id": str(row.get("id") or ""),
    }


# --- applicability and instances -------------------------------------------------------------

def declarative(template: dict[str, Any]) -> list[str]:
    return list(template.get("required") or []) + list(template.get("optional") or [])


def applies(
    template: dict[str, Any],
    category: str,
    subject: dict[str, Any],
    snapshot: dict[str, Any],
    programme_projects: dict[str, set[str]],
) -> bool:
    if template.get("subject_kind") != subject["kind"]:
        return False
    if subject["kind"] in {"programme", "capability", "tranche", "benefit"}:
        return True
    if subject["kind"] == "phase":
        # A phase document is the record of that phase, not a copy in every other phase.
        return category == f"phase-{subject['phase']}"
    if subject["kind"] == "artifact":
        # The typed page for one architecture artefact; the category names the instance.
        return category == "architecture-artifact-instance"
    if subject["kind"] == "card":
        return True
    if subject["kind"] == "project":
        meta = subject.get("meta") or {}
        # An archived project is history, not work: it has no live artefacts to be missing. The
        # programme seed keeps the row so the record is complete; the artefact ledger does not
        # ask an archived project to produce anything.
        if str(meta.get("measured_status") or "") == "archived":
            return False
        if category == "closure-review" and str(subject.get("status") or "") not in {"closed", "archived"}:
            # A closure review is due at closure; an active project is not missing one.
            if str(meta.get("measured_status") or "") not in {"closed", "archived"}:
                return False
        declared = category in (meta.get("expected_outputs") or []) or category in (meta.get("closure_outputs") or [])
        if declared:
            return True
        # PMP's artefacts are every project's artefacts, and TOGAF's contract and its Phase-G
        # compliance review are what a delivery project cannot pass design without. Instancing
        # them only when evidence already exists hid the very gaps the programme must rework.
        if template.get("mandatory") and template.get("standard") == "PMP":
            return True
        if template.get("mandatory") and category in {"delivery-contract", "compliance-review"}:
            return True
        docs = documents_for(snapshot, subject, str(template.get("document_category") or category), "subject", programme_projects)
        if docs:
            return True
        for source in (template.get("sources") or {}).values():
            for item in source:
                if item.get("kind") == "rows" and rows_for(snapshot, item, subject, category, subject.get("phase")):
                    return True
        architecture = snapshot.get("architecture") or {}
        governed = {str(contract.get("project_id")) for contract in architecture.get("contracts") or []}
        if category in TOGAF_PROJECT_CATEGORIES and subject["id"] in governed:
            return True
        return False
    return False


def gate_state(
    template: dict[str, Any],
    category: str,
    subject: dict[str, Any],
    snapshot: dict[str, Any],
) -> dict[str, Any]:
    approvals = (snapshot.get("work") or {}).get("approvals") or []
    contract = None
    for item in (snapshot.get("architecture") or {}).get("contracts") or []:
        if str(item.get("project_id")) == subject.get("project_id") and (
            category in TOGAF_PROJECT_CATEGORIES or subject["kind"] == "phase"
        ):
            contract = item
            break
    if contract is not None:
        return {
            "state": str(contract.get("status") or "not recorded"),
            "approval_id": str(contract.get("approval_id") or ""),
            "kind": str(contract.get("kind") or ""),
        }
    matching = [
        approval for approval in approvals
        if str(approval.get("project_id") or "") == subject.get("project_id")
    ]
    if matching:
        approval = sorted(matching, key=lambda item: str(item.get("created_at") or ""))[-1]
        return {
            "state": str(approval.get("status") or "recorded"),
            "approval_id": str(approval.get("id") or ""),
            "kind": str(approval.get("gate") or approval.get("kind") or ""),
        }
    return {"state": "not recorded", "approval_id": "", "kind": ""}


def owner_of(template: dict[str, Any], subject: dict[str, Any]) -> str:
    if subject["kind"] in {"project", "phase", "card", "artifact"} and subject.get("project_key"):
        return str(subject["project_key"])
    return "PLATFORM"


def slug(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", value.lower()).strip("-") or "x"


def build_instances(
    std: dict[str, Any],
    snapshot: dict[str, Any],
    outputs_dir: Path,
    declared_phases: dict[str, str] | None = None,
) -> tuple[list[dict[str, Any]], dict[str, list[dict[str, Any]]]]:
    subjects = build_subjects(snapshot, declared_phases)
    programme_doc = snapshot.get("programme") or {}
    programme_projects: dict[str, set[str]] = {}
    for project in programme_doc.get("projects") or []:
        programme_projects.setdefault(str(project.get("programme_id")), set()).add(str(project.get("id")))
    project_keys = {str(project.get("key")) for project in programme_doc.get("projects") or []}
    instances: list[dict[str, Any]] = []
    seen: set[str] = set()
    for category in std.get("categories") or []:
        template = std["templates"][category]
        for kind in ("programme", "capability", "tranche", "benefit", "phase", "artifact", "project", "card"):
            for subject in subjects.get(kind, []):
                if not applies(template, category, subject, snapshot, programme_projects):
                    continue
                identifier = f"std-{slug(str(template['standard']))}-{slug(category)}-{slug(subject['key'])}"
                if identifier in seen:
                    # Two subjects whose keys slug to the same id would otherwise drop one
                    # artefact silently — and every gate would rebuild the same omission.
                    raise GenerationError(
                        f"two subjects share the artefact id {identifier!r}; "
                        "their keys collide after slugging"
                    )
                seen.add(identifier)
                required = list(template.get("required") or [])
                sections = [
                    resolve_section(
                        heading, template, category, subject, snapshot, outputs_dir, programme_projects
                    )
                    for heading in required
                ]
                for heading in template.get("optional") or []:
                    if heading in (template.get("sources") or {}):
                        sections.append(resolve_section(
                            heading, template, category, subject, snapshot, outputs_dir, programme_projects
                        ))
                evidenced = [section for section in sections if section["status"] == "evidenced"]
                if len(evidenced) == len(required):
                    status = "evidenced"
                elif evidenced:
                    status = "partial"
                else:
                    status = "absent"
                owner = owner_of(template, subject)
                scope = owner if owner in project_keys else "programme"
                package_key = f"{identifier}:{scope}"
                instances.append({
                    "id": identifier,
                    "standard": str(template["standard"]),
                    "level": str(template["level"]),
                    "subject_kind": str(template["subject_kind"]),
                    "category": category,
                    "title": f"{category.replace('-', ' ').title()} — {subject['title']}",
                    "spec": str(template["spec"]),
                    "mandatory": bool(template.get("mandatory")),
                    "subject": {
                        "kind": subject["kind"], "id": subject["id"], "key": subject["key"],
                        "title": subject["title"], "programme_id": subject.get("programme_id", ""),
                        "project_key": subject.get("project_key", ""),
                    },
                    "status": status,
                    "sections": sections,
                    "gate": gate_state(template, category, subject, snapshot),
                    "package_key": package_key,
                    "covered_rule": COVERED_BY.get(category) or ABSENT_ONLY_COVERED.get(category) or "",
                    "owner_project": owner,
                })
    instances.sort(key=lambda item: (item["standard"], item["level"], item["category"], item["id"]))
    return instances, subjects


# --- findings ------------------------------------------------------------------------------

def coverage_key(
    category: str,
    artefact: dict[str, Any],
    covered_keys: set[str],
) -> str:
    """The data-gap key that already owns this defect, or "" when the artefact is alone.

    A gap rule and a standards artefact can describe the same missing record (for example
    `mandatory_artefacts:T9` and the phase record). When the gap exists, the gap owns the
    package; the artefact record links to it instead of creating a second card.
    """
    rule = COVERED_BY.get(category) or ABSENT_ONLY_COVERED.get(category)
    if not rule:
        return ""
    subject = str(artefact["subject"].get("project_key") or artefact["subject"]["key"])
    if rule == "programme_baselines":
        for kind in ("management", "governance", "boundary"):
            key = f"{rule}:{kind}"
            if key in covered_keys:
                return key
        return ""
    key = f"{rule}:{subject}"
    return key if key in covered_keys else ""


def build_findings(
    instances: Iterable[dict[str, Any]],
    snapshot: dict[str, Any],
    covered_keys: set[str] | None = None,
) -> list[dict[str, Any]]:
    covered_keys = covered_keys or set()
    findings: list[dict[str, Any]] = []
    manifest = snapshot.get("manifest") or {}
    sha = str(manifest.get("git_sha") or "")
    if not re.fullmatch(r"[0-9a-f]{40}", sha):
        sha = "0" * 40
    # A project does not start before its tranche is authorised (SPEC_V2 §8.3). An artefact of a
    # project in a still-planned tranche is therefore not yet due: the record keeps the honest
    # `absent` status, but the finding is planning work (P2), not a P0 gate failure. Without this
    # a planned tranche's missing baselines would outrank the tranche the programme is actually
    # working. A closed or authorised tranche keeps the full severity.
    programme_doc = snapshot.get("programme") or {}
    project_meta = {
        str(project.get("key") or project.get("id")): (project.get("meta") or {})
        for project in programme_doc.get("projects") or []
    }
    tranche_state = {
        str(tranche.get("number")): str(tranche.get("state") or "")
        for tranche in (snapshot.get("msp") or {}).get("tranches") or []
    }
    for artefact in instances:
        if artefact["status"] == "evidenced" or not artefact["mandatory"]:
            continue
        meta = project_meta.get(str(artefact["subject"].get("project_key") or ""), {})
        tranche = meta.get("tranche")
        state = tranche_state.get(str(tranche), "") if tranche is not None else ""
        not_due = bool(state) and state not in {"closed", "in_review", "authorised", "active"}
        archived = str(meta.get("measured_status") or "") == "archived"
        if archived:
            continue
        covered = coverage_key(artefact["category"], artefact, covered_keys)
        artefact["covered_key"] = covered
        if covered and artefact["status"] == "absent":
            # The gap rule owns the whole artefact. The artefact record keeps the honest status
            # and links to that package; a second finding would be the same defect twice. A
            # PARTIAL artefact is different: its missing sections are section-level findings the
            # gap rules do not see, so it keeps its own package.
            continue
        missing = [
            section["heading"] for section in artefact["sections"] if section["status"] != "evidenced"
        ]
        hints: list[str] = []
        for section in artefact["sections"]:
            if section["status"] == "evidenced":
                continue
            for source in section["sources"]:
                if source.get("file") and source.get("pointer"):
                    hints.append(f"{source['file']}#{source['pointer']}")
                elif source.get("category"):
                    hints.append(f"a published {source['category']} document")
        hints = list(dict.fromkeys(hints)) or [f"seed/output-standard.json#{artefact['category']}"]
        record_hint = ", ".join(hints)
        severity = "P2"
        if artefact["status"] == "absent":
            severity = "P0" if artefact["category"] in GATE_OBJECTS else "P1"
        if not_due:
            severity = "P2"
        due_note = (
            f" Not yet due: tranche {tranche} is {state or 'planned'}."
            if not_due else ""
        )
        evidence = []
        for section in artefact["sections"]:
            if section["status"] == "evidenced":
                continue
            evidence.append({
                "source": "artefact",
                "artefact": artefact["id"],
                "section": section["heading"],
                "snapshot": [
                    [source.get("file"), source.get("pointer"), source.get("count")]
                    for source in section["sources"]
                ] or [["seed/output-standard.json", artefact["category"], 0]],
            })
        findings.append({
            "id": artefact["id"],
            # The audit puts `area` on the package as its artefact; for an artefact finding the
            # category is the useful name, while `spec` still carries the clause.
            "area": artefact["category"],
            "standard": artefact["standard"],
            "spec": artefact["spec"],
            "severity": severity,
            "title": f"{artefact['standard']} artefact {artefact['status']}: {artefact['category']}",
            "problem": (
                f"The {artefact['category']} artefact for {artefact['subject']['title']} is "
                f"{artefact['status']}; missing required sections: {', '.join(missing)}."
                + due_note
            ),
            "evidence": evidence,
            "failure_scenario": (
                f"The published record claims a {artefact['standard']} artefact the row set does not "
                "support, or omits a mandatory one without a work package."
            ),
            "acceptance": (
                f"{artefact['id']}: evidence the missing section(s) {', '.join(missing) or 'all'} "
                f"of {artefact['title']} from {record_hint}, or retire the category with an "
                "owner's decision; `python3 scripts/generate-standards.py --check "
                "--snapshot docs/design/data` must then report no finding for this artefact."
            ),
            "owner_project": artefact["owner_project"],
            "source": "artefact",
            "category": artefact["category"],
            "subject": artefact["subject"],
            "missing": missing,
            "package_key": artefact["package_key"],
            "audited_sha": sha,
        })
    findings.sort(key=lambda item: (item["severity"], item["id"]))
    return findings


def findings_document(findings: list[dict[str, Any]], snapshot: dict[str, Any]) -> dict[str, Any]:
    manifest = snapshot.get("manifest") or {}
    sha = str(manifest.get("git_sha") or "")
    if not re.fullmatch(r"[0-9a-f]{40}", sha):
        sha = "0" * 40
    return {
        "schema": FINDINGS_SCHEMA,
        "audited_sha": sha,
        "audited_at": str(manifest.get("generated_at") or "1970-01-01T00:00:00Z")[:10],
        "method": (
            "generate-standards.py resolves every section of every applicable standards artefact "
            "against the published snapshot, its documents and the artefact standard; only an "
            "unresolved mandatory section becomes a finding."
        ),
        "findings": findings,
    }


# --- rendering -----------------------------------------------------------------------------

def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def escape(value: Any) -> str:
    return html.escape(str(value), quote=True)


def skeleton_bodies(template: dict[str, Any]) -> dict[str, str]:
    """The authored explanation under each heading, keyed by the normalised heading.

    The catalogue's skeleton is the artefact standard's documentation, not decoration: a page
    that shows only row counts tells a reader nothing about what the section is for. The prose
    is authored once per category; the rows are appended beneath it.
    """
    text = str(template.get("skeleton") or "")
    bodies: dict[str, str] = {}
    current: str | None = None
    buffer: list[str] = []
    for line in text.splitlines():
        if line.startswith("## "):
            if current is not None:
                bodies[current] = " ".join(" ".join(buffer).split())
            current = normalise(line[3:])
            buffer = []
        elif current is not None and line.strip() and not line.startswith("# "):
            buffer.append(line.strip())
    if current is not None:
        bodies[current] = " ".join(" ".join(buffer).split())
    return bodies


def render_page(
    artefact: dict[str, Any],
    generated_at: str,
    git_sha: str,
    skeleton: dict[str, str] | None = None,
) -> str:
    skeleton = skeleton or {}
    sections: list[str] = []
    for section in artefact["sections"]:
        body: list[str] = []
        intent = skeleton.get(normalise(section["heading"]))
        if intent:
            body.append(f"<p class=\"std-intent\">{escape(intent)}</p>")
        evidenced_here = False
        for source in section["sources"]:
            if source["count"] == 0:
                continue
            evidenced_here = True
            body.append(
                f"<p class=\"std-source\">Source: <code>{escape(source.get('file') or source.get('category') or source.get('name'))}"
                f"{('#' + escape(source['pointer'])) if source.get('pointer') else ''}</code> — "
                f"{source['count']} row(s)</p>"
            )
            if source.get("items"):
                body.append("<ul>")
                for item in source["items"]:
                    label = item.get("title") or item.get("id")
                    status = item.get("status")
                    body.append(
                        f"<li>{escape(label)}"
                        + (f" <span class=\"std-state\">{escape(status)}</span>" if status else "")
                        + "</li>"
                    )
                body.append("</ul>")
        if not evidenced_here:
            body.append(
                f"<p class=\"std-absent\">Not evidenced — {escape('; '.join(section['missing']) or 'no source')} "
                f"(see <code>{escape(artefact['package_key'])}</code>).</p>"
            )
        sections.append(f"<h2>{escape(section['heading'])}</h2>\n" + "\n".join(body))
    return f"""<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width,initial-scale=1" />
    <meta name="generator" content="conveyor generate-standards.py" />
    <meta name="description" content="{escape(artefact['standard'])} artefact generated from the published record." />
    <title>{escape(artefact['title'])} · conveyor standards</title>
    <link rel="icon" href="../../assets/conveyor.svg" type="image/svg+xml" />
    <link rel="stylesheet" href="../../assets/plex-sans.css" />
    <link rel="stylesheet" href="../../assets/plex-mono.css" />
    <link rel="stylesheet" href="../../docs.css" />
    <style>
      .std-meta {{ display:grid; grid-template-columns:repeat(auto-fit,minmax(11rem,1fr)); gap:.5rem 1.5rem; margin:1rem 0 2rem; }}
      .std-meta div {{ border-top:1px solid var(--line, #d8d8d8); padding-top:.35rem; }}
      .std-meta dt {{ font-size:.75rem; text-transform:uppercase; letter-spacing:.08em; opacity:.65; }}
      .std-meta dd {{ margin:.15rem 0 0; }}
      .std-state {{ font-size:.75rem; text-transform:uppercase; letter-spacing:.06em; opacity:.7; }}
      .std-absent {{ border-left:3px solid #b45309; padding:.35rem .75rem; background:#fff7ed; }}
      .std-intent {{ max-width:68ch; }}
      .std-source {{ font-size:.85rem; opacity:.8; }}
      .std-index {{ list-style:none; padding:0; }}
      .std-index li {{ border-top:1px solid var(--line, #e5e5e5); padding:.5rem 0; }}
    </style>
  </head>
  <body>
    <header class="doc-topbar">
      <a class="doc-brand" href="../../studio.html"><span class="doc-mark">C</span> conveyor</a>
      <span class="doc-sep">/</span>
      <a href="index.html">Standards</a>
      <span class="doc-grow"></span>
      <a class="doc-source" href="../../data/standards.json">data</a>
    </header>
    <main class="doc-body">
      <h1>{escape(artefact['title'])}</h1>
      <dl class="std-meta">
        <div><dt>Standard</dt><dd>{escape(artefact['standard'])}</dd></div>
        <div><dt>Level</dt><dd>{escape(artefact['level'])}</dd></div>
        <div><dt>Subject</dt><dd>{escape(artefact['subject']['title'])}</dd></div>
        <div><dt>Status</dt><dd>{escape(artefact['status'])}</dd></div>
        <div><dt>SPEC</dt><dd>{escape(artefact['spec'])}</dd></div>
        <div><dt>Gate</dt><dd>{escape(artefact['gate']['state'])}</dd></div>
        <div><dt>Package</dt><dd><code>{escape(artefact['package_key'])}</code></dd></div>
        <div><dt>Provenance</dt><dd>{escape(generated_at)} · {escape(git_sha[:12])}</dd></div>
      </dl>
      {''.join(sections)}
    </main>
  </body>
</html>
"""


def render_index(instances: list[dict[str, Any]], generated_at: str, git_sha: str) -> str:
    by_standard: dict[str, list[dict[str, Any]]] = {}
    for artefact in instances:
        by_standard.setdefault(artefact["standard"], []).append(artefact)
    blocks: list[str] = []
    for standard in sorted(by_standard):
        rows = [
            f"<li><a href=\"{escape(artefact['id'])}.html\">{escape(artefact['title'])}</a> "
            f"<span class=\"std-state\">{escape(artefact['status'])}</span></li>"
            for artefact in by_standard[standard]
        ]
        blocks.append(f"<h2>{escape(standard)}</h2>\n<ul class=\"std-index\">\n" + "\n".join(rows) + "\n</ul>")
    return f"""<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width,initial-scale=1" />
    <meta name="generator" content="conveyor generate-standards.py" />
    <title>Standards artefact index · conveyor</title>
    <link rel="icon" href="../../assets/conveyor.svg" type="image/svg+xml" />
    <link rel="stylesheet" href="../../assets/plex-sans.css" />
    <link rel="stylesheet" href="../../assets/plex-mono.css" />
    <link rel="stylesheet" href="../../docs.css" />
    <style>
      .std-state {{ font-size:.75rem; text-transform:uppercase; letter-spacing:.06em; opacity:.7; }}
      .std-index {{ list-style:none; padding:0; }}
      .std-index li {{ border-top:1px solid var(--line, #e5e5e5); padding:.5rem 0; }}
    </style>
  </head>
  <body>
    <header class="doc-topbar">
      <a class="doc-brand" href="../../studio.html"><span class="doc-mark">C</span> conveyor</a>
      <span class="doc-sep">/</span>
      <a href="../../studio.html">Studio</a>
      <span class="doc-grow"></span>
      <a class="doc-source" href="../../data/standards.json">data</a>
    </header>
    <main class="doc-body">
      <h1>Standards artefact index</h1>
      <p>Every MSP, TOGAF, SAFe and PMP artefact generated from the published record at
      {escape(generated_at)} ({escape(git_sha[:12])}). A status of absent or partial names the package
      that must close before the artefact is evidenced.</p>
      {''.join(blocks)}
    </main>
  </body>
</html>
"""


# --- the document --------------------------------------------------------------------------

def build_document(
    std: dict[str, Any],
    snapshot: dict[str, Any],
    instances: list[dict[str, Any]],
    findings: list[dict[str, Any]],
    html_out: Path | None,
) -> dict[str, Any]:
    manifest = snapshot.get("manifest") or {}
    generated_at = str(manifest.get("generated_at") or "1970-01-01T00:00:00Z")
    git_sha = str(manifest.get("git_sha") or "")
    skeletons = {
        category: skeleton_bodies(template)
        for category, template in (std.get("templates") or {}).items()
    }
    for artefact in instances:
        page = render_page(
            artefact, generated_at, git_sha, skeletons.get(artefact["category"])
        )
        artefact["document"] = {
            "path": f"docs/standards/{artefact['id']}.html",
            "sha256": sha256_text(page),
        }
        if html_out is not None:
            html_out.mkdir(parents=True, exist_ok=True)
            (html_out / f"{artefact['id']}.html").write_text(page, encoding="utf-8")
    if html_out is not None:
        html_out.mkdir(parents=True, exist_ok=True)
        (html_out / "index.html").write_text(render_index(instances, generated_at, git_sha), encoding="utf-8")
    summary = {
        "artefacts": len(instances),
        "mandatory": sum(1 for item in instances if item["mandatory"]),
        "evidenced": sum(1 for item in instances if item["status"] == "evidenced"),
        "partial": sum(1 for item in instances if item["status"] == "partial"),
        "absent": sum(1 for item in instances if item["status"] == "absent"),
        "findings": len(findings),
    }
    by_standard: dict[str, dict[str, int]] = {}
    for artefact in instances:
        bucket = by_standard.setdefault(artefact["standard"], {
            "artefacts": 0, "evidenced": 0, "partial": 0, "absent": 0,
        })
        bucket["artefacts"] += 1
        bucket[artefact["status"]] += 1
    document = {
        "schema": SCHEMA,
        "generated_at": generated_at,
        "git_sha": git_sha,
        "summary": summary,
        "by_standard": by_standard,
        "artefacts": instances,
    }
    canonical = json.dumps(document, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    document["payload_sha256"] = sha256_text(canonical)
    return document


def js_assignment(name: str, payload: Any) -> str:
    return (
        "window."
        + name
        + " = "
        + json.dumps(payload, indent=2, sort_keys=True, ensure_ascii=False)
        + ";\n"
    )


def load_register(snapshot_dir: Path) -> dict[str, Any] | None:
    """The register the committed record names its packages from.

    A live publish writes `rework.json` beside the snapshot; the committed pack writes the same
    document as `rework.js`. `--check` must use it, or every artefact renders without
    `package_published` and a reproducible record can never be verified.
    """
    json_path = snapshot_dir / "rework.json"
    if json_path.is_file():
        return json.loads(json_path.read_text(encoding="utf-8"))
    js_path = snapshot_dir / "rework.js"
    if not js_path.is_file():
        return None
    text = js_path.read_text(encoding="utf-8")
    match = re.search(r"window\.CONVEYOR_REWORK\s*=\s*", text)
    if not match:
        raise ValueError(f"{js_path} has no CONVEYOR_REWORK assignment")
    payload = text[match.end():].strip()
    if payload.endswith(";"):
        payload = payload[:-1]
    return json.loads(payload)


def write_outputs(
    document: dict[str, Any],
    findings: dict[str, Any],
    data_out: Path,
    findings_out: Path,
    register: dict[str, Any] | None = None,
) -> None:
    data_out.mkdir(parents=True, exist_ok=True)
    (data_out / "standards.json").write_text(
        json.dumps(document, indent=2, sort_keys=True, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    (data_out / "standards.js").write_text(
        js_assignment("CONVEYOR_STANDARDS", document), encoding="utf-8"
    )
    # The console's offline rework register. The canonical register stays `rework.json` in the
    # repo evidence; this is the same document as the global `delivery.js` reads when the live
    # snapshot carries no `rework` section.
    if register is not None:
        (data_out / "rework.js").write_text(
            js_assignment("CONVEYOR_REWORK", register), encoding="utf-8"
        )
    findings_out.parent.mkdir(parents=True, exist_ok=True)
    findings_out.write_text(
        json.dumps(findings, indent=2, sort_keys=True, ensure_ascii=False) + "\n", encoding="utf-8"
    )


def generate(
    std: dict[str, Any],
    snapshot: dict[str, Any],
    outputs_dir: Path,
    register: dict[str, Any] | None = None,
    html_out: Path | None = None,
    covered_keys: set[str] | None = None,
    declared_phases: dict[str, str] | None = None,
) -> tuple[dict[str, Any], dict[str, Any]]:
    instances, _ = build_instances(std, snapshot, outputs_dir, declared_phases)
    findings = build_findings(instances, snapshot, covered_keys)
    if register is not None:
        packages = [package for package in register.get("packages") or [] if isinstance(package, dict)]
        by_key = {str(package.get("key")): package for package in packages}
        for artefact in instances:
            if artefact["status"] == "evidenced":
                artefact.pop("package_published", None)
                continue
            expected_rule = (
                artefact.get("covered_key") if artefact["status"] == "absent" else ""
            )
            expected_rule = (expected_rule or "").split(":")[0] or artefact["id"]
            found = by_key.get(artefact["package_key"])
            if expected_rule != artefact["id"]:
                found = None
                subject_key = str(artefact["subject"]["key"])
                subject_project = str(artefact["subject"].get("project_key") or "")
                for package in packages:
                    if str(package.get("rule")) != expected_rule:
                        continue
                    subjects = {
                        str(record.get("subject"))
                        for record in package.get("subjects") or []
                        if isinstance(record, dict)
                    }
                    if subject_key in subjects or subject_project in subjects or (
                        str(package.get("scope")) == "programme"
                        and artefact["subject_kind"] in {"programme", "capability", "benefit", "tranche"}
                    ):
                        found = package
                        break
            if found is not None:
                artefact["package_key"] = str(found.get("key"))
                artefact["package_published"] = True
            else:
                artefact["package_published"] = False
    # Render only after the register has been resolved: the page and its hash must name the
    # published package, not the pre-register fallback.
    document = build_document(std, snapshot, instances, findings, html_out)
    findings_doc = findings_document(findings, snapshot)
    return document, findings_doc


# --- check and self-test --------------------------------------------------------------------

def check(
    std: dict[str, Any],
    snapshot: dict[str, Any],
    snapshot_dir: Path,
    outputs_dir: Path,
    covered_keys: set[str],
    declared_phases: dict[str, str] | None = None,
    findings_path: Path | None = None,
) -> int:
    findings_path = findings_path or FINDINGS_PATH
    # The record names a package for every mandatory artefact that is not evidenced, so a
    # committed record can only be reproduced with the register the pack published. A live
    # publish writes `rework.json`; the committed pack writes `rework.js`.
    try:
        register = load_register(snapshot_dir)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(
            f"generate-standards: the register beside {snapshot_dir} does not parse: {error}",
            file=sys.stderr,
        )
        return 1
    document, findings = generate(
        std,
        snapshot,
        outputs_dir,
        register,
        covered_keys=covered_keys,
        declared_phases=declared_phases,
    )
    problems: list[str] = pointer_problems(std, snapshot)
    if findings_path.is_file():
        committed = json.loads(findings_path.read_text(encoding="utf-8"))
        if committed != findings:
            problems.append(f"{findings_path} is stale; regenerate it")
    else:
        problems.append(f"{findings_path} is missing; generate it")
    record_path = snapshot_dir / "standards.json"
    if record_path.is_file():
        committed = json.loads(record_path.read_text(encoding="utf-8"))
        if committed.get("payload_sha256") != document.get("payload_sha256"):
            problems.append(f"{record_path} is stale; regenerate it")
        else:
            # The record claims every artefact has a page whose bytes hash to it. A deleted or
            # edited page is stale even when the payload is unchanged. Containment is asserted
            # so a record cannot point the check outside the pack.
            pack_root = snapshot_dir.parent.resolve()
            for artefact in committed.get("artefacts") or []:
                artefact_id = str(artefact.get("id") or "?")
                page_doc = artefact.get("document") or {}
                if not page_doc.get("path"):
                    problems.append(f"{artefact_id}: the record names no page")
                    continue
                page = (pack_root / str(page_doc["path"])).resolve()
                try:
                    page.relative_to(pack_root)
                except ValueError:
                    problems.append(f"{artefact_id}: page {page_doc['path']} escapes the pack")
                else:
                    if not page.is_file():
                        problems.append(f"{page} is missing; regenerate the record")
                    elif sha256_text(page.read_text(encoding="utf-8")) != page_doc.get("sha256"):
                        problems.append(f"{page} does not match its recorded hash")
        script_path = snapshot_dir / "standards.js"
        if script_path.is_file():
            if script_path.read_text(encoding="utf-8") != js_assignment(
                "CONVEYOR_STANDARDS", committed
            ):
                problems.append(f"{script_path} is stale; regenerate it")
        else:
            problems.append(f"{script_path} is missing; generate it")
    else:
        problems.append(f"{record_path} is missing; generate the record")
    for problem in problems:
        print(f"generate-standards: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(
        f"generate-standards: {document['summary']['artefacts']} artefacts, "
        f"{document['summary']['evidenced']} evidenced, {document['summary']['partial']} partial, "
        f"{document['summary']['absent']} absent, {document['summary']['findings']} findings"
    )
    return 0


def self_test() -> int:
    fixture = {
        "manifest": {"generated_at": "2026-11-20T00:00:00Z", "git_sha": "0" * 40},
        "programme": {
            "programme": [{"id": "pg1", "name": "fixture programme", "status": "active"}],
            "projects": [
                {"id": "p1", "key": "P0", "programme_id": "pg1", "status": "active",
                 "meta": {"track": "programme", "tranche": 0,
                          "expected_outputs": ["vision", "project-brief"],
                          "closure_outputs": ["closure-review"]}},
                {"id": "p2", "key": "T1", "programme_id": "pg1", "status": "closed",
                 "meta": {"track": "v2", "tranche": 1, "expected_outputs": ["evidence"],
                          "closure_outputs": ["closure-review"]}},
            ],
        },
        "msp": {
            "tranches": [{"id": "t1", "number": 1, "programme_id": "pg1", "state": "planned"}],
            "tranche_reviews": [],
            "benefit_profiles": [],
            "benefit_links": [],
            "benefit_transitions": [],
            "programme_baselines": [],
            "project_baselines": [],
            "register_items": [],
            "wbs_nodes": [],
            "cadences": [],
            "project_dependencies": [],
            "stakeholder_profiles": [],
        },
        "architecture": {
            "profiles": [{"phase_set": ["preliminary", "a"], "artifact_requirements": {"a": {"psa": ["stakeholder_map"]}}}],
            "standard": [{"key": "stakeholder_map", "name": "Stakeholder Map", "phase_code": "a", "plateau": "psa"}],
            "artifacts": [{
                "id": "a1", "title": "Principle derivation", "artifact_kind": "diagram",
                "phase_code": "preliminary", "plateau": "psa", "status": "approved",
                "project_id": "p1", "document_id": "d3",
                "entries": [{"id": "e1", "axis_a": "P1", "axis_b": "One model, one store"}],
            }],
            "links": [{"id": "l1", "kind": "derivation", "from_artifact_id": "a1", "to_artifact_id": "a2"}],
            "contracts": [],
            "board": [],
            "maturity": [],
            "skills": [],
        },
        "work": {"cards": [], "approvals": []},
        "artefacts": {"documents": [
            {"id": "d1", "category": "vision", "project_id": "p1", "publishable": True,
             "created_at": "2026-11-20T00:00:00Z", "title": "Vision"},
            {"id": "d2", "category": "closure-review", "project_id": "p2", "publishable": True,
             "created_at": "2026-11-20T00:00:00Z", "title": "Closure"},
            {"id": "d3", "category": "architecture-artifact", "project_id": "p1", "publishable": True,
             "created_at": "2026-11-20T00:00:00Z", "title": "Principle derivation"},
        ]},
    }
    std = json.loads(STANDARD_PATH.read_text(encoding="utf-8"))
    with tempfile.TemporaryDirectory() as tmp:
        outputs = Path(tmp)
        (outputs / "output-d1.html").write_text(
            "<h1>Vision</h1><h2>Purpose</h2><h2>Outcomes</h2><h2>Boundary</h2><h2>Who it serves</h2>",
            encoding="utf-8",
        )
        (outputs / "output-d2.html").write_text(
            "<h1>Closure</h1><h2>Outcome vs acceptance</h2><h2>Evidence</h2>",
            encoding="utf-8",
        )
        covered = {
            "programme_baselines:management",
            "programme_baselines:governance",
            "programme_baselines:boundary",
        }
        document, findings = generate(std, fixture, outputs, covered_keys=covered)
        by_id = {artefact["id"]: artefact for artefact in document["artefacts"]}
        vision = by_id.get("std-msp-vision-p0") or by_id.get("std-msp-vision-pg1")
        assert vision is not None and vision["status"] == "evidenced", "vision should be evidenced"
        closure = by_id.get("std-pmp-closure-review-t1")
        assert closure is not None and closure["status"] == "partial", "the closure document lacks two headings"
        absent = [artefact for artefact in document["artefacts"]
                  if artefact["id"] == "std-msp-programme-strategies-pg1"]
        assert absent and absent[0]["status"] == "absent", "the strategies have no rows"
        assert any(f["category"] == "programme-strategies" for f in findings["findings"]), "an absent artefact must be a finding"
        baseline_artefact = [artefact for artefact in document["artefacts"]
                             if artefact["id"] == "std-msp-programme-baselines-pg1"]
        assert baseline_artefact and baseline_artefact[0]["status"] == "absent", "the baselines have no rows"
        assert not any(f["category"] == "programme-baselines" for f in findings["findings"]), "a covered gap must not duplicate"
        typed = by_id.get("std-togaf-architecture-artifact-instance-a1")
        assert typed is not None and typed["status"] == "evidenced", "one page per typed artefact"
        entries = next(section for section in typed["sections"] if section["heading"] == "Entries")
        assert entries["sources"][0]["items"][0]["title"] == "One model, one store", entries
        links = next(section for section in typed["sections"] if section["heading"] == "Links")
        assert "derivation" in links["sources"][0]["items"][0]["title"], links
        # A mandatory PMP artefact is instanced for every project even with no evidence, so the
        # gap is visible and packaged instead of silently omitted.
        pmp_gap = by_id.get("std-pmp-project-registers-t1")
        assert pmp_gap is not None and pmp_gap["status"] == "absent", "PMP set instances per project"
        again, findings2 = generate(std, fixture, outputs, covered_keys=covered)
        first_ids = {item["id"] for item in findings["findings"]}
        second_ids = {item["id"] for item in findings2["findings"]}
        assert again["payload_sha256"] == document["payload_sha256"], (
            f"the run must be byte-identical: {document['summary']} vs {again['summary']}; "
            f"only second {sorted(second_ids - first_ids)}, only first {sorted(first_ids - second_ids)}"
        )
        assert len(document["artefacts"]) == len(again["artefacts"])
    print(f"generate-standards: self-test OK ({len(document['artefacts'])} artefact instances)")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--snapshot", type=Path, default=ROOT / "docs/design/data")
    parser.add_argument("--outputs-dir", type=Path, default=ROOT / "docs/design/docs")
    parser.add_argument("--standard", type=Path, default=STANDARD_PATH)
    parser.add_argument("--register", type=Path)
    parser.add_argument("--gaps", type=Path,
                        help="the conformance gap register; a gap that already owns a defect "
                             "suppresses the duplicate artefact finding")
    parser.add_argument("--data-out", type=Path)
    parser.add_argument("--html-out", type=Path)
    parser.add_argument("--findings-out", type=Path)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    std = json.loads(args.standard.read_text(encoding="utf-8"))
    if args.self_test:
        return self_test()
    snapshot = load_snapshot(args.snapshot)
    declared_phases = load_declared_phases()
    for problem in pointer_problems(std, snapshot):
        print(f"generate-standards: WARNING broken source: {problem}", file=sys.stderr)
    gap_path = args.gaps or (ROOT / "docs" / "evidence" / "conformance-gap-register.json")
    covered_keys = load_gap_keys(gap_path) if gap_path.is_file() else set()
    if args.check:
        return check(
            std, snapshot, args.snapshot, args.outputs_dir, covered_keys, declared_phases,
            findings_path=args.findings_out or FINDINGS_PATH,
        )
    register = None
    if args.register:
        register = json.loads(args.register.read_text(encoding="utf-8"))
    document, findings = generate(
        std, snapshot, args.outputs_dir, register, args.html_out, covered_keys, declared_phases
    )
    if args.data_out:
        write_outputs(
            document, findings,
            args.data_out,
            args.findings_out or (args.data_out / "standards-findings.json"),
            register,
        )
    elif args.findings_out:
        # Pass 1 of the publish writes findings only; the audit consumes the file before the
        # final snapshot exists.
        args.findings_out.parent.mkdir(parents=True, exist_ok=True)
        args.findings_out.write_text(
            json.dumps(findings, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
            encoding="utf-8",
        )
    print(
        f"generate-standards: {document['summary']['artefacts']} artefacts, "
        f"{document['summary']['evidenced']} evidenced, {document['summary']['partial']} partial, "
        f"{document['summary']['absent']} absent, {document['summary']['findings']} findings"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
