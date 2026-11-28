#!/usr/bin/env python3
"""The unit issue content contract — pure parsers only.

WHY THIS FILE EXISTS
--------------------
`scripts/ci/board.py` is the instrument: it reads GitHub, plans label moves and writes comments.
The parsing half of it is pure, and other readers need the same answer — the proof collector
cross-checks `Verify:` lines, and the public readiness derivation reads story/scenario facts. A
second parser would drift, so the rules live here once and `board.py` delegates to this module.

THE RULES (docs/plans/03-DELIVERY.md § "Issue content contract", SPEC_V2 §5.8)
-----------------------------------------------------------------------------
- Every contract section is present and non-empty.
- The `Programme` rows (tranche, brief, ADM phase/plateau, benefits) carry a value.
- Every `### P<n>` part carries a `Verify:` line.
- A unit created on/after `INVEST_SINCE` names the six INVEST criteria with its own evidence —
  a row that restates the template's generic explanation is not evidence.
- A unit created on/after `STORY_SINCE` carries one `## User story` (`As a …, I want …, so
  that …`) and, per part, a Gherkin `Scenario:` with a `@verify=<id>` tag; the Definition of done
  has one table row per scenario, keyed by that id.
- The public progress record publishes `readiness_summary` — the same derivations as counts and
  booleans under `unit-readiness/v1` — so it can show whether a unit uses the standard without
  publishing one word of its body.

FORWARD-ONLY
------------
The story and INVEST rules start at the date the contract gained them. An earlier issue is never
made unready retroactively, and `STORY_EXEMPT` names the open issues created under the old body
shape that must not be re-shaped while they are in flight.

This module has no network, git, clock or filesystem access. `board.py --self-test` is its matrix.
"""

from __future__ import annotations

import datetime as dt
import re

# `03-DELIVERY.md` § "Issue content contract": the programme context is required for issues
# created on/after this date; an earlier issue gains it at its next claim. Keep the two in step.
PROGRAMME_SINCE = dt.date(2026, 11, 18)
PROGRAMME_FIELDS = ("Tranche", "Brief", "ADM phase/plateau", "Benefits")

# The INVEST Definition of Ready and the acceptance-criteria Definition of Done (owner direction
# 2026-11-28). Forward-only from the day the contract gained them: an issue created earlier is
# never made unready retroactively.
INVEST_SINCE = dt.date(2026, 11, 28)
INVEST_CRITERIA = ("Independent", "Negotiable", "Valuable", "Estimable", "Small", "Testable")

# The BDD user story and Gherkin acceptance (SPEC_V2 §5.8, owner direction 2026-11-28). Same
# forward-only rule as INVEST.
STORY_SINCE = dt.date(2026, 11, 28)
# #930 predates the rule's merge; remove it when that issue closes.
STORY_EXEMPT = frozenset({930})

# The issue content contract's sections, in the template's order. Both `##` and `###` headings
# count: the Markdown template and older issues use h2, and a GitHub issue form renders every
# field as h3 (`### Goal`), so one parser must read both or the form could never satisfy it.
CONTRACT_SECTIONS = (
    "Goal",
    "Programme",
    "References",
    "Baseline",
    "User story",
    "Parts and acceptance criteria",
    "Dependencies",
    "Definition of ready",
    "Definition of done",
)
SECTION_RE = re.compile(r"^(#{2,3})\s+(.*?)\s*#*\s*$")
# A scenario's acceptance line, however it is emphasised: `Verify:` in the template, `**Verify:**`
# in most authored bodies. The leading emphasis markers are not part of the token.
VERIFY_RE = re.compile(r"^\s*[*_`]*verify\s*:", re.IGNORECASE)

# Every heading the unit template (and so the form) can carry. A same-level heading in this set
# ends the section it follows; any other same-level heading is an author's subheading.
KNOWN_SECTIONS = tuple(name.lower() for name in CONTRACT_SECTIONS) + (
    "plan / audit source",
    "claim",
    "current state (measured)",
    "out of scope",
    "deployment prerequisite",
    "open questions",
    "evidence at completion",
)

# One user story per unit, all three clauses non-blank. `re.S` lets an author wrap the sentence
# over lines without weakening the three-clause test.
STORY_RE = re.compile(r"As (?:an?|the)\b .+?,\s*I want .+?,\s*so that .+", re.I | re.S)
# A Gherkin scenario heading; `Scenario Outline` counts once (its examples expand at parse time).
SCENARIO_RE = re.compile(r"^\s*Scenario(?: Outline)?:\s*(.+?)\s*$", re.M | re.I)
# The tag that ties a scenario to its test or the issue's `Verify:` line. The id never contains a
# space, so the line rule above remains the only place the command itself appears.
VERIFY_TAG_RE = re.compile(r"@verify[=:]\s*([A-Za-z0-9._:/-]+)", re.I)

# The public readiness summary's schema (SPEC_V2 §5.8, D69): fixed vocabulary counts, never body
# text. `scripts/fetch-issues.sh` writes one per `unit` row; the design view renders it.
READINESS_SCHEMA = "unit-readiness/v1"
# A DoD evidence cell the author has not filled yet, at any case and however trimmed.
UNDONE_DOD_CELLS = frozenset({"to fill", "—", "-"})

# The template's generic DoR text (`.github/ISSUE_TEMPLATE/unit.md`). A row whose evidence is the
# template's restatement is not the unit's own evidence. The comparison is on the evidence after the
# criterion, so a row that states the definition and then appends the unit's facts passes; only a
# row that is the definition alone (or the current template's `<…>` placeholder) fails.
TEMPLATE_DOR_PHRASES = (
    (
        "Independent",
        (
            "the unit lands without another unit's files; a real ordering is a typed dependency.",
            "the unit lands without another unit's files",
        ),
    ),
    (
        "Negotiable",
        (
            "the outcome is fixed; the implementation is settled at delivery, not in the brief.",
            "the implementation is settled at delivery",
        ),
    ),
    (
        "Valuable",
        (
            "the outcome maps to a benefit or removes a named gap.",
            "the outcome maps to a benefit or removes a named gap",
        ),
    ),
    (
        "Estimable",
        ("the baseline and every file are named.", "the baseline and every file are named"),
    ),
    ("Small", ("one issue, one branch, one PR.", "one issue, one branch, one PR")),
    (
        "Testable",
        (
            "every part has ≥1 bdd scenario with a `verify:` command or test target.",
            "every part has ≥1 bdd scenario",
        ),
    ),
)


def _criterion_line(line: str, criterion: str):
    """The DoR label for `criterion`, anchored to the bold label at the row's start.

    The prose inside another criterion's evidence may contain the ordinary word (`…changes are
    small`); matching anywhere would attribute that row to the Small criterion.
    """
    return re.search(
        rf"^\s*(?:[-*]\s*\[[ xX]\]\s*)?\*\*{re.escape(criterion)}\*\*", line, re.IGNORECASE
    )

# A `### P<n>` part heading. The number is what the gaps name, so an author can find the block.
PART_RE = re.compile(r"(?m)^###\s+P(\d+)\b")
# Characters an `@verify` id may contain: a DoD cell must name the id as a token, not as the
# prefix of a longer one (`P1` must not be satisfied by a `P10` row).
ID_CHARS = "A-Za-z0-9._:/-"


def story_required(created: dt.date, number: int) -> bool:
    """Whether the story/Gherkin half of the contract applies to this issue.

    Forward-only: only units created on/after `STORY_SINCE`, and never an exempt number.
    """
    return created >= STORY_SINCE and number not in STORY_EXEMPT


def section_lines(body: str, heading: str) -> list[str] | None:
    """The lines under a `## Heading` or `### Heading`, or None when it is absent.

    The section ends at the next heading of the same or a shallower level, so an h3 section ends
    at the next `###` or `##`, and an h2 section ends at the next `##` or `#`.
    """
    lines = body.splitlines()
    wanted = heading.strip().lower()
    known = set(KNOWN_SECTIONS)
    for index, line in enumerate(lines):
        match = SECTION_RE.match(line)
        if not match or match.group(2).strip().lower() != wanted:
            continue
        level = len(match.group(1))
        content: list[str] = []
        for following in lines[index + 1 :]:
            next_match = SECTION_RE.match(following)
            if next_match:
                next_level = len(next_match.group(1))
                next_name = next_match.group(2).strip().lower()
                # A shallower heading always ends the section. A heading at the same level ends
                # it only when it names another contract section: the issue form renders
                # `### Parts…` and the author's `### P1 — …` subheadings inside it must stay.
                if next_level < level or (next_level == level and next_name in known):
                    break
            content.append(following)
        return content
    return None


def programme_gaps(body: str) -> list[str]:
    """The programme fields this body is missing or leaves empty.

    `[]` means the section is present and all four rows carry a value. A body with no Programme
    heading returns every field name, so the report says what to add rather than only that
    something is absent. A row value of `—` (the template's ready-state placeholder) counts as
    empty: the template's own blank must not pass as a filled field.
    """
    content = section_lines(body, "Programme")
    if content is None:
        return list(PROGRAMME_FIELDS)
    gaps: list[str] = []
    found: set[str] = set()
    for line in content:
        # Template rows: `| Field | Value |`. The heading row and the separator are skipped by
        # the field-name match; a value that is the placeholder or blank is a gap.
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) != 2:
            continue
        field, value = cells
        if field not in PROGRAMME_FIELDS:
            continue
        found.add(field)
        if not value or value in {"—", "-"}:
            gaps.append(field)
    for field in PROGRAMME_FIELDS:
        if field not in found:
            gaps.append(field)
    return gaps


def _dor_evidence(line: str, criterion: str) -> str:
    """The text after the criterion's name and separator, or '' when the row has none."""
    match = re.search(rf"\b{criterion}\b", line, re.IGNORECASE)
    rest = line[match.end():] if match else line
    separator = re.search(r"[—–:\-]", rest)
    return rest[separator.end():].strip() if separator else rest.strip()


def _normalise_evidence(text: str) -> str:
    text = text.replace("`", "").replace("*", "").lower()
    text = re.sub(r"\s+", " ", text).strip()
    return text.strip(" .;:")


def _dor_states(body: str) -> dict[str, str]:
    """Per INVEST criterion: `missing`, `restatement` or `evidence`.

    The one derivation both the claim gate (`invest_gaps`) and the public readiness summary
    (`dor_criteria`) read, so a row can never be ready for one reader and not the other. A
    criterion is `missing` with no bold label, `restatement` when any labelled row carries the
    template's generic text (or its `<…>` placeholder), and `evidence` only when every labelled
    row names this unit's own facts.
    """
    content = section_lines(body, "Definition of ready")
    states = {criterion: "missing" for criterion in INVEST_CRITERIA}
    if content is None:
        return states
    for criterion, phrases in TEMPLATE_DOR_PHRASES:
        rows = [line for line in content if _criterion_line(line, criterion)]
        if not rows:
            continue
        state = "evidence"
        for line in rows:
            evidence = _dor_evidence(line, criterion)
            normalised = _normalise_evidence(evidence)
            if (
                not normalised
                or evidence.lstrip().startswith("<")
                or any(normalised == _normalise_evidence(phrase) for phrase in phrases)
            ):
                state = "restatement"
                break
        states[criterion] = state
    return states


def dor_criteria(body: str) -> dict[str, bool]:
    """The six INVEST tests, each evidenced by this unit's own facts (SPEC_V2 §5.8).

    The boolean view of `_dor_states` for readers that count rather than report: the public
    readiness summary publishes these, never the rows' text.
    """
    return {
        criterion.lower(): state == "evidence"
        for criterion, state in _dor_states(body).items()
    }


def invest_gaps(body: str) -> list[str]:
    """The INVEST criteria the `## Definition of ready` section does not name, or only restates.

    A missing section is the larger defect `contract_gaps` already reports; this names the
    criteria a present section omits, and the rows whose evidence is the template's restatement
    instead of this unit's own facts. A row may state the definition and then append the unit's
    facts — only the definition alone (or the template's `<…>` placeholder) is a gap.
    """
    if section_lines(body, "Definition of ready") is None:
        return []
    states = _dor_states(body)
    missing = [criterion for criterion in INVEST_CRITERIA if states[criterion] == "missing"]
    gaps = [f"the DoR does not name INVEST: {', '.join(missing)}"] if missing else []
    for criterion in INVEST_CRITERIA:
        if states[criterion] == "restatement":
            gaps.append(f"the DoR row for {criterion} is the template's restatement, not evidence")
    return gaps


def story_gaps(body: str) -> list[str]:
    """The `## User story` section, present and in `As a …, I want …, so that …` form."""
    content = section_lines(body, "User story")
    if content is None:
        return ["`User story` section"]
    text = "\n".join(content).strip()
    if not text:
        return ["`User story` section is empty"]
    if not STORY_RE.search(text):
        return ["a `User story` in `As a …, I want …, so that …` form"]
    return []


def _part_blocks(body: str) -> list[tuple[str, str]]:
    """(P<n>, block text) for every part in `Parts and acceptance criteria`, in order.

    A section with no `### P<n>` heading is one block named `P1`, matching `contract_gaps`'
    treatment of a single-part brief. An absent Parts section has no blocks: the section gap
    names the larger defect.
    """
    parts = section_lines(body, "Parts and acceptance criteria")
    if parts is None:
        return []
    text = "\n".join(parts)
    headings = list(PART_RE.finditer(text))
    if not headings:
        return [("P1", text)] if text.strip() else []
    blocks: list[tuple[str, str]] = []
    for index, match in enumerate(headings):
        end = headings[index + 1].start() if index + 1 < len(headings) else len(text)
        blocks.append((f"P{match.group(1)}", text[match.start() : end]))
    return blocks


def scenario_gaps(body: str) -> list[str]:
    """Parts without a Gherkin scenario, a `@verify` tag, or the three step keywords.

    A `Scenario:` line alone is not acceptance: each scenario needs `Given`, `When` and `Then`
    (the `@verify` tag is on the line above it, so the step check starts after the name line).
    """
    gaps: list[str] = []
    for part, block in _part_blocks(body):
        scenarios = list(SCENARIO_RE.finditer(block))
        tags = len(VERIFY_TAG_RE.findall(block))
        if not scenarios or tags < len(scenarios):
            gaps.append(f"the {part} part has no Gherkin scenario with @verify")
            continue
        for index, match in enumerate(scenarios):
            end = scenarios[index + 1].start() if index + 1 < len(scenarios) else len(block)
            segment = block[match.end() : end]
            missing = [
                keyword
                for keyword in ("Given", "When", "Then")
                if not re.search(rf"^\s*{keyword}\b", segment, re.IGNORECASE | re.MULTILINE)
            ]
            if missing:
                gaps.append(
                    f"the {part} part's scenario has no {'/'.join(missing)} step"
                )
    return gaps


def scenario_verify_ids(body: str) -> list[str]:
    """The distinct `@verify` ids of the parts' scenarios, in body order."""
    ids: list[str] = []
    for _part, block in _part_blocks(body):
        for value in VERIFY_TAG_RE.findall(block):
            if value not in ids:
                ids.append(value)
    return ids


def _cell_names(cell: str, value: str) -> bool:
    """Whether the cell names this `@verify` id as a whole token."""
    pattern = rf"(?<![{ID_CHARS}]){re.escape(value)}(?![{ID_CHARS}])"
    return re.search(pattern, cell) is not None


def dod_gaps(body: str) -> list[str]:
    """The acceptance-criteria rows the `## Definition of done` section is missing.

    The DoD is an acceptance-criteria table: one row per scenario, and every scenario's
    `@verify` id must appear in its row's first cell. A prose checklist is not the DoD the
    contract requires.
    """
    content = section_lines(body, "Definition of done")
    if content is None:
        return []
    first_cells: list[str] = []
    header_seen = False
    has_verify = False
    for line in content:
        stripped = line.strip()
        if not stripped.startswith("|"):
            continue
        cells = [cell.strip() for cell in stripped.strip("|").split("|")]
        if cells and all(re.fullmatch(r":?-{3,}:?", cell) for cell in cells):
            continue
        if not header_seen:
            header_seen = True
            has_verify = any("verify" in cell.lower() for cell in cells)
            continue
        first_cells.append(cells[0] if cells else "")
    gaps: list[str] = []
    if not has_verify:
        gaps.append("the DoD table names no `Verify:` column")
    for verify_id in scenario_verify_ids(body):
        if not any(_cell_names(cell, verify_id) for cell in first_cells):
            gaps.append(f"the DoD has no row for `{verify_id}`")
    return gaps


def _dod_counts(body: str) -> dict[str, int]:
    """The DoD table's data rows, and how many carry a filled evidence cell.

    Counts every table in the section, each table's first row as its header (the `Evidence`
    column when a header names one, else the second cell). A cell still reading `to fill`, `—`
    or `-` is unfilled: a template row must not read as evidence.
    """
    content = section_lines(body, "Definition of done")
    if content is None:
        return {"rows": 0, "evidenced": 0}
    tables: list[list[list[str]]] = []
    current: list[list[str]] = []
    for line in content:
        stripped = line.strip()
        if stripped.startswith("|"):
            current.append([cell.strip() for cell in stripped.strip("|").split("|")])
        elif current:
            tables.append(current)
            current = []
    if current:
        tables.append(current)
    rows = 0
    evidenced = 0
    for table in tables:
        header = table[0] if table else []
        evidence_index = next(
            (index for index, cell in enumerate(header) if "evidence" in cell.lower()), 1
        )
        for cells in table[1:]:
            if cells and all(re.fullmatch(r":?-{3,}:?", cell) for cell in cells):
                continue
            rows += 1
            value = cells[evidence_index] if evidence_index < len(cells) else ""
            normalised = _normalise_evidence(value)
            if normalised and normalised not in UNDONE_DOD_CELLS:
                evidenced += 1
    return {"rows": rows, "evidenced": evidenced}


def readiness_summary(body: str) -> dict:
    """The public-safe readiness of one unit body (SPEC_V2 §5.8, D69).

    Fixed-vocabulary values only — counts and booleans — so the published registry can show
    whether a unit carries the story/Gherkin/INVEST/DoD standard without a word of the body.
    `parts` counts the `### P` blocks, `scenarios` the `Scenario:` lines across the Parts
    section, and `verified` caps each part's `@verify` tags at its scenario count (the same
    measure `scenario_gaps` refuses below). The DoR booleans and DoD counts share the gate's
    own derivation, so the public record and the claim gate can never disagree.
    """
    parts_text = "\n".join(section_lines(body, "Parts and acceptance criteria") or [])
    scenarios = 0
    verified = 0
    for _part, block in _part_blocks(body):
        count = len(SCENARIO_RE.findall(block))
        scenarios += count
        verified += min(count, len(VERIFY_TAG_RE.findall(block)))
    return {
        "schema": READINESS_SCHEMA,
        "story": not story_gaps(body),
        "parts": len(PART_RE.findall(parts_text)),
        "scenarios": scenarios,
        "verified": verified,
        "dor": dor_criteria(body),
        "dod": _dod_counts(body),
    }


def contract_gaps(
    body: str, require_invest: bool = False, require_story: bool = False
) -> list[str]:
    """Every section or field the issue content contract requires and this body lacks.

    A gap names the missing thing, not a rule number: the report is posted on the issue, where a
    reader has the body in front of them. `require_invest` and `require_story` are the
    forward-only switches; the CLI derives both from the issue's creation date.
    """
    gaps: list[str] = []
    for name in CONTRACT_SECTIONS:
        # The story is conditionally required; `story_gaps` owns its presence and shape so an
        # issue created before STORY_SINCE is never made unready by this loop.
        if name == "User story" and not require_story:
            continue
        content = section_lines(body, name)
        if content is None:
            gaps.append(f"`{name}` section")
        elif not "\n".join(content).strip():
            gaps.append(f"`{name}` section is empty")
    missing = programme_gaps(body)
    if missing:
        gaps.append(f"`Programme` rows: {', '.join(missing)}")
    parts = section_lines(body, "Parts and acceptance criteria")
    # One `Verify:` per `###` part — 03-DELIVERY's DoR says the acceptance is per part, and it
    # is the rule `DeliveryModel.lean`'s `partsVerified` certifies. A body with no `###` part is
    # measured as one block, so a single-part brief still needs its line; a body with no Parts
    # section lacks it too (the section gap above names the larger defect).
    if parts is None:
        gaps.append("a `Verify:` line in every part of `Parts and acceptance criteria`")
    else:
        blocks = re.split(r"(?m)^###\s+", "\n".join(parts))[1:] or ["\n".join(parts)]
        if not all(
            any(VERIFY_RE.match(line) for line in block.splitlines()) for block in blocks
        ):
            gaps.append("a `Verify:` line in every part of `Parts and acceptance criteria`")
    if require_invest:
        gaps.extend(invest_gaps(body))
        gaps.extend(dod_gaps(body))
    if require_story:
        gaps.extend(story_gaps(body))
        gaps.extend(scenario_gaps(body))
    return gaps
