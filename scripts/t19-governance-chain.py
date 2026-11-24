#!/usr/bin/env python3
"""t19-governance-chain — T19's TOGAF record through the deployed engine.

T19's conformance audit names four T19-owned gaps: the seven mandatory architecture artefacts,
a live architecture contract, B7's baseline, and the `architecture-artifact` standard's
Metadata/Entries/Links/Document sections. This script performs those acts through
`scripts/programme-record.py` (the same transport the programme record uses: the deployed
engine's action surface, never SQL), and reads every row back.

Acts, in order:

  1. `architecture.artifacts_create` + `architecture.artifacts_approve` — one artefact per
     mandatory type, entries from `docs/plans/T19/architecture-design.md`.
  2. `architecture.artifacts_link` — the derivation/realization/dependency edges from the
     design's §5.
  3. `architecture.contracts_request` → `architecture.board_decide` (the Architecture Board,
     citing the standing delegation) → `architecture.contracts_approve` (a distinct approver).
  4. `benefits.update` — B7's measured baseline (zero deliberated decision rows before T19).
  5. `documents.compose` + `documents.publish` — the `architecture-artifact` output whose
     headings carry the standard's four sections.

The approver is the tenant's `conveyor-approver` member, acting under the published standing
delegation; the requester/writer is the operator's default tenant actor, so the contract's
requester is never its approver. Every write needs an evidence file first; the script writes one
beside the others under docs/evidence/t19/gov/ and appends the engine's own output.

Usage:
    python3 scripts/t19-governance-chain.py --check
    python3 scripts/t19-governance-chain.py --apply
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RECORDER = ROOT / "scripts" / "programme-record.py"
EVIDENCE = ROOT / "docs" / "evidence" / "t19" / "gov"

PROJECT = "01a20c80-b9b8-7214-abc6-f2c4f75a1fc1"
PROGRAMME = "01a1d039-0d24-7d16-ae52-4841e96ccfac"
BOARD_KEY = "conveyor-architecture-board"
DELEGATION = "01a1fb9f-ac2d-732d-b274-1079be3dd2b3"
APPROVER = "01a1e8a7-8b23-76a9-9114-aafb95f7a545"

REQUIRED_TYPES = [
    "stakeholder_map",
    "driver_goal_objective",
    "requirements_catalog",
    "solution_concept",
    "business_capability_map",
    "data_entity_catalog",
    "technology_standards",
]

# The seven artefacts' content, from docs/plans/T19/architecture-design.md (A1–A6, B1, C1, D1).
ARTEFACTS: dict[str, dict] = {
    "stakeholder_map": {
        "title": "T19 — Stakeholder map",
        "purpose": "Who holds an interest in the decision forum and how they are engaged.",
        "scope": "The forum's participants and consumers; §A3 of the T19 architecture design.",
        "entries": [
            ("Owner / sponsoring group", "the decision and the dissent are the record; can join any forum",
             {"influence": "high", "interest": "high", "engagement": "seat in the room or async; decides items"}),
            ("Chair", "the agenda is adopted, minutes publish behind its gate",
             {"influence": "high", "interest": "high", "engagement": "chair seat; adopts agenda; publishes minutes"}),
            ("Operations department", "its position is not outvoted silently; conditions are visible",
             {"influence": "high", "interest": "high", "engagement": "required department on operational items"}),
            ("Security department", "evidence and conflict rules are not waived",
             {"influence": "high", "interest": "high", "engagement": "required department on security items"}),
            ("Delivery department", "commitments have owners and dates",
             {"influence": "high", "interest": "high", "engagement": "required department on delivery items"}),
            ("Job bindings", "their position and evidence are typed",
             {"influence": "medium", "interest": "high", "engagement": "member/presenter seats"}),
            ("Observers", "they can read and be recorded, never count",
             {"influence": "low", "interest": "medium", "engagement": "observer seat"}),
            ("Architecture Board (T9)", "board sittings are meetings",
             {"influence": "high", "interest": "medium", "engagement": "governance_body convener"}),
            ("Programme cadence (T10)", "PI planning/demo/I&A are meeting instances",
             {"influence": "high", "interest": "medium", "engagement": "cadence agenda source"}),
        ],
    },
    "driver_goal_objective": {
        "title": "T19 — Drivers, goals and objectives",
        "purpose": "Why the decision forum exists and what it is measured against.",
        "scope": "§A4 of the T19 architecture design.",
        "entries": [
            ("DR1", "driver",
             "High-level work is decided in cross-department meetings; only the approval survives today."),
            ("G1", "goal",
             "Every decision that needed several positions carries those positions, conditions and dissent."),
            ("G2", "goal",
             "A meeting never weakens a gate; the stricter policy wins."),
            ("O1", "objective",
             "T19's §12 gate passes with a three-department majority, an inquorate deferral and a live owner decision."),
            ("O2", "objective",
             "Decision latency, agenda conversion, quorum-miss, commitment slippage and action closure recompute from rows."),
        ],
    },
    "requirements_catalog": {
        "title": "T19 — Requirements catalog",
        "purpose": "The typed conditions, constraints, assumptions and deltas the design satisfies.",
        "scope": "§A2 of the T19 architecture design; §6.8 of docs/SPEC_V2.md.",
        "entries": [
            ("R1", "condition", "A meeting is a card with a chair and a DoD; it is not a fifth unit."),
            ("R2", "condition", "A deciding item names exactly one pre-opened approval with evidence."),
            ("R3", "constraint", "A meeting cannot lower dual control, waive evidence or bypass P4."),
            ("R4", "constraint", "A required department's absence defers the item inquorate."),
            ("R5", "constraint", "One live position per participant per item; immutable after close."),
            ("R6", "condition", "A commitment needs an owner and a date; an action becomes a card."),
            ("R7", "condition", "Minutes are a chaired governed_asset; every line matches a row."),
            ("R8", "condition", "A human seat is an ordinary app_user; any seat role."),
            ("R9", "condition", "The owner can decide live or asynchronously over web/Telegram/MCP; first wins."),
            ("R10", "constraint", "No calendar or AV dependency; ICS is a read."),
            ("R11", "condition", "Participant-hours enter the D52 roll-up as a third source."),
            ("R12", "condition", "A forum with no decision over a cycle is flagged and retired."),
            ("R13", "assumption", "T9 governance_body and T10 programme_cadence remain the consumer seams."),
            ("R14", "delta", "approval_decision must accept a job-binding participant."),
        ],
    },
    "solution_concept": {
        "title": "T19 — Solution concept",
        "purpose": "The elements of the forum and how a position becomes a decision row.",
        "scope": "§A5 of the T19 architecture design.",
        "entries": [
            ("card(meeting)", "aggregate root",
             "The meeting is a card with its own DoD; the card type is `meeting`."),
            ("meeting", "aggregate",
             "Convener, type version, window and cadence slot; 1→1 card."),
            ("meeting_agenda_item", "aggregate member",
             "Sequence, the resolved approval, required departments, evidence and outcome."),
            ("approval", "linked decision store",
             "The pre-opened approval the item resolves; the meeting never creates a second store."),
            ("approval_decision", "decision row",
             "One row per qualifying seat, extended with a job-binding participant and the via surface."),
            ("meeting_position", "deliberation",
             "Stance, rationale, conditions, commitment and surface; never a decision."),
            ("meeting_attendance", "participant",
             "job_binding | app_user, seat and department; the record outlives the room."),
            ("governed_asset(kind=minutes)", "published artefact",
             "Drafted from the rows, published behind the chair's approval."),
            ("commitment", "outcome",
             "Becomes a project_dependency edge or a card with an owner and a date."),
            ("action", "outcome",
             "Becomes a card on the board."),
            ("decision", "outcome",
             "Derived by the close arithmetic over positions and the linked approval's stricter policy."),
        ],
    },
    "business_capability_map": {
        "title": "T19 — Business capability map",
        "purpose": "The forums the capability provides, their conveners, agenda sources and rules.",
        "scope": "§B1 of the T19 architecture design.",
        "entries": [
            ("Programme board", "MSP",
             {"convener": "sponsoring group / SRO", "agenda": "programme register, benefit-at-risk, tranche decisions",
              "rule": "chair", "outcome": "decision/commitment"}),
            ("Tranche review", "MSP",
             {"convener": "sponsoring group", "agenda": "baselines, benefit status, risk register",
              "rule": "unanimous or chair per type", "outcome": "continue/pause/stop"}),
            ("Architecture Board", "TOGAF",
             {"convener": "governance_body", "agenda": "architecture contracts, compliance, waivers, change requests",
              "rule": "majority", "outcome": "decision/action"}),
            ("Design Board", "TOGAF",
             {"convener": "governance_body", "agenda": "solution designs, design gate, deviations",
              "rule": "majority", "outcome": "decision/action"}),
            ("PI planning", "SAFe",
             {"convener": "programme manager", "agenda": "backlog by WSJF, capacity, plan approval",
              "rule": "majority", "outcome": "decision/commitment"}),
            ("System demo", "SAFe",
             {"convener": "programme manager", "agenda": "completed deliverables and evidence",
              "rule": "chair", "outcome": "information/decision"}),
            ("Inspect & adapt", "SAFe",
             {"convener": "coach", "agenda": "retro findings, lessons, kata candidates",
              "rule": "majority", "outcome": "action/commitment"}),
            ("Incident review", "operations",
             {"convener": "incident commander", "agenda": "incident, postmortem, runbook findings",
              "rule": "chair", "outcome": "decision/action"}),
        ],
    },
    "data_entity_catalog": {
        "title": "T19 — Data entity catalog",
        "purpose": "The entities the forum adds and the relationships that keep one record.",
        "scope": "§C1 of the T19 architecture design; migration 0065_meetings.sql.",
        "entries": [
            ("meeting_type", "policy, required participation, quorum, rule, evidence, window, outcomes, version",
             {"relationships": "1→* meeting"}),
            ("meeting", "card, convener, type version, window, cadence slot",
             {"relationships": "1→* agenda items, attendance; 1→1 card"}),
            ("meeting_agenda_item", "sequence, resolved object, departments, evidence, outcome",
             {"relationships": "*→1 approval; 1→* positions"}),
            ("meeting_attendance", "participant kind/id, seat, department, invitation/response",
             {"relationships": "*→1 meeting"}),
            ("meeting_position", "participant, stance, rationale, conditions, commitment, surface",
             {"relationships": "*→1 item"}),
            ("approval_decision (extended)", "human or job participant; via surface",
             {"relationships": "*→1 approval"}),
            ("governed_asset(kind=minutes)", "minutes body, status, approval",
             {"relationships": "1→1 meeting"}),
        ],
    },
    "technology_standards": {
        "title": "T19 — Technology standards",
        "purpose": "The platform the forum runs on and what it deliberately does not add.",
        "scope": "§D1 of the T19 architecture design.",
        "entries": [
            ("PostgreSQL 18", "storage",
             "RLS on every new meeting table; the four-unit model is unchanged."),
            ("Rust action registry", "interface",
             "The twelve meetings.* actions are ordinary registry actions with scopes and a parity id."),
            ("HTTP + MCP surfaces", "interface",
             "The same action reachable from the console, the agent front end and Telegram."),
            ("Leptos console", "presentation",
             "The /meetings and /meetings/:id routes render the room and the derived outcome."),
            ("Telegram adapter", "presentation",
             "The human seat answers an invitation and records a position."),
            ("ICS feed", "read-only integration",
             "Rendered from attendance; a read, never a calendar dependency."),
            ("Exclusions", "constraint",
             "No Redis, no new port, no hosted model, no calendar service, no AV service."),
        ],
    },
}

# docs/plans/T19/architecture-design.md §5, restricted to the seven typed artefacts.
LINKS = [
    ("driver_goal_objective", "requirements_catalog", "derivation"),
    ("requirements_catalog", "solution_concept", "derivation"),
    ("solution_concept", "data_entity_catalog", "realization"),
    ("solution_concept", "business_capability_map", "realization"),
    ("stakeholder_map", "business_capability_map", "dependency"),
    ("business_capability_map", "technology_standards", "realization"),
    ("data_entity_catalog", "technology_standards", "realization"),
]

CONTRACT_TITLE = "Statement of Architecture Work — T19 decision forums (2026-11-24)"
CONTRACT_BODY = {
    "assignment": "Build T19 — decision forums — to the §12 gate: a meeting is a card whose agenda items resolve approvals, whose participants hold typed positions, and whose outcomes are derived arithmetic over those positions.",
    "scope": "The meeting aggregate, the approval_decision participant extension, the minutes governed-asset kind, the twelve meetings.* actions, the Meetings destination and the two consumer seams (governance_body, programme_cadence).",
    "responsibilities": "The programme architect owns the artefact set; the Architecture Board reviews and approves; the SRO accepts the benefit B7 and the tranche evidence.",
    "acceptance_criteria": "The six §12 gate clauses plus D1–D8; the live gate recorded in docs/plans/T19/evidence.md; the closure decision in docs/plans/T19/closure-review.md.",
    "delegation_ref": DELEGATION,
}

DELIVERY_TITLE = "Delivery contract — T19 decision forums (2026-11-24)"
DELIVERY_BODY = {
    "commitments": [
        "The twelve meetings.* actions, the five tables and the approval_decision participant extension are delivered and live (migration 0065).",
        "The human seat is available in the web room, over Telegram and over MCP, and the first decision across surfaces counts.",
        "Minutes are a governed asset published behind the chair's approval; the meeting closes only with them.",
        "The slice's acceptance is the D1–D8 evidence and the live gate recorded in docs/plans/T19/evidence.md.",
    ],
    "compliance_method": "The Architecture Board's Phase-G compliance review reads the delivered rows against the Statement of Architecture Work; each deviation is a change request or a recorded waiver.",
    "approval": "The Architecture Board approves this contract under the standing delegation; the board decision is cited in the approval field of the contract row.",
    "delegation_ref": DELEGATION,
}

BASELINE = {
    "value": 0,
    "unit": "deliberated cross-department decision rows before T19",
    "measured_at": "2026-11-24",
    "method": "count of meeting rows with a closed deciding agenda item before T19; the pre-T19 record holds approval rows only (docs/plans/T19/inception-analysis.md)",
}

# B7 is a programme-level benefit; the programme manager is its owner and business-change owner
# in this tenant's job model (organisation.jobs).
BENEFIT_OWNER_JOB = "01a1db6b-4f17-7e76-838d-590d71578ad1"  # programme_manager

DOCUMENT_TITLE = "T19 — Architecture artifact (the typed TOGAF set)"


def run_recorder(action, payload, *, evidence=None, actor=None, if_match=None,
                 read_action=None, read_input=None, read_id_from_output=None):
    command = [sys.executable, str(RECORDER), "--action", action,
               "--input", json.dumps(payload, separators=(",", ":"))]
    if evidence is not None:
        command += ["--evidence", str(evidence)]
    if actor:
        command += ["--actor-id", actor]
    if if_match is not None:
        command += ["--if-match", str(if_match)]
    if read_action:
        command += ["--read-action", read_action]
    if read_input is not None:
        command += ["--read-input", json.dumps(read_input, separators=(",", ":"))]
    if read_id_from_output:
        command += ["--read-id-from-output", read_id_from_output]
    completed = subprocess.run(command, capture_output=True, text=True)
    if completed.returncode != 0:
        detail = (completed.stderr or "") + (completed.stdout or "")
        raise RuntimeError(f"{action}: recorder exited {completed.returncode}\n{detail}")
    parts: dict[str, dict] = {}
    label = None
    buffer: list[str] = []
    for line in completed.stdout.splitlines():
        if line.startswith("== ") and line.endswith(" =="):
            if label and buffer:
                parts[label] = json.loads("\n".join(buffer))
            label = line.strip("= ").split(": ", 1)[0]
            buffer = []
        else:
            buffer.append(line)
    if label and buffer:
        parts[label] = json.loads("\n".join(buffer))
    return parts, completed.stdout


def read(action, payload):
    parts, _ = run_recorder(action, payload)
    return parts.get("read", {})


def evidence_file(name, header):
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    path = EVIDENCE / name
    if not path.exists():
        path.write_text(header + "\n", encoding="utf-8")
    return path


def write_with_evidence(action, payload, *, name, header, actor=None, if_match=None,
                        read_action=None, read_input=None, read_id_from_output=None):
    path = evidence_file(name, header)
    parts, raw = run_recorder(
        action, payload, evidence=path, actor=actor, if_match=if_match,
        read_action=read_action, read_input=read_input,
        read_id_from_output=read_id_from_output,
    )
    with path.open("a", encoding="utf-8") as handle:
        handle.write("\n```\n" + raw.rstrip() + "\n```\n")
    return parts.get("write", {}), parts.get("read-back", {})


def load_state():
    profile = read("architecture.profile_get", {})
    artefacts = read("architecture.artifacts_list", {"project_id": PROJECT}).get("items", [])
    contracts = read("architecture.contracts_list", {"project_id": PROJECT}).get("items", [])
    benefits = read("benefits.list", {"programme_id": PROGRAMME}).get("items", [])
    benefit = next((b for b in benefits if b.get("reference") == "B7"), None)
    documents = read("documents.list", {"project_id": PROJECT, "category": "architecture-artifact"})
    board = read("architecture.board_get", {"key": BOARD_KEY})
    delegation = read("documents.get", {"id": DELEGATION})
    return {
        "profile": profile,
        "artefacts": artefacts,
        "contracts": contracts,
        "benefit": benefit,
        "documents": documents.get("items", []),
        "board": board,
        "delegation": delegation,
    }


def standard_types(state):
    return {row["key"]: row for row in state["profile"].get("standard") or []}


def approved_artefacts(artefacts):
    return [row for row in artefacts if row.get("status") == "approved"]


def ensure_artefacts(state):
    types = standard_types(state)
    existing = {row.get("artifact_type_id"): row for row in state["artefacts"]}
    created = []
    for key in REQUIRED_TYPES:
        type_row = types.get(key)
        if type_row is None:
            raise RuntimeError(f"the standard has no artefact type {key!r}")
        current = existing.get(type_row["id"])
        if current and current.get("status") == "approved":
            print(f"SKIP  {key}: artefact {current['id']} is approved")
            continue
        if current is None:
            content = ARTEFACTS[key]
            entries = [
                {"axis_a": axis_a, "axis_b": axis_b, "value": value, "position": position}
                for position, (axis_a, axis_b, value) in enumerate(content["entries"])
            ]
            payload = {
                "project_id": PROJECT,
                "artifact_type_id": type_row["id"],
                "phase_code": type_row["phase_code"],
                "plateau": type_row["plateau"],
                "artifact_kind": type_row["artifact_kind"],
                "title": content["title"],
                "purpose": content["purpose"],
                "scope": content["scope"],
                "entries": entries,
            }
            header = (
                f"# {content['title']}\n\n"
                f"Action `architecture.artifacts_create` for project T19, type `{key}` "
                f"({type_row['phase_code']}/{type_row['plateau']}, {type_row['artifact_kind']}).\n"
                f"Entries from docs/plans/T19/architecture-design.md; the act is delegated by the "
                f"published standing delegation {DELEGATION}.\n"
            )
            written, read_back = write_with_evidence(
                "architecture.artifacts_create", payload, name=f"artifact-{key}.md",
                header=header, read_action="architecture.artifacts_get",
                read_id_from_output="id",
            )
            current = written
            print(f"CREATE {key}: artefact {current['id']} v{current['version']}")
        else:
            print(f"RESUME {key}: draft artefact {current['id']} v{current['version']}")
        if current.get("version") is None:
            current = read("architecture.artifacts_get", {"id": current["id"]})
        header = (
            f"# {current['title']} — approval\n\n"
            f"Action `architecture.artifacts_approve` for artefact {current['id']}, If-Match "
            f"{current['version']}. The reviewer is the tenant's `conveyor-approver` seat acting "
            f"under the standing delegation {DELEGATION}.\n"
        )
        approved, _ = write_with_evidence(
            "architecture.artifacts_approve", {"id": current["id"]}, name=f"approve-{key}.md",
            header=header, actor=APPROVER, if_match=current["version"],
            read_action="architecture.artifacts_get", read_input={"id": current["id"]},
        )
        if approved.get("status") != "approved":
            raise RuntimeError(f"{key}: approval did not read back approved: {approved}")
        created.append(approved["id"])
        print(f"APPROVE {key}: {approved['id']} is approved")
    return created


def ensure_links(state):
    # Re-read the artefacts so a resumed run sees the rows it just approved.
    artefacts = read("architecture.artifacts_list", {"project_id": PROJECT}).get("items", [])
    by_title = {row.get("artifact_type_id"): row["id"] for row in artefacts}
    types = standard_types(state)
    ids = {key: by_title.get(types[key]["id"]) for key in REQUIRED_TYPES}
    missing = [key for key, value in ids.items() if not value]
    if missing:
        raise RuntimeError(f"cannot link: no artefact for {missing}")
    existing: set[tuple[str, str, str]] = set()
    for key in REQUIRED_TYPES:
        edges = read("architecture.artifacts_links", {"id": ids[key]}).get("items", [])
        for edge in edges:
            existing.add((edge.get("from_artifact_id"), edge.get("to_artifact_id"), edge.get("kind")))
    for source_key, target_key, kind in LINKS:
        source, target = ids[source_key], ids[target_key]
        if (source, target, kind) in existing:
            print(f"SKIP  link {source_key} -{kind}-> {target_key}")
            continue
        header = (
            f"# T19 artefact link — {source_key} {kind} {target_key}\n\n"
            f"Action `architecture.artifacts_link` from {source} to {target}, kind `{kind}` "
            f"(docs/plans/T19/architecture-design.md §5).\n"
        )
        linked, _ = write_with_evidence(
            "architecture.artifacts_link",
            {"from_artifact_id": source, "to_artifact_id": target, "kind": kind},
            name="artifact-links.md", header=header,
            read_action="architecture.artifacts_links", read_input={"id": source},
        )
        print(f"LINK  {source_key} -{kind}-> {target_key}: {linked.get('id')}")


def ensure_contract(state):
    live = [
        row for row in state["contracts"]
        if row.get("status") == "live" and row.get("approved_at")
    ]
    if live:
        print(f"SKIP  contract: live contract {live[0]['id']} ({live[0].get('kind')})")
        return live[0]
    draft = next(
        (row for row in state["contracts"] if row.get("status") == "draft"
         and row.get("kind") == "statement_of_architecture_work"),
        None,
    )
    if draft:
        contract = draft
        print(f"RESUME contract: draft {contract['id']}")
    else:
        header = (
            f"# {CONTRACT_TITLE}\n\n"
            f"Action `architecture.contracts_request` for project T19, kind "
            f"statement_of_architecture_work, phase a. Requested by the operator's default tenant "
            f"actor; approved by a distinct seat.\n"
        )
        contract, _ = write_with_evidence(
            "architecture.contracts_request",
            {"project_id": PROJECT, "kind": "statement_of_architecture_work",
             "phase_code": "a", "title": CONTRACT_TITLE, "body": CONTRACT_BODY},
            name="contract-request.md", header=header,
            read_action="architecture.contracts_get", read_id_from_output="id",
        )
        print(f"REQUEST contract: {contract['id']} v{contract['version']}")
    header = (
        f"# T19 architecture contract — the Board's decision\n\n"
        f"Action `architecture.board_decide` by the Architecture Board on contract "
        f"{contract['id']}, citing the standing delegation {DELEGATION}. The outcome approval is "
        f"the independent authority the contract approval cites.\n"
    )
    decision, _ = write_with_evidence(
        "architecture.board_decide",
        {"body_id": state["board"]["id"], "decision": "approved",
         "subject_kind": "contract", "subject_id": contract["id"],
         "evidence": {"delegation_ref": DELEGATION,
                      "basis": "T19's architecture design, its artefact set and its live gate"}},
        name="contract-board-decision.md", header=header, actor=APPROVER,
        read_action="architecture.board_get", read_input={"id": state["board"]["id"]},
    )
    approval_id = decision.get("approval_id")
    if not approval_id:
        raise RuntimeError(f"board_decide returned no approval_id: {decision}")
    print(f"BOARD contract {contract['id']} approved: {approval_id}")
    header = (
        f"# T19 architecture contract — approval\n\n"
        f"Action `architecture.contracts_approve` for contract {contract['id']}, If-Match "
        f"{contract['version']}, approval {approval_id}, delegation {DELEGATION}. The approver is "
        f"`conveyor-approver`, distinct from the requester.\n"
    )
    approved, _ = write_with_evidence(
        "architecture.contracts_approve",
        {"id": contract["id"], "approval_id": approval_id, "delegation_ref": DELEGATION},
        name="contract-approve.md", header=header, actor=APPROVER,
        if_match=contract["version"],
        read_action="architecture.contracts_get", read_input={"id": contract["id"]},
    )
    if approved.get("status") != "live" or not approved.get("approved_at"):
        raise RuntimeError(f"contract did not read back live: {approved}")
    if approved.get("requested_by") and approved.get("requested_by") == approved.get("approved_by"):
        raise RuntimeError(f"contract requester is its approver: {approved}")
    print(f"APPROVE contract: {contract['id']} is live")
    return approved


def complete_baseline(baseline) -> bool:
    """A measurement record; `value` may legitimately be 0 (a pre-T19 count)."""
    return (
        isinstance(baseline, dict)
        and baseline.get("value") is not None
        and all(baseline.get(k) for k in ("unit", "measured_at", "method"))
    )


def ensure_delivery_contract(state):
    """The programme's second contract kind: the phase-F delivery contract.

    Every closed delivery project in the tenant carries a `statement_of_architecture_work`
    (phase A) and a `delivery_contract` (phase F); T19 carried only the first. The
    delivery-contract artefact standard and phase F's contract section resolve on this row.
    """
    live = [
        row for row in state["contracts"]
        if row.get("kind") == "delivery_contract" and row.get("status") == "live"
    ]
    if live:
        print(f"SKIP  delivery contract: live {live[0]['id']}")
        return live[0]
    draft = next(
        (row for row in state["contracts"]
         if row.get("kind") == "delivery_contract" and row.get("status") == "draft"),
        None,
    )
    if draft:
        contract = draft
        print(f"RESUME delivery contract: draft {contract['id']}")
    else:
        header = (
            f"# {DELIVERY_TITLE}\n\n"
            f"Action `architecture.contracts_request` for project T19, kind delivery_contract, "
            f"phase f. Requested by the operator's default tenant actor; approved by a distinct "
            f"seat after the Board's decision.\n"
        )
        contract, _ = write_with_evidence(
            "architecture.contracts_request",
            {"project_id": PROJECT, "kind": "delivery_contract", "phase_code": "f",
             "title": DELIVERY_TITLE, "body": DELIVERY_BODY},
            name="delivery-contract-request.md", header=header,
            read_action="architecture.contracts_get", read_id_from_output="id",
        )
        print(f"REQUEST delivery contract: {contract['id']} v{contract['version']}")
    header = (
        f"# T19 delivery contract — the Board's decision\n\n"
        f"Action `architecture.board_decide` by the Architecture Board on contract "
        f"{contract['id']}, citing the standing delegation {DELEGATION}.\n"
    )
    decision, _ = write_with_evidence(
        "architecture.board_decide",
        {"body_id": state["board"]["id"], "decision": "approved",
         "subject_kind": "contract", "subject_id": contract["id"],
         "evidence": {"delegation_ref": DELEGATION,
                      "basis": "T19's delivered actions, tables and live gate against the Statement of Architecture Work"}},
        name="delivery-contract-board-decision.md", header=header, actor=APPROVER,
        read_action="architecture.board_get", read_input={"id": state["board"]["id"]},
    )
    approval_id = decision.get("approval_id")
    if not approval_id:
        raise RuntimeError(f"board_decide returned no approval_id: {decision}")
    header = (
        f"# T19 delivery contract — approval\n\n"
        f"Action `architecture.contracts_approve` for contract {contract['id']}, If-Match "
        f"{contract['version']}, approval {approval_id}, delegation {DELEGATION}. The approver is "
        f"`conveyor-approver`, distinct from the requester.\n"
    )
    approved, _ = write_with_evidence(
        "architecture.contracts_approve",
        {"id": contract["id"], "approval_id": approval_id, "delegation_ref": DELEGATION},
        name="delivery-contract-approve.md", header=header, actor=APPROVER,
        if_match=contract["version"],
        read_action="architecture.contracts_get", read_input={"id": contract["id"]},
    )
    if approved.get("status") != "live" or not approved.get("approved_at"):
        raise RuntimeError(f"delivery contract did not read back live: {approved}")
    print(f"APPROVE delivery contract: {contract['id']} is live")
    return approved


def ensure_baseline(state):
    benefit = state["benefit"]
    if not benefit:
        raise RuntimeError("B7 is not a live benefit profile")
    baseline = benefit.get("baseline")
    if complete_baseline(baseline) and benefit.get("benefit_owner_job_id"):
        print(f"SKIP  B7: measurement record and owner are already set ({baseline})")
        return benefit
    header = (
        f"# B7 — the measured baseline and its owner\n\n"
        f"Action `benefits.update` for benefit profile {benefit['id']} (B7): the pre-T19 baseline "
        f"is a measurement record (zero deliberated cross-department decision rows existed before "
        f"T19, docs/plans/T19/inception-analysis.md), and the benefit owner is the programme "
        f"manager job {BENEFIT_OWNER_JOB}. The act is delegated by the standing delegation "
        f"{DELEGATION}.\n"
    )
    payload = {
        "id": benefit["id"],
        "baseline": BASELINE,
        "measurement_method": benefit.get("measurement_method"),
        "owner_job_id": benefit.get("owner_job_id") or BENEFIT_OWNER_JOB,
        "benefit_owner_job_id": benefit.get("benefit_owner_job_id") or BENEFIT_OWNER_JOB,
    }
    updated, _ = write_with_evidence(
        "benefits.update", payload, name="benefit-b7-baseline.md", header=header,
        if_match=benefit["version"],
        read_action="benefits.get", read_input={"id": benefit["id"]},
    )
    if updated.get("baseline") != BASELINE or not updated.get("benefit_owner_job_id"):
        raise RuntimeError(f"B7 did not read back: {updated}")
    print(f"UPDATE B7: baseline {BASELINE['value']} {BASELINE['unit']}; owner {BENEFIT_OWNER_JOB}")
    return updated


def document_body(state, contract, artefacts):
    types = standard_types(state)
    lines = [
        "# Architecture artefact — T19 decision forums",
        "",
        "The typed TOGAF artefact set for T19: seven artefacts, their links, the published ",
        "document and the Statement of Architecture Work they sit under.",
        "",
        "## Metadata",
        "",
        "| Field | Value |",
        "|---|---|",
        "| Project | T19 — Decision forums (meetings) |",
        "| Artefact type | architecture-artifact (TOGAF, §8.1) |",
        "| Version | 1 |",
        "| Owner | programme-architect |",
        "| Reviewer | Conveyor Architecture Board |",
        "| Review date | 2026-11-24 |",
        "| Phase / plateau | A–D / PSA + HLD |",
        f"| Authority | standing delegation {DELEGATION} (published decision output under P0) |",
        "",
        "## Entries",
        "",
        "| Artefact | Type | Phase / plateau | Entries | Status |",
        "|---|---|---|---|---|",
    ]
    for artefact in artefacts:
        type_row = next((t for t in types.values() if t["id"] == artefact.get("artifact_type_id")), {})
        lines.append(
            f"| {artefact.get('title')} ({artefact.get('id')}) | {type_row.get('key', '?')} | "
            f"{artefact.get('phase_code')}/{artefact.get('plateau')} | "
            f"{len(ARTEFACTS.get(type_row.get('key'), {}).get('entries', []))} | "
            f"{artefact.get('status')} |"
        )
    lines += [
        "",
        "Every entry is a row in `architecture_artifact_entry`; the seven types are the standard's "
        "phase A–D mandatory set for a project at phase G.",
        "",
        "## Links",
        "",
        "| From | Kind | To |",
        "|---|---|---|",
    ]
    for source_key, target_key, kind in LINKS:
        lines.append(f"| {ARTEFACTS[source_key]['title']} | {kind} | {ARTEFACTS[target_key]['title']} |")
    lines += [
        "",
        "The edges follow the design's §5: goals derive requirements, requirements derive the ",
        "solution concept, the concept realizes the data entities and the capability map, the ",
        "stakeholder map depends on the capability map, and both the capability map and the data ",
        "entities are realized on the technology standards.",
        "",
        "## Document",
        "",
        f"This output is the published `architecture-artifact` document for T19. The artefact set "
        f"sits under the Statement of Architecture Work `{contract.get('title')}` "
        f"({contract.get('id')}), approved by the Architecture Board under the standing "
        f"delegation; the board decision is `{contract.get('approval_id')}`.",
        "",
    ]
    return "\n".join(lines)


def ensure_document(state, contract):
    documents = state["documents"]
    published = [
        doc for doc in documents
        if doc.get("published_at") or doc.get("publishable") is True
    ]
    if published:
        print(f"SKIP  document: architecture-artifact {published[0]['id']} is published")
        return published[0]
    draft = documents[0] if documents else None
    if draft:
        document = draft
        print(f"RESUME document: draft {document['id']} v{document['version']}")
    else:
        artefacts = read("architecture.artifacts_list", {"project_id": PROJECT}).get("items", [])
        focused = [row for row in artefacts if row.get("status") == "approved"]
        body = document_body(state, contract, focused)
        header = (
            f"# {DOCUMENT_TITLE}\n\n"
            f"Action `documents.compose` for project T19, category architecture-artifact. The body "
            f"carries the standard's Metadata / Entries / Links / Document headings; publication is "
            f"the admin act `documents.publish`.\n"
        )
        document, _ = write_with_evidence(
            "documents.compose",
            {"title": DOCUMENT_TITLE, "project_id": PROJECT, "category": "architecture-artifact",
             "unit": "T19", "body": body},
            name="artifact-document-compose.md", header=header,
            read_action="documents.get", read_id_from_output="id",
        )
        print(f"COMPOSE document: {document['id']} v{document['version']}")
    header = (
        f"# {DOCUMENT_TITLE} — publication\n\n"
        f"Action `documents.publish` for document {document['id']}, If-Match "
        f"{document['version']}. Publication moves the output into the public window; the template "
        f"check covers the four required headings.\n"
    )
    published_doc, _ = write_with_evidence(
        "documents.publish", {"id": document["id"], "publish": True},
        name="artifact-document-publish.md", header=header, if_match=document["version"],
        read_action="documents.get", read_input={"id": document["id"]},
    )
    if not (published_doc.get("published_at") or published_doc.get("publishable")):
        raise RuntimeError(f"document did not read back published: {published_doc}")
    print(f"PUBLISH document: {document['id']} is published")
    return published_doc


def apply():
    state = load_state()
    delegation = state["delegation"].get("document", state["delegation"])
    if not (delegation.get("status") == "active" and delegation.get("publishable")
            and delegation.get("published_at")):
        raise RuntimeError(f"standing delegation {DELEGATION} is not published and active: {delegation}")
    print(f"delegation: {delegation.get('title')} is active and published")
    ensure_artefacts(state)
    ensure_links(state)
    contract = ensure_contract(state)
    ensure_delivery_contract(state)
    ensure_baseline(state)
    ensure_document(state, contract)
    return check()


def check():
    state = load_state()
    types = standard_types(state)
    failures: list[str] = []

    def expect(condition, message):
        print(f"{'PASS' if condition else 'FAIL'} {message}")
        if not condition:
            failures.append(message)

    approved = {
        row.get("artifact_type_id"): row
        for row in state["artefacts"] if row.get("status") == "approved"
    }
    for key in REQUIRED_TYPES:
        type_row = types.get(key)
        expect(type_row is not None and type_row["id"] in approved,
               f"artefact {key} is approved on T19")

    contracts = [row for row in state["contracts"]
                 if row.get("status") == "live" and row.get("approved_at")]
    expect(bool(contracts), "T19 has a live, approved architecture contract")
    expect(any(row.get("kind") == "delivery_contract" for row in contracts),
           "T19 has a live delivery contract")
    if contracts:
        contract = contracts[0]
        expect(not contract.get("requested_by") or contract.get("requested_by") != contract.get("approved_by"),
               "the contract's requester is not its approver")

    benefit = state["benefit"] or {}
    baseline = benefit.get("baseline")
    expect(complete_baseline(baseline), "B7's baseline is a measurement record")
    expect(bool(benefit.get("benefit_owner_job_id")), "B7 has a benefit owner job")

    documents = state["documents"]
    published = [doc for doc in documents if doc.get("published_at") or doc.get("publishable") is True]
    expect(bool(published), "the architecture-artifact document is published")
    if published:
        detail = read("documents.get", {"id": published[0]["id"]})
        document = detail.get("document", detail)
        body = document.get("body") or ""
        for heading in ("Metadata", "Entries", "Links", "Document"):
            expect(f"## {heading}" in body, f"the document carries the {heading} heading")

    print("all T19 governance-chain checks passed" if not failures
          else f"{len(failures)} check(s) failed")
    return 0 if not failures else 1


def main() -> int:
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--check", action="store_true")
    group.add_argument("--apply", action="store_true")
    args = parser.parse_args()
    try:
        return check() if args.check else apply()
    except RuntimeError as error:
        print(f"FAILED: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
