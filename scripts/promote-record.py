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
* Measurements: this release has **no** ``benefits.measurement_list`` action.  ``--apply``
  therefore requires ``--snapshot DIR`` and reads the production MSP export
  (``msp.json``).  A measurement created in this run cannot appear in a pre-run export,
  so its readback is a **failure** until a measurement read action exists; the verdict is
  recorded as ``readback_failed`` and the command exits non-zero.  It is never reported
  as a pass.

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
    # The tr3 record family: a register risk edited in place, a benefits-map edge, and the
    # architecture decision register's rows.
    "registers.update",
    "benefits.link",
    "decisions.create",
    "decisions.transition",
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
}

# The release this script ships with has no action that lists measurements.  If one is
# added later, `load_state` prefers it over the snapshot.  Kept as a constant so the
# absence is visible rather than implicit.
MEASUREMENT_LIST_ACTION = "benefits.measurement_list"

TOKEN = r"[A-Za-z0-9][A-Za-z0-9._-]*"
RISK_KEY_RE = re.compile(rf"^risk:({TOKEN})(?:/(update))?$")
BENEFIT_KEY_RE = re.compile(rf"^benefit:({TOKEN})(?:/(baseline|fields|status))?$")
CHANGE_KEY_RE = re.compile(r"^change:(.+)$")
APPROVAL_KEY_RE = re.compile(r"^approval:([^/:]+)[/:](.+)$")
DOCUMENT_KEY_RE = re.compile(r"^document:(.+)$")
DOCUMENT_PUBLISH_KEY_RE = re.compile(r"^document-publish:(.+)$")
PROJECT_CLOSURE_KEY_RE = re.compile(r"^project:closure:([0-9a-f-]{36})$")
PROJECT_CLOSE_KEY_RE = re.compile(r"^project:close:([0-9a-f-]{36})$")
CARD_COMMENT_KEY_RE = re.compile(r"^card-comment:([0-9a-f-]{36}):([0-9a-f]{8,64})$")
CARD_MOVE_KEY_RE = re.compile(r"^card-move:([0-9a-f-]{36}):(intake|active|waiting|done|archive)$")
BENEFIT_LINK_KEY_RE = re.compile(rf"^benefit-link:({TOKEN}):(.+)$")
DECISION_KEY_RE = re.compile(rf"^decision:({TOKEN})(?:/(draft|in_review|approved|superseded|archived))?$")


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
        if self.kind == "project_close":
            return f"project:close:{self.first}"
        if self.kind == "card_comment":
            return f"card-comment:{self.first}:{self.second}"
        if self.kind == "card_move":
            return f"card-move:{self.first}:{self.second}"
        if self.kind == "benefit_link":
            return f"benefit-link:{self.first}:{self.second}"
        if self.kind == "decision":
            return f"decision:{self.first}" + (f"/{self.second}" if self.second else "")
        raise PromoteError(f"unknown natural-key kind {self.kind!r}")


def parse_natural_key(value: str) -> NaturalKey:
    if not isinstance(value, str) or not value:
        raise PromoteError("natural_key must be a non-empty string")
    match = RISK_KEY_RE.match(value)
    if match:
        return NaturalKey("risk", match.group(1), match.group(2))
    match = DECISION_KEY_RE.match(value)
    if match:
        return NaturalKey("decision", match.group(1), match.group(2))
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
    match = PROJECT_CLOSE_KEY_RE.match(value)
    if match:
        return NaturalKey("project_close", match.group(1))
    match = CARD_COMMENT_KEY_RE.match(value)
    if match:
        return NaturalKey("card_comment", match.group(1), match.group(2))
    match = CARD_MOVE_KEY_RE.match(value)
    if match:
        return NaturalKey("card_move", match.group(1), match.group(2))
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


def build_entries(raws: list[tuple[int, dict]]) -> tuple[list[Entry], list[str]]:
    entries: list[Entry] = []
    errors: list[str] = []
    for lineno, raw in raws:
        action = raw.get("action")
        if not isinstance(action, str) or action not in WRITE_ACTIONS:
            errors.append(f"line {lineno}: unknown action {action!r}")
            continue
        natural_key = raw.get("natural_key")
        try:
            key = parse_natural_key(natural_key)
        except PromoteError as error:
            errors.append(f"line {lineno}: {error}")
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
            )
        )
    return entries, errors


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
            if key.kind != "benefit" or key.second not in ("fields", "status"):
                errors.append(
                    f"line {entry.lineno}: benefits.update key must be "
                    "benefit:<reference>/fields or benefit:<reference>/status"
                )
                continue
            if not entry.payload.get("id"):
                errors.append(f"line {entry.lineno}: update input has no id")
            if key.second == "status":
                if not entry.payload.get("status"):
                    errors.append(
                        f"line {entry.lineno}: status update input has no status"
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
        elif entry.action == "approvals.request":
            if key.kind != "approval":
                errors.append(
                    f"line {entry.lineno}: approvals.request key must be approval:<gate>/<target>"
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
    return errors


def check_journal(path: Path) -> tuple[list[Entry], list[str]]:
    header, raws, errors = load_journal(path)
    if header is not None and header.get("schema") != JOURNAL_SCHEMA:
        errors.append(
            f"journal header schema is {header.get('schema')!r}, expected {JOURNAL_SCHEMA!r}"
        )
    entries, entry_errors = build_entries(raws)
    errors.extend(entry_errors)
    errors.extend(validate_entries(entries))
    return entries, errors


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
    ) -> Any:
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
    which the CLI would read as its envelope, so it is wrapped.
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
    ) -> Any:
        stdin = self.build_stdin(payload)
        environment = dict(os.environ)
        environment.setdefault("RUST_LOG", "error")
        try:
            completed = subprocess.run(
                self.build(action, approval_id=approval_id, if_match=if_match),
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
    # The tr3 record family's production rows: benefits-map edges by benefit.
    links: list[dict] = field(default_factory=list)
    # The document family's production rows, by (project, category) as listed.
    documents: list[dict] = field(default_factory=list)
    # The architecture decision register's production rows, by key.
    decisions: list[dict] = field(default_factory=list)


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


def collect_ids(entries: list[Entry]) -> tuple[set[str], set[str]]:
    programme_ids: set[str] = set()
    project_ids: set[str] = set()
    for entry in entries:
        programme = entry.payload.get("programme_id")
        if isinstance(programme, str) and programme:
            programme_ids.add(programme)
        project = entry.payload.get("project_id")
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
        for project_id in sorted(project_ids):
            row = client.invoke("projects.get", {"id": project_id})
            if isinstance(row, dict):
                state.projects[project_id] = row
        for entry in entries:
            if entry.action not in ("cards.comment", "cards.move"):
                continue
            card_id = entry.payload.get("id")
            if not card_id:
                raise PromoteError(f"{entry.action}: the input names no card id")
            card = client.invoke("cards.get", {"id": card_id})
            if not isinstance(card, dict):
                raise PromoteError(f"cards.get: no card {card_id}")
            for comment in card.get("comments") or []:
                body = comment.get("body") if isinstance(comment, dict) else None
                if isinstance(body, str):
                    comment["_sha"] = hashlib.sha256(body.encode("utf-8")).hexdigest()
            state.cards[card_id] = card
            board_id = card.get("board_id")
            if board_id:
                for row in list_all(client, "lists.list", {"board_id": board_id}):
                    if row.get("id"):
                        state.lists[row["id"]] = row
        # A projects.close replay needs the project's gates even when no change entry does.
        for project_id in sorted(project_ids):
            if not any(row.get("project_id") == project_id for row in state.approvals):
                state.approvals.extend(
                    list_all(client, "approvals.list", {"project_id": project_id})
                )
    if snapshot is not None:
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


def change_rows(state: State, key: NaturalKey, payload: dict) -> list[dict]:
    labels = {key.first}
    title = payload.get("title")
    if isinstance(title, str) and title:
        labels.add(title)
    return [
        row
        for row in state.registers
        if row.get("kind") == "change" and row.get("title") in labels
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


def update_already_reflected(state: State, key: NaturalKey, payload: dict) -> bool:
    benefit = benefit_by_reference(state, key.first)
    if benefit is None:
        return False
    if key.second == "status":
        return benefit.get("status") == payload.get("status")
    for name, value in payload.items():
        if name in ("id", "change_id"):
            continue
        if benefit.get(name) != value:
            return False
    return True


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


def requested_target(payload: dict) -> str | None:
    """The engine's target for an `approvals.request` payload, where it is derivable."""
    if payload.get("target"):
        return str(payload["target"])
    if payload.get("action") == "projects.close":
        inner = payload.get("input")
        project_id = inner.get("id") if isinstance(inner, dict) else None
        return f"project/{project_id}/close" if project_id else None
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
) -> dict | None:
    """The production row that already carries this entry's natural key, if any."""
    key = entry.key
    assert key is not None
    if entry.action == "registers.create":
        return risk_by_code(state, key.first)
    if entry.action == "benefits.create":
        return benefit_by_reference(state, key.first)
    if entry.action == "benefits.measure":
        return baseline_measurement(state, key.first)
    if entry.action == "changes.request":
        return change_row(state, key, entry.payload)
    if entry.action == "approvals.decide" and key.first in ("closure", "projects.close"):
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
        if update_already_reflected(state, key, entry.payload):
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
    if entry.action == "projects.close_request":
        return closure_gate(state, key.first)
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
        if card is not None and lane_kind(state, card.get("list_id")) == key.second:
            return card
        return None
    return None


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
        index[entry.key.first] = {
            "change_id": change.get("id"),
            "approval_id": gate.get("id") if gate else None,
        }
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


def readback_measurement(snapshot: str | None, row_id: str) -> dict:
    if not snapshot:
        raise ReadbackError(
            "no benefits.measurement_list action exists and no --snapshot was given; "
            f"measurement {row_id} cannot be read back"
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
    elif entry.action == "documents.publish":
        staging = payload.get("id")
        production = id_map.get(staging) if staging else None
        if not production:
            raise PromoteError(
                f"document {staging!r} has no production id; the compose entry must "
                "create or find it first"
            )
        payload["id"] = production
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
        if resolved is None:
            raise PromoteError(f"cards.move: no {key.second!r} lane on the card's board")
        payload["list_id"] = resolved
    elif entry.action == "approvals.request":
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
    elif entry.action == "approvals.decide":
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
) -> dict | None:
    """Read the created row back.  Raises PromoteError when it cannot be read."""
    key = entry.key
    assert key is not None
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
        return readback_measurement(snapshot, production_id)
    if entry.action == "projects.close_request":
        project_id = entry.payload.get("id")
        if not project_id:
            raise ReadbackError("projects.close_request: the input names no project")
        project = read_project(client, str(project_id))
        if gate_id:
            readback_gate(client, str(project_id), gate_id)
        if closure_gate(state, str(project_id)) is None:
            state.approvals.append(
                {"id": gate_id, "gate": "closure", "status": "approved",
                 "project_id": str(project_id)}
            )
        return project
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
        card = read_row(client, "cards.get", key.first)
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
        card = read_row(client, "cards.get", key.first)
        kind = lane_kind(state, card.get("list_id"))
        if kind != key.second:
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
    raise PromoteError(f"{entry.action}: no readback is defined")


def apply_entries(
    client: Any,
    entries: list[Entry],
    state: State,
    snapshot: str | None,
) -> tuple[dict[str, str], list[Outcome]]:
    id_map: dict[str, str] = {}
    change_index = build_change_index(entries, state)
    outcomes: list[Outcome] = []
    for entry in entries:
        key = entry.key
        assert key is not None
        try:
            existing = existing_for(entry, state, change_index)
            if existing is not None:
                production_id = str(existing["id"])
                id_map[entry.output_id] = production_id
                if entry.action == "changes.request":
                    gate = gate_for_change(state, existing)
                    change_index[key.first] = {
                        "change_id": production_id,
                        "approval_id": str(gate["id"]) if gate else None,
                    }
                    if entry.raw.get("approval_id") and gate is not None:
                        id_map[entry.raw["approval_id"]] = str(gate["id"])
                outcomes.append(
                    Outcome(entry, entry.output_id, production_id, "skipped")
                )
                continue

            payload = remap_payload(entry, state, id_map, change_index)
            if entry.action == "cards.move":
                current = read_row(client, "cards.get", str(payload["id"]))
                version = current.get("version")
                if not isinstance(version, int):
                    raise PromoteError(
                        f"cards.get: {payload['id']} has no version for If-Match"
                    )
                output = client.invoke(entry.action, payload, if_match=version)
            elif entry.action == "documents.publish":
                current = read_row(client, "documents.get", str(payload["id"]))
                version = current.get("version")
                if not isinstance(version, int):
                    raise PromoteError(
                        f"documents.get: {payload['id']} has no version for If-Match"
                    )
                output = client.invoke(entry.action, payload, if_match=version)
            elif entry.action == "registers.update":
                current = read_row(client, "registers.get", str(payload["id"]))
                version = current.get("version")
                if not isinstance(version, int):
                    raise PromoteError(
                        f"registers.get: {payload['id']} has no version for If-Match"
                    )
                output = client.invoke(entry.action, payload, if_match=version)
            elif entry.action == "benefits.update":
                current = read_row(client, "benefits.get", payload["id"])
                version = current.get("version")
                if not isinstance(version, int):
                    raise PromoteError(
                        f"benefits.get: {payload['id']} has no version for If-Match"
                    )
                output = client.invoke(entry.action, payload, if_match=version)
            else:
                output = client.invoke(entry.action, payload)
            production_id, gate_id = output_id_for(entry.action, output)
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
                if entry.raw.get("approval_id") and gate_id:
                    id_map[entry.raw["approval_id"]] = gate_id

            row = readback_entry(
                entry, client, state, production_id, gate_id, snapshot
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
    ) -> Any:
        self.calls.append(
            (action, copy.deepcopy(payload), if_match, approval_id)
        )
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
                    gate["status"] = expected_gate_status(payload)
                    gate["version"] += 1
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
            return copy.deepcopy(card)
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
            }
        elif name == "close_gate":
            raw = {
                "action": "approvals.request", "input": str(paths["close_gate"]),
                "natural_key": "approval:projects.close:project/"
                                "01a1dcb8-d24b-76de-b901-56fd56e001bc/close",
                "output_id": "01a20520-6853-73c3-8b04-4139d64d3ef8",
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
        entries, errors = build_entries(_fixture_entries(paths, full))
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

        # 2. duplicate refusal.
        duplicate = _fixture_entries(paths, ("risk", "risk"))
        entries, errors = build_entries(duplicate)
        ok(not errors, "duplicate fixture parses")
        ok(
            any("duplicate natural key" in error for error in validate_entries(entries)),
            "a duplicate natural key is refused",
        )

        # 3. malformed keys and unknown actions are refused.
        malformed = [
            (2, {"action": "registers.create", "input": str(paths["risk"]),
                 "natural_key": "risk:", "output_id": "x"}),
            (3, {"action": "risks.create", "input": str(paths["risk"]),
                 "natural_key": "risk:R1", "output_id": "x"}),
            (4, {"action": "registers.create", "input": "/does/not/exist.json",
                 "natural_key": "risk:R1", "output_id": "x"}),
        ]
        _, errors = build_entries(malformed)
        ok(any("malformed natural key" in error for error in errors),
           "a malformed natural key is refused")
        ok(any("unknown action" in error for error in errors),
           "an unknown action is refused")
        ok(any("does/not/exist" in error for error in errors),
           "a missing input is refused")

        # 4. natural-key matching against fake rows.
        state = State()
        state.registers.append(
            {"id": "prod-risk", "kind": "risk", "code": "R1",
             "programme_id": "p", "version": 1}
        )
        entries, _ = build_entries(_fixture_entries(paths, ("risk",)))
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
        entries, _ = build_entries(_fixture_entries(paths, full))
        state = State()
        client = FakeClient(state)
        id_map, outcomes = apply_entries(client, entries, state, str(snapshot))
        ok(
            [outcome.verdict for outcome in outcomes]
            == ["created"] * len(full),
            f"first apply creates every entry: "
            f"{[(o.entry.natural_key, o.verdict, o.detail) for o in outcomes]}",
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

        # 6. a created row that cannot be read back is a failure, never a pass.
        entries, _ = build_entries(_fixture_entries(paths, ("risk",)))
        state = State()
        _, outcomes = apply_entries(
            FakeClient(state, unreadable={"registers.get"}),
            entries, state, str(snapshot),
        )
        ok(outcomes[0].verdict == "readback_failed",
           f"an unreadable row is readback_failed: {outcomes[0].verdict}")

        # 7. a measurement has no product read action: its readback fails, loudly.
        entries, _ = build_entries(_fixture_entries(paths, ("benefit", "measure")))
        state = State()
        _, outcomes = apply_entries(FakeClient(state), entries, state, str(snapshot))
        ok(outcomes[1].verdict == "readback_failed",
           f"a measurement absent from msp.json fails readback: {outcomes[1].verdict}")
        ok(
            "measurement" in outcomes[1].detail,
            "the measurement failure says why",
        )

        # 8. the closure family: a close chain applies once and replays as skips.
        project = "01a1dcb8-d24b-76de-b901-56fd56e001bc"
        card = "01a1ffeb-311e-708e-a994-83eb40fd3a69"
        board = "01a1d3e7-bbfa-7e5f-a4d2-4b44ca69886c"
        intake = "01a1d3e7-bc47-7384-94cf-f1009fd9c0b7"
        staging_done = "01a1d3e7-bd2f-7998-bf7b-8b4039393db9"
        production_done = "01a1d3e7-0000-4000-8000-00000000d0e5"
        closure_names = ("closure", "close_gate", "close", "comment", "move")
        entries, errors = build_entries(_fixture_entries(paths, closure_names))
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
        _, second = apply_entries(FakeClient(state), entries, state, str(snapshot))
        ok(
            [outcome.verdict for outcome in second]
            == ["skipped"] * len(closure_names),
            f"a replay skips the whole closure chain: "
            f"{[(o.entry.natural_key, o.verdict, o.detail) for o in second]}",
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
        entries, errors = build_entries(raw_entries)
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
        entries, errors = build_entries(doc_entries)
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
        entries, errors = build_entries([
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
        entries, errors = build_entries([
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
        create_entries, errors = build_entries([
            (2, {"action": "decisions.create", "input": str(decide_create_path),
                 "natural_key": "decision:adr-9999", "output_id": "staging-decision"}),
        ])
        _, third = apply_entries(FakeClient(state), create_entries, state, None)
        ok([o.verdict for o in third] == ["skipped"], f"a create replay skips: {third}")

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
        help="directory holding the production msp.json export",
    )
    parser.add_argument("--programme-id", default=None)
    parser.add_argument("--timeout", type=float, default=60.0)
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()

    journal_path = resolve_path(args.journal)
    entries, errors = check_journal(journal_path)
    if errors:
        for error in errors:
            print(f"error: {error}", file=sys.stderr)
        print(f"check: {len(errors)} error(s); refusing to continue", file=sys.stderr)
        return 1

    if args.dry_run:
        print(json.dumps(dry_run_plan(entries), indent=2))
        print(
            f"dry-run: {len(entries)} entries ({summary(entries)}); no network",
            file=sys.stderr,
        )
        return 0

    if not args.apply:
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
    needs_snapshot = any(entry.action == "benefits.measure" for entry in entries)
    snapshot = args.snapshot
    if needs_snapshot and not snapshot:
        print(
            "error: this release has no benefits.measurement_list action; --apply "
            "requires --snapshot DIR holding the production msp.json export",
            file=sys.stderr,
        )
        return 1

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

    document = build_promotion(
        entries, id_map, outcomes, journal_path, args.url, snapshot,
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
        print(
            f"{outcome.entry.natural_key}: {outcome.verdict} "
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
