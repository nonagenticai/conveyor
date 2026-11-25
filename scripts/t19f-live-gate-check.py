#!/usr/bin/env python3
"""t19f-live-gate-check — perform and assert T19F's live acceptance on the deployed engine.

T19F's plan (docs/plans/T19F-meeting-bindings.md §P5) gates the bindings on three live rows:

  * a forum policy whose v2 is published through the authoring route (intake → draft → a human
    review approval → `authoring.publish`) while a meeting scheduled against v1 still resolves v1;
  * a board sitting on a real subject decided through `architecture.board_sit_decide`, whose
    approval carries a per-seat `approval_decision` row with `via.surface='meeting'`;
  * a cadence row bound to a meeting type, materialised twice, yielding exactly one meeting.

`--apply` performs the acts through `scripts/programme-record.py` (the deployed engine's action
surface, never SQL): every write names an evidence file first and reads its row back. `--check`
re-reads the recorded rows and exits non-zero when any invariant fails.

Transport: `scripts/programme-record.py`, so the operator environment is `CONVEYOR_SSH_KEY`,
`CONVEYOR_HOST` and `TENANT`.

Usage:
    python3 scripts/t19f-live-gate-check.py --apply
    python3 scripts/t19f-live-gate-check.py --check
    python3 scripts/t19f-live-gate-check.py --apply --project-id <uuid>

The project id is resolved from the published snapshot (`docs/design/data/programme.json`) like the
governance chain's; pass `--project-id` when the snapshot predates the seed.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RECORDER = ROOT / "scripts" / "programme-record.py"
SNAPSHOT = ROOT / "docs" / "design" / "data" / "programme.json"
EVIDENCE = ROOT / "docs" / "evidence" / "t19f" / "gate"
STATE = ROOT / "docs" / "evidence" / "t19f" / "live-gate.json"

TYPE_KEY = "t19f_live_gate_forum"
ASSET_KEY = "t19f-live-gate-policy-v2"
BODY_KEY = "t19f-live-gate-board"
CADENCE_PERIOD = "2026-PI-T19F"

POLICY_V1 = {
    "name": "T19F live-gate forum v1",
    "decision_rule": "chair",
    "quorum": 1,
    "agenda_sources": [],
    "required_participation": [],
    "evidence_minimum": 0,
}

POLICY_V2 = {
    "meeting_type": {
        "meeting_type_key": TYPE_KEY,
        "name": "T19F live-gate forum v2",
        "purpose": "The v2 published through the authoring route by the T19F live gate.",
        "decision_rule": "majority",
        "quorum": 1,
        "agenda_sources": [],
        "required_participation": [],
        "evidence_minimum": 0,
    }
}


def run_recorder(action, payload, *, evidence=None, actor=None, read_action=None,
                 read_input=None, read_id_from_output=None):
    command = [sys.executable, str(RECORDER), "--action", action,
               "--input", json.dumps(payload, separators=(",", ":"))]
    if evidence is not None:
        command += ["--evidence", str(evidence)]
    if actor:
        command += ["--actor-id", actor]
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


def write_with_evidence(action, payload, *, name, header, actor=None, read_action=None,
                        read_input=None, read_id_from_output=None):
    path = evidence_file(name, header)
    parts, raw = run_recorder(
        action, payload, evidence=path, actor=actor, read_action=read_action,
        read_input=read_input, read_id_from_output=read_id_from_output,
    )
    with path.open("a", encoding="utf-8") as handle:
        handle.write("\n```\n" + raw.rstrip() + "\n```\n")
    return parts.get("write", {}), parts.get("read-back", {})


def resolve_project(project_id: str | None) -> tuple[str, str]:
    if project_id:
        return project_id, ""
    if not SNAPSHOT.exists():
        raise RuntimeError(
            f"{SNAPSHOT} is missing; run docs/design/scripts/publish-data.sh after the seed, "
            f"or pass --project-id"
        )
    snapshot = json.loads(SNAPSHOT.read_text(encoding="utf-8"))
    row = next((p for p in snapshot.get("projects") or [] if p.get("key") == "T19F"), None)
    if row is None:
        raise RuntimeError(
            "the snapshot has no T19F project; run `migrate --seed` and publish-data.sh first, "
            "or pass --project-id"
        )
    return row["id"], row.get("programme_id") or ""


def resolve_base(project_id_arg: str | None) -> dict:
    project_id, programme_id = resolve_project(project_id_arg)
    project = read("projects.get", {"id": project_id})
    body = project.get("project", project)
    boards = project.get("boards", body.get("boards", []))
    if not boards:
        raise RuntimeError("the T19F project has no board; run `migrate --seed`")
    board_id = boards[0]["id"]
    board = read("boards.get", {"id": board_id})
    lists = board.get("lists", [])
    if not lists:
        raise RuntimeError("the T19F board has no lane to place a card in")
    list_id = lists[0]["list"]["id"]
    members = read("members.list", {}).get("items", [])
    approver = next(
        (m for m in members
         if "conveyor-approver" in (m.get("display_name") or "").lower()
         or "conveyor-approver" in (m.get("email") or "").lower()),
        None,
    )
    if approver is None:
        raise RuntimeError("the tenant has no conveyor-approver member (the distinct human seat)")
    operator = next((m for m in members if m["user_id"] != approver["user_id"]), None)
    if operator is None:
        raise RuntimeError("the tenant has only the approver; a sitting needs a distinct requester")
    return {
        "project_id": project_id,
        "programme_id": body.get("programme_id") or programme_id,
        "board_id": board_id,
        "list_id": list_id,
        "approver_id": approver["user_id"],
        "operator_id": operator["user_id"],
    }


def completed_state(state: dict) -> dict | None:
    """Refresh a recorded state when every live row it names already satisfies the gate.

    A completed run must be re-runnable (the issue's P2.2): the policy v2 is live, the sitting
    item closed as a decision, the cadence meeting carries its slot — so there is nothing to
    write but the state, and no second meeting/asset/approval may be created.
    """
    versions = read("meetings.type_list", {"key": TYPE_KEY}).get("items", [])
    v2 = next(
        (v for v in versions if v.get("version") == 2 and v.get("status") == "active"), None
    )
    if v2 is None:
        return None
    detail = read("meetings.get", {"meeting_id": state["sitting"]["meeting_id"]})
    if not any(i.get("outcome") == "decision" for i in detail.get("agenda", [])):
        return None
    materialised = read("meetings.get", {"meeting_id": state["cadence"]["meeting_id"]})
    meeting = materialised.get("meeting", materialised)
    if (
        meeting.get("meeting_type") != TYPE_KEY
        or meeting.get("cadence_slot") != state["cadence"]["cadence_id"]
        or meeting.get("status") == "cancelled"
    ):
        return None
    assets = read("authoring.assets", {"kind": "policy"}).get("items", [])
    asset = next((a for a in assets if a.get("key") == state["policy"]["asset_key"]), None)
    if asset is None or asset.get("status") != "published":
        return None
    approvals = read("approvals.list", {"project_id": state["project_id"]}).get("items", [])
    human = next(
        (
            a
            for a in approvals
            if a.get("gate") == "authoring.review"
            and a.get("target") == asset["id"]
            and (a.get("decisions") or [])
            and a["decisions"][-1].get("decision") == "approve"
            and a["decisions"][-1].get("decided_by_kind") == "human"
        ),
        None,
    )
    if human is None:
        return None
    refreshed = json.loads(json.dumps(state))
    refreshed["policy"]["review_blocked"] = None
    refreshed["policy"]["asset_id"] = asset["id"]
    refreshed["policy"]["v2_version"] = v2.get("version")
    refreshed["policy"]["v2_meeting_type_id"] = v2.get("id")
    refreshed["policy"]["human_approval_id"] = human["id"]
    return refreshed


def apply(project_id_arg: str | None) -> int:
    base = resolve_base(project_id_arg)
    print(f"project {base['project_id']} board {base['board_id']} list {base['list_id']}")
    recorded = json.loads(STATE.read_text(encoding="utf-8")) if STATE.exists() else None
    if recorded is not None:
        refreshed = completed_state(recorded)
        if refreshed is not None:
            STATE.write_text(json.dumps(refreshed, indent=2) + "\n", encoding="utf-8")
            print("RESUME: policy v2 live, sitting decided, cadence materialised; state refreshed")
            return check()

    # P1 — v1 exists with the schedule; a meeting pins it. A resumed run reuses the
    # meeting an earlier attempt already scheduled (meetings.get needs `meeting_id`, so the
    # recorder's read-back is the meetings list and the pin is read explicitly below).
    existing = read("meetings.list", {"limit": 200}).get("items", [])
    resumed = next(
        (m for m in existing
         if m.get("meeting_type") == TYPE_KEY and m.get("meeting_type_version") == 1),
        None,
    )
    if resumed is not None:
        v1 = resumed
        ev = EVIDENCE / "01-schedule-v1.md"
        ev.parent.mkdir(parents=True, exist_ok=True)
        if not ev.exists():
            ev.write_text("# T19F live gate — policy v1 and its pinned meeting\n")
        with ev.open("a", encoding="utf-8") as handle:
            handle.write(
                "\nResumed: this meeting was created by the first attempt before the "
                "read-back schema fix.\n```\n" + json.dumps(resumed, indent=2) + "\n```\n"
            )
        print(f"RESUME v1: meeting {v1['id']} is already pinned at version 1")
    else:
        v1, _ = write_with_evidence(
            "meetings.schedule",
            {"title": "T19F live gate — the meeting pinned at policy v1",
             "list_id": base["list_id"], "meeting_type_key": TYPE_KEY,
             "policy": POLICY_V1, "convener_kind": "app_user",
             "convener_id": base["operator_id"]},
            name="01-schedule-v1.md",
            header=(
                f"# T19F live gate — policy v1 and its pinned meeting\n\n"
                f"Action `meetings.schedule` creating meeting type `{TYPE_KEY}` v1 and a meeting "
                f"pinned to it; the meeting must still resolve v1 after v2 is published.\n"
            ),
            read_action="meetings.list", read_input={"limit": 5},
        )
    v1_meeting = v1["id"]
    print(f"v1: meeting {v1_meeting} pinned at version {v1.get('meeting_type_version')}")

    # P1 — v2 through the authoring route: intake → draft → review approval → review → publish.
    # Resumable: an asset that already exists under its key is reused; an approval already
    # requested for it is reused. `authoring.review` requires the approval's LAST HUMAN decision
    # (a service/rule decision is refused by design), so the act is attempted and, when only the
    # machine decision exists, recorded as BLOCKED rather than forged — the sitting and cadence
    # steps below do not depend on it.
    assets = read("authoring.assets", {"kind": "policy"}).get("items", [])
    asset = next((a for a in assets if a.get("key") == ASSET_KEY), None)
    if asset is None:
        intake, _ = write_with_evidence(
            "authoring.intake",
            {"brief": "Publish the T19F live-gate forum policy v2 through the reviewed authoring route.",
             "sources": [{"kind": "document", "ref": "docs/plans/T19F-meeting-bindings.md"}]},
            name="02-intake.md",
            header="# T19F live gate — the policy intake\n\nAction `authoring.intake`.\n",
            read_action="authoring.intakes",
        )
        draft, _ = write_with_evidence(
            "authoring.draft",
            {"intake_id": intake["id"], "kind": "policy", "key": ASSET_KEY, "payload": POLICY_V2},
            name="03-draft.md",
            header=(
                f"# T19F live gate — the policy v2 draft\n\nAction `authoring.draft`, kind "
                f"`policy`, key `{ASSET_KEY}`, payload carrying meeting_type `{TYPE_KEY}`.\n"
            ),
            read_action="authoring.assets", read_input={"kind": "policy"},
        )
        asset = draft
    else:
        print(f"RESUME policy: asset {asset['id']} exists ({asset.get('status')})")
    asset_id = asset["id"]
    approvals = read("approvals.list", {"project_id": base["project_id"]}).get("items", [])

    def human_approve(candidate: dict) -> bool:
        """The gate whose latest decision is a human approve (the only kind authoring.review takes)."""
        decisions = candidate.get("decisions") or []
        latest = decisions[-1] if decisions else None
        return bool(latest) and latest.get("decision") == "approve" and latest.get(
            "decided_by_kind"
        ) == "human"

    matching = [
        a for a in approvals
        if a.get("gate") == "authoring.review" and a.get("target") == asset_id
    ]
    eligible = [a for a in matching if human_approve(a)]
    if eligible:
        # The newest human-decided gate wins; a service-decided gate stays as history only.
        approval_id = eligible[-1]["id"]
        print(f"REVIEW GATE: {approval_id} carries the human decision")
    else:
        if not matching:
            request, _ = write_with_evidence(
                "approvals.request",
                {"project_id": base["project_id"], "gate": "authoring.review", "kind": "gate",
                 "description": f"Human review of the T19F live-gate policy asset {asset_id}",
                 "target": asset_id},
                name="04-review-request.md",
                header=(
                    "# T19F live gate — the review gate\n\nAction `approvals.request`, gate "
                    "`authoring.review`, requested by the distinct `conveyor-approver` seat so "
                    "a human can decide it.\n"
                ),
                actor=base["approver_id"],
                read_action="approvals.list", read_input={"project_id": base["project_id"]},
            )
            pending = request["id"]
        else:
            pending = matching[-1]["id"]
        # The admin CLI records `service`, which authoring.review refuses by design. A person
        # decides the gate in the signed-in console; this script never records that decision.
        raise RuntimeError(
            "the authoring.review gate has no human decision; approve gate "
            f"{pending} in the signed-in console, then re-run --apply"
        )
    reviewed = asset.get("status") in ("reviewed", "published")
    blocked_review = None
    if not reviewed:
        try:
            write_with_evidence(
                "authoring.review",
                {"asset_id": asset_id, "approval_id": approval_id},
                name="06-authoring-review.md",
                header=(
                    f"# T19F live gate — the authoring review\n\nAction `authoring.review` for "
                    f"asset {asset_id}, citing the approved gate {approval_id}.\n"
                ),
                read_action="authoring.assets", read_input={"kind": "policy"},
            )
            reviewed = True
        except RuntimeError as error:
            detail = str(error)
            if "approval_missing" not in detail and "does not match the action schema" not in detail:
                raise
            blocked_review = (
                "authoring.review refused: the approval has no human decision (the admin CLI "
                "records `service`); a person must approve it in the console, then re-run"
            )
            print(f"BLOCKED policy v2: {blocked_review}")
            ev = EVIDENCE / "06-authoring-review.md"
            with ev.open("a", encoding="utf-8") as handle:
                handle.write(
                    "\nBLOCKED — the review needs a human decision. The approval was decided by "
                    "the service actor; the product refuses a machine-only review.\n\n```\n"
                    + detail.rstrip() + "\n```\n"
                )
    published = {}
    if reviewed:
        published, _ = write_with_evidence(
            "authoring.publish",
            {"asset_id": asset_id},
            name="07-authoring-publish.md",
            header=(
                f"# T19F live gate — publishing the policy v2\n\nAction `authoring.publish` for "
                f"asset {asset_id}; the hook writes the next immutable `{TYPE_KEY}` version.\n"
            ),
            read_action="authoring.assets", read_input={"kind": "policy"},
        )
        print(f"v2: asset {asset_id} published as meeting_type version "
              f"{published.get('published_meeting_type_id')}")
    pinned = read("meetings.get", {"meeting_id": v1_meeting})
    pinned_meeting = pinned.get("meeting", pinned)
    if pinned_meeting.get("meeting_type_version") != 1:
        raise RuntimeError(
            f"the meeting pinned at v1 now resolves v{pinned_meeting.get('meeting_type_version')}"
        )
    versions = read("meetings.type_list", {"key": TYPE_KEY})
    version_rows = versions.get("items", [])
    if reviewed and len(version_rows) < 2:
        raise RuntimeError(f"the type list does not carry two versions: {versions}")
    print(f"pinned: meeting {v1_meeting} still resolves v1; the type list carries "
          f"{len(version_rows)} version row(s)")

    # P2 — the board sitting on a real subject.
    body, _ = write_with_evidence(
        "architecture.board_open",
        {"kind": "architecture_board", "key": BODY_KEY, "name": "T19F live-gate board",
         "charter": "Decides the T19F live-gate subject in a sitting.",
         "meeting_type_key": TYPE_KEY},
        name="08-board-open.md",
        header=(
            f"# T19F live gate — the sitting body\n\nAction `architecture.board_open`, key "
            f"`{BODY_KEY}`, bound to forum type `{TYPE_KEY}`.\n"
        ),
        read_action="architecture.board_get", read_input={"key": BODY_KEY},
    )
    body_id = body["id"]
    write_with_evidence(
        "architecture.board_decide",
        {"body_id": body_id, "decision": "approved", "subject_kind": "board"},
        name="09-board-activate.md",
        header=f"# T19F live gate — activating the body\n\nAction `architecture.board_decide`.\n",
        actor=base["approver_id"],
        read_action="architecture.board_get", read_input={"id": body_id},
    )
    subject, _ = write_with_evidence(
        "architecture.contracts_request",
        {"project_id": base["project_id"], "kind": "delivery_contract", "phase_code": "f",
         "title": "T19F live gate — the sitting subject"},
        name="10-sitting-subject.md",
        header="# T19F live gate — the sitting subject\n\nAction `architecture.contracts_request`.\n",
        read_action="architecture.contracts_get", read_id_from_output="id",
    )
    subject_id = subject["id"]
    sitting, _ = write_with_evidence(
        "architecture.board_sit_open",
        {"body_id": body_id, "subject_kind": "contract", "subject_id": subject_id,
         "list_id": base["list_id"],
         "evidence": [{"kind": "document", "ref": "docs/plans/T19F/architecture-design.md"}]},
        name="11-sitting-open.md",
        header=(
            f"# T19F live gate — the sitting opens\n\nAction `architecture.board_sit_open` on "
            f"contract {subject_id}: a pending approval, a live meeting and a deciding item.\n"
        ),
        read_action="architecture.board_get", read_input={"id": body_id},
    )
    meeting_id = sitting["meeting_id"]
    sit_approval = sitting["approval_id"]
    write_with_evidence(
        "meetings.invite",
        {"meeting_id": meeting_id, "participant_kind": "app_user",
         "participant_id": base["approver_id"], "seat": "chair"},
        name="12-sitting-invite.md",
        header=f"# T19F live gate — the chair seat\n\nAction `meetings.invite`.\n",
        read_action="meetings.get", read_input={"meeting_id": meeting_id},
    )
    write_with_evidence(
        "meetings.respond",
        {"meeting_id": meeting_id, "participant_kind": "app_user",
         "participant_id": base["approver_id"], "response": "accepted"},
        name="13-sitting-respond.md",
        header=f"# T19F live gate — the chair accepts\n\nAction `meetings.respond`.\n",
        actor=base["approver_id"],
        read_action="meetings.get", read_input={"meeting_id": meeting_id},
    )
    detail = read("meetings.get", {"meeting_id": meeting_id})
    items = detail.get("agenda", [])
    if not items:
        raise RuntimeError("the sitting meeting carries no agenda item")
    item_id = items[0]["id"]
    write_with_evidence(
        "meetings.position",
        {"meeting_id": meeting_id, "agenda_item_id": item_id,
         "participant_kind": "app_user", "participant_id": base["approver_id"],
         "stance": "approve", "surface": "web"},
        name="14-sitting-position.md",
        header=(
            f"# T19F live gate — the chair's position\n\nAction `meetings.position` on item "
            f"{item_id}, stance `approve`, surface `web`.\n"
        ),
        actor=base["approver_id"],
        read_action="meetings.get", read_input={"meeting_id": meeting_id},
    )
    decided, _ = write_with_evidence(
        "architecture.board_sit_decide",
        {"body_id": body_id, "meeting_id": meeting_id, "agenda_item_id": item_id},
        name="15-sitting-decide.md",
        header=(
            f"# T19F live gate — the sitting decides\n\nAction "
            f"`architecture.board_sit_decide`; the item closes through T19's close_item_tx.\n"
        ),
        read_action="architecture.board_get", read_input={"id": body_id},
    )
    print(f"sitting: approval {sit_approval} -> {decided.get('approval_status')} with "
          f"{decided.get('decisions_written')} decision row(s)")

    # P3 — the cadence row materialises its meeting once.
    cadence, _ = write_with_evidence(
        "cadences.create",
        {"programme_id": base["programme_id"], "kind": "pi", "period": CADENCE_PERIOD,
         "meeting_type_key": TYPE_KEY},
        name="16-cadence-create.md",
        header=(
            f"# T19F live gate — the cadence row\n\nAction `cadences.create`, period "
            f"`{CADENCE_PERIOD}`, bound to `{TYPE_KEY}`.\n"
        ),
        read_action="cadences.list", read_input={"programme_id": base["programme_id"]},
    )
    cadence_id = cadence["id"]
    first, _ = write_with_evidence(
        "cadences.materialise", {"id": cadence_id, "list_id": base["list_id"]},
        name="17-cadence-materialise.md",
        header=f"# T19F live gate — materialising the cadence\n\nAction `cadences.materialise`.\n",
        read_action="meetings.list", read_input={"limit": 5},
    )
    second, _ = write_with_evidence(
        "cadences.materialise", {"id": cadence_id, "list_id": base["list_id"]},
        name="18-cadence-materialise-again.md",
        header=(
            "# T19F live gate — materialising the cadence again\n\nAction "
            "`cadences.materialise`; the second call must return the first meeting.\n"
        ),
        read_action="meetings.list", read_input={"limit": 5},
    )
    if first.get("id") != second.get("id"):
        raise RuntimeError(
            f"materialise is not idempotent: {first.get('id')} then {second.get('id')}"
        )
    print(f"cadence: {cadence_id} materialised as meeting {first.get('id')} twice")

    v2_row = next(
        (v for v in version_rows if v.get("version") == 2 and v.get("status") == "active"), None
    )
    state = {
        **base,
        "policy": {"type_key": TYPE_KEY, "asset_key": ASSET_KEY, "asset_id": asset_id,
                   "v1_meeting_id": v1_meeting,
                   "v2_version": (v2_row or {}).get("version"),
                   "v2_meeting_type_id": (v2_row or {}).get("id"),
                   "review_blocked": blocked_review},
        "sitting": {"body_id": body_id, "contract_id": subject_id, "meeting_id": meeting_id,
                    "agenda_item_id": item_id, "approval_id": sit_approval},
        "cadence": {"cadence_id": cadence_id, "meeting_id": first.get("id")},
    }
    STATE.parent.mkdir(parents=True, exist_ok=True)
    STATE.write_text(json.dumps(state, indent=2) + "\n", encoding="utf-8")
    print(f"state: {STATE}")
    return check()


def check() -> int:
    if not STATE.exists():
        raise RuntimeError(f"{STATE} is missing; run --apply first")
    state = json.loads(STATE.read_text(encoding="utf-8"))
    failures: list[str] = []

    def expect(condition, message):
        print(f"{'PASS' if condition else 'FAIL'} {message}")
        if not condition:
            failures.append(message)

    project = read("projects.get", {"id": state["project_id"]})
    expect(project.get("project", project).get("key") == "T19F",
           "the live project row is T19F")

    pinned = read("meetings.get", {"meeting_id": state["policy"]["v1_meeting_id"]})
    pinned_meeting = pinned.get("meeting", pinned)
    expect(pinned_meeting.get("meeting_type_version") == 1,
           f"the v1-pinned meeting still resolves version 1 "
           f"(got {pinned_meeting.get('meeting_type_version')!r})")
    expect(pinned_meeting.get("meeting_type") == state["policy"]["type_key"],
           "the pinned meeting names the forum type")

    versions = read("meetings.type_list", {"key": state["policy"]["type_key"]})
    rows = versions.get("items", [])
    if state["policy"].get("review_blocked"):
        expect(False, "the policy v2 is published after a human review: "
                      + state["policy"]["review_blocked"])
    else:
        expect(len(rows) >= 2, f"the forum type carries v1 and v2 (got {len(rows)} row(s))")
        expect(any(v.get("version") == 2 and v.get("status") == "active" for v in rows),
               "the v2 policy is an active version")

    approval = None
    # The sitting's approval is tenant-scoped (its subject lives in meta), not project-scoped.
    approvals = read("approvals.list", {}).get("items", [])
    approval = next((a for a in approvals if a.get("id") == state["sitting"]["approval_id"]), None)
    expect(approval is not None, "the sitting's approval is a row")
    if approval is not None:
        expect(approval.get("status") == "approved",
               f"the sitting's approval is approved (got {approval.get('status')!r})")
        rows = approval.get("decisions") or []
        meeting_rows = [d for d in rows if (d.get("via") or {}).get("surface") == "meeting"]
        expect(len(meeting_rows) == 1,
               f"one per-seat decision row carries via.surface='meeting' (got {len(meeting_rows)})")
        if meeting_rows:
            expect(meeting_rows[0].get("approver_id") == state["approver_id"],
                   "the meeting decision row names the chair seat")

    sitting = read("meetings.get", {"meeting_id": state["sitting"]["meeting_id"]})
    deciding = [i for i in sitting.get("agenda", []) if i.get("outcome") == "decision"]
    expect(len(deciding) == 1, f"the sitting item closed as decision (got {len(deciding)})")

    materialised = read("meetings.get", {"meeting_id": state["cadence"]["meeting_id"]})
    cadence_meeting = materialised.get("meeting", materialised)
    expect(cadence_meeting.get("meeting_type") == state["policy"]["type_key"],
           "the materialised meeting names the cadence's forum type")
    expect(cadence_meeting.get("cadence_slot") == state["cadence"]["cadence_id"],
           "the materialised meeting carries the cadence slot")
    expect(cadence_meeting.get("status") != "cancelled",
           "the materialised meeting is non-cancelled")

    print("all T19F live-gate checks passed" if not failures
          else f"{len(failures)} check(s) failed")
    return 0 if not failures else 1


def main() -> int:
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--apply", action="store_true")
    group.add_argument("--check", action="store_true")
    parser.add_argument("--project-id", help="override the resolved T19F project id")
    args = parser.parse_args()
    try:
        if args.check:
            return check()
        return apply(args.project_id)
    except RuntimeError as error:
        print(f"FAILED: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
