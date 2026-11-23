#!/usr/bin/env python3
"""Materialise conformance rework packages as real work packages (cards).

The register (`docs/design/data/rework.json`, `conveyor-rework/v1`) is published. This tool is
the *write* path: it turns its open packages into cards on the owning project's board through
the same action engine as `POST /api/actions/<name>/invocations`, using
`conveyor admin action invoke`. It is deliberately NOT called by `publish-data.sh`, whose
database transaction is READ ONLY.

Guarantees:
  - dry-run is the default; `--apply` is required to write;
  - idempotent: a package is skipped when the receipt records it or the board already carries a
    card whose `meta.rework_key` matches (checked with a live `cards.list`);
  - the receipt is written after EVERY create, so an interrupted run can be resumed;
  - a cleared package (materialised before, no longer in the register) is reported and only
    commented, never auto-archived;
  - a package whose `apply_to` project has no board fails loudly for that package.

Usage:
    python3 scripts/materialise-rework.py --register /tmp/rework.json --data /tmp/draft
    DATABASE_URL=postgres://… python3 scripts/materialise-rework.py --apply --severity P0
    DATABASE_URL=postgres://… python3 scripts/materialise-rework.py --apply --severity P1 --kind CODE
    python3 scripts/materialise-rework.py --self-test
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_REGISTER = ROOT / "docs" / "design" / "data" / "rework.json"
DEFAULT_DATA = ROOT / "docs" / "design" / "data"
DEFAULT_RECEIPT = ROOT / "docs" / "evidence" / "rework-apply.json"
SCHEMA = "conveyor-rework-apply/v1"
LIST_NAME = "Conformance rework"
LIST_KIND = "intake"
SEVERITIES = ("P0", "P1", "P2", "P3")
KINDS = ("CODE", "RECORD", "STANDARD")
CARD_TYPES = {
    "closed_without_closure_review": "project_closure",
    "TOGAF": "architecture_task",
}


# ── reading the published record ────────────────────────────────────────────────────


def read_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def write_json(path: Path, document: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text(
        json.dumps(document, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    temporary.replace(path)


class Snapshot:
    """The published (or draft) pack: project keys, their boards, and their lanes."""

    def __init__(self, data_dir: Path) -> None:
        programme = read_json(data_dir / "programme.json")
        work = read_json(data_dir / "work.json")
        self.projects: dict[str, dict[str, Any]] = {
            str(project["key"]): project
            for project in programme.get("projects", [])
            if isinstance(project, dict) and project.get("key")
        }
        self.boards_by_project: dict[str, list[dict[str, Any]]] = {}
        for board in work.get("boards", []):
            if isinstance(board, dict) and board.get("project_id"):
                self.boards_by_project.setdefault(str(board["project_id"]), []).append(board)
        for boards in self.boards_by_project.values():
            boards.sort(key=lambda board: (board.get("position", 0), str(board.get("id"))))
        self.lists = [lane for lane in work.get("lists", []) if isinstance(lane, dict)]

    def board_for(self, key: str) -> dict[str, Any] | None:
        project = self.projects.get(key)
        if not project:
            return None
        boards = self.boards_by_project.get(str(project.get("id")))
        return boards[0] if boards else None

    def lane_for(self, board_id: str, name: str) -> dict[str, Any] | None:
        for lane in self.lists:
            if str(lane.get("board_id")) == board_id and lane.get("name") == name:
                return lane
        return None


# ── rendering one package ───────────────────────────────────────────────────────────


def card_type_for(package: dict[str, Any]) -> str:
    specific = CARD_TYPES.get(str(package.get("rule")))
    if specific:
        return specific
    return CARD_TYPES.get(str(package.get("standard")), "development_chore")


def card_name(package: dict[str, Any]) -> str:
    name = f"{package['title']} [{package['scope']}]"
    return name if len(name) <= 200 else name[:197] + "..."


def card_description(package: dict[str, Any]) -> str:
    subjects = package.get("subjects") or []
    lines = [
        f"Rework package: {package['key']}",
        f"Standard: {package['standard']} {package['spec']} — artefact: {package['artefact']}",
        f"Severity: {package['severity']}",
        f"Owner: {package['owner_project']} (card on {package['apply_to']})",
        "",
        "Why this exists",
        *[f"- {record.get('subject')}: {record.get('detail')}" for record in subjects[:20]],
    ]
    if len(subjects) > 20:
        lines.append(f"- … and {len(subjects) - 20} more subject(s)")
    code = [
        item
        for record in subjects[:5]
        for item in (record.get("evidence") or [])
        if isinstance(item, dict) and item.get("source") == "code"
    ]
    if code:
        lines += ["", "Code evidence"]
        for item in code[:10]:
            lines.append(f"- {item.get('path')}:{item.get('line')} — {item.get('quote')}")
    lines += ["", "Definition of done"]
    for item in package.get("dod") or []:
        lines.append(f"- [ ] {item.get('item')}")
    lines += ["", f"Verify: {package.get('acceptance_command')}"]
    return "\n".join(lines)


def card_meta(package: dict[str, Any]) -> dict[str, Any]:
    """The card's identity and the package it answers; kept well under the 16 KiB limit."""
    return {
        "rework_key": package["key"],
        "rule": package["rule"],
        "standard": package["standard"],
        "spec": package["spec"],
        "artefact": package["artefact"],
        "severity": package["severity"],
        "scope": package["scope"],
        "owner_project": package["owner_project"],
        "apply_to": package["apply_to"],
        "subject_count": package["subject_count"],
        "subjects": [str(record.get("subject")) for record in (package.get("subjects") or [])][:50],
        "dod": [str(item.get("item")) for item in (package.get("dod") or [])][:20],
        "acceptance_command": package.get("acceptance_command"),
        "evidence": (package.get("evidence") or [])[:5],
    }


def create_payload(package: dict[str, Any], board_id: str, list_id: str) -> dict[str, Any]:
    return {
        "board_id": board_id,
        "list_id": list_id,
        "name": card_name(package),
        "description": card_description(package),
        "card_type_code": card_type_for(package),
        "meta": card_meta(package),
    }


# ── the receipt ─────────────────────────────────────────────────────────────────────


def empty_receipt(tenant: str) -> dict[str, Any]:
    return {
        "schema": SCHEMA,
        "tenant": tenant,
        "applied": {},
        "cleared": [],
        "failures": [],
        "updated_at": None,
    }


def load_receipt(path: Path, tenant: str) -> dict[str, Any]:
    if not path.exists():
        return empty_receipt(tenant)
    document = read_json(path)
    if document.get("schema") != SCHEMA:
        raise SystemExit(f"materialise-rework: {path} is not a {SCHEMA} receipt")
    document.setdefault("applied", {})
    document.setdefault("cleared", [])
    document.setdefault("failures", [])
    document["tenant"] = tenant
    return document


def stamp(receipt: dict[str, Any]) -> None:
    receipt["updated_at"] = datetime.now(timezone.utc).replace(microsecond=0).isoformat()


# ── planning ────────────────────────────────────────────────────────────────────────


@dataclass
class Item:
    package: dict[str, Any]
    action: str  # create | existing | skipped
    reason: str = ""
    board: dict[str, Any] | None = None
    lane: dict[str, Any] | None = None


def parse_severities(raw: str) -> set[str]:
    values = {item.strip().upper() for item in raw.split(",") if item.strip()}
    unknown = values - set(SEVERITIES)
    if unknown:
        raise SystemExit(f"materialise-rework: unknown severity {sorted(unknown)}")
    return values


def parse_kinds(raw: str | None) -> set[str] | None:
    """The kinds to materialise; None (the default) means every kind."""
    if raw is None:
        return None
    values = {item.strip().upper() for item in raw.split(",") if item.strip()}
    if not values:
        return None
    unknown = values - set(KINDS)
    if unknown:
        raise SystemExit(f"materialise-rework: unknown kind {sorted(unknown)}")
    return values


def select(
    register: dict[str, Any],
    snapshot: Snapshot,
    receipt: dict[str, Any],
    severities: set[str],
    kinds: set[str] | None,
    limit: int | None,
    reapply: set[str],
) -> list[Item]:
    items: list[Item] = []
    created = 0
    for package in register.get("packages", []):
        key = str(package.get("key"))
        if package.get("status") != "open":
            continue
        if str(package.get("severity")) not in severities:
            continue
        if kinds is not None and str(package.get("kind")) not in kinds:
            continue
        if key in receipt["applied"] and key not in reapply:
            items.append(Item(package, "existing", "already in the receipt"))
            continue
        board = snapshot.board_for(str(package.get("apply_to")))
        if board is None:
            items.append(Item(package, "skipped", f"no board for {package.get('apply_to')}"))
            continue
        lane = snapshot.lane_for(str(board.get("id")), LIST_NAME)
        if limit is not None and created >= limit:
            items.append(Item(package, "skipped", "limit reached"))
            continue
        created += 1
        items.append(Item(package, "create", "", board=board, lane=lane))
    return items


# ── the action engine ───────────────────────────────────────────────────────────────


class ActionError(RuntimeError):
    pass


def invoke(
    conveyor: str,
    tenant: str,
    database_url: str,
    name: str,
    payload: dict[str, Any],
    actor_id: str | None,
) -> Any:
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as handle:
        json.dump(payload, handle)
        path = handle.name
    command = [
        conveyor,
        "admin",
        "action",
        "invoke",
        "--tenant",
        tenant,
        "--name",
        name,
        "--input",
        path,
    ]
    if actor_id:
        command += ["--actor-id", actor_id]
    environment = os.environ.copy()
    environment["DATABASE_URL"] = database_url
    # `admin action invoke` opens its pool on the migration role (direct, not the pooler), so a
    # URL passed once must reach both variables or the CLI falls back to its compose default.
    environment["DATABASE_MIGRATE_URL"] = database_url
    try:
        completed = subprocess.run(
            command, capture_output=True, text=True, env=environment, timeout=120
        )
    finally:
        os.unlink(path)
    if completed.returncode != 0:
        raise ActionError(f"{name}: {completed.stderr.strip() or completed.stdout.strip()}")
    try:
        document = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise ActionError(f"{name}: the CLI did not return JSON ({error})") from error
    return document.get("output", document)


def live_lane(
    conveyor: str, tenant: str, database_url: str, board_id: str, actor_id: str | None
) -> dict[str, Any] | None:
    board = invoke(conveyor, tenant, database_url, "boards.get", {"id": board_id}, actor_id)
    for entry in board.get("lists", []):
        lane = entry.get("list") if isinstance(entry, dict) else None
        if lane and lane.get("name") == LIST_NAME:
            return lane
    return None


def live_reworks(
    conveyor: str, tenant: str, database_url: str, board_id: str, actor_id: str | None
) -> dict[str, str]:
    found: dict[str, str] = {}
    cursor: str | None = None
    for _ in range(50):  # a board with >10 000 rework cards is not a real board
        page: dict[str, Any] = {"limit": 200}
        if cursor:
            page["cursor"] = cursor
        document = invoke(
            conveyor,
            tenant,
            database_url,
            "cards.list",
            {"board_id": board_id, "page": page},
            actor_id,
        )
        for card in document.get("items", []):
            meta = card.get("meta") if isinstance(card, dict) else None
            if isinstance(meta, dict) and meta.get("rework_key"):
                found[str(meta["rework_key"])] = str(card.get("id"))
        cursor = document.get("next_cursor")
        if not cursor:
            break
    return found


# ── apply ───────────────────────────────────────────────────────────────────────────


def apply(
    items: list[Item],
    conveyor: str,
    tenant: str,
    database_url: str,
    receipt: dict[str, Any],
    receipt_path: Path,
    actor_id: str | None,
) -> dict[str, int]:
    counts = {"created": 0, "existing": 0, "skipped": 0, "failed": 0, "cleared": 0}
    lanes: dict[str, str] = {}
    live: dict[str, dict[str, str]] = {}
    for item in items:
        package = item.package
        key = str(package["key"])
        if item.action == "existing":
            counts["existing"] += 1
            continue
        if item.action == "skipped":
            counts["skipped"] += 1
            print(f"skip  {key}: {item.reason}")
            continue
        assert item.board is not None
        board_id = str(item.board["id"])
        try:
            if board_id not in lanes:
                lane = live_lane(conveyor, tenant, database_url, board_id, actor_id)
                if lane is None:
                    lane = invoke(
                        conveyor,
                        tenant,
                        database_url,
                        "lists.create",
                        {"board_id": board_id, "name": LIST_NAME, "kind": LIST_KIND},
                        actor_id,
                    )
                    print(f"list  {board_id}: created {LIST_NAME} ({lane.get('id')})")
                lanes[board_id] = str(lane["id"])
                live[board_id] = live_reworks(
                    conveyor, tenant, database_url, board_id, actor_id
                )
            lane_id = lanes[board_id]
            if key in live[board_id] and key not in {
                str(value) for value in receipt["applied"]
            }:
                receipt["applied"][key] = {
                    "card_id": live[board_id][key],
                    "apply_to": package["apply_to"],
                    "severity": package["severity"],
                    "at": datetime.now(timezone.utc).replace(microsecond=0).isoformat(),
                    "source": "existing-card",
                }
                stamp(receipt)
                write_json(receipt_path, receipt)
                counts["existing"] += 1
                continue
            payload = create_payload(package, board_id, lane_id)
            card = invoke(conveyor, tenant, database_url, "cards.create", payload, actor_id)
            card_id = str(card.get("id"))
            live[board_id][key] = card_id
            receipt["applied"][key] = {
                "card_id": card_id,
                "apply_to": package["apply_to"],
                "severity": package["severity"],
                "at": datetime.now(timezone.utc).replace(microsecond=0).isoformat(),
            }
            stamp(receipt)
            write_json(receipt_path, receipt)
            counts["created"] += 1
            print(f"create {key} -> {card_id} ({package['severity']}, {package['apply_to']})")
        except (ActionError, KeyError) as error:
            counts["failed"] += 1
            receipt["failures"].append({"key": key, "error": str(error)})
            stamp(receipt)
            write_json(receipt_path, receipt)
            print(f"FAIL  {key}: {error}", file=sys.stderr)
    return counts


def clear_comments(
    items: list[Item],
    register: dict[str, Any],
    receipt: dict[str, Any],
    receipt_path: Path,
    conveyor: str,
    tenant: str,
    database_url: str,
    actor_id: str | None,
    comment: bool,
) -> int:
    open_keys = {str(package.get("key")) for package in register.get("packages", [])}
    cleared = 0
    for key in sorted(set(receipt["applied"]) - open_keys):
        if key in receipt["cleared"]:
            continue
        record = receipt["applied"][key]
        if comment:
            try:
                invoke(
                    conveyor,
                    tenant,
                    database_url,
                    "cards.comment",
                    {
                        "id": record["card_id"],
                        "body": (
                            "The latest conformance audit reports no gap for this package. "
                            "Closing it is a human decision; nothing was archived automatically."
                        ),
                    },
                    actor_id,
                )
            except ActionError as error:
                print(f"FAIL  comment {key}: {error}", file=sys.stderr)
                continue
        receipt["cleared"].append(key)
        stamp(receipt)
        write_json(receipt_path, receipt)
        cleared += 1
        print(f"cleared {key} (card {record['card_id']})")
    return cleared


# ── self-test ───────────────────────────────────────────────────────────────────────


def self_test() -> None:
    with tempfile.TemporaryDirectory() as temporary:
        data = Path(temporary) / "data"
        data.mkdir()
        write_json(
            data / "programme.json",
            {
                "programme": [{"name": "P"}],
                "projects": [
                    {"id": "t1", "key": "T1", "status": "active"},
                    {"id": "t2", "key": "T2", "status": "closed"},
                ],
            },
        )
        write_json(
            data / "work.json",
            {
                "boards": [{"id": "b1", "project_id": "t1", "position": 0}],
                "lists": [{"id": "l1", "board_id": "b1", "name": "Intake"}],
            },
        )
        register = {
            "schema": "conveyor-rework/v1",
            "packages": [
                {
                    "key": "contract_missing:tranche-1",
                    "rule": "contract_missing",
                    "standard": "TOGAF",
                    "spec": "§8.1",
                    "artefact": "architecture_contract",
                    "title": "Put the project under an approved architecture contract",
                    "severity": "P1",
                    "kind": "RECORD",
                    "status": "open",
                    "scope": "tranche-1",
                    "subjects": [
                        {"subject": "T1", "detail": "no contract", "disposition": "rework"}
                    ],
                    "subject_count": 1,
                    "evidence": [{"source": "snapshot", "file": "architecture.json"}],
                    "dod": [{"item": "An approved contract exists", "met": False}],
                    "owner_project": "T9",
                    "apply_to": "T1",
                    "acceptance_command": "audit",
                },
                {
                    "key": "benefit_baseline_placeholder:programme",
                    "rule": "benefit_baseline_placeholder",
                    "standard": "MSP",
                    "spec": "§8.3",
                    "artefact": "benefit_profile.baseline",
                    "title": "Measure the benefit baselines",
                    "severity": "P1",
                    "kind": "RECORD",
                    "status": "suppressed",
                    "scope": "programme",
                    "subjects": [],
                    "subject_count": 0,
                    "evidence": [],
                    "dod": [],
                    "owner_project": "T12",
                    "apply_to": "PLATFORM",
                    "acceptance_command": "audit",
                },
                {
                    "key": "v1-x-1:T1",
                    "rule": "v1-x-1",
                    "standard": "SAFe",
                    "spec": "§9.4",
                    "artefact": "code",
                    "title": "Fix the loop",
                    "severity": "P0",
                    "kind": "CODE",
                    "status": "open",
                    "scope": "T1",
                    "subjects": [
                        {
                            "subject": "T1",
                            "detail": "the remedy is recorded, never applied",
                            "disposition": "rework",
                            "evidence": [
                                {
                                    "source": "code",
                                    "path": "crates/actions/src/coach.rs",
                                    "line": 738,
                                    "quote": "q",
                                }
                            ],
                        }
                    ],
                    "subject_count": 1,
                    "evidence": [],
                    "dod": [{"item": "The remedy is applied", "met": False}],
                    "owner_project": "T4",
                    "apply_to": "T1",
                    "acceptance_command": "audit",
                },
                {
                    "key": "closed_without_closure_review:T2",
                    "rule": "closed_without_closure_review",
                    "standard": "MSP",
                    "spec": "§8.4",
                    "artefact": "closure_review",
                    "title": "Publish the closure review",
                    "severity": "P0",
                    "kind": "RECORD",
                    "status": "open",
                    "scope": "T2",
                    "subjects": [],
                    "subject_count": 0,
                    "evidence": [],
                    "dod": [],
                    "owner_project": "T2",
                    "apply_to": "T2",
                    "acceptance_command": "audit",
                },
            ],
        }
        snapshot = Snapshot(data)
        receipt = empty_receipt("test")
        receipt["applied"]["already:done"] = {"card_id": "c0"}
        items = select(register, snapshot, receipt, {"P0", "P1"}, None, None, set())
        actions = [item.action for item in items]
        if actions != ["create", "create", "skipped"]:
            raise SystemExit(f"self-test: expected two creates and one skipped, got {actions}")
        first = items[0].package
        if card_type_for(first) != "architecture_task":
            raise SystemExit("self-test: TOGAF package did not map to architecture_task")
        if "Definition of done" not in card_description(first):
            raise SystemExit("self-test: the description omits the DoD")
        if card_meta(first)["rework_key"] != first["key"]:
            raise SystemExit("self-test: the meta omits rework_key")
        mandatory = [
            item.package["key"]
            for item in items
            if item.action == "create" and item.package["severity"] == "P0"
        ]
        if mandatory != ["v1-x-1:T1"]:
            raise SystemExit(f"self-test: the P0 plan was {mandatory}")
        missing = [
            item
            for item in select(register, snapshot, receipt, {"P0"}, None, None, set())
        ]
        if any(item.action == "create" and item.package["apply_to"] == "T2" for item in missing):
            raise SystemExit("self-test: a closed project's package was not skipped")
        suppressed = [
            item
            for item in select(register, snapshot, receipt, {"P1"}, None, None, set())
            if item.package["key"].startswith("benefit_baseline_placeholder")
        ]
        if suppressed:
            raise SystemExit("self-test: a suppressed package entered the plan")
        code_keys = [
            item.package["key"]
            for item in select(
                register, snapshot, receipt, {"P0"}, parse_kinds("CODE"), None, set()
            )
        ]
        if code_keys != ["v1-x-1:T1"]:
            raise SystemExit(f"self-test: --kind CODE planned {code_keys}")
        record_keys = [
            item.package["key"]
            for item in select(
                register, snapshot, receipt, {"P0"}, parse_kinds("RECORD"), None, set()
            )
        ]
        if "v1-x-1:T1" in record_keys:
            raise SystemExit("self-test: --kind RECORD kept a CODE package")
        if "closed_without_closure_review:T2" not in record_keys:
            raise SystemExit("self-test: --kind RECORD dropped a RECORD package")
        if parse_kinds(None) is not None:
            raise SystemExit("self-test: an absent --kind must select every kind")
    print("materialise-rework: self-test OK")


# ── CLI ─────────────────────────────────────────────────────────────────────────────


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--register", type=Path, default=DEFAULT_REGISTER)
    parser.add_argument("--data", type=Path, default=DEFAULT_DATA, help="the pack the register describes")
    parser.add_argument("--receipt", type=Path, default=DEFAULT_RECEIPT)
    parser.add_argument("--tenant", default=os.environ.get("CONVEYOR_TENANT", "nonagentic"))
    parser.add_argument("--database-url", default=os.environ.get("DATABASE_URL"))
    parser.add_argument("--conveyor", default=os.environ.get("CONVEYOR_BIN", "conveyor"))
    parser.add_argument("--actor-id", default=None)
    parser.add_argument("--severity", default="P0,P1,P2,P3")
    parser.add_argument(
        "--kind",
        default=None,
        help="comma list of package kinds to materialise (CODE, RECORD, STANDARD); default all",
    )
    parser.add_argument("--limit", type=int, default=None)
    parser.add_argument("--reapply", action="append", default=[])
    parser.add_argument("--apply", action="store_true", help="write; without it this is a dry run")
    parser.add_argument("--no-comment-cleared", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)

    if args.self_test:
        self_test()
        return 0

    register = read_json(args.register)
    if register.get("schema") != "conveyor-rework/v1":
        print(f"materialise-rework: {args.register} is not a conveyor-rework/v1 register", file=sys.stderr)
        return 2
    snapshot = Snapshot(args.data)
    receipt = load_receipt(args.receipt, args.tenant)
    items = select(
        register,
        snapshot,
        receipt,
        parse_severities(args.severity),
        parse_kinds(args.kind),
        args.limit,
        set(args.reapply),
    )
    creates = [item for item in items if item.action == "create"]
    print(
        f"materialise-rework: {len(creates)} to create, "
        f"{sum(1 for item in items if item.action == 'existing')} already applied, "
        f"{sum(1 for item in items if item.action == 'skipped')} skipped"
    )
    if not args.apply:
        for item in creates:
            print(
                f"would create {item.package['key']} on {item.package['apply_to']} "
                f"({item.package['severity']}, {card_type_for(item.package)})"
            )
        return 0
    if not args.database_url:
        print("materialise-rework: --apply needs --database-url or DATABASE_URL", file=sys.stderr)
        return 2
    counts = apply(
        items,
        args.conveyor,
        args.tenant,
        args.database_url,
        receipt,
        args.receipt,
        args.actor_id,
    )
    counts["cleared"] = clear_comments(
        items,
        register,
        receipt,
        args.receipt,
        args.conveyor,
        args.tenant,
        args.database_url,
        args.actor_id,
        not args.no_comment_cleared,
    )
    print(
        "materialise-rework: "
        + ", ".join(f"{name}={value}" for name, value in counts.items())
    )
    return 1 if counts["failed"] else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
