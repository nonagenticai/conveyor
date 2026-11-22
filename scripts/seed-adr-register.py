#!/usr/bin/env python3
"""Seed the conveyor ADR register (``governed_asset`` rows of kind ``adr``).

The register has two source families and one row per source decision:

* ``docs/decisions/NNNN-slug.md`` — one ADR row per file, keyed by the filename stem
  (``0012-no-deferrals``).  The filename number is authoritative: ADR 0012's H1 still says
  ``# 0010`` (a documentation defect), and that must not collide with the real 0010.
* ``docs/SPEC_V2.md`` — one row per distinct ``D##`` decision, keyed ``spec-dNN``.  ``D106``
  and ``D115`` are agent-platform references, not conveyor decisions, and are excluded.  A
  decision an ADR records is *not* a second row: it is named in that ADR row's
  ``meta.spec_decision``.

Supersession is deliberately narrow.  ``0002`` says its editor selection was replaced by SPEC
``D6``; because the surviving record is the ADR itself, that is a ``meta.superseded_by`` note,
not a same-table ``superseded_by_id`` edge.  ``0005`` amends ``0004`` and ``0007`` amends
``0006``; those are amendment metadata, not status changes.  A ``D18`` supersession (by ``D19``)
*is* a same-table edge and is applied through ``decisions.transition`` after both rows are
approved — the domain allows only an approved asset to be superseded.

``--check`` is offline and is the default.  It prints the source counts and refuses duplicate
numbers/keys, missing title or status lines, conflicting dispositions, forged or forked edges,
and any source file with no row mapping.

``--apply`` speaks ``POST /api/actions/<name>/invocations``.  ``scripts/programme-record.py``
exposes a CLI/ssh transport, not an HTTP client, so this script carries its own minimal
``urllib`` client.  ``--local-bin PATH`` instead runs the branch-built binary's
``admin action invoke`` against the database its ``DATABASE_MIGRATE_URL`` names, recording
``--actor-id`` as the principal; stdin carries the action input and stdout its JSON output.
Credentials come from ``--url``/``--token``/``--app-key`` or
``CONVEYOR_URL``/``CONVEYOR_TOKEN``/``CONVEYOR_APP_KEY``.  An approval id is recorded in
``meta`` only when supplied; the ``decisions.*`` actions have no approval gate and this script
never invents one.  A row whose ``meta.content_sha256`` is unchanged is never updated.

``--self-test`` builds a fixture tree under a temporary directory, exercises the parser and
the refusal paths, and drives an in-memory action client twice to prove idempotency.  It never
touches the network.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
import uuid
from dataclasses import dataclass, field
from pathlib import Path

ADR_DIR = Path("docs/decisions")
SPEC_FILE = Path("docs/SPEC_V2.md")
REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_TENANT = "nonagentic"

# D106/D115 are the agent platform's KH concepts, cited as references rather than adopted as
# conveyor decisions.  Every other D## in SPEC_V2 must resolve to a disposition.
AGENT_PLATFORM_REFS = {106, 115}

# The two amendment edges and the one meta-only supersession the current ADR set declares.
# Later ADRs (0011, 0013, ...) may add edges of their own; the structural checks below still
# apply to them, but these three must not silently disappear.  Keyed by filename stem, so a
# fixture numbered 0002 is not mistaken for the real `0002-editor`.
EXPECTED_ADR_AMENDS = {
    "0005-absorption-sequence-and-repo-scope": {4},
    "0007-tool-estate-conveyor-and-coding": {6},
}
EXPECTED_SUPERSESSION_NOTES = {"0002-editor": 6}

STATUS_MAP = {
    "accepted": "approved",
    "approved": "approved",
    "proposed": "draft",
    "draft": "draft",
    "in review": "in_review",
    "superseded": "superseded",
    "archived": "archived",
}

ADR_NAME_RE = re.compile(r"^(\d{4})-([a-z0-9][a-z0-9.-]*)\.md$")
DREF_RE = re.compile(r"\bD(\d{1,3})\b")
DATE_RE = re.compile(r"\b(20\d{2}-\d{2}-\d{2})\b")
ADR_NUMBER_RE = re.compile(r"^(\d{4})\s*[—–-]\s*(.+)$")
STATUS_LINE_RE = re.compile(r"^Status:\s*(.+)$")
TABLE_ESCAPED_PIPE_RE = re.compile(r"(?<!\\)\|")


class SeedError(Exception):
    """A source or register error that refuses the check/apply."""


# ── source documents ─────────────────────────────────────────────────────────────────────


@dataclass
class SpecDecision:
    number: int
    title: str
    body: str
    section: str
    status: str
    disposition: str
    date: str = ""
    amended_by: list[int] = field(default_factory=list)
    superseded_by: int | None = None
    specific: bool = True


@dataclass
class Row:
    key: str
    title: str
    body: str
    status: str
    meta: dict
    source: str
    adr_number: int | None = None
    spec_decision: str | None = None
    superseded_by_key: str | None = None
    row_id: str | None = None
    version: int | None = None
    current_status: str | None = None

    def create_payload(self) -> dict:
        return {
            "key": self.key,
            "title": self.title,
            "body": self.body,
            "meta": self.meta,
        }

    def update_payload(self) -> dict:
        return {
            "id": self.row_id,
            "title": self.title,
            "body": self.body,
            "meta": self.meta,
        }


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def sha256_text(*parts: object) -> str:
    digest = hashlib.sha256()
    for part in parts:
        digest.update(str(part).encode("utf-8"))
        digest.update(b"\0")
    return digest.hexdigest()


def normalise_status(token: str) -> str:
    key = re.sub(r"[^a-z]+", " ", token.lower()).strip()
    try:
        return STATUS_MAP[key]
    except KeyError:
        raise SeedError(f"unknown status {token!r}") from None


def split_table_row(line: str) -> list[str] | None:
    stripped = line.strip()
    if not stripped.startswith("|") or not stripped.endswith("|"):
        return None
    body = stripped[1:-1]
    return [cell.replace("\\|", "|").strip() for cell in TABLE_ESCAPED_PIPE_RE.split(body)]


def is_table_separator(cells: list[str]) -> bool:
    return bool(cells) and all(re.fullmatch(r":?-{3,}:?", cell) for cell in cells)


def strip_markdown(text: str) -> str:
    return text.replace("**", "").replace("__", "").strip()


def last_date(text: str) -> str:
    matches = DATE_RE.findall(text)
    return matches[-1] if matches else ""


def expand_refs(cell: str) -> list[int]:
    refs: set[int] = set()
    for match in re.finditer(r"D(\d+)(?:\s*[–—-]\s*D(\d+))?", cell):
        first = int(match.group(1))
        second = int(match.group(2)) if match.group(2) else first
        if second < first:
            first, second = second, first
        refs.update(range(first, second + 1))
    return sorted(refs)


def adr_description(cell: str) -> str:
    explicit = {int(n): text.strip() for n, text in re.findall(r"D(\d+)\s*\(([^)]*)\)", cell)}
    if explicit:
        return ""
    match = re.search(r"\(([^)]*)\)", cell)
    return match.group(1).strip() if match else ""


# ── ADR parsing ─────────────────────────────────────────────────────────────────────────


def parse_adr(path: Path, root: Path) -> Row:
    match = ADR_NAME_RE.fullmatch(path.name)
    if not match:
        raise SeedError(
            f"{path.relative_to(root)}: source file has no row mapping "
            "(an ADR must be NNNN-slug.md)"
        )
    number = int(match.group(1))
    raw = path.read_bytes()
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as error:
        raise SeedError(f"{path.relative_to(root)}: not UTF-8: {error}") from None
    lines = text.splitlines()
    if not lines:
        raise SeedError(f"{path.relative_to(root)}: empty file (missing title line)")

    heading_match = re.match(r"^#\s+(.+)$", lines[0].strip())
    if not heading_match:
        raise SeedError(f"{path.relative_to(root)}: missing title line (expected '# NNNN — title')")
    heading = heading_match.group(1).strip()
    numbered = ADR_NUMBER_RE.match(heading)
    if numbered:
        title_number = int(numbered.group(1))
        title = numbered.group(2).strip()
    else:
        title_number = None
        title = heading
    if not title:
        raise SeedError(f"{path.relative_to(root)}: missing title after the heading number")

    status_index = None
    for index, line in enumerate(lines[:20]):
        if STATUS_LINE_RE.match(line.strip()):
            status_index = index
            break
    if status_index is None:
        raise SeedError(f"{path.relative_to(root)}: missing status line")
    # The status header may wrap over several physical lines (0007 does); keep the whole
    # block so `status_line` is the register's readable provenance, not a truncation.
    status_parts: list[str] = []
    for line in lines[status_index : status_index + 6]:
        stripped = line.strip()
        if not stripped or (status_parts and stripped.startswith("#")):
            break
        status_parts.append(stripped)
    status_line = " ".join(status_parts)
    status_token = status_parts[0].split("·")[0][len("Status:") :].strip()
    status = normalise_status(status_token)
    date_match = re.search(r"Date:\s*(20\d{2}-\d{2}-\d{2})", status_line)
    date = date_match.group(1) if date_match else last_date(status_line)

    relative = str(path.relative_to(root))
    meta = {
        "source": relative,
        "adr_number": number,
        "status_line": status_line,
        "date": date,
        "content_sha256": hashlib.sha256(raw).hexdigest(),
    }
    if title_number is not None and title_number != number:
        # The filename is the register identity.  Keep the defect visible rather than
        # letting a stale H1 collide with another ADR's number.
        meta["title_number"] = title_number
    return Row(
        key=path.stem,
        title=title,
        body=text,
        status=status,
        meta=meta,
        source=relative,
        adr_number=number,
    )


def derive_adr_edges(row: Row, text: str) -> tuple[set[int], set[int], set[int]]:
    """Return (this file amends, this file is amended by, SPEC supersession notes)."""
    outgoing: set[int] = set()
    incoming: set[int] = set()
    notes: set[int] = set()
    for match in re.finditer(r"Amends:\s*ADR\s+(\d{4})", text):
        outgoing.add(int(match.group(1)))
    for match in re.finditer(r"This supersedes ADR\s+(\d{4})", text, re.IGNORECASE):
        outgoing.add(int(match.group(1)))
    for match in re.finditer(r"Amended by\s+(\d{4})", text):
        incoming.add(int(match.group(1)))
    for pattern in (
        r"superseded by SPEC\s+(D\d+)",
        r"replaces the DAG\s*\(SPEC\s+(D\d+)",
    ):
        for match in re.finditer(pattern, text, re.IGNORECASE):
            notes.add(int(match.group(1)[1:]))
    return outgoing, incoming, notes


# ── SPEC_V2 parsing ─────────────────────────────────────────────────────────────────────


def parse_spec(path: Path) -> dict[int, SpecDecision]:
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as error:
        raise SeedError(f"{path}: cannot read: {error}") from None

    # The register has one row per *literal* D## reference.  A range such as
    # ``D1–D5, D7–D17`` supplies a disposition but does not invent rows for the numbers
    # between its endpoints; that is why the register has 51 decisions, not 63.
    literal_refs = {
        int(match.group(1))
        for match in DREF_RE.finditer(text)
        if int(match.group(1)) not in AGENT_PLATFORM_REFS
    }
    if not literal_refs:
        raise SeedError("SPEC_V2: no D## decisions found")

    chosen: dict[int, SpecDecision] = {}

    def offer(decision: SpecDecision) -> None:
        existing = chosen.get(decision.number)
        if existing is None:
            chosen[decision.number] = decision
            return
        if existing.specific and decision.specific:
            if existing.status != decision.status:
                raise SeedError(
                    f"SPEC_V2 D{decision.number}: two conflicting dispositions "
                    f"({existing.disposition!r} and {decision.disposition!r})"
                )
            raise SeedError(f"SPEC_V2 D{decision.number}: duplicate decision row")
        if decision.specific and not existing.specific:
            # A specific row (the V2 table or an explicit Appendix B row) overrides the
            # disposition a range row offered; that is how D14 becomes "amended" while
            # remaining inside the D7–D17 "retained" range.
            chosen[decision.number] = decision
            return
        if existing.specific and not decision.specific:
            return
        if existing.status != decision.status:
            raise SeedError(
                f"SPEC_V2 D{decision.number}: two conflicting dispositions "
                f"({existing.disposition!r} and {decision.disposition!r})"
            )

    # §15.1 — the V2 decisions table.
    section_start = text.find("### 15.1")
    section_end = text.find("### 15.2", section_start + 1)
    if section_start < 0 or section_end < 0:
        raise SeedError("SPEC_V2: cannot find §15.1 V2 decisions table")
    for line in text[section_start:section_end].splitlines():
        cells = split_table_row(line)
        if cells is None or is_table_separator(cells) or len(cells) < 3:
            continue
        ref = re.fullmatch(r"\*\*(D\d+)\*\*", cells[0])
        if not ref:
            continue
        number = int(ref.group(1)[1:])
        title = strip_markdown(cells[1])
        body = cells[2].strip()
        proposed = bool(re.search(r"\(proposed\b", body, re.IGNORECASE))
        offer(
            SpecDecision(
                number=number,
                title=title,
                body=body,
                section="15.1",
                status="draft" if proposed else "approved",
                disposition="proposed" if proposed else "approved",
                date=last_date(body),
                specific=True,
            )
        )

    # Appendix B — the V1 decisions carried into V2.  A range row offers its disposition to
    # every literal reference inside the range; a specific row overrides the range.
    appendix_start = text.find("## Appendix B")
    appendix_end = text.find("## Appendix C", appendix_start + 1)
    if appendix_start < 0 or appendix_end < 0:
        raise SeedError("SPEC_V2: cannot find Appendix B V1 decisions table")
    for line in text[appendix_start:appendix_end].splitlines():
        cells = split_table_row(line)
        if cells is None or is_table_separator(cells) or len(cells) < 2:
            continue
        first = strip_markdown(cells[0])
        if not re.match(r"^D\d", first):
            continue
        if re.match(r"^D\d+'s\b", first):
            # A statement *inside* D19, not a second disposition for D19.
            continue
        covered = [number for number in expand_refs(first) if number in literal_refs]
        if not covered:
            continue
        status_cell = cells[1]
        status_text = status_cell.replace("**", "").strip()
        bold = re.search(r"\*\*(.+?)\*\*", status_cell)
        headline = bold.group(1).strip() if bold else status_text
        is_range = bool(re.search(r"D\d+\s*[–-]\s*D\d+", first))
        shared_description = adr_description(first)
        per_ref = {
            int(n): description.strip()
            for n, description in re.findall(r"D(\d+)\s*\(([^)]*)\)", first)
        }
        for number in covered:
            ref = f"D{number}"
            amended_match = re.search(rf"\b{ref}\s+amended by\s+(D\d+)", status_text)
            superseded_match = re.search(rf"\b{ref}\s+superseded by\b.*?\b(D\d+)", status_text)
            retained_match = re.search(rf"\b{ref}\s+retained\b", status_text, re.IGNORECASE)
            amended_by: list[int] = []
            superseded_by: int | None = None
            if amended_match:
                disposition = "amended"
                amended_by = [int(amended_match.group(1)[1:])]
            elif superseded_match:
                disposition = "superseded"
                superseded_by = int(superseded_match.group(1)[1:])
            elif retained_match:
                disposition = "retained"
            elif re.search(r"\bsuperseded\b", headline, re.IGNORECASE):
                disposition = "superseded"
                found = re.search(r"\bD(\d+)", headline)
                if not found:
                    raise SeedError(f"SPEC_V2 {ref}: superseded with no successor named")
                superseded_by = int(found.group(1))
            elif re.search(r"\bamended by\b", headline, re.IGNORECASE):
                disposition = "amended"
                found = re.search(r"\bD(\d+)", headline)
                if not found:
                    raise SeedError(f"SPEC_V2 {ref}: amended with no successor named")
                amended_by = [int(found.group(1))]
            elif re.search(r"\b(Retained|clarified|Extended)\b", headline, re.IGNORECASE):
                disposition = "retained"
            else:
                raise SeedError(f"SPEC_V2 {ref}: no disposition in Appendix B")
            status = "superseded" if disposition == "superseded" else "approved"
            if disposition == "amended":
                status_line = "amended by " + ", ".join(f"D{n}" for n in amended_by)
            elif disposition == "superseded":
                status_line = f"superseded by D{superseded_by}"
            else:
                status_line = disposition
            description = per_ref.get(number) or shared_description or "V1 decision"
            offer(
                SpecDecision(
                    number=number,
                    title=f"D{number} — {description}",
                    body=status_cell.strip(),
                    section="Appendix B",
                    status=status,
                    disposition=status_line,
                    date=last_date(line),
                    amended_by=amended_by,
                    superseded_by=superseded_by,
                    specific=not is_range,
                )
            )

    missing = sorted(literal_refs - set(chosen))
    if missing:
        raise SeedError(
            "SPEC_V2 decisions with no disposition: "
            + ", ".join(f"D{number}" for number in missing)
        )
    decisions = dict(sorted(chosen.items()))
    for decision in decisions.values():
        if decision.superseded_by is not None and decision.superseded_by not in decisions:
            raise SeedError(
                f"SPEC_V2 D{decision.number}: supersession target D{decision.superseded_by} "
                "is not a decision"
            )
        for target in decision.amended_by:
            if target not in decisions:
                raise SeedError(
                    f"SPEC_V2 D{decision.number}: amendment target D{target} is not a decision"
                )
    return decisions


# ── register construction ───────────────────────────────────────────────────────────────


@dataclass
class Register:
    rows: list[Row]
    spec: dict[int, SpecDecision]
    errors: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)


def build_register(root: Path = REPO_ROOT) -> Register:
    errors: list[str] = []
    warnings: list[str] = []
    adr_rows: list[Row] = []

    decisions_dir = root / ADR_DIR
    if not decisions_dir.is_dir():
        raise SeedError(f"{decisions_dir}: decisions directory is missing")
    for path in sorted(decisions_dir.glob("*.md")):
        try:
            adr_rows.append(parse_adr(path, root))
        except SeedError as error:
            errors.append(str(error))

    try:
        spec = parse_spec(root / SPEC_FILE)
    except SeedError as error:
        errors.append(str(error))
        spec = {}

    # Duplicate numbers and keys are register identity failures, not warnings.
    by_number: dict[int, list[str]] = {}
    by_key: dict[str, list[str]] = {}
    for row in adr_rows:
        by_number.setdefault(row.adr_number, []).append(row.source)
        by_key.setdefault(row.key, []).append(row.source)
    for number, sources in sorted(by_number.items()):
        if len(sources) > 1:
            errors.append(f"duplicate adr_number {number:04d}: {', '.join(sources)}")
    for key, sources in sorted(by_key.items()):
        if len(sources) > 1:
            errors.append(f"duplicate key {key!r}: {', '.join(sources)}")

    # Every D## an ADR names is that ADR's record of the decision; no spec row is created.
    mapped: dict[int, str] = {}
    for row in adr_rows:
        outgoing, incoming, notes = derive_adr_edges(row, row.body)
        if outgoing:
            row.meta["amends"] = sorted(f"{number:04d}" for number in outgoing)
        if incoming:
            row.meta["amended_by"] = sorted(f"{number:04d}" for number in incoming)
        expected = EXPECTED_ADR_AMENDS.get(row.key)
        if expected is not None and not expected.issubset(outgoing):
            errors.append(
                f"{row.source}: the declared amendment edge "
                f"{', '.join(f'{row.adr_number:04d}→{n:04d}' for n in sorted(expected))} "
                "is no longer present in the source text"
            )
        expected_note = EXPECTED_SUPERSESSION_NOTES.get(row.key)
        if expected_note is not None and expected_note not in notes:
            errors.append(
                f"{row.source}: the declared supersession note "
                f"{row.adr_number:04d}→SPEC D{expected_note} is no longer present in the source"
            )

        refs: set[int] = set()
        for match in DREF_RE.finditer(row.body):
            number = int(match.group(1))
            if number in AGENT_PLATFORM_REFS:
                continue
            refs.add(number)
        if refs:
            row.meta["spec_decision"] = sorted(f"D{number}" for number in refs)
        for number in sorted(refs):
            if number not in spec:
                errors.append(
                    f"{row.source}: references D{number}, which is not a SPEC_V2 decision"
                )
                continue
            if number in mapped:
                errors.append(
                    f"D{number} is recorded by both {mapped[number]} and {row.source}"
                )
                continue
            mapped[number] = row.key

        for number in sorted(notes):
            row.meta["superseded_by"] = f"SPEC D{number}"
            if number not in spec:
                errors.append(
                    f"{row.source}: supersession note names SPEC D{number}, which is not a decision"
                )

    # The two directions of an amendment are one fact: fill the reciprocal side even when
    # only the amending ADR's text names the edge.
    adr_by_number = {row.adr_number: row for row in adr_rows}
    for row in adr_rows:
        for target in row.meta.get("amends", []):
            target_row = adr_by_number.get(int(target))
            if target_row is not None:
                amended_by = set(target_row.meta.get("amended_by", []))
                amended_by.add(f"{row.adr_number:04d}")
                target_row.meta["amended_by"] = sorted(amended_by)
        for source in row.meta.get("amended_by", []):
            source_row = adr_by_number.get(int(source))
            if source_row is not None:
                amends = set(source_row.meta.get("amends", []))
                amends.add(f"{row.adr_number:04d}")
                source_row.meta["amends"] = sorted(amends)

    # Amendment edges: every target exists, no self-edge, no fork, no cycle.
    incoming_count: dict[int, int] = {}
    for row in adr_rows:
        for target in row.meta.get("amends", []):
            target_number = int(target)
            if target_number not in adr_by_number:
                errors.append(f"{row.source}: amends ADR {target} which does not exist")
            elif target_number == row.adr_number:
                errors.append(f"{row.source}: an ADR cannot amend itself")
            else:
                incoming_count[target_number] = incoming_count.get(target_number, 0) + 1
    for target, count in sorted(incoming_count.items()):
        if count > 1:
            errors.append(
                f"forked amendment chain: ADR {target:04d} is amended by {count} ADRs"
            )
    graph = {
        row.adr_number: [int(target) for target in row.meta.get("amends", [])]
        for row in adr_rows
    }

    def visit(node: int, trail: tuple[int, ...]) -> None:
        if node in trail:
            cycle = " → ".join(f"{n:04d}" for n in trail + (node,))
            errors.append(f"amendment cycle: {cycle}")
            return
        for nxt in graph.get(node, []):
            if nxt in graph:
                visit(nxt, trail + (node,))

    for node in sorted(graph):
        visit(node, ())

    # Spec rows: every decision not recorded by an ADR becomes spec-dNN.
    rows: list[Row] = list(adr_rows)
    for number, decision in sorted(spec.items()):
        if number in mapped:
            adr_row = next(row for row in adr_rows if row.key == mapped[number])
            if decision.status != adr_row.status:
                errors.append(
                    f"conflicting disposition for D{number}: SPEC_V2 says "
                    f"{decision.disposition!r} but {adr_row.source} is {adr_row.status!r}"
                )
            continue
        successor_key = None
        meta = {
            "source": str(SPEC_FILE),
            "spec_section": decision.section,
            "spec_decision": f"D{number}",
            "status_line": decision.disposition,
            "date": decision.date,
        }
        if decision.amended_by:
            meta["amended_by"] = [f"D{target}" for target in decision.amended_by]
        if decision.superseded_by is not None:
            meta["superseded_by"] = f"D{decision.superseded_by}"
            successor_key = f"spec-d{decision.superseded_by}"
        content_sha = sha256_text(
            f"spec-d{number}",
            decision.title,
            decision.body,
            decision.status,
            decision.superseded_by,
            decision.amended_by,
        )
        meta["content_sha256"] = content_sha
        rows.append(
            Row(
                key=f"spec-d{number}",
                title=f"D{number} — {decision.title}"
                if not decision.title.startswith(f"D{number}")
                else decision.title,
                body=decision.body,
                status=decision.status,
                meta=meta,
                source=str(SPEC_FILE),
                spec_decision=f"D{number}",
                superseded_by_key=successor_key,
            )
        )

    # Same-table supersession edges must name a live row and must not fork.
    row_by_key = {row.key: row for row in rows}
    successor_users: dict[str, int] = {}
    for row in rows:
        if row.status == "superseded" and row.superseded_by_key is None:
            errors.append(
                f"{row.key}: status is superseded but no same-table successor is mapped"
            )
        if row.superseded_by_key is None:
            continue
        if row.superseded_by_key == row.key:
            errors.append(f"{row.key}: an asset cannot supersede itself")
        elif row.superseded_by_key not in row_by_key:
            errors.append(f"{row.key}: superseded_by names {row.superseded_by_key} which is absent")
        else:
            successor_users[row.superseded_by_key] = (
                successor_users.get(row.superseded_by_key, 0) + 1
            )
    for successor, count in sorted(successor_users.items()):
        if count > 1:
            errors.append(f"forked supersession chain: {successor} supersedes {count} rows")

    # Final duplicate-key guard covers ADR/spec key collisions too.
    keys: dict[str, list[str]] = {}
    for row in rows:
        keys.setdefault(row.key, []).append(row.source)
    for key, sources in sorted(keys.items()):
        if len(sources) > 1 and not any(key in error for error in errors):
            errors.append(f"duplicate key {key!r}: {', '.join(sources)}")

    for row in adr_rows:
        title_number = row.meta.get("title_number")
        if title_number is not None:
            warnings.append(
                f"{row.source}: H1 names ADR {title_number:04d} but the filename number "
                f"{row.adr_number:04d} is authoritative"
            )

    if not adr_rows and not errors:
        errors.append("no ADR sources found")
    return Register(rows=rows, spec=spec, errors=errors, warnings=warnings)


def summary_lines(register: Register) -> list[str]:
    adr_rows = [row for row in register.rows if row.adr_number is not None]
    spec_rows = [row for row in register.rows if row.spec_decision is not None and row.adr_number is None]
    recorded = sum(1 for decision in register.spec if decision in {
        int(ref[1:]) for row in adr_rows for ref in row.meta.get("spec_decision", [])
    })
    unrecorded = len(register.spec) - recorded
    statuses: dict[str, int] = {}
    for row in register.rows:
        statuses[row.status] = statuses.get(row.status, 0) + 1
    amendments = sum(len(row.meta.get("amends", [])) for row in adr_rows)
    notes = sum(1 for row in adr_rows if row.meta.get("superseded_by"))
    table_edges = sum(1 for row in register.rows if row.superseded_by_key)
    numbers = sorted(row.adr_number for row in adr_rows)
    spec_numbers = sorted(register.spec)
    return [
        f"ADR sources: {len(adr_rows)} ({', '.join(f'{n:04d}' for n in numbers) or 'none'})",
        f"SPEC_V2 decisions: {len(register.spec)} "
        f"({recorded} recorded by an ADR, {unrecorded} unrecorded)",
        f"literal D refs span D{spec_numbers[0]}–D{spec_numbers[-1]} (not contiguous)"
        if spec_numbers
        else "literal D refs: none",
        f"rows: {len(register.rows)} ({len(adr_rows)} adr + {len(spec_rows)} spec)",
        "dispositions: "
        + ", ".join(f"{status}={statuses.get(status, 0)}" for status in
                    ("approved", "draft", "in_review", "superseded", "archived")),
        f"edges: {amendments} amendment(s), {notes} meta supersession note(s), "
        f"{table_edges} same-table supersession(s)",
    ]


# ── action clients ──────────────────────────────────────────────────────────────────────


class ActionTransport:
    """Paging shared by every transport: one ``invoke``, ``decisions.list`` for the rest."""

    def invoke(
        self,
        action: str,
        payload: dict,
        if_match: int | None = None,
    ) -> dict:
        raise NotImplementedError

    def list_all(self) -> list[dict]:
        items: list[dict] = []
        cursor = None
        while True:
            payload: dict = {"limit": 200}
            if cursor:
                payload["cursor"] = cursor
            page = self.invoke("decisions.list", payload)
            items.extend(page.get("items", []))
            cursor = page.get("next_cursor")
            if not cursor:
                return items


class ActionClient(ActionTransport):
    """The one HTTP action endpoint: ``POST /api/actions/<name>/invocations``."""

    def __init__(
        self,
        url: str,
        token: str | None = None,
        app_key: str | None = None,
        tenant: str | None = None,
        timeout: float = 60.0,
    ) -> None:
        if not url:
            raise SeedError("--apply requires --url or CONVEYOR_URL")
        if not token and not app_key:
            raise SeedError("--apply requires --token or --app-key (or their env vars)")
        base = url.rstrip("/")
        if not base.endswith("/api"):
            base += "/api"
        self.base = base
        self.token = token
        self.app_key = app_key
        self.tenant = tenant
        self.timeout = timeout

    def invoke(
        self,
        action: str,
        payload: dict,
        if_match: int | None = None,
    ) -> dict:
        endpoint = f"{self.base}/actions/{action}/invocations"
        request = urllib.request.Request(
            endpoint,
            data=json.dumps(payload, separators=(",", ":"), ensure_ascii=False).encode("utf-8"),
            method="POST",
            headers={"Content-Type": "application/json", "Accept": "application/json"},
        )
        if self.token:
            request.add_header("Authorization", f"Bearer {self.token}")
        if self.app_key:
            request.add_header("X-App-Key", self.app_key)
        if self.tenant:
            request.add_header("X-Tenant-Id", self.tenant)
        if if_match is not None:
            request.add_header("If-Match", f'"{if_match}"')
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:
                body = response.read().decode("utf-8")
        except urllib.error.HTTPError as error:
            detail = error.read().decode("utf-8", "replace")
            raise SeedError(f"{action}: HTTP {error.code}: {detail[:2000]}") from None
        except (urllib.error.URLError, OSError) as error:
            raise SeedError(f"{action}: cannot reach {endpoint}: {error}") from None
        try:
            envelope = json.loads(body)
        except ValueError as error:
            raise SeedError(f"{action}: response is not JSON: {body[:500]!r}") from None
        if isinstance(envelope, dict) and "output" in envelope:
            return envelope["output"]
        return envelope


class LocalBinClient(ActionTransport):
    """Runs ``admin action invoke`` on a branch-built binary over stdin/stdout.

    The binary reads ``DATABASE_MIGRATE_URL`` and ``APP_KEY`` from the inherited environment, so
    the writes land in whatever database the caller points the binary at; ``RUST_LOG`` is pinned
    to ``error`` so logs cannot corrupt the single JSON document on stdout.  ``actor_id`` is the
    real principal the action records — the reason this transport exists where the deployed
    server has no seeded dev principal.
    """

    def __init__(
        self,
        binary: str,
        tenant: str | None = None,
        actor_id: str | None = None,
        timeout: float = 120.0,
    ) -> None:
        if not binary:
            raise SeedError("--local-bin requires a binary path")
        if actor_id is not None:
            try:
                uuid.UUID(actor_id)
            except ValueError:
                raise SeedError(f"--actor-id is not a UUID: {actor_id!r}") from None
        self.binary = binary
        self.tenant = tenant or os.environ.get("TENANT") or DEFAULT_TENANT
        self.actor_id = actor_id
        self.timeout = timeout

    def build_argv(self, action: str, if_match: int | None = None) -> list[str]:
        argv = [
            self.binary,
            "admin", "action", "invoke",
            "--tenant", self.tenant,
            "--name", action,
            "--input", "/dev/stdin",
        ]
        if self.actor_id:
            argv += ["--actor-id", self.actor_id]
        if if_match is not None:
            argv += ["--if-match", str(if_match)]
        return argv

    def invoke(
        self,
        action: str,
        payload: dict,
        if_match: int | None = None,
    ) -> dict:
        stdin = json.dumps(payload, separators=(",", ":"), ensure_ascii=False) + "\n"
        env = dict(os.environ)
        env.setdefault("RUST_LOG", "error")
        try:
            completed = subprocess.run(
                self.build_argv(action, if_match),
                input=stdin,
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
                timeout=self.timeout,
                env=env,
            )
        except subprocess.TimeoutExpired:
            raise SeedError(
                f"{action}: local binary timed out after {self.timeout:g}s"
            ) from None
        except OSError as error:
            raise SeedError(
                f"{action}: cannot run local binary {self.binary}: {error}"
            ) from None
        if completed.returncode != 0:
            detail = completed.stderr.strip() or completed.stdout.strip()
            raise SeedError(
                f"{action}: local binary exited {completed.returncode}: {detail[:2000]}"
            )
        try:
            output = json.loads(completed.stdout)
        except ValueError:
            raise SeedError(
                f"{action}: local binary output is not JSON: {completed.stdout[:500]!r}"
            ) from None
        if not isinstance(output, dict):
            raise SeedError(f"{action}: local binary output is not a JSON object: {output!r}")
        return output


# ── apply ───────────────────────────────────────────────────────────────────────────────


def transition(row: Row, client, writes: dict, to: str, successor_id: str | None = None) -> None:
    payload: dict = {"id": row.row_id, "to": to}
    if successor_id is not None:
        payload["superseded_by"] = successor_id
    output = client.invoke("decisions.transition", payload)
    row.current_status = output.get("status", to)
    row.version = output.get("version", row.version)
    writes["transitions"] += 1


def advance_to_approved(row: Row, client, writes: dict) -> None:
    if row.current_status == "approved":
        return
    if row.current_status in ("superseded", "archived"):
        raise SeedError(
            f"{row.key}: is {row.current_status} and cannot be moved back to approved"
        )
    if row.current_status in (None, "draft"):
        transition(row, client, writes, "in_review")
    if row.current_status == "in_review":
        transition(row, client, writes, "approved")
    if row.current_status != "approved":
        raise SeedError(f"{row.key}: failed to reach approved (now {row.current_status})")


def apply_register(
    register: Register,
    client,
    approval_id: str | None = None,
) -> dict:
    if register.errors:
        raise SeedError("refusing to apply while the check has errors")
    if approval_id:
        for row in register.rows:
            row.meta["approval_id"] = approval_id

    existing = {item["key"]: item for item in client.list_all()}
    writes = {"created": 0, "updated": 0, "unchanged": 0, "transitions": 0}
    for row in register.rows:
        current = existing.get(row.key)
        if current is None:
            output = client.invoke("decisions.create", row.create_payload())
            row.row_id = output["id"]
            row.version = output.get("version")
            row.current_status = output.get("status", "draft")
            writes["created"] += 1
            continue
        row.row_id = current["id"]
        row.version = current.get("version")
        row.current_status = current.get("status", "draft")
        current_meta = current.get("meta") or {}
        if current_meta.get("content_sha256") == row.meta.get("content_sha256"):
            writes["unchanged"] += 1
            if approval_id and current_meta.get("approval_id") != approval_id:
                print(
                    f"note: {row.key} is content-unchanged; the supplied approval id was not "
                    "written (an unchanged row is never updated)",
                    file=sys.stderr,
                )
            continue
        output = client.invoke(
            "decisions.update", row.update_payload(), if_match=row.version
        )
        row.version = output.get("version", row.version)
        row.current_status = output.get("status", row.current_status)
        writes["updated"] += 1

    rows_by_key = {row.key: row for row in register.rows}
    for row in register.rows:
        if row.status == "draft":
            if row.current_status == "in_review":
                transition(row, client, writes, "draft")
            elif row.current_status not in (None, "draft"):
                raise SeedError(
                    f"{row.key}: desired draft but the row is {row.current_status}; "
                    "demotion is not automatic"
                )
        elif row.status == "approved":
            advance_to_approved(row, client, writes)
        elif row.status == "superseded":
            if row.current_status == "superseded":
                continue
            successor = rows_by_key.get(row.superseded_by_key or "")
            if successor is None:
                raise SeedError(f"{row.key}: no successor row {row.superseded_by_key!r}")
            advance_to_approved(successor, client, writes)
            advance_to_approved(row, client, writes)
            transition(row, client, writes, "superseded", successor.row_id)
        else:
            raise SeedError(f"{row.key}: unknown desired status {row.status!r}")
    return writes


# ── self-test ───────────────────────────────────────────────────────────────────────────


class FakeClient:
    def __init__(self) -> None:
        self.rows: dict[str, dict] = {}
        self.calls: list[tuple[str, dict, int | None]] = []

    def writes(self) -> int:
        return sum(1 for action, _, _ in self.calls if action != "decisions.list")

    def list_all(self) -> list[dict]:
        return self.invoke("decisions.list", {"limit": 200})["items"]

    def invoke(self, action: str, payload: dict, if_match: int | None = None) -> dict:
        self.calls.append((action, dict(payload), if_match))
        if action == "decisions.list":
            return {"items": [dict(row) for row in self.rows.values()], "next_cursor": None}
        if action == "decisions.create":
            key = payload["key"]
            assert key not in self.rows, f"duplicate create {key}"
            row = {
                "id": f"id-{key}",
                "key": key,
                "title": payload["title"],
                "body": payload["body"],
                "meta": payload["meta"],
                "status": "draft",
                "version": 1,
            }
            self.rows[key] = row
            return dict(row)
        if action == "decisions.update":
            row = self.rows[payload["id"].removeprefix("id-")]
            assert if_match == row["version"], f"stale If-Match for {row['key']}"
            row.update(
                title=payload["title"],
                body=payload["body"],
                meta=payload["meta"],
                version=row["version"] + 1,
            )
            return dict(row)
        if action == "decisions.transition":
            row = self.rows[payload["id"].removeprefix("id-")]
            to = payload["to"]
            if to == "superseded":
                assert row["status"] == "approved", "only approved can be superseded"
                successor = self.rows[payload["superseded_by"].removeprefix("id-")]
                assert successor["status"] == "approved", "successor must be live"
            else:
                legal = {
                    ("draft", "in_review"),
                    ("in_review", "approved"),
                    ("in_review", "draft"),
                }
                assert (row["status"], to) in legal, f"illegal {row['status']}→{to}"
            row["status"] = to
            row["version"] += 1
            return dict(row)
        raise AssertionError(f"unexpected action {action}")


def write_fixture(
    root: Path,
    *,
    spec_rows: list[str] | None = None,
    appendix_rows: list[str] | None = None,
) -> None:
    decisions = root / ADR_DIR
    decisions.mkdir(parents=True)
    (decisions / "0001-first.md").write_text(
        "# 0001 — First\n\nStatus: Accepted · Date: 2026-01-01\n\n"
        "## Decision\n\nThe first decision (D1).\n",
        encoding="utf-8",
    )
    (decisions / "0002-second.md").write_text(
        "# 0002 — Second\n\nStatus: Accepted · Date: 2026-01-02 · Amended by 0003\n\n"
        "## Decision\n\nThe second decision (D2).\n",
        encoding="utf-8",
    )
    (decisions / "0003-third.md").write_text(
        "# 0003 — Third\n\nStatus: Proposed · Date: 2026-01-03 · Amends: ADR 0002\n\n"
        "## Decision\n\nThe third decision.\n",
        encoding="utf-8",
    )
    rows = spec_rows or [
        "| **D1** | One | First answer (owner, 2026-01-01) |",
        "| **D2** | Two | Second answer |",
        "| **D3** | Three | Third answer (proposed 2026-01-02) |",
    ]
    appendix = appendix_rows or ["| (none) | n/a |"]
    (root / SPEC_FILE).write_text(
        "## 15. Decisions and open questions\n\n"
        "### 15.1 V2 decisions\n\n"
        "| # | Decision | Answer |\n|---|---|---|\n"
        + "\n".join(rows)
        + "\n\n### 15.2 Open questions\n\n"
        "## Appendix B — V1 decisions\n\n"
        "| V1 decision | V2 status |\n|---|---|\n"
        + "\n".join(appendix)
        + "\n\n## Appendix C — Carry\n",
        encoding="utf-8",
    )


def expect_error(name: str, register: Register, needle: str) -> None:
    matches = [error for error in register.errors if needle in error]
    assert matches, f"{name}: expected an error containing {needle!r}, got {register.errors!r}"


def run_self_test() -> None:
    checks = 0
    with tempfile.TemporaryDirectory(prefix="seed-adr-selftest-") as raw:
        base = Path(raw)

        # 1. The clean fixture parses, counts, and refuses nothing.
        root = base / "clean"
        write_fixture(root)
        register = build_register(root)
        assert not register.errors, register.errors
        assert len(register.rows) == 4, register.rows
        assert len(register.spec) == 3
        assert [row.key for row in register.rows] == [
            "0001-first", "0002-second", "0003-third", "spec-d3",
        ]
        spec_row = next(row for row in register.rows if row.key == "spec-d3")
        assert spec_row.status == "draft", spec_row.status
        assert register.rows[1].meta["amended_by"] == ["0003"]
        assert register.rows[2].meta["amends"] == ["0002"]
        checks += 1

        # 2. Duplicate adr_number is refused even when the keys differ.
        root = base / "duplicate-number"
        write_fixture(root)
        (root / ADR_DIR / "0001-copy.md").write_text(
            "# 0001 — Copy\n\nStatus: Accepted · Date: 2026-01-04\n", encoding="utf-8"
        )
        expect_error("duplicate number", build_register(root), "duplicate adr_number 0001")
        checks += 1

        # 3. A missing status line is refused.
        root = base / "missing-status"
        write_fixture(root)
        (root / ADR_DIR / "0001-first.md").write_text("# 0001 — First\n\nNo status.\n")
        expect_error("missing status", build_register(root), "missing status line")
        checks += 1

        # 4. A source file with no row mapping is refused.
        root = base / "unmapped"
        write_fixture(root)
        (root / ADR_DIR / "notes.md").write_text("# Notes\n", encoding="utf-8")
        expect_error("unmapped", build_register(root), "no row mapping")
        checks += 1

        # 5. Two conflicting dispositions for one D## are refused.
        root = base / "conflict"
        write_fixture(
            root,
            spec_rows=[
                "| **D1** | One | First answer |",
                "| **D1** | One again | Second answer (proposed 2026-01-02) |",
            ],
        )
        expect_error("conflict", build_register(root), "two conflicting dispositions")
        checks += 1

        # 6. A forged amendment edge (target absent) is refused.
        root = base / "forged"
        write_fixture(root)
        (root / ADR_DIR / "0003-third.md").write_text(
            "# 0003 — Third\n\nStatus: Proposed · Date: 2026-01-03 · "
            "Amends: ADR 0099\n",
            encoding="utf-8",
        )
        expect_error("forged", build_register(root), "amends ADR 0099 which does not exist")
        checks += 1

        # 7. A forked amendment chain (two ADRs amending one) is refused.
        root = base / "fork"
        write_fixture(root)
        (root / ADR_DIR / "0004-fourth.md").write_text(
            "# 0004 — Fourth\n\nStatus: Accepted · Date: 2026-01-04 · "
            "Amends: ADR 0002\n",
            encoding="utf-8",
        )
        expect_error("fork", build_register(root), "forked amendment chain")
        checks += 1

        # 8. Apply is idempotent: the second pass writes nothing.
        root = base / "apply"
        write_fixture(root)
        register = build_register(root)
        client = FakeClient()
        first = apply_register(register, client)
        assert first == {"created": 4, "updated": 0, "unchanged": 0, "transitions": 4}, first
        assert client.rows["0001-first"]["status"] == "approved"
        assert client.rows["0002-second"]["status"] == "approved"
        assert client.rows["spec-d3"]["status"] == "draft"
        client.calls.clear()
        second = apply_register(build_register(root), client)
        assert second == {"created": 0, "updated": 0, "unchanged": 4, "transitions": 0}, second
        assert client.writes() == 0, client.calls
        checks += 1

        # 9. A changed source updates exactly that row and leaves its status alone.
        (root / ADR_DIR / "0001-first.md").write_text(
            "# 0001 — First\n\nStatus: Accepted · Date: 2026-01-01\n\n"
            "## Decision\n\nThe first decision, revised (D1).\n",
            encoding="utf-8",
        )
        client.calls.clear()
        third = apply_register(build_register(root), client)
        assert third == {"created": 0, "updated": 1, "unchanged": 3, "transitions": 0}, third
        update_calls = [call for call in client.calls if call[0] == "decisions.update"]
        assert len(update_calls) == 1 and update_calls[0][1]["id"] == "id-0001-first"
        checks += 1

        # 10. A same-table supersession is applied once, and a re-run is a no-op.
        root = base / "supersede"
        write_fixture(
            root,
            appendix_rows=[
                "| D18 (old) | **Already superseded by D19** |",
                "| D19 (new) | **Retained** |",
            ],
        )
        register = build_register(root)
        assert not register.errors, register.errors
        successor = next(row for row in register.rows if row.key == "spec-d18")
        assert successor.status == "superseded"
        assert successor.superseded_by_key == "spec-d19"
        client = FakeClient()
        first = apply_register(register, client)
        assert first["created"] == 6, first
        assert client.rows["spec-d18"]["status"] == "superseded"
        assert client.rows["spec-d19"]["status"] == "approved"
        client.calls.clear()
        second = apply_register(build_register(root), client)
        assert second == {"created": 0, "updated": 0, "unchanged": 6, "transitions": 0}, second
        assert client.writes() == 0, client.calls
        checks += 1

        # 11. The local-binary transport builds the admin argv and round-trips JSON.
        local = base / "local-bin"
        local.mkdir()
        stub = local / "stub.py"
        stub.write_text(
            "#!/usr/bin/env python3\n"
            "import json, sys\n"
            "from pathlib import Path\n"
            "here = Path(__file__).resolve().parent\n"
            "(here / 'argv.json').write_text(json.dumps(sys.argv[1:]), encoding='utf-8')\n"
            "data = json.loads(sys.stdin.read())\n"
            "(here / 'stdin.json').write_text(json.dumps(data), encoding='utf-8')\n"
            "print(json.dumps({'echo': data}))\n",
            encoding="utf-8",
        )
        stub.chmod(0o755)
        actor = "3f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d"
        client = LocalBinClient(str(stub), tenant="tenant-x", actor_id=actor)
        output = client.invoke("decisions.list", {"limit": 7}, if_match=3)
        assert output == {"echo": {"limit": 7}}, output
        argv = json.loads((local / "argv.json").read_text(encoding="utf-8"))
        assert argv == [
            "admin", "action", "invoke",
            "--tenant", "tenant-x",
            "--name", "decisions.list",
            "--input", "/dev/stdin",
            "--actor-id", actor,
            "--if-match", "3",
        ], argv
        assert json.loads((local / "stdin.json").read_text(encoding="utf-8")) == {"limit": 7}
        checks += 1

        # 12. A non-zero binary exit is a SeedError naming the code and the stderr.
        failing = local / "fail.py"
        failing.write_text(
            "#!/usr/bin/env python3\n"
            "import sys\n"
            "print('no seeded principal', file=sys.stderr)\n"
            "sys.exit(3)\n",
            encoding="utf-8",
        )
        failing.chmod(0o755)
        try:
            LocalBinClient(str(failing), actor_id=actor).invoke("decisions.list", {})
        except SeedError as error:
            assert "exited 3" in str(error), error
            assert "no seeded principal" in str(error), error
        else:
            raise AssertionError("a non-zero local binary must raise SeedError")
        checks += 1

        # 13. A binary that prints anything but a JSON object is refused.
        garbage = local / "garbage.py"
        garbage.write_text(
            "#!/usr/bin/env python3\nprint('hello from a log line')\n",
            encoding="utf-8",
        )
        garbage.chmod(0o755)
        try:
            LocalBinClient(str(garbage), actor_id=actor).invoke("decisions.list", {})
        except SeedError as error:
            assert "not JSON" in str(error), error
        else:
            raise AssertionError("non-JSON local binary output must raise SeedError")
        checks += 1

    print(f"self-test: {checks} checks passed")


# ── CLI ─────────────────────────────────────────────────────────────────────────────────


def cli(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Seed governed_asset(kind='adr') from docs/decisions and docs/SPEC_V2.md."
    )
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--check", action="store_true", help="offline check (default)")
    mode.add_argument("--apply", action="store_true", help="write rows through decisions.* actions")
    mode.add_argument("--self-test", action="store_true", help="run the offline fixture tests")
    parser.add_argument("--root", type=Path, default=REPO_ROOT, help="repository root")
    parser.add_argument("--url", default=os.environ.get("CONVEYOR_URL"))
    parser.add_argument("--token", default=os.environ.get("CONVEYOR_TOKEN"))
    parser.add_argument("--app-key", default=os.environ.get("CONVEYOR_APP_KEY"))
    parser.add_argument("--tenant", default=os.environ.get("CONVEYOR_TENANT"))
    parser.add_argument(
        "--local-bin",
        default=os.environ.get("CONVEYOR_LOCAL_BIN"),
        metavar="PATH",
        help="run the branch-built binary's `admin action invoke` instead of HTTP",
    )
    parser.add_argument(
        "--actor-id",
        default=os.environ.get("CONVEYOR_ACTOR_ID"),
        metavar="UUID",
        help="principal recorded by --local-bin",
    )
    parser.add_argument(
        "--approval-id",
        default=os.environ.get("CONVEYOR_APPROVAL_ID"),
        help="recorded in meta only; the decisions.* actions have no approval gate",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = cli(argv)
    if args.self_test:
        try:
            run_self_test()
        except AssertionError as error:
            print(f"self-test: FAILED: {error}", file=sys.stderr)
            return 1
        return 0

    try:
        register = build_register(args.root)
    except SeedError as error:
        print(f"seed-adr-register: {error}", file=sys.stderr)
        return 1

    for warning in register.warnings:
        print(f"warning: {warning}", file=sys.stderr)
    for line in summary_lines(register):
        print(line)
    if register.errors:
        for error in register.errors:
            print(f"error: {error}", file=sys.stderr)
        print(f"REFUSED: {len(register.errors)} error(s)", file=sys.stderr)
        return 1
    print("check: OK")

    if not args.apply:
        return 0

    try:
        if args.local_bin:
            client = LocalBinClient(
                args.local_bin,
                tenant=args.tenant,
                actor_id=args.actor_id,
            )
        else:
            if args.actor_id:
                raise SeedError(
                    "--actor-id requires --local-bin; the HTTP transport has no actor id"
                )
            client = ActionClient(
                url=args.url,
                token=args.token,
                app_key=args.app_key,
                tenant=args.tenant,
            )
        writes = apply_register(register, client, approval_id=args.approval_id)
    except SeedError as error:
        print(f"apply: FAILED: {error}", file=sys.stderr)
        return 1
    print(
        "apply: "
        f"created={writes['created']} updated={writes['updated']} "
        f"unchanged={writes['unchanged']} transitions={writes['transitions']}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
