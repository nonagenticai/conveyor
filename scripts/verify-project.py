#!/usr/bin/env python3
"""Verify one programme project against its executable acceptance manifest.

The manifest is `acceptance/programme/<KEY>.toml`. Each `[[check]]` is either a named
snapshot check from the shared registry below or a shell `command`; the verifier runs it,
captures the exit and output, and writes `docs/evidence/rework/<KEY>.json` with the
snapshot's SHA. A project closes only on a green manifest (`--project <KEY>` exits 0).

Usage:
    python3 scripts/verify-project.py --self-test
    python3 scripts/verify-project.py --project P0
    python3 scripts/verify-project.py --all
    python3 scripts/verify-project.py --project P0 --snapshot /tmp/u5a-snapshot/data
    python3 scripts/verify-project.py --project P0 --kind all|snapshot|command

Manifest shape:

    [project]
    key = "P0"
    name = "..."
    gate = "the project's declared acceptance, in one sentence"
    gate_checks = ["P0-closure-outputs", "P0-charter"]   # ids that must run and pass

    [[check]]
    id = "P0-closure-outputs"
    check = "closure_outputs"          # a registry name, or omit and give `command`
    description = "..."
    severity = "P0"                   # P0 | P1 | P2
    kind = "snapshot"                 # snapshot | command
    # command = "..."                 # required when kind = "command"

A `[[allow]]` block is refused as a finding: no deferral, waiver or residual is a
disposition for programme work (docs/decisions/0012-no-deferrals.md). An active or
closed project's `gate` must be discharged by its `gate_checks`: each named id must
resolve to a check in this manifest, and that check must run and pass.

The shared registry is deliberately small: a check that needs the live engine is a
`command` clause (usually `programme-record.py --check`), so the static verdict is always
reproducible from the checked-in snapshot.
"""

from __future__ import annotations

import argparse
import copy
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable, NoReturn

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from rework_register import RegisterError, build as build_rework_register  # noqa: E402

DEFAULT_SNAPSHOT = ROOT / "docs" / "design" / "data"
MANIFEST_DIR = ROOT / "acceptance" / "programme"
DEFAULT_EVIDENCE = ROOT / "docs" / "evidence" / "rework"
PLACEHOLDER = re.compile(
    r"(?<![A-Za-z0-9])"
    r"(not measured|no instrument|unknown|tbd|to be determined|n/?a|placeholder|none)"
    r"(?![A-Za-z0-9])",
    re.IGNORECASE,
)
SEVERITY = ("P0", "P1", "P2")
OPEN_PROJECT_STATES = ("active", "closed")
UNRESOLVED_PACKAGE_STATES = ("open", "suppressed")


def fail(message: str, code: int = 2) -> NoReturn:
    print(f"verify-project: {message}", file=sys.stderr)
    raise SystemExit(code)


def read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        fail(f"{path} is missing")
    except json.JSONDecodeError as error:
        fail(f"{path} is not JSON: {error}")


def read_outputs(path: Path) -> dict[str, Any]:
    try:
        text = path.read_text(encoding="utf-8")
    except FileNotFoundError:
        fail(f"{path} is missing (the snapshot's output index)")
    start = text.find("{")
    end = text.rfind("}")
    if start < 0 or end < start:
        fail(f"{path} carries no JSON object")
    try:
        value = json.loads(text[start : end + 1])
    except json.JSONDecodeError as error:
        fail(f"{path} is not JSON: {error}")
    if not isinstance(value, dict) or not isinstance(value.get("outputs"), list):
        fail(f"{path} does not carry an `outputs` array")
    return value


class Snapshot:
    """The read-only public record the verifier reads."""

    def __init__(self, path: Path) -> None:
        self.path = path
        self.programme = read_json(path / "programme.json")
        self.architecture = read_json(path / "architecture.json")
        self.work = read_json(path / "work.json")
        self.msp = read_json(path / "msp.json")
        self.artefacts = read_json(path / "artefacts.json")
        self.manifest = read_json(path / "manifest.json")
        self.outputs = read_outputs(path.parent / "docs" / "outputs.js").get("outputs", [])
        self.projects = {
            str(row.get("key")): row for row in self.programme.get("projects", [])
        }

    def project(self, key: str) -> dict[str, Any]:
        row = self.projects.get(key)
        if row is None:
            fail(f"the snapshot has no project {key!r}")
        return row

    def contracts(self, project_id: str) -> list[dict[str, Any]]:
        return [
            row
            for row in self.architecture.get("contracts", [])
            if row.get("project_id") == project_id
        ]

    def reviews(self, project_id: str) -> list[dict[str, Any]]:
        contracts = {row.get("id") for row in self.contracts(project_id)}
        return [
            row
            for row in self.architecture.get("compliance_reviews", [])
            if row.get("contract_id") in contracts or row.get("project_id") == project_id
        ]

    def approvals(self, project_id: str, gate: str | None = None) -> list[dict[str, Any]]:
        return [
            row
            for row in self.work.get("approvals", [])
            if row.get("project_id") == project_id and (gate is None or row.get("gate") == gate)
        ]

    def decisions(self, approval_id: str) -> list[dict[str, Any]]:
        return [
            row
            for row in self.work.get("approval_decisions", [])
            if row.get("approval_id") == approval_id
        ]

    def approval_by_id(self, approval_id: str) -> dict[str, Any] | None:
        for row in self.work.get("approvals", []):
            if row.get("id") == approval_id:
                return row
        return None

    def waivers(self, project_id: str) -> list[dict[str, Any]]:
        review_ids = {row.get("id") for row in self.reviews(project_id)}
        return [
            row
            for row in self.architecture.get("waivers", [])
            if row.get("review_id") in review_ids
        ]

    def published(self, project_id: str, category: str) -> bool:
        return any(
            row.get("project_id") == project_id
            and row.get("category") == category
            and row.get("published_at")
            for row in self.outputs
        )

    def published_id(self, document_id: str) -> bool:
        return any(
            row.get("id") == document_id and row.get("published_at") for row in self.outputs
        )


# ── the shared check registry ────────────────────────────────────────────────────────


def check_project(
    snapshot: Snapshot, key: str, params: dict[str, Any]
) -> tuple[bool, str]:
    row = snapshot.project(key)
    expected = params.get("status")
    if expected and row.get("status") != expected:
        return False, f"status is {row.get('status')!r}, expected {expected!r}"
    return True, f"project {key} is {row.get('status')}"


def check_closure_outputs(
    snapshot: Snapshot, key: str, params: dict[str, Any]
) -> tuple[bool, str]:
    row = snapshot.project(key)
    categories = row.get("meta", {}).get("closure_outputs") or []
    if not categories:
        if params.get("allow_empty"):
            return True, "no closure outputs declared (allowed for this row)"
        return False, "the project declares no closure_outputs"
    missing = [name for name in categories if not snapshot.published(row["id"], name)]
    if missing:
        return False, "declared closure outputs are not published: " + ", ".join(missing)
    return True, f"{len(categories)} closure outputs published"


def check_project_baseline(
    snapshot: Snapshot, key: str, params: dict[str, Any]
) -> tuple[bool, str]:
    row = snapshot.project(key)
    baselines = [
        baseline
        for baseline in snapshot.msp.get("project_baselines", [])
        if baseline.get("project_id") == row["id"]
    ]
    if not baselines:
        return False, "no project baseline"
    approved = [baseline for baseline in baselines if baseline.get("approved_by")]
    if not params.get("require_approved", False):
        return True, f"{len(baselines)} project baseline(s); {len(approved)} approved"
    for baseline in approved:
        taken_by = baseline.get("taken_by")
        if taken_by and baseline.get("approved_by") == taken_by:
            continue
        if not (baseline.get("snapshot") or {}).get("frozen"):
            continue
        return True, (
            f"baseline {baseline.get('id')} approved by {baseline.get('approved_by')}, "
            f"taken by {taken_by or 'no recorded taker'}, snapshot frozen"
        )
    return False, (
        "no approved baseline is a frozen snapshot approved by someone other than its taker"
    )


def check_contract(
    snapshot: Snapshot, key: str, params: dict[str, Any]
) -> tuple[bool, str]:
    row = snapshot.project(key)
    live = [c for c in snapshot.contracts(row["id"]) if c.get("status") == "live"]
    if not live:
        if params.get("required", True):
            return False, "no live board-approved delivery contract"
        return True, "no delivery contract required for this row"
    for contract in live:
        approval_id = contract.get("approval_id")
        requester = contract.get("requested_by")
        approver = contract.get("approved_by")
        if approval_id:
            approval = snapshot.approval_by_id(str(approval_id))
            if approval is None or approval.get("status") != "approved":
                continue
            # `architecture.contracts_approve` accepts only an approved `architecture_board`
            # decision ABOUT THIS CONTRACT (meta subject_kind/subject_id). The check mirrors the
            # action, so a pasted id or another contract's decision cannot pass.
            meta = approval.get("meta") or {}
            if approval.get("gate") != "architecture_board":
                continue
            if meta.get("subject_kind") != "contract":
                continue
            if str(meta.get("subject_id")) != str(contract.get("id")):
                continue
            # `decide_board_tx` writes the board decision as an APPROVED approval row with no
            # separate decision row (its `requested_by` is the deciding actor). The action's
            # distinctness rule is therefore the contract's requester vs the actor who approved
            # the contract, not an approval_decision — requiring one would fail every lawful
            # board decision.
            if requester and approver and requester != approver:
                return True, (
                    f"contract {contract.get('id')} approved by board decision {approval_id} "
                    f"about this contract; approver {approver} distinct from requester {requester}"
                )
            continue
        # Mirrors the action's other lawful path: a contract with a recorded requester and a kind
        # other than `delivery_contract` may be approved directly by a distinct actor.
        if (
            contract.get("kind") != "delivery_contract"
            and requester
            and approver
            and requester != approver
        ):
            return True, (
                f"contract {contract.get('id')} approved by {approver}, "
                f"requested by {requester} (a distinct actor)"
            )
    return False, (
        "the live contract has no approved board decision about this contract and no distinct "
        "requester/approver pair"
    )


def _timestamp(value: Any) -> datetime:
    """Parse an ISO-8601 column for ordering; an unparseable value sorts first."""
    try:
        parsed = datetime.fromisoformat(str(value).replace("Z", "+00:00"))
    except (TypeError, ValueError):
        return datetime.min.replace(tzinfo=timezone.utc)
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed


def _active_waiver(snapshot: Snapshot, project_id: str) -> dict[str, Any] | None:
    """The active TOGAF architecture waiver on a compliance review, if one exists.

    Only an `architecture_waiver` may suppress a non-conformant architecture review. A
    programme waiver (any other kind) never suppresses a programme finding (ADR 0012).
    """
    now = datetime.now(timezone.utc)
    for waiver in snapshot.waivers(project_id):
        if waiver.get("status") != "active":
            continue
        kind = str(
            waiver.get("kind") or waiver.get("waiver_type") or "architecture_waiver"
        )
        if kind != "architecture_waiver" or not waiver.get("review_id"):
            continue
        if _timestamp(waiver.get("reassessment_at")) <= now:
            continue
        return waiver
    return None


def check_review(
    snapshot: Snapshot, key: str, params: dict[str, Any]
) -> tuple[bool, str]:
    row = snapshot.project(key)
    live = {
        contract.get("id")
        for contract in snapshot.contracts(row["id"])
        if contract.get("status") == "live"
    }
    reviews = [
        review
        for review in snapshot.architecture.get("compliance_reviews", [])
        if review.get("contract_id") in live
    ]
    if not reviews:
        if params.get("required", True):
            return False, "no Phase-G compliance review for the project's live contract"
        return True, "no compliance review required for this row"
    waiver = _active_waiver(snapshot, row["id"])
    for review in reviews:
        evidence = review.get("evidence") or {}
        reference = evidence.get("delegation_ref")
        if not reference or not snapshot.published_id(str(reference)):
            continue
        level = review.get("level")
        if level in ("non_conformant", "irrelevant") and waiver is None:
            continue
        return True, (
            f"review {review.get('id')} ({level}) of the live contract cites "
            f"the published delegation {reference}"
        )
    return False, (
        "the live contract's review's delegation_ref is missing or unpublished, "
        "or its level is non-conformant/irrelevant without an active waiver"
    )


def check_closure_gate(
    snapshot: Snapshot, key: str, params: dict[str, Any]
) -> tuple[bool, str]:
    row = snapshot.project(key)
    gates = [
        gate
        for gate in snapshot.approvals(row["id"], "closure")
        if gate.get("status") == "approved"
    ]
    if not gates:
        return False, "no approved closure gate for the project"
    gate = max(gates, key=lambda gate_row: _timestamp(gate_row.get("created_at")))
    distinct = [
        decision
        for decision in snapshot.decisions(str(gate["id"]))
        if decision.get("decision") == "approve"
        and decision.get("approver_id")
        and decision.get("approver_id") != gate.get("requested_by")
    ]
    if not distinct:
        return False, (
            f"the latest approved closure gate {gate.get('id')} carries no approve "
            "decision by an approver other than its requester"
        )
    # The gate must authorise THIS project's close, not another row's. The action computes the
    # target; the projection now carries meta, so the claim is checkable.
    target = (gate.get("meta") or {}).get("target")
    if target and str(target) != f"project/{row['id']}/close":
        return False, (
            f"the latest approved closure gate {gate.get('id')} targets {target}, not this project"
        )
    return True, f"latest closure gate {gate.get('id')} decided by a distinct approver"


def check_waivers(
    snapshot: Snapshot, key: str, params: dict[str, Any]
) -> tuple[bool, str]:
    row = snapshot.project(key)
    reviews = {r.get("id"): r for r in snapshot.reviews(row["id"])}
    now = datetime.now(timezone.utc)
    active = []
    for waiver in snapshot.waivers(row["id"]):
        if waiver.get("status") != "active":
            continue
        active.append(waiver)
        reassessment = waiver.get("reassessment_at")
        try:
            expires = datetime.fromisoformat(str(reassessment).replace("Z", "+00:00"))
        except (TypeError, ValueError):
            return False, f"waiver {waiver.get('id')} has no valid reassessment date"
        if expires <= now:
            return False, f"waiver {waiver.get('id')} is expired"
        review = reviews.get(waiver.get("review_id"))
        if review and waiver.get("owner_user_id") == review.get("reviewed_by"):
            return False, f"waiver {waiver.get('id')} is owned by its reviewer"
    return True, f"{len(active)} active waiver(s), all dated and distinct from the reviewer"


def check_benefit_baseline(
    snapshot: Snapshot, key: str, params: dict[str, Any]
) -> tuple[bool, str]:
    profiles = snapshot.msp.get("benefit_profiles", [])
    if not profiles:
        return False, "the programme has no benefit profiles"
    for profile in profiles:
        baseline = profile.get("baseline")
        # A baseline is a measurement, not a sentence: a string carries no value, unit,
        # date, method or evidence, so it cannot prove the benefit was measured — whether
        # or not it happens to read like a placeholder.
        if not isinstance(baseline, dict):
            return False, (
                f"benefit {profile.get('id')} baseline is not a structured measurement: "
                f"{baseline!r}"
            )
        missing = [
            field
            for field in ("value", "unit", "measured_at", "method", "evidence")
            if baseline.get(field) is None
            or (isinstance(baseline.get(field), str) and not baseline.get(field).strip())
        ]
        if missing:
            return False, f"benefit {profile.get('id')} baseline is missing {', '.join(missing)}"
    return True, f"{len(profiles)} benefit baselines are measured"


# ── SAFe §8.2 — the cadence, the plan, the demo and the retro ─────────────────────────


def _tranche_period(snapshot: Snapshot, key: str) -> tuple[dict[str, Any], int]:
    row = snapshot.project(key)
    tranche = (row.get("meta") or {}).get("tranche")
    if tranche is None:
        fail(f"{key}: the project names no tranche; the SAFe cadence cannot be resolved")
    return row, int(tranche)


def _tranche_row(snapshot: Snapshot, number: int) -> dict[str, Any] | None:
    for tranche in snapshot.msp.get("tranches", []):
        if tranche.get("number") == number:
            return tranche
    return None


def _cadences(
    snapshot: Snapshot, row: dict[str, Any], tranche_number: int, kind: str
) -> list[dict[str, Any]]:
    """The cadence rows for this tranche, linked by the row's meta or its period.

    The product's own link is the cadence row's `meta` (`tranche_id`/`tranche_number`); a
    period of `tranche-<n>` or `<n>` is accepted too, because `period` is free text and the
    retrospective batch writes it that way.
    """
    tranche = _tranche_row(snapshot, tranche_number) or {}
    tranche_id = tranche.get("id")
    accepted_periods = {f"tranche-{tranche_number}", str(tranche_number)}
    found = []
    for cadence in snapshot.msp.get("cadences", []):
        if cadence.get("programme_id") != row.get("programme_id"):
            continue
        if cadence.get("kind") != kind:
            continue
        meta = cadence.get("meta") if isinstance(cadence.get("meta"), dict) else {}
        linked = (
            (tranche_id is not None and cadence.get("tranche_id") == tranche_id)
            or (tranche_id is not None and meta.get("tranche_id") == tranche_id)
            or cadence.get("tranche_number") == tranche_number
            or meta.get("tranche_number") == tranche_number
            or cadence.get("period") in accepted_periods
            or meta.get("period") in accepted_periods
        )
        if linked:
            found.append(cadence)
    return found


def _wbs_nodes(snapshot: Snapshot, row: dict[str, Any]) -> list[dict[str, Any]]:
    return [
        node
        for node in snapshot.msp.get("wbs_nodes", [])
        if node.get("project_id") == row.get("id")
    ]


def check_pi_cadence(
    snapshot: Snapshot, key: str, params: dict[str, Any]
) -> tuple[bool, str]:
    row, tranche = _tranche_period(snapshot, key)
    period = f"tranche-{tranche}"
    pi = _cadences(snapshot, row, tranche, "pi")
    refinement = _cadences(snapshot, row, tranche, "refinement")
    if not pi:
        return False, f"no PI cadence row for {period}"
    if not refinement:
        return False, f"no backlog-refinement cadence row for {period}"
    return True, f"PI cadence {pi[0].get('id')} and refinement {refinement[0].get('id')} for {period}"


WSJF_INPUTS = ("business_value", "time_criticality", "risk_reduction", "job_size")


def _wsjf_integer(value: Any) -> int | None:
    """The value when it is a JSON integer, else None (a bool or float is not an input)."""
    if isinstance(value, bool) or not isinstance(value, int):
        return None
    return value


def check_wsjf(
    snapshot: Snapshot, key: str, params: dict[str, Any]
) -> tuple[bool, str]:
    row = snapshot.project(key)
    nodes = _wbs_nodes(snapshot, row)
    if not nodes:
        return False, "no WBS deliverable carries the plan"
    bad = []
    for node in nodes:
        values = {field: _wsjf_integer(node.get(field)) for field in WSJF_INPUTS}
        absent = [field for field in WSJF_INPUTS if values[field] is None]
        if absent:
            bad.extend(f"{node.get('id')}: {field}" for field in absent)
            continue
        if values["job_size"] < 1:
            bad.append(f"{node.get('id')}: job_size")
            continue
        if all(
            values[field] == 0
            for field in ("business_value", "time_criticality", "risk_reduction")
        ):
            bad.append(f"{node.get('id')}: every value input is zero")
    if bad:
        return False, "WBS nodes without WSJF inputs: " + ", ".join(bad)
    return True, f"{len(nodes)} WBS node(s) carry the four WSJF inputs"


def _evidence_text(evidence: Any) -> str:
    """The credit's evidence as text, for the placeholder guard.

    The store copies the WBS node's evidence array onto the credit, but a snapshot may carry a
    single string; both shapes (and a stray object) are read so a placeholder cannot hide.
    """
    if isinstance(evidence, dict):
        return " ".join(str(value) for value in evidence.values())
    if isinstance(evidence, (list, tuple)):
        return " ".join(_evidence_text(item) for item in evidence)
    return str(evidence)


def check_demo_credits(
    snapshot: Snapshot, key: str, params: dict[str, Any]
) -> tuple[bool, str]:
    row = snapshot.project(key)
    accepted = [node for node in _wbs_nodes(snapshot, row) if node.get("status") == "accepted"]
    if not accepted:
        return False, "no accepted deliverable to demo"
    credits = {
        credit.get("deliverable_id"): credit
        for credit in snapshot.msp.get("demo_credits", [])
    }
    problems = []
    for node in accepted:
        evidence = (credits.get(node.get("id")) or {}).get("evidence")
        if not evidence:
            problems.append(f"{node.get('id')}: no evidence")
            continue
        text = _evidence_text(evidence)
        if PLACEHOLDER.search(text):
            problems.append(f"{node.get('id')}: placeholder evidence {text!r}")
    if problems:
        return False, (
            f"{len(problems)} accepted deliverable(s) have no demo credit with real evidence: "
            + ", ".join(problems)
        )
    return True, f"{len(accepted)} accepted deliverable(s), each with a demo credit"


def check_lessons(
    snapshot: Snapshot, key: str, params: dict[str, Any]
) -> tuple[bool, str]:
    row, tranche = _tranche_period(snapshot, key)
    period = f"tranche-{tranche}"
    retros = _cadences(snapshot, row, tranche, "retro")
    if not retros:
        return False, f"no retro cadence row for {period}"
    retro_ids = {retro.get("id") for retro in retros}
    lessons = [
        lesson
        for lesson in snapshot.msp.get("lessons", [])
        if lesson.get("cadence_id") in retro_ids
    ]
    if not lessons:
        return False, f"the {period} retro produced no lesson"
    for lesson in lessons:
        statement = str(lesson.get("statement") or "").strip()
        evidence = lesson.get("evidence")
        if statement and isinstance(evidence, list) and evidence:
            return True, (
                f"lesson {lesson.get('id')} carries a statement and evidence "
                f"from the {period} retro"
            )
    return False, f"the {period} retro's lessons carry no statement with evidence"


def _published_for_period(
    snapshot: Snapshot, category: str, period: str, programme_key: str
) -> bool:
    """The published PI artefact names its tranche in the title or unit (`tranche-N`).

    `unit` is the output's programme unit (see `documents.compose`); the retrospective batch
    writes the planning period there, and the title repeats it for a human reader. Only a row
    with `published_at` is published, and the period must be a whole token: `tranche-30` does
    not name `tranche-3`.
    """
    pattern = re.compile(rf"(?<![0-9]){re.escape(period.lower())}(?![0-9])")
    for row in snapshot.outputs:
        if row.get("category") != category or row.get("project_key") != programme_key:
            continue
        if not str(row.get("published_at") or "").strip():
            continue
        label = f"{row.get('title') or ''} {row.get('unit') or ''}".lower()
        if pattern.search(label):
            return True
    return False


def check_pi_plan(
    snapshot: Snapshot, key: str, params: dict[str, Any]
) -> tuple[bool, str]:
    _, tranche = _tranche_period(snapshot, key)
    period = f"tranche-{tranche}"
    programme_key = params.get("programme_key") or "P0"
    if not _published_for_period(snapshot, "pi-plan", period, programme_key):
        return False, f"no published pi-plan output for {period}"
    return True, f"the {period} PI plan is published"


def check_pi_objectives(
    snapshot: Snapshot, key: str, params: dict[str, Any]
) -> tuple[bool, str]:
    _, tranche = _tranche_period(snapshot, key)
    period = f"tranche-{tranche}"
    programme_key = params.get("programme_key") or "P0"
    if not _published_for_period(snapshot, "pi-objectives", period, programme_key):
        return False, f"no published pi-objectives output for {period}"
    return True, f"the {period} PI objectives are published"


_REWORK_CACHE: dict[str, Any] | None = None
_REWORK_ERROR: str | None = None


def rework_register(snapshot: Snapshot) -> tuple[dict[str, Any] | None, str | None]:
    """Build the one rework register from the same snapshot under verification.

    The register is derived, never stored; `scripts/rework_register.py` owns the build so the
    factory's card discharge and this receipt read the same document. One build per verifier
    process is enough; every project check reads the same result.
    """
    global _REWORK_CACHE, _REWORK_ERROR
    if _REWORK_CACHE is not None:
        return _REWORK_CACHE, None
    if _REWORK_ERROR is not None:
        return None, _REWORK_ERROR
    try:
        _REWORK_CACHE = build_rework_register(
            Path(snapshot.path), Path(snapshot.path).parent / "docs"
        )
    except RegisterError as error:
        _REWORK_ERROR = str(error)
        return None, _REWORK_ERROR
    return _REWORK_CACHE, None


def check_product_findings(
    snapshot: Snapshot, key: str, params: dict[str, Any]
) -> tuple[bool, str]:
    """No open P0 rework package may outlive the project that owns it.

    The register is derived from this snapshot and the checked-in v1 ledger. A package is
    cleared only when its gap disappears from a fresh audit or its finding carries
    `resolved_by`; a waiver or suppression is not a disposition (ADR 0012), so a
    suppressed package counts as open. A project cannot close over an open P0 package.
    """
    register, error = rework_register(snapshot)
    if error:
        return False, error
    # A register without packages is a derivation failure, not a clean bill; reading the
    # missing key as "no P0" is the false green this guard closes.
    if not isinstance(register, dict) or "packages" not in register:
        return False, "the register carries no packages"
    row = snapshot.project(key)
    owners = {key, str(row["id"])}
    unowned: list[str] = []
    open_p0: list[str] = []
    for package in register.get("packages") or []:
        if (
            package.get("status") not in UNRESOLVED_PACKAGE_STATES
            or package.get("severity") != "P0"
        ):
            continue
        owner = package.get("owner_project")
        # An open P0 with no owner at all is not attributable to another project; it must not
        # disappear from every project's manifest.
        if not str(owner or "").strip():
            unowned.append(str(package.get("key")))
            continue
        # The register may name the owner by project key or by project id; both are this row.
        if str(owner) in owners:
            open_p0.append(str(package.get("key")))
    if unowned:
        return False, "an open P0 package has no owner: " + ", ".join(unowned)
    if open_p0:
        return False, f"{len(open_p0)} open P0 package(s): " + ", ".join(open_p0)
    return True, "no open P0 package for this project"


STANDARD_ARTEFACT_STANDARDS = ("TOGAF", "PMP", "MSP", "SAFe")


def _register(snapshot: Snapshot) -> tuple[dict[str, Any] | None, str | None]:
    register, error = rework_register(snapshot)
    if error:
        return None, error
    if not isinstance(register, dict) or not isinstance(register.get("packages"), list):
        return None, "the register carries no packages"
    return register, None


def _planned_tranches(snapshot: Snapshot) -> set[int]:
    """The tranche numbers the programme has not yet authorised.

    SPEC_V2 §8.3: a project does not start before its tranche is authorised. A rework package
    scoped to a planned tranche is therefore not yet due. The register keeps it — the honest
    worklist — but a project is not red for work the programme's own authorisation rule forbids it
    from starting. The moment the tranche is authorised the package counts in full, and the
    tranche-exit check refuses its `continue` until it is closed.
    """
    planned: set[int] = set()
    for row in (snapshot.msp or {}).get("tranches") or []:
        try:
            number = int(row.get("number"))
        except (TypeError, ValueError):
            continue
        if str(row.get("state")) == "planned":
            planned.add(number)
    return planned


def _tranche_states(snapshot: Snapshot) -> dict[int, str]:
    """Each tranche's recorded state, by number."""
    states: dict[int, str] = {}
    for row in (snapshot.msp or {}).get("tranches") or []:
        try:
            number = int(row.get("number"))
        except (TypeError, ValueError):
            continue
        states[number] = str(row.get("state") or "")
    return states


def _package_due(
    package: dict[str, Any], planned: set[int], states: dict[int, str]
) -> bool:
    """A tranche-scope package is due only once its tranche is authorised.

    A tranche's review is its closing act, not its opening one: it is due only when the
    tranche is **closed**. A review demanded for a planned or authorised tranche would
    gate every member on work the programme's own sequence forbids it from doing yet.
    """
    scope = str(package.get("scope") or "")
    match = re.fullmatch(r"tranche-(\d+)", scope)
    if str(package.get("artefact") or "") == "tranche-review":
        number: int | None = int(match.group(1)) if match else None
        if number is None:
            named = re.search(r"tranche-review-tranche-(\d+)", str(package.get("key") or ""))
            number = int(named.group(1)) if named else None
        if number is not None:
            return states.get(number) == "closed"
    if not planned:
        return True
    if not match:
        return True
    return int(match.group(1)) not in planned


def _gate_owner(package: dict[str, Any]) -> str:
    """The project whose manifest gates this package.

    A project-scope package is the project's own obligation and gates the subject the audit
    scopes it to. `owner_project` names the accountable standard owner and `apply_to` names
    where a shared card is created; neither may move a subject's package onto another project's
    manifest (that defect put every subject's `mandatory_artefacts:*` card on the standard
    owner's manifest). A tranche- or programme-scope package is shared work: `apply_to` is the
    project that must act, and gating the acting project is what makes the tranche pipeline
    executable. The accountable role stays on the package for reporting, and the final
    acceptance still sees every package.
    """
    scope = str(package.get("scope") or "")
    if scope == "programme" or scope.startswith("tranche-"):
        return str(package.get("apply_to") or package.get("owner_project") or "")
    return scope or str(package.get("owner_project") or "")


def check_artefact_standard(
    snapshot: Snapshot, key: str, params: dict[str, Any]
) -> tuple[bool, str]:
    """Every mandatory standard artefact this project owns is evidenced.

    A standard package (TOGAF, PMP, MSP, SAFe) is cleared only when its gap disappears from the
    fresh audit; severity does not exempt it and a suppressed package counts as open (ADR 0012).
    The owner may be named by project key or by project id.
    """
    register, error = _register(snapshot)
    if error:
        return False, error
    row = snapshot.project(key)
    owners = {key, str(row["id"])}
    planned = _planned_tranches(snapshot)
    states = _tranche_states(snapshot)
    open_keys: list[str] = []
    not_due: list[str] = []
    for package in register["packages"]:
        if (
            package.get("standard") not in STANDARD_ARTEFACT_STANDARDS
            or package.get("status") not in UNRESOLVED_PACKAGE_STATES
            or _gate_owner(package) not in owners
        ):
            continue
        target = open_keys if _package_due(package, planned, states) else not_due
        target.append(str(package.get("key")))
    if open_keys:
        preview = ", ".join(open_keys[:8])
        more = "" if len(open_keys) <= 8 else f", +{len(open_keys) - 8} more"
        suffix = f" ({len(not_due)} not yet due: tranche planned)" if not_due else ""
        return False, f"{len(open_keys)} mandatory artefact package(s) open: {preview}{more}{suffix}"
    if not_due:
        return True, (
            f"no due mandatory artefact package; {len(not_due)} not yet due (tranche planned)"
        )
    return True, "no open standard artefact package for this project"


def check_rework_resolution(
    snapshot: Snapshot, key: str, params: dict[str, Any]
) -> tuple[bool, str]:
    """Fail every unresolved P0/P1 rework package.

    An open P2 is advisory and passes. No disposition — waiver, deferral, suppression or a
    dated residual — resolves a P0/P1 package (ADR 0012): it closes when the work lands.
    """
    register, error = _register(snapshot)
    if error:
        return False, error
    row = snapshot.project(key)
    owners = {key, str(row["id"])}
    planned = _planned_tranches(snapshot)
    states = _tranche_states(snapshot)
    blocking: list[str] = []
    not_due: list[str] = []
    for package in register["packages"]:
        # The register lists unresolved packages; only an explicit resolution clears one.
        if package.get("status") in ("resolved", "closed"):
            continue
        if _gate_owner(package) not in owners:
            continue
        severity = str(package.get("severity"))
        if severity == "P2":
            continue
        entry = f"{package.get('key')} ({severity})"
        (blocking if _package_due(package, planned, states) else not_due).append(entry)
    if blocking:
        preview = ", ".join(blocking[:8])
        more = "" if len(blocking) <= 8 else f", +{len(blocking) - 8} more"
        suffix = f" ({len(not_due)} not yet due: tranche planned)" if not_due else ""
        return False, f"{len(blocking)} unresolved P0/P1 package(s): {preview}{more}{suffix}"
    if not_due:
        return True, (
            f"no due unresolved P0/P1 package; {len(not_due)} not yet due (tranche planned)"
        )
    return True, "no unresolved P0/P1 package for this project"


def check_delegation_provenance(
    snapshot: Snapshot, key: str, params: dict[str, Any]
) -> tuple[bool, str]:
    """Every approval the project relies on is a rule decision under a published delegation.

    A historical `service` decision is tolerated only when the project's accepted provenance
    residual (a `change` register item with `meta.provenance_residual`) names its approval id.
    """
    row = snapshot.project(key)
    work = snapshot.work or {}
    approvals = [
        approval
        for approval in work.get("approvals") or []
        if approval.get("project_id") == row["id"] and approval.get("status") == "approved"
    ]
    by_approval: dict[str, list[dict[str, Any]]] = {}
    for decision in work.get("approval_decisions") or []:
        by_approval.setdefault(str(decision.get("approval_id")), []).append(decision)
    residual: set[str] = set()
    for item in snapshot.msp.get("register_items") or []:
        if (
            item.get("project_id") == row["id"]
            and item.get("kind") == "change"
            and str(item.get("status")) == "accepted"
        ):
            meta = item.get("meta") or {}
            for ref in meta.get("provenance_residual") or []:
                residual.add(str(ref))
    rule_seen = 0
    exempt = 0
    offending: list[str] = []
    for approval in approvals:
        approval_id = str(approval.get("id"))
        decisions = by_approval.get(approval_id, [])
        lawful = False
        for decision in decisions:
            via = decision.get("via")
            if (
                decision.get("decision") == "approve"
                and decision.get("decided_by_kind") == "rule"
                and isinstance(via, dict)
                and via.get("delegation_ref")
                and snapshot.published_id(str(via["delegation_ref"]))
            ):
                lawful = True
                break
        if lawful:
            rule_seen += 1
            continue
        if approval_id in residual:
            exempt += 1
            continue
        offending.append(f"{approval_id} ({approval.get('gate')})")
    if offending:
        preview = ", ".join(offending[:6])
        more = "" if len(offending) <= 6 else f", +{len(offending) - 6} more"
        return False, (
            f"{len(offending)} approval(s) not decided as a rule under a published delegation: "
            f"{preview}{more}"
        )
    return True, f"{rule_seen} approval(s) decided as rule under a published delegation ({exempt} exempt)"


def check_published_proof(
    snapshot: Snapshot, key: str, params: dict[str, Any]
) -> tuple[bool, str]:
    """The snapshot's output index is whole, and the project's closure outputs are in it.

    Every indexed output's file must exist under the snapshot's docs root (a broken export
    manifest is a red check, never a skipped one), and every declared closure output must be an
    indexed output of THIS project with its file present.
    """
    row = snapshot.project(key)
    docs_root = snapshot.path.parent
    outputs = snapshot.outputs
    broken = []
    for output in outputs:
        path = output.get("path")
        if not path or not (docs_root / str(path)).exists():
            broken.append(str(output.get("id")))
    if broken:
        return False, f"{len(broken)} indexed output(s) carry no file under docs/: " + ", ".join(broken[:6])
    categories = (row.get("meta") or {}).get("closure_outputs") or []
    if not categories:
        return True, f"{len(outputs)} indexed outputs; no closure outputs declared"
    missing = [
        category
        for category in categories
        if not any(
            output.get("project_id") == row["id"]
            and output.get("category") == category
            and output.get("path")
            and (docs_root / str(output["path"])).exists()
            for output in outputs
        )
    ]
    if missing:
        return False, "declared closure outputs are not published: " + ", ".join(missing)
    return True, (
        f"{len(outputs)} indexed outputs; {len(categories)} closure output(s) published with their file"
    )


CHECKS: dict[str, Callable[[Snapshot, str, dict[str, Any]], tuple[bool, str]]] = {
    "project": check_project,
    "project_baseline": check_project_baseline,
    "closure_outputs": check_closure_outputs,
    "contract": check_contract,
    "review": check_review,
    "closure_gate": check_closure_gate,
    "waivers": check_waivers,
    "benefit_baseline": check_benefit_baseline,
    "pi_cadence": check_pi_cadence,
    "pi_plan": check_pi_plan,
    "pi_objectives": check_pi_objectives,
    "wsjf": check_wsjf,
    "demo_credits": check_demo_credits,
    "lessons": check_lessons,
    "product_findings": check_product_findings,
    "artefact_standard": check_artefact_standard,
    "rework_resolution": check_rework_resolution,
    "delegation_provenance": check_delegation_provenance,
    "published_proof": check_published_proof,
}


def run_command(command: str, key: str, snapshot: Path) -> tuple[bool, str, int, str, str]:
    env = dict(os.environ)
    env.update(
        {
            "VERIFY_PROJECT": key,
            "VERIFY_SNAPSHOT": str(snapshot),
            "VERIFY_ROOT": str(ROOT),
        }
    )
    try:
        completed = subprocess.run(
            command,
            shell=True,
            cwd=ROOT,
            env=env,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=300,
        )
    except subprocess.TimeoutExpired:
        return False, "the command timed out after 300s", 124, "", ""
    stdout = completed.stdout[-4000:]
    stderr = completed.stderr[-4000:]
    if completed.returncode != 0:
        detail = (stderr.strip() or stdout.strip() or "no output")[-500:]
        return False, f"exit {completed.returncode}: {detail}", completed.returncode, stdout, stderr
    return True, "exit 0", 0, stdout, stderr


def load_manifest(path: Path) -> dict[str, Any]:
    try:
        return tomllib.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        fail(f"{path} is missing")
    except tomllib.TOMLDecodeError as error:
        fail(f"{path} is not TOML: {error}")


def snapshot_label(path: Path, manifest: dict[str, Any]) -> str:
    """Where the evidence says the snapshot was: repo-relative, or a retained one named by SHA.

    A snapshot built outside the repository (the live batch keeps its retained directory under
    the operator's home) must not leak an absolute path into a committed or published artefact.
    """
    try:
        return str(path.resolve().relative_to(ROOT))
    except ValueError:
        return f"retained-snapshot/{manifest.get('git_sha') or 'unknown'}"


def gate_problem(
    manifest: dict[str, Any], row: dict[str, Any], checks: list[dict[str, Any]]
) -> tuple[bool, str | None]:
    """Whether the gate is required, and why it is not discharged by executable checks.

    An active or closed project's `[project].gate` is prose; `gate_checks` names the check ids
    that discharge it. Each id must resolve to a check in the same manifest, and that check
    must have run and passed in this verdict. A gate with no `gate_checks` names no executable
    checks and therefore fails.
    """
    if str(row.get("status")) not in OPEN_PROJECT_STATES:
        return False, None
    declared = manifest.get("project", {})
    gate_checks = declared.get("gate_checks")
    if not isinstance(gate_checks, list) or not gate_checks:
        return True, "the gate names no executable checks"
    declared_ids = {str(check.get("id")) for check in manifest.get("check", [])}
    ran = {str(record.get("id")): record for record in checks}
    problems: list[str] = []
    for raw in gate_checks:
        identifier = str(raw)
        if identifier not in declared_ids:
            problems.append(f"the gate check {identifier!r} resolves to no check in the manifest")
        elif identifier not in ran:
            problems.append(f"the gate check {identifier!r} did not run")
        elif ran[identifier].get("result") != "pass":
            problems.append(f"the gate check {identifier!r} did not pass")
    if problems:
        return True, "; ".join(problems)
    return True, None


def missing_project_evidence(
    key: str,
    snapshot: "Snapshot",
    snapshot_dir: Path,
    manifest_path: Path,
    evidence_dir: Path,
    kind_filter: str,
) -> dict[str, Any]:
    """A declared project with no snapshot row is a red project, not a stopped board.

    `--all` must reach every manifest: one absent project row (a project created on main but
    not yet in the restored copy) previously raised out of `Snapshot.project` and ended the
    whole run, silently dropping every manifest after it. The board records the miss as a red
    evidence row and continues, so the count of reds is the truth and no project disappears.
    """
    detail = f"the snapshot has no project {key!r}"
    checks = [
        {
            "id": f"{key}-project",
            "check": None,
            "kind": "snapshot",
            "severity": "P0",
            "description": "the snapshot carries the project",
            "result": "fail",
            "detail": detail,
        }
    ]
    evidence: dict[str, Any] = {
        "schema": "project-acceptance/v1",
        "project": key,
        "name": None,
        "gate": None,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "snapshot": {
            "path": snapshot_label(snapshot_dir, snapshot.manifest),
            "git_sha": snapshot.manifest.get("git_sha"),
            "generated_at": snapshot.manifest.get("generated_at"),
        },
        "manifest": str(manifest_path.relative_to(ROOT))
        if manifest_path.is_relative_to(ROOT)
        else str(manifest_path),
        "kind": kind_filter,
        "checks": checks,
        "failures": [f"{key}-project (P0): {detail}"],
        "warnings": [],
        "verdict": "red",
    }
    evidence_dir.mkdir(parents=True, exist_ok=True)
    (evidence_dir / f"{key}.json").write_text(
        json.dumps(evidence, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return evidence


def run_project(
    key: str,
    snapshot_dir: Path,
    manifest_path: Path,
    evidence_dir: Path,
    kind_filter: str = "all",
    quiet: bool = False,
    exclude: set[str] | None = None,
) -> dict[str, Any]:
    snapshot = Snapshot(snapshot_dir)
    row = snapshot.project(key)
    manifest = load_manifest(manifest_path)
    declared = manifest.get("project", {})
    if str(declared.get("key", key)) != key:
        fail(f"the manifest names {declared.get('key')!r}, not {key!r}")
    disallowed = manifest.get("allow") or []
    excluded = exclude or set()
    checks: list[dict[str, Any]] = []
    failures: list[str] = []
    warnings: list[str] = []
    # A `[[allow]]` block is refused as a finding, never applied: no deferral, waiver or
    # residual is a disposition for programme work (ADR 0012). The named check must execute.
    if disallowed:
        detail = (
            f"{len(disallowed)} [[allow]] block(s) in the manifest; programme work permits "
            "no allow — execute the check"
        )
        checks.append(
            {
                "id": "allow",
                "check": None,
                "kind": "manifest",
                "severity": "P0",
                "description": "no [[allow]] block is permitted in an acceptance manifest",
                "result": "fail",
                "detail": detail,
            }
        )
        failures.append(f"allow (P0): {detail}")
        if not quiet:
            print(f"FAIL  allow  {detail}")
    # An active project cannot gate on closure: the rework factory records no closure for a
    # project it keeps active, so such a manifest is unsatisfiable by construction (measured
    # 2026-11-24: 16 manifests, 15 red). A project that truly closes declares status = "closed".
    closure_gated = [
        str(check.get("id") or "?")
        for check in manifest.get("check", [])
        if str(check.get("check") or "") in {"closure_outputs", "closure_gate"}
    ]
    declared_status = next(
        (
            str(check.get("status") or "")
            for check in manifest.get("check", [])
            if str(check.get("check") or "") == "project"
        ),
        "",
    )
    if declared_status == "active" and closure_gated:
        detail = (
            "the manifest declares an active project but gates on "
            f"{', '.join(closure_gated)}; an active project records no closure — "
            'declare status = "closed" or drop the closure checks'
        )
        checks.append(
            {
                "id": "manifest-status",
                "check": None,
                "kind": "manifest",
                "severity": "P0",
                "description": "an active project does not gate on closure",
                "result": "fail",
                "detail": detail,
            }
        )
        failures.append(f"manifest-status (P0): {detail}")
        if not quiet:
            print(f"FAIL  manifest-status  {detail}")
    for index, check in enumerate(manifest.get("check", []), start=1):
        identifier = str(check.get("id") or f"check-{index}")
        name = str(check.get("check") or "")
        kind = str(check.get("kind") or ("command" if "command" in check else "snapshot"))
        if kind_filter != "all" and kind != kind_filter:
            continue
        if identifier in excluded or name in excluded:
            continue
        severity = str(check.get("severity") or "P0").upper()
        if severity not in SEVERITY:
            fail(f"{identifier}: severity must be one of {', '.join(SEVERITY)}")
        description = str(check.get("description") or "")
        record: dict[str, Any] = {
            "id": identifier,
            "check": name or None,
            "kind": kind,
            "severity": severity,
            "description": description,
        }
        if kind in ("command", "probe"):
            command = str(check.get("command") or "")
            if not command:
                fail(f"{identifier}: a command check needs `command`")
            ok, detail, exit_code, stdout, stderr = run_command(command, key, snapshot_dir)
            record.update(
                {"command": command, "exit": exit_code, "stdout": stdout, "stderr": stderr}
            )
        else:
            if kind != "snapshot":
                fail(f"{identifier}: kind must be snapshot, command or probe")
            if name not in CHECKS:
                fail(f"{identifier}: no shared check named {name!r}")
            ok, detail = CHECKS[name](snapshot, key, check)
        record["result"] = "pass" if ok else "fail"
        record["detail"] = detail
        checks.append(record)
        if not ok:
            target = failures if severity in ("P0", "P1") else warnings
            target.append(f"{identifier} ({severity}): {detail}")
        if not quiet:
            print(f"{'PASS' if ok else 'FAIL'}  {identifier}  {detail}")
    required, gate_detail = gate_problem(manifest, row, checks)
    if required and gate_detail:
        checks.append(
            {
                "id": "gate",
                "check": None,
                "kind": "manifest",
                "severity": "P0",
                "description": "the declared gate is discharged by executable checks",
                "result": "fail",
                "detail": gate_detail,
            }
        )
        failures.append(f"gate (P0): {gate_detail}")
        if not quiet:
            print(f"FAIL  gate  {gate_detail}")
    elif required:
        discharged = ", ".join(str(item) for item in declared.get("gate_checks") or [])
        checks.append(
            {
                "id": "gate",
                "check": None,
                "kind": "manifest",
                "severity": "P0",
                "description": "the declared gate is discharged by executable checks",
                "result": "pass",
                "detail": f"gate discharged by executable checks: {discharged}",
            }
        )
    verdict = "red" if failures else "green"
    if row.get("status") == "closed" and verdict == "red":
        checks.append(
            {
                "id": "closed-on-red",
                "check": None,
                "kind": "snapshot",
                "severity": "P0",
                "description": "a closed project must not carry a red acceptance manifest",
                "result": "fail",
                "detail": "the project is closed while its manifest is red",
            }
        )
        failures.append("closed-on-red (P0): the project is closed on a red manifest")
    evidence = {
        "schema": "project-acceptance/v1",
        "project": key,
        "name": declared.get("name"),
        "gate": declared.get("gate"),
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "snapshot": {
            "path": snapshot_label(snapshot_dir, snapshot.manifest),
            "git_sha": snapshot.manifest.get("git_sha"),
            "generated_at": snapshot.manifest.get("generated_at"),
        },
        "manifest": str(manifest_path.relative_to(ROOT)) if manifest_path.is_relative_to(ROOT) else str(manifest_path),
        "kind": kind_filter,
        "checks": checks,
        "failures": failures,
        "warnings": warnings,
        "verdict": verdict,
    }
    evidence_dir.mkdir(parents=True, exist_ok=True)
    (evidence_dir / f"{key}.json").write_text(
        json.dumps(evidence, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return evidence


def self_test() -> int:
    # A placeholder is a token, never a substring: `n/?a` inside `tenant` is an ordinary word,
    # and flagging it refused a real demo credit carrying `the_tenant_policy_sets_the_quorum`
    # (measured 2026-11-24).
    if PLACEHOLDER.search("a destructive call on the tenant is refused"):
        raise AssertionError("PLACEHOLDER matches inside 'tenant'")
    if not PLACEHOLDER.search("TBD"):
        raise AssertionError("PLACEHOLDER misses a standalone TBD")
    workdir = Path(tempfile.mkdtemp(prefix="verify-project-selftest-"))
    snapshot = workdir / "snapshot"
    (snapshot / "data").mkdir(parents=True)
    (snapshot / "docs").mkdir()
    (snapshot / "data" / "programme.json").write_text(
        json.dumps(
            {
                "projects": [
                    {
                        "key": "TEST",
                        "id": "project-1",
                        "programme_id": "prog-1",
                        "status": "closed",
                        "meta": {"closure_outputs": [], "tranche": 4},
                    }
                ]
            }
        )
    )
    (snapshot / "data" / "architecture.json").write_text(
        json.dumps({"contracts": [], "compliance_reviews": [], "waivers": []})
    )
    (snapshot / "data" / "work.json").write_text(
        json.dumps({"approvals": [], "approval_decisions": []})
    )
    (snapshot / "data" / "msp.json").write_text(
        json.dumps(
            {
                "benefit_profiles": [
                    {
                        "id": "bp1",
                        "baseline": {
                            "value": 5,
                            "unit": "decisions/week",
                            "measured_at": "2026-11-01",
                            "method": "action ledger count",
                            "evidence": "o1",
                        },
                    }
                ],
                "tranches": [{"id": "tr-4", "number": 4, "state": "closed"}],
                "cadences": [
                    {"id": "c-pi", "programme_id": "prog-1", "kind": "pi",
                     "period": "PI-2027.1", "meta": {"tranche_id": "tr-4"}},
                    {"id": "c-ref", "programme_id": "prog-1", "kind": "refinement",
                     "period": "PI-2027.1", "meta": {"tranche_id": "tr-4"}},
                    {"id": "c-demo", "programme_id": "prog-1", "kind": "demo",
                     "period": "PI-2027.1", "meta": {"tranche_id": "tr-4"}},
                    {"id": "c-retro", "programme_id": "prog-1", "kind": "retro",
                     "period": "PI-2027.1", "meta": {"tranche_id": "tr-4"}},
                ],
                "wbs_nodes": [
                    {"id": "w1", "project_id": "project-1", "kind": "deliverable",
                     "status": "accepted", "business_value": 5, "time_criticality": 3,
                     "risk_reduction": 2, "job_size": 2}
                ],
                "demo_credits": [
                    {"id": "dc1", "cadence_id": "c-demo", "deliverable_id": "w1",
                     "evidence": ["manifest-run"]}
                ],
                "lessons": [
                    {"id": "l1", "cadence_id": "c-retro",
                     "statement": "the plan preceded the demo",
                     "evidence": ["manifest-run"]}
                ],
            }
        )
    )
    (snapshot / "data" / "artefacts.json").write_text(json.dumps({"documents": []}))
    (snapshot / "data" / "manifest.json").write_text(
        json.dumps({"git_sha": "self-test", "generated_at": "2026-11-21T00:00:00Z"})
    )
    (snapshot / "docs" / "outputs.js").write_text(
        "window.CONVEYOR_OUTPUTS = "
        + json.dumps(
            {
                "schema": "v1",
                "outputs": [
                    {"id": "o1", "project_key": "P0", "category": "pi-plan",
                     "title": "PI plan — tranche 4", "unit": "tranche-4",
                     "published_at": "2026-11-01T00:00:00+00:00"},
                    {"id": "o2", "project_key": "P0", "category": "pi-objectives",
                     "title": "PI objectives — tranche 4", "unit": "tranche-4",
                     "published_at": "2026-11-01T00:00:00+00:00"},
                ],
            }
        )
        + ";\n"
    )
    manifest = workdir / "TEST.toml"
    manifest.write_text(
        """
[project]
key = "TEST"
name = "Self-test project"
gate = "a closed project has published closure outputs"
gate_checks = ["project", "outputs"]

[[check]]
id = "project"
check = "project"
severity = "P0"

[[check]]
id = "outputs"
check = "closure_outputs"
severity = "P0"
"""
    )
    failures: list[str] = []

    red = run_project(
        "TEST", snapshot / "data", manifest, workdir / "evidence", quiet=True
    )
    if red["verdict"] != "red":
        failures.append("a closed project with no closure outputs was not red")
    if not any(row.get("id") == "closed-on-red" for row in red["checks"]):
        failures.append("the closed-on-red finding was not recorded")

    # A [[allow]] is refused as a finding, never applied: the failing check stays red and the
    # refusal is visible in the evidence.
    allow_manifest = workdir / "TEST-allow.toml"
    allow_manifest.write_text(
        manifest.read_text()
        + """
[[allow]]
check = "outputs"
reason = "self-test residual"
owner = "programme owner"
issue = "U6"
expires = "2027-02-18"
"""
    )
    refused = run_project(
        "TEST", snapshot / "data", allow_manifest, workdir / "evidence", quiet=True
    )
    if refused["verdict"] != "red":
        failures.append("an [[allow]] block cleared the verdict")
    if not any(
        row.get("id") == "allow" and row.get("result") == "fail"
        for row in refused["checks"]
    ):
        failures.append("the refused [[allow]] block was not recorded as a finding")

    # An active project that gates on closure is refused as a manifest error: the factory never
    # records a closure for a project it keeps active, so such a manifest is unsatisfiable.
    active_manifest = workdir / "TEST-active-closure.toml"
    active_manifest.write_text(
        manifest.read_text().replace(
            '[[check]]\nid = "project"\ncheck = "project"',
            '[[check]]\nid = "project"\ncheck = "project"\nstatus = "active"',
        )
    )
    active_refused = run_project(
        "TEST", snapshot / "data", active_manifest, workdir / "evidence", quiet=True
    )
    if active_refused["verdict"] != "red":
        failures.append("an active manifest gating on closure cleared the verdict")
    if not any(
        row.get("id") == "manifest-status" and row.get("result") == "fail"
        for row in active_refused["checks"]
    ):
        failures.append("the active/closure contradiction was not recorded as a finding")

    # The gate contract: an active or closed project's gate is discharged only by named
    # checks in the same manifest that run and pass.
    def gate_case(name: str, body: str, expect_green: bool, detail: str) -> None:
        path = workdir / f"TEST-gate-{name}.toml"
        path.write_text(body)
        result = run_project(
            "TEST", snapshot / "data", path, workdir / f"evidence-gate-{name}", quiet=True
        )
        if (result["verdict"] == "green") != expect_green:
            failures.append(
                f"gate case {name}: verdict {result['verdict']}, "
                f"expected {'green' if expect_green else 'red'}"
            )
        if detail and not any(
            detail in str(row.get("detail") or "") for row in result["checks"]
        ):
            failures.append(f"gate case {name}: no check detail names {detail!r}")

    gate_case(
        "missing",
        """
[project]
key = "TEST"
name = "Self-test gate missing"
gate = "a gate with no executable checks"

[[check]]
id = "project"
check = "project"
severity = "P0"
""",
        False,
        "the gate names no executable checks",
    )
    gate_case(
        "unknown",
        """
[project]
key = "TEST"
name = "Self-test gate unknown"
gate = "a gate naming a check that does not exist"
gate_checks = ["missing-check"]

[[check]]
id = "project"
check = "project"
severity = "P0"
""",
        False,
        "resolves to no check in the manifest",
    )
    gate_case(
        "passing",
        """
[project]
key = "TEST"
name = "Self-test gate passing"
gate = "a gate discharged by one executable check"
gate_checks = ["ok"]

[[check]]
id = "ok"
kind = "command"
command = "python3 -c 'import sys; sys.exit(0)'"
severity = "P0"
""",
        True,
        "gate discharged by executable checks: ok",
    )
    gate_case(
        "failing",
        """
[project]
key = "TEST"
name = "Self-test gate failing"
gate = "a gate whose check fails"
gate_checks = ["bad"]

[[check]]
id = "bad"
kind = "command"
command = "python3 -c 'import sys; sys.exit(3)'"
severity = "P0"
""",
        False,
        "did not pass",
    )

    command_manifest = workdir / "TEST-command.toml"
    command_manifest.write_text(
        """
[project]
key = "TEST"
name = "Self-test project"
gate = "a command check"
gate_checks = ["ok"]

[[check]]
id = "ok"
kind = "command"
command = "python3 -c 'import os,sys; sys.exit(0 if os.environ.get(\\"VERIFY_PROJECT\\") == \\"TEST\\" else 1)'"
severity = "P0"

[[check]]
id = "bad"
kind = "command"
command = "python3 -c 'import sys; sys.exit(3)'"
severity = "P2"
"""
    )
    commands = run_project(
        "TEST", snapshot / "data", command_manifest, workdir / "evidence", quiet=True
    )
    if commands["verdict"] != "green":
        failures.append("a failing P2 command made the verdict red")
    if not commands["warnings"]:
        failures.append("the failing P2 command was not recorded as a warning")
    bad = [row for row in commands["checks"] if row["id"] == "bad"]
    if not bad or bad[0]["exit"] != 3:
        failures.append("the failing command's exit was not captured")

    # ── the SAFe checks: one green case and one red case per control ──────────────────
    safe_manifest = workdir / "TEST-safe.toml"
    safe_manifest.write_text(
        """
[project]
key = "TEST"
name = "Self-test SAFe"
gate = "the SAFe cadence, plan, demo and retro"
gate_checks = ["pi-cadence", "benefit-baseline", "wsjf", "demo", "lessons", "pi-plan", "pi-objectives"]

[[check]]
id = "pi-cadence"
check = "pi_cadence"
severity = "P0"

[[check]]
id = "benefit-baseline"
check = "benefit_baseline"
severity = "P0"

[[check]]
id = "wsjf"
check = "wsjf"
severity = "P0"

[[check]]
id = "demo"
check = "demo_credits"
severity = "P0"

[[check]]
id = "lessons"
check = "lessons"
severity = "P0"

[[check]]
id = "pi-plan"
check = "pi_plan"
severity = "P0"

[[check]]
id = "pi-objectives"
check = "pi_objectives"
severity = "P0"
"""
    )
    base_msp = json.loads((snapshot / "data" / "msp.json").read_text())
    base_outputs = json.loads(
        (snapshot / "docs" / "outputs.js").read_text().split("=", 1)[1].strip().rstrip(";")
    )["outputs"]

    def safe_case(name: str, msp: dict, outputs: list, expect_green: bool) -> None:
        case = workdir / f"safe-{name}"
        (case / "data").mkdir(parents=True)
        (case / "docs").mkdir()
        for filename in (
            "programme.json", "architecture.json", "work.json", "artefacts.json",
            "manifest.json",
        ):
            shutil.copy(snapshot / "data" / filename, case / "data" / filename)
        (case / "data" / "msp.json").write_text(json.dumps(msp))
        (case / "docs" / "outputs.js").write_text(
            "window.CONVEYOR_OUTPUTS = "
            + json.dumps({"schema": "v1", "outputs": outputs})
            + ";\n"
        )
        result = run_project(
            "TEST", case / "data", safe_manifest, workdir / f"evidence-{name}", quiet=True
        )
        if (result["verdict"] == "green") != expect_green:
            failures.append(
                f"SAFe case {name}: verdict {result['verdict']}, "
                f"expected {'green' if expect_green else 'red'}"
            )

    safe_case("ok", base_msp, base_outputs, True)
    zero_baseline = copy.deepcopy(base_msp)
    zero_baseline["benefit_profiles"][0]["baseline"]["value"] = 0
    safe_case("zero-baseline", zero_baseline, copy.deepcopy(base_outputs), True)

    def mutate(name: str, change) -> None:
        msp = copy.deepcopy(base_msp)
        outputs = copy.deepcopy(base_outputs)
        change(msp, outputs)
        safe_case(name, msp, outputs, False)

    mutate(
        "no-pi-cadence",
        lambda m, o: m.__setitem__("cadences", [c for c in m["cadences"] if c["id"] != "c-pi"]),
    )
    mutate(
        "no-refinement",
        lambda m, o: m.__setitem__("cadences", [c for c in m["cadences"] if c["id"] != "c-ref"]),
    )
    mutate(
        "zero-wsjf",
        lambda m, o: m["wbs_nodes"][0].update(
            {"business_value": 0, "time_criticality": 0, "risk_reduction": 0}
        ),
    )
    mutate("no-credit", lambda m, o: m.__setitem__("demo_credits", []))
    mutate("no-lesson", lambda m, o: m.__setitem__("lessons", []))
    mutate(
        "no-plan",
        lambda m, o: o.__setitem__(
            slice(None), [row for row in o if row["category"] != "pi-plan"]
        ),
    )
    mutate(
        "no-objectives",
        lambda m, o: o.__setitem__(
            slice(None), [row for row in o if row["category"] != "pi-objectives"]
        ),
    )

    # ── the tightened manifest rules: each crafted case greens under the old rule and reds now ─
    mutate(
        "benefit-string-baseline",
        lambda m, o: m["benefit_profiles"][0].__setitem__(
            "baseline", "five decisions a week, counted from the action ledger"
        ),
    )
    mutate(
        "wsjf-missing-input",
        lambda m, o: m["wbs_nodes"][0].pop("risk_reduction"),
    )
    mutate(
        "credit-placeholder-evidence",
        lambda m, o: m["demo_credits"][0].__setitem__("evidence", "TBD"),
    )

    def draft_other_tranche(msp, outputs) -> None:
        plan = next(row for row in outputs if row["category"] == "pi-plan")
        plan["title"] = "PI plan — tranche 40"
        plan["unit"] = "tranche-40"
        plan.pop("published_at", None)

    mutate("pi-plan-draft-tranche-40", draft_other_tranche)

    # product_findings reads a derived register; craft the register directly so the rule is
    # tested at its exact shapes without building one from the synthetic snapshot.
    product_manifest = workdir / "TEST-product.toml"
    product_manifest.write_text(
        """
[project]
key = "TEST"
name = "Self-test product findings"
gate = "no open P0 product finding remains for the project"
gate_checks = ["product"]

[[check]]
id = "product"
check = "product_findings"
severity = "P0"
"""
    )

    def product_case(name: str, register: Any, expect_green: bool) -> None:
        original = globals()["rework_register"]
        globals()["rework_register"] = lambda snapshot: (copy.deepcopy(register), None)
        try:
            result = run_project(
                "TEST", snapshot / "data", product_manifest,
                workdir / f"evidence-product-{name}", quiet=True,
            )
        finally:
            globals()["rework_register"] = original
        if (result["verdict"] == "green") != expect_green:
            failures.append(
                f"product case {name}: verdict {result['verdict']}, "
                f"expected {'green' if expect_green else 'red'}"
            )

    product_case(
        "ok",
        {
            "packages": [
                {"key": "p1", "status": "open", "severity": "P1", "owner_project": "TEST"},
                {"key": "p2", "status": "open", "severity": "P0", "owner_project": "OTHER"},
            ]
        },
        True,
    )
    product_case("no-packages", {}, False)
    product_case(
        "suppressed-p0",
        {
            "packages": [
                {"key": "p1", "status": "suppressed", "severity": "P0", "owner_project": "TEST"}
            ]
        },
        False,
    )

    # ── the hardened rules: every crafted case passes under the old rule and fails now ─
    hardening_manifest = workdir / "TEST-hardening.toml"
    hardening_manifest.write_text(
        """
[project]
key = "TEST"
name = "Self-test hardening"
gate = "each check proves its TOML description"
gate_checks = ["baseline", "contract", "review", "closure-gate", "lessons"]

[[check]]
id = "baseline"
check = "project_baseline"
severity = "P0"
require_approved = true

[[check]]
id = "contract"
check = "contract"
severity = "P0"

[[check]]
id = "review"
check = "review"
severity = "P0"

[[check]]
id = "closure-gate"
check = "closure_gate"
severity = "P0"

[[check]]
id = "lessons"
check = "lessons"
severity = "P0"
"""
    )
    hard_arch = {
        "contracts": [
            {"id": "ct-live", "project_id": "project-1", "status": "live",
             "approval_id": "ap-contract", "requested_by": "u-requester",
             "approved_by": "u-approver"},
            {"id": "ct-old", "project_id": "project-1", "status": "superseded",
             "approval_id": "ap-contract-old", "requested_by": "u-requester",
             "approved_by": "u-approver"},
        ],
        "compliance_reviews": [
            {"id": "rv-live", "project_id": "project-1", "contract_id": "ct-live",
             "level": "compliant", "evidence": {"delegation_ref": "o-review"}},
        ],
        "waivers": [],
    }
    hard_work = {
        "approvals": [
            {"id": "ap-contract", "project_id": "project-1", "gate": "architecture_board",
             "status": "approved", "requested_by": "u-requester",
             "meta": {"subject_kind": "contract", "subject_id": "ct-live"},
             "created_at": "2026-11-01T00:00:00+00:00"},
            {"id": "ap-contract-old", "project_id": "project-1", "gate": "contract",
             "status": "approved", "requested_by": "u-requester",
             "created_at": "2026-11-01T00:00:00+00:00"},
            {"id": "ap-close-1", "project_id": "project-1", "gate": "closure",
             "status": "approved", "requested_by": "u-requester",
             "created_at": "2026-11-02T00:00:00+00:00"},
        ],
        "approval_decisions": [
            {"approval_id": "ap-contract", "decision": "approve",
             "approver_id": "u-approver"},
            {"approval_id": "ap-contract-old", "decision": "approve",
             "approver_id": "u-approver"},
            {"approval_id": "ap-close-1", "decision": "approve",
             "approver_id": "u-approver"},
        ],
    }
    hard_msp = copy.deepcopy(base_msp)
    hard_msp["project_baselines"] = [
        {"id": "b1", "project_id": "project-1", "kind": "scope",
         "taken_by": "u-taker", "approved_by": "u-approver",
         "snapshot": {"frozen": True}}
    ]
    hard_outputs = copy.deepcopy(base_outputs) + [
        {"id": "o-review", "project_key": "P0", "category": "closure-review",
         "title": "close review", "unit": "tranche-4",
         "published_at": "2026-11-01T00:00:00+00:00"}
    ]

    def hardening_case(name: str, change, expect_green: bool) -> None:
        case = workdir / f"hardening-{name}"
        (case / "data").mkdir(parents=True)
        (case / "docs").mkdir()
        for filename in ("programme.json", "artefacts.json", "manifest.json"):
            shutil.copy(snapshot / "data" / filename, case / "data" / filename)
        arch = copy.deepcopy(hard_arch)
        work = copy.deepcopy(hard_work)
        msp = copy.deepcopy(hard_msp)
        outputs = copy.deepcopy(hard_outputs)
        change(arch, work, msp, outputs)
        (case / "data" / "architecture.json").write_text(json.dumps(arch))
        (case / "data" / "work.json").write_text(json.dumps(work))
        (case / "data" / "msp.json").write_text(json.dumps(msp))
        (case / "docs" / "outputs.js").write_text(
            "window.CONVEYOR_OUTPUTS = "
            + json.dumps({"schema": "v1", "outputs": outputs})
            + ";\n"
        )
        result = run_project(
            "TEST", case / "data", hardening_manifest,
            workdir / f"evidence-hardening-{name}", quiet=True,
        )
        if (result["verdict"] == "green") != expect_green:
            failures.append(
                f"hardening case {name}: verdict {result['verdict']}, "
                f"expected {'green' if expect_green else 'red'}"
            )

    hardening_case("ok", lambda a, w, m, o: None, True)
    hardening_case(
        "baseline-self-approved",
        lambda a, w, m, o: m["project_baselines"][0].update({"approved_by": "u-taker"}),
        False,
    )
    # A board decision is an approved approval row about THIS contract; a decision row is not
    # written by decide_board_tx. The red cases are a decision about another subject and a
    # contract whose approver is its requester.
    hardening_case(
        "contract-wrong-subject",
        lambda a, w, m, o: w["approvals"][0]["meta"].__setitem__("subject_id", "ct-other"),
        False,
    )
    hardening_case(
        "contract-self-approved",
        lambda a, w, m, o: a["contracts"][0].__setitem__("approved_by", "u-requester"),
        False,
    )
    hardening_case(
        "review-non-conformant",
        lambda a, w, m, o: a["compliance_reviews"][0].update({"level": "non_conformant"}),
        False,
    )

    def review_with_waiver(kind: str):
        def mutate(arch, work, msp, outputs) -> None:
            arch["compliance_reviews"][0]["level"] = "non_conformant"
            arch["waivers"].append(
                {
                    "id": f"w-{kind}",
                    "review_id": "rv-live",
                    "kind": kind,
                    "status": "active",
                    "owner_user_id": "u-owner",
                    "reason": "self-test",
                    "reassessment_at": "2099-01-01T00:00:00+00:00",
                }
            )

        return mutate

    # Only an architecture_waiver on the compliance review may suppress a non-conformant
    # verdict; a programme waiver never does (ADR 0012).
    hardening_case(
        "review-architecture-waiver", review_with_waiver("architecture_waiver"), True
    )
    hardening_case("review-programme-waiver", review_with_waiver("programme"), False)
    hardening_case(
        "review-superseded-contract",
        lambda a, w, m, o: a["compliance_reviews"][0].update({"contract_id": "ct-old"}),
        False,
    )

    def closure_latest_self_approved(arch, work, msp, outputs) -> None:
        work["approvals"].append(
            {"id": "ap-close-2", "project_id": "project-1", "gate": "closure",
             "status": "approved", "requested_by": "u-requester",
             "created_at": "2026-11-03T00:00:00+00:00"}
        )
        work["approval_decisions"].append(
            {"approval_id": "ap-close-2", "decision": "approve",
             "approver_id": "u-requester"}
        )

    hardening_case("closure-latest-self-approved", closure_latest_self_approved, False)
    hardening_case(
        "lesson-without-evidence",
        lambda a, w, m, o: m["lessons"][0].update({"evidence": []}),
        False,
    )

    # ── the four programme-level checks ────────────────────────────────────────────────
    # Each runs on a crafted snapshot and a crafted register; every case is one property that
    # must green under the right record and red under the wrong one.
    global _REWORK_CACHE, _REWORK_ERROR
    saved_cache, saved_error = _REWORK_CACHE, _REWORK_ERROR

    def extra_snapshot(tag: str, **parts: Any) -> Snapshot:
        base = workdir / f"extra-{tag}"
        (base / "data").mkdir(parents=True)
        (base / "docs").mkdir()
        project = {
            "key": "TEST",
            "id": "project-1",
            "status": "active",
            "meta": {"closure_outputs": parts.get("closure_outputs", [])},
        }
        (base / "data" / "programme.json").write_text(json.dumps({"projects": [project]}))
        (base / "data" / "architecture.json").write_text(
            json.dumps({"contracts": [], "compliance_reviews": [], "waivers": []})
        )
        (base / "data" / "work.json").write_text(json.dumps(parts.get("work") or {}))
        (base / "data" / "msp.json").write_text(json.dumps(parts.get("msp") or {}))
        (base / "data" / "artefacts.json").write_text(json.dumps({"documents": []}))
        (base / "data" / "manifest.json").write_text(json.dumps({"git_sha": "self-test"}))
        outputs = parts.get("outputs") or []
        for output in outputs:
            if output.get("path"):
                target = base / str(output["path"])
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text("x")
        (base / "docs" / "outputs.js").write_text(
            "window.CONVEYOR_OUTPUTS = " + json.dumps({"outputs": outputs}) + ";\n"
        )
        return Snapshot(base / "data")

    def register_case(
        tag: str,
        packages: Any,
        check: Callable[..., tuple[bool, str]],
        expect: bool,
        *,
        msp: dict[str, Any] | None = None,
    ) -> None:
        global _REWORK_CACHE
        _REWORK_CACHE = {"packages": packages}
        snapshot_case = extra_snapshot(tag, msp=msp)
        ok, detail = check(snapshot_case, "TEST", {})
        if ok is not expect:
            failures.append(f"{tag}: expected {'pass' if expect else 'fail'}, got {detail}")

    def standard_package(key: str, severity: str, **fields: Any) -> dict[str, Any]:
        package = {
            "key": key,
            "standard": "PMP",
            "status": "open",
            "severity": severity,
            "owner_project": "TEST",
        }
        package.update(fields)
        return package

    register_case("standard-open", [standard_package("std-pmp-x", "P2")], check_artefact_standard, False)
    register_case(
        "standard-other-owner",
        [standard_package("std-pmp-x", "P0", owner_project="OTHER")],
        check_artefact_standard,
        True,
    )
    register_case(
        "standard-suppressed",
        [standard_package("std-pmp-x", "P0", status="suppressed")],
        check_artefact_standard,
        False,
    )
    _REWORK_CACHE = {"packages": None}
    ok, detail = check_artefact_standard(extra_snapshot("standard-missing"), "TEST", {})
    if ok:
        failures.append("a register without a packages array did not red the artefact-standard check")

    planned_msp = {"tranches": [{"number": 5, "state": "planned"}]}
    authorised_msp = {"tranches": [{"number": 5, "state": "authorised"}]}
    tranche_pkg = standard_package("mandatory_artefacts:tranche-5", "P1", scope="tranche-5")
    register_case(
        "standard-planned-tranche", [tranche_pkg], check_artefact_standard, True, msp=planned_msp
    )
    register_case(
        "standard-authorised-tranche",
        [tranche_pkg],
        check_artefact_standard,
        False,
        msp=authorised_msp,
    )
    register_case(
        "standard-unknown-tranche", [tranche_pkg], check_artefact_standard, False
    )
    register_case(
        "standard-project-scope-still-due",
        [standard_package(
            "mandatory_artefacts:TEST", "P1", scope="TEST", owner_project="OTHER",
        )],
        check_artefact_standard,
        False,
        msp=planned_msp,
    )
    register_case(
        "standard-tranche-gate-moved",
        [standard_package(
            "mandatory_artefacts:tranche-5", "P1",
            scope="tranche-5", owner_project="TEST", apply_to="OTHER",
        )],
        check_artefact_standard,
        True,
        msp=authorised_msp,
    )
    register_case(
        "standard-tranche-gate-acting",
        [standard_package(
            "mandatory_artefacts:tranche-5", "P1",
            scope="tranche-5", owner_project="OTHER", apply_to="TEST",
        )],
        check_artefact_standard,
        False,
        msp=authorised_msp,
    )
    register_case(
        "standard-programme-gate-acting",
        [standard_package(
            "std-pmp-programme", "P1",
            scope="programme", owner_project="TEST", apply_to="OTHER",
        )],
        check_artefact_standard,
        True,
    )
    register_case(
        "standard-project-scope-own-obligation",
        [standard_package(
            "mandatory_artefacts:TEST", "P1",
            scope="TEST", owner_project="OTHER", apply_to="OTHER",
        )],
        check_artefact_standard,
        False,
    )
    # The live defect this pins: the register writes owner_project as the standard's
    # accountable role (T9) and apply_to as the delivery track (PLATFORM); the subject is the
    # scope. Gating by either field put every subject's package on the owner's manifest.
    register_case(
        "standard-subject-not-owner",
        [standard_package(
            "mandatory_artefacts:Z9", "P1",
            scope="Z9", owner_project="TEST", apply_to="PLATFORM",
        )],
        check_artefact_standard,
        True,
    )

    register_case("resolution-open-p1", [standard_package("pkg-1", "P1")], check_rework_resolution, False)
    register_case(
        "resolution-waiver-p1",
        [standard_package(
            "pkg-1", "P1", disposition="waiver",
            owners=["programme owner"], decision_ref="01a2", earliest_expires="2099-01-01",
        )],
        check_rework_resolution,
        False,
    )
    register_case(
        "resolution-waiver-p0",
        [standard_package(
            "pkg-1", "P0", disposition="waiver",
            owners=["programme owner"], decision_ref="01a2", earliest_expires="2099-01-01",
        )],
        check_rework_resolution,
        False,
    )
    register_case(
        "resolution-suppressed-p1",
        [standard_package("pkg-1", "P1", status="suppressed", disposition="unprovable")],
        check_rework_resolution,
        False,
    )
    register_case("resolution-open-p2", [standard_package("pkg-1", "P2")], check_rework_resolution, True)
    register_case(
        "resolution-planned-tranche", [tranche_pkg], check_rework_resolution, True, msp=planned_msp
    )
    register_case(
        "resolution-authorised-tranche",
        [tranche_pkg],
        check_rework_resolution,
        False,
        msp=authorised_msp,
    )
    register_case(
        "resolution-tranche-gate-moved",
        [standard_package(
            "mandatory_artefacts:tranche-5", "P1",
            scope="tranche-5", owner_project="TEST", apply_to="OTHER",
        )],
        check_rework_resolution,
        True,
        msp=authorised_msp,
    )
    register_case(
        "resolution-tranche-gate-acting",
        [standard_package(
            "mandatory_artefacts:tranche-5", "P1",
            scope="tranche-5", owner_project="OTHER", apply_to="TEST",
        )],
        check_rework_resolution,
        False,
        msp=authorised_msp,
    )
    register_case(
        "resolution-subject-not-owner",
        [standard_package(
            "mandatory_artefacts:Z9", "P1",
            scope="Z9", owner_project="TEST", apply_to="PLATFORM",
        )],
        check_rework_resolution,
        True,
    )
    register_case(
        "resolution-project-scope-own-obligation",
        [standard_package(
            "mandatory_artefacts:TEST", "P1",
            scope="TEST", owner_project="OTHER", apply_to="OTHER",
        )],
        check_rework_resolution,
        False,
    )
    register_case(
        "resolution-expired",
        [standard_package(
            "pkg-1", "P0", disposition="deferral",
            owners=["programme owner"], decision_ref="01a2", earliest_expires="2020-01-01",
        )],
        check_rework_resolution,
        False,
    )

    def work_case(tag: str, work: dict[str, Any], msp: dict[str, Any], expect: bool) -> None:
        snapshot_case = extra_snapshot(
            tag,
            work=work,
            msp=msp,
            outputs=[{"id": "deleg-1", "published_at": "2026-11-01T00:00:00+00:00",
                      "path": "docs/deleg.html", "category": "decision",
                      "project_id": "project-1"}],
        )
        ok, detail = check_delegation_provenance(snapshot_case, "TEST", {})
        if ok is not expect:
            failures.append(f"{tag}: expected {'pass' if expect else 'fail'}, got {detail}")

    rule_approval = {"id": "ap-1", "project_id": "project-1", "gate": "charter", "status": "approved"}
    rule_decision = {
        "approval_id": "ap-1", "decision": "approve", "decided_by_kind": "rule",
        "via": {"delegation_ref": "deleg-1"},
    }
    service_decision = {
        "approval_id": "ap-1", "decision": "approve", "decided_by_kind": "service",
        "via": {"channel": "internal"},
    }
    work_case("delegation-rule", {"approvals": [rule_approval], "approval_decisions": [rule_decision]}, {}, True)
    work_case("delegation-service", {"approvals": [rule_approval], "approval_decisions": [service_decision]}, {}, False)
    work_case(
        "delegation-rule-no-via",
        {"approvals": [rule_approval], "approval_decisions": [
            {"approval_id": "ap-1", "decision": "approve", "decided_by_kind": "rule"}]},
        {},
        False,
    )
    work_case(
        "delegation-exempt",
        {"approvals": [rule_approval], "approval_decisions": [service_decision]},
        {"register_items": [{
            "id": "chg-1", "project_id": "project-1", "kind": "change", "status": "accepted",
            "meta": {"provenance_residual": ["ap-1"]},
        }]},
        True,
    )

    def proof_case(tag: str, outputs: list[dict[str, Any]], closure: list[str], expect: bool) -> None:
        snapshot_case = extra_snapshot(tag, outputs=outputs, closure_outputs=closure)
        ok, detail = check_published_proof(snapshot_case, "TEST", {})
        if ok is not expect:
            failures.append(f"{tag}: expected {'pass' if expect else 'fail'}, got {detail}")

    proof_case(
        "proof-complete",
        [{"id": "o1", "project_id": "project-1", "category": "closure-review",
          "path": "docs/out.html", "published_at": "2026-11-01T00:00:00+00:00"}],
        ["closure-review"],
        True,
    )
    proof_case("proof-missing", [], ["closure-review"], False)
    proof_case(
        "proof-other-project",
        [{"id": "o1", "project_id": "project-2", "category": "closure-review",
          "path": "docs/out.html", "published_at": "2026-11-01T00:00:00+00:00"}],
        ["closure-review"],
        False,
    )
    broken = extra_snapshot(
        "proof-broken",
        outputs=[{"id": "o1", "project_id": "project-1", "category": "closure-review",
                  "path": "docs/gone.html", "published_at": "2026-11-01T00:00:00+00:00"}],
        closure_outputs=["closure-review"],
    )
    (broken.path.parent / "docs" / "gone.html").unlink()
    ok, detail = check_published_proof(broken, "TEST", {})
    if ok:
        failures.append("an indexed output whose file is gone did not red the published-proof check")

    missing = extra_snapshot("missing-project")
    missing_evidence = missing_project_evidence(
        "NOT-A-PROJECT",
        missing,
        missing.path,
        MANIFEST_DIR / "T18.toml",
        workdir / "missing-evidence",
        "all",
    )
    if (
        missing_evidence["verdict"] != "red"
        or "no project" not in missing_evidence["failures"][0]
    ):
        failures.append("a manifest whose project is absent did not produce a red evidence row")

    _REWORK_CACHE, _REWORK_ERROR = saved_cache, saved_error

    if failures:
        print("SELF-TEST FAILED")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print("verify-project: self-test OK")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project", metavar="KEY")
    parser.add_argument("--all", action="store_true")
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--snapshot", type=Path, default=DEFAULT_SNAPSHOT)
    parser.add_argument("--evidence-dir", type=Path, default=DEFAULT_EVIDENCE)
    parser.add_argument(
        "--kind", choices=("all", "snapshot", "command", "probe"), default="all"
    )
    parser.add_argument("--exclude", default="", help="comma-separated check ids or names to skip")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    if args.self_test:
        return self_test()
    if not args.project and not args.all:
        fail("--project <KEY> or --all is required (or --self-test)")
    if args.manifest and args.all:
        fail("--manifest names one project's manifest; use --project with it, not --all")
    keys = (
        [args.project]
        if args.project
        else sorted(
            path.stem for path in MANIFEST_DIR.glob("*.toml") if path.stem != "README"
        )
    )
    board = Snapshot(args.snapshot) if args.all else None
    red = 0
    for key in keys:
        manifest = args.manifest or (MANIFEST_DIR / f"{key}.toml")
        print(f"== {key} ==")
        if board is not None and key not in board.projects:
            evidence = missing_project_evidence(
                key, board, args.snapshot, manifest, args.evidence_dir, args.kind
            )
            print(f"FAIL  {key}-project  {evidence['failures'][0]}")
            print(f"{key}: red — 1 checks, 1 failures, 0 warnings")
            red += 1
            continue
        excluded = {item.strip() for item in args.exclude.split(",") if item.strip()}
        evidence = run_project(
            key, args.snapshot, manifest, args.evidence_dir, kind_filter=args.kind, exclude=excluded
        )
        print(
            f"{key}: {evidence['verdict']} — "
            f"{len(evidence['checks'])} checks, {len(evidence['failures'])} failures, "
            f"{len(evidence['warnings'])} warnings"
        )
        if evidence["verdict"] != "green":
            red += 1
    return 1 if red else 0


if __name__ == "__main__":
    sys.exit(main())
