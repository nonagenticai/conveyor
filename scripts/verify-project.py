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

    [[check]]
    id = "P0-closure-outputs"
    check = "closure_outputs"          # a registry name, or omit and give `command`
    description = "..."
    severity = "P0"                   # P0 | P1 | P2
    kind = "snapshot"                 # snapshot | command
    # command = "..."                 # required when kind = "command"

    [[allow]]                         # a documented residual, never a silent pass
    check = "benefit_baseline"
    reason = "..."
    owner = "programme owner"
    issue = "U6"
    expires = "2027-02-18"

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
DEFAULT_SNAPSHOT = ROOT / "docs" / "design" / "data"
MANIFEST_DIR = ROOT / "acceptance" / "programme"
DEFAULT_EVIDENCE = ROOT / "docs" / "evidence" / "rework"
PLACEHOLDER = re.compile(
    r"(not measured|no instrument|unknown|tbd|to be determined|n/?a|placeholder|none)",
    re.IGNORECASE,
)
SEVERITY = ("P0", "P1", "P2")


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
    if params.get("require_approved", False) and not approved:
        return False, "the project baseline has no distinct approver"
    return True, f"{len(baselines)} project baseline(s); {len(approved)} approved"


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
        approval = contract.get("approval_id")
        requester = contract.get("requested_by")
        approver = contract.get("approved_by")
        if approval and requester and approver and requester != approver:
            return True, f"contract {contract['id']} approved by {approver}, requested by {requester}"
    return False, "the live contract lacks an approval or a distinct requester/approver"


def check_review(
    snapshot: Snapshot, key: str, params: dict[str, Any]
) -> tuple[bool, str]:
    row = snapshot.project(key)
    reviews = snapshot.reviews(row["id"])
    if not reviews:
        if params.get("required", True):
            return False, "no Phase-G compliance review for the project's contract"
        return True, "no compliance review required for this row"
    for review in reviews:
        evidence = review.get("evidence") or {}
        reference = evidence.get("delegation_ref")
        if reference and snapshot.published_id(str(reference)):
            return True, f"review {review['id']} cites the published delegation {reference}"
    return False, "the review's delegation_ref is missing or does not resolve to a published output"


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
    for gate in gates:
        decisions = [
            decision
            for decision in snapshot.decisions(str(gate["id"]))
            if decision.get("decision") == "approve"
        ]
        distinct = [
            decision
            for decision in decisions
            if decision.get("approver_id") and decision.get("approver_id") != gate.get("requested_by")
        ]
        if distinct:
            return True, f"closure gate {gate['id']} decided by a distinct approver"
    return False, "the closure gate was approved only by its requester"


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
        if not baseline:
            return False, f"benefit {profile.get('id')} has no baseline"
        if isinstance(baseline, str):
            if PLACEHOLDER.search(baseline):
                return False, f"benefit {profile.get('id')} has a placeholder baseline: {baseline!r}"
            continue
        if isinstance(baseline, dict):
            missing = [
                field
                for field in ("value", "unit", "measured_at", "method", "evidence")
                if not baseline.get(field)
            ]
            if missing:
                return False, f"benefit {profile.get('id')} baseline is missing {', '.join(missing)}"
            continue
        return False, f"benefit {profile.get('id')} baseline has an unexpected type"
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


def check_wsjf(
    snapshot: Snapshot, key: str, params: dict[str, Any]
) -> tuple[bool, str]:
    row = snapshot.project(key)
    nodes = _wbs_nodes(snapshot, row)
    if not nodes:
        return False, "no WBS deliverable carries the plan"
    bad = []
    for node in nodes:
        if int(node.get("job_size") or 0) < 1:
            bad.append(f"{node.get('id')}: job_size")
            continue
        if all(int(node.get(field) or 0) == 0 for field in ("business_value", "time_criticality", "risk_reduction")):
            bad.append(f"{node.get('id')}: every value input is zero")
    if bad:
        return False, "WBS nodes without WSJF inputs: " + ", ".join(bad)
    return True, f"{len(nodes)} WBS node(s) carry the four WSJF inputs"


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
    missing = [
        node.get("id")
        for node in accepted
        if not (credits.get(node.get("id")) or {}).get("evidence")
    ]
    if missing:
        return False, f"{len(missing)} accepted deliverable(s) have no demo credit with evidence"
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
    return True, f"{len(lessons)} lesson(s) from the {period} retro"


def _published_for_period(
    snapshot: Snapshot, category: str, period: str, programme_key: str
) -> bool:
    """The PI artefact names its tranche in the title or unit (`tranche-N`).

    `unit` is the output's programme unit (see `documents.compose`); the retrospective batch
    writes the planning period there, and the title repeats it for a human reader.
    """
    needle = period.lower()
    for row in snapshot.outputs:
        if row.get("category") != category or row.get("project_key") != programme_key:
            continue
        label = f"{row.get('title') or ''} {row.get('unit') or ''}".lower()
        if needle in label:
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

    The register is derived, never stored: the conformance audit, the fresh standards
    findings and the checked-in v1 ledger are merged by `audit --rework-out`. One build per
    verifier process is enough; every project check reads the same result.
    """
    global _REWORK_CACHE, _REWORK_ERROR
    if _REWORK_CACHE is not None:
        return _REWORK_CACHE, None
    if _REWORK_ERROR is not None:
        return None, _REWORK_ERROR
    work = Path(tempfile.mkdtemp(prefix="verify-rework-"))
    try:
        conformance = work / "conformance"
        conformance.mkdir()
        audit = [
            sys.executable,
            str(ROOT / "scripts" / "audit-programme-conformance.py"),
            "--snapshot",
            str(snapshot.path),
            "--out",
            str(conformance),
            "--report",
        ]
        completed = subprocess.run(audit, capture_output=True, text=True)
        if completed.returncode != 0:
            _REWORK_ERROR = (
                "the conformance audit failed: "
                + (completed.stderr.strip() or completed.stdout.strip())[:300]
            )
            return None, _REWORK_ERROR
        standards = work / "standards-findings.json"
        completed = subprocess.run(
            [
                sys.executable,
                str(ROOT / "scripts" / "generate-standards.py"),
                "--snapshot",
                str(snapshot.path),
                "--outputs-dir",
                str(snapshot.path.parent / "docs"),
                "--gaps",
                str(conformance / "conformance-gap-register.json"),
                "--findings-out",
                str(standards),
            ],
            capture_output=True,
            text=True,
        )
        if completed.returncode != 0:
            _REWORK_ERROR = (
                "the standards generator failed: "
                + (completed.stderr.strip() or completed.stdout.strip())[:300]
            )
            return None, _REWORK_ERROR
        register_path = work / "rework.json"
        register_out = work / "register-conformance"
        register_out.mkdir()
        command = [
            sys.executable,
            str(ROOT / "scripts" / "audit-programme-conformance.py"),
            "--snapshot",
            str(snapshot.path),
            "--out",
            str(register_out),
            "--report",
            "--rework-out",
            str(register_path),
            "--findings",
            str(ROOT / "docs" / "evidence" / "v1-conformance-findings.json"),
            "--findings",
            str(standards),
        ]
        completed = subprocess.run(command, capture_output=True, text=True)
        if completed.returncode != 0:
            _REWORK_ERROR = (
                "the rework register failed: "
                + (completed.stderr.strip() or completed.stdout.strip())[:300]
            )
            return None, _REWORK_ERROR
        _REWORK_CACHE = json.loads(register_path.read_text(encoding="utf-8"))
        return _REWORK_CACHE, None
    finally:
        shutil.rmtree(work, ignore_errors=True)


def check_product_findings(
    snapshot: Snapshot, key: str, params: dict[str, Any]
) -> tuple[bool, str]:
    """No open P0 rework package may outlive the project that owns it.

    The register is derived from this snapshot and the checked-in v1 ledger. A package is
    cleared only when its gap disappears from a fresh audit or its finding carries
    `resolved_by`; a waiver is suppressed, not passed silently. A project cannot close over
    an open P0 package it owns.
    """
    register, error = rework_register(snapshot)
    if error:
        return False, error
    open_p0 = [
        package.get("key")
        for package in (register or {}).get("packages", [])
        if package.get("status") == "open"
        and package.get("severity") == "P0"
        and package.get("owner_project") == key
    ]
    if open_p0:
        return False, f"{len(open_p0)} open P0 package(s): " + ", ".join(open_p0)
    return True, "no open P0 package for this project"


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


def allow_error(allowed: dict[str, Any]) -> str | None:
    """An allow is a documented residual, not a silent pass.

    It must name who owns it, why it is accepted, and when it must be reassessed.  A manifest
    whose allow has no owner, no reason or no unexpired date is refused: otherwise a residual
    clears the verdict by simply existing.
    """
    for field in ("reason", "owner", "expires"):
        if not str(allowed.get(field) or "").strip():
            return f"the allow entry has no {field}"
    raw = str(allowed.get("expires")).replace("Z", "+00:00")
    try:
        expires = datetime.fromisoformat(raw)
    except ValueError:
        return f"the allow entry's expires is not a date: {allowed.get('expires')!r}"
    if expires.tzinfo is None:
        expires = expires.replace(tzinfo=timezone.utc)
    if expires <= datetime.now(timezone.utc):
        return f"the allow entry expired at {allowed.get('expires')}"
    return None


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
    allows = {
        str(entry.get("check")): entry for entry in manifest.get("allow", [])
    }
    excluded = exclude or set()
    checks: list[dict[str, Any]] = []
    failures: list[str] = []
    warnings: list[str] = []
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
        allowed = allows.get(name) or allows.get(identifier)
        if allowed:
            problem = allow_error(allowed)
            if problem:
                fail(f"{identifier}: {problem}")
        record: dict[str, Any] = {
            "id": identifier,
            "check": name or None,
            "kind": kind,
            "severity": severity,
            "description": description,
            "allowed": False,
        }
        if kind == "command":
            command = str(check.get("command") or "")
            if not command:
                fail(f"{identifier}: a command check needs `command`")
            ok, detail, exit_code, stdout, stderr = run_command(command, key, snapshot_dir)
            record.update(
                {"command": command, "exit": exit_code, "stdout": stdout, "stderr": stderr}
            )
        else:
            if kind != "snapshot":
                fail(f"{identifier}: kind must be snapshot or command")
            if name not in CHECKS:
                fail(f"{identifier}: no shared check named {name!r}")
            ok, detail = CHECKS[name](snapshot, key, check)
        if not ok and allowed:
            ok = True
            record["allowed"] = True
            record["allow"] = {
                field: allowed.get(field)
                for field in ("reason", "owner", "issue", "expires")
            }
        record["result"] = "pass" if ok else "fail"
        record["detail"] = detail
        checks.append(record)
        if not ok:
            target = failures if severity in ("P0", "P1") else warnings
            target.append(f"{identifier} ({severity}): {detail}")
        if not quiet:
            label = "ALLOW" if record["allowed"] else ("PASS" if ok else "FAIL")
            print(f"{label}  {identifier}  {detail}")
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
                "allowed": False,
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
                "benefit_profiles": [],
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
                     "statement": "the plan preceded the demo"}
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
                     "title": "PI plan — tranche 4", "unit": "tranche-4"},
                    {"id": "o2", "project_key": "P0", "category": "pi-objectives",
                     "title": "PI objectives — tranche 4", "unit": "tranche-4"},
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

    allowed = workdir / "TEST-allowed.toml"
    allowed.write_text(
        manifest.read_text()
        + """
[[allow]]
check = "closure_outputs"
reason = "self-test residual"
owner = "programme owner"
issue = "U6"
expires = "2027-02-18"
"""
    )
    green = run_project(
        "TEST", snapshot / "data", allowed, workdir / "evidence", quiet=True
    )
    if green["verdict"] != "green":
        failures.append("an allowed residual did not clear the verdict")
    if not any(row.get("allowed") for row in green["checks"]):
        failures.append("the allowed residual was not recorded")

    # An allow is a documented residual: ownerless and expired ones are refused, never
    # silently accepted. These are the two ways a residual could otherwise pass by existing.
    def refused_allow(name: str, body: str) -> None:
        path = workdir / f"TEST-allow-{name}.toml"
        path.write_text(manifest.read_text() + body)
        try:
            run_project("TEST", snapshot / "data", path, workdir / "evidence", quiet=True)
        except SystemExit:
            return
        failures.append(f"an invalid allow ({name}) was accepted")

    refused_allow(
        "ownerless",
        """
[[allow]]
check = "closure_outputs"
reason = "self-test residual"
expires = "2027-02-18"
""",
    )
    refused_allow(
        "expired",
        """
[[allow]]
check = "closure_outputs"
reason = "self-test residual"
owner = "programme owner"
expires = "2020-01-01"
""",
    )

    command_manifest = workdir / "TEST-command.toml"
    command_manifest.write_text(
        """
[project]
key = "TEST"
name = "Self-test project"
gate = "a command check"

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

[[check]]
id = "pi-cadence"
check = "pi_cadence"
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
    parser.add_argument("--kind", choices=("all", "snapshot", "command"), default="all")
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
    red = 0
    for key in keys:
        manifest = args.manifest or (MANIFEST_DIR / f"{key}.toml")
        print(f"== {key} ==")
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
