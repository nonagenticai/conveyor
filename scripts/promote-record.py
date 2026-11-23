#!/usr/bin/env python3
"""Promote the staging programme record into the production record.

The record was created in a staging copy of production (a production dump migrated to
0073) and is described by ``docs/evidence/record/journal.jsonl``.  After the deploy has
applied the migrations, this script replays that journal through the *same product
actions*, idempotently, and writes ``docs/evidence/record/promotion.json`` with the
staging-id -> production-id map and a per-entry verdict.

Modes
-----
``--check``      offline: every entry names a known action, its input file exists and
                 parses, ``output_id``/``natural_key`` are present and the natural key
                 follows the grammar.  Duplicate natural keys are refused.
``--dry-run``    offline: print ``{action, natural_key, will_create}`` for every entry.
                 No network, no database.
``--apply``      read production through the product read actions, skip an entry whose
                 natural key already exists, invoke the missing action through
                 ``POST /api/actions/<name>/invocations``, read every created row back,
                 and write ``promotion.json``.
``--self-test``  fully offline: fixture journal, fake rows and a fake action client.

Read surfaces
-------------
* Registers: ``registers.list`` with ``programme_id`` (programme-level risks).  A
  ``changes.request`` row is project-scoped, so the project's register is also read with
  each ``project_id`` named in a change input.
* Benefits: ``benefits.list`` with ``programme_id``.
* Approvals: ``approvals.list`` filtered by the change's ``project_id``.
* Measurements: ``benefits.measurement_list`` (the action exists on this lane).  When the
  target image predates it, ``--apply`` falls back to ``--snapshot DIR`` and the MSP export
  (``msp.json``); with neither, the verdict is ``readback_failed`` and the command exits
  non-zero.  It is never reported as a pass.

Natural keys and existence
--------------------------
``risk:<code>``
    a register item with ``kind='risk'`` and that ``code``.
``benefit:<reference>``
    a benefit profile with that ``reference``.
``benefit:<reference>/baseline``
    a measurement with ``kind='baseline'`` on that profile.
``change:<title>``
    a ``kind='change'`` register item.  The journal's key suffix is accepted as a label
    as well as the action input's title, so a promoted change is not created twice.
``approval:<gate>/<change>``
    an approval with that ``gate`` whose target/description/proposed input names the
    change (the journal writes ``approval:change:<label>``; both separators are legal).
``benefit:<reference>/fields``, ``/status``
    the profile named by the reference *already reflects* the patch.  Row existence alone
    would skip the first run, because B1-B6 pre-exist in the production dump; the update's
    own effect is the idempotency test.  Likewise a decided gate, not merely an open gate,
    is an existing approval decision.

Staging ids are remapped from earlier action outputs: ``changes.request`` returns
``change_id`` and ``approval_id``; ``benefits.create`` returns ``id``; measurements,
registers and updates return ``id``.  Programme, project, baseline and job ids are
production ids from the dump and are never rewritten.

The journal is never modified.  ``--apply`` only reads it; the write target is the
``promotion.json`` given by ``--output``.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_JOURNAL = "docs/evidence/record/journal.jsonl"
DEFAULT_PROMOTION = "docs/evidence/record/promotion.json"
JOURNAL_SCHEMA = "conveyor-record-journal/v1"
PROMOTION_SCHEMA = "conveyor-record-promotion/v1"

# The journal's write actions.  A read action in an entry is malformed.
WRITE_ACTIONS = {
    "registers.create",
    "benefits.create",
    "benefits.measure",
    "changes.request",
    "approvals.decide",
    "benefits.update",
    "documents.compose",
    "documents.publish",
    # The closure family: the retrospective rework's close chains (T13/T10/T9/T4 corpus).
    "projects.close_request",
    "approvals.request",
    "projects.close",
    "cards.comment",
    "cards.move",
    # The T3 record-construction family: the retrospective rework charters a project that
    # already exists (`projects.charter`), then plans it (`projects.baseline` writes the WBS,
    # the scope baseline and the gate that freezes it). Without these two the change that
    # accepts T3's deliverables has no approved scope baseline to cite.
    "projects.charter",
    "projects.baseline",
    # A rework deliverable the record accepted, with its typed evidence: the demo credit that
    # follows it is refused until the WBS row is `accepted` and carries evidence.
    "wbs.update",
    # The tr3 record family: a register risk edited in place, a benefits-map edge, and the
    # architecture decision register's rows.
    "registers.update",
    "benefits.link",
    "decisions.create",
    "decisions.transition",
    # The tr3 SAFe cadence family: the cadence rows themselves, their metas, the plan's gate,
    # the demo credits, the retro and its lessons, and the tranche review chain.
    "cadences.create",
    "cadences.update",
    "cadences.approve_plan",
    "cadences.credit_demo",
    "retros.record_findings",
    "retros.close",
    "lessons.propose",
    "tranches.review",
    "tranches.authorise",
    # The architecture compliance review: one row per contract, and the table is unique per
    # (project, contract), so the replay reads the row it already wrote before recording one.
    "architecture.reviews_record",
    # The TOGAF artefact rows the close gate requires. The create is idempotent per
    # (project, type); the approve names the create's staging id, which the replay maps.
    "architecture.artifacts_create",
    "architecture.artifacts_approve",
    # The governed-asset family (SPEC_V2 §8.3, §10.4): the MSP programme artefacts no fixed
    # family owns are composed on the programme-definition project and stepped through
    # draft → in_review → approved. Without these three the MSP rows cannot replay at all.
    "governed_assets.compose",
    "governed_assets.submit",
    "governed_assets.approve",
}

# The ADR register's 57 source rows are NOT journaled one entry per row: the seed
# (`scripts/seed-adr-register.py`) is already idempotent and writes through `decisions.*`, so
# the promotion runs it against production with the approval id from the journal's
# `change:Seed the architecture decision register` entry. Journaling 57 creates + transitions
# would duplicate the seed's own sequencing (supersession included) without adding a check.

# Product read actions used by --apply.
READ_ACTIONS = {
    "registers.list",
    "registers.get",
    "benefits.list",
    "benefits.get",
    "approvals.list",
    "documents.get",
    "projects.get",
    "cards.get",
    "lists.list",
    "pm.get",
    "benefits.link_list",
    "documents.list",
    "decisions.list",
    "decisions.get",
    "cadences.list",
    "cadences.credit_list",
    "lessons.list",
    "tranches.list",
    "tranches.review_list",
    "cards.list",
    "wbs.list",
    "baselines.list",
    "architecture.artifacts_list",
    "architecture.artifacts_get",
}

# The release this script ships with has no action that lists measurements.  If one is
# added later, `load_state` prefers it over the snapshot.  Kept as a constant so the
# absence is visible rather than implicit.
MEASUREMENT_LIST_ACTION = "benefits.measurement_list"

TOKEN = r"[A-Za-z0-9][A-Za-z0-9._-]*"
RISK_KEY_RE = re.compile(rf"^risk:({TOKEN})(?:/(update))?$")
BENEFIT_KEY_RE = re.compile(
    rf"^benefit:({TOKEN})(?:/(baseline|fields|status|preconditions|baseline-fields))?$"
)
CHANGE_KEY_RE = re.compile(r"^change:(.+)$")
APPROVAL_KEY_RE = re.compile(r"^approval:([^/:]+)[/:](.+)$")
DOCUMENT_KEY_RE = re.compile(r"^document:(.+)$")
DOCUMENT_PUBLISH_KEY_RE = re.compile(r"^document-publish:(.+)$")
PROJECT_CLOSURE_KEY_RE = re.compile(r"^project:closure:([0-9a-f-]{36})$")
PROJECT_CHARTER_KEY_RE = re.compile(r"^project:charter:([0-9a-f-]{36})$")
PROJECT_BASELINE_KEY_RE = re.compile(r"^project:baseline:([0-9a-f-]{36})$")
# A title carries colons ("The PM request-to-plan path: pm.plan writes the WBS"), so the
# project id is the only fixed delimiter and the title is everything after it.
WBS_ACCEPT_KEY_RE = re.compile(r"^wbs-accept:([0-9a-f-]{36}):(.+)$")
# A card's Definition-of-Done decide. The request keys on `approval:card_done/<card>`; the
# decide carries its own form so the two acts of one gate cannot collide on a natural key.
CARD_DONE_DECIDE_KEY_RE = re.compile(r"^card-done-decide:([0-9a-f-]{36})$")
REVIEW_KEY_RE = re.compile(r"^review:([0-9a-f-]{36})$")
PROJECT_CLOSE_KEY_RE = re.compile(r"^project:close:([0-9a-f-]{36})$")
CARD_COMMENT_KEY_RE = re.compile(r"^card-comment:([0-9a-f-]{36}):([0-9a-f]{8,64})$")
CARD_MOVE_KEY_RE = re.compile(r"^card-move:([0-9a-f-]{36}):(intake|active|waiting|done|archive)$")
ARTIFACT_KEY_RE = re.compile(r"^architecture-artifact:([0-9a-f-]{36}):([0-9a-f-]{36})$")
ARTIFACT_APPROVE_KEY_RE = re.compile(r"^architecture-artifact-approve:([0-9a-f-]{36})$")
BENEFIT_LINK_KEY_RE = re.compile(rf"^benefit-link:({TOKEN}):(.+)$")
DECISION_KEY_RE = re.compile(rf"^decision:({TOKEN})(?:/(draft|in_review|approved|superseded|archived))?$")
# The governed-asset family (SPEC_V2 §8.3).  Compose keys by its content — kind, owning
# project, title — because the staging id is not portable.  A lifecycle act keys by the
# asset id, with the act itself as the suffix: submit and approve are two distinct writes on
# the one asset, and a bare id would collide on the duplicate-key check and collapse their
# verdicts into one.
GOVERNED_ASSET_KEY_RE = re.compile(rf"^governed-asset:({TOKEN}):([0-9a-f-]{{36}}):(.+)$")
GOVERNED_ASSET_LIFECYCLE_KEY_RE = re.compile(
    rf"^governed-asset:({TOKEN})/(submit|approve)$"
)
# The tr3 SAFe family.  `cadence:<period>/<kind>` is the row; `/meta` is the update that puts the
# PI plan, the refinement and the retro's board into its meta.  The tranche chain keys carry the
# tranche NUMBER, because the staging tranche id is not portable and production has its own rows.
CADENCE_KEY_RE = re.compile(rf"^cadence:({TOKEN})/(sprint|pi|demo|retro|refinement)(?:/(meta))?$")
PI_PLAN_KEY_RE = re.compile(rf"^pi-plan:({TOKEN})$")
DEMO_CREDIT_KEY_RE = re.compile(rf"^demo-credit:({TOKEN}):(.+)$")
RETRO_KEY_RE = re.compile(rf"^retro:({TOKEN})(?:/(close))?$")
LESSON_KEY_RE = re.compile(rf"^lesson:({TOKEN}):({TOKEN})$")
TRANCHE_REVIEW_GATE_KEY_RE = re.compile(rf"^tranche-review-gate:(\d+)$")
TRANCHE_REVIEW_DECIDE_KEY_RE = re.compile(rf"^tranche-review-decide:(\d+)$")
TRANCHE_REVIEW_KEY_RE = re.compile(rf"^tranche-review:(\d+)$")
TRANCHE_AUTHORISE_KEY_RE = re.compile(rf"^tranche-authorise:(\d+)$")

# The product's own gate/target grammar, mirrored here so a matcher compares what the engine
# compares.  `cadences.approve_plan` opens its gate itself; a tranche review needs a gate whose
# target is `tranche/<id>/review` (crates/actions/src/spine/mod.rs::target_tranches_review).
PI_PLAN_GATE = "cadences.approve_plan"
TRANCHE_REVIEW_GATE = "tranches.review"


class PromoteError(Exception):
    """A refused check, plan or apply."""


class ReadbackError(PromoteError):
    """A row was created but the product could not read it back."""


# ── journal parsing ──────────────────────────────────────────────────────────────────────


@dataclass
class NaturalKey:
    kind: str
    first: str
    second: str | None = None

    def canonical(self) -> str:
        if self.kind == "risk":
            return f"risk:{self.first}" + (f"/{self.second}" if self.second else "")
        if self.kind == "benefit":
            return f"benefit:{self.first}" + (f"/{self.second}" if self.second else "")
        if self.kind == "change":
            return f"change:{self.first}"
        if self.kind == "approval":
            return f"approval:{self.first}/{self.second}"
        if self.kind == "document":
            return f"document:{self.first}"
        if self.kind == "document_publish":
            return f"document-publish:{self.first}"
        if self.kind == "project_closure":
            return f"project:closure:{self.first}"
        if self.kind == "project_charter":
            return f"project:charter:{self.first}"
        if self.kind == "project_baseline":
            return f"project:baseline:{self.first}"
        if self.kind == "wbs_accept":
            return f"wbs-accept:{self.first}:{self.second}"
        if self.kind == "card_done_decide":
            return f"card-done-decide:{self.first}"
        if self.kind == "review":
            return f"review:{self.first}"
        if self.kind == "project_close":
            return f"project:close:{self.first}"
        if self.kind == "card_comment":
            return f"card-comment:{self.first}:{self.second}"
        if self.kind == "card_move":
            return f"card-move:{self.first}:{self.second}"
        if self.kind == "architecture_artifact":
            return f"architecture-artifact:{self.first}:{self.second}"
        if self.kind == "architecture_artifact_approve":
            return f"architecture-artifact-approve:{self.first}"
        if self.kind == "benefit_link":
            return f"benefit-link:{self.first}:{self.second}"
        if self.kind == "decision":
            return f"decision:{self.first}" + (f"/{self.second}" if self.second else "")
        if self.kind == "governed_asset":
            return f"governed-asset:{self.first}:{self.second}"
        if self.kind == "governed_asset_lifecycle":
            return f"governed-asset:{self.first}/{self.second}"
        if self.kind == "cadence":
            return f"cadence:{self.first}/{self.second}"
        if self.kind == "cadence_meta":
            return f"cadence:{self.first}/{self.second}/meta"
        if self.kind == "pi_plan":
            return f"pi-plan:{self.first}"
        if self.kind == "demo_credit":
            return f"demo-credit:{self.first}:{self.second}"
        if self.kind == "retro":
            return f"retro:{self.first}" + (f"/{self.second}" if self.second else "")
        if self.kind == "lesson":
            return f"lesson:{self.first}:{self.second}"
        if self.kind == "tranche_review_gate":
            return f"tranche-review-gate:{self.first}"
        if self.kind == "tranche_review_decide":
            return f"tranche-review-decide:{self.first}"
        if self.kind == "tranche_review":
            return f"tranche-review:{self.first}"
        if self.kind == "tranche_authorise":
            return f"tranche-authorise:{self.first}"
        raise PromoteError(f"unknown natural-key kind {self.kind!r}")


def parse_natural_key(value: str) -> NaturalKey:
    if not isinstance(value, str) or not value:
        raise PromoteError("natural_key must be a non-empty string")
    match = RISK_KEY_RE.match(value)
    if match:
        return NaturalKey("risk", match.group(1), match.group(2))
    match = GOVERNED_ASSET_KEY_RE.match(value)
    if match:
        title = match.group(3).strip()
        if not title or any(ord(char) < 0x20 for char in title):
            raise PromoteError(f"malformed governed-asset natural key: {value!r}")
        # The project id is the key's fixed second part; the kind is the first, so a keyed
        # compose row and its input can be checked against each other in validate_entries.
        return NaturalKey(
            "governed_asset", f"{match.group(1)}:{match.group(2)}", title
        )
    match = GOVERNED_ASSET_LIFECYCLE_KEY_RE.match(value)
    if match:
        return NaturalKey("governed_asset_lifecycle", match.group(1), match.group(2))
    match = DECISION_KEY_RE.match(value)
    if match:
        return NaturalKey("decision", match.group(1), match.group(2))
    match = CADENCE_KEY_RE.match(value)
    if match:
        kind = "cadence_meta" if match.group(3) else "cadence"
        return NaturalKey(kind, match.group(1), match.group(2))
    match = TRANCHE_REVIEW_GATE_KEY_RE.match(value)
    if match:
        return NaturalKey("tranche_review_gate", match.group(1))
    match = TRANCHE_REVIEW_DECIDE_KEY_RE.match(value)
    if match:
        return NaturalKey("tranche_review_decide", match.group(1))
    match = TRANCHE_REVIEW_KEY_RE.match(value)
    if match:
        return NaturalKey("tranche_review", match.group(1))
    match = TRANCHE_AUTHORISE_KEY_RE.match(value)
    if match:
        return NaturalKey("tranche_authorise", match.group(1))
    match = PI_PLAN_KEY_RE.match(value)
    if match:
        return NaturalKey("pi_plan", match.group(1))
    match = DEMO_CREDIT_KEY_RE.match(value)
    if match:
        title = match.group(2).strip()
        if not title or any(ord(char) < 0x20 for char in title):
            raise PromoteError(f"malformed demo-credit natural key: {value!r}")
        return NaturalKey("demo_credit", match.group(1), title)
    match = RETRO_KEY_RE.match(value)
    if match:
        return NaturalKey("retro", match.group(1), match.group(2))
    match = LESSON_KEY_RE.match(value)
    if match:
        return NaturalKey("lesson", match.group(1), match.group(2))
    match = BENEFIT_LINK_KEY_RE.match(value)
    if match:
        return NaturalKey("benefit_link", match.group(1), match.group(2).strip())
    match = BENEFIT_KEY_RE.match(value)
    if match:
        return NaturalKey("benefit", match.group(1), match.group(2))
    match = APPROVAL_KEY_RE.match(value)
    if match:
        return NaturalKey("approval", match.group(1), match.group(2).strip())
    match = CHANGE_KEY_RE.match(value)
    if match:
        title = match.group(1).strip()
        if not title or any(ord(char) < 0x20 for char in title):
            raise PromoteError(f"malformed change natural key: {value!r}")
        return NaturalKey("change", title)
    match = DOCUMENT_PUBLISH_KEY_RE.match(value)
    if match:
        title = match.group(1).strip()
        if not title or any(ord(char) < 0x20 for char in title):
            raise PromoteError(f"malformed document publish natural key: {value!r}")
        return NaturalKey("document_publish", title)
    match = DOCUMENT_KEY_RE.match(value)
    if match:
        title = match.group(1).strip()
        if not title or any(ord(char) < 0x20 for char in title):
            raise PromoteError(f"malformed document natural key: {value!r}")
        return NaturalKey("document", title)
    match = PROJECT_CLOSURE_KEY_RE.match(value)
    if match:
        return NaturalKey("project_closure", match.group(1))
    match = PROJECT_CHARTER_KEY_RE.match(value)
    if match:
        return NaturalKey("project_charter", match.group(1))
    match = PROJECT_BASELINE_KEY_RE.match(value)
    if match:
        return NaturalKey("project_baseline", match.group(1))
    match = WBS_ACCEPT_KEY_RE.match(value)
    if match:
        title = match.group(2).strip()
        if not title or any(ord(char) < 0x20 for char in title):
            raise PromoteError(f"malformed wbs-accept natural key: {value!r}")
        return NaturalKey("wbs_accept", match.group(1), title)
    match = CARD_DONE_DECIDE_KEY_RE.match(value)
    if match:
        return NaturalKey("card_done_decide", match.group(1))
    match = REVIEW_KEY_RE.match(value)
    if match:
        return NaturalKey("review", match.group(1))
    match = PROJECT_CLOSE_KEY_RE.match(value)
    if match:
        return NaturalKey("project_close", match.group(1))
    match = CARD_COMMENT_KEY_RE.match(value)
    if match:
        return NaturalKey("card_comment", match.group(1), match.group(2))
    match = CARD_MOVE_KEY_RE.match(value)
    if match:
        return NaturalKey("card_move", match.group(1), match.group(2))
    match = ARTIFACT_APPROVE_KEY_RE.match(value)
    if match:
        return NaturalKey("architecture_artifact_approve", match.group(1))
    match = ARTIFACT_KEY_RE.match(value)
    if match:
        return NaturalKey("architecture_artifact", match.group(1), match.group(2))
    raise PromoteError(f"malformed natural key: {value!r}")


@dataclass
class Entry:
    lineno: int
    action: str
    natural_key: str
    output_id: str
    input_path: str
    payload: dict
    raw: dict
    key: NaturalKey | None = None
    # Resolution scope the action's own input cannot carry: the programme a cadence belongs to,
    # the project whose WBS names the demo's deliverable, the tranche number an approval chain
    # reviews.  Optional; validated as an object, read only by the matchers that need it.
    meta: dict = field(default_factory=dict)
    # Set on a journal row that cannot be replayed at all (an unknown action, a missing or
    # malformed natural key).  Such a row is carried as an Entry with `key=None` so it becomes
    # an `unreplayable_input` verdict in the promotion document — never a silent drop.
    unreplayable: str | None = None


def resolve_path(value: str) -> Path:
    path = Path(value)
    return path if path.is_absolute() else REPO_ROOT / path


def load_journal(path: Path) -> tuple[dict | None, list[tuple[int, dict]], list[str]]:
    if not path.is_file():
        return None, [], [f"journal not found: {path}"]
    header: dict | None = None
    raws: list[tuple[int, dict]] = []
    errors: list[str] = []
    for lineno, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not raw.strip():
            continue
        try:
            obj = json.loads(raw)
        except ValueError as error:
            errors.append(f"line {lineno}: not JSON: {error}")
            continue
        if not isinstance(obj, dict):
            errors.append(f"line {lineno}: entry must be an object")
            continue
        if header is None and "schema" in obj and "action" not in obj:
            header = obj
            continue
        raws.append((lineno, obj))
    return header, raws, errors


def unreplayable_entry(lineno: int, raw: dict, reason: str) -> Entry:
    """A journal row that cannot be replayed, carried so it becomes a verdict, not a drop."""
    def text(name: str) -> str:
        value = raw.get(name)
        return value if isinstance(value, str) else ""

    action = raw.get("action")
    return Entry(
        lineno=lineno,
        action=action if isinstance(action, str) else "",
        natural_key=text("natural_key"),
        output_id=text("output_id"),
        input_path=text("input"),
        payload={},
        raw=raw,
        key=None,
        unreplayable=reason,
    )


def build_entries(
    raws: list[tuple[int, dict]]
) -> tuple[list[Entry], list[Entry], list[str]]:
    """The replayable entries, the rows that cannot be replayed, and the hard errors."""
    entries: list[Entry] = []
    unreplayable: list[Entry] = []
    errors: list[str] = []
    for lineno, raw in raws:
        action = raw.get("action")
        if not isinstance(action, str) or action not in WRITE_ACTIONS:
            unreplayable.append(
                unreplayable_entry(lineno, raw, f"unknown action {action!r}")
            )
            continue
        natural_key = raw.get("natural_key")
        try:
            key = parse_natural_key(natural_key)
        except PromoteError as error:
            unreplayable.append(unreplayable_entry(lineno, raw, str(error)))
            continue
        output_id = raw.get("output_id")
        if not isinstance(output_id, str) or not output_id.strip():
            errors.append(f"line {lineno}: output_id is required")
            continue
        input_path = raw.get("input")
        if not isinstance(input_path, str) or not input_path.strip():
            errors.append(f"line {lineno}: input is required")
            continue
        path = resolve_path(input_path)
        try:
            payload = json.loads(path.read_text(encoding="utf-8"))
        except OSError as error:
            errors.append(f"line {lineno}: input {input_path}: {error}")
            continue
        except ValueError as error:
            errors.append(f"line {lineno}: input {input_path} is not JSON: {error}")
            continue
        if not isinstance(payload, dict):
            errors.append(f"line {lineno}: input {input_path} must be an object")
            continue
        meta = raw.get("meta")
        if meta is None:
            meta = {}
        if not isinstance(meta, dict):
            errors.append(f"line {lineno}: meta must be an object")
            continue
        entries.append(
            Entry(
                lineno=lineno,
                action=action,
                natural_key=natural_key,
                output_id=output_id,
                input_path=input_path,
                payload=payload,
                raw=raw,
                key=key,
                meta=meta,
            )
        )
    return entries, unreplayable, errors


def validate_entries(entries: list[Entry]) -> list[str]:
    errors: list[str] = []
    seen: dict[str, int] = {}
    for entry in entries:
        key = entry.key
        assert key is not None
        canonical = key.canonical()
        if canonical in seen:
            errors.append(
                f"line {entry.lineno}: duplicate natural key {canonical!r} "
                f"(first on line {seen[canonical]})"
            )
        else:
            seen[canonical] = entry.lineno

        if entry.action == "registers.create":
            if key.kind != "risk":
                errors.append(
                    f"line {entry.lineno}: registers.create key must be risk:<code>"
                )
                continue
            if entry.payload.get("kind") != "risk":
                errors.append(
                    f"line {entry.lineno}: registers.create natural key is a risk but "
                    f"input kind is {entry.payload.get('kind')!r}"
                )
            code = entry.payload.get("code")
            if code != key.first:
                errors.append(
                    f"line {entry.lineno}: natural key code {key.first!r} does not match "
                    f"input code {code!r}"
                )
        elif entry.action == "benefits.create":
            if key.kind != "benefit" or key.second is not None:
                errors.append(
                    f"line {entry.lineno}: benefits.create key must be benefit:<reference>"
                )
                continue
            if entry.payload.get("reference") != key.first:
                errors.append(
                    f"line {entry.lineno}: natural key reference {key.first!r} does not "
                    f"match input reference {entry.payload.get('reference')!r}"
                )
        elif entry.action == "benefits.measure":
            if key.kind != "benefit" or key.second != "baseline":
                errors.append(
                    f"line {entry.lineno}: benefits.measure key must be "
                    "benefit:<reference>/baseline"
                )
                continue
            if entry.payload.get("kind") != "baseline":
                errors.append(
                    f"line {entry.lineno}: benefits.measure key is a baseline but input "
                    f"kind is {entry.payload.get('kind')!r}"
                )
        elif entry.action == "changes.request":
            if key.kind != "change":
                errors.append(
                    f"line {entry.lineno}: changes.request key must be change:<title>"
                )
                continue
            if not isinstance(entry.payload.get("title"), str) or not entry.payload["title"]:
                errors.append(f"line {entry.lineno}: change input has no title")
        elif entry.action == "approvals.decide":
            if key.kind == "card_done_decide":
                if entry.payload.get("decision") not in ("approve", "reject"):
                    errors.append(
                        f"line {entry.lineno}: approval decision must be approve or reject"
                    )
                if not entry.payload.get("approval_id"):
                    errors.append(
                        f"line {entry.lineno}: approval input has no approval_id"
                    )
                continue
            if key.kind == "tranche_review_decide":
                if entry.payload.get("decision") not in ("approve", "reject"):
                    errors.append(
                        f"line {entry.lineno}: approval decision must be approve or reject"
                    )
                if not entry.payload.get("approval_id"):
                    errors.append(
                        f"line {entry.lineno}: approval input has no approval_id"
                    )
                continue
            if key.kind != "approval":
                errors.append(
                    f"line {entry.lineno}: approvals.decide key must be "
                    "approval:<gate>/<change>"
                )
                continue
            if entry.payload.get("decision") not in ("approve", "reject"):
                errors.append(
                    f"line {entry.lineno}: approval decision must be approve or reject"
                )
            if not entry.payload.get("approval_id"):
                errors.append(f"line {entry.lineno}: approval input has no approval_id")
        elif entry.action == "benefits.update":
            if key.kind != "benefit" or key.second not in (
                "fields",
                "status",
                "preconditions",
                "baseline-fields",
            ):
                errors.append(
                    f"line {entry.lineno}: benefits.update key must be "
                    "benefit:<reference>/fields, /status, /preconditions or "
                    "/baseline-fields"
                )
                continue
            if not entry.payload.get("id"):
                errors.append(f"line {entry.lineno}: update input has no id")
            if key.second == "status":
                if not entry.payload.get("status"):
                    errors.append(
                        f"line {entry.lineno}: status update input has no status"
                    )
            elif key.second == "preconditions":
                if not isinstance(entry.payload.get("preconditions"), list):
                    errors.append(
                        f"line {entry.lineno}: a preconditions update needs a list"
                    )
            elif key.second == "baseline-fields":
                if "baseline" not in entry.payload:
                    errors.append(
                        f"line {entry.lineno}: a baseline update needs the baseline field"
                    )
            elif entry.payload.get("status") is not None:
                errors.append(
                    f"line {entry.lineno}: fields update must not change status"
                )
        elif entry.action == "registers.update":
            if key.kind != "risk" or key.second != "update":
                errors.append(
                    f"line {entry.lineno}: registers.update key must be risk:<code>/update"
                )
                continue
            if not entry.payload.get("id"):
                errors.append(f"line {entry.lineno}: update input has no id")
            if not any(
                entry.payload.get(name) is not None
                for name in ("owner_job_id", "review_at", "probability", "impact", "links")
            ):
                errors.append(
                    f"line {entry.lineno}: registers.update names no field to change"
                )
        elif entry.action == "benefits.link":
            if key.kind != "benefit_link":
                errors.append(
                    f"line {entry.lineno}: benefits.link key must be "
                    "benefit-link:<benefit reference>:<output ref>"
                )
                continue
            for name in ("programme_id", "benefit_id", "output_ref", "benefit_ref"):
                if not entry.payload.get(name):
                    errors.append(
                        f"line {entry.lineno}: benefits.link input has no {name}"
                    )
            if key.first != entry.payload.get("benefit_ref"):
                errors.append(
                    f"line {entry.lineno}: natural key {key.first!r} does not match "
                    f"input benefit_ref {entry.payload.get('benefit_ref')!r}"
                )
        elif entry.action == "decisions.create":
            if key.kind != "decision" or key.second is not None:
                errors.append(
                    f"line {entry.lineno}: decisions.create key must be decision:<key>"
                )
                continue
            if entry.payload.get("key") != key.first:
                errors.append(
                    f"line {entry.lineno}: natural key {key.first!r} does not match "
                    f"input key {entry.payload.get('key')!r}"
                )
            if not entry.payload.get("title"):
                errors.append(f"line {entry.lineno}: decisions.create input has no title")
        elif entry.action == "decisions.transition":
            if key.kind != "decision" or key.second is None:
                errors.append(
                    f"line {entry.lineno}: decisions.transition key must be "
                    "decision:<key>/<status>"
                )
                continue
            if entry.payload.get("to") != key.second:
                errors.append(
                    f"line {entry.lineno}: natural key status {key.second!r} does not "
                    f"match input to {entry.payload.get('to')!r}"
                )
            if not entry.payload.get("id"):
                errors.append(f"line {entry.lineno}: transition input has no id")
        elif entry.action == "governed_assets.compose":
            if key.kind != "governed_asset":
                errors.append(
                    f"line {entry.lineno}: governed_assets.compose key must be "
                    "governed-asset:<kind>:<project id>:<title>"
                )
                continue
            kind, _, project_id = key.first.partition(":")
            if entry.payload.get("kind") != kind:
                errors.append(
                    f"line {entry.lineno}: natural key kind {kind!r} does not match "
                    f"input kind {entry.payload.get('kind')!r}"
                )
            if entry.payload.get("project_id") != project_id:
                errors.append(
                    f"line {entry.lineno}: natural key project {project_id!r} does not "
                    f"match input project_id {entry.payload.get('project_id')!r}"
                )
            if entry.payload.get("title") != key.second:
                errors.append(
                    f"line {entry.lineno}: natural key title {key.second!r} does not match "
                    f"input title {entry.payload.get('title')!r}"
                )
        elif entry.action in ("governed_assets.submit", "governed_assets.approve"):
            act = entry.action.rsplit(".", 1)[-1]
            if key.kind != "governed_asset_lifecycle" or key.second != act:
                errors.append(
                    f"line {entry.lineno}: {entry.action} key must be "
                    f"governed-asset:<asset id>/{act}"
                )
                continue
            if entry.payload.get("id") != key.first:
                errors.append(
                    f"line {entry.lineno}: natural key {key.first!r} does not match "
                    f"input id {entry.payload.get('id')!r}"
                )
        elif entry.action == "projects.close_request":
            if key.kind != "project_closure":
                errors.append(
                    f"line {entry.lineno}: projects.close_request key must be "
                    "project:closure:<project id>"
                )
                continue
            if entry.payload.get("id") != key.first:
                errors.append(
                    f"line {entry.lineno}: natural key {key.first!r} does not match "
                    f"input id {entry.payload.get('id')!r}"
                )
        elif entry.action in ("projects.charter", "projects.baseline"):
            wanted = "project_charter" if entry.action == "projects.charter" else "project_baseline"
            shape = (
                "project:charter:<project id>"
                if entry.action == "projects.charter"
                else "project:baseline:<project id>"
            )
            if key.kind != wanted:
                errors.append(
                    f"line {entry.lineno}: {entry.action} key must be {shape}"
                )
                continue
            if entry.payload.get("project_id") != key.first:
                errors.append(
                    f"line {entry.lineno}: natural key {key.first!r} does not match "
                    f"input project_id {entry.payload.get('project_id')!r}"
                )
            if entry.action == "projects.charter":
                if not isinstance(entry.payload.get("charter"), str) or not entry.payload[
                    "charter"
                ].strip():
                    errors.append(
                        f"line {entry.lineno}: a charter input needs non-empty charter text"
                    )
            else:
                wbs = entry.payload.get("wbs")
                if not isinstance(wbs, list) or not wbs:
                    errors.append(
                        f"line {entry.lineno}: a baseline input needs a non-empty wbs list"
                    )
                else:
                    for index, node in enumerate(wbs):
                        if not isinstance(node, dict) or not node.get("kind") or not node.get(
                            "title"
                        ):
                            errors.append(
                                f"line {entry.lineno}: wbs node {index} needs kind and title"
                            )
        elif entry.action == "architecture.reviews_record":
            if key.kind != "review":
                errors.append(
                    f"line {entry.lineno}: architecture.reviews_record key must be "
                    "review:<contract id>"
                )
                continue
            if entry.payload.get("contract_id") != key.first:
                errors.append(
                    f"line {entry.lineno}: natural key {key.first!r} does not match "
                    f"input contract_id {entry.payload.get('contract_id')!r}"
                )
            if not entry.payload.get("level"):
                errors.append(f"line {entry.lineno}: a review needs a level")
            if not entry.payload.get("project_id"):
                errors.append(f"line {entry.lineno}: a review needs a project_id")
        elif entry.action == "architecture.artifacts_create":
            if key.kind != "architecture_artifact":
                errors.append(
                    f"line {entry.lineno}: architecture.artifacts_create key must be "
                    "architecture-artifact:<project id>:<artifact type id>"
                )
                continue
            if str(entry.payload.get("project_id")) != key.first:
                errors.append(
                    f"line {entry.lineno}: natural key {key.first!r} does not match "
                    f"input project_id {entry.payload.get('project_id')!r}"
                )
            if str(entry.payload.get("artifact_type_id")) != key.second:
                errors.append(
                    f"line {entry.lineno}: natural key {key.second!r} does not match "
                    f"input artifact_type_id {entry.payload.get('artifact_type_id')!r}"
                )
        elif entry.action == "architecture.artifacts_approve":
            if key.kind != "architecture_artifact_approve":
                errors.append(
                    f"line {entry.lineno}: architecture.artifacts_approve key must be "
                    "architecture-artifact-approve:<staging artifact id>"
                )
                continue
            if str(entry.payload.get("id")) != key.first:
                errors.append(
                    f"line {entry.lineno}: natural key {key.first!r} does not match "
                    f"input id {entry.payload.get('id')!r}"
                )
        elif entry.action == "wbs.update":
            if key.kind != "wbs_accept":
                errors.append(
                    f"line {entry.lineno}: wbs.update key must be "
                    "wbs-accept:<project id>:<deliverable title>"
                )
                continue
            if entry.payload.get("status") != "accepted":
                errors.append(
                    f"line {entry.lineno}: a rework WBS update records `accepted`"
                )
            evidence = entry.payload.get("evidence")
            if not isinstance(evidence, list) or not evidence:
                errors.append(
                    f"line {entry.lineno}: an accepted deliverable needs typed evidence"
                )
        elif entry.action == "approvals.request":
            if key.kind == "tranche_review_gate":
                # The tranche review's gate: the request names the review action and the input
                # the engine computes `tranche/<id>/review` from; there is no evidence list.
                if entry.payload.get("gate") != TRANCHE_REVIEW_GATE:
                    errors.append(
                        f"line {entry.lineno}: a tranche-review gate must be named "
                        f"{TRANCHE_REVIEW_GATE!r}"
                    )
                if entry.payload.get("action") != TRANCHE_REVIEW_GATE:
                    errors.append(
                        f"line {entry.lineno}: a tranche-review gate must open "
                        f"{TRANCHE_REVIEW_GATE!r}"
                    )
                inner = entry.payload.get("input")
                if not isinstance(inner, dict) or not inner.get("id"):
                    errors.append(
                        f"line {entry.lineno}: the tranche-review gate input names no tranche id"
                    )
                continue
            if key.kind != "approval":
                errors.append(
                    f"line {entry.lineno}: approvals.request key must be approval:<gate>/<target>"
                )
                continue
            if key.first == "card_done":
                # A card's Definition-of-Done gate: the card is the target, and the engine
                # accepts it with typed evidence or the explicit waiver reason it stores.
                if entry.payload.get("gate") != "card_done":
                    errors.append(
                        f"line {entry.lineno}: a card-done gate must be named 'card_done'"
                    )
                if entry.payload.get("card_id") != key.second:
                    errors.append(
                        f"line {entry.lineno}: natural key {key.second!r} does not match "
                        f"input card_id {entry.payload.get('card_id')!r}"
                    )
                evidence = entry.payload.get("evidence")
                waiver = entry.payload.get("evidence_waiver_reason")
                if not (
                    isinstance(evidence, list) and evidence
                ) and not (isinstance(waiver, str) and waiver.strip()):
                    errors.append(
                        f"line {entry.lineno}: a card-done gate needs typed evidence or a waiver "
                        "reason"
                    )
                continue
            if not entry.payload.get("gate"):
                errors.append(f"line {entry.lineno}: approvals.request input has no gate")
            if not isinstance(entry.payload.get("evidence"), list) or not entry.payload["evidence"]:
                errors.append(
                    f"line {entry.lineno}: a destructive gate needs typed evidence"
                )
        elif entry.action == "projects.close":
            if key.kind != "project_close":
                errors.append(
                    f"line {entry.lineno}: projects.close key must be project:close:<project id>"
                )
                continue
            if entry.payload.get("id") != key.first:
                errors.append(
                    f"line {entry.lineno}: natural key {key.first!r} does not match "
                    f"input id {entry.payload.get('id')!r}"
                )
        elif entry.action == "cards.comment":
            if key.kind != "card_comment":
                errors.append(
                    f"line {entry.lineno}: cards.comment key must be card-comment:<card>:<sha>"
                )
                continue
            if entry.payload.get("id") != key.first:
                errors.append(
                    f"line {entry.lineno}: natural key {key.first!r} does not match "
                    f"input id {entry.payload.get('id')!r}"
                )
            body = entry.payload.get("body")
            if not isinstance(body, str) or not body:
                errors.append(f"line {entry.lineno}: comment input has no body")
            elif not hashlib.sha256(body.encode("utf-8")).hexdigest().startswith(key.second):
                errors.append(
                    f"line {entry.lineno}: natural key sha does not name the comment body"
                )
        elif entry.action == "cards.move":
            if key.kind != "card_move":
                errors.append(
                    f"line {entry.lineno}: cards.move key must be card-move:<card>:<lane kind>"
                )
                continue
            if entry.payload.get("id") != key.first:
                errors.append(
                    f"line {entry.lineno}: natural key {key.first!r} does not match "
                    f"input id {entry.payload.get('id')!r}"
                )
            if not entry.payload.get("list_id"):
                errors.append(f"line {entry.lineno}: move input has no list_id")
        elif entry.action == "cadences.create":
            if key.kind != "cadence":
                errors.append(
                    f"line {entry.lineno}: cadences.create key must be cadence:<period>/<kind>"
                )
                continue
            if entry.payload.get("period") != key.first:
                errors.append(
                    f"line {entry.lineno}: natural key period {key.first!r} does not match "
                    f"input period {entry.payload.get('period')!r}"
                )
            if entry.payload.get("kind") != key.second:
                errors.append(
                    f"line {entry.lineno}: natural key kind {key.second!r} does not match "
                    f"input kind {entry.payload.get('kind')!r}"
                )
            if not entry.payload.get("programme_id"):
                errors.append(f"line {entry.lineno}: cadence input has no programme_id")
        elif entry.action == "cadences.update":
            if key.kind != "cadence_meta":
                errors.append(
                    f"line {entry.lineno}: cadences.update key must be "
                    "cadence:<period>/<kind>/meta"
                )
                continue
            if not entry.payload.get("id"):
                errors.append(f"line {entry.lineno}: cadence update input has no id")
            if not isinstance(entry.payload.get("meta"), dict):
                errors.append(f"line {entry.lineno}: cadence update input has no meta object")
            if not entry.meta.get("programme_id"):
                errors.append(
                    f"line {entry.lineno}: cadence update names no meta.programme_id"
                )
        elif entry.action == "cadences.approve_plan":
            if key.kind != "pi_plan":
                errors.append(
                    f"line {entry.lineno}: cadences.approve_plan key must be pi-plan:<period>"
                )
                continue
            if not entry.payload.get("cadence_id"):
                errors.append(f"line {entry.lineno}: plan approval input has no cadence_id")
            if entry.payload.get("decision") not in (None, "approve", "reject"):
                errors.append(
                    f"line {entry.lineno}: plan approval decision must be approve or reject"
                )
            if not entry.meta.get("programme_id"):
                errors.append(f"line {entry.lineno}: plan approval names no meta.programme_id")
        elif entry.action == "cadences.credit_demo":
            if key.kind != "demo_credit":
                errors.append(
                    f"line {entry.lineno}: cadences.credit_demo key must be "
                    "demo-credit:<period>:<deliverable title>"
                )
                continue
            for name in ("cadence_id", "deliverable_id"):
                if not entry.payload.get(name):
                    errors.append(f"line {entry.lineno}: demo credit input has no {name}")
            if not entry.meta.get("programme_id") or not entry.meta.get("project_id"):
                errors.append(
                    f"line {entry.lineno}: demo credit names no meta.programme_id/project_id"
                )
        elif entry.action == "retros.record_findings":
            if key.kind != "retro" or key.second is not None:
                errors.append(
                    f"line {entry.lineno}: retros.record_findings key must be retro:<period>"
                )
                continue
            findings = entry.payload.get("findings")
            if not isinstance(findings, list) or not findings:
                errors.append(f"line {entry.lineno}: a retro needs at least one finding")
            elif any(
                not isinstance(item, dict) or not item.get("key") for item in findings
            ):
                errors.append(f"line {entry.lineno}: every finding needs a key")
            if not entry.payload.get("cadence_id"):
                errors.append(f"line {entry.lineno}: retro findings have no cadence_id")
        elif entry.action == "retros.close":
            if key.kind != "retro" or key.second != "close":
                errors.append(
                    f"line {entry.lineno}: retros.close key must be retro:<period>/close"
                )
                continue
            if not entry.payload.get("cadence_id"):
                errors.append(f"line {entry.lineno}: retro close has no cadence_id")
        elif entry.action == "lessons.propose":
            if key.kind != "lesson":
                errors.append(
                    f"line {entry.lineno}: lessons.propose key must be "
                    "lesson:<period>:<finding key>"
                )
                continue
            if entry.payload.get("finding_key") != key.second:
                errors.append(
                    f"line {entry.lineno}: natural key finding {key.second!r} does not "
                    f"match input finding_key {entry.payload.get('finding_key')!r}"
                )
            if not entry.payload.get("statement"):
                errors.append(f"line {entry.lineno}: a lesson needs a statement")
            if not isinstance(entry.payload.get("evidence"), list):
                errors.append(f"line {entry.lineno}: a lesson's evidence must be a list")
            if not entry.meta.get("programme_id"):
                errors.append(f"line {entry.lineno}: a lesson names no meta.programme_id")
        elif entry.action == "tranches.review":
            if key.kind != "tranche_review":
                errors.append(
                    f"line {entry.lineno}: tranches.review key must be tranche-review:<number>"
                )
                continue
            if entry.payload.get("decision") not in ("continue", "pause", "stop"):
                errors.append(
                    f"line {entry.lineno}: a tranche review must be continue, pause or stop"
                )
            if not entry.payload.get("id"):
                errors.append(f"line {entry.lineno}: a tranche review names no tranche id")
            if not entry.raw.get("approval_id"):
                errors.append(
                    f"line {entry.lineno}: a tranche review names no approved gate"
                )
            if not entry.meta.get("programme_id"):
                errors.append(f"line {entry.lineno}: a tranche review names no meta.programme_id")
        elif entry.action == "tranches.authorise":
            if key.kind != "tranche_authorise":
                errors.append(
                    f"line {entry.lineno}: tranches.authorise key must be "
                    "tranche-authorise:<number>"
                )
                continue
            if not entry.payload.get("review_id"):
                errors.append(f"line {entry.lineno}: an authorisation names no review_id")

    # The chain is only as good as its links: the review is called with the gate the request
    # opened, and the authorisation with the review the review produced.
    gate_outputs = {
        entry.key.first: entry.output_id
        for entry in entries
        if entry.action == "approvals.request"
        and entry.key is not None
        and entry.key.kind == "tranche_review_gate"
    }
    review_outputs = {
        entry.key.first: entry.output_id
        for entry in entries
        if entry.action == "tranches.review" and entry.key is not None
    }
    for entry in entries:
        key = entry.key
        if entry.action == "tranches.review" and key is not None:
            gate = entry.raw.get("approval_id")
            expected = gate_outputs.get(key.first)
            if expected is not None and gate != expected:
                errors.append(
                    f"line {entry.lineno}: the review's gate {gate!r} is not the request "
                    f"entry's output {expected!r}"
                )
        if entry.action == "tranches.authorise" and key is not None:
            try:
                reviewed = str(int(key.first) - 1)
            except ValueError:
                reviewed = ""
            expected = review_outputs.get(reviewed)
            if expected is not None and entry.payload.get("review_id") != expected:
                errors.append(
                    f"line {entry.lineno}: the authorisation's review "
                    f"{entry.payload.get('review_id')!r} is not the review entry's "
                    f"output {expected!r}"
                )

    # A retro closes only when every finding it recorded has its lesson.
    findings_by_period: dict[str, set[str]] = {}
    lessons_by_period: dict[str, set[str]] = {}
    for entry in entries:
        if entry.key is None:
            continue
        if entry.action == "retros.record_findings":
            findings = entry.payload.get("findings") or []
            findings_by_period[entry.key.first] = {
                str(item.get("key"))
                for item in findings
                if isinstance(item, dict) and item.get("key")
            }
        elif entry.action == "lessons.propose":
            lessons_by_period.setdefault(entry.key.first, set()).add(str(entry.key.second))
    for entry in entries:
        if entry.action != "retros.close" or entry.key is None:
            continue
        missing = findings_by_period.get(entry.key.first, set()) - lessons_by_period.get(
            entry.key.first, set()
        )
        if missing:
            errors.append(
                f"line {entry.lineno}: the retro closes with no lesson for "
                f"{sorted(missing)}"
            )
    return errors


def check_journal(path: Path) -> tuple[list[Entry], list[Entry], list[str]]:
    header, raws, errors = load_journal(path)
    if header is not None and header.get("schema") != JOURNAL_SCHEMA:
        errors.append(
            f"journal header schema is {header.get('schema')!r}, expected {JOURNAL_SCHEMA!r}"
        )
    entries, unreplayable, entry_errors = build_entries(raws)
    errors.extend(entry_errors)
    errors.extend(validate_entries(entries))
    return entries, unreplayable, errors


# ── action transports ───────────────────────────────────────────────────────────────────


class ActionClient:
    """The one action endpoint: ``POST /api/actions/<name>/invocations``.

    The body is the invoke boundary's envelope, not the bare input:
    ``{"input": …, "approval_id": …}`` (``crates/api/src/actions.rs::parse_body``).  The
    ``approval_id`` is a property of the call, never of the action's input, and it is what makes
    a gated action (a tranche review, a destructive close) callable at all.
    """

    def __init__(
        self,
        url: str,
        token: str | None = None,
        timeout: float = 60.0,
    ) -> None:
        if not url:
            raise PromoteError("--apply requires --url or CONVEYOR_URL")
        if not token:
            raise PromoteError("--apply over HTTP requires --token (or CONVEYOR_TOKEN)")
        base = url.rstrip("/")
        if not base.endswith("/api"):
            base += "/api"
        self.base = base
        self.token = token
        self.timeout = timeout

    def build_body(self, payload: dict, approval_id: str | None = None) -> dict:
        """The invoke boundary's envelope: the input, and the call's gate when there is one."""
        body: dict[str, Any] = {"input": payload}
        if approval_id:
            body["approval_id"] = approval_id
        return body

    def invoke(
        self,
        action: str,
        payload: dict,
        if_match: int | None = None,
        approval_id: str | None = None,
        actor: str | None = None,
    ) -> Any:
        # The HTTP principal is the bearer token's; the journal's actor cannot be assumed
        # there. The local transport (--local-bin) is the record's own and passes it.
        del actor
        endpoint = f"{self.base}/actions/{action}/invocations"
        request = urllib.request.Request(
            endpoint,
            data=json.dumps(
                self.build_body(payload, approval_id),
                separators=(",", ":"),
                ensure_ascii=False,
            ).encode("utf-8"),
            method="POST",
            headers={"Content-Type": "application/json", "Accept": "application/json"},
        )
        request.add_header("Authorization", f"Bearer {self.token}")
        if if_match is not None:
            request.add_header("If-Match", f'"{if_match}"')
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:
                text = response.read().decode("utf-8")
        except urllib.error.HTTPError as error:
            detail = error.read().decode("utf-8", "replace")
            raise PromoteError(f"{action}: HTTP {error.code}: {detail[:2000]}") from None
        except (urllib.error.URLError, OSError) as error:
            raise PromoteError(f"{action}: cannot reach {endpoint}: {error}") from None
        try:
            envelope = json.loads(text)
        except ValueError:
            raise PromoteError(f"{action}: response is not JSON: {text[:500]!r}") from None
        if isinstance(envelope, dict) and "output" in envelope:
            return envelope["output"]
        return envelope


class LocalActionClient:
    """Invoke actions through the branch binary's admin CLI.

    This is the record's own transport (``programme-record.py``): the action layer runs inside
    the binary the record was written with, against ``DATABASE_MIGRATE_URL``, so no HTTP
    authentication stands between the replay and the actions.  stdin is the action's input —
    except for an action whose input itself carries an ``input`` key (``approvals.request``),
    which the CLI would read as its envelope, so it is wrapped.  The entry's recorded ``actor``
    is passed as ``--actor-id``: a gate's requester and its approver are different people, and
    without the flag both would be the CLI's default service actor.
    """

    def __init__(
        self,
        binary: str,
        tenant: str = "nonagentic",
        timeout: float = 300.0,
    ) -> None:
        if not binary:
            raise PromoteError("--apply requires --local-bin PATH")
        self.binary = binary
        self.tenant = tenant
        self.timeout = timeout

    def build(
        self,
        action: str,
        approval_id: str | None = None,
        if_match: int | None = None,
        actor: str | None = None,
    ) -> list[str]:
        args = [
            self.binary,
            "admin",
            "action",
            "invoke",
            "--tenant",
            self.tenant,
            "--name",
            action,
            "--input",
            "/dev/stdin",
        ]
        if approval_id:
            args += ["--approval-id", str(approval_id)]
        if if_match is not None:
            args += ["--if-match", str(if_match)]
        # The journal records who acted. Without it every entry runs as the CLI's default
        # service actor, and a gate's distinct-approver rule refuses its own requester.
        if actor:
            args += ["--actor-id", str(actor)]
        return args

    def build_stdin(self, payload: dict) -> str:
        """The action input on stdin, wrapped only where the CLI would read an envelope."""
        if isinstance(payload, dict) and "input" in payload:
            return json.dumps(
                {"input": payload}, separators=(",", ":"), ensure_ascii=False
            ) + "\n"
        return json.dumps(payload, separators=(",", ":"), ensure_ascii=False) + "\n"

    def invoke(
        self,
        action: str,
        payload: dict,
        if_match: int | None = None,
        approval_id: str | None = None,
        actor: str | None = None,
    ) -> Any:
        stdin = self.build_stdin(payload)
        environment = dict(os.environ)
        environment.setdefault("RUST_LOG", "error")
        try:
            completed = subprocess.run(
                self.build(
                    action, approval_id=approval_id, if_match=if_match, actor=actor
                ),
                input=stdin,
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
                env=environment,
                timeout=self.timeout,
            )
        except (OSError, subprocess.TimeoutExpired) as error:
            raise PromoteError(f"{action}: cannot run {self.binary}: {error}") from None
        if completed.returncode != 0:
            detail = (completed.stderr or completed.stdout or "").strip()
            raise PromoteError(
                f"{action}: transport exited {completed.returncode}: {detail[:2000]}"
            )
        try:
            return json.loads(completed.stdout)
        except ValueError:
            raise PromoteError(
                f"{action}: transport output is not JSON: {completed.stdout[:500]!r}"
            ) from None


def page_items(output: Any) -> list[dict]:
    if isinstance(output, dict) and isinstance(output.get("items"), list):
        return [row for row in output["items"] if isinstance(row, dict)]
    if isinstance(output, list):
        return [row for row in output if isinstance(row, dict)]
    return []


def list_all(client: Any, action: str, payload: dict) -> list[dict]:
    items: list[dict] = []
    cursor: str | None = None
    while True:
        page_payload = dict(payload)
        page_payload["limit"] = 200
        if cursor:
            page_payload["cursor"] = cursor
        output = client.invoke(action, page_payload)
        items.extend(page_items(output))
        cursor = output.get("next_cursor") if isinstance(output, dict) else None
        if not cursor:
            return items


# ── production state ────────────────────────────────────────────────────────────────────


@dataclass
class State:
    registers: list[dict] = field(default_factory=list)
    benefits: list[dict] = field(default_factory=list)
    measurements: list[dict] = field(default_factory=list)
    approvals: list[dict] = field(default_factory=list)
    # The closure family's production rows: projects by id (status, meta.reopened_at),
    # cards by id (list_id, board_id, version, comments with a body hash), lanes by id.
    projects: dict[str, dict] = field(default_factory=dict)
    cards: dict[str, dict] = field(default_factory=dict)
    lists: dict[str, dict] = field(default_factory=dict)
    # A card's own gates. `cards.move` into a done lane needs an approved, card-scoped
    # `card_done` gate when the card's type declares a Definition of Done, and those gates hang
    # on the card, not on a project, so they are read by card id.
    card_gates: dict[str, list[dict]] = field(default_factory=dict)
    # The architecture compliance reviews, by contract id. (The plain `reviews` field is the
    # tranche review list; this is the architecture one.)
    compliance_reviews: dict[str, list[dict]] = field(default_factory=dict)
    # The tr3 record family's production rows: benefits-map edges by benefit.
    links: list[dict] = field(default_factory=list)
    # The document family's production rows, by (project, category) as listed.
    documents: list[dict] = field(default_factory=list)
    # The governed-asset family's production rows: the MSP artefacts composed on the
    # programme-definition project and stepped through draft → in_review → approved.
    governed_assets: list[dict] = field(default_factory=list)
    # The architecture decision register's production rows, by key.
    decisions: list[dict] = field(default_factory=list)
    # The tr3 SAFe family's production rows.  A cadence is read per programme and matched by
    # (kind, period); a demo credit, a lesson and a review are read for the row they hang on.
    cadences: list[dict] = field(default_factory=list)
    credits: dict[str, list[dict]] = field(default_factory=dict)
    lessons: list[dict] = field(default_factory=list)
    tranches: list[dict] = field(default_factory=list)
    reviews: list[dict] = field(default_factory=list)
    wbs: dict[str, list[dict]] = field(default_factory=dict)
    board_cards: dict[str, list[dict]] = field(default_factory=dict)
    baselines: dict[str, list[dict]] = field(default_factory=dict)


def snapshot_file(directory: str) -> Path:
    path = Path(directory)
    candidates = [path / "msp.json", path / "data" / "msp.json"]
    if path.is_file():
        candidates.insert(0, path)
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    raise PromoteError(f"--snapshot {directory}: no msp.json")


def load_snapshot(directory: str) -> dict:
    path = snapshot_file(directory)
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except ValueError as error:
        raise PromoteError(f"--snapshot {path}: not JSON: {error}") from None
    if not isinstance(data, dict):
        raise PromoteError(f"--snapshot {path}: top level must be an object")
    return data


def rows(client: Any, action: str, payload: dict) -> list[dict]:
    """One page-less list action's rows.  Sending `limit` to an input that has no PageIn is
    a 422 under `deny_unknown_fields`, so the paginated helper is not used for these."""
    return page_items(client.invoke(action, payload))


def collect_ids(entries: list[Entry]) -> tuple[set[str], set[str]]:
    programme_ids: set[str] = set()
    project_ids: set[str] = set()
    for entry in entries:
        for source in (entry.payload, entry.meta):
            programme = source.get("programme_id")
            if isinstance(programme, str) and programme:
                programme_ids.add(programme)
            project = source.get("project_id")
            if isinstance(project, str) and project:
                project_ids.add(project)
    return programme_ids, project_ids


def load_state(
    client: Any,
    entries: list[Entry],
    snapshot: dict | None,
    programme_id: str | None = None,
) -> State:
    programme_ids, project_ids = collect_ids(entries)
    if programme_id is None:
        if len(programme_ids) > 1:
            raise PromoteError(
                f"inputs name more than one programme_id: {sorted(programme_ids)}"
            )
        if not programme_ids:
            raise PromoteError("no input names a programme_id; --apply needs one")
        programme_id = next(iter(programme_ids))
    state = State()
    state.registers.extend(
        page_items(client.invoke("registers.list", {"programme_id": programme_id}))
    )
    state.benefits.extend(
        page_items(client.invoke("benefits.list", {"programme_id": programme_id}))
    )
    seen_registers = {row.get("id") for row in state.registers}
    for project_id in sorted(project_ids):
        for row in page_items(
            client.invoke("registers.list", {"project_id": project_id})
        ):
            if row.get("id") not in seen_registers:
                state.registers.append(row)
                seen_registers.add(row.get("id"))
    if any(
        entry.action in ("decisions.create", "decisions.transition") for entry in entries
    ):
        state.decisions.extend(list_all(client, "decisions.list", {}))
    if any(
        entry.action in ("documents.compose", "documents.publish") for entry in entries
    ):
        seen: set[tuple[str, str]] = set()
        for entry in entries:
            if entry.action == "documents.publish":
                # A publish's key names a unit, not a project, so the row it publishes is read
                # by id: without it the skip test cannot see a document the compose did not
                # describe (the programme-level publications have no project to list by).
                document_id = entry.payload.get("id")
                if not document_id:
                    raise PromoteError("documents.publish: the input names no id")
                try:
                    row = client.invoke("documents.get", {"id": document_id})
                except PromoteError:
                    row = None
                if isinstance(row, dict) and row.get("id"):
                    if not any(
                        existing.get("id") == row["id"] for existing in state.documents
                    ):
                        state.documents.append(row)
                continue
            if entry.action != "documents.compose":
                continue
            project_id = entry.payload.get("project_id")
            category = entry.payload.get("category")
            if not project_id or not category:
                raise PromoteError(
                    f"{entry.action}: the input names no project_id/category"
                )
            key = (str(project_id), str(category))
            if key in seen:
                continue
            seen.add(key)
            state.documents.extend(
                list_all(
                    client,
                    "documents.list",
                    {"project_id": project_id, "category": category},
                )
            )
    card_gate_cards = {
        entry.key.second if entry.key.kind == "approval" else entry.key.first
        for entry in entries
        if entry.key is not None
        and (
            (
                entry.action == "approvals.request"
                and entry.key.kind == "approval"
                and entry.key.first == "card_done"
            )
            or (entry.action == "approvals.decide" and entry.key.kind == "card_done_decide")
        )
    }
    review_contracts = {
        entry.key.first
        for entry in entries
        if entry.action == "architecture.reviews_record"
        and entry.key is not None
        and entry.key.kind == "review"
    }
    for contract_id in sorted(review_contracts):
        state.compliance_reviews[contract_id] = list_all(
            client, "architecture.reviews_list", {"contract_id": contract_id}
        )
    for card_id in sorted(card_gate_cards):
        state.card_gates[card_id] = list_all(
            client, "approvals.list", {"card_id": card_id}
        )
    if any(entry.action == "benefits.link" for entry in entries):
        state.links.extend(
            list_all(client, "benefits.link_list", {"programme_id": programme_id})
        )
    if any(entry.action in ("changes.request", "approvals.decide") for entry in entries):
        for project_id in sorted(project_ids):
            state.approvals.extend(
                list_all(client, "approvals.list", {"project_id": project_id})
            )
    closure_actions = {
        "projects.close_request",
        "approvals.request",
        "projects.close",
        "cards.comment",
        "cards.move",
    }
    if any(entry.action in closure_actions for entry in entries):
        close_projects = {
            entry.key.first
            for entry in entries
            if entry.key is not None
            and entry.key.kind in ("project_close", "project_closure")
        }
        for project_id in sorted(project_ids | close_projects):
            output = client.invoke("projects.get", {"id": project_id})
            row = output.get("project") if isinstance(output, dict) else None
            if isinstance(row, dict):
                state.projects[project_id] = row
        for entry in entries:
            if entry.action not in ("cards.comment", "cards.move"):
                continue
            card_id = entry.payload.get("id")
            if not card_id:
                raise PromoteError(f"{entry.action}: the input names no card id")
            card = read_card(client, card_id)
            for comment in card.get("comments") or []:
                body = comment.get("body") if isinstance(comment, dict) else None
                if isinstance(body, str):
                    comment["_sha"] = hashlib.sha256(body.encode("utf-8")).hexdigest()
            state.cards[card_id] = card
            board_id = card.get("board_id")
            if board_id:
                try:
                    for row in list_all(client, "lists.list", {"board_id": board_id}):
                        if row.get("id"):
                            state.lists[row["id"]] = row
                except PromoteError:
                    # No action lists a board's lanes (the board slice has never needed one
                    # from an agent). The journal's lane ids are restored rows and therefore
                    # identical in both targets; a move falls back to the recorded id and the
                    # readback proves the landing.
                    pass
        # A projects.close replay needs the project's gates even when no change entry does.
        for project_id in sorted(project_ids):
            if not any(row.get("project_id") == project_id for row in state.approvals):
                state.approvals.extend(
                    list_all(client, "approvals.list", {"project_id": project_id})
                )
    if any(
        entry.action == "changes.request" and entry.payload.get("baseline_id")
        for entry in entries
    ):
        for project_id in sorted(
            {
                entry.payload.get("project_id")
                for entry in entries
                if entry.action == "changes.request"
                and entry.payload.get("baseline_id")
                and entry.payload.get("project_id")
            }
        ):
            state.baselines[str(project_id)] = rows(
                client, "baselines.list", {"project_id": project_id}
            )
    cadence_actions = {
        "cadences.create",
        "cadences.update",
        "cadences.approve_plan",
        "cadences.credit_demo",
        "retros.record_findings",
        "retros.close",
        "lessons.propose",
        "tranches.review",
        "tranches.authorise",
    }
    if any(
        entry.action in cadence_actions or entry.action == "wbs.update" for entry in entries
    ):
        for scope in sorted(programme_ids):
            state.cadences.extend(
                rows(client, "cadences.list", {"programme_id": scope})
            )
            state.lessons.extend(rows(client, "lessons.list", {"programme_id": scope}))
            state.tranches.extend(rows(client, "tranches.list", {"programme_id": scope}))
            state.reviews.extend(
                rows(client, "tranches.review_list", {"programme_id": scope})
            )
        for entry in entries:
            if entry.action == "cadences.credit_demo" and entry.key is not None:
                cadence = cadence_row(state, entry.key.first, "demo")
                cadence_id = cadence.get("id") if cadence else None
                if cadence_id and cadence_id not in state.credits:
                    state.credits[cadence_id] = rows(
                        client, "cadences.credit_list", {"cadence_id": cadence_id}
                    )
            if (
                entry.action in ("retros.record_findings", "retros.close")
                and entry.key is not None
            ):
                cadence = cadence_row(state, entry.key.first, "retro")
                meta = (cadence or {}).get("meta") or {}
                board = meta.get("board_id") if isinstance(meta, dict) else None
                if board and board not in state.board_cards:
                    state.board_cards[board] = list_all(
                        client, "cards.list", {"board_id": board}
                    )
        for project_id in sorted(
            {
                entry.meta.get("project_id")
                for entry in entries
                if entry.action == "cadences.credit_demo"
                and entry.meta.get("project_id")
            }
            | {
                entry.key.first
                for entry in entries
                if entry.action == "wbs.update"
                and entry.key is not None
                and entry.key.kind == "wbs_accept"
            }
        ):
            state.wbs[project_id] = rows(
                client, "wbs.list", {"project_id": project_id}
            )
    chain_actions = {"cadences.approve_plan", "tranches.review", "tranches.authorise"}
    if any(
        entry.action in chain_actions
        or (
            entry.action == "approvals.request"
            and entry.key is not None
            and entry.key.kind == "tranche_review_gate"
        )
        for entry in entries
    ):
        seen_gates = {gate.get("id") for gate in state.approvals}
        for gate in list_all(client, "approvals.list", {}):
            if gate.get("id") not in seen_gates:
                state.approvals.append(gate)
                seen_gates.add(gate.get("id"))
    if any(entry.action == "benefits.measure" for entry in entries):
        try:
            state.measurements.extend(
                rows(
                    client,
                    MEASUREMENT_LIST_ACTION,
                    {"programme_id": programme_id},
                )
            )
        except PromoteError:
            # The deployed image may predate the action; the MSP export is the fallback.
            pass
    if snapshot is not None and not state.measurements:
        state.measurements.extend(snapshot.get("benefit_measurements") or [])
    return state


def benefit_by_reference(state: State, reference: str) -> dict | None:
    for row in state.benefits:
        if row.get("reference") == reference:
            return row
    return None


def decision_by_key(state: State, key: str) -> dict | None:
    for row in state.decisions:
        if row.get("key") == key:
            return row
    return None


def risk_by_code(state: State, code: str) -> dict | None:
    for row in state.registers:
        if row.get("kind") == "risk" and row.get("code") == code:
            return row
    return None


def cadence_row(state: State, period: str, kind: str) -> dict | None:
    for row in state.cadences:
        if row.get("period") == period and row.get("kind") == kind:
            return row
    return None


def tranche_by_number(state: State, number: str | int) -> dict | None:
    try:
        wanted = int(number)
    except (TypeError, ValueError):
        return None
    for row in state.tranches:
        try:
            if int(row.get("number")) == wanted:
                return row
        except (TypeError, ValueError):
            continue
    return None


def review_for(
    state: State, tranche_id: str | None, decision: str | None = None
) -> dict | None:
    matches = [
        row
        for row in state.reviews
        if row.get("tranche_id") == tranche_id
        and (decision is None or row.get("decision") == decision)
    ]
    if not matches:
        return None
    # A tranche can be reviewed more than once; the newest review is the one that stands.
    return sorted(matches, key=lambda row: str(row.get("reviewed_at") or ""))[-1]


def wbs_deliverable(state: State, project_id: str | None, title: str) -> dict | None:
    """The project's DELIVERABLE row with this title.

    A WBS node is not always a deliverable: a summary/group row with the same title carries no
    deliverable id, and passing one to `cadences.credit_demo` is a 422 on the id field rather
    than a named refusal. Only a row the credit action can accept is returned here.
    """
    for row in state.wbs.get(project_id or "", []):
        if row.get("kind") != "deliverable":
            continue
        if row.get("id") is None:
            continue
        if row.get("title") == title:
            return row
    return None


def lesson_for(state: State, cadence_id: str | None, finding_key: str) -> dict | None:
    for row in state.lessons:
        if row.get("finding_key") != finding_key:
            continue
        if cadence_id is None or row.get("cadence_id") == cadence_id:
            return row
    return None


def fresh_review(
    client: Any, programme: str | None, tranche_id: str, decision: str | None = None
) -> dict | None:
    """The review list is re-read after a write: a run's own review is not in the pre-read."""
    matches = [
        row
        for row in rows(client, "tranches.review_list", {"programme_id": programme})
        if row.get("tranche_id") == tranche_id
        and (decision is None or row.get("decision") == decision)
    ]
    if not matches:
        return None
    return sorted(matches, key=lambda row: str(row.get("reviewed_at") or ""))[-1]


def upsert_row(items: list[dict], row: dict) -> None:
    row_id = row.get("id")
    items[:] = [row if item.get("id") == row_id else item for item in items]
    if not any(item.get("id") == row_id for item in items):
        items.append(row)


def retro_card(state: State, cadence: dict | None) -> dict | None:
    meta = (cadence or {}).get("meta") or {}
    board = meta.get("board_id") if isinstance(meta, dict) else None
    for card in state.board_cards.get(board or "", []):
        card_meta = card.get("meta") or {}
        if isinstance(card_meta, dict) and card_meta.get("cadence_id") == (cadence or {}).get("id"):
            return card
    return None


def plan_gate(state: State, cadence_id: str) -> dict | None:
    for gate in state.approvals:
        if (
            gate.get("gate") == PI_PLAN_GATE
            and gate.get("target") == cadence_id
            and gate.get("status") == "approved"
        ):
            return gate
    return None


def tranche_review_target(tranche_id: str) -> str:
    return f"tranche/{tranche_id}/review"


def map_ids(value: Any, id_map: dict[str, str]) -> Any:
    """Deep-replace a staging id with its production id, wherever it is embedded.

    A cadence's meta carries links (its own id in the plan's evidence, the refinement's id);
    those are staging ids like any other journal output, and a meta that keeps them would point
    at rows production does not have.
    """
    if isinstance(value, str):
        return id_map.get(value, value)
    if isinstance(value, dict):
        return {key: map_ids(item, id_map) for key, item in value.items()}
    if isinstance(value, list):
        return [map_ids(item, id_map) for item in value]
    return value


def expected_cadence_meta(
    state: State, entry: Entry, id_map: dict[str, str] | None = None
) -> dict | None:
    """The meta the cadence will carry: the journal's, with every staging id made production."""
    meta = entry.payload.get("meta")
    if not isinstance(meta, dict):
        return None
    expected = copy.deepcopy(meta)
    if expected.get("tranche_number") is not None:
        tranche = tranche_by_number(state, expected["tranche_number"])
        if tranche is not None:
            expected["tranche_id"] = tranche["id"]
    return map_ids(expected, id_map or {})


def tranche_review_gate(state: State, tranche_id: str | None) -> dict | None:
    if not tranche_id:
        return None
    target = tranche_review_target(tranche_id)
    for gate in state.approvals:
        if (
            gate.get("gate") == TRANCHE_REVIEW_GATE
            and gate.get("target") == target
            and gate.get("status") != "rejected"
        ):
            return gate
    return None


def change_rows(state: State, key: NaturalKey, payload: dict) -> list[dict]:
    labels = {key.first}
    title = payload.get("title")
    if isinstance(title, str) and title:
        labels.add(title)
    return [
        row
        for row in state.registers
        if row.get("kind") == "change"
        and (row.get("title") in labels or row.get("id") in labels)
    ]


def change_row(state: State, key: NaturalKey, payload: dict) -> dict | None:
    rows = change_rows(state, key, payload)
    return rows[0] if rows else None


def gate_for_change(state: State, change: dict) -> dict | None:
    change_id = change.get("id")
    if not change_id:
        return None
    for gate in state.approvals:
        if gate.get("gate") != "change":
            continue
        proposed = gate.get("proposed_input")
        proposed_id = proposed.get("change_id") if isinstance(proposed, dict) else None
        description = gate.get("description") or ""
        if (
            gate.get("target") == change_id
            or proposed_id == change_id
            or change_id in description
        ):
            return gate
    return None


def update_already_reflected(
    state: State,
    key: NaturalKey,
    payload: dict,
    ignore: set[str] | None = None,
) -> bool:
    benefit = benefit_by_reference(state, key.first)
    if benefit is None:
        return False
    if key.second == "status":
        return benefit.get("status") == payload.get("status")
    compared = 0
    for name, value in payload.items():
        if name in ("id", "change_id") or (ignore and name in ignore):
            continue
        compared += 1
        if benefit.get(name) != value:
            return False
    # Every field this entry changes is owned by a later entry: the row's existence is all
    # that is left to test, and re-applying the early value would undo the later one.
    return compared > 0 or benefit is not None


def baseline_measurement(state: State, reference: str) -> dict | None:
    benefit = benefit_by_reference(state, reference)
    benefit_ids = {benefit.get("id")} if benefit else set()
    for row in state.measurements:
        if row.get("benefit_id") in benefit_ids and row.get("kind") == "baseline":
            return row
    return None


def expected_gate_status(payload: dict) -> str:
    return "approved" if payload.get("decision") == "approve" else "rejected"


def closure_gate(state: State, project_id: str) -> dict | None:
    """The project's closure gate, in whatever state — a request that happened is a skip."""
    for gate in state.approvals:
        if gate.get("gate") == "closure" and gate.get("project_id") == project_id:
            return gate
    return None


def project_gate(state: State, project_id: str, name: str) -> dict | None:
    """A named project gate in whatever state — the act that opened it is a skip.

    A second `projects.charter` is refused by the engine (`charter_approved`) and a second
    `projects.baseline` would create a second baseline, so the skip test keys on the gate's
    existence rather than on its status: the decide entry after it is what finishes the act.
    """
    newest: dict | None = None
    for gate in state.approvals:
        if gate.get("gate") != name or gate.get("project_id") != project_id:
            continue
        if newest is None or str(gate.get("created_at") or "") > str(
            newest.get("created_at") or ""
        ):
            newest = gate
    return newest


def requested_target(payload: dict) -> str | None:
    """The engine's target for an `approvals.request` payload, where it is derivable."""
    if payload.get("target"):
        return str(payload["target"])
    if payload.get("action") == "projects.close":
        inner = payload.get("input")
        project_id = inner.get("id") if isinstance(inner, dict) else None
        return f"project/{project_id}/close" if project_id else None
    if payload.get("action") == TRANCHE_REVIEW_GATE:
        inner = payload.get("input")
        tranche_id = inner.get("id") if isinstance(inner, dict) else None
        return f"tranche/{tranche_id}/review" if tranche_id else None
    return None


def card_gate(state: State, card_id: str, status: str | None = None) -> dict | None:
    """The card's `card_done` gate, optionally only when it is in `status`.

    A card-scoped gate has no target to match on: the card is its identity, which is why the
    state reads the gate by card id and every matcher here names the card.
    """
    for gate in state.card_gates.get(card_id, []):
        if gate.get("gate") != "card_done" or gate.get("status") == "rejected":
            continue
        if status is None or gate.get("status") == status:
            return gate
    return None


def request_gate(state: State, payload: dict) -> dict | None:
    """A gate with the same gate name and target is the request that already happened."""
    gate_name = payload.get("gate")
    target = requested_target(payload)
    if not gate_name:
        return None
    for gate in state.approvals:
        if gate.get("gate") != gate_name:
            continue
        if gate.get("status") == "rejected":
            continue
        if target is not None and gate.get("target") == target:
            return gate
    return None


def card_comment(card: dict, wanted: str) -> dict | None:
    for comment in card.get("comments") or []:
        if isinstance(comment, dict) and str(comment.get("_sha") or "").startswith(wanted):
            return comment
    return None


def lane_kind(state: State, list_id: str | None) -> str | None:
    lane = state.lists.get(list_id or "")
    return str(lane.get("kind")) if lane and lane.get("kind") else None


def existing_for(
    entry: Entry,
    state: State,
    change_index: dict[str, dict],
    id_map: dict[str, str] | None = None,
    ignore_fields: set[str] | None = None,
) -> dict | None:
    """The production row that already carries this entry's natural key, if any."""
    key = entry.key
    assert key is not None
    # The tr3 SAFe family keys by content, never by a staging id.
    if entry.action == "cadences.create":
        return cadence_row(state, key.first, key.second or "")
    if entry.action == "cadences.update":
        row = cadence_row(state, key.first, key.second or "")
        if row is not None and row.get("meta") == expected_cadence_meta(
            state, entry, id_map
        ):
            return row
        return None
    if entry.action == "cadences.approve_plan":
        cadence = cadence_row(state, key.first, "pi")
        cadence_id = cadence.get("id") if cadence else None
        return plan_gate(state, cadence_id) if cadence_id else None
    if entry.action == "cadences.credit_demo":
        cadence = cadence_row(state, key.first, "demo")
        deliverable = wbs_deliverable(
            state, entry.meta.get("project_id"), key.second or ""
        )
        if cadence is None or deliverable is None:
            return None
        for credit in state.credits.get(cadence.get("id"), []):
            if credit.get("deliverable_id") == deliverable.get("id"):
                return credit
        return None
    if entry.action == "retros.record_findings":
        card = retro_card(state, cadence_row(state, key.first, "retro"))
        if card is None:
            return None
        meta = card.get("meta") or {}
        recorded = {
            str(item.get("key"))
            for item in (meta.get("findings") or [])
            if isinstance(item, dict) and item.get("key")
        }
        wanted = {
            str(item.get("key"))
            for item in entry.payload.get("findings") or []
            if isinstance(item, dict) and item.get("key")
        }
        return card if recorded == wanted else None
    if entry.action == "retros.close":
        cadence = cadence_row(state, key.first, "retro")
        if cadence is not None and cadence.get("state") == "closed":
            return cadence
        return None
    if entry.action == "lessons.propose":
        cadence = cadence_row(state, key.first, "retro")
        cadence_id = cadence.get("id") if cadence else None
        return lesson_for(state, cadence_id, key.second or "")
    if entry.action == "tranches.review":
        tranche = tranche_by_number(state, key.first)
        tranche_id = tranche.get("id") if tranche else None
        return review_for(state, tranche_id, entry.payload.get("decision"))
    if entry.action == "tranches.authorise":
        tranche = tranche_by_number(state, key.first)
        reviewed = str(int(key.first) - 1) if key.first.isdigit() else ""
        predecessor = tranche_by_number(state, reviewed)
        review = review_for(
            state, predecessor.get("id") if predecessor else None
        )
        if (
            tranche is not None
            and review is not None
            and tranche.get("state") == "authorised"
            and tranche.get("authorisation_review_id") == review.get("id")
        ):
            return tranche
        return None
    if entry.action == "approvals.request" and key.kind == "approval" and key.first == "card_done":
        # The request that happened is a skip; the decide after it is what finishes the gate.
        return card_gate(state, key.second or "")
    if (
        entry.action == "approvals.decide"
        and key.kind == "card_done_decide"
    ):
        return card_gate(state, key.first, expected_gate_status(entry.payload))
    if entry.action == "approvals.request" and key.kind == "tranche_review_gate":
        tranche = tranche_by_number(state, key.first)
        return tranche_review_gate(
            state, tranche.get("id") if tranche else None
        )
    if entry.action == "approvals.decide" and key.kind == "tranche_review_decide":
        tranche = tranche_by_number(state, key.first)
        gate = tranche_review_gate(state, tranche.get("id") if tranche else None)
        if gate is not None and gate.get("status") == expected_gate_status(entry.payload):
            return gate
        return None
    if entry.action == "registers.create":
        return risk_by_code(state, key.first)
    if entry.action == "benefits.create":
        return benefit_by_reference(state, key.first)
    if entry.action == "benefits.measure":
        return baseline_measurement(state, key.first)
    if entry.action == "changes.request":
        return change_row(state, key, entry.payload)
    if entry.action == "approvals.decide" and key.first in (
        "closure",
        "projects.close",
        "charter",
        "baseline",
    ):
        # The closure chain's decides: the key names the gate and the project, not a change.
        for gate in state.approvals:
            if gate.get("gate") != key.first or gate.get("project_id") != key.second:
                continue
            if gate.get("status") == expected_gate_status(entry.payload):
                return gate
        return None
    if entry.action == "approvals.decide":
        known = change_index.get(key.second or "")
        gate = None
        if known and known.get("approval_id"):
            for row in state.approvals:
                if row.get("id") == known["approval_id"]:
                    gate = row
                    break
        if gate is None:
            change = change_row(
                state, NaturalKey("change", key.second or ""), entry.payload
            )
            if change is not None:
                gate = gate_for_change(state, change)
        if gate is not None and gate.get("status") == expected_gate_status(entry.payload):
            return gate
        return None
    if entry.action == "benefits.update":
        if update_already_reflected(state, key, entry.payload, ignore_fields):
            return benefit_by_reference(state, key.first)
        return None
    if entry.action == "decisions.create":
        return decision_by_key(state, key.first)
    if entry.action == "decisions.transition":
        row = decision_by_key(state, key.first)
        if row is not None and row.get("status") == key.second:
            return row
        return None
    if entry.action == "documents.compose":
        for row in state.documents:
            if (
                row.get("title") == entry.payload.get("title")
                and row.get("category") == entry.payload.get("category")
                and row.get("project_id") == entry.payload.get("project_id")
            ):
                return row
        return None
    if entry.action == "documents.publish":
        # A publish whose compose already ran knows the production document exactly; a key whose
        # third part is a content hash rather than a title (the programme-level publications)
        # can only be matched this way.
        staging_id = str(entry.payload.get("id") or "")
        production = (id_map or {}).get(staging_id)
        if production:
            for row in state.documents:
                if (
                    row.get("id") == production
                    and row.get("published_at")
                    and row.get("status") != "deleted"
                ):
                    return row
        parts = key.first.split(":", 2)
        if len(parts) != 3:
            raise PromoteError(
                "document-publish key must be document-publish:<unit>:<category>:<title>"
            )
        _unit, category, title = parts
        for row in state.documents:
            if (
                row.get("title") == title
                and row.get("category") == category
                and row.get("published_at")
                and row.get("status") != "deleted"
            ):
                return row
        return None
    if entry.action == "registers.update":
        row = risk_by_code(state, key.first)
        if row is None:
            return None
        for name, value in entry.payload.items():
            if name in ("id", "change_id"):
                continue
            if row.get(name) != value:
                return None
        return row
    if entry.action == "benefits.link":
        benefit = benefit_by_reference(state, entry.payload.get("benefit_ref") or "")
        if benefit is None:
            return None
        for link in state.links:
            if (
                link.get("benefit_id") == benefit.get("id")
                and link.get("output_ref") == entry.payload.get("output_ref")
            ):
                return link
        return None
    if entry.action == "architecture.reviews_record":
        # One review per contract: the row that exists is the act that happened.
        for row in state.compliance_reviews.get(key.first, []):
            if row.get("contract_id") == key.first:
                return row
        return None
    if entry.action == "projects.close_request":
        return closure_gate(state, key.first)
    if entry.action == "projects.charter":
        # The act is done once the project's charter gate exists at all: a pending gate still
        # needs its decide, an approved one needs nothing, and a second charter is refused.
        return project_gate(state, key.first, "charter")
    if entry.action == "projects.baseline":
        # Same shape: a baseline gate means the plan ran; the decide entry approves the row.
        return project_gate(state, key.first, "baseline")
    if entry.action == "wbs.update":
        node = wbs_deliverable(state, key.first, key.second or "")
        if node is None or node.get("status") != entry.payload.get("status"):
            return None
        return node
    if entry.action == "approvals.request":
        return request_gate(state, entry.payload)
    if entry.action == "projects.close":
        project = state.projects.get(key.first)
        if project is not None and project.get("status") == "closed":
            return project
        return None
    if entry.action == "cards.comment":
        card = state.cards.get(key.first)
        return card_comment(card, key.second) if card is not None else None
    if entry.action == "cards.move":
        card = state.cards.get(key.first)
        if card is None:
            return None
        if lane_kind(state, card.get("list_id")) == key.second:
            return card
        # The lanes are not readable as rows (no action lists a board's lanes), so the two
        # id-based signals carry the skip test: the card is already in the lane the entry
        # names, or it has already moved on from that lane.
        if str(card.get("list_id") or "") == str(entry.payload.get("list_id") or ""):
            return card
        # A card that has already moved ON from this entry's lane has nothing left to do here:
        # the journal records intake→active→done, and a second pass sees the card in `done`
        # with `active` behind it. Without this the replay asks for a done→active move and the
        # engine refuses it (409) — a correct refusal of a wrong request.
        if str(card.get("prev_list_id") or "") == str(entry.payload.get("list_id") or ""):
            return card
        return None
    return None


def superseded_fields(entries: list[Entry]) -> dict[int, set[str]]:
    """For each benefit patch, the fields a LATER patch to the same profile owns.

    Replaying the record in order must reach the same final row.  An earlier patch whose
    effect was superseded would, if re-applied on a complete record, undo the later one —
    the skip test must therefore leave those fields to the entry that owns them.
    """
    superseded: dict[int, set[str]] = {}
    for index, entry in enumerate(entries):
        if entry.action != "benefits.update" or entry.key is None:
            continue
        owned: set[str] = set()
        for later in entries[index + 1:]:
            if later.key is None or later.key.first != entry.key.first:
                continue
            if later.action == "benefits.update":
                owned.update(
                    name for name in later.payload if name not in ("id", "change_id")
                )
            elif later.action == "benefits.measure":
                # A measurement writes the profile's baseline (measured_at and method included).
                owned.add("baseline")
        if owned:
            superseded[entry.lineno] = owned
    return superseded


def build_change_index(
    entries: list[Entry], state: State
) -> dict[str, dict]:
    index: dict[str, dict] = {}
    for entry in entries:
        if entry.action != "changes.request" or entry.key is None:
            continue
        change = change_row(state, entry.key, entry.payload)
        if change is None:
            continue
        gate = gate_for_change(state, change)
        record = {
            "change_id": change.get("id"),
            "approval_id": gate.get("id") if gate else None,
        }
        index[entry.key.first] = record
        if entry.output_id:
            # A decide may name the change by the staging id the journal recorded.
            index[str(entry.output_id)] = record
    return index


# ── readback ────────────────────────────────────────────────────────────────────────────


def read_project(client: Any, project_id: str) -> dict:
    """`projects.get` answers `{project, boards}`; the row is the nested project."""
    try:
        output = client.invoke("projects.get", {"id": project_id})
    except PromoteError as error:
        raise ReadbackError(str(error)) from None
    project = output.get("project") if isinstance(output, dict) else None
    if not isinstance(project, dict) or project.get("id") != project_id:
        raise ReadbackError("projects.get: the row was not read back")
    return project


def read_row(client: Any, action: str, row_id: str) -> dict:
    try:
        output = client.invoke(action, {"id": row_id})
    except PromoteError as error:
        raise ReadbackError(str(error)) from None
    if not isinstance(output, dict):
        raise ReadbackError(f"{action}: readback returned no row")
    if output.get("id") != row_id:
        raise ReadbackError(
            f"{action}: readback returned id {output.get('id')!r}, expected {row_id!r}"
        )
    return output


def read_card(client: Any, card_id: str) -> dict:
    """`cards.get` answers `{card: {…}}`; every other read answers the row itself."""
    try:
        output = client.invoke("cards.get", {"id": card_id})
    except PromoteError as error:
        raise ReadbackError(str(error)) from None
    card = output.get("card") if isinstance(output, dict) else None
    if not isinstance(card, dict) or card.get("id") != card_id:
        raise ReadbackError(f"cards.get: no card {card_id}")
    return card


def readback_register(
    client: Any, row_id: str, kind: str, code: str | None = None,
    title: str | None = None,
) -> dict:
    row = read_row(client, "registers.get", row_id)
    if row.get("kind") != kind:
        raise ReadbackError(f"registers.get: {row_id} is {row.get('kind')!r}, not {kind!r}")
    if code is not None and row.get("code") != code:
        raise ReadbackError(
            f"registers.get: {row_id} code is {row.get('code')!r}, not {code!r}"
        )
    if title is not None and row.get("title") != title:
        raise ReadbackError(
            f"registers.get: {row_id} title is {row.get('title')!r}, not {title!r}"
        )
    return row


def readback_benefit(client: Any, row_id: str, reference: str) -> dict:
    row = read_row(client, "benefits.get", row_id)
    if row.get("reference") != reference:
        raise ReadbackError(
            f"benefits.get: {row_id} reference is {row.get('reference')!r}, "
            f"not {reference!r}"
        )
    return row


def readback_gate(client: Any, project_id: str, gate_id: str) -> dict:
    try:
        gates = list_all(client, "approvals.list", {"project_id": project_id})
    except PromoteError as error:
        raise ReadbackError(str(error)) from None
    for gate in gates:
        if gate.get("id") == gate_id:
            return gate
    raise ReadbackError(f"approvals.list: gate {gate_id} is not readable")


def readback_retro_card(client: Any, cadence: dict, findings: list) -> dict:
    """The retro's coach card, carrying exactly the finding keys the input records."""
    meta = cadence.get("meta") or {}
    board = meta.get("board_id") if isinstance(meta, dict) else None
    if not board:
        raise ReadbackError("cards.list: the retro cadence names no board")
    try:
        cards = list_all(client, "cards.list", {"board_id": board})
    except PromoteError as error:
        raise ReadbackError(str(error)) from None
    wanted = {
        str(item.get("key"))
        for item in findings
        if isinstance(item, dict) and item.get("key")
    }
    for card in cards:
        card_meta = card.get("meta") or {}
        if not isinstance(card_meta, dict) or card_meta.get("cadence_id") != cadence.get("id"):
            continue
        recorded = {
            str(item.get("key"))
            for item in (card_meta.get("findings") or [])
            if isinstance(item, dict) and item.get("key")
        }
        if recorded != wanted:
            raise ReadbackError(
                f"cards.list: the retro card records {sorted(recorded)}, not {sorted(wanted)}"
            )
        return card
    raise ReadbackError("cards.list: the retro card is not readable")


def readback_measurement(
    client: Any,
    state: State,
    entry: Entry,
    snapshot: str | None,
    row_id: str,
) -> dict:
    """The measurement's row: the read action first, the MSP export only as a fallback."""
    key = entry.key
    assert key is not None
    benefit = benefit_by_reference(state, key.first)
    benefit_id = benefit.get("id") if benefit else None
    if benefit_id:
        try:
            for row in rows(
                client, MEASUREMENT_LIST_ACTION, {"benefit_id": benefit_id}
            ):
                if row.get("id") == row_id:
                    return row
            raise ReadbackError(
                f"{MEASUREMENT_LIST_ACTION}: measurement {row_id} is not on the benefit"
            )
        except ReadbackError:
            raise
        except PromoteError as error:
            if not snapshot:
                raise ReadbackError(str(error)) from None
    if not snapshot:
        raise ReadbackError(
            f"measurement {row_id} cannot be read back: no read action result and no "
            "--snapshot"
        )
    data = load_snapshot(snapshot)
    for row in data.get("benefit_measurements") or []:
        if isinstance(row, dict) and row.get("id") == row_id:
            return row
    raise ReadbackError(
        f"measurement {row_id} is absent from msp.json and no product read action "
        "lists measurements"
    )


# ── apply ──────────────────────────────────────────────────────────────────────────────


@dataclass
class Outcome:
    entry: Entry
    staging_id: str
    production_id: str | None
    verdict: str
    detail: str = ""


def remap_payload(
    entry: Entry,
    state: State,
    id_map: dict[str, str],
    change_index: dict[str, dict],
) -> dict:
    key = entry.key
    assert key is not None
    payload = copy.deepcopy(entry.payload)
    if entry.action == "benefits.measure":
        benefit = benefit_by_reference(state, key.first)
        if benefit is None:
            benefit_id = id_map.get(payload.get("benefit_id", ""))
            if not benefit_id:
                raise PromoteError(
                    f"benefit {key.first!r} has no production id; cannot measure it"
                )
            payload["benefit_id"] = benefit_id
        else:
            payload["benefit_id"] = benefit["id"]
    elif entry.action == "benefits.update":
        benefit = benefit_by_reference(state, key.first)
        if benefit is None:
            raise PromoteError(
                f"benefit {key.first!r} has no production profile to update"
            )
        payload["id"] = benefit["id"]
        change_id = payload.get("change_id")
        if change_id:
            production_change = id_map.get(change_id)
            if not production_change:
                raise PromoteError(
                    f"change_id {change_id!r} has no production mapping; "
                    "the change entry must create or find it first"
                )
            payload["change_id"] = production_change
    elif entry.action == "registers.update":
        row = risk_by_code(state, key.first)
        if row is None:
            raise PromoteError(
                f"risk {key.first!r} has no production row to update"
            )
        payload["id"] = row["id"]
    elif entry.action == "benefits.link":
        benefit = benefit_by_reference(state, entry.payload.get("benefit_ref") or "")
        if benefit is None:
            raise PromoteError(
                f"benefit {entry.payload.get('benefit_ref')!r} has no production profile"
            )
        payload["benefit_id"] = benefit["id"]
        payload.pop("benefit_ref", None)
    elif entry.action == "decisions.transition":
        row = decision_by_key(state, key.first)
        if row is None:
            raise PromoteError(
                f"decision {key.first!r} has no production row to transition"
            )
        payload["id"] = row["id"]
    elif entry.action in ("governed_assets.submit", "governed_assets.approve"):
        # The lifecycle entry names the compose entry's staging asset id; the write and its
        # read-back both take the production id the compose act mapped.
        staging = payload.get("id")
        production = id_map.get(str(staging)) if staging else None
        if not production:
            raise PromoteError(
                f"{entry.action}: asset {staging!r} has no production id; the compose "
                "entry must create or find it first"
            )
        payload["id"] = production
    elif entry.action == "architecture.artifacts_create":
        # The artefact's contract is an architecture-domain row the journal does not yet
        # replay (v1-promotion-contract-actions-1). A mapped contract is carried; an
        # unmapped staging reference is dropped rather than written as a dangling FK.
        contract = payload.get("contract_id")
        if contract:
            payload["contract_id"] = id_map.get(str(contract))
    elif entry.action == "architecture.artifacts_approve":
        staging = payload.get("id")
        production = id_map.get(str(staging)) if staging else None
        if not production:
            raise PromoteError(
                f"architecture.artifacts_approve: artifact {staging!r} has no production "
                "id; the create entry must create or find it first"
            )
        payload["id"] = production
    elif entry.action == "documents.publish":
        staging = payload.get("id")
        production = id_map.get(staging) if staging else None
        if not production:
            raise PromoteError(
                f"document {staging!r} has no production id; the compose entry must "
                "create or find it first"
            )
        payload["id"] = production
    elif entry.action in ("projects.charter", "projects.baseline"):
        project_id = payload.get("project_id")
        if not project_id:
            raise PromoteError(f"{entry.action}: the input names no project_id")
        # A project the rework reopens is the same row in both targets, but the map is the
        # rule everywhere else and a stale id here would charter the wrong project.
        payload["project_id"] = id_map.get(str(project_id), str(project_id))
        if entry.action == "projects.baseline":
            # The WBS carries titles and WSJF inputs, never an id: the plan is what the
            # entry creates, so there is nothing to remap inside it.
            if not isinstance(payload.get("wbs"), list) or not payload["wbs"]:
                raise PromoteError(f"{entry.action}: the input names no wbs")
    elif entry.action == "wbs.update":
        node = wbs_deliverable(state, key.first, key.second or "")
        if node is None:
            raise PromoteError(
                f"wbs-accept: no production deliverable {key.second!r} in project "
                f"{key.first!r}; the baseline entry must create it first"
            )
        payload["id"] = node["id"]
        change_id = payload.get("change_id")
        if change_id:
            production_change = id_map.get(str(change_id))
            if not production_change:
                raise PromoteError(
                    f"wbs-accept: change_id {change_id!r} has no production mapping; "
                    "the change entry must create or find it first"
                )
            payload["change_id"] = production_change
    elif entry.action == "cards.move":
        card = state.cards.get(key.first)
        target_id = payload.get("list_id")
        # The journal's lane id is the staging copy's; the production lane is the one of the
        # SAME KIND on the same board. A kind is the matrix semantics; an id is not portable.
        resolved = None
        if card is not None:
            for lane_id, lane in state.lists.items():
                if lane.get("board_id") != card.get("board_id"):
                    continue
                if lane.get("kind") == key.second:
                    resolved = lane_id
                    break
        if resolved is None:
            if lane_kind(state, target_id) == key.second:
                resolved = target_id
        if resolved is None and not any(
            lane.get("board_id") == (card or {}).get("board_id")
            for lane in state.lists.values()
        ):
            # The lanes are unreadable (no action lists them). A restored board's lane ids are
            # the same rows in both targets, and the readback proves the card landed there.
            resolved = target_id
        if resolved is None:
            raise PromoteError(f"cards.move: no {key.second!r} lane on the card's board")
        payload["list_id"] = resolved
    elif entry.action == "approvals.request" and key.kind != "tranche_review_gate":
        evidence = payload.get("evidence")
        if isinstance(evidence, list):
            remapped: list[dict] = []
            for item in evidence:
                item = copy.deepcopy(item)
                staging = item.get("id") if isinstance(item, dict) else None
                if staging and id_map.get(str(staging)):
                    item["id"] = id_map[str(staging)]
                remapped.append(item)
            payload["evidence"] = remapped
    elif entry.action == "approvals.decide" and key.kind != "tranche_review_decide":
        staging_approval = payload.get("approval_id")
        production = id_map.get(staging_approval) if staging_approval else None
        if not production:
            known = change_index.get(key.second or "")
            production = known.get("approval_id") if known else None
        if not production:
            raise PromoteError(
                f"approval gate {staging_approval!r} has no production id; "
                "the change entry must create or find it first"
            )
        payload["approval_id"] = production
    elif entry.action == "approvals.decide" and key.kind == "tranche_review_decide":
        tranche = tranche_by_number(state, key.first)
        gate = tranche_review_gate(state, tranche.get("id") if tranche else None)
        if gate is None:
            raise PromoteError(
                f"tranche-review-decide:{key.first}: no gate on the production tranche"
            )
        payload["approval_id"] = gate["id"]
    elif entry.action == "changes.request" and payload.get("baseline_id"):
        staging = str(payload["baseline_id"])
        known = {str(row.get("id")) for row in state.baselines.get(
            str(payload.get("project_id")), []
        )}
        if staging not in known:
            kind = entry.meta.get("baseline_kind") or "scope"
            candidates = [
                row
                for row in state.baselines.get(str(payload.get("project_id")), [])
                if row.get("kind") == kind
            ]
            if not candidates:
                raise PromoteError(
                    f"changes.request: no {kind!r} baseline on project "
                    f"{payload.get('project_id')!r} to cite"
                )
            approved = [row for row in candidates if row.get("approved_by")]
            chosen = (approved or candidates)[-1]
            payload["baseline_id"] = chosen["id"]
    elif entry.action == "changes.request" and payload.get("baseline_id"):
        staging = str(payload["baseline_id"])
        known = {
            str(row.get("id"))
            for row in state.baselines.get(str(payload.get("project_id")), [])
        }
        if staging not in known:
            kind = entry.meta.get("baseline_kind") or "scope"
            candidates = [
                row
                for row in state.baselines.get(str(payload.get("project_id")), [])
                if row.get("kind") == kind
            ]
            if not candidates:
                raise PromoteError(
                    f"changes.request: no {kind!r} baseline on project "
                    f"{payload.get('project_id')!r} to cite"
                )
            approved = [row for row in candidates if row.get("approved_by")]
            chosen = (approved or candidates)[-1]
            payload["baseline_id"] = chosen["id"]
    elif entry.action == "cadences.create":
        programme = entry.meta.get("programme_id") or payload.get("programme_id")
        if not programme:
            raise PromoteError(f"cadence {key.first}/{key.second}: no programme to create in")
        payload["programme_id"] = programme
        meta = payload.get("meta")
        if isinstance(meta, dict) and meta.get("tranche_number") is not None:
            tranche = tranche_by_number(state, meta["tranche_number"])
            if tranche is None:
                raise PromoteError(
                    f"cadence {key.first}/{key.second}: no production tranche "
                    f"{meta.get('tranche_number')!r}"
                )
            meta["tranche_id"] = tranche["id"]
        if isinstance(meta, dict):
            payload["meta"] = map_ids(meta, id_map)
    elif entry.action == "cadences.update":
        row = cadence_row(state, key.first, key.second or "")
        if row is None:
            raise PromoteError(
                f"cadence {key.first}/{key.second}: no production row to update"
            )
        payload["id"] = row["id"]
        meta = payload.get("meta")
        if isinstance(meta, dict) and meta.get("tranche_number") is not None:
            tranche = tranche_by_number(state, meta["tranche_number"])
            if tranche is not None:
                meta["tranche_id"] = tranche["id"]
        if isinstance(meta, dict):
            payload["meta"] = map_ids(meta, id_map)
    elif entry.action == "cadences.approve_plan":
        cadence = cadence_row(state, key.first, "pi")
        if cadence is None:
            raise PromoteError(f"pi-plan:{key.first}: no production pi cadence")
        payload["cadence_id"] = cadence["id"]
    elif entry.action == "cadences.credit_demo":
        cadence = cadence_row(state, key.first, "demo")
        deliverable = wbs_deliverable(
            state, entry.meta.get("project_id"), key.second or ""
        )
        if cadence is None:
            raise PromoteError(f"demo-credit:{key.first}: no production demo cadence")
        if deliverable is None:
            raise PromoteError(
                f"demo-credit:{key.first}: no production deliverable {key.second!r} "
                f"in project {entry.meta.get('project_id')!r}"
            )
        payload["cadence_id"] = cadence["id"]
        payload["deliverable_id"] = deliverable["id"]
    elif entry.action in ("retros.record_findings", "retros.close"):
        cadence = cadence_row(state, key.first, "retro")
        if cadence is None:
            raise PromoteError(f"retro:{key.first}: no production retro cadence")
        payload["cadence_id"] = cadence["id"]
    elif entry.action == "lessons.propose":
        cadence = cadence_row(state, key.first, "retro")
        if cadence is None:
            raise PromoteError(f"lesson:{key.first}: no production retro cadence")
        payload["cadence_id"] = cadence["id"]
        programme = entry.meta.get("programme_id") or payload.get("programme_id")
        if not programme:
            raise PromoteError(f"lesson:{key.first}: no programme for the lesson")
        payload["programme_id"] = programme
    elif entry.action == "tranches.review":
        tranche = tranche_by_number(state, key.first)
        if tranche is None:
            raise PromoteError(f"tranche-review:{key.first}: no production tranche")
        payload["id"] = tranche["id"]
    elif entry.action == "tranches.authorise":
        staging = payload.get("review_id")
        production = id_map.get(staging) if staging else None
        if not production:
            reviewed = str(int(key.first) - 1) if key.first.isdigit() else ""
            predecessor = tranche_by_number(state, reviewed)
            review = review_for(
                state, predecessor.get("id") if predecessor else None
            )
            production = review.get("id") if review else None
        if not production:
            raise PromoteError(
                f"tranche-authorise:{key.first}: no production review to authorise from"
            )
        payload["review_id"] = production
    elif entry.action == "approvals.request" and key.kind == "tranche_review_gate":
        tranche = tranche_by_number(state, key.first)
        inner = payload.get("input")
        if tranche is None or not isinstance(inner, dict):
            raise PromoteError(
                f"tranche-review-gate:{key.first}: no production tranche to gate"
            )
        inner["id"] = tranche["id"]
    return payload


def output_id_for(action: str, output: Any) -> tuple[str, str | None]:
    """The created row's id and, for a change, its gate id."""
    if not isinstance(output, dict):
        raise PromoteError(f"{action}: response has no output object")
    if action == "changes.request":
        change_id = output.get("change_id")
        approval_id = output.get("approval_id")
        if not change_id:
            raise PromoteError("changes.request: output has no change_id")
        return str(change_id), str(approval_id) if approval_id else None
    if action == "projects.charter":
        approval_id = output.get("approval_id")
        if not approval_id:
            raise PromoteError("projects.charter: output has no approval_id")
        # The act's output is the gate it opened; the decide entry keys off it.
        return str(approval_id), None
    if action == "projects.baseline":
        approval_id = output.get("approval_id")
        baseline_id = output.get("baseline_id")
        if not approval_id or not baseline_id:
            raise PromoteError("projects.baseline: output has no baseline_id/approval_id")
        return str(approval_id), str(baseline_id)
    if action == "projects.close_request":
        project_id = output.get("project_id")
        if not project_id:
            raise PromoteError("projects.close_request: output has no project_id")
        return str(project_id), None
    if action == "projects.close":
        project = output.get("project")
        project_id = project.get("id") if isinstance(project, dict) else None
        if not project_id:
            raise PromoteError("projects.close: output has no project id")
        return str(project_id), None
    if action == "cadences.approve_plan":
        # The act's output is the gate it opened (or reused), not the cadence.
        approval_id = output.get("approval_id")
        if not approval_id:
            raise PromoteError("cadences.approve_plan: output has no approval_id")
        return str(approval_id), None
    row_id = output.get("id")
    if not row_id:
        raise PromoteError(f"{action}: output has no id")
    return str(row_id), None


def readback_entry(
    entry: Entry,
    client: Any,
    state: State,
    production_id: str,
    gate_id: str | None,
    snapshot: str | None,
    id_map: dict[str, str] | None = None,
) -> dict | None:
    """Read the created row back.  Raises PromoteError when it cannot be read."""
    key = entry.key
    assert key is not None
    # The card's own gates are read by card id, never by project: they hang on the card, and
    # the generic decide readback below would find no project for them.
    if entry.action == "approvals.request" and key.kind == "approval" and key.first == "card_done":
        gates = list_all(client, "approvals.list", {"card_id": key.second})
        state.card_gates[key.second or ""] = gates
        for gate in gates:
            if gate.get("id") != production_id:
                continue
            if gate.get("status") not in ("pending", "approved"):
                raise ReadbackError(
                    f"approvals.list: the card-done gate is {gate.get('status')!r}"
                )
            return gate
        raise ReadbackError("approvals.list: the card-done gate is not readable")
    if entry.action == "approvals.decide" and key.kind == "card_done_decide":
        gates = list_all(client, "approvals.list", {"card_id": key.first})
        state.card_gates[key.first] = gates
        for gate in gates:
            if gate.get("id") != production_id:
                continue
            if gate.get("status") != expected_gate_status(entry.payload):
                raise ReadbackError(
                    f"approvals.list: the card-done gate is {gate.get('status')!r}, not "
                    f"{expected_gate_status(entry.payload)!r}"
                )
            return gate
        raise ReadbackError("approvals.list: the decided card-done gate is not readable")
    if entry.action == "registers.create":
        return readback_register(client, production_id, "risk", code=key.first)
    if entry.action == "benefits.create":
        return readback_benefit(client, production_id, key.first)
    if entry.action == "benefits.update":
        row = readback_benefit(client, production_id, key.first)
        if key.second == "status":
            if row.get("status") != entry.payload.get("status"):
                raise ReadbackError(
                    f"benefits.get: status is {row.get('status')!r}, not "
                    f"{entry.payload.get('status')!r}"
                )
        else:
            for name, value in entry.payload.items():
                if name in ("id", "change_id"):
                    continue
                if row.get(name) != value:
                    raise ReadbackError(
                        f"benefits.get: field {name!r} is {row.get(name)!r}, not {value!r}"
                    )
        return row
    if entry.action == "changes.request":
        change = readback_register(
            client, production_id, "change", title=entry.payload.get("title")
        )
        project_id = entry.payload.get("project_id")
        if gate_id and project_id:
            readback_gate(client, project_id, gate_id)
        return change
    if entry.action == "approvals.decide" and key.kind == "tranche_review_decide":
        for gate in list_all(client, "approvals.list", {}):
            if gate.get("id") != production_id:
                continue
            if gate.get("status") != expected_gate_status(entry.payload):
                raise ReadbackError(
                    f"approvals.list: the review gate is {gate.get('status')!r}, not "
                    f"{expected_gate_status(entry.payload)!r}"
                )
            return gate
        raise ReadbackError("approvals.list: the decided review gate is not readable")
    if entry.action == "approvals.decide":
        project_id = None
        for known in state.approvals:
            if known.get("id") == production_id:
                project_id = known.get("project_id")
                break
        if not project_id:
            raise ReadbackError(
                "approvals.list: no project_id is known for the decided gate"
            )
        gate = readback_gate(client, project_id, production_id)
        if gate.get("status") != expected_gate_status(entry.payload):
            raise ReadbackError(
                f"approvals.list: gate is {gate.get('status')!r}, not "
                f"{expected_gate_status(entry.payload)!r}"
            )
        return gate
    if entry.action == "benefits.measure":
        return readback_measurement(client, state, entry, snapshot, production_id)
    if entry.action == "architecture.reviews_record":
        found = rows(client, "architecture.reviews_list", {"contract_id": key.first})
        state.compliance_reviews[key.first] = found
        for row in found:
            if row.get("id") != production_id:
                continue
            if row.get("level") != entry.payload.get("level"):
                raise ReadbackError(
                    f"architecture.reviews_list: level is {row.get('level')!r}, not "
                    f"{entry.payload.get('level')!r}"
                )
            return row
        raise ReadbackError("architecture.reviews_list: the review is not readable")
    if entry.action == "architecture.artifacts_create":
        row = read_row(client, "architecture.artifacts_get", production_id)
        if str(row.get("artifact_type_id")) != key.second:
            raise ReadbackError(
                f"architecture.artifacts_get: type is {row.get('artifact_type_id')!r}, "
                f"not {key.second!r}"
            )
        return row
    if entry.action == "architecture.artifacts_approve":
        row = read_row(client, "architecture.artifacts_get", production_id)
        if row.get("status") != "approved":
            raise ReadbackError(
                f"architecture.artifacts_get: status is {row.get('status')!r}, "
                "not 'approved'"
            )
        return row
    if entry.action == "wbs.update":
        nodes = rows(client, "wbs.list", {"project_id": key.first})
        for node in nodes:
            if node.get("id") != production_id:
                continue
            if node.get("status") != entry.payload.get("status"):
                raise ReadbackError(
                    f"wbs.list: status is {node.get('status')!r}, not "
                    f"{entry.payload.get('status')!r}"
                )
            if not node.get("evidence"):
                raise ReadbackError("wbs.list: the accepted node carries no evidence")
            state.wbs[key.first] = nodes
            return node
        raise ReadbackError("wbs.list: the updated node is not readable")
    if entry.action in ("projects.charter", "projects.baseline"):
        gate = readback_gate(client, key.first, production_id)
        if not any(item.get("id") == production_id for item in state.approvals):
            state.approvals.append(gate)
        if entry.action == "projects.baseline":
            # The plan's own rows are what the change after it cites; a gate over an empty
            # plan would replay the change with no deliverable behind it.
            baselines = rows(client, "baselines.list", {"project_id": key.first})
            if not any(row.get("kind") == "scope" for row in baselines):
                raise ReadbackError(
                    "baselines.list: no scope baseline after projects.baseline"
                )
            state.baselines[key.first] = baselines
            # The plan's rows are what the acceptances and the change after it resolve
            # against; a stale WBS map would make the very next entry fail.
            state.wbs[key.first] = rows(client, "wbs.list", {"project_id": key.first})
        return gate
    if entry.action == "projects.close_request":
        project_id = entry.payload.get("id")
        if not project_id:
            raise ReadbackError("projects.close_request: the input names no project")
        project = read_project(client, str(project_id))
        # Record the gate as the server has it — pending until the closure decide runs.
        # A fabricated "approved" row makes that decide skip and leaves the draft closure
        # asset unapproved, which the destructive gate's evidence check then refuses.
        gate = readback_gate(client, str(project_id), gate_id) if gate_id else None
        if gate is not None and closure_gate(state, str(project_id)) is None:
            state.approvals.append(gate)
        return project
    if entry.action == "approvals.request" and key.kind == "tranche_review_gate":
        for gate in list_all(client, "approvals.list", {}):
            if gate.get("id") != production_id:
                continue
            if gate.get("status") not in ("pending", "approved"):
                raise ReadbackError(
                    f"approvals.list: the review gate is {gate.get('status')!r}, not pending"
                )
            return gate
        raise ReadbackError("approvals.list: the tranche-review gate is not readable")
    if entry.action == "approvals.request":
        project_id = entry.payload.get("project_id")
        if not project_id:
            raise ReadbackError("approvals.request: the input names no project")
        gate = readback_gate(client, str(project_id), production_id)
        if gate.get("status") not in ("pending", "approved"):
            raise ReadbackError(
                f"approvals.list: gate is {gate.get('status')!r}, not pending"
            )
        state.approvals.append(gate)
        return gate
    if entry.action == "projects.close":
        project = read_project(client, production_id)
        if project.get("status") != "closed":
            raise ReadbackError(
                f"projects.get: status is {project.get('status')!r}, not closed"
            )
        state.projects[production_id] = project
        return project
    if entry.action == "cards.comment":
        card = read_card(client, key.first)
        for comment in card.get("comments") or []:
            body = comment.get("body") if isinstance(comment, dict) else None
            if isinstance(body, str):
                digest = hashlib.sha256(body.encode("utf-8")).hexdigest()
                comment["_sha"] = digest
                if comment.get("id") == production_id or digest.startswith(key.second):
                    state.cards[key.first] = card
                    return comment
        raise ReadbackError("cards.get: the comment is not on the card")
    if entry.action == "cards.move":
        card = read_card(client, key.first)
        kind = lane_kind(state, card.get("list_id"))
        if kind is None:
            previous = state.cards.get(key.first) or {}
            if not card.get("list_id") or card.get("list_id") == previous.get("list_id"):
                raise ReadbackError("cards.get: the card did not move")
        elif kind != key.second:
            raise ReadbackError(
                f"cards.get: the card is in a {kind!r} lane, not {key.second!r}"
            )
        state.cards[key.first] = card
        return card
    if entry.action == "decisions.create":
        row = read_row(client, "decisions.get", production_id)
        if row.get("key") != key.first:
            raise ReadbackError(
                f"decisions.get: key is {row.get('key')!r}, not {key.first!r}"
            )
        state.decisions.append(row)
        return row
    if entry.action == "decisions.transition":
        row = read_row(client, "decisions.get", production_id)
        if row.get("status") != key.second:
            raise ReadbackError(
                f"decisions.get: status is {row.get('status')!r}, not {key.second!r}"
            )
        state.decisions = [
            row if item.get("id") == production_id else item
            for item in state.decisions
        ]
        return row
    if entry.action in ("documents.compose", "documents.publish"):
        row = read_row(client, "documents.get", production_id)
        if entry.action == "documents.compose":
            if row.get("title") != entry.payload.get("title"):
                raise ReadbackError(
                    f"documents.get: title is {row.get('title')!r}, not "
                    f"{entry.payload.get('title')!r}"
                )
        elif not row.get("published_at"):
            raise ReadbackError("documents.get: the document is not published")
        return row
    if entry.action == "registers.update":
        row = read_row(client, "registers.get", production_id)
        for name, value in entry.payload.items():
            if name in ("id", "change_id"):
                continue
            if row.get(name) != value:
                raise ReadbackError(
                    f"registers.get: field {name!r} is {row.get(name)!r}, not {value!r}"
                )
        return row
    if entry.action == "benefits.link":
        programme_id = entry.payload.get("programme_id")
        for row in list_all(client, "benefits.link_list", {"programme_id": programme_id}):
            if row.get("id") == production_id:
                if row.get("output_ref") != entry.payload.get("output_ref"):
                    raise ReadbackError(
                        "benefits.link_list: the link's output_ref differs from the input"
                    )
                return row
        raise ReadbackError("benefits.link_list: the link is not on the programme")
    if entry.action in ("cadences.create", "cadences.update"):
        programme = entry.meta.get("programme_id") or entry.payload.get("programme_id")
        if not programme:
            raise ReadbackError(f"{entry.action}: the entry names no programme")
        for row in rows(client, "cadences.list", {"programme_id": programme}):
            if row.get("id") != production_id:
                continue
            if entry.action == "cadences.create":
                if row.get("kind") != key.second or row.get("period") != key.first:
                    raise ReadbackError(
                        "cadences.list: the row is not the kind/period the key names"
                    )
            elif row.get("meta") != expected_cadence_meta(state, entry, id_map):
                raise ReadbackError("cadences.list: the cadence meta is not the input's")
            return row
        raise ReadbackError("cadences.list: the cadence is not readable")
    if entry.action == "cadences.approve_plan":
        for gate in list_all(client, "approvals.list", {}):
            if gate.get("id") != production_id:
                continue
            if gate.get("status") != "approved":
                raise ReadbackError(
                    f"approvals.list: the plan gate is {gate.get('status')!r}, not approved"
                )
            return gate
        raise ReadbackError("approvals.list: the plan gate is not readable")
    if entry.action == "cadences.credit_demo":
        cadence = cadence_row(state, key.first, "demo")
        cadence_id = cadence.get("id") if cadence else None
        if not cadence_id:
            raise ReadbackError("cadences.credit_list: no production demo cadence")
        deliverable = wbs_deliverable(
            state, entry.meta.get("project_id"), key.second or ""
        )
        for credit in rows(client, "cadences.credit_list", {"cadence_id": cadence_id}):
            if credit.get("id") != production_id:
                continue
            if deliverable is not None and credit.get("deliverable_id") != deliverable.get("id"):
                raise ReadbackError(
                    "cadences.credit_list: the credit is on another deliverable"
                )
            return credit
        raise ReadbackError("cadences.credit_list: the credit is not readable")
    if entry.action == "retros.record_findings":
        cadence = cadence_row(state, key.first, "retro")
        if cadence is None:
            raise ReadbackError("retros.record_findings: no production retro cadence")
        card = readback_retro_card(client, cadence, entry.payload.get("findings") or [])
        return card
    if entry.action == "retros.close":
        programme = entry.meta.get("programme_id") or entry.payload.get("programme_id")
        for row in rows(client, "cadences.list", {"programme_id": programme}):
            if row.get("id") != production_id:
                continue
            if row.get("state") != "closed":
                raise ReadbackError(
                    f"cadences.list: the retro is {row.get('state')!r}, not closed"
                )
            return row
        raise ReadbackError("cadences.list: the closed retro is not readable")
    if entry.action == "lessons.propose":
        programme = entry.meta.get("programme_id")
        for row in rows(client, "lessons.list", {"programme_id": programme}):
            if row.get("id") != production_id:
                continue
            if row.get("finding_key") != key.second:
                raise ReadbackError(
                    f"lessons.list: the lesson is {row.get('finding_key')!r}, "
                    f"not {key.second!r}"
                )
            return row
        raise ReadbackError("lessons.list: the lesson is not readable")
    if entry.action == "tranches.review":
        programme = entry.meta.get("programme_id")
        for row in rows(client, "tranches.review_list", {"programme_id": programme}):
            if row.get("id") != production_id:
                continue
            if row.get("decision") != entry.payload.get("decision"):
                raise ReadbackError(
                    f"tranches.review_list: the decision is {row.get('decision')!r}, "
                    f"not {entry.payload.get('decision')!r}"
                )
            return row
        raise ReadbackError("tranches.review_list: the review is not readable")
    if entry.action == "tranches.authorise":
        programme = entry.meta.get("programme_id")
        for row in rows(client, "tranches.list", {"programme_id": programme}):
            if row.get("id") != production_id:
                continue
            if row.get("state") != "authorised":
                raise ReadbackError(
                    f"tranches.list: the tranche is {row.get('state')!r}, not authorised"
                )
            return row
        raise ReadbackError("tranches.list: the authorised tranche is not readable")
    if entry.action == "governed_assets.compose":
        # The compose output is the asset itself, so the generic output-id helper already
        # handed us its id; `get` flattens AssetOut, so it reads back through `read_row`.
        row = read_row(client, "governed_assets.get", production_id)
        if row.get("kind") != entry.payload.get("kind"):
            raise ReadbackError(
                f"governed_assets.get: kind is {row.get('kind')!r}, not "
                f"{entry.payload.get('kind')!r}"
            )
        if row.get("title") != entry.payload.get("title"):
            raise ReadbackError(
                f"governed_assets.get: title is {row.get('title')!r}, not "
                f"{entry.payload.get('title')!r}"
            )
        return row
    if entry.action in ("governed_assets.submit", "governed_assets.approve"):
        # The input id is the staging asset id; the write went to its mapped production id.
        staging = str(entry.payload.get("id") or "")
        asset_id = (id_map or {}).get(staging, staging)
        row = read_row(client, "governed_assets.get", asset_id)
        wanted = "in_review" if entry.action == "governed_assets.submit" else "approved"
        if row.get("status") != wanted:
            raise ReadbackError(
                f"governed_assets.get: status is {row.get('status')!r}, not {wanted!r}"
            )
        return row
    raise PromoteError(f"{entry.action}: no readback is defined")


def apply_entries(
    client: Any,
    entries: list[Entry],
    state: State,
    snapshot: str | None,
) -> tuple[dict[str, str], list[Outcome]]:
    id_map: dict[str, str] = {}
    change_index = build_change_index(entries, state)
    superseded = superseded_fields(entries)
    outcomes: list[Outcome] = []
    for entry in entries:
        key = entry.key
        assert key is not None
        actor = str(entry.raw.get("actor") or "") or None
        try:
            if entry.action == "architecture.artifacts_create":
                found = rows(
                    client, "architecture.artifacts_list", {"project_id": key.first}
                )
                existing = next(
                    (
                        row
                        for row in found
                        if str(row.get("artifact_type_id")) == key.second
                    ),
                    None,
                )
            elif entry.action == "architecture.artifacts_approve":
                production = id_map.get(key.first)
                row = (
                    read_row(client, "architecture.artifacts_get", production)
                    if production
                    else None
                )
                existing = row if row and row.get("status") == "approved" else None
            else:
                existing = existing_for(
                    entry, state, change_index, id_map, superseded.get(entry.lineno)
                )
            if existing is not None:
                production_id = str(existing["id"])
                id_map[entry.output_id] = production_id
                if entry.action == "changes.request":
                    gate = gate_for_change(state, existing)
                    change_index[key.first] = {
                        "change_id": production_id,
                        "approval_id": str(gate["id"]) if gate else None,
                    }
                    if entry.output_id:
                        change_index[str(entry.output_id)] = change_index[key.first]
                    if entry.raw.get("approval_id") and gate is not None:
                        id_map[entry.raw["approval_id"]] = str(gate["id"])
                outcomes.append(
                    Outcome(entry, entry.output_id, production_id, "skipped")
                )
                continue

            payload = remap_payload(entry, state, id_map, change_index)
            approval = None
            if entry.action == "projects.close":
                project_id = str(payload.get("id") or "")
                expected_target = f"project/{project_id}/close"
                for gate in state.approvals:
                    if (
                        gate.get("gate") == "projects.close"
                        and gate.get("target") == expected_target
                        and gate.get("status") == "approved"
                    ):
                        approval = gate.get("id")
                        break
                if not approval:
                    raise PromoteError(
                        f"projects.close: no approved gate for {expected_target}"
                    )
            if entry.action == "tranches.review":
                approval = id_map.get(str(entry.raw.get("approval_id") or ""))
                if not approval:
                    tranche = tranche_by_number(state, key.first)
                    gate = tranche_review_gate(
                        state, tranche.get("id") if tranche else None
                    )
                    approval = gate.get("id") if gate else None
                if not approval:
                    raise PromoteError(
                        f"tranche-review:{key.first}: no production gate to review under"
                    )
            if entry.action == "cards.move":
                current = read_card(client, str(payload["id"]))
                version = current.get("version")
                if not isinstance(version, int):
                    raise PromoteError(
                        f"cards.get: {payload['id']} has no version for If-Match"
                    )
                output = client.invoke(entry.action, payload, if_match=version, actor=actor)
            elif entry.action == "wbs.update":
                current = read_row(client, "wbs.get", str(payload["id"]))
                version = current.get("version")
                if not isinstance(version, int):
                    raise PromoteError(
                        f"wbs.get: {payload['id']} has no version for If-Match"
                    )
                output = client.invoke(entry.action, payload, if_match=version, actor=actor)
            elif entry.action == "architecture.artifacts_approve":
                current = read_row(
                    client, "architecture.artifacts_get", str(payload["id"])
                )
                version = current.get("version")
                if not isinstance(version, int):
                    raise PromoteError(
                        f"architecture.artifacts_get: {payload['id']} has no version "
                        "for If-Match"
                    )
                output = client.invoke(entry.action, payload, if_match=version, actor=actor)
            elif entry.action == "documents.publish":
                current = read_row(client, "documents.get", str(payload["id"]))
                version = current.get("version")
                if not isinstance(version, int):
                    raise PromoteError(
                        f"documents.get: {payload['id']} has no version for If-Match"
                    )
                output = client.invoke(entry.action, payload, if_match=version, actor=actor)
            elif entry.action == "registers.update":
                current = read_row(client, "registers.get", str(payload["id"]))
                version = current.get("version")
                if not isinstance(version, int):
                    raise PromoteError(
                        f"registers.get: {payload['id']} has no version for If-Match"
                    )
                output = client.invoke(entry.action, payload, if_match=version, actor=actor)
            elif entry.action == "benefits.update":
                current = read_row(client, "benefits.get", payload["id"])
                version = current.get("version")
                if not isinstance(version, int):
                    raise PromoteError(
                        f"benefits.get: {payload['id']} has no version for If-Match"
                    )
                output = client.invoke(entry.action, payload, if_match=version, actor=actor)
            elif entry.action == "cadences.update":
                row = cadence_row(state, key.first, key.second or "")
                version = row.get("version") if row else None
                if not isinstance(version, int):
                    raise PromoteError(
                        f"cadences.update: {key.first}/{key.second} has no version "
                        "for If-Match"
                    )
                output = client.invoke(entry.action, payload, if_match=version, actor=actor)
            else:
                output = client.invoke(
                    entry.action, payload, approval_id=approval, actor=actor
                )
            production_id, gate_id = output_id_for(entry.action, output)
            if entry.action == "tranches.review":
                # The action returns the tranche; the review it recorded is only readable
                # through review_list, and it is the review id the authorisation needs.
                tranche_id = str(output.get("id")) if isinstance(output, dict) else ""
                review = fresh_review(
                    client,
                    entry.meta.get("programme_id"),
                    tranche_id,
                    entry.payload.get("decision"),
                )
                if review is None:
                    raise ReadbackError(
                        "tranches.review_list: the review just recorded is not readable"
                    )
                production_id = str(review["id"])
            id_map[entry.output_id] = production_id
            if entry.action == "projects.close_request":
                asset = output.get("closure_asset_id") if isinstance(output, dict) else None
                if asset:
                    id_map[str(asset)] = str(asset)
                staging_asset = entry.raw.get("closure_asset_id")
                if staging_asset and asset:
                    id_map[str(staging_asset)] = str(asset)
                gate_id = (
                    str(output.get("approval_id"))
                    if isinstance(output, dict) and output.get("approval_id")
                    else None
                )
                staging_gate = entry.raw.get("approval_id")
                if staging_gate and gate_id:
                    id_map[str(staging_gate)] = str(gate_id)
            if entry.action == "changes.request":
                change_index[key.first] = {
                    "change_id": production_id,
                    "approval_id": gate_id,
                }
                if entry.output_id:
                    change_index[str(entry.output_id)] = change_index[key.first]
                if entry.raw.get("approval_id") and gate_id:
                    id_map[entry.raw["approval_id"]] = gate_id

            row = readback_entry(
                entry, client, state, production_id, gate_id, snapshot, id_map
            )
            if row is not None:
                if entry.action == "registers.create":
                    state.registers.append(row)
                elif entry.action == "benefits.create":
                    state.benefits.append(row)
                elif entry.action == "benefits.update":
                    state.benefits = [
                        row if item.get("id") == production_id else item
                        for item in state.benefits
                    ]
                elif entry.action == "changes.request":
                    state.registers.append(row)
                    if gate_id:
                        project_id = entry.payload.get("project_id")
                        if project_id:
                            for gate in list_all(
                                client, "approvals.list", {"project_id": project_id}
                            ):
                                if gate.get("id") == gate_id:
                                    state.approvals.append(gate)
                                    break
                elif entry.action == "approvals.decide":
                    state.approvals = [
                        row if item.get("id") == production_id else item
                        for item in state.approvals
                    ]
                elif entry.action in (
                    "cadences.create",
                    "cadences.update",
                    "retros.close",
                ):
                    upsert_row(state.cadences, row)
                elif entry.action == "cadences.approve_plan":
                    upsert_row(state.approvals, row)
                elif entry.action == "cadences.credit_demo":
                    cadence = cadence_row(state, key.first, "demo")
                    cadence_id = cadence.get("id") if cadence else None
                    if cadence_id:
                        upsert_row(state.credits.setdefault(cadence_id, []), row)
                elif entry.action == "lessons.propose":
                    upsert_row(state.lessons, row)
                elif entry.action == "tranches.review":
                    upsert_row(state.reviews, row)
                elif entry.action == "tranches.authorise":
                    upsert_row(state.tranches, row)
                elif entry.action == "approvals.request" and key.kind == "tranche_review_gate":
                    upsert_row(state.approvals, row)
            outcomes.append(
                Outcome(entry, entry.output_id, production_id, "created")
            )
        except ReadbackError as error:
            outcomes.append(
                Outcome(entry, entry.output_id, id_map.get(entry.output_id),
                        "readback_failed", str(error))
            )
        except PromoteError as error:
            outcomes.append(
                Outcome(entry, entry.output_id, id_map.get(entry.output_id),
                        "failed", str(error))
            )
    return id_map, outcomes


# ── promotion file ─────────────────────────────────────────────────────────────────────


def build_promotion(
    entries: list[Entry],
    id_map: dict[str, str],
    outcomes: list[Outcome],
    journal_path: Path,
    url: str | None,
    snapshot: str | None,
    local_bin: str | None = None,
) -> dict:
    verdicts = {}
    for outcome in outcomes:
        verdicts[outcome.entry.natural_key] = outcome.verdict
    return {
        "schema": PROMOTION_SCHEMA,
        "generated_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "journal": {
            "path": str(journal_path),
            "sha256": hashlib.sha256(journal_path.read_bytes()).hexdigest(),
            "entries": len(entries),
        },
        "target": {"url": url, "snapshot": snapshot, "local_bin": local_bin},
        "map": {key: id_map[key] for key in sorted(id_map)},
        "entries": [
            {
                "line": outcome.entry.lineno,
                "action": outcome.entry.action,
                "natural_key": outcome.entry.natural_key,
                "staging_id": outcome.staging_id,
                "production_id": outcome.production_id,
                "verdict": outcome.verdict,
                **({"detail": outcome.detail} if outcome.detail else {}),
            }
            for outcome in outcomes
        ],
        "complete": all(
            verdict in ("created", "skipped") for verdict in verdicts.values()
        ),
    }


def write_promotion(path: Path, document: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(
        prefix=".promotion-", suffix=".json", dir=str(path.parent)
    )
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
            json.dump(document, handle, indent=2, sort_keys=False)
            handle.write("\n")
        os.replace(temporary, path)
    except BaseException:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass
        raise


# ── self-test ──────────────────────────────────────────────────────────────────────────


class FakeClient:
    """An in-memory product: fake rows, fake read actions, same invoke signature."""

    def __init__(self, state: State, unreadable: set[str] | None = None) -> None:
        self.state = state
        self.calls: list[tuple[str, dict, int | None, str | None]] = []
        self.actors: list[str | None] = []
        self.unreadable = unreadable or set()
        self.counter = 0

    def _id(self, prefix: str) -> str:
        self.counter += 1
        return f"00000000-0000-4000-8000-{self.counter:012d}-{prefix}"

    def _rows(self, rows: list[dict], payload: dict) -> list[dict]:
        selected = rows
        if payload.get("programme_id"):
            selected = [
                row for row in selected
                if row.get("programme_id") == payload["programme_id"]
            ]
        if payload.get("project_id"):
            selected = [
                row for row in selected
                if row.get("project_id") == payload["project_id"]
            ]
        return [copy.deepcopy(row) for row in selected]

    def invoke(
        self,
        action: str,
        payload: dict,
        if_match: int | None = None,
        approval_id: str | None = None,
        actor: str | None = None,
    ) -> Any:
        self.calls.append(
            (action, copy.deepcopy(payload), if_match, approval_id)
        )
        self.actors.append(actor)
        if action == "registers.list":
            return {"items": self._rows(self.state.registers, payload)}
        if action == "benefits.list":
            return {"items": self._rows(self.state.benefits, payload)}
        if action == "approvals.list":
            return {
                "items": self._rows(self.state.approvals, payload),
                "next_cursor": None,
            }
        if action == "registers.get":
            if "registers.get" in self.unreadable:
                return None
            for row in self.state.registers:
                if row.get("id") == payload.get("id"):
                    return copy.deepcopy(row)
            return None
        if action == "benefits.get":
            if "benefits.get" in self.unreadable:
                return None
            for row in self.state.benefits:
                if row.get("id") == payload.get("id"):
                    return copy.deepcopy(row)
            return None
        if action == "registers.create":
            row = copy.deepcopy(payload)
            row.update(id=self._id("register"), version=1)
            row.setdefault("status", "open")
            self.state.registers.append(row)
            return copy.deepcopy(row)
        if action == "benefits.create":
            row = copy.deepcopy(payload)
            row.update(id=self._id("benefit"), version=1)
            row.setdefault("status", "draft")
            self.state.benefits.append(row)
            return copy.deepcopy(row)
        if action == "decisions.create":
            row = copy.deepcopy(payload)
            row.update(id=self._id("decision"), version=1, status="draft")
            self.state.decisions.append(row)
            return copy.deepcopy(row)
        if action == "decisions.transition":
            for row in self.state.decisions:
                if row.get("id") == payload.get("id"):
                    if row.get("status") != payload.get("to"):
                        row["status"] = payload.get("to")
                        row["version"] = int(row.get("version") or 1) + 1
                    return copy.deepcopy(row)
            raise AssertionError(f"decisions.transition: no decision {payload.get('id')}")
        if action == "decisions.list":
            return {"items": self._rows(self.state.decisions, payload), "next_cursor": None}
        if action == "decisions.get":
            for row in self.state.decisions:
                if row.get("id") == payload.get("id"):
                    return copy.deepcopy(row)
            return None
        if action == "documents.compose":
            row = copy.deepcopy(payload)
            row.update(id=self._id("document"), version=1, status="draft", published_at=None)
            self.state.documents.append(row)
            return copy.deepcopy(row)
        if action == "documents.publish":
            for row in self.state.documents:
                if row.get("id") == payload.get("id"):
                    row["status"] = "active"
                    row["published_at"] = "2026-11-22T00:00:00Z"
                    row["version"] = int(row.get("version") or 1) + 1
                    return copy.deepcopy(row)
            raise AssertionError(f"documents.publish: no document {payload.get('id')}")
        if action == "documents.list":
            return {"items": self._rows(self.state.documents, payload), "next_cursor": None}
        if action == "documents.get":
            for row in self.state.documents:
                if row.get("id") == payload.get("id"):
                    return copy.deepcopy(row)
            return None
        if action == "governed_assets.compose":
            row = copy.deepcopy(payload)
            row.update(
                id=self._id("governed-asset"), key=payload.get("title"),
                status="draft", version=1,
            )
            self.state.governed_assets.append(row)
            return copy.deepcopy(row)
        if action == "governed_assets.get":
            for row in self.state.governed_assets:
                if row.get("id") == payload.get("id"):
                    return copy.deepcopy(row)
            return None
        if action in ("governed_assets.submit", "governed_assets.approve"):
            wanted = "in_review" if action.endswith("submit") else "approved"
            for row in self.state.governed_assets:
                if row.get("id") == payload.get("id"):
                    row["status"] = wanted
                    row["version"] = int(row.get("version") or 1) + 1
                    return copy.deepcopy(row)
            raise AssertionError(f"{action}: no governed asset {payload.get('id')}")
        if action == "registers.update":
            for row in self.state.registers:
                if row.get("id") == payload.get("id"):
                    for name, value in payload.items():
                        if name != "id":
                            row[name] = value
                    row["version"] = int(row.get("version") or 1) + 1
                    return copy.deepcopy(row)
            raise AssertionError(f"registers.update: no row {payload.get('id')}")
        if action == "benefits.link":
            row = copy.deepcopy(payload)
            row.update(id=self._id("link"))
            self.state.links.append(row)
            return copy.deepcopy(row)
        if action == "benefits.link_list":
            return {"items": self._rows(self.state.links, payload), "next_cursor": None}
        if action == "benefits.measure":
            row = copy.deepcopy(payload)
            row.update(id=self._id("measurement"))
            self.state.measurements.append(row)
            return copy.deepcopy(row)
        if action == "benefits.measurement_list":
            if "benefits.measurement_list" in self.unreadable:
                raise PromoteError("benefits.measurement_list: not served by this image")
            return {
                "items": [
                    copy.deepcopy(row)
                    for row in self.state.measurements
                    if row.get("benefit_id") == payload.get("benefit_id")
                    or (
                        payload.get("programme_id")
                        and row.get("programme_id") == payload.get("programme_id")
                    )
                ],
                "next_cursor": None,
            }
        if action == "changes.request":
            change_id = self._id("change")
            gate_id = self._id("gate")
            change = {
                "id": change_id,
                "kind": "change",
                "code": "",
                "title": payload["title"],
                "project_id": payload.get("project_id"),
                "programme_id": None,
                "status": "open",
                "version": 1,
                "meta": {"impact": payload.get("impact")},
            }
            gate = {
                "id": gate_id,
                "gate": "change",
                "status": "pending",
                "project_id": payload.get("project_id"),
                "target": None,
                "description": f"Approve change {change_id}",
                "proposed_input": {"change_id": change_id},
                "version": 1,
            }
            self.state.registers.append(change)
            self.state.approvals.append(gate)
            return {"change_id": change_id, "approval_id": gate_id}
        if action == "approvals.decide":
            for gate in self.state.approvals:
                if gate.get("id") == payload.get("approval_id"):
                    if gate.get("status") != "pending":
                        raise PromoteError(
                            "approvals.decide: approval is "
                            f"{gate.get('status')}; only a pending gate can change"
                        )
                    gate["status"] = expected_gate_status(payload)
                    gate["version"] += 1
                    if gate["status"] == "approved" and gate.get("gate") == "baseline":
                        # The engine's gate effect approves the baseline the gate names.
                        for row in self.state.baselines.get(gate.get("project_id"), []):
                            if row.get("approved_by") is None:
                                row["approved_by"] = "approver"
                    return copy.deepcopy(gate)
            raise PromoteError(f"approvals.decide: no gate {payload.get('approval_id')}")
        if action == "benefits.update":
            for row in self.state.benefits:
                if row.get("id") == payload.get("id"):
                    if if_match != row.get("version"):
                        raise PromoteError("benefits.update: If-Match is stale")
                    for name, value in payload.items():
                        if name in ("id", "change_id"):
                            continue
                        row[name] = value
                    row["version"] += 1
                    return copy.deepcopy(row)
            raise PromoteError("benefits.update: no benefit")
        if action == "projects.get":
            project = self.state.projects.get(payload.get("id"))
            if project is None:
                return None
            return {"project": copy.deepcopy(project), "boards": []}
        if action == "lists.list":
            rows = [
                row for row in self.state.lists.values()
                if row.get("board_id") == payload.get("board_id")
            ]
            return {"items": copy.deepcopy(rows), "next_cursor": None}
        if action == "cards.get":
            card = self.state.cards.get(payload.get("id"))
            if card is None:
                return None
            # The product answers `{card: {…}}`; the fake must not be kinder than the engine.
            return {"card": copy.deepcopy(card)}
        if action == "projects.close_request":
            asset = self._id("closure")
            gate = {
                "id": self._id("closure-gate"),
                "gate": "closure",
                "status": "pending",
                "project_id": payload.get("id"),
                "target": None,
                "description": "Close the fixture project",
                "version": 1,
            }
            self.state.approvals.append(gate)
            return {
                "project_id": payload.get("id"),
                "status": "requested",
                "closure_asset_id": asset,
                "approval_id": gate["id"],
            }
        if action == "approvals.request":
            gate = {
                "id": self._id("gate"),
                "gate": payload.get("gate"),
                "status": "pending",
                "project_id": payload.get("project_id"),
                "target": requested_target(payload),
                "description": payload.get("description"),
                "proposed_input": payload.get("proposed_input"),
                "version": 1,
            }
            self.state.approvals.append(gate)
            return {"id": gate["id"], "gate": gate["gate"], "status": gate["status"]}
        if action == "projects.close":
            project = self.state.projects.get(payload.get("id"))
            if project is None:
                raise PromoteError("projects.close: no project")
            project["status"] = "closed"
            return {"project": copy.deepcopy(project), "boards": []}
        if action == "cards.comment":
            card = self.state.cards.get(payload.get("id"))
            if card is None:
                raise PromoteError("cards.comment: no card")
            comment = {"id": self._id("comment"), "body": payload.get("body")}
            card.setdefault("comments", []).append(comment)
            return copy.deepcopy(comment)
        if action == "cards.move":
            card = self.state.cards.get(payload.get("id"))
            if card is None:
                raise PromoteError("cards.move: no card")
            if if_match != card.get("version"):
                raise PromoteError("cards.move: If-Match is stale")
            card["list_id"] = payload.get("list_id")
            card["version"] = int(card.get("version") or 0) + 1
            return copy.deepcopy(card)
        if action == "cadences.list":
            return {
                "items": [
                    copy.deepcopy(row)
                    for row in self.state.cadences
                    if row.get("programme_id") == payload.get("programme_id")
                ],
                "next_cursor": None,
            }
        if action == "cadences.create":
            row = copy.deepcopy(payload)
            row.update(id=self._id("cadence"), state="planned", version=1)
            self.state.cadences.append(row)
            return copy.deepcopy(row)
        if action == "cadences.update":
            for row in self.state.cadences:
                if row.get("id") != payload.get("id"):
                    continue
                if if_match != row.get("version"):
                    raise PromoteError("cadences.update: If-Match is stale")
                for name in ("period", "state", "meta"):
                    if payload.get(name) is not None:
                        row[name] = payload[name]
                row["version"] = int(row.get("version") or 1) + 1
                return copy.deepcopy(row)
            raise PromoteError("cadences.update: no cadence")
        if action == "cadences.approve_plan":
            cadence = next(
                (
                    row
                    for row in self.state.cadences
                    if row.get("id") == payload.get("cadence_id")
                ),
                None,
            )
            if cadence is None:
                raise PromoteError("cadences.approve_plan: no cadence")
            gate = plan_gate(self.state, cadence["id"])
            if gate is None:
                gate = {
                    "id": self._id("plan-gate"),
                    "gate": PI_PLAN_GATE,
                    "status": "pending",
                    "project_id": None,
                    "target": cadence["id"],
                    "version": 1,
                }
                self.state.approvals.append(gate)
            if payload.get("decision") == "approve":
                gate["status"] = "approved"
                cadence["state"] = "awaiting_approval"
            return {
                "cadence_id": cadence["id"],
                "approval_id": gate["id"],
                "status": gate["status"],
                "decision": payload.get("decision"),
                "state": cadence["state"],
                "version": gate.get("version", 1),
            }
        if action == "cadences.credit_demo":
            credits = self.state.credits.setdefault(payload["cadence_id"], [])
            for row in credits:
                if row.get("deliverable_id") == payload.get("deliverable_id"):
                    return copy.deepcopy(row)
            row = {
                "id": self._id("credit"),
                "cadence_id": payload["cadence_id"],
                "deliverable_id": payload["deliverable_id"],
                "version": 1,
            }
            credits.append(row)
            return copy.deepcopy(row)
        if action == "cadences.credit_list":
            return {
                "items": copy.deepcopy(
                    self.state.credits.get(payload.get("cadence_id"), [])
                ),
                "next_cursor": None,
            }
        if action == "retros.record_findings":
            cadence = next(
                (
                    row
                    for row in self.state.cadences
                    if row.get("id") == payload.get("cadence_id")
                ),
                None,
            )
            if cadence is None:
                raise PromoteError("retros.record_findings: no cadence")
            board = (cadence.get("meta") or {}).get("board_id")
            card = next(
                (
                    row
                    for row in self.state.board_cards.get(board, [])
                    if (row.get("meta") or {}).get("cadence_id") == cadence["id"]
                ),
                None,
            )
            if card is None:
                card = {
                    "id": self._id("retro-card"),
                    "board_id": board,
                    "meta": {"cadence_id": cadence["id"]},
                    "version": 1,
                }
                self.state.board_cards.setdefault(board, []).append(card)
                self.state.cards[card["id"]] = card
            card["meta"]["findings"] = copy.deepcopy(payload.get("findings"))
            card["version"] = int(card.get("version") or 1) + 1
            return copy.deepcopy(cadence)
        if action == "retros.close":
            for row in self.state.cadences:
                if row.get("id") == payload.get("cadence_id"):
                    row["state"] = "closed"
                    return copy.deepcopy(row)
            raise PromoteError("retros.close: no cadence")
        if action == "lessons.list":
            return {
                "items": [
                    copy.deepcopy(row)
                    for row in self.state.lessons
                    if row.get("programme_id") == payload.get("programme_id")
                ],
                "next_cursor": None,
            }
        if action == "lessons.propose":
            row = copy.deepcopy(payload)
            row.update(id=self._id("lesson"), state="proposed", version=1)
            self.state.lessons.append(row)
            return copy.deepcopy(row)
        if action == "tranches.list":
            return {
                "items": [
                    copy.deepcopy(row)
                    for row in self.state.tranches
                    if row.get("programme_id") == payload.get("programme_id")
                ],
                "next_cursor": None,
            }
        if action == "tranches.review_list":
            return {
                "items": [
                    copy.deepcopy(row)
                    for row in self.state.reviews
                    if row.get("programme_id") == payload.get("programme_id")
                ],
                "next_cursor": None,
            }
        if action == "tranches.review":
            tranche = next(
                (
                    row
                    for row in self.state.tranches
                    if row.get("id") == payload.get("id")
                ),
                None,
            )
            if tranche is None:
                raise PromoteError("tranches.review: no tranche")
            review = {
                "id": self._id("review"),
                "programme_id": tranche.get("programme_id"),
                "tranche_id": tranche["id"],
                "decision": payload.get("decision"),
                "reviewed_at": "2026-11-22T00:00:00Z",
                "evidence": payload.get("evidence"),
                "version": 1,
            }
            self.state.reviews.append(review)
            successor = next(
                (
                    row
                    for row in self.state.tranches
                    if row.get("programme_id") == tranche.get("programme_id")
                    and int(row.get("number") or -1) == int(tranche.get("number") or -1) + 1
                ),
                None,
            )
            if payload.get("decision") == "continue" and successor is not None:
                successor["state"] = "authorised"
                successor["authorisation_review_id"] = review["id"]
            return copy.deepcopy(tranche)
        if action == "tranches.authorise":
            review = next(
                (
                    row
                    for row in self.state.reviews
                    if row.get("id") == payload.get("review_id")
                ),
                None,
            )
            if review is None:
                raise PromoteError("tranches.authorise: no review")
            tranche = next(
                (
                    row
                    for row in self.state.tranches
                    if row.get("programme_id") == review.get("programme_id")
                    and row.get("id") == review.get("tranche_id")
                ),
                None,
            )
            if tranche is None:
                raise PromoteError("tranches.authorise: no reviewed tranche")
            successor = next(
                (
                    row
                    for row in self.state.tranches
                    if row.get("programme_id") == review.get("programme_id")
                    and int(row.get("number") or -1)
                    == int(tranche.get("number") or -1) + 1
                ),
                None,
            )
            if successor is None:
                raise PromoteError("tranches.authorise: no successor tranche")
            successor["state"] = "authorised"
            successor["authorisation_review_id"] = review["id"]
            return copy.deepcopy(successor)
        if action == "baselines.list":
            return {
                "items": copy.deepcopy(self.state.baselines.get(payload.get("project_id"), [])),
                "next_cursor": None,
            }
        if action == "wbs.list":
            return {
                "items": copy.deepcopy(self.state.wbs.get(payload.get("project_id"), [])),
                "next_cursor": None,
            }
        if action == "wbs.get":
            for row in self.state.wbs.values():
                for node in row:
                    if node.get("id") == payload.get("id"):
                        return copy.deepcopy(node)
            return None
        if action == "wbs.update":
            for rows in self.state.wbs.values():
                for node in rows:
                    if node.get("id") != payload.get("id"):
                        continue
                    if if_match != node.get("version"):
                        raise PromoteError("wbs.update: If-Match is stale")
                    for name in ("status", "evidence", "title", "body"):
                        if name in payload:
                            node[name] = payload[name]
                    node["version"] = int(node.get("version") or 0) + 1
                    return copy.deepcopy(node)
            raise PromoteError(f"wbs.update: no node {payload.get('id')}")
        if action == "projects.charter":
            project_id = payload.get("project_id")
            gate_id = self._id("charter-gate")
            self.state.approvals.append({
                "id": gate_id, "gate": "charter", "status": "pending",
                "project_id": project_id, "target": None,
                "description": "Approve the charter", "version": 1,
                "created_at": "2026-11-22T00:00:00Z",
            })
            return {"project_id": project_id,
                    "charter_asset_id": self._id("charter-asset"),
                    "approval_id": gate_id}
        if action == "projects.baseline":
            project_id = payload.get("project_id")
            gate_id = self._id("baseline-gate")
            baseline_id = self._id("baseline")
            nodes = []
            for node in payload.get("wbs") or []:
                row = {"id": self._id("wbs"), "project_id": project_id,
                       "kind": node.get("kind"), "title": node.get("title"),
                       "status": "open", "evidence": [], "version": 1}
                nodes.append(row)
                self.state.wbs.setdefault(project_id, []).append(row)
            self.state.baselines.setdefault(project_id, []).append({
                "id": baseline_id, "project_id": project_id,
                "kind": payload.get("baseline_kind") or "scope", "approved_by": None})
            self.state.approvals.append({
                "id": gate_id, "gate": "baseline", "status": "pending",
                "project_id": project_id, "target": None,
                "description": "Approve the scope baseline", "version": 1,
                "created_at": "2026-11-22T00:00:01Z",
            })
            return {"baseline_id": baseline_id,
                    "wbs_ids": [node["id"] for node in nodes],
                    "approval_id": gate_id}
        if action == "cards.list":
            rows = [
                copy.deepcopy(row)
                for row in self.state.board_cards.get(payload.get("board_id"), [])
                if not payload.get("list_id") or row.get("list_id") == payload.get("list_id")
            ]
            return {"items": rows, "next_cursor": None}
        raise PromoteError(f"fake client: unknown action {action}")


def _fixture_inputs(root: Path) -> dict[str, Path]:
    programme = "01a1d039-0d24-7d16-ae52-4841e96ccfac"
    project = "01a1dcb8-d24b-76de-b901-56fd56e001bc"
    baseline = "01a1fbb1-b05c-76b6-a5ac-7186fb859dcc"
    staging_benefit = "01a1e9de-4e4f-757e-a8c8-bf3522e5adcf"
    staging_change = "01a204d4-7f2e-7a73-bf79-4b8767d52db8"
    staging_gate = "01a204d4-7f35-791c-8809-dc947d85a7b4"
    files = {
        "risk": {
            "programme_id": programme, "kind": "risk", "code": "R1",
            "title": "fixture risk",
        },
        "benefit": {
            "programme_id": programme, "reference": "B1",
            "description": "fixture benefit", "status": "draft",
        },
        "change": {
            "project_id": project, "baseline_id": baseline,
            "title": "Record the fixture baselines",
            "impact": {"scope": "fixture"},
        },
        "decide": {"approval_id": staging_gate, "decision": "approve"},
        "fields": {
            "id": staging_benefit, "category": "governance",
            "change_id": staging_change,
        },
        "status": {"id": staging_benefit, "status": "active"},
        "measure": {
            "benefit_id": staging_benefit, "kind": "baseline",
            "actual_value": 1.0, "unit": "fixture",
            "measured_at": "2026-11-22T20:00:00Z",
        },
        "closure": {
            "id": project, "closure": "fixture closure",
            "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
            "evidence": {"manifest": "fixture", "retrospective": True},
        },
        "close_gate": {
            "project_id": project, "gate": "projects.close", "kind": "action",
            "action": "projects.close", "input": {"id": project},
            "proposed_input": {"id": project, "reason": "fixture close"},
            "description": "Close the fixture project",
            "evidence": [{"kind": "governed_asset",
                          "id": "01a2051f-d60f-7f6c-bf3e-935e8fc43482"}],
        },
        "close": {"id": project, "reason": "fixture close"},
        "decide_close": {
            "approval_id": "01a20520-6853-73c3-8b04-4139d64d3ef8",
            "decision": "approve",
            "note": "approved",
        },
        "comment": {"id": "01a1ffeb-311e-708e-a994-83eb40fd3a69",
                    "body": "Resolved: the fixture receipt"},
        "move": {"id": "01a1ffeb-311e-708e-a994-83eb40fd3a69",
                 "list_id": "01a1d3e7-bd2f-7998-bf7b-8b4039393db9"},
    }
    paths: dict[str, Path] = {}
    for name, payload in files.items():
        path = root / "inputs" / f"{name}.json"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(payload), encoding="utf-8")
        paths[name] = path
    return paths


def _fixture_entries(
    paths: dict[str, Path], include: tuple[str, ...]
) -> list[tuple[int, dict]]:
    rows: list[tuple[int, dict]] = []
    for name in include:
        if name == "risk":
            raw = {
                "action": "registers.create", "input": str(paths["risk"]),
                "natural_key": "risk:R1",
                "output_id": "01a204cf-fe6e-783a-a837-ed168b16c967",
            }
        elif name == "benefit":
            raw = {
                "action": "benefits.create", "input": str(paths["benefit"]),
                "natural_key": "benefit:B1",
                "output_id": "01a204d2-dbf6-750a-a031-9f00f5fe7754",
            }
        elif name == "change":
            raw = {
                "action": "changes.request", "input": str(paths["change"]),
                "natural_key": "change:fixture-baselines",
                "output_id": "01a204d4-7f2e-7a73-bf79-4b8767d52db8",
                "approval_id": "01a204d4-7f35-791c-8809-dc947d85a7b4",
            }
        elif name == "approval":
            raw = {
                "action": "approvals.decide", "input": str(paths["decide"]),
                "natural_key": "approval:change:fixture-baselines",
                "output_id": "01a204d4-7f35-791c-8809-dc947d85a7b4",
            }
        elif name == "fields":
            raw = {
                "action": "benefits.update", "input": str(paths["fields"]),
                "natural_key": "benefit:B1/fields",
                "output_id": "01a1e9de-4e4f-757e-a8c8-bf3522e5adcf",
            }
        elif name == "status":
            raw = {
                "action": "benefits.update", "input": str(paths["status"]),
                "natural_key": "benefit:B1/status",
                "output_id": "01a1e9de-4e4f-757e-a8c8-bf3522e5adcf",
            }
        elif name == "measure":
            raw = {
                "action": "benefits.measure", "input": str(paths["measure"]),
                "natural_key": "benefit:B1/baseline",
                "output_id": "01a204d3-87c0-736a-a0bf-a93aa2b638c0",
            }
        elif name == "closure":
            raw = {
                "action": "projects.close_request", "input": str(paths["closure"]),
                "natural_key": "project:closure:01a1dcb8-d24b-76de-b901-56fd56e001bc",
                "output_id": "01a1dcb8-d24b-76de-b901-56fd56e001bc",
                "closure_asset_id": "01a2051f-d60f-7f6c-bf3e-935e8fc43482",
                "actor": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
            }
        elif name == "close_gate":
            raw = {
                "action": "approvals.request", "input": str(paths["close_gate"]),
                "natural_key": "approval:projects.close:project/"
                                "01a1dcb8-d24b-76de-b901-56fd56e001bc/close",
                "output_id": "01a20520-6853-73c3-8b04-4139d64d3ef8",
            }
        elif name == "decide_close":
            raw = {
                "action": "approvals.decide", "input": str(paths["decide_close"]),
                "natural_key": "approval:projects.close:"
                                "01a1dcb8-d24b-76de-b901-56fd56e001bc",
                "output_id": "01a20520-6853-73c3-8b04-4139d64d3ef8",
                "actor": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
            }
        elif name == "close":
            raw = {
                "action": "projects.close", "input": str(paths["close"]),
                "natural_key": "project:close:01a1dcb8-d24b-76de-b901-56fd56e001bc",
                "output_id": "01a1dcb8-d24b-76de-b901-56fd56e001bc",
            }
        elif name == "comment":
            body = json.loads(paths["comment"].read_text(encoding="utf-8"))["body"]
            sha = hashlib.sha256(body.encode("utf-8")).hexdigest()[:8]
            raw = {
                "action": "cards.comment", "input": str(paths["comment"]),
                "natural_key": f"card-comment:01a1ffeb-311e-708e-a994-83eb40fd3a69:{sha}",
                "output_id": "01a20544-0000-7000-8000-00000000c001",
            }
        elif name == "move":
            raw = {
                "action": "cards.move", "input": str(paths["move"]),
                "natural_key": "card-move:01a1ffeb-311e-708e-a994-83eb40fd3a69:done",
                "output_id": "01a1ffeb-311e-708e-a994-83eb40fd3a69",
            }
        else:
            raise AssertionError(name)
        rows.append((len(rows) + 2, raw))
    return rows


def self_test() -> int:
    checks = 0

    def ok(condition: bool, message: str) -> None:
        nonlocal checks
        checks += 1
        if not condition:
            raise AssertionError(message)

    def refused(call) -> bool:
        try:
            call()
        except PromoteError:
            return True
        return False

    full = ("risk", "benefit", "change", "approval", "fields", "status")
    with tempfile.TemporaryDirectory(prefix="promote-record-self-test-") as tmp:
        root = Path(tmp)
        paths = _fixture_inputs(root)

        # 1. the parser accepts a well-formed fixture.
        entries, unreplayable, errors = build_entries(_fixture_entries(paths, full))
        ok(not errors, f"fixture journal should parse: {errors}")
        ok(not validate_entries(entries), "fixture journal should validate")
        ok(
            parse_natural_key("approval:change:label").canonical()
            == "approval:change/label",
            "both approval separators canonicalise",
        )
        ok(
            parse_natural_key("benefit:B1/baseline").second == "baseline",
            "benefit suffix parses",
        )

        # 1b. the TOGAF artefact family: the create key is (project, type), the approve names
        #     the create's staging id, and a non-uuid key is refused rather than guessed.
        project = "01a1d039-0d38-76fb-9394-b75934d16fcd"
        artifact_type = "01a1f2c1-13ea-7764-afc8-4f6322e0ad3e"
        artifact_id = "01a20a9a-d84d-70c0-8a32-59ad4e87a2be"
        create_key = f"architecture-artifact:{project}:{artifact_type}"
        approve_key = f"architecture-artifact-approve:{artifact_id}"
        parsed = parse_natural_key(create_key)
        ok(parsed.kind == "architecture_artifact", "the artefact create key parses")
        ok(parsed.canonical() == create_key, "the artefact create key round-trips")
        parsed = parse_natural_key(approve_key)
        ok(
            parsed.kind == "architecture_artifact_approve"
            and parsed.canonical() == approve_key,
            "the artefact approve key round-trips",
        )
        ok(
            refused(lambda: parse_natural_key("architecture-artifact:not-a-uuid:x")),
            "a non-uuid artefact key is refused",
        )
        create_input = root / "artifact-create.json"
        create_input.write_text(
            json.dumps(
                {
                    "project_id": project,
                    "artifact_type_id": artifact_type,
                    "phase_code": "a",
                    "plateau": "psa",
                    "artifact_kind": "matrix",
                    "title": "a self-test artefact",
                }
            ),
            encoding="utf-8",
        )
        approve_input = root / "artifact-approve.json"
        approve_input.write_text(json.dumps({"id": artifact_id}), encoding="utf-8")
        family = [
            (10, {"action": "architecture.artifacts_create",
                  "input": str(create_input), "natural_key": create_key,
                  "output_id": artifact_id}),
            (11, {"action": "architecture.artifacts_approve",
                  "input": str(approve_input), "natural_key": approve_key,
                  "output_id": artifact_id}),
        ]
        entries, unreplayable, errors = build_entries(family)
        ok(not errors and not unreplayable, f"the artefact family parses: {errors}")
        ok(not validate_entries(entries), "the artefact family validates")

        # 2. duplicate refusal.
        duplicate = _fixture_entries(paths, ("risk", "risk"))
        entries, unreplayable, errors = build_entries(duplicate)
        ok(not errors, "duplicate fixture parses")
        ok(
            any("duplicate natural key" in error for error in validate_entries(entries)),
            "a duplicate natural key is refused",
        )

        # 3. a malformed key and an unknown action become unreplayable verdicts, never a
        #    silent drop; a missing input file stays a hard check error.
        malformed = [
            (2, {"action": "registers.create", "input": str(paths["risk"]),
                 "natural_key": "risk:", "output_id": "x"}),
            (3, {"action": "risks.create", "input": str(paths["risk"]),
                 "natural_key": "risk:R1", "output_id": "x"}),
            (4, {"action": "registers.create", "input": "/does/not/exist.json",
                 "natural_key": "risk:R1", "output_id": "x"}),
        ]
        _, unreplayable, errors = build_entries(malformed)
        reasons = " | ".join(entry.unreplayable or "" for entry in unreplayable)
        ok(len(unreplayable) == 2, f"two rows are unreplayable, not dropped: {reasons}")
        ok("malformed natural key" in reasons,
           "a malformed natural key is unreplayable")
        ok("unknown action" in reasons,
           "an unknown action is unreplayable")
        ok(any("does/not/exist" in error for error in errors),
           "a missing input is refused")

        # 4. natural-key matching against fake rows.
        state = State()
        state.registers.append(
            {"id": "prod-risk", "kind": "risk", "code": "R1",
             "programme_id": "p", "version": 1}
        )
        entries, unreplayable, _ = build_entries(_fixture_entries(paths, ("risk",)))
        index = build_change_index(entries, state)
        existing = existing_for(entries[0], state, index)
        ok(existing is not None and existing["id"] == "prod-risk",
           "risk:R1 matches the existing row")

        # 5. a full apply creates everything, remaps ids, and a second run skips all.
        snapshot = root / "snapshot"
        snapshot.mkdir()
        (snapshot / "msp.json").write_text(
            json.dumps({"benefit_measurements": [], "benefit_profiles": []}),
            encoding="utf-8",
        )
        entries, unreplayable, _ = build_entries(_fixture_entries(paths, full))
        state = State()
        # The fixture's change cites a STAGING baseline id; production has its own row.
        state.baselines["01a1dcb8-d24b-76de-b901-56fd56e001bc"] = [
            {"id": "production-scope-baseline",
             "project_id": "01a1dcb8-d24b-76de-b901-56fd56e001bc",
             "kind": "scope", "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545"},
        ]
        client = FakeClient(state)
        id_map, outcomes = apply_entries(client, entries, state, str(snapshot))
        ok(
            [outcome.verdict for outcome in outcomes]
            == ["created"] * len(full),
            f"first apply creates every entry: "
            f"{[(o.entry.natural_key, o.verdict, o.detail) for o in outcomes]}",
        )
        change_call = [
            payload for action, payload, _, _ in client.calls
            if action == "changes.request"
        ][-1]
        ok(
            change_call["baseline_id"] == "production-scope-baseline",
            "the change cites the production baseline resolved by kind",
        )
        gate_decision = [
            payload for action, payload, _, _ in client.calls
            if action == "approvals.decide"
        ][-1]
        ok(
            gate_decision["approval_id"] == id_map[entries[3].output_id],
            "approvals.decide is invoked with the production gate id",
        )
        update_call = [
            payload for action, payload, _, _ in client.calls
            if action == "benefits.update"
        ][0]
        ok(
            update_call["change_id"] == id_map[entries[2].output_id],
            "benefits.update change_id is remapped",
        )
        ok(len(set(id_map.values())) >= 4, "the staging -> production map is populated")
        second_map, second = apply_entries(
            FakeClient(state), entries, state, str(snapshot)
        )
        ok(
            [outcome.verdict for outcome in second] == ["skipped"] * len(full),
            f"second apply skips every entry: "
            f"{[(o.entry.natural_key, o.verdict, o.detail) for o in second]}",
        )
        ok(second_map == id_map, "the map is stable across runs")

        # 5b. A decide keyed by the staging change id must find the approved gate on a
        # replay. Otherwise it re-invokes the decide, and the server refuses a decided gate
        # (measured on the production rehearsal, 2026-11-23 — seven such entries).
        replay_entries, unreplayable, _ = build_entries(
            _fixture_entries(paths, ("change",)) + [(9, {
                "action": "approvals.decide", "input": str(paths["decide"]),
                "natural_key":
                    "approval:change:01a204d4-7f2e-7a73-bf79-4b8767d52db8",
                "output_id": "01a204d4-7f35-791c-8809-dc947d85a7b4",
            })]
        )
        _, replay = apply_entries(
            FakeClient(state), replay_entries, state, str(snapshot)
        )
        ok(
            [outcome.verdict for outcome in replay] == ["skipped", "skipped"],
            f"an id-keyed change decide skips its approved gate: "
            f"{[(o.entry.natural_key, o.verdict, o.detail) for o in replay]}",
        )

        # 6. a created row that cannot be read back is a failure, never a pass.
        entries, unreplayable, _ = build_entries(_fixture_entries(paths, ("risk",)))
        state = State()
        _, outcomes = apply_entries(
            FakeClient(state, unreadable={"registers.get"}),
            entries, state, str(snapshot),
        )
        ok(outcomes[0].verdict == "readback_failed",
           f"an unreadable row is readback_failed: {outcomes[0].verdict}")

        # 7. a measurement is read back through benefits.measurement_list, not the export.
        entries, unreplayable, _ = build_entries(_fixture_entries(paths, ("benefit", "measure")))
        state = State()
        _, outcomes = apply_entries(FakeClient(state), entries, state, None)
        ok(
            outcomes[1].verdict == "created",
            f"a measurement is readable through the action, with no snapshot: "
            f"{outcomes[1].verdict} {outcomes[1].detail}",
        )
        # 7b. without the action and without the export, the failure is loud.
        state = State()
        _, outcomes = apply_entries(
            FakeClient(state, unreadable={"benefits.measurement_list"}),
            entries, state, None,
        )
        ok(
            outcomes[1].verdict == "readback_failed",
            f"a measurement with no read surface fails readback: {outcomes[1].verdict}",
        )
        ok("measurement" in outcomes[1].detail, "the measurement failure says why")

        # 8. the closure family: a close chain applies once and replays as skips.
        project = "01a1dcb8-d24b-76de-b901-56fd56e001bc"
        card = "01a1ffeb-311e-708e-a994-83eb40fd3a69"
        board = "01a1d3e7-bbfa-7e5f-a4d2-4b44ca69886c"
        intake = "01a1d3e7-bc47-7384-94cf-f1009fd9c0b7"
        staging_done = "01a1d3e7-bd2f-7998-bf7b-8b4039393db9"
        production_done = "01a1d3e7-0000-4000-8000-00000000d0e5"
        closure_names = ("closure", "close_gate", "decide_close", "close",
                         "comment", "move")
        entries, unreplayable, errors = build_entries(_fixture_entries(paths, closure_names))
        ok(not errors, f"the closure fixture parses: {errors}")
        ok(not validate_entries(entries), "the closure fixture validates")
        state = State()
        state.projects[project] = {"id": project, "status": "active", "meta": {}}
        state.cards[card] = {
            "id": card, "board_id": board, "list_id": intake,
            "version": 1, "comments": [],
        }
        state.lists[intake] = {"id": intake, "board_id": board, "kind": "intake"}
        # The production board carries ONE done lane, under an id the staging journal does not
        # know: resolution must be by kind.
        state.lists[production_done] = {
            "id": production_done, "board_id": board, "kind": "done",
        }
        client = FakeClient(state)
        id_map, outcomes = apply_entries(client, entries, state, str(snapshot))
        ok(
            [outcome.verdict for outcome in outcomes]
            == ["created"] * len(closure_names),
            f"the closure chain applies: "
            f"{[(o.entry.natural_key, o.verdict, o.detail) for o in outcomes]}",
        )
        ok(state.projects[project]["status"] == "closed", "the close landed")
        ok(
            state.cards[card]["list_id"] == production_done,
            "the move resolved the production done lane by kind, not by id",
        )
        request_call = [
            payload for action, payload, _, _ in client.calls
            if action == "approvals.request"
        ][0]
        ok(
            request_call["evidence"][0]["id"]
            == id_map["01a2051f-d60f-7f6c-bf3e-935e8fc43482"],
            "the destructive gate's evidence is the production closure asset",
        )
        ok(
            "01a1e607-17d9-77d3-98d3-a43a87c4633d" in client.actors
            and "01a1e8a7-8b23-76a9-9114-aafb95f7a545" in client.actors,
            f"the replay passes each entry's recorded actor: {client.actors}",
        )
        local_client = LocalActionClient("/bin/true")
        local_args = local_client.build(
            "approvals.decide", approval_id="gate", if_match=2, actor="member"
        )
        ok(
            "--actor-id" in local_args
            and local_args[local_args.index("--actor-id") + 1] == "member",
            f"the local transport carries the actor: {local_args}",
        )
        _, second = apply_entries(FakeClient(state), entries, state, str(snapshot))
        ok(
            [outcome.verdict for outcome in second]
            == ["skipped"] * len(closure_names),
            f"a replay skips the whole closure chain: "
            f"{[(o.entry.natural_key, o.verdict, o.detail) for o in second]}",
        )

        # 8b. A pending closure gate is not a decision. The close_request readback must
        # record the gate's real state; a fabricated "approved" row makes the decide that
        # follows skip, leaves the closure asset draft, and the destructive gate then
        # refuses its evidence (replay finding, 2026-11-23).
        closure_raw = _fixture_entries(paths, ("closure",))[0][1]
        closure_raw["approval_id"] = "01a2051f-d614-7c07-a57e-ac31652e51d1"
        closure_entry = build_entries([(2, closure_raw)])[0][0]
        pending_gate = {
            "id": "01a2072f-0000-4000-8000-000000000063",
            "gate": "closure",
            "status": "pending",
            "project_id": project,
            "target": None,
            "version": 1,
        }

        class _CloseRequestServer:
            def __init__(self) -> None:
                self.calls: list[str] = []

            def invoke(self, action: str, payload: dict, **_: object) -> dict:
                self.calls.append(action)
                if action == "projects.get":
                    return {
                        "project": {"id": project, "status": "active", "meta": {}},
                        "boards": [],
                    }
                if action == "approvals.list":
                    return {"items": [copy.deepcopy(pending_gate)]}
                raise AssertionError(f"unexpected read {action}")

        server = _CloseRequestServer()
        closure_state = State()
        row = readback_entry(
            closure_entry, server, closure_state, project, pending_gate["id"], None, {}
        )
        ok(row.get("id") == project, "the close request reads its project back")
        recorded = [
            gate for gate in closure_state.approvals
            if gate.get("gate") == "closure" and gate.get("project_id") == project
        ]
        ok(
            len(recorded) == 1 and recorded[0].get("status") == "pending",
            f"the readback records the gate as pending, not approved: {recorded}",
        )
        decide_entry = build_entries([(3, {
            "action": "approvals.decide", "input": str(paths["decide"]),
            "natural_key": f"approval:closure:{project}",
            "output_id": "01a2051f-d614-7c07-a57e-ac31652e51d1",
        })])[0][0]
        ok(
            existing_for(decide_entry, closure_state, {}, {}) is None,
            "a pending closure gate is not an existing decision: the decide must run",
        )

        # 9. the tr3 record family: a risk edited in place and a benefits-map edge replay.
        risk_payload = {
            "id": "staging-risk",
            "owner_job_id": "01a1db6b-4f17-7e76-838d-590d71578ad1",
            "review_at": "2026-11-29",
        }
        link_payload = {
            "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
            "benefit_id": "staging-b7",
            "benefit_ref": "B7",
            "output_ref": "01a20277-8d10-702f-a975-4dd6469e7b1d",
            "capability_ref": "the inference ledger",
            "operational_outcome": "fewer tokens per deliverable",
            "strategic_objective": "an economical programme",
        }
        extra_inputs = {}
        for name, payload in (("risk_update", risk_payload), ("link", link_payload)):
            path = root / "inputs" / f"{name}.json"
            path.write_text(json.dumps(payload), encoding="utf-8")
            extra_inputs[name] = path
        raw_entries = [
            (2, {"action": "registers.update", "input": str(extra_inputs["risk_update"]),
                 "natural_key": "risk:R1/update", "output_id": "staging-risk"}),
            (3, {"action": "benefits.link", "input": str(extra_inputs["link"]),
                 "natural_key": "benefit-link:B7:01a20277-8d10-702f-a975-4dd6469e7b1d",
                 "output_id": "staging-link"}),
        ]
        entries, unreplayable, errors = build_entries(raw_entries)
        ok(not errors, f"the tr3 record fixture parses: {errors}")
        ok(not validate_entries(entries), "the tr3 record fixture validates")
        state = State()
        state.registers.append({
            "id": "prod-R1", "kind": "risk", "code": "R1", "version": 1,
            "owner_job_id": None, "review_at": None,
            "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
        })
        state.benefits.append({
            "id": "prod-B7", "reference": "B7",
            "programme_id": "01a1d039-0d24-7d16-ae52-4841e96ccfac",
        })
        client = FakeClient(state)
        id_map, outcomes = apply_entries(client, entries, state, None)
        ok(
            [outcome.verdict for outcome in outcomes] == ["created", "created"],
            f"the tr3 record family applies: "
            f"{[(o.entry.natural_key, o.verdict, o.detail) for o in outcomes]}",
        )
        ok(
            state.registers[0]["owner_job_id"] == risk_payload["owner_job_id"],
            "the risk update landed on the production row by code",
        )
        ok(
            state.links and state.links[0]["benefit_id"] == "prod-B7",
            "the link resolved the benefit by reference, not by staging id",
        )
        _, second = apply_entries(FakeClient(state), entries, state, None)
        ok(
            [outcome.verdict for outcome in second] == ["skipped", "skipped"],
            f"a replay skips the tr3 record entries: "
            f"{[(o.entry.natural_key, o.verdict, o.detail) for o in second]}",
        )

        # 10. the document family: a compose+publish pair applies once, then skips; a
        # pre-existing published document with the same title/category suppresses both.
        doc_payload = {
            "title": "Fixture Plan", "project_id": "01a1dcb8-d24b-76de-b901-56fd56e001bc",
            "body": "# Fixture Plan", "category": "pi-plan", "publishable": True,
        }
        doc_path = root / "inputs" / "document.json"
        doc_path.write_text(json.dumps(doc_payload), encoding="utf-8")
        publish_path = root / "inputs" / "document_publish.json"
        publish_path.write_text(
            json.dumps({"id": "staging-document", "publish": True}), encoding="utf-8"
        )
        doc_entries = [
            (2, {"action": "documents.compose", "input": str(doc_path),
                 "natural_key": "document:tranche-3:pi-plan:Fixture Plan",
                 "output_id": "staging-document"}),
            (3, {"action": "documents.publish", "input": str(publish_path),
                 "natural_key": "document-publish:tranche-3:pi-plan:Fixture Plan",
                 "output_id": "staging-document"}),
        ]
        entries, unreplayable, errors = build_entries(doc_entries)
        ok(not errors, f"the document fixture parses: {errors}")
        state = State()
        client = FakeClient(state)
        _, outcomes = apply_entries(client, entries, state, None)
        ok(
            [o.verdict for o in outcomes] == ["created", "created"],
            f"the document pair applies: "
            f"{[(o.entry.natural_key, o.verdict, o.detail) for o in outcomes]}",
        )
        ok(
            any(row.get("published_at") for row in state.documents),
            "the publish landed",
        )
        _, second = apply_entries(FakeClient(state), entries, state, None)
        ok(
            [o.verdict for o in second] == ["skipped", "skipped"],
            f"a replay skips compose and publish: "
            f"{[(o.entry.natural_key, o.verdict, o.detail) for o in second]}",
        )
        pre = State()
        pre.documents.append({
            "id": "prod-document", "title": "Fixture Plan", "category": "pi-plan",
            "project_id": doc_payload["project_id"], "version": 2, "status": "active",
            "published_at": "2026-11-01T00:00:00Z",
        })
        _, third = apply_entries(FakeClient(pre), entries, pre, None)
        ok(
            [o.verdict for o in third] == ["skipped", "skipped"],
            f"a pre-existing published document suppresses both: "
            f"{[(o.entry.natural_key, o.verdict, o.detail) for o in third]}",
        )

        # 11. the decision register family: a row is created once and transitioned once.
        decision_payload = {
            "key": "adr-9999", "title": "Fixture decision", "body": "# Fixture decision",
            "meta": {"number": "9999", "deciders": ["the programme owner"]},
        }
        decide_create_path = root / "inputs" / "decision_create.json"
        decide_create_path.write_text(json.dumps(decision_payload), encoding="utf-8")
        state = State()
        client = FakeClient(state)
        entries, unreplayable, errors = build_entries([
            (2, {"action": "decisions.create", "input": str(decide_create_path),
                 "natural_key": "decision:adr-9999", "output_id": "staging-decision"}),
        ])
        ok(not errors and not validate_entries(entries), "the decision create fixture validates")
        _, outcomes = apply_entries(client, entries, state, None)
        ok([o.verdict for o in outcomes] == ["created"], f"decision create: {outcomes}")
        production_id = state.decisions[0]["id"]
        transition_payload = {"id": production_id, "to": "approved"}
        transition_path = root / "inputs" / "decision_transition.json"
        transition_path.write_text(json.dumps(transition_payload), encoding="utf-8")
        entries, unreplayable, errors = build_entries([
            (2, {"action": "decisions.transition", "input": str(transition_path),
                 "natural_key": "decision:adr-9999/approved",
                 "output_id": production_id}),
        ])
        ok(not errors and not validate_entries(entries), "the decision transition fixture validates")
        _, outcomes = apply_entries(client, entries, state, None)
        ok([o.verdict for o in outcomes] == ["created"], f"decision transition: {outcomes}")
        ok(state.decisions[0]["status"] == "approved", "the transition landed")
        _, second = apply_entries(FakeClient(state), entries, state, None)
        ok([o.verdict for o in second] == ["skipped"], f"a transition replay skips: {second}")
        create_entries, unreplayable, errors = build_entries([
            (2, {"action": "decisions.create", "input": str(decide_create_path),
                 "natural_key": "decision:adr-9999", "output_id": "staging-decision"}),
        ])
        _, third = apply_entries(FakeClient(state), create_entries, state, None)
        ok([o.verdict for o in third] == ["skipped"], f"a create replay skips: {third}")

        # 12. the tr3 SAFe family: cadences, the plan gate, the demo credit, the retro and its
        #     lessons, and the tranche review chain.  Content-keyed; created once, then skipped.
        programme = "01a1d039-0d24-7d16-ae52-4841e96ccfac"
        project = "01a1dcb8-d24b-76de-b901-56fd56e001bc"
        tranche_nine = "aaaaaaaa-0000-4000-8000-000000000009"
        board = "bbbbbbbb-0000-4000-8000-00000000000b"
        delegation = "01a1ff1f-e4ba-72dc-9c1a-13049933cf14"
        cadence_dir = root / "cadence"
        cadence_dir.mkdir()

        def cadence_input(name: str, payload: dict) -> str:
            path = cadence_dir / f"{name}.json"
            path.write_text(json.dumps(payload), encoding="utf-8")
            return str(path)

        link = {"tranche_number": 9, "tranche_id": "staging-t9", "delegation_ref": delegation}
        paths_new = {
            "create_pi": cadence_input(
                "create_pi",
                {"programme_id": programme, "kind": "pi", "period": "tranche-9",
                 "meta": dict(link)},
            ),
            "create_demo": cadence_input(
                "create_demo",
                {"programme_id": programme, "kind": "demo", "period": "tranche-9",
                 "meta": dict(link)},
            ),
            "create_retro": cadence_input(
                "create_retro",
                {"programme_id": programme, "kind": "retro", "period": "tranche-9",
                 "meta": {**link, "board_id": board}},
            ),
            "create_refinement": cadence_input(
                "create_refinement",
                {"programme_id": programme, "kind": "refinement", "period": "tranche-9",
                 "meta": dict(link)},
            ),
            "update_pi": cadence_input(
                "update_pi",
                {"id": "staging-pi",
                 "meta": {**link, "plan": "fixture",
                          "refinement_cadence_id": "staging-refinement"}},
            ),
            "plan": cadence_input(
                "plan",
                {"cadence_id": "staging-pi", "decision": "approve",
                 "note": "rule approval", "delegation_ref": delegation},
            ),
            "credit": cadence_input(
                "credit",
                {"cadence_id": "staging-demo", "deliverable_id": "staging-deliverable"},
            ),
            "findings": cadence_input(
                "findings",
                {"cadence_id": "staging-retro",
                 "findings": [{"key": "k1", "statement": "fixture", "evidence": ["x"]}]},
            ),
            "lesson": cadence_input(
                "lesson",
                {"programme_id": programme, "cadence_id": "staging-retro",
                 "finding_key": "k1", "statement": "fixture", "evidence": ["x"]},
            ),
            "close": cadence_input("close", {"cadence_id": "staging-retro"}),
            "gate": cadence_input(
                "gate",
                {"gate": "tranches.review", "kind": "action", "action": "tranches.review",
                 "input": {"id": "staging-t9", "decision": "continue",
                           "evidence": {"snapshot_sha": "a" * 40},
                           "delegation_ref": delegation},
                 "description": "review tranche 9"},
            ),
            "decide": cadence_input(
                "decide",
                {"approval_id": "staging-gate", "decision": "approve", "note": "approved"},
            ),
            "review": cadence_input(
                "review",
                {"id": "staging-t9", "decision": "continue",
                 "evidence": {"snapshot_sha": "a" * 40}, "delegation_ref": delegation},
            ),
            "authorise": cadence_input("authorise", {"review_id": "staging-review"}),
        }
        scope = {"programme_id": programme}
        raw_new = [
            {"action": "cadences.create", "input": paths_new["create_pi"],
             "natural_key": "cadence:tranche-9/pi", "output_id": "staging-pi",
             "meta": scope},
            {"action": "cadences.create", "input": paths_new["create_demo"],
             "natural_key": "cadence:tranche-9/demo", "output_id": "staging-demo-cadence",
             "meta": scope},
            {"action": "cadences.create", "input": paths_new["create_retro"],
             "natural_key": "cadence:tranche-9/retro", "output_id": "staging-retro-cadence",
             "meta": scope},
            {"action": "cadences.create", "input": paths_new["create_refinement"],
             "natural_key": "cadence:tranche-9/refinement",
             "output_id": "staging-refinement", "meta": scope},
            {"action": "cadences.update", "input": paths_new["update_pi"],
             "natural_key": "cadence:tranche-9/pi/meta", "output_id": "staging-pi-updated",
             "meta": scope},
            {"action": "cadences.approve_plan", "input": paths_new["plan"],
             "natural_key": "pi-plan:tranche-9", "output_id": "staging-plan-gate",
             "meta": scope},
            {"action": "cadences.credit_demo", "input": paths_new["credit"],
             "natural_key": "demo-credit:tranche-9:The accepted deliverable",
             "output_id": "staging-credit",
             "meta": {"programme_id": programme, "project_id": project}},
            {"action": "retros.record_findings", "input": paths_new["findings"],
             "natural_key": "retro:tranche-9", "output_id": "staging-retro",
             "meta": scope},
            {"action": "lessons.propose", "input": paths_new["lesson"],
             "natural_key": "lesson:tranche-9:k1", "output_id": "staging-lesson-k1",
             "meta": scope},
            {"action": "retros.close", "input": paths_new["close"],
             "natural_key": "retro:tranche-9/close", "output_id": "staging-retro-closed",
             "meta": scope},
            {"action": "approvals.request", "input": paths_new["gate"],
             "natural_key": "tranche-review-gate:9", "output_id": "staging-gate",
             "meta": scope},
            {"action": "approvals.decide", "input": paths_new["decide"],
             "natural_key": "tranche-review-decide:9", "output_id": "staging-gate-decided",
             "meta": scope},
            {"action": "tranches.review", "input": paths_new["review"],
             "natural_key": "tranche-review:9", "output_id": "staging-review",
             "approval_id": "staging-gate", "meta": scope},
            {"action": "tranches.authorise", "input": paths_new["authorise"],
             "natural_key": "tranche-authorise:10", "output_id": "staging-tranche-10",
             "meta": scope},
        ]
        entries, unreplayable, errors = build_entries(
            [(index + 2, raw) for index, raw in enumerate(raw_new)]
        )
        ok(not errors, f"the cadence fixture parses: {errors}")
        ok(not validate_entries(entries), f"the cadence fixture validates: "
                                          f"{validate_entries(entries)}")
        ok(
            parse_natural_key("cadence:tranche-9/pi/meta").canonical()
            == "cadence:tranche-9/pi/meta",
            "a cadence meta key canonicalises to its update form",
        )
        state = State()
        state.tranches.extend([
            {"id": tranche_nine, "number": 9, "programme_id": programme,
             "state": "authorised"},
            {"id": "aaaaaaaa-0000-4000-8000-000000000010", "number": 10,
             "programme_id": programme, "state": "planned"},
        ])
        state.wbs[project] = [
            {"id": "c1", "project_id": project, "kind": "deliverable",
             "title": "The accepted deliverable"}
        ]
        client = FakeClient(state)
        id_map, outcomes = apply_entries(client, entries, state, None)
        ok(
            [outcome.verdict for outcome in outcomes]
            == ["created"] * 13 + ["skipped"],
            f"the cadence run creates the record, then skips the review it already "
            f"authorised: {[(o.entry.natural_key, o.verdict, o.detail) for o in outcomes]}",
        )
        review_call = [
            (payload, approval) for action, payload, _, approval in client.calls
            if action == "tranches.review"
        ][-1]
        ok(
            review_call[0]["id"] == tranche_nine,
            "the review's staging tranche id is resolved to the production tranche",
        )
        ok(
            review_call[1] == id_map["staging-gate"],
            "the review is called with the gate the request opened",
        )
        credit_call = [
            payload for action, payload, _, _ in client.calls
            if action == "cadences.credit_demo"
        ][-1]
        ok(
            credit_call["deliverable_id"] == "c1",
            "the demo credit resolves its deliverable by title",
        )
        pi_cadence = cadence_row(state, "tranche-9", "pi")
        pi_meta = (pi_cadence or {}).get("meta") or {}
        ok(
            pi_cadence is not None and pi_meta.get("tranche_id") == tranche_nine,
            "the cadence meta names the production tranche, not the staging id",
        )
        ok(
            pi_meta.get("refinement_cadence_id")
            == id_map["staging-refinement"],
            "a cadence id embedded in another cadence's meta is remapped too",
        )
        ok(
            (cadence_row(state, "tranche-9", "retro") or {}).get("state") == "closed",
            "the retro closed",
        )
        _, second = apply_entries(FakeClient(state), entries, state, None)
        ok(
            [outcome.verdict for outcome in second] == ["skipped"] * len(entries),
            f"the cadence replay skips everything: "
            f"{[(o.entry.natural_key, o.verdict, o.detail) for o in second]}",
        )

        # 13. the family's refusals: a lesson-free close, a malformed key, a broken chain.
        lessonless = [
            {"action": "retros.record_findings", "input": paths_new["findings"],
             "natural_key": "retro:tranche-9", "output_id": "x", "meta": scope},
            {"action": "retros.close", "input": paths_new["close"],
             "natural_key": "retro:tranche-9/close", "output_id": "x", "meta": scope},
        ]
        entries, unreplayable, errors = build_entries([(2, raw) for raw in lessonless])
        ok(not errors, "the lesson-free close parses")
        ok(
            any("no lesson" in error for error in validate_entries(entries)),
            "a retro close with no lesson for its finding is refused",
        )
        _, unreplayable, errors = build_entries([
            (2, {"action": "cadences.create", "input": paths_new["create_pi"],
                 "natural_key": "cadence:", "output_id": "x"}),
        ])
        ok(
            any("malformed natural key" in (entry.unreplayable or "")
                for entry in unreplayable),
            "a malformed cadence key is unreplayable",
        )
        broken = [dict(raw) for raw in raw_new]
        for index, raw in enumerate(broken):
            if raw["action"] == "tranches.review":
                broken[index] = {**raw, "approval_id": "some-other-gate"}
        entries, unreplayable, errors = build_entries(
            [(index + 2, raw) for index, raw in enumerate(broken)]
        )
        ok(not errors, "the broken-chain fixture parses")
        ok(
            any("is not the request entry's output" in error
                for error in validate_entries(entries)),
            "a review whose gate is not the request's output is refused",
        )

    # Transport: the HTTP envelope and the admin CLI's argv/stdin are what the boundary reads.
    http = ActionClient("http://127.0.0.1:1", token="t")
    ok(http.build_body({"id": "x"}) == {"input": {"id": "x"}},
       "the HTTP body wraps the action input")
    ok(
        http.build_body({"id": "x"}, "a-approval")
        == {"input": {"id": "x"}, "approval_id": "a-approval"},
        "an approval is a property of the call, not of the input",
    )
    ok(
        refused(lambda: ActionClient("http://127.0.0.1:1")),
        "HTTP apply refuses a client with no bearer token",
    )
    ok(refused(lambda: LocalActionClient("")),
       "the local transport requires a binary")
    local = LocalActionClient("/tmp/conveyor-test", tenant="t")
    ok(
        local.build("tranches.review", approval_id="a", if_match=3)
        == [
            "/tmp/conveyor-test", "admin", "action", "invoke", "--tenant", "t",
            "--name", "tranches.review", "--input", "/dev/stdin",
            "--approval-id", "a", "--if-match", "3",
        ],
        "the local transport names the action, the gate and the precondition",
    )
    ok(local.build_stdin({"id": "x"}) == '{"id":"x"}\n',
       "an ordinary input goes on stdin bare")
    ok(
        local.build_stdin({"gate": "g", "input": {"id": "x"}})
        == '{"input":{"gate":"g","input":{"id":"x"}}}\n',
        "an input carrying its own input key is wrapped for the CLI",
    )

    # 12. the T3 construction chain: the rework charters a project that already exists, plans
    # it, freezes the plan behind its own gate and accepts the deliverables the demo credits
    # then name. Each act opens the gate its decide resolves; a second pass skips all five.
    with tempfile.TemporaryDirectory(prefix="promote-record-chain-") as tmp:
        chain_dir = Path(tmp)
        chain_project = "01a1d039-0d33-7915-8833-75f4dc31cc07"
        charter_gate = "01a2052d-d77d-7415-bc64-8289f1cb6139"
        baseline_gate = "01a2052e-0621-78fd-b0ef-8af02b8bc715"
        (chain_dir / "charter.json").write_text(
            json.dumps({"project_id": chain_project, "charter": "# The T3 charter"}),
            encoding="utf-8",
        )
        (chain_dir / "decide-charter.json").write_text(
            json.dumps({"approval_id": charter_gate, "decision": "approve"}),
            encoding="utf-8",
        )
        (chain_dir / "baseline.json").write_text(
            json.dumps({
                "project_id": chain_project,
                "baseline_kind": "scope",
                "wbs": [{"kind": "deliverable", "title": "The accepted deliverable",
                         "business_value": 8, "time_criticality": 8,
                         "risk_reduction": 5, "job_size": 3}],
            }),
            encoding="utf-8",
        )
        (chain_dir / "decide-baseline.json").write_text(
            json.dumps({"approval_id": baseline_gate, "decision": "approve"}),
            encoding="utf-8",
        )
        (chain_dir / "accept.json").write_text(
            json.dumps({"status": "accepted",
                        "evidence": [{"ref": "the gate", "kind": "test",
                                      "path": "crates/actions/tests/spine_gates.rs"}]}),
            encoding="utf-8",
        )
        chain_raw = [
            {"action": "projects.charter", "input": str(chain_dir / "charter.json"),
             "natural_key": f"project:charter:{chain_project}",
             "output_id": charter_gate, "approval_id": charter_gate},
            {"action": "approvals.decide", "input": str(chain_dir / "decide-charter.json"),
             "natural_key": f"approval:charter/{chain_project}", "output_id": charter_gate},
            {"action": "projects.baseline", "input": str(chain_dir / "baseline.json"),
             "natural_key": f"project:baseline:{chain_project}",
             "output_id": baseline_gate, "approval_id": baseline_gate},
            {"action": "approvals.decide", "input": str(chain_dir / "decide-baseline.json"),
             "natural_key": f"approval:baseline/{chain_project}", "output_id": baseline_gate},
            {"action": "wbs.update", "input": str(chain_dir / "accept.json"),
             "natural_key": f"wbs-accept:{chain_project}:The accepted deliverable",
             "output_id": "staging-wbs"},
        ]
        chain_entries, chain_unreplayable, chain_errors = build_entries(
            [(index + 2, raw) for index, raw in enumerate(chain_raw)]
        )
        ok(not chain_errors, f"the construction chain parses: {chain_errors}")
        ok(not validate_entries(chain_entries), "the construction chain validates")
        chain_state = State()
        _, chain_first = apply_entries(FakeClient(chain_state), chain_entries, chain_state, None)
        ok(
            [outcome.verdict for outcome in chain_first] == ["created"] * 5,
            f"the construction chain applies: "
            f"{[(o.entry.natural_key, o.verdict, o.detail) for o in chain_first]}",
        )
        ok(
            any(row.get("approved_by") for row in chain_state.baselines[chain_project]),
            "the baseline gate's decide approves the baseline it names",
        )
        ok(
            chain_state.wbs[chain_project][0].get("status") == "accepted"
            and chain_state.wbs[chain_project][0].get("evidence"),
            "the acceptance lands with its typed evidence",
        )
        _, chain_second = apply_entries(FakeClient(chain_state), chain_entries, chain_state, None)
        ok(
            [outcome.verdict for outcome in chain_second] == ["skipped"] * 5,
            f"a replay skips the whole construction chain: "
            f"{[(o.entry.natural_key, o.verdict, o.detail) for o in chain_second]}",
        )

    # 14. the governed-asset family: compose → submit → approve replays, every read-back is
    #     `governed_assets.get`, and an unknown action is a verdict and a non-zero run, never
    #     a silent drop.
    with tempfile.TemporaryDirectory(prefix="promote-record-governed-") as tmp:
        governed_dir = Path(tmp)
        project = "01a1dcb8-d24b-76de-b901-56fd56e001bc"
        compose_path = governed_dir / "compose.json"
        compose_path.write_text(json.dumps({
            "kind": "mandate", "project_id": project,
            "title": "Fixture mandate", "body": "# Mandate",
            "meta": {"number": "M1"},
        }), encoding="utf-8")
        submit_path = governed_dir / "submit.json"
        submit_path.write_text(json.dumps({"id": "staging-mandate"}), encoding="utf-8")
        approve_path = governed_dir / "approve.json"
        approve_path.write_text(json.dumps({"id": "staging-mandate"}), encoding="utf-8")
        governed_raw = [
            (2, {"action": "governed_assets.compose", "input": str(compose_path),
                 "natural_key": f"governed-asset:mandate:{project}:Fixture mandate",
                 "output_id": "staging-mandate"}),
            (3, {"action": "governed_assets.submit", "input": str(submit_path),
                 "natural_key": "governed-asset:staging-mandate/submit",
                 "output_id": "staging-mandate"}),
            (4, {"action": "governed_assets.approve", "input": str(approve_path),
                 "natural_key": "governed-asset:staging-mandate/approve",
                 "output_id": "staging-mandate"}),
        ]
        governed_entries, governed_unreplayable, governed_errors = build_entries(
            governed_raw
        )
        ok(
            not governed_errors and not governed_unreplayable,
            f"the governed-asset trio parses: {governed_errors} "
            f"{[entry.unreplayable for entry in governed_unreplayable]}",
        )
        ok(not validate_entries(governed_entries), "the governed-asset trio validates")
        governed_state = State()
        governed_client = FakeClient(governed_state)
        governed_map, governed_outcomes = apply_entries(
            governed_client, governed_entries, governed_state, None
        )
        ok(
            [outcome.verdict for outcome in governed_outcomes] == ["created"] * 3,
            f"the governed-asset trio applies: "
            f"{[(o.entry.natural_key, o.verdict, o.detail) for o in governed_outcomes]}",
        )
        asset_id = governed_map["staging-mandate"]
        get_calls = [
            payload for action, payload, _, _ in governed_client.calls
            if action == "governed_assets.get"
        ]
        ok(
            get_calls == [{"id": asset_id}] * 3,
            f"every governed-asset read-back is governed_assets.get: {get_calls}",
        )
        ok(
            governed_state.governed_assets
            and governed_state.governed_assets[0]["status"] == "approved",
            "the composed asset reaches `approved` in production",
        )

        bad_path = governed_dir / "bad-journal.jsonl"
        bad_path.write_text(json.dumps({
            "action": "risks.create", "input": str(compose_path),
            "natural_key": "risk:R1", "output_id": "x",
        }) + "\n", encoding="utf-8")
        _, bad_unreplayable, bad_errors = build_entries(
            [(2, json.loads(bad_path.read_text(encoding="utf-8")))]
        )
        ok(
            not bad_errors and len(bad_unreplayable) == 1,
            f"an unknown action becomes an unreplayable row: {bad_errors}",
        )
        bad_outcome = Outcome(
            bad_unreplayable[0], bad_unreplayable[0].output_id, None,
            "unreplayable_input", bad_unreplayable[0].unreplayable or "",
        )
        ok(bad_outcome.verdict == "unreplayable_input",
           "the unknown action's verdict is unreplayable_input")
        document = build_promotion(
            bad_unreplayable, {}, [bad_outcome], bad_path, None, None
        )
        ok(
            [entry["verdict"] for entry in document["entries"]]
            == ["unreplayable_input"],
            "the verdict JSON lists the unreplayable row",
        )
        ok(not document["complete"], "an unreplayable row makes the promotion incomplete")
        ok(main(["--check", "--journal", str(bad_path)]) == 1,
           "a check run with an unreplayable row exits non-zero")

    print(f"self-test: {checks} checks passed (offline; no network, no database)")
    return 0


# ── CLI ────────────────────────────────────────────────────────────────────────────────


def dry_run_plan(entries: list[Entry]) -> list[dict]:
    return [
        {
            "action": entry.action,
            "natural_key": entry.natural_key,
            "will_create": True,
        }
        for entry in entries
    ]


def summary(entries: list[Entry]) -> str:
    counts: dict[str, int] = {}
    for entry in entries:
        counts[entry.action] = counts.get(entry.action, 0) + 1
    return ", ".join(f"{count} {action}" for action, count in sorted(counts.items()))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Promote the staging programme record through the product actions."
    )
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--check", action="store_true", help="offline validation (default)")
    mode.add_argument(
        "--dry-run", action="store_true", dest="dry_run", help="offline plan"
    )
    mode.add_argument("--apply", action="store_true", help="apply to production")
    mode.add_argument("--self-test", action="store_true", dest="self_test",
                      help="offline self-test")
    parser.add_argument("--journal", default=DEFAULT_JOURNAL)
    parser.add_argument("--output", default=DEFAULT_PROMOTION)
    parser.add_argument("--url", default=os.environ.get("CONVEYOR_URL"))
    parser.add_argument("--token", default=os.environ.get("CONVEYOR_TOKEN"))
    parser.add_argument(
        "--local-bin",
        default=os.environ.get("PROGRAMME_RECORD_LOCAL_BIN"),
        help="run the actions in this branch binary (its DATABASE_MIGRATE_URL), not over HTTP",
    )
    parser.add_argument("--tenant", default=os.environ.get("TENANT") or "nonagentic")
    parser.add_argument(
        "--snapshot",
        default=os.environ.get("CONVEYOR_SNAPSHOT"),
        help="fallback export for measurements when the target image has no read action",
    )
    parser.add_argument("--programme-id", default=None)
    parser.add_argument("--timeout", type=float, default=60.0)
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()

    journal_path = resolve_path(args.journal)
    entries, unreplayable, errors = check_journal(journal_path)
    if errors:
        for error in errors:
            print(f"error: {error}", file=sys.stderr)
        print(f"check: {len(errors)} error(s); refusing to continue", file=sys.stderr)
        return 1
    # A row that cannot be replayed is a verdict of its own: it is listed in the promotion
    # document and it fails the run, so a malformed entry is never dropped in silence.
    unreplayable_outcomes = [
        Outcome(entry, entry.output_id, None, "unreplayable_input",
                entry.unreplayable or "")
        for entry in unreplayable
    ]

    def report_unreplayable() -> None:
        for outcome in unreplayable_outcomes:
            print(
                f"line {outcome.entry.lineno}: unreplayable_input: {outcome.detail}",
                file=sys.stderr,
            )
        print(
            f"{len(unreplayable_outcomes)} unreplayable journal row(s); refusing",
            file=sys.stderr,
        )

    if args.dry_run:
        plan = dry_run_plan(entries) + [
            {
                "action": outcome.entry.action,
                "natural_key": outcome.entry.natural_key,
                "will_create": False,
                "verdict": outcome.verdict,
                "detail": outcome.detail,
            }
            for outcome in unreplayable_outcomes
        ]
        print(json.dumps(plan, indent=2))
        if unreplayable_outcomes:
            report_unreplayable()
            return 1
        print(
            f"dry-run: {len(entries)} entries ({summary(entries)}); no network",
            file=sys.stderr,
        )
        return 0

    if not args.apply:
        if unreplayable_outcomes:
            report_unreplayable()
            return 1
        print(f"check: {len(entries)} entries ({summary(entries)}); journal is well formed")
        return 0

    if not args.local_bin:
        if not args.url:
            print(
                "error: --apply requires --local-bin PATH, or --url with --token",
                file=sys.stderr,
            )
            return 1
        if not args.token:
            print("error: --apply over HTTP requires --token", file=sys.stderr)
            return 1
    snapshot = args.snapshot

    if args.local_bin:
        client: Any = LocalActionClient(
            args.local_bin, tenant=args.tenant, timeout=args.timeout
        )
    else:
        client = ActionClient(args.url, token=args.token, timeout=args.timeout)
    snapshot_document = None
    if snapshot:
        try:
            snapshot_document = load_snapshot(snapshot)
        except PromoteError as error:
            print(f"error: {error}", file=sys.stderr)
            return 1
    try:
        state = load_state(
            client, entries, snapshot_document, args.programme_id
        )
        id_map, outcomes = apply_entries(client, entries, state, snapshot)
    except PromoteError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    # The promotion document is the journal's whole verdict, in journal order: replayable
    # outcomes and the unreplayable rows the check refused to drop.
    outcomes = sorted(
        unreplayable_outcomes + outcomes, key=lambda outcome: outcome.entry.lineno
    )

    document = build_promotion(
        entries + unreplayable, id_map, outcomes, journal_path, args.url, snapshot,
        local_bin=args.local_bin,
    )
    output_path = resolve_path(args.output)
    write_promotion(output_path, document)
    failures = [
        outcome for outcome in outcomes
        if outcome.verdict not in ("created", "skipped")
    ]
    for outcome in outcomes:
        detail = f": {outcome.detail}" if outcome.detail else ""
        label = outcome.entry.natural_key or f"line {outcome.entry.lineno}"
        print(
            f"{label}: {outcome.verdict} "
            f"-> {outcome.production_id}{detail}",
            file=sys.stderr,
        )
    print(f"promotion written: {output_path}", file=sys.stderr)
    if failures:
        print(
            f"apply: {len(failures)} of {len(outcomes)} entries failed; see "
            f"{output_path}",
            file=sys.stderr,
        )
        return 1
    print(f"apply: {len(outcomes)} entries created or skipped", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
