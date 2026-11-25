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
    python3 scripts/generate-standards.py --check-category KEY  # verify one category (repeatable)
    python3 scripts/generate-standards.py --data-out DIR --html-out DIR --findings-out FILE
    python3 scripts/generate-standards.py --self-test

Exit 0 = the record was generated (or the committed one is current); exit 1 = drift or a
defect. `--check-category` exits 0 = every applicable instance has a published document whose
body covers the template's required headings and the composer composes it; 1 = a document is
absent, incomplete, stale or composition refuses; 2 = the category is unknown or has no
instance row (an untriggered on-demand category prints `on-demand untriggered: <key>` and
exits 0). The publish pipeline runs pass 1 (findings) before the audit and pass 2 (data + HTML,
with the register) after the strict gate.
"""

from __future__ import annotations

import argparse
import contextlib
import copy
import hashlib
import html
import importlib.util
import io
import json
import re
import sys
import tempfile
import types
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
# The E–H artefacts are the architecture track's. Owner decision 2026-11-24 tailors the ADM:
# every delivery project carries Preliminary, A–D and the Phase-G review; the programme
# (architecture) track also carries E–H. Applying E–H to a delivery project is the old
# over-instancing that made 14 projects "miss" artefacts their tailoring does not require.
ARCHITECTURE_TRACK_CATEGORIES = {
    "gap-analysis", "candidate-solutions", "project-context-diagram", "benefits-diagram",
    "implementation-migration-plan", "request-for-architecture-work",
}


def architecture_track(subject: dict[str, Any]) -> bool:
    """The programme (architecture) track, from the live meta — never a guess.

    `meta.adm_profile` wins when it names an architecture profile; otherwise the programme seed's
    `meta.track == "programme"` is the mapping measured in the 2026-11-24 snapshot (four projects:
    P0, PLATFORM, DESIGN-PUBLISH, SPEC-V2). v1/v2 delivery projects are not the architecture
    track, and a project that declares an E–H output still gets it (the declared check is first).
    """
    meta = subject.get("meta") or {}
    named = str(meta.get("adm_profile") or "").strip().lower()
    if named:
        return "architecture" in named or named in {"programme", "a-h", "ah"}
    return str(meta.get("track") or "").strip().lower() == "programme"
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


def pointer_defects(std: dict[str, Any], snapshot: dict[str, Any]) -> list[dict[str, Any]]:
    """Every declared row source must resolve against the snapshot it will be read with.

    A wrong `file` or a mistyped pointer used to be indistinguishable from a real data gap: the
    section rendered `absent` forever and the catalogue never failed. This gate makes the pointer
    part of the contract. Each defect is structured (category, heading, source path, reason) so
    the generator can emit a STANDARD finding for it; `--check` refuses on any defect, and a
    generate run also prints the warnings so stderr still describes the snapshot honestly.
    """
    defects: list[dict[str, Any]] = []

    def add(category: str, heading: str, file: str, pointer: str, reason: str) -> None:
        defects.append({
            "category": category,
            "heading": heading,
            "file": file,
            "pointer": pointer,
            "source": f"{file}#{pointer}" if pointer else file,
            "reason": reason,
        })

    for category, template in sorted((std.get("templates") or {}).items()):
        for heading, sources in sorted((template.get("sources") or {}).items()):
            for source in sources:
                if source.get("kind") != "rows":
                    continue
                # A source the current snapshot projection does not carry yet (compliance
                # reviews, waivers, lessons, demo credits) is declared optional: its absence is
                # a P2 section gap, not the severity the source declares, and the pointer is not
                # a typo. The flag is removed when the projection lands, and
                # `check-standards.cjs` still requires the package.
                if source.get("optional"):
                    continue
                # The records session's flip makes a live-projected source mandatory while the
                # committed pack is still frozen before the projection: the pointer is absent
                # from this snapshot and is not a typo. `projection: live` says exactly that —
                # the section stays absent at the declared severity, and the pointer check
                # resumes once a live snapshot carries the collection.
                if source.get("projection") == "live":
                    continue
                file = str(source.get("file") or "")
                document = snapshot.get(Path(file).stem)
                if document is None:
                    add(category, heading, file, str(source.get("pointer") or ""),
                        f"no snapshot file {file!r}")
                    continue
                path = str(source.get("pointer") or "/")
                if path in ("", "/"):
                    continue
                if not path.startswith("/"):
                    add(category, heading, file, path, f"{path!r} is not a JSON pointer")
                    continue
                value: Any = document
                broken = False
                for part in path[1:].split("/"):
                    part = part.replace("~1", "/").replace("~0", "~")
                    if isinstance(value, list):
                        try:
                            value = value[int(part)]
                        except (ValueError, IndexError):
                            add(category, heading, file, path,
                                f"{file}#{path} does not resolve")
                            broken = True
                            break
                    elif isinstance(value, dict):
                        if part not in value:
                            add(category, heading, file, path,
                                f"{file}#{path} has no key {part!r}")
                            broken = True
                            break
                        value = value[part]
                    else:
                        add(category, heading, file, path,
                            f"{file}#{path} is not a container")
                        broken = True
                        break
                if broken:
                    continue
    return defects


def pointer_problems(std: dict[str, Any], snapshot: dict[str, Any]) -> list[str]:
    """The historical string form of `pointer_defects`, one line per defect."""
    return [
        f"{defect['category']} / {defect['heading']}: {defect['reason']}"
        for defect in pointer_defects(std, snapshot)
    ]


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


# --- WSJF (§8.2) ----------------------------------------------------------------------------

# The four typed integer inputs. WSJF is arithmetic, not opinion: the score
# `(business_value + time_criticality + risk_reduction) / job_size` is computed on read and
# never stored. A row missing one input, carrying a non-integer input, a negative value or a
# `job_size` below 1 cannot be ordered, so it is a record defect, not a row to skip.
WSJF_FIELDS = ("business_value", "time_criticality", "risk_reduction", "job_size")
WSJF_VALUE_FIELDS = WSJF_FIELDS[:3]
SEVERITY_RANK = {"": 0, "P2": 1, "P1": 2, "P0": 3}


def json_integer(value: Any) -> int | None:
    """The value when it is a JSON integer, else None (a bool or float is not an input)."""
    if isinstance(value, bool) or not isinstance(value, int):
        return None
    return value


def wsjf_inputs(row: dict[str, Any]) -> dict[str, int] | None:
    """The four inputs when the row is a valid WSJF candidate, else None."""
    values: dict[str, int] = {}
    for field in WSJF_FIELDS:
        value = json_integer(row.get(field))
        if value is None:
            return None
        values[field] = value
    if values["job_size"] < 1:
        return None
    if any(values[field] < 0 for field in WSJF_VALUE_FIELDS):
        return None
    return values


def wsjf_ordered(rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """The rows in WSJF order, or [] when ANY row is malformed.

    An empty list makes the section absent, so one malformed candidate cannot hide behind its
    valid siblings. The computed `wsjf` and `rank` are rendering fields: the snapshot is never
    written, and the score stays out of the record.
    """
    scored: list[tuple[float, dict[str, Any]]] = []
    for row in rows:
        values = wsjf_inputs(row)
        if values is None:
            return []
        score = (
            values["business_value"] + values["time_criticality"] + values["risk_reduction"]
        ) / values["job_size"]
        scored.append((score, row))
    scored.sort(key=lambda item: (-item[0], str(item[1].get("position") or ""), str(item[1].get("id") or "")))
    return [
        {**row, "wsjf": score, "rank": rank}
        for rank, (score, row) in enumerate(scored, start=1)
    ]


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
    rows = [row for row in value if isinstance(row, dict) and matches(row, wanted)]
    if source.get("validate") == "wsjf":
        # A WSJF read must go through the category's own filter: `/wbs_nodes` unfiltered is
        # every phase and milestone, not the plan's candidate set, so an unfiltered source is
        # itself the defect. Every selected row must then carry the four typed inputs.
        if not wanted:
            return []
        return wsjf_ordered(rows)
    return rows


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


def shared_config_rows(snapshot: dict[str, Any], subject: dict[str, Any]) -> list[dict[str, Any]]:
    """The programme's `shared_config` (§8.2), as one summary row when it is a real record.

    The migration defaults the column to `{}`, so an empty object is absence, not evidence. A
    malformed shape (non-object, `values` not an object, `locked` not an array) yields no rows,
    which makes the section absent and the artefact's finding name it. A configuration with
    neither values nor locks is likewise empty: the programme shares nothing.
    """
    programmes = (snapshot.get("programme") or {}).get("programme") or []
    programme = next(
        (row for row in programmes if str(row.get("id")) == str(subject.get("programme_id"))),
        programmes[0] if programmes else None,
    )
    if not isinstance(programme, dict):
        return []
    config = programme.get("shared_config")
    if not isinstance(config, dict):
        return []
    values = config.get("values")
    locked = config.get("locked")
    if "values" in config and not isinstance(values, dict):
        return []
    if "locked" in config and not isinstance(locked, list):
        return []
    # The store reads a flat object as the values (migration 0068: `{ "values": {...},
    # "locked": [...] }`, with a flat object accepted and read as values with no locks).
    if "values" not in config:
        values = {key: value for key, value in config.items() if key != "locked"}
    values = values if isinstance(values, dict) else {}
    locked = locked if isinstance(locked, list) else []
    if not values and not locked:
        return []
    return [{
        "kind": "shared_config",
        "title": (
            f"shared configuration — {len(values)} value(s), {len(locked)} locked"
        ),
        "programme_id": str(programme.get("id") or subject.get("programme_id") or ""),
        "values": len(values),
        "locked": len(locked),
    }]


def dependency_graph_rows(snapshot: dict[str, Any]) -> list[dict[str, Any]]:
    """The project dependency graph: the typed rows, else the programme's declared edges.

    `msp.project_dependencies` is the typed `project_dependency` table; `msp.dependencies` is the
    declared graph (each project's `meta.depends_on`, projected by the store). Either is a real
    graph; an edge without both endpoints is malformed and yields no rows, so the section cannot
    pass on a partial graph.
    """
    msp = snapshot.get("msp") or {}
    typed = [row for row in msp.get("project_dependencies") or [] if isinstance(row, dict)]
    if typed:
        if any(not str(row.get("from_project") or "").strip() or not str(row.get("to_project") or "").strip()
               for row in typed):
            return []
        return [
            {
                **row,
                "title": (
                    f"{row.get('from_project')} → {row.get('to_project')}"
                    f" ({row.get('kind') or 'untyped'})"
                ),
            }
            for row in typed
        ]
    declared_doc = msp.get("dependencies")
    if isinstance(declared_doc, dict):
        declared = declared_doc.get("edges") or []
    elif isinstance(declared_doc, list):
        declared = declared_doc
    else:
        declared = []
    declared = [row for row in declared if isinstance(row, dict)]
    if any(not str(row.get("from") or "").strip() or not str(row.get("to") or "").strip()
           for row in declared):
        return []
    return [
        {
            **row,
            "title": f"{row.get('from')} → {row.get('to')} ({row.get('kind') or 'untyped'})",
        }
        for row in declared
    ]


def refinement_cadence_rows(
    snapshot: dict[str, Any],
    source: dict[str, Any],
    subject: dict[str, Any],
    category: str,
    phase: str | None,
) -> list[dict[str, Any]]:
    """The backlog-refinement cadence for a period (§8.2).

    A cadence of kind `refinement` without a period cannot be matched to the PI it refines, so it
    is malformed and yields no rows. An unfiltered read of `/cadences` is not a refinement row.
    """
    wanted = substitute(source.get("match") or {"kind": "refinement"}, subject, category, phase)
    if not wanted:
        return []
    cadences = (snapshot.get("msp") or {}).get("cadences") or []
    selected = [row for row in cadences if isinstance(row, dict) and matches(row, wanted)]
    if any(not str(row.get("period") or "").strip() for row in selected):
        return []
    return [
        {
            **row,
            "title": f"Refinement — {row.get('period')} ({row.get('state') or 'recorded'})",
        }
        for row in selected
    ]


def kata_candidate_rows(snapshot: dict[str, Any]) -> list[dict[str, Any]]:
    """Kata candidates: a retro lesson's typed marker, plus the kata register when projected.

    §8.2's inspect-and-adapt produces lessons and kata candidates. A lesson's candidate travels
    in `meta.kata_candidate` (the typed marker the kata opener consumes); a snapshot may also
    project the `kata` / `kata_candidate` registers. A candidate with no identifier, signature,
    title or statement is malformed: the section stays absent rather than counting an empty row.
    """
    rows: list[dict[str, Any]] = []
    for lesson in (snapshot.get("msp") or {}).get("lessons") or []:
        if not isinstance(lesson, dict):
            continue
        meta = lesson.get("meta")
        marker = meta.get("kata_candidate") if isinstance(meta, dict) else None
        source_kind = str(lesson.get("source") or "").lower()
        if marker or source_kind in {"kata", "kata_candidate"}:
            rows.append({**lesson, "kind": "kata_candidate"})
    kata_document = snapshot.get("kata")
    if isinstance(kata_document, dict):
        registers = [
            value for key in ("candidates", "kata_candidates", "kata", "katas")
            if isinstance(value := kata_document.get(key), list)
        ]
    elif isinstance(kata_document, list):
        registers = [kata_document]
    else:
        registers = []
    for register in registers:
        rows.extend(
            {**row, "kind": "kata_candidate"}
            for row in register
            if isinstance(row, dict)
        )
    if not rows:
        return []
    malformed = [
        row for row in rows
        if not any(
            str(row.get(field) or "").strip()
            for field in ("id", "signature", "title", "statement")
        )
    ]
    if malformed:
        return []
    seen: set[str] = set()
    unique: list[dict[str, Any]] = []
    for row in rows:
        key = next(
            (str(row.get(field)) for field in ("id", "signature", "title", "statement") if row.get(field)),
            "",
        )
        if key in seen:
            continue
        seen.add(key)
        unique.append({
            **row,
            "title": str(
                row.get("title") or row.get("signature") or row.get("statement")
                or row.get("id") or "kata candidate"
            ),
        })
    return unique


def computed_rows(
    name: str,
    source: dict[str, Any],
    snapshot: dict[str, Any],
    subject: dict[str, Any],
    programme_projects: dict[str, set[str]],
    category: str = "",
    phase: str | None = None,
    *,
    register: dict[str, Any] | None = None,
) -> list[dict[str, Any]]:
    msp = snapshot.get("msp") or {}
    architecture = snapshot.get("architecture") or {}
    programme_doc = snapshot.get("programme") or {}
    if name == "gap-analysis":
        return gap_analysis_rows(snapshot, register, subject)
    if name == "shared_config":
        return shared_config_rows(snapshot, subject)
    if name == "dependency_graph":
        return dependency_graph_rows(snapshot)
    if name == "refinement_cadence":
        return refinement_cadence_rows(snapshot, source, subject, category, phase)
    if name == "kata_candidates":
        return kata_candidate_rows(snapshot)
    if name == "wsjf_order":
        # The plan's candidate set is `status: accepted` unless the source narrows it further.
        wanted = substitute(
            source.get("match") or {"status": "accepted"}, subject, category, phase
        )
        selected = [
            row for row in msp.get("wbs_nodes") or []
            if isinstance(row, dict) and matches(row, wanted)
        ]
        return wsjf_ordered(selected)
    if name == "programme_summary":
        projects = programme_doc.get("projects") or []
        programme_id = subject.get("programme_id")
        bound = [p for p in projects if str(p.get("programme_id")) == programme_id] if programme_id else projects
        # Every status has its own bucket, archived included, or the canvas's total cannot
        # equal the sum of its breakdown (measured: 48 named, 26 closed + 21 active = 47).
        status_counts: dict[str, int] = {}
        for project in bound:
            status = str(project.get("status") or "unknown")
            status_counts[status] = status_counts.get(status, 0) + 1
        return [{
            "kind": "summary",
            "projects": sum(status_counts.values()),
            "closed": status_counts.get("closed", 0),
            "active": status_counts.get("active", 0),
            "archived": status_counts.get("archived", 0),
            "status_counts": status_counts,
            "tranches": len([t for t in msp.get("tranches") or [] if t.get("programme_id") in (None, programme_id)]),
            "benefits": len([b for b in msp.get("benefit_profiles") or [] if b.get("programme_id") in (None, programme_id)]),
            "cadences": len(msp.get("cadences") or []),
            "baselines": len(msp.get("programme_baselines") or []),
            "register_items": len(msp.get("register_items") or []),
        }]
    if name == "change_impact":
        wanted = substitute(
            source.get("match") or {"kind": "change"}, subject, category, phase
        )
        affected: list[dict[str, Any]] = []
        for row in msp.get("register_items") or []:
            if not isinstance(row, dict) or not matches(row, wanted):
                continue
            meta = row.get("meta") if isinstance(row.get("meta"), dict) else {}
            affected.append({
                **row,
                # The record carries its impact in `meta.impact`; the typed column is
                # null. Publish the field that actually holds the text.
                "impact": row.get("impact") if row.get("impact") is not None else meta.get("impact"),
                "title": row.get("title") or row.get("code") or row.get("id"),
            })
        return affected
    if name == "governing_review":
        project_id = str(subject.get("project_id") or subject.get("id") or "")
        reviews = [
            row for row in architecture.get("compliance_reviews") or []
            if isinstance(row, dict) and str(row.get("project_id") or "") == project_id
        ]
        if not reviews:
            return []
        latest = max(
            reviews,
            key=lambda row: (str(row.get("reviewed_at") or ""), str(row.get("version") or "")),
        )
        evidence = latest.get("evidence") if isinstance(latest.get("evidence"), dict) else {}
        return [{
            "kind": "governing_review",
            "id": latest.get("id"),
            "level": latest.get("level"),
            "outcome": latest.get("outcome"),
            "conditions": latest.get("conditions") or evidence.get("conditions"),
            "reviewed_at": latest.get("reviewed_at"),
            "reviewed_by_name": latest.get("reviewed_by_name"),
            "findings": latest.get("findings") or [],
            "title": (
                f"governing review {latest.get('id')}: "
                f"{latest.get('level') or 'unlevelled'} / {latest.get('outcome') or 'no outcome'}"
            ),
        }]
    if name == "waiver_status":
        project_id = str(subject.get("project_id") or subject.get("id") or "")
        waivers = [
            row for row in architecture.get("waivers") or []
            if isinstance(row, dict) and str(row.get("project_id") or "") == project_id
        ]
        if waivers:
            return [
                {
                    **row,
                    "title": (
                        f"waiver {row.get('reason') or row.get('title') or row.get('id')} "
                        f"until {row.get('reassess_at') or row.get('reassessment_at') or 'no date'}"
                    ),
                }
                for row in waivers
            ]
        return [{
            "kind": "waiver_status",
            "waivers": 0,
            "title": "None recorded — 0 waivers for this project",
        }]
    if name == "programme_charter":
        projects = programme_doc.get("projects") or []
        definition = next(
            (row for row in projects if isinstance(row, dict) and str(row.get("key") or "") == "P0"),
            None,
        )
        charters = [
            row for row in (snapshot.get("artefacts") or {}).get("governed_assets") or []
            if isinstance(row, dict)
            and str(row.get("kind") or "") == "charter"
            and str(row.get("owner_user_id") or "")
        ]
        if definition is not None:
            charters = [
                row for row in charters
                if str(row.get("project_id") or "") == str(definition.get("id") or "")
            ]
        if not charters:
            return []
        charter = max(charters, key=lambda row: str(row.get("created_at") or ""))
        return [{
            **charter,
            "title": (
                f"{charter.get('title') or 'Charter'} — owner "
                f"{charter.get('owner_user_id')}"
            ),
        }]
    if name == "action_registry":
        # `actions.json` carries the served action names as strings; `rows_for` keeps
        # dicts only, so the blueprint's registry source resolved to zero even though the
        # registry was in the snapshot. Wrap every served action as a row, then carry the
        # registry's own tables/invariants/events when the snapshot projects them.
        actions_doc = snapshot.get("actions") or {}
        rows: list[dict[str, Any]] = []
        for action in actions_doc.get("actions") or []:
            if isinstance(action, str):
                rows.append({"kind": "action", "name": action, "title": action})
            elif isinstance(action, dict):
                rows.append({
                    "kind": "action",
                    **action,
                    "title": action.get("name") or action.get("key") or action.get("id") or "action",
                })
        config = snapshot.get("config") or {}
        for key, kind in (("tables", "table"), ("invariants", "invariant"), ("events", "event")):
            for item in config.get(key) or []:
                if isinstance(item, str):
                    rows.append({"kind": kind, "name": item, "title": item})
                elif isinstance(item, dict):
                    rows.append({
                        "kind": kind,
                        **item,
                        "title": item.get("name") or item.get("key") or json.dumps(item, sort_keys=True),
                    })
        return rows
    if name == "contract_approval":
        project_id = str(subject.get("project_id") or subject.get("id") or "")
        contracts = [
            row for row in architecture.get("contracts") or []
            if isinstance(row, dict)
            and str(row.get("kind") or "") == "delivery_contract"
            and str(row.get("project_id") or "") == project_id
        ]
        if not contracts:
            return []
        contract = max(
            contracts,
            key=lambda row: (str(row.get("approved_at") or ""), str(row.get("version") or "")),
        )
        approval_id = str(contract.get("approval_id") or "")
        if not approval_id:
            return []
        work = snapshot.get("work") or {}
        approval = next(
            (
                row for row in work.get("approvals") or []
                if isinstance(row, dict) and str(row.get("id") or "") == approval_id
            ),
            None,
        )
        return [{
            "kind": "contract_approval",
            "id": approval_id,
            "status": str((approval or {}).get("status") or contract.get("status") or ""),
            "approved_by": contract.get("approved_by"),
            "approved_at": contract.get("approved_at"),
            "requested_by": contract.get("requested_by"),
            "title": (
                f"approval {approval_id} — "
                f"{(approval or {}).get('status') or contract.get('status') or 'unrecorded'}"
            ),
        }]
    if name == "contract_field":
        field = str(source.get("field") or "")
        project_id = str(subject.get("project_id") or subject.get("id") or "")
        contracts = [
            row for row in architecture.get("contracts") or []
            if isinstance(row, dict)
            and str(row.get("kind") or "") == "delivery_contract"
            and str(row.get("project_id") or "") == project_id
        ]
        if not contracts or not field:
            return []
        contract = max(
            contracts,
            key=lambda row: (str(row.get("approved_at") or ""), str(row.get("version") or "")),
        )
        body = contract.get("body") if isinstance(contract.get("body"), dict) else {}
        value = body.get(field)
        if value in (None, "", [], {}):
            return []
        return [{
            "kind": "contract_field",
            "field": field,
            "value": value,
            "contract_id": contract.get("id"),
            "approved_by": contract.get("approved_by"),
            "title": (
                f"{field}: "
                f"{value if isinstance(value, str) else json.dumps(value, sort_keys=True)}"
            ),
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
                typed_matches = [
                    artifact for artifact in evidence
                    if artifact_satisfies_requirement(artifact, str(key))
                ]
                if not typed_matches:
                    continue
                rows.append({
                    "kind": "required",
                    "plateau": plateau,
                    "key": key,
                    "name": names.get(str(key), str(key)),
                    "evidence": len(typed_matches),
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
        # The snapshot projects the artefact's typed fields on the ROW (artifact_kind,
        # document_id, entries, ...); meta carries the retrospective binding. Reading only meta
        # false-reddened every artefact-instance section (measured 2026-11-25: 86 artefacts had
        # a document while the resolver reported none).
        meta = dict(subject.get("meta") or {})
        fields = {
            key: subject.get(key)
            for key in (
                "artifact_kind",
                "artifact_type_id",
                "phase_code",
                "plateau",
                "status",
                "version",
                "contract_id",
            )
            if subject.get(key) is not None
        }
        fields.update(meta)
        return [{"kind": str(fields.get("artifact_kind") or "artifact"), **fields}]
    if name == "artifact_entries":
        entries = (
            subject.get("entries")
            or (subject.get("meta") or {}).get("entries")
            or []
        )
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
        document_id = str(
            subject.get("document_id")
            or (subject.get("meta") or {}).get("document_id")
            or ""
        )
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


def gap_analysis_rows(
    snapshot: dict[str, Any], register: dict[str, Any] | None, subject: dict[str, Any],
) -> list[dict[str, Any]]:
    """The open gaps as one list (§8.1 gap analysis): rework, phase E and the MSP register.

    Three measured sources. An open rework package is a gap; a suppressed or resolved record is
    not. A required phase-E artifact type with no artefact that satisfies it is a gap; an
    evidenced type is not. An MSP `register_items` row of kind `issue`/`dependency` that is not
    closed is a gap; any other kind, or a closed row, is not. A project subject sees its own
    packages and its own phase-E artefacts; a programme subject sees every open one.
    """
    msp = snapshot.get("msp") or {}
    architecture = snapshot.get("architecture") or {}
    is_project = str(subject.get("kind") or "") in {"project", "phase"}
    project_ids = {
        str(subject.get("project_id") or ""),
        str(subject.get("id") or ""),
        str(subject.get("project_key") or ""),
        str(subject.get("key") or ""),
    } - {""}
    rows: list[dict[str, Any]] = []
    for package in (register or {}).get("packages") or []:
        if not isinstance(package, dict) or str(package.get("status") or "") != "open":
            continue
        if is_project:
            applies_to = {
                str(package.get("apply_to") or ""),
                str(package.get("owner_project") or ""),
            }
            if str(package.get("scope") or "") != "programme" and not (applies_to & project_ids):
                continue
        rows.append({
            "kind": "rework",
            "id": str(package.get("key") or ""),
            "title": str(package.get("title") or package.get("rule") or "rework package"),
            "severity": str(package.get("severity") or ""),
            "status": "open",
        })
    profile = (architecture.get("profiles") or [{}])[0]
    requirements = profile.get("artifact_requirements") or {}
    wanted = requirements.get("e") or requirements.get("E") or {}
    names = {
        str(row.get("key")): str(row.get("name") or row.get("key"))
        for row in architecture.get("standard") or []
    }
    evidence = [
        artifact for artifact in architecture.get("artifacts") or []
        if str(artifact.get("phase_code") or "").lower() == "e"
        and (not is_project or str(artifact.get("project_id") or "") in project_ids)
    ]
    for plateau in sorted(wanted):
        for key in wanted[plateau] or []:
            if any(artifact_satisfies_requirement(artifact, str(key)) for artifact in evidence):
                continue
            rows.append({
                "kind": "phase_e",
                "id": str(key),
                "title": names.get(str(key), str(key)),
                "status": "missing",
            })
    for item in msp.get("register_items") or []:
        if not isinstance(item, dict):
            continue
        kind = str(item.get("kind") or "")
        if kind not in {"issue", "dependency"}:
            continue
        if str(item.get("status") or "").lower() in {"closed", "resolved", "done"}:
            continue
        rows.append({
            **item,
            "kind": kind,
            "title": str(item.get("title") or item.get("statement") or item.get("id") or kind),
        })
    return rows


def resolve_section(
    heading: str,
    template: dict[str, Any],
    category: str,
    subject: dict[str, Any],
    snapshot: dict[str, Any],
    outputs_dir: Path,
    programme_projects: dict[str, set[str]],
    register: dict[str, Any] | None = None,
    item_limit: int | None = 12,
) -> dict[str, Any]:
    sources = template.get("sources") or {}
    declared = sources.get(heading)
    # A declared source flagged `optional` names a projection the snapshot does not carry yet.
    # Its severity is ignored: the section's absence stays the section-level P2 the artefact has
    # always carried. Removing the flag — the flip the records session makes when the projection
    # lands — makes a declared `severity` apply, so the mandatory set fails at that severity.
    declared_severity = ""
    for source in declared or []:
        if source.get("optional"):
            continue
        declared_severity = max(
            declared_severity, str(source.get("severity") or ""),
            key=lambda value: SEVERITY_RANK.get(value, 0),
        )
    resolved: list[dict[str, Any]] = []
    missing_headings: set[str] = set()
    if declared:
        for source in declared:
            kind = source["kind"]
            if kind == "rows":
                rows = rows_for(snapshot, source, subject, category, subject.get("phase"))
                entry = {
                    "kind": "rows", "file": source.get("file", ""),
                    "pointer": source.get("pointer", ""), "count": len(rows),
                    "items": [row_summary(row) for row in rows[:item_limit]],
                }
                if item_limit is None:
                    # The publisher authors a document from the whole record; the pack page keeps
                    # its bounded excerpt.
                    entry["rows"] = rows
                resolved.append(entry)
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
                    missing_headings.add(heading)
                resolved.append(document_source)
            elif kind == "computed":
                rows = computed_rows(
                    str(source.get("name")), source, snapshot, subject, programme_projects,
                    category, subject.get("phase"), register=register,
                )
                entry = {
                    "kind": "computed", "name": str(source.get("name")),
                    "count": len(rows), "items": [row_summary(row) for row in rows[:item_limit]],
                }
                if item_limit is None:
                    entry["rows"] = rows
                resolved.append(entry)
            elif kind == "register":
                resolved.append({"kind": "register", "count": 0, "items": []})
            elif kind == "absence":
                # A declared explicit absence names the record that would satisfy the
                # heading. It is rendered as an absence, never counted as evidence: the
                # section stays absent until the record exists.
                resolved.append({
                    "kind": "absence",
                    "record": str(source.get("record") or ""),
                    "count": 0,
                    "items": [],
                })

    # A document-backed artefact falls back to its published document only when the heading
    # declares no source at all. A declared source that resolves to zero rows is a record
    # gap: falling back to the artefact's own document would present the document (or its
    # skeleton) as the heading's evidence, which is how the business case's four headings
    # and every compliance review's Waivers heading passed while carrying no facts.
    if template.get("document_category") and not declared:
        document_source, heading_found = resolve_documents(
            heading, template, subject, snapshot, outputs_dir, programme_projects,
        )
        if document_source["count"] > 0 or document_source.get("items"):
            resolved.append(document_source)
        if not heading_found:
            missing_headings.add(heading)

    evidence = [source for source in resolved if source["count"] > 0]
    status = "evidenced" if evidence else ("partial" if missing_headings else "absent")
    # An absence flagged `fallback` is declared for a heading whose real source may not
    # exist yet: it renders as an explicit absence only when the heading has no facts, and
    # yields to the row the moment the record lands. A plain absence always renders.
    fallback_records = [
        str(source.get("record") or "")
        for source in declared or []
        if source.get("kind") == "absence" and source.get("fallback")
    ]
    # Why the section could not resolve decides the package kind (R11): a heading the template
    # sources but whose rows/documents are absent is a RECORD gap; a heading the template declares
    # required yet gives no source (and no document fallback) is a STANDARD defect — it can never
    # resolve however many rows exist.
    cause = "record" if (declared or template.get("document_category")) else "standard"
    return {
        "heading": heading,
        "status": status,
        "severity": declared_severity,
        "cause": cause,
        "sources": resolved,
        "missing": [] if evidence else (
            [f"Explicit absence — {record}" for record in fallback_records]
            if fallback_records else [
                f"No row or document satisfies '{heading}'"
                + ("; a document exists but lacks the heading" if missing_headings else "")
            ]
        ),
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
        # A phase document is the record of that phase, not a copy in every other phase. The
        # E/F/H phase records are the architecture track's (owner tailoring 2026-11-24); the
        # Phase-G review is every delivery project's gate.
        if category != f"phase-{subject['phase']}":
            return False
        return subject["phase"] not in {"e", "f", "h"} or architecture_track(subject)
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
        if category in ARCHITECTURE_TRACK_CATEGORIES and not architecture_track(subject):
            return False
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
    register: dict[str, Any] | None = None,
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
                sections: list[dict[str, Any]] = []
                document_missing = False
                for heading in required:
                    section = resolve_section(
                        heading, template, category, subject, snapshot, outputs_dir,
                        programme_projects, register=register,
                    )
                    section["required"] = True
                    sections.append(section)
                for heading in template.get("optional") or []:
                    if heading in (template.get("sources") or {}):
                        section = resolve_section(
                            heading, template, category, subject, snapshot, outputs_dir,
                            programme_projects, register=register,
                        )
                        section["required"] = False
                        sections.append(section)
                # A programme-level artefact is a published document, not merely the rows behind
                # it (SPEC_V2 §8, plan v6 bar 2: "a typed row AND a published document"). Row
                # evidence must not make an artefact green while no document exists. Scoped to
                # programme-subject artefacts: per-project and typed instances are the tranche
                # batches' work and keep their section-level status.
                if (str(template.get("level")) == "programme"
                        and subject["kind"] == "programme"
                        and template.get("document_category")):
                    category_docs = documents_for(
                        snapshot, subject, str(template["document_category"]), "programme",
                        programme_projects,
                    )
                    if not category_docs:
                        document_missing = True
                        sections.append({
                            "heading": "Published document",
                            "status": "absent",
                            "severity": "",
                            "sources": [{
                                "kind": "documents", "file": "artefacts.json",
                                "pointer": "/documents",
                                "category": str(template["document_category"]),
                                "scope": "programme", "count": 0, "items": [],
                            }],
                            "required": True,
                            "missing": [
                                f"No published {template['document_category']} document "
                                "for the programme"
                            ],
                        })
                # The status counts every section the page carries, exactly as
                # `check-standards.cjs` recomputes it; a section is not dropped from the
                # artefact's honesty because it was declared optional.
                evidenced = [section for section in sections if section["status"] == "evidenced"]
                if document_missing:
                    status = "absent"
                elif len(evidenced) == len(sections):
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
    standard_defect_categories: set[str] | None = None,
) -> list[dict[str, Any]]:
    covered_keys = covered_keys or set()
    standard_defect_categories = standard_defect_categories or set()
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
        if artefact["category"] in standard_defect_categories:
            # A source this category declares cannot resolve, so the STANDARD defect finding
            # owns the category (one per broken source). A record package here would queue work
            # no row or document can discharge until the pointer or the projection lands.
            continue
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
        # A source may declare the severity its absence carries. The standard declares it, not
        # the generator: the SAFe mandatory set is P1 because its entries say so, and an
        # unmarked partial artefact keeps the section-level P2 it has always had.
        declared = max(
            (
                str(section.get("severity") or "")
                for section in artefact["sections"]
                if section.get("required", True)
                and section["status"] != "evidenced"
            ),
            key=lambda value: SEVERITY_RANK.get(value, 0),
            default="",
        )
        if SEVERITY_RANK.get(declared, 0) > SEVERITY_RANK[severity]:
            severity = declared
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
            # The package kind (R11): a required heading the template never sources is a STANDARD
            # defect and must be fixed before any row can satisfy it; otherwise the missing rows
            # or documents are the RECORD work.
            "kind": "STANDARD"
            if any(
                section["status"] != "evidenced"
                and section.get("required", True)
                and section.get("cause") == "standard"
                for section in artefact["sections"]
            )
            else "RECORD",
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


def pointer_defect_findings(
    defects: list[dict[str, Any]], std: dict[str, Any], snapshot: dict[str, Any]
) -> list[dict[str, Any]]:
    """One STANDARD finding per declared source that cannot resolve.

    The pointer is the standard's own defect: no row or published document can satisfy the
    heading until the source is corrected or projected, so the package must be STANDARD and the
    artefact must stay partial or absent. The finding names the category, the heading and the
    unresolvable source so the fix is a pointer edit, not a record hunt.
    """
    manifest = snapshot.get("manifest") or {}
    sha = str(manifest.get("git_sha") or "")
    if not re.fullmatch(r"[0-9a-f]{40}", sha):
        sha = "0" * 40
    findings: list[dict[str, Any]] = []
    used: set[str] = set()
    for defect in defects:
        template = (std.get("templates") or {}).get(defect["category"]) or {}
        base = "std-source-" + "-".join(
            slug(part) for part in (defect["category"], defect["heading"], defect["source"])
        )
        identifier = base
        duplicate = 2
        while identifier in used:
            identifier = f"{base}-{duplicate}"
            duplicate += 1
        used.add(identifier)
        findings.append({
            "id": identifier,
            "area": defect["category"],
            "category": defect["category"],
            "heading": defect["heading"],
            "source": defect["source"],
            "reason": defect["reason"],
            "standard": str(template.get("standard") or ""),
            "spec": str(template.get("spec") or ""),
            "severity": "P1",
            "kind": "STANDARD",
            "title": (
                f"{template.get('standard') or 'STANDARD'} artefact standard: "
                f"{defect['category']} / {defect['heading']} names an unresolvable source"
            ),
            "problem": (
                f"The {defect['category']} template's {defect['heading']!r} section reads "
                f"{defect['source']}, which cannot resolve against the snapshot: "
                f"{defect['reason']}. The section therefore renders as an absent record, and no "
                "row or document can evidence it until the standard's source is corrected."
            ),
            "evidence": [{
                "source": "standard",
                "path": "seed/output-standard.json",
                "category": defect["category"],
                "heading": defect["heading"],
                "snapshot": [defect["file"], defect["pointer"]],
            }],
            "failure_scenario": (
                f"The {defect['category']} artefact publishes or reports "
                f"{defect['heading']!r} as missing rows while the source pointer itself is "
                "broken, so record work is queued for a defect no record can discharge."
            ),
            "acceptance": (
                f"Correct seed/output-standard.json's {defect['category']} / "
                f"{defect['heading']} source {defect['source']}, or project the collection it "
                "names; `python3 scripts/generate-standards.py --check` must then report no "
                "finding for it."
            ),
            "owner_project": "PLATFORM",
            "audited_sha": sha,
        })
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
            "generate-standards.py resolves every declared source and every section of every "
            "applicable standards artefact against the published snapshot, its documents and "
            "the artefact standard; an unresolvable source or an unresolved mandatory section "
            "becomes a finding."
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
    defects = pointer_defects(std, snapshot)
    defect_categories = {defect["category"] for defect in defects}
    instances, _ = build_instances(std, snapshot, outputs_dir, declared_phases, register=register)
    # A source that cannot resolve is a standard defect: the artefact that depends on it stays
    # partial or absent, never evidenced by its other sources.
    for artefact in instances:
        if artefact["category"] in defect_categories and artefact["status"] == "evidenced":
            artefact["status"] = "partial"
    findings = build_findings(instances, snapshot, covered_keys, defect_categories)
    findings.extend(pointer_defect_findings(defects, std, snapshot))
    findings.sort(key=lambda item: (item["severity"], item["id"]))
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


def self_test_fixture() -> dict[str, Any]:
    return {
        "manifest": {"generated_at": "2026-11-20T00:00:00Z", "git_sha": "0" * 40},
        "programme": {
            "programme": [{"id": "pg1", "name": "fixture programme", "status": "active",
                           "shared_config": {}}],
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
            "dependencies": {"edges": [], "counts": {}, "untyped": 0,
                             "unknown_targets": [], "cycles": []},
            "demo_credits": [],
            "lessons": [],
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
            {"id": "d4", "category": "inspect-adapt", "project_id": "p1", "publishable": True,
             "created_at": "2026-11-20T00:00:00Z", "title": "Retro"},
            {"id": "d5", "category": "system-demo", "project_id": "p1", "publishable": True,
             "created_at": "2026-11-20T00:00:00Z", "title": "Demo"},
            {"id": "d6", "category": "pi-plan", "project_id": "p1", "publishable": True,
             "created_at": "2026-11-20T00:00:00Z", "title": "PI plan"},
            {"id": "d7", "category": "pi-objectives", "project_id": "p1", "publishable": True,
             "created_at": "2026-11-20T00:00:00Z", "title": "PI objectives"},
        ]},
    }


def self_test_outputs(outputs: Path) -> None:
    (outputs / "output-d1.html").write_text(
        "<h1>Vision</h1><h2>Purpose</h2><h2>Outcomes</h2><h2>Boundary</h2><h2>Who it serves</h2>",
        encoding="utf-8",
    )
    (outputs / "output-d2.html").write_text(
        "<h1>Closure</h1><h2>Outcome vs acceptance</h2><h2>Evidence</h2>",
        encoding="utf-8",
    )
    (outputs / "output-d4.html").write_text(
        "<h1>Retro</h1><h2>What went well</h2><h2>What did not</h2>",
        encoding="utf-8",
    )
    (outputs / "output-d5.html").write_text(
        "<h1>Demo</h1><h2>Findings</h2>",
        encoding="utf-8",
    )
    (outputs / "output-d6.html").write_text(
        "<h1>PI plan</h1><h2>Objectives</h2>",
        encoding="utf-8",
    )
    (outputs / "output-d7.html").write_text(
        "<h1>PI objectives</h1><h2>Objectives</h2>",
        encoding="utf-8",
    )


def mandatory_safe_standard(std: dict[str, Any]) -> dict[str, Any]:
    """The standard with the records session's exact SAFe entries applied, in memory.

    The committed `seed/output-standard.json` is the records session's file. This variant is the
    contract the generator must honour once they write it: the computed sources are named, the
    dependency and refinement headings are required, and the `optional` flags on the demo-credit
    and lesson sources are removed.
    """
    variant = copy.deepcopy(std)
    safe_art = variant["templates"]["safe-art"]
    if "Shared configuration" not in safe_art["required"]:
        safe_art["required"].append("Shared configuration")
    safe_art.setdefault("sources", {}).update({
        "Shared configuration": [
            {"kind": "computed", "name": "shared_config", "severity": "P1"}
        ],
        "Backlog with WSJF": [
            {"kind": "computed", "name": "wsjf_order", "severity": "P1",
             "match": {"status": "accepted"}}
        ],
    })
    plan = variant["templates"]["pi-plan"]
    plan["optional"] = [heading for heading in plan.get("optional") or [] if heading != "Dependencies"]
    for heading in ("Dependencies", "Backlog refinement"):
        if heading not in plan["required"]:
            plan["required"].append(heading)
    plan.setdefault("sources", {}).update({
        "WSJF order": [
            {"kind": "computed", "name": "wsjf_order", "severity": "P1",
             "match": {"status": "accepted"}}
        ],
        "Dependencies": [{"kind": "computed", "name": "dependency_graph", "severity": "P1"}],
        "Backlog refinement": [
            {"kind": "computed", "name": "refinement_cadence", "severity": "P1"}
        ],
    })
    for category in ("system-demo", "pi-objectives"):
        for sources in (variant["templates"][category].get("sources") or {}).values():
            for source in sources:
                if source.get("pointer") == "/demo_credits":
                    source.pop("optional", None)
                    source["severity"] = "P1"
    retro = variant["templates"]["inspect-adapt"]
    for sources in (retro.get("sources") or {}).values():
        for source in sources:
            if source.get("pointer") == "/lessons":
                source.pop("optional", None)
                source["severity"] = "P1"
    if "Kata candidates" not in retro["required"]:
        retro["required"].append("Kata candidates")
    retro.setdefault("sources", {})["Kata candidates"] = [
        {"kind": "computed", "name": "kata_candidates", "severity": "P1"}
    ]
    return variant


def safe_evidence_fixture() -> dict[str, Any]:
    """The fixture with every SAFe source present and valid: the `evidenced` side of the contract."""
    fixture = self_test_fixture()
    fixture["programme"]["programme"][0]["shared_config"] = {
        "values": {"cadence": "two-week"}, "locked": ["cadence"],
    }
    fixture["msp"]["project_dependencies"] = [{
        "id": "dep1", "programme_id": "pg1", "from_project": "P0", "to_project": "T1",
        "kind": "needs", "status": "active", "meta": {},
    }]
    fixture["msp"]["cadences"] = [
        {"id": "c-pi", "programme_id": "pg1", "kind": "pi",
         "period": "tranche-1", "state": "planned", "meta": {}},
        {"id": "c-demo", "programme_id": "pg1", "kind": "demo",
         "period": "tranche-1", "state": "planned", "meta": {}},
        {"id": "c-retro", "programme_id": "pg1", "kind": "retro",
         "period": "tranche-1", "state": "planned", "meta": {}},
        {"id": "c-ref", "programme_id": "pg1", "kind": "refinement",
         "period": "tranche-1", "state": "planned", "meta": {}},
    ]
    fixture["msp"]["wbs_nodes"] = [{
        "id": "w1", "project_id": "p1", "kind": "deliverable", "position": 1,
        "title": "Deliverable A", "status": "accepted", "business_value": 8,
        "time_criticality": 5, "risk_reduction": 3, "job_size": 2,
    }]
    fixture["msp"]["demo_credits"] = [{
        "id": "dc1", "cadence_id": "c-demo", "deliverable_id": "w1",
        "evidence": [{"kind": "acceptance", "ref": "run-1"}],
    }]
    fixture["msp"]["lessons"] = [
        {"id": "l1", "programme_id": "pg1", "cadence_id": "c-retro", "finding_key": "f1",
         "statement": "name the check first", "evidence": [], "state": "reviewed",
         "source": "retro", "meta": {"kata_candidate": {"signature": "sig-1",
                                                        "title": "instrument-first"}}},
        {"id": "l2", "programme_id": "pg1", "cadence_id": "c-retro", "finding_key": "f2",
         "statement": "a second lesson", "evidence": [], "state": "proposed",
         "source": "retro", "meta": {}},
    ]
    fixture["work"]["approvals"] = [{
        "id": "ap1", "project_id": "p1", "gate": "tranche_review",
        "status": "approved", "created_at": "2026-11-20T00:00:00Z",
    }]
    return fixture


def finding_for(findings: dict[str, Any], category: str) -> dict[str, Any] | None:
    return next((row for row in findings["findings"] if row["category"] == category), None)


def self_test_safe_sources_absent(outputs: Path, std: dict[str, Any]) -> None:
    """Absent or malformed derived sources produce the category's P1 finding."""
    fixture = safe_evidence_fixture()
    fixture["programme"]["programme"][0]["shared_config"] = {}
    fixture["msp"]["project_dependencies"] = []
    fixture["msp"]["cadences"] = [
        row for row in fixture["msp"]["cadences"] if row["kind"] != "refinement"
    ]
    fixture["msp"]["demo_credits"] = []
    for lesson in fixture["msp"]["lessons"]:
        lesson["meta"] = {}
    _, findings = generate(mandatory_safe_standard(std), fixture, outputs)
    expected = {
        "safe-art": ["Shared configuration"],
        "pi-plan": ["Dependencies", "Backlog refinement"],
        "system-demo": ["Evidence", "Credits"],
        "inspect-adapt": ["Kata candidates"],
        "pi-objectives": ["Measurement", "Evidence"],
    }
    for category, headings in expected.items():
        finding = finding_for(findings, category)
        assert finding is not None, f"{category}: an absent derived source must be a finding"
        assert finding["severity"] == "P1", f"{category}: expected P1, got {finding['severity']}"
        for heading in headings:
            assert heading in finding["missing"], f"{category}: {heading} not in {finding['missing']}"
    # A malformed shared_config (an array where the migration requires an object) is absence too.
    malformed = safe_evidence_fixture()
    malformed["programme"]["programme"][0]["shared_config"] = {"values": ["not", "an", "object"]}
    _, findings = generate(mandatory_safe_standard(std), malformed, outputs)
    finding = finding_for(findings, "safe-art")
    assert finding is not None and "Shared configuration" in finding["missing"], finding
    print("generate-standards: self-test case safe-derived-sources-absent-is-a-finding OK")


def self_test_safe_sources_evidenced(outputs: Path, std: dict[str, Any]) -> None:
    """Each derived source passes on a present, valid record: its section is evidenced.

    The artefact itself also needs its published document (the programme-level artefact gate),
    which this fixture does not write; the assertion is therefore on the derived section and on
    the finding never naming it.
    """
    fixture = safe_evidence_fixture()
    document, findings = generate(mandatory_safe_standard(std), fixture, outputs)
    by_id = {artefact["id"]: artefact for artefact in document["artefacts"]}
    derived = {
        "safe-art": ["Shared configuration"],
        "pi-plan": ["Dependencies", "Backlog refinement"],
        "system-demo": ["Evidence", "Credits"],
        "inspect-adapt": ["Kata candidates"],
        "pi-objectives": ["Measurement", "Evidence"],
    }
    for category, headings in derived.items():
        artefact = by_id[f"std-safe-{category}-pg1"]
        status = {section["heading"]: section["status"] for section in artefact["sections"]}
        for heading in headings:
            assert status.get(heading) == "evidenced", (
                f"{category}: {heading} expected evidenced, got {status.get(heading)}"
            )
        finding = finding_for(findings, category)
        assert finding is None or not (set(finding["missing"]) & set(headings)), (
            f"{category}: a present derived source must not be missing: {finding and finding['missing']}"
        )
    # A flat object is read as values with no locks (migration 0068), so it is evidence too.
    flat = safe_evidence_fixture()
    flat["programme"]["programme"][0]["shared_config"] = {"cadence": "two-week"}
    _, findings = generate(mandatory_safe_standard(std), flat, outputs)
    finding = finding_for(findings, "safe-art")
    assert finding is None or "Shared configuration" not in finding["missing"], (
        "a flat shared_config is a real configuration"
    )
    print("generate-standards: self-test case safe-derived-sources-evidenced-passes OK")


def self_test_wsjf(outputs: Path, std: dict[str, Any]) -> None:
    """Malformed or unfiltered WSJF candidates are findings; valid candidates are ordered."""
    cases = {
        "missing business_value": lambda node: node.pop("business_value"),
        "job_size zero": lambda node: node.update({"job_size": 0}),
        "negative time_criticality": lambda node: node.update({"time_criticality": -1}),
        "boolean is not an integer": lambda node: node.update({"business_value": True}),
        "float is not an integer": lambda node: node.update({"risk_reduction": 3.0}),
    }
    for label, mutate in cases.items():
        fixture = safe_evidence_fixture()
        mutate(fixture["msp"]["wbs_nodes"][0])
        _, findings = generate(mandatory_safe_standard(std), fixture, outputs)
        finding = finding_for(findings, "pi-plan")
        assert finding is not None, f"{label}: a malformed WSJF row must be a finding"
        assert finding["severity"] == "P1", f"{label}: expected P1, got {finding['severity']}"
        assert "WSJF order" in finding["missing"], f"{label}: {finding['missing']}"
        art = finding_for(findings, "safe-art")
        assert art is not None and "Backlog with WSJF" in art["missing"], (
            f"{label}: the backlog-with-WSJF section must fail on the same row"
        )
    # An unfiltered `/wbs_nodes` read is itself the defect; the filtered read validates.
    fixture = safe_evidence_fixture()
    subject = {"id": "p1", "key": "P0", "programme_id": "pg1",
               "project_id": "p1", "project_key": "P0", "phase": ""}
    source = {"kind": "rows", "file": "msp.json", "pointer": "/wbs_nodes", "validate": "wsjf"}
    assert rows_for(fixture, source, subject, "pi-plan", None) == [], "an unfiltered WSJF read must not pass"
    source["match"] = {"status": "accepted"}
    assert len(rows_for(fixture, source, subject, "pi-plan", None)) == 1, "a filtered read resolves"
    # The order is the computed score, descending; the inputs are never rewritten.
    fixture["msp"]["wbs_nodes"].append({
        "id": "w2", "project_id": "p1", "kind": "deliverable", "position": 2,
        "title": "Deliverable B", "status": "accepted", "business_value": 1,
        "time_criticality": 1, "risk_reduction": 1, "job_size": 1,
    })
    ordered = computed_rows("wsjf_order", {"name": "wsjf_order"}, fixture, subject, {}, "pi-plan", None)
    assert [row["id"] for row in ordered] == ["w1", "w2"], ordered
    assert [row["rank"] for row in ordered] == [1, 2], ordered
    assert [row["wsjf"] for row in ordered] == [8.0, 3.0], ordered
    print("generate-standards: self-test case wsjf-malformed-inputs-is-a-finding OK")


def self_test_demo_and_lessons_mandatory(outputs: Path, std: dict[str, Any]) -> None:
    """Demo-credit and lesson absence is a P2 section gap while optional, P1 once mandatory."""
    # The committed standard is already flipped (the records session's edits), so the P2 half
    # restores the optional flag explicitly and the P1 half reads the standard as committed.
    optional_std = copy.deepcopy(std)
    for category in ("system-demo", "pi-objectives", "inspect-adapt"):
        for sources in (optional_std["templates"][category].get("sources") or {}).values():
            for source in sources:
                if source.get("pointer") in {"/demo_credits", "/lessons"}:
                    source["optional"] = True
                    source.pop("projection", None)
    # The flip also made "Kata candidates" a required heading; the pre-flip shape has neither the
    # heading nor its computed source, so the P2 half removes both.
    retro = optional_std["templates"]["inspect-adapt"]
    if "Kata candidates" in retro["required"]:
        retro["required"].remove("Kata candidates")
    (retro.get("sources") or {}).pop("Kata candidates", None)
    for category, key, headings in (
        ("system-demo", "demo_credits", ["Evidence", "Credits"]),
        ("inspect-adapt", "lessons", ["Lessons", "Actions"]),
    ):
        fixture = safe_evidence_fixture()
        fixture["msp"][key] = []
        document, findings = generate(optional_std, fixture, outputs)
        by_id = {artefact["id"]: artefact for artefact in document["artefacts"]}
        artefact = by_id[f"std-safe-{category}-pg1"]
        assert artefact["status"] == "partial", (
            f"{category}: the page must state the absence honestly, got {artefact['status']}"
        )
        optional = finding_for(findings, category)
        assert optional is not None and optional["severity"] == "P2", (
            f"{category}: an `optional` source is a section-level P2, got {optional and optional['severity']}"
        )
        _, findings = generate(mandatory_safe_standard(std), fixture, outputs)
        finding = finding_for(findings, category)
        assert finding is not None, f"{category}: absence must fail once mandatory"
        assert finding["severity"] == "P1", f"{category}: expected P1, got {finding['severity']}"
        for heading in headings:
            assert heading in finding["missing"], f"{category}: {heading} not in {finding['missing']}"
    print("generate-standards: self-test case demo-and-lesson-absence-fails-once-mandatory OK")


def self_test_gap_analysis(outputs: Path, std: dict[str, Any]) -> None:
    """Open rework, phase-E gaps and msp register items are one computed gap list."""
    fixture = safe_evidence_fixture()
    subject = {"id": "pg1", "key": "pg1", "kind": "programme", "phase": ""}
    register = {"packages": [
        {"key": "v1-x-1:T0", "status": "open", "severity": "P1",
         "title": "S04 has no closure review"},
        {"key": "v1-x-2:T1", "status": "suppressed", "severity": "P1",
         "title": "waived by the conformance owner"},
    ]}
    fixture["msp"]["register_items"] = [
        {"id": "ri1", "kind": "issue", "status": "open",
         "title": "The supplier SLA is unsigned"},
        {"id": "ri2", "kind": "dependency", "status": "resolved",
         "title": "The uplink is commissioned"},
        {"id": "ri3", "kind": "risk", "status": "open", "title": "not a gap kind"},
    ]
    profile = fixture["architecture"]["profiles"][0]
    profile["artifact_requirements"]["e"] = {"psa": ["solution_candidate"]}
    fixture["architecture"]["standard"].append({
        "key": "solution_candidate", "name": "Solution Candidate",
        "phase_code": "e", "plateau": "psa",
    })

    rows = computed_rows(
        "gap-analysis", {"name": "gap-analysis"}, fixture, subject, {},
        "gap-analysis", None, register=register,
    )
    kinds = sorted(str(row.get("kind")) for row in rows)
    assert kinds == ["issue", "phase_e", "rework"], (
        f"the three gap kinds must resolve, got {rows}"
    )
    rework = next(row for row in rows if row.get("kind") == "rework")
    assert rework["id"] == "v1-x-1:T0" and rework["severity"] == "P1", rework

    # An evidenced phase-E type is not a gap; a closed or suppressed record is not one either.
    fixture["architecture"]["artifacts"].append({
        "id": "a9", "title": "Solution candidate", "artifact_kind": "solution_candidate",
        "phase_code": "e", "plateau": "psa", "status": "approved", "project_id": "p1",
    })
    rows = computed_rows(
        "gap-analysis", {"name": "gap-analysis"}, fixture, subject, {},
        "gap-analysis", None, register=register,
    )
    assert "phase_e" not in [row.get("kind") for row in rows], rows

    # No register and no register items: the section is absent, not an empty pass.
    assert computed_rows(
        "gap-analysis", {"name": "gap-analysis"}, self_test_fixture(), subject, {},
        "gap-analysis", None,
    ) == []
    print("generate-standards: self-test case gap-analysis-resolves-open-gaps OK")


def self_test_sources_declared(std: dict[str, Any]) -> None:
    """Every required heading names where its fact lives.

    A required heading with no source can never be evidenced, so its absence looks like a record
    gap forever. The standard must declare the source — rows, a computed source or the artefact's
    own published document — before a finding about that heading is honest.
    """
    problems: list[str] = []
    for category, template in sorted((std.get("templates") or {}).items()):
        sources = template.get("sources") or {}
        for heading in template.get("required") or []:
            if not sources.get(heading):
                problems.append(f"{category} / {heading}: no source")
    assert not problems, "; ".join(problems)


def self_test_section_cause(outputs: Path, fixture: dict[str, Any]) -> None:
    """The section's cause decides the package kind (R11): an unsourced heading is STANDARD,
    a sourced heading whose rows are absent is RECORD."""
    subject = {"kind": "programme", "id": "pg1", "title": "Programme", "project_key": ""}
    unsourced = {"required": ["Objectives"], "sources": {}}
    section = resolve_section(
        "Objectives", unsourced, "vision", subject, fixture, outputs, {}
    )
    assert section["status"] != "evidenced" and section["cause"] == "standard", section
    sourced = {
        "required": ["Objectives"],
        "sources": {"Objectives": [{"kind": "rows", "file": "msp.json",
                                     "pointer": "/does-not-exist"}]},
    }
    section = resolve_section(
        "Objectives", sourced, "vision", subject, fixture, outputs, {}
    )
    assert section["status"] != "evidenced" and section["cause"] == "record", section
    # A declared source resolving zero must not fall back to the artefact's own document: the
    # business case's four headings and every compliance review's Waivers heading were resolved
    # by that fallback while carrying no facts.
    own = {
        "subject_kind": "project", "document_category": "vision", "required": ["Purpose"],
        "sources": {"Purpose": [{"kind": "rows", "file": "msp.json",
                                     "pointer": "/register_items"}]},
    }
    project = {"kind": "project", "id": "p1", "title": "P1", "project_key": "P1",
               "status": "active", "meta": {}, "project_id": "p1"}
    section = resolve_section(
        "Purpose", own, "vision", project, fixture, outputs, {"pg1": {"p1"}}
    )
    assert all(source["kind"] != "documents" for source in section["sources"]), section
    assert section["status"] != "evidenced", section
    print("generate-standards: self-test case section-cause-decides-kind OK")


def self_test_broken_source_finding(outputs: Path, std: dict[str, Any]) -> None:
    """A source that cannot resolve is a STANDARD finding, and `--check` refuses it.

    The broken source is added beside the heading's working document source, so the section
    alone would still be evidenced: the artefact must not stay evidenced while one of its
    declared sources is broken.
    """
    broken = copy.deepcopy(std)
    broken["templates"]["vision"]["sources"]["Purpose"].append({
        "kind": "rows", "file": "msp.json", "pointer": "/does-not-exist",
    })
    fixture = self_test_fixture()
    defects = pointer_defects(broken, fixture)
    assert any(
        defect["category"] == "vision" and defect["heading"] == "Purpose"
        for defect in defects
    ), defects
    document, findings = generate(broken, fixture, outputs)
    named = [
        row for row in findings["findings"]
        if row.get("category") == "vision" and row.get("heading") == "Purpose"
    ]
    assert len(named) == 1, f"one finding per defect, got {named}"
    finding = named[0]
    assert finding["kind"] == "STANDARD", finding
    assert finding["severity"] == "P1", finding
    assert finding["source"] == "msp.json#/does-not-exist", finding
    assert "has no key" in finding["reason"], finding
    by_id = {artefact["id"]: artefact for artefact in document["artefacts"]}
    vision = by_id.get("std-msp-vision-p0") or by_id.get("std-msp-vision-pg1")
    assert vision is not None and vision["status"] == "partial", (
        f"a broken source must keep the artefact out of evidenced, got {vision and vision['status']}"
    )
    # `--check` is the gate: with a current findings file the defect must still fail it.
    with tempfile.TemporaryDirectory() as tmp:
        snapshot_dir = Path(tmp) / "data"
        snapshot_dir.mkdir()
        findings_path = Path(tmp) / "standards-findings.json"
        _, committed = generate(broken, fixture, outputs)
        findings_path.write_text(
            json.dumps(committed, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
            encoding="utf-8",
        )
        stderr = io.StringIO()
        with contextlib.redirect_stderr(stderr):
            code = check(
                broken, fixture, snapshot_dir, outputs, set(), findings_path=findings_path,
            )
        assert code == 1, f"--check must fail on a broken source, got {code}"
        assert "vision / Purpose" in stderr.getvalue(), stderr.getvalue()
    print("generate-standards: self-test case broken-source-finding OK")


def self_test_computed_sources(fixture: dict[str, Any]) -> None:
    """The sources the W4 STANDARD packages depend on resolve from the record they name.

    Each case copies the fixture so one case's rows cannot mask another's absence.
    """
    snap = json.loads(json.dumps(fixture))
    snap["actions"] = {"actions": ["card.get", "board.list"]}
    rows = computed_rows(
        "action_registry", {}, snap, {"kind": "programme", "id": "pg1"}, {}
    )
    assert [row["title"] for row in rows] == ["card.get", "board.list"], rows

    snap = json.loads(json.dumps(fixture))
    snap["msp"]["register_items"] = [{
        "id": "c1", "kind": "change", "title": "Change one", "impact": None,
        "meta": {"impact": {"scope": "one", "cost": "none"}},
    }]
    rows = computed_rows(
        "change_impact", {"match": {"kind": "change"}}, snap,
        {"kind": "project", "id": "p1"}, {},
    )
    assert rows and rows[0]["impact"] == {"scope": "one", "cost": "none"}, rows

    snap = json.loads(json.dumps(fixture))
    snap["programme"]["projects"].append(
        {"id": "p3", "key": "T9", "programme_id": "pg1", "status": "archived"}
    )
    rows = computed_rows(
        "programme_summary", {}, snap,
        {"kind": "programme", "id": "pg1", "programme_id": "pg1"}, {},
    )
    summary = rows[0]
    assert summary["archived"] == 1, summary
    assert summary["projects"] == sum(summary["status_counts"].values()) == 3, summary

    snap = json.loads(json.dumps(fixture))
    snap["artefacts"]["governed_assets"] = [{
        "id": "ch1", "kind": "charter", "project_id": "p1", "owner_user_id": "u1",
        "created_at": "2026-11-20T00:00:00Z", "title": "Charter",
    }]
    rows = computed_rows(
        "programme_charter", {}, snap,
        {"kind": "programme", "id": "pg1", "programme_id": "pg1"}, {},
    )
    assert rows and rows[0]["owner_user_id"] == "u1", rows
    print("generate-standards: self-test case computed-sources OK")


def self_test() -> int:
    fixture = self_test_fixture()
    std = json.loads(STANDARD_PATH.read_text(encoding="utf-8"))
    self_test_sources_declared(std)
    self_test_computed_sources(fixture)
    with tempfile.TemporaryDirectory() as tmp:
        outputs = Path(tmp)
        self_test_outputs(outputs)
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
        strategies = [artefact for artefact in document["artefacts"]
                      if artefact["id"] == "std-msp-programme-strategies-pg1"]
        assert strategies and strategies[0]["status"] == "absent", (
            "row evidence alone is not the artefact: its published document is absent"
        )
        assert any(f["category"] == "programme-strategies" for f in findings["findings"]), (
            "an absent artefact must be a finding"
        )
        refined = [artefact for artefact in document["artefacts"]
                   if artefact["id"] == "std-msp-refined-vision-pg1"]
        assert refined and refined[0]["status"] == "absent", "the refined vision has no record"
        assert any(f["category"] == "refined-vision" for f in findings["findings"]), (
            "an absent artefact must be a finding"
        )
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
        self_test_safe_sources_absent(outputs, std)
        self_test_safe_sources_evidenced(outputs, std)
        self_test_wsjf(outputs, std)
        self_test_demo_and_lessons_mandatory(outputs, std)
        self_test_gap_analysis(outputs, std)
        self_test_section_cause(outputs, fixture)
        self_test_broken_source_finding(outputs, std)
    print(f"generate-standards: self-test OK ({len(document['artefacts'])} artefact instances)")
    return 0


# --- per-category verification ---------------------------------------------------------------

COMPOSE_PATH = ROOT / "scripts" / "compose-artefact-bodies.py"
MD_HEADING = re.compile(r"^\s{0,3}#{1,6}\s+(.+?)\s*#*\s*$")


def load_composer():
    """The body composer, imported by path as `publish-programme-artefacts.py` imports us."""
    spec = importlib.util.spec_from_file_location("compose_artefact_bodies", COMPOSE_PATH)
    if spec is None or spec.loader is None:
        raise GenerationError(f"cannot load {COMPOSE_PATH}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def generator_api() -> types.SimpleNamespace:
    """The resolver surface `compose()` calls, so the verifier runs the one authority."""
    return types.SimpleNamespace(
        applies=applies,
        rows_for=rows_for,
        substitute=substitute,
        skeleton_bodies=skeleton_bodies,
        normalise=normalise,
        resolve_section=resolve_section,
    )


def snapshot_document_rows(snapshot: dict[str, Any]) -> list[dict[str, Any]]:
    """Every document row the snapshot carries, in either published shape."""
    rows: list[dict[str, Any]] = []
    artefacts = snapshot.get("artefacts") or {}
    for row in artefacts.get("documents") or []:
        if isinstance(row, dict):
            rows.append(row)
    documents_file = snapshot.get("documents") or {}
    for row in documents_file.get("documents") or []:
        if isinstance(row, dict):
            rows.append(row)
    return rows


def is_published(document: dict[str, Any]) -> bool:
    # #537 made publication a timestamp; the old flag is still read for older snapshots.
    if "published_at" in document:
        return bool(document.get("published_at"))
    return bool(document.get("publishable", True))


def document_text(document: dict[str, Any]) -> str | None:
    meta = document.get("meta") if isinstance(document.get("meta"), dict) else {}
    for value in (document.get("body"), meta.get("body")):
        if isinstance(value, str) and value.strip():
            return value
    return None


def text_headings(text: str) -> set[str]:
    if re.search(r"<h[1-6]\b", text, re.IGNORECASE):
        return {normalise(TAG.sub("", body)) for _level, body in HEADING.findall(text)}
    found: set[str] = set()
    for line in text.splitlines():
        match = MD_HEADING.match(line)
        if match:
            found.add(normalise(match.group(1)))
    return found


def document_heading_set(document: dict[str, Any]) -> set[str] | None:
    """The document body's headings, or None when the snapshot carries no body to read."""
    meta = document.get("meta") if isinstance(document.get("meta"), dict) else {}
    for value in (document.get("headings"), meta.get("headings")):
        if isinstance(value, list):
            return {normalise(str(item)) for item in value}
    text = document_text(document)
    if text is None:
        return None
    return text_headings(text)


def instance_documents(
    snapshot: dict[str, Any],
    template: dict[str, Any],
    category: str,
    subject: dict[str, Any],
) -> list[dict[str, Any]]:
    """The published documents that belong to one instance, newest last."""
    wanted = str(template.get("document_category") or category)
    kind = str(subject.get("kind") or "")
    allowed: set[str] | None = None
    if kind == "programme":
        allowed = set()
        for project in ((snapshot.get("programme") or {}).get("projects") or []):
            if (isinstance(project, dict)
                    and str(project.get("programme_id")) == str(subject.get("id"))):
                allowed.add(str(project.get("id")))
    elif kind == "project":
        allowed = {str(subject.get("id"))}
    selected: list[dict[str, Any]] = []
    for document in snapshot_document_rows(snapshot):
        if str(document.get("category") or "") != wanted or not is_published(document):
            continue
        if kind == "programme" and str(document.get("programme_id") or "") == str(subject.get("id")):
            selected.append(document)
            continue
        if allowed is not None and str(document.get("project_id") or "") not in allowed:
            continue
        selected.append(document)
    return sorted(
        selected,
        key=lambda item: (str(item.get("created_at") or ""), str(item.get("id") or "")),
    )


def check_instance(
    gen: types.SimpleNamespace,
    composer,
    std: dict[str, Any],
    template: dict[str, Any],
    category: str,
    instance: dict[str, Any],
    snapshot: dict[str, Any],
    snapshot_dir: Path,
) -> list[str]:
    subject = instance.get("subject") or {}
    where = str(instance.get("id") or "")
    documents = instance_documents(snapshot, template, category, subject)
    if not documents:
        wanted = str(template.get("document_category") or category)
        return [f"{where}: no published {wanted} document in the snapshot"]
    document = documents[-1]
    headings = document_heading_set(document)
    if headings is None:
        return [
            f"{where}: document {document.get('id')} carries no body or headings; "
            "required-heading coverage cannot be confirmed"
        ]
    missing = [
        str(heading) for heading in (template.get("required") or [])
        if normalise(str(heading)) not in headings
    ]
    if missing:
        return [
            f"{where}: document {document.get('id')} misses required heading(s): "
            f"{', '.join(missing)}"
        ]
    kind = str(subject.get("kind") or "")
    key = str(subject.get("key") or subject.get("id") or "")
    try:
        composed = composer.compose(gen, std, snapshot, snapshot_dir, category, kind, key)
    except (composer.Refusal, composer.Abort) as exc:
        return [f"{where}: composition refused ({exc.code}): {exc.message}"]
    meta = document.get("meta") if isinstance(document.get("meta"), dict) else {}
    recorded = meta.get("sha256")
    if not isinstance(recorded, str) or not recorded:
        recorded = document.get("sha256")
    if isinstance(recorded, str) and recorded and recorded != composed["body_sha256"]:
        return [
            f"{where}: stale document {document.get('id')}: composed body_sha256 "
            f"{composed['body_sha256']} differs from the recorded {recorded}"
        ]
    return []


def check_category(
    category: str,
    std: dict[str, Any],
    snapshot: dict[str, Any],
    snapshot_dir: Path,
    outputs_dir: Path,
) -> tuple[int, list[str], list[str]]:
    """One category's verdict: (exit code, result lines, problem lines). Reads only."""
    categories = list(std.get("categories") or [])
    templates = std.get("templates") or {}
    if category not in templates or (categories and category not in categories):
        return 2, [], [f"check-category {category}: unknown category"]
    try:
        instances, _ = build_instances(
            std, snapshot, outputs_dir, load_declared_phases(), register=None,
        )
    except GenerationError as exc:
        return 2, [], [f"check-category {category}: the record cannot be built: {exc}"]
    rows = [item for item in instances if item.get("category") == category]
    if not rows:
        if category in (std.get("on_demand") or []):
            return 0, [f"on-demand untriggered: {category}"], []
        return 2, [], [f"check-category {category}: no instance row or trigger exists"]
    try:
        composer = load_composer()
    except GenerationError as exc:
        return 2, [], [f"check-category {category}: {exc}"]
    gen = generator_api()
    template = templates[category]
    problems: list[str] = []
    for instance in rows:
        problems.extend(check_instance(
            gen, composer, std, template, category, instance, snapshot, snapshot_dir,
        ))
    if problems:
        return 1, [], [f"check-category {category}: {problem}" for problem in problems]
    return 0, [f"check-category {category}: ok ({len(rows)} instance(s))"], []


def check_categories(
    keys: list[str],
    std: dict[str, Any],
    snapshot_dir: Path,
    outputs_dir: Path,
) -> int:
    """Run the repeatable check; the process exits with the worst category verdict."""
    snapshot = load_snapshot(snapshot_dir)
    worst = 0
    for key in keys:
        code, lines, problems = check_category(key, std, snapshot, snapshot_dir, outputs_dir)
        for line in lines + problems:
            print(line)
        worst = max(worst, code)
    return worst


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
    parser.add_argument(
        "--check-category", action="append", default=[], metavar="KEY",
        help="verify one standard category (repeatable); exits 0 clean, 1 defect, 2 absent",
    )
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    std = json.loads(args.standard.read_text(encoding="utf-8"))
    if args.self_test:
        return self_test()
    if args.check_category:
        return check_categories(
            args.check_category, std, args.snapshot, args.outputs_dir,
        )
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
