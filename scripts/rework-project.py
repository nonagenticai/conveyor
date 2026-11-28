#!/usr/bin/env python3
"""Rework one project through the retrospective MSP/TOGAF/SAFe lifecycle.

One project per invocation. Every write goes through `scripts/programme-record.py` (the deployed
engine's action layer, or a branch-built binary for the actions and categories the deployed image
does not serve yet); every step reads the live record first and skips what is already there, so a
re-run resumes rather than duplicating. Nothing here writes SQL, a seed or a timestamp.

The content (charter, Statement of Architecture Work, the A–D artefacts, the delivery contract,
the Phase-G review, the closure record and the closure review) comes from a JSON file authored
for the project — this runner never invents it.

Usage:
    python3 scripts/rework-project.py --project S00 --content /tmp/rework-content-S00.json
    python3 scripts/rework-project.py --project S00 --content ... --dry-run

Environment: CONVEYOR_SSH_KEY, CONVEYOR_HOST (the deployed transport), PROGRAMME_RECORD_BIN and
SNAPSHOT_DATABASE_URL (the branch binary and the read-only database for the review read-back),
TENANT (default nonagentic), CONVEYOR_E2E and CONVEYOR_APPROVER (the two delegated identities).
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from private_refs import redact  # noqa: E402
from rework_register import RegisterError, build as build_rework_register  # noqa: E402

HARNESS = ROOT / "scripts" / "programme-record.py"
PROGRAMME = "01a1d039-0d24-7d16-ae52-4841e96ccfac"
DELEGATION = "01a1ff1f-e4ba-72dc-9c1a-13049933cf14"
TRANCHE_ZERO = "01a1e9e0-ced2-7769-b12f-f6fba0997914"
DEMO_CADENCE = "01a1ff21-7354-7777-996b-4c8cb12f25e9"
# The adopted profile's seven A–D artefacts, in the order the content file carries them
# (`scripts/rework-content-check.py` refuses any other order). The key binds each row to the
# standard's `artifact_type`, which is what makes the artefact resolvable by the conformance
# audit; a row without the binding is invisible to it.
ARTEFACT_KEYS = [
    "stakeholder_map",
    "driver_goal_objective",
    "requirements_catalog",
    "solution_concept",
    "business_capability_map",
    "data_entity_catalog",
    "technology_standards",
]
# The programme (architecture) track also carries E–H (owner tailoring 2026-11-24; migration
# 0102). They follow the seven A–D artefacts in the standard's seeded order.
ARCHITECTURE_ARTEFACT_KEYS = [
    "gap_analysis",
    "candidate_solutions",
    "project_context_diagram",
    "benefits_diagram",
    "implementation_migration_plan",
    "compliance_review",
    "architecture_change_request",
    "architecture_requirement",
]
BIN = os.environ.get("PROGRAMME_RECORD_BIN", "")
E2E = os.environ.get("CONVEYOR_E2E", "01a1e607-17d9-77d3-98d3-a43a87c4633d")
APPROVER = os.environ.get("CONVEYOR_APPROVER", "01a1e8a7-8b23-76a9-9114-aafb95f7a545")
TENANT = os.environ.get("TENANT", "nonagentic")


def fail(message: str) -> "NoReturn":  # noqa: F821 - a deliberate abort
    print(f"rework: {message}", file=sys.stderr)
    raise SystemExit(1)


def now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


class DryValue(dict):
    """A dry-run result: any field is the placeholder, so the flow can be inspected whole."""

    def __getitem__(self, key):  # noqa: D105 - a stub
        return "dry-run"

    def get(self, key, default=None):  # noqa: D105 - a stub
        return "dry-run"


class Runner:
    def __init__(
        self,
        project: str,
        content: dict,
        evidence: Path,
        dry_run: bool,
        demo_cadence: str = DEMO_CADENCE,
        delegation: str = DELEGATION,
        tranche_label: str = "tranche 0",
        acts_path: Path | None = None,
        today: str | None = None,
        keep_active: bool = False,
        closure_asset: str | None = None,
    ) -> None:
        self.project = project
        self.content = content
        self.evidence = evidence
        self.dry_run = dry_run
        self.demo_cadence = demo_cadence
        self.delegation = delegation
        self.tranche_label = tranche_label
        self.acts_path = acts_path or evidence
        self.today = today or datetime.now(timezone.utc).strftime("%Y-%m-%d")
        self.keep_active = keep_active
        self.closure_asset = closure_asset
        self.transcript = evidence.parent / "transcript.jsonl"
        self.snapshot = ""
        self.baseline_approval: str | None = None
        self.board = ""
        self.programme_id = ""
        self.head = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True
        ).stdout.strip()
        self.acts = 0

    # ── labels ──────────────────────────────────────────────────────────────────────

    def reopen_reason(self) -> str:
        return f"the retrospective MSP/TOGAF/SAFe rework of {self.tranche_label} (#526)"

    def close_reason(self) -> str:
        return f"the retrospective rework of {self.tranche_label} (#526)"

    def acts_ref(self) -> str:
        try:
            return str(self.acts_path.relative_to(ROOT))
        except ValueError:
            return str(self.acts_path)

    def closes_project(self) -> bool:
        """A project with an open issue is reworked but never closed (#526's demotion policy)."""
        return not self.keep_active

    # ── transport ───────────────────────────────────────────────────────────────────

    def invoke(
        self,
        action: str,
        payload: dict,
        *,
        actor: str = E2E,
        read_action: str | None = None,
        read_input: dict | None = None,
        read_id_from_output: str | None = None,
        read_match: list[str] | None = None,
        if_match: int | None = None,
        approval_id: str | None = None,
        use_snapshot: bool = False,
        snapshot_command: str | None = None,
        local: bool = False,
    ) -> dict:
        argv = [
            sys.executable, "-I", str(HARNESS), "--quiet",
            "--action", action,
            "--input", json.dumps(payload),
            "--evidence", str(self.evidence),
            "--actor-id", actor,
        ]
        if read_action:
            argv += ["--read-action", read_action]
        if read_input:
            argv += ["--read-input", json.dumps(read_input)]
        if read_id_from_output:
            argv += ["--read-id-from-output", read_id_from_output]
        for item in read_match or []:
            argv += ["--read-match", item]
        if if_match is not None:
            argv += ["--if-match", str(if_match)]
        if approval_id:
            argv += ["--approval-id", approval_id]
        if use_snapshot:
            if not self.snapshot:
                fail(f"{self.project}: a snapshot is required for {action}")
            argv += ["--snapshot", self.snapshot]
        if snapshot_command:
            argv += ["--snapshot-command", snapshot_command]
        if local:
            if not BIN:
                fail("PROGRAMME_RECORD_BIN is required for a branch-only action")
            argv += ["--local-bin", BIN]
        if self.dry_run:
            print(f"DRY {action} input={json.dumps(payload)[:160]}")
            return DryValue()
        completed = subprocess.run(argv, cwd=ROOT, capture_output=True, text=True)
        if completed.returncode != 0:
            fail(
                f"{action} refused at act {self.acts + 1}: "
                f"{completed.stderr.strip()[:600]}{completed.stdout.strip()[:300]}"
            )
        self.acts += 1
        result = json.loads(completed.stdout)
        with self.transcript.open("a", encoding="utf-8") as handle:
            handle.write(
                json.dumps(
                    redact(
                        {
                            "at": now(),
                            "act": self.acts,
                            "action": action,
                            "actor": "e2e" if actor == E2E else "approver",
                            "input": payload,
                            "result_id": result.get("id") or result.get("baseline_id"),
                        }
                    ),
                    sort_keys=True,
                )
                + "\n"
            )
        print(f"  {self.acts:02d} {action} ok")
        return result

    # ── reads ───────────────────────────────────────────────────────────────────────

    def _read(self, action: str, payload: dict) -> dict:
        argv = [
            sys.executable, "-I", str(HARNESS), "--check", "--quiet",
            "--action", action, "--input", json.dumps(payload),
        ]
        completed = subprocess.run(argv, cwd=ROOT, capture_output=True, text=True)
        if completed.returncode != 0:
            fail(f"read {action} failed: {completed.stderr.strip()[:400]}")
        return json.loads(completed.stdout)

    def snapshot_now(self, label: str) -> str:
        database = os.environ.get("SNAPSHOT_DATABASE_URL")
        if not database:
            fail("SNAPSHOT_DATABASE_URL is required to build the review read-back snapshot")
        directory = Path(
            f"{Path.home()}/conveyor-evidence/rework-{self.project}-{label}-"
            f"{datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')}"
        )
        (directory / "data").mkdir(parents=True, exist_ok=True)
        (directory / "docs").mkdir(parents=True, exist_ok=True)
        completed = subprocess.run(
            [
                BIN, "snapshot", "--tenant", TENANT,
                "--out", str(directory / "data"),
                "--outputs-dir", str(directory / "docs"),
                "--git-sha", self.head,
                "--issues", "docs/design/data/issues.json",
                "--database-url", database,
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        if completed.returncode != 0:
            fail(f"snapshot failed: {completed.stderr.strip()[:400]}")
        self.snapshot_root = str(directory)
        self.snapshot = str(directory / "data")
        return self.snapshot

    def snapshot_json(self, name: str) -> dict:
        return json.loads((Path(self.snapshot) / name).read_text(encoding="utf-8"))

    def artefact_keys(self) -> list[str]:
        """The project's artefact set: A–D for delivery, A–H for the architecture track.

        Same rule as the generator and the audit: `meta.adm_profile` wins when it names an
        architecture profile; otherwise `meta.track == "programme"` (owner tailoring 2026-11-24).
        """
        keys = list(ARTEFACT_KEYS)
        for row in self.snapshot_json("programme.json").get("projects") or []:
            if row.get("key") != self.project:
                continue
            meta = row.get("meta") or {}
            named = str(meta.get("adm_profile") or "").strip().lower()
            architecture = (
                "architecture" in named or named in {"programme", "a-h", "ah"}
                if named
                else str(meta.get("track") or "").strip().lower() == "programme"
            )
            if architecture:
                keys += ARCHITECTURE_ARTEFACT_KEYS
            break
        return keys

    # ── rework cards ────────────────────────────────────────────────────────────────
    #
    # The audit materialises every open gap as a card on the owning project's board
    # (`scripts/materialise-rework.py`), and `projects.close` refuses while a P0 card is open
    # (`crates/actions/src/programme.rs`). Nothing moved those cards when the factory performed
    # their work, so a project whose receipt was clear could not close. The discharge is the
    # missing driver: the card's own Definition of Done is the audit's clearance of its package,
    # read from a snapshot taken after the factory's writes.

    def board_cards(self, board_id: str) -> list[dict]:
        cards: list[dict] = []
        cursor = None
        for _ in range(50):  # a board with more than 10 000 cards is not a real board
            page: dict = {"limit": 200}
            if cursor:
                page["cursor"] = cursor
            document = self._read("cards.list", {"board_id": board_id, "page": page})
            cards.extend(document.get("items") or [])
            cursor = document.get("next_cursor")
            if not cursor:
                break
        return cards

    def owned_rework_cards(self) -> list[dict]:
        """The project's open rework cards, wherever the register materialised them.

        A package is materialised on its `apply_to` project's board — `owner_project` names the
        subject the close gate reads, so DESIGN-PUBLISH's cards sit on PLATFORM's board. The
        snapshot projects the cards but not their meta, so the cards are listed live; the
        boards are exactly the ones carrying a `Conformance rework` lane.
        """
        work = self.snapshot_json("work.json")
        lanes = work.get("lists") or []
        kinds_by_id = {
            str(lane.get("id")): str(lane.get("kind") or "") for lane in lanes
        }
        lanes_by_board: dict[str, dict[str, dict]] = {}
        board_ids = set()
        for lane in lanes:
            board_id = str(lane.get("board_id") or "")
            lanes_by_board.setdefault(board_id, {}).setdefault(
                str(lane.get("kind") or ""), lane
            )
            if str(lane.get("name") or "") == "Conformance rework":
                board_ids.add(board_id)
        found: list[dict] = []
        for board_id in sorted(board_ids):
            active = lanes_by_board.get(board_id, {}).get("active")
            done = lanes_by_board.get(board_id, {}).get("done")
            if active is None or done is None:
                continue
            for card in self.board_cards(board_id):
                meta = card.get("meta") or {}
                key = str(meta.get("rework_key") or "")
                if not key or str(meta.get("owner_project") or "") != self.project:
                    continue
                if kinds_by_id.get(str(card.get("list_id") or ""), "") not in {
                    "intake", "active", "waiting",
                }:
                    continue
                found.append({"card": card, "key": key, "active": active, "done": done})
        return found

    def approved_card_done(self, card_id: str) -> str | None:
        approvals = self._read(
            "approvals.list", {"card_id": card_id, "status": "approved"}
        ).get("items") or []
        for approval in approvals:
            if approval.get("gate") == "card_done":
                return str(approval.get("id"))
        return None

    def discharge_card(self, card: dict, key: str, active: dict, done: dict) -> None:
        """Comment the evidence, approve the Definition of Done, move intake→active→done."""
        card_id = str(card["id"])
        body = (
            f"Rework package `{key}` is cleared. The factory rebuilt the conformance register "
            f"from its post-write snapshot and the fresh standards findings; the register "
            f"carries no package for this key, and the package's own acceptance rule is the "
            f"gap's disappearance from that audit. The card's Definition of Done is met by the "
            f"record, not by this comment. Cleared by the retrospective rework of "
            f"{self.tranche_label} (#526)."
        )
        approval = self.approved_card_done(card_id)
        if approval is None:
            payload: dict = {
                "card_id": card_id,
                "gate": "card_done",
                "kind": "card_dod",
                "description": body,
            }
            if key.startswith("closed_without_closure_review") and self.closure_asset:
                payload["evidence"] = [
                    {"kind": "governed_asset", "id": self.closure_asset}
                ]
            else:
                payload["require_evidence"] = True
                payload["evidence_waiver_reason"] = (
                    f"the rework package {key} is a materialised view of its audit finding; "
                    f"the fresh audit of the project's post-write record reports no open "
                    f"package for this key, and that clearance is the card's Definition of Done"
                )
            approval = str(
                self.invoke(
                    "approvals.request",
                    payload,
                    read_action="approvals.list",
                    read_input={"card_id": card_id},
                    read_match=["gate=card_done", "status=pending"],
                )["id"]
            )
            self.decide(approval, "approve the card's Definition of Done")
        self.invoke(
            "cards.comment",
            {"id": card_id, "body": body},
            read_action="cards.get",
            read_input={"id": card_id},
        )
        version = int(card.get("version") or 1)
        moved = card
        if str(card.get("list_id") or "") != str(active["id"]):
            moved = self.invoke(
                "cards.move",
                {"id": card_id, "list_id": str(active["id"])},
                read_action="cards.get",
                read_input={"id": card_id},
                if_match=version,
            )
            version = int(moved.get("version") or version + 1)
        self.invoke(
            "cards.move",
            {"id": card_id, "list_id": str(done["id"])},
            read_action="cards.get",
            read_input={"id": card_id},
            if_match=version,
        )

    def discharge_rework_cards(self) -> int:
        """Discharge every rework card whose package the fresh audit no longer reports open.

        The rule is the one `--rework-check` publishes: a key absent from the register's
        packages is cleared. A card whose package is still open — or suppressed, which ADR 0012
        counts as unresolved — stays exactly where it is.
        """
        candidates = self.owned_rework_cards()
        if not candidates:
            print("  card discharge: no open rework card owned by the project")
            return 0
        if self.dry_run:
            print(f"  card discharge: dry run over {len(candidates)} card(s)")
            return 0
        snapshot = self.snapshot_now("discharge")
        try:
            register = build_rework_register(
                Path(snapshot), Path(snapshot).parent / "docs"
            )
        except RegisterError as error:
            fail(f"{self.project}: the discharge register failed: {error}")
        open_keys = {
            str(package.get("key")) for package in register.get("packages") or []
        }
        cleared = 0
        for candidate in candidates:
            if candidate["key"] in open_keys:
                continue
            self.discharge_card(
                candidate["card"], candidate["key"], candidate["active"], candidate["done"]
            )
            cleared += 1
            print(f"  card cleared: {candidate['key']}")
        print(f"  card discharge: {cleared} card(s) cleared of {len(open_keys)} open package(s)")
        return cleared

    # ── lifecycle ───────────────────────────────────────────────────────────────────

    def project_row(self) -> dict:
        return json.loads(
            subprocess.run(
                [
                    sys.executable, "-I", str(HARNESS), "--check", "--quiet",
                    "--action", "projects.get",
                    "--input", json.dumps({"id": self.project_id}),
                ],
                cwd=ROOT, capture_output=True, text=True, check=True,
            ).stdout
        )["project"]

    def gate_approved(self, gates: list, name: str) -> bool:
        return any(
            gate.get("gate") == name and gate.get("status") == "approved" for gate in gates
        )

    def run(self) -> None:
        self.snapshot_now("pre")
        project = next(
            row for row in self.snapshot_json("programme.json")["projects"]
            if row.get("key") == self.project
        )
        self.project_id = project["id"]
        self.programme_id = str(
            next(
                (
                    row.get("id")
                    for row in self.snapshot_json("programme.json").get("programme") or []
                ),
                "",
            )
        )
        board = self._read("architecture.board_list", {})
        self.board = board["items"][0]["id"]

        # 1. reopen a closed project, citing the delegation the rework runs under
        if project["status"] == "closed":
            # A reopen is legal only for a project the retrospective judged "rework"; the
            # verdict is a programme-owned key, so the factory records it first (measured on
            # T19: `projects.reopen` refused with meta.viability absent).
            row = self.project_row()
            if not (row.get("meta") or {}).get("viability"):
                self.invoke(
                    "projects.programme_meta",
                    {"id": self.project_id, "meta": {"viability": "rework"}},
                    if_match=row["version"],
                )
            self.invoke(
                "projects.reopen",
                {
                    "id": self.project_id,
                    "reason": self.reopen_reason(),
                    "decision_ref": self.delegation,
                },
            )
        else:
            print("  reopen: not needed")

        # 2. charter (the U5a batch may already have one)
        pm = self._read("pm.get", {"project_id": self.project_id})
        charter_gates = pm.get("gates") or []
        if not pm.get("charter_asset_id") or not self.gate_approved(charter_gates, "charter"):
            charter = self.invoke(
                "projects.charter",
                {"project_id": self.project_id, "charter": self.content["charter"]},
                read_input={"project_id": self.project_id},
            )
            self.decide(charter["approval_id"], "approve the charter")
        else:
            print("  charter: already approved")

        # 3. declare the closure outputs the project publishes
        row = self.project_row()
        meta = dict(row.get("meta") or {})
        if not self.closes_project():
            print("  closure outputs: not applicable (the project stays active)")
        elif set(meta.get("closure_outputs") or []) != {"closure-review"}:
            self.invoke(
                "projects.programme_meta",
                {"id": self.project_id, "meta": {"closure_outputs": ["closure-review"]}},
                if_match=row["version"],
            )
        else:
            print(f"  closure outputs: already {meta['closure_outputs']}")

        # 4. the Statement of Architecture Work
        contracts = self._read(
            "architecture.contracts_list", {"project_id": self.project_id}
        ).get("items", [])
        soaw = next(
            (c for c in contracts
             if c.get("kind") == "statement_of_architecture_work"
             and c.get("status") in ("live", "draft")),
            None,
        )
        if soaw is None:
            requested = self.invoke(
                "architecture.contracts_request",
                {
                    "project_id": self.project_id,
                    "kind": "statement_of_architecture_work",
                    "phase_code": "a",
                    "title": self.content["soaw"]["title"],
                    "body": self.content["soaw"]["body"],
                },
                read_action="architecture.contracts_get",
                read_id_from_output="id",
            )
            soaw = self.approve_contract(
                requested["id"], "approve the Statement of Architecture Work", version=1
            )
        elif soaw.get("status") == "draft":
            soaw = self.approve_contract(
                soaw["id"], "approve the Statement of Architecture Work",
                version=soaw.get("version", 1),
            )
        else:
            print("  SoAW: already live")

        # 5. the project's artefacts (A–D; plus E–H on the architecture track)
        existing = self._read(
            "architecture.artifacts_list", {"project_id": self.project_id}
        ).get("items", [])
        titles = {
            row.get("title"): row
            for row in existing
            if row.get("status") not in ("superseded", "archived")
        }
        standard = {
            row.get("key"): row.get("id")
            for row in (self.snapshot_json("architecture.json").get("standard") or [])
        }
        artefact_keys = self.artefact_keys()
        required = len(artefact_keys)
        if len(self.content["artifacts"]) != required:
            fail(f"the content must carry the {required} artefacts in the standard order")
        artefact_ids: dict[str, str] = {}
        snapshot_documents = {
            str(row.get("id")): row.get("document_id")
            for row in (self.snapshot_json("architecture.json").get("artifacts") or [])
        }
        snapshot_entries = {
            str(row.get("id")): len(row.get("entries") or [])
            for row in (self.snapshot_json("architecture.json").get("artifacts") or [])
        }
        def has_instance_document(row: dict) -> bool:
            # The standard's artefact-instance rule resolves the `Document` section from a
            # document of category `architecture-artifact-instance`. A v1 row's document has no
            # category at all, so reusing it by id leaves the section unresolved and the package
            # open while the row exists (measured on S00, 2026-11-26: the package stayed open
            # after the artefacts were bound).
            document = snapshot_documents.get(str(row.get("id"))) or row.get("document_id")
            item = (
                next(
                    (doc for doc in self._published_outputs() if str(doc.get("id")) == str(document)),
                    None,
                )
                if document
                else None
            )
            return item is not None and str(item.get("category") or "") == (
                "architecture-artifact-instance"
            )

        for index, artefact in enumerate(self.content["artifacts"]):
            key = artefact_keys[index]
            row = titles.get(artefact["title"])
            if row is None:
                # A pre-rework row of the standard type is the row the standard counts: T19F's
                # live gate created its seven artefacts under its own titles, so a title-only
                # match authored a second artefact per type and left the original's instance
                # package open. Repair the row of the standard type instead of duplicating it,
                # preferring one the standard still reports as undocumented.
                type_id = str(standard.get(key) or "")
                candidates = [
                    candidate
                    for candidate in existing
                    if str(candidate.get("artifact_type_id") or "") == type_id
                    and candidate.get("status") not in ("superseded", "archived")
                ]
                row = next(
                    (candidate for candidate in candidates if not has_instance_document(candidate)),
                    candidates[0] if candidates else None,
                )
            if row is not None:
                meta = dict(row.get("meta") or {})
                document_id = snapshot_documents.get(str(row.get("id"))) or row.get("document_id")
                # The standard's artefact-instance rule resolves the `Document` section from a
                # document of category `architecture-artifact-instance`. A v1 row's document has
                # no category at all, so reusing it by id leaves the section unresolved and the
                # package open while the row exists (measured on S00, 2026-11-26: the package
                # stayed open after the artefacts were bound). The repair therefore requires an
                # instance document, not merely some document.
                document_row = (
                    next(
                        (
                            item
                            for item in self._published_outputs()
                            if str(item.get("id")) == str(document_id)
                        ),
                        None,
                    )
                    if document_id
                    else None
                )
                instance_document = (
                    document_row is not None
                    and str(document_row.get("category") or "")
                    == "architecture-artifact-instance"
                )
                # The close and phase gates resolve an approved artefact by its standard type,
                # not by `meta.artifact_key` (measured on S00, 2026-11-26: a re-versioned row
                # carried the meta key but no type, so `projects.close` named every artefact
                # missing while the row existed). Both bindings must hold before a row counts as
                # present; otherwise the repair re-versions it and sets the type below.
                expected_type = standard.get(key)
                needs_key = (
                    meta.get("artifact_key") != key
                    or str(row.get("artifact_type_id") or "") != str(expected_type or "")
                    or not instance_document
                )
                # A supersede copies the row but not its entries: a re-versioned artefact whose
                # pack carries entries and whose row has none is repaired in place (measured on
                # T9's seven artefacts, 2026-11-25: every Entries section resolved to zero).
                needs_entries = (
                    bool(artefact.get("entries"))
                    and snapshot_entries.get(str(row.get("id")), 0) == 0
                )
                if not needs_key and document_id and not needs_entries:
                    artefact_ids[artefact["title"]] = str(row["id"])
                    print(f"  artefact: {artefact['title']} already present")
                    continue
                # An approved artefact is immutable; the repair is a new version that carries
                # the standard key and the published document it sits under. The document is
                # published first, so the new row pins a real body from its first version, and a
                # v1 document without the instance category is replaced rather than reused.
                document_id = (
                    document_id if instance_document else self.publish_artefact_document(artefact)
                )
                draft = self.invoke(
                    "architecture.artifacts_supersede",
                    {"id": row["id"]},
                    read_action="architecture.artifacts_get",
                    read_id_from_output="id",
                    if_match=row.get("version", 1),
                )
                meta = dict(draft.get("meta") or {}) if not self.dry_run else {}
                meta.update(
                    {
                        "retrospective": True,
                        "delegation_ref": self.delegation,
                        "artifact_key": key,
                    }
                )
                draft = self.invoke(
                    "architecture.artifacts_update",
                    {
                        "id": draft["id"],
                        "artifact_type_id": expected_type,
                        "meta": meta,
                        "document_id": document_id,
                        "entries": artefact.get("entries") or [],
                    },
                    read_action="architecture.artifacts_get",
                    read_input={"id": draft["id"]},
                    if_match=draft.get("version", 1),
                )
                self.invoke(
                    "architecture.artifacts_approve",
                    {"id": draft["id"]},
                    actor=APPROVER,
                    if_match=draft.get("version", 1),
                )
                artefact_ids[artefact["title"]] = str(draft["id"])
                print(f"  artefact: {artefact['title']} re-versioned, bound and documented")
                continue
            document_id = self.publish_artefact_document(artefact)
            created = self.invoke(
                "architecture.artifacts_create",
                dict(
                    artefact,
                    project_id=self.project_id,
                    artifact_type_id=standard.get(key),
                    document_id=document_id,
                    meta={
                        "retrospective": True,
                        "delegation_ref": self.delegation,
                        "artifact_key": key,
                    },
                ),
                read_action="architecture.artifacts_get",
                read_id_from_output="id",
            )
            self.invoke(
                "architecture.artifacts_approve",
                {"id": created["id"]},
                actor=APPROVER,
                if_match=1,
            )
            artefact_ids[artefact["title"]] = str(created["id"])
        self.ensure_artefact_links(artefact_ids)
        self.ensure_documents()

        # 6. the delivery contract
        delivery = [
            c for c in contracts
            if c.get("kind") == "delivery_contract" and c.get("status") in ("live", "draft")
        ]
        if not delivery:
            delivery = [
                candidate for candidate in self._read(
                    "architecture.contracts_list", {"project_id": self.project_id}
                ).get("items", [])
                if candidate.get("kind") == "delivery_contract"
                and candidate.get("status") in ("live", "draft")
            ]
        live = next((c for c in delivery if c.get("status") == "live"), None)
        draft = next((c for c in delivery if c.get("status") == "draft"), None)
        pack_body = self.content["delivery_contract"]["body"] or {}

        def supersede_live() -> None:
            # The store refuses to approve a draft while a live sibling of the same
            # (project, kind, phase) exists, and an approved row is never edited: the replacement
            # is a superseding revision (measured on T19F, whose live gate left a live delivery
            # contract the factory's first pass could not revise).
            self.invoke(
                "architecture.contracts_supersede",
                {"id": live["id"]},
                if_match=live.get("version", 1),
                read_action="architecture.contracts_get",
                read_input={"id": live["id"]},
            )

        if live is None and draft is None:
            requested = self.invoke(
                "architecture.contracts_request",
                {
                    "project_id": self.project_id,
                    "kind": "delivery_contract",
                    "phase_code": "f",
                    "title": self.content["delivery_contract"]["title"],
                    "body": pack_body,
                },
                read_action="architecture.contracts_get",
                read_id_from_output="id",
            )
            contract = self.approve_contract(
                requested["id"], "approve the delivery contract", version=1
            )
        elif live is not None and draft is not None:
            # A refused first pass left its revision pending: supersede the live row and approve
            # the draft it already created rather than authoring a second one.
            supersede_live()
            contract = self.approve_contract(
                draft["id"], "approve the delivery contract revision",
                version=draft.get("version", 1),
            )
            print("  delivery contract: approved the pending revision over the superseded row")
        elif draft is not None:
            contract = self.approve_contract(
                draft["id"], "approve the delivery contract",
                version=draft.get("version", 1),
            )
        else:
            # A live contract whose body lacks a field the standard's page reads is repaired by
            # a superseding revision, never by editing an approved row: the packs author
            # commitments/compliance/approval, and the record's contract must carry them for the
            # page's sections to resolve (measured 2026-11-25: every member's Commitments empty).
            live_body = live.get("body") or {}
            missing = [name for name in pack_body if name not in live_body]
            if missing:
                supersede_live()
                requested = self.invoke(
                    "architecture.contracts_request",
                    {
                        "project_id": self.project_id,
                        "kind": "delivery_contract",
                        "phase_code": "f",
                        "title": self.content["delivery_contract"]["title"],
                        "body": pack_body,
                        "supersedes_id": live["id"],
                    },
                    read_action="architecture.contracts_get",
                    read_id_from_output="id",
                )
                contract = self.approve_contract(
                    requested["id"],
                    "approve the delivery contract revision",
                    version=1,
                )
                print("  delivery contract: revised to carry the authored body")
            else:
                contract = live
                print("  delivery contract: already live")

        # 6b. the six-section project register, before the baseline freezes it (§8.4)
        self.ensure_registers()

        # 6c. the programme's governed decision records (the licensing frame, the blueprint
        # states, the options and programme appraisals, the solution intent) that the MSP
        # standards' sections source, and the programme's shared configuration.
        self.ensure_assets()

        # 6d. the benefit realization records: one per profile, the row all five
        # benefit-transition sections and both evaluation sections resolve from (§8.3).
        self.ensure_benefit_transitions()

        # 7. baseline with the project's one WBS deliverable
        baselines = [
            b for b in self.snapshot_json("msp.json").get("project_baselines", [])
            if b.get("project_id") == self.project_id
            and str(b.get("kind") or "scope") == "scope"
        ]
        deliverable = None
        if baselines:
            nodes = [
                n for n in self.snapshot_json("msp.json").get("wbs_nodes", [])
                if n.get("project_id") == self.project_id and n.get("kind") == "deliverable"
            ]
            deliverable = nodes[0]["id"] if nodes else None
            print("  baseline: already taken")
        else:
            plan = self.invoke(
                "projects.baseline",
                {
                    "project_id": self.project_id,
                    "baseline_kind": "scope",
                    "wbs": [
                        {
                            "kind": "deliverable",
                            "title": self.content["wbs_deliverable"]["title"],
                            "business_value": self.content["wbs_deliverable"]["business_value"],
                            "time_criticality": self.content["wbs_deliverable"]["time_criticality"],
                            "risk_reduction": self.content["wbs_deliverable"]["risk_reduction"],
                            "job_size": self.content["wbs_deliverable"]["job_size"],
                        }
                    ],
                },
                read_action="baselines.get",
                read_id_from_output="baseline_id",
            )
            deliverable = plan["wbs_ids"][0]
            # The baseline's approval is deferred until the deliverable is accepted: once an
            # approved scope baseline exists, every `wbs.*` mutation needs an approved change
            # request (`409 rebaseline_requires_change`, v1-p5-4), and this tenant has no PM
            # process or job binding through which `changes.request` could open one. Accepting
            # the deliverable before the baseline freezes the scope is also the truer order:
            # the baseline records the accepted scope.
            self.baseline_approval = plan["approval_id"]

        # 7b. the schedule and cost baselines the PMP baseline rule reads
        self.ensure_extra_baselines()

        # 8. publish the retrospective closure review output
        outputs = self._published_outputs()
        if not self.closes_project():
            print("  closure-review output: not applicable (the project stays active)")
        elif not any(
            row.get("category") == "closure-review"
            and row.get("project_id") == self.project_id
            and "retrospective" in (row.get("title") or "")
            for row in outputs
        ):
            review = self.invoke(
                "documents.compose",
                {
                    "title": f"Closure review — {self.project} (retrospective, {self.today})",
                    "project_id": self.project_id,
                    "body": self.content["closure_review"],
                    "category": "closure-review",
                    "publishable": True,
                },
                read_action="documents.get",
                read_id_from_output="id",
            )
            self.invoke(
                "documents.publish",
                {"id": review["id"], "publish": True},
                read_input={"id": review["id"]},
                if_match=1,
            )
        else:
            print("  closure-review output: already published")

        # 8b. the project's other expected outputs (the close gate requires every declared one)
        expected = (self.project_row().get("meta") or {}).get("expected_outputs") or []
        for category in expected:
            body = self.content.get(
                "spec_output" if category == "spec" else f"{category.replace('-', '_')}_output"
            )
            if not body:
                fail(f"{category}: no body in the content file")
            self.ensure_output(category, body)

        # 9. the Phase-G compliance review, read back from a fresh snapshot
        if self.dry_run:
            reviews = []
        else:
            self.snapshot_now("review")
            reviews = [
                r for r in self.snapshot_json("architecture.json").get("compliance_reviews", [])
                if r.get("project_id") == self.project_id
                and r.get("contract_id") == contract["id"]
            ]
        if not reviews:
            self.invoke(
                "architecture.reviews_record",
                {
                    "project_id": self.project_id,
                    "contract_id": contract["id"],
                    "level": self.content["review"]["level"],
                    "findings": self.content["review"]["findings"],
                    "evidence": dict(
                        self.content["review"]["evidence"],
                        delegation_ref=self.delegation,
                        manifest=f"acceptance/programme/{self.project}.toml",
                        sha=self.head,
                    ),
                    "delegation_ref": self.delegation,
                },
                actor=APPROVER,
                use_snapshot=True,
                snapshot_command=f"scripts/snapshot-live.sh {self.snapshot_root}",
            )
        else:
            print("  Phase-G review: already recorded")

        # 10. accept the WBS deliverable with the evidence and credit the demo
        if not deliverable:
            node = None
        elif self.dry_run:
            node = DryValue()
        else:
            node = self._read("wbs.get", {"id": deliverable})
        if node and node.get("status") != "accepted":
            wbs = self.content["wbs_deliverable"]
            change_id = None if self.dry_run else self.ensure_change()
            payload = {
                "id": deliverable,
                "status": "accepted",
                    "evidence": [
                        f"acceptance/programme/{self.project}.toml",
                        f"docs/evidence/rework/{self.project}.json",
                        self.acts_ref(),
                    ],
                    "business_value": wbs["business_value"],
                    "time_criticality": wbs["time_criticality"],
                    "risk_reduction": wbs["risk_reduction"],
                "job_size": wbs["job_size"],
            }
            if change_id:
                payload["change_id"] = change_id
            self.invoke(
                "wbs.update",
                payload,
                read_action="wbs.get",
                read_input={"id": deliverable},
                if_match=node.get("version", 1),
            )
        if deliverable and not self.dry_run:
            self.invoke(
                "cadences.credit_demo",
                {"cadence_id": self.demo_cadence, "deliverable_id": deliverable},
                read_action="cadences.credit_list",
                read_input={"cadence_id": self.demo_cadence},
                read_match=[f"deliverable_id={deliverable}"],
                local=True,
            )

        # 10b. approve the scope baseline now that its deliverable is accepted
        approval = self.baseline_approval
        if approval is None and not self.dry_run:
            pending = [
                gate
                for gate in self._read(
                    "approvals.list", {"project_id": self.project_id, "status": "pending"}
                ).get("items", [])
                if gate.get("gate") == "baseline"
            ]
            approval = pending[0]["id"] if pending else None
        if approval:
            self.decide(approval, "approve the project baseline")
        else:
            print("  baseline approval: none pending")

        # 10c. every project carries at least one approved change. The standard's change-control
        # and project-registers documents read a `change` row, and the close gate names
        # change-control among its mandatory artefacts. The acceptance path opens one when it
        # rewrites the frozen plan; a project whose deliverable was already accepted records the
        # retrospective rework itself as the change (measured on S05/S07/S09/T19, 2026-11-25).
        changes = [
            row
            for row in self._read(
                "registers.list", {"project_id": self.project_id}
            ).get("items", [])
            if row.get("kind") == "change"
        ]
        if changes:
            print(f"  change control: {len(changes)} change row(s) present")
            # The composer refuses a register row with an empty code (`EMPTY_REGISTER_CODE`),
            # and a change row is a register row: `changes.request` writes none, so a row an
            # earlier run created is named here (measured on S07/S09, tr4 repair 2026-11-26:
            # project-registers and change-control stayed refused on the empty code).
            for row in changes:
                self.name_change(str(row["id"]))
        else:
            baseline = None
            for row in self._read(
                "baselines.list", {"project_id": self.project_id}
            ).get("items", []):
                if str(row.get("kind") or "scope") == "scope" and row.get("approved_by"):
                    baseline = row
                    break
            if baseline is None:
                fail(f"{self.project}: no approved scope baseline for the retrospective change")
            change = self.invoke(
                "changes.request",
                {
                    "project_id": self.project_id,
                    "baseline_id": baseline["id"],
                    "title": f"The retrospective rework of {self.project}",
                    "body": (
                        "The retrospective MSP/TOGAF/SAFe rework changed this project's record: "
                        "its artefacts were re-versioned with documents and links, its register "
                        "was written, and its schedule and cost baselines were taken. This change "
                        "records that rework against the frozen scope baseline."
                    ),
                    "impact": {"scope": "the retrospective rework's record acts"},
                },
                read_action="registers.get",
                read_id_from_output="change_id",
            )
            self.name_change(str(change["change_id"]))
            self.decide(
                change["approval_id"],
                "approve the change that records the retrospective rework",
            )

        # 10e. the pack's declarative record corrections: the probe rows retired, the
        # ownerless risks backfilled, the empty work packages completed, a tranche's review
        # date set, the programme's shared configuration written. Every write is a product
        # action with a read-back, and the approved change above authorises the WBS edits.
        self.ensure_record_fixes()

        # 10d. discharge the project's own rework cards whose packages this run cleared.
        # The close gate reads the board, not the register: without this the project stays
        # open on its own P0 card even when the receipt's product check is green
        # (measured 2026-11-26 on DESIGN-PUBLISH).
        self.discharge_rework_cards()

        # 11. closure request -> decision -> the destructive close gate -> close
        row = self.project_row()
        if not self.closes_project():
            print("  closure: not applicable (the project stays active)")
        elif row["status"] != "closed":
            approvals = self._read(
                "approvals.list", {"project_id": self.project_id}
            ).get("items", [])
            # A reopen voids the earlier closure decision: the close gate only counts a closure
            # approval taken after `reopened_at` (`assert_project_can_close` reads `since`).
            # Without this, a resumed run sees the first run's approval, skips the request and
            # the close is refused `closure_gate_missing`.
            since = (row.get("meta") or {}).get("reopened_at") or row.get("reopened_at")

            def current(approval: dict, since: str | None) -> bool:
                if not since:
                    return True
                at = approval.get("decided_at") or approval.get("created_at")
                if not at:
                    return False
                try:
                    return datetime.fromisoformat(
                        str(at).replace("Z", "+00:00")
                    ) >= datetime.fromisoformat(str(since).replace("Z", "+00:00"))
                except ValueError:
                    return True

            approved_closure = [
                approval
                for approval in approvals
                if approval.get("gate") == "closure"
                and approval.get("status") == "approved"
                and current(approval, since)
            ]
            if approved_closure:
                print("  closure gate: already approved")
                # The closure asset that gate approved is the evidence. `approvals.list` does
                # not project `meta.closure_asset_id`, but the snapshot's approval row carries
                # it (step 9 refreshed the snapshot), so a resumed run reads the record instead
                # of demanding an operator id. The live gate id is the key: the newest approval
                # the close action will read is the one whose decision names the asset.
                closure_asset = self.closure_asset
                if not closure_asset:
                    snapshots = {
                        str(row.get("id")): row
                        for row in self.snapshot_json("work.json").get("approvals") or []
                    }
                    for approval in approved_closure:
                        asset = (snapshots.get(str(approval.get("id"))) or {}).get("meta") or {}
                        if asset.get("closure_asset_id"):
                            closure_asset = str(asset["closure_asset_id"])
                            break
            else:
                closure = self.invoke(
                    "projects.close_request",
                    {
                        "id": self.project_id,
                        "closure": self.content["closure"],
                        "delegation_ref": self.delegation,
                        "evidence": {
                            "manifest": f"acceptance/programme/{self.project}.toml",
                            "sha": self.head,
                            "retrospective": True,
                        },
                    },
                    read_input={"id": self.project_id},
                )
                self.decide(closure["approval_id"], "decide the closure gate")
                # The destructive gate needs a typed, live evidence reference: the closure asset
                # the closure gate just approved (a draft asset is refused, so this request runs
                # only after its decide). `proposed_input` pins the exact close call it authorises.
                closure_asset = closure.get("closure_asset_id")
            if not closure_asset:
                fail(
                    f"{self.project}: projects.close_request returned no closure_asset_id; "
                    "the destructive gate has no typed evidence to name"
                )
            approved_close = [
                approval
                for approval in approvals
                if approval.get("gate") == "projects.close"
                and approval.get("status") == "approved"
                and current(approval, since)
            ]
            if approved_close:
                # A resumed run must not open a second gate for the same close: the first
                # one is decided and names the same target.
                print("  destructive gate: already approved")
                gate = approved_close[0]
            else:
                gate = self.invoke(
                    "approvals.request",
                    {
                        "project_id": self.project_id,
                        "gate": "projects.close",
                        "kind": "action",
                        "action": "projects.close",
                        "input": {"id": self.project_id},
                        "proposed_input": {
                            "id": self.project_id,
                            "reason": self.close_reason(),
                        },
                        "description": f"Close {self.project} after the retrospective rework",
                        "evidence": [{"kind": "governed_asset", "id": closure_asset}],
                    },
                    read_action="approvals.list",
                    read_match=["gate=projects.close", "status=pending"],
                )
                self.decide(gate["id"], "approve the destructive close", close_gate=True)
            closed = self.invoke(
                "projects.close",
                {"id": self.project_id, "reason": self.close_reason()},
                approval_id=gate["id"],
                read_action="projects.get",
                read_input={"id": self.project_id},
            )
            print(f"  closed {self.project}: {closed.get('id') or closed.get('project', {}).get('id')}")

        print(f"{self.project}: {self.acts} acts recorded")
        if not self.dry_run:
            total = (
                sum(1 for _ in self.transcript.open(encoding="utf-8"))
                if self.transcript.exists()
                else self.acts
            )
            with Path(self.acts_path).open("a", encoding="utf-8") as page:
                page.write(
                    f"\n{self.project}: {total} acts in `transcript.jsonl`; "
                    f"the last run recorded {self.acts}.\n"
                )

    def ensure_registers(self) -> None:
        """Write the project register the PMP rule reads (§8.4).

        The content authors the rows; this runner only writes them. A risk's owner is the
        pack's `meta.owner` role when no queue job owns the work (owner's 2026-11-25
        decision: backfill and keep the risk managed); probability, impact, review date and
        a link stay mandatory. On resume the live list is read first — `registers.create` is
        not idempotent.
        """
        rows = self.content.get("registers") or {}
        if not rows:
            print("  registers: none in the content")
            return
        existing = self._read(
            "registers.list", {"project_id": self.project_id}
        ).get("items", [])
        by_key = {
            (row.get("kind"), row.get("title")): row for row in existing
        }
        for kind, items in rows.items():
            for item in items:
                title = str(item.get("title") or "").strip()
                if not title:
                    fail(f"registers: a {kind} row has no title")
                present = by_key.get((kind, title))
                if present is not None:
                    # A row written by an earlier run can carry an empty code; the
                    # standard's project-registers document refuses it (`EMPTY_REGISTER_CODE`)
                    # and the close gate then refuses the project. The pack is the authority
                    # for its authored code, so an empty live code is filled from it
                    # (measured on S02/S03/T1/T11, 2026-11-26).
                    code = str(item.get("code") or "").strip()
                    if code and not str(present.get("code") or "").strip():
                        self.invoke(
                            "registers.update",
                            {"id": present["id"], "code": code},
                            read_action="registers.get",
                            read_input={"id": present["id"]},
                            if_match=present.get("version", 1),
                        )
                        print(f"  register {kind}: {title} code set to {code}")
                    else:
                        print(f"  register {kind}: {title} already present")
                    continue
                self.invoke(
                    "registers.create",
                    {"project_id": self.project_id, "kind": kind, **item},
                    read_action="registers.list",
                    read_input={"project_id": self.project_id},
                    read_match=[f"kind={kind}", f"title={title}"],
                )

    # ── the programme's governed decision records ──────────────────────────────────

    def ensure_assets(self) -> None:
        """Write the pack's programme governed assets and the shared configuration.

        The MSP standards' sections that name a record no row carries — the licensing frame,
        the blueprint's states, the options and the programme appraisal, the solution intent —
        source a governed asset by kind and key. Composing the asset here is what makes the
        source real: an absent record is not a section. The assets are ADR-kind decision
        records, the one governed kind the generic family composes without a new migration.
        """
        rows = self.content.get("assets") or []
        if rows:
            existing = {
                str(row.get("key")): row
                for row in self._read("governed_assets.list", {}).get("items") or []
            }
            for item in rows:
                kind = str(item.get("kind") or "adr").strip()
                title = str(item.get("title") or "").strip()
                body = str(item.get("body") or "")
                if not title or not body.strip():
                    fail("assets: every asset needs a title and a body")
                found = next(
                    (row for row in existing.values() if str(row.get("title")) == title),
                    None,
                )
                if found is None:
                    asset = self.invoke(
                        "governed_assets.compose",
                        {
                            "kind": kind,
                            "title": title,
                            "body": body,
                            "project_id": self.project_id,
                        },
                        read_action="governed_assets.get",
                        read_id_from_output="id",
                    )
                else:
                    asset = found
                    print(f"  governed asset {title}: already present")
                status = str(asset.get("status") or "")
                if status == "draft":
                    self.invoke(
                        "governed_assets.submit",
                        {"id": asset["id"]},
                        read_action="governed_assets.get",
                        read_input={"id": asset["id"]},
                    )
                    status = "in_review"
                if status == "in_review":
                    self.invoke(
                        "governed_assets.approve",
                        {"id": asset["id"]},
                        read_action="governed_assets.get",
                        read_input={"id": asset["id"]},
                    )
        shared = self.content.get("programme_shared_config")
        if shared:
            self.write_shared_config(shared)

    def write_shared_config(self, shared: dict) -> None:
        """Set the programme's shared configuration (§8.2), if the pack carries one."""
        row = next(
            (r for r in self.snapshot_json("programme.json").get("programme") or []),
            None,
        )
        if row is None:
            fail(f"{self.project}: no programme row to carry shared_config")
        if row.get("shared_config") == shared:
            print("  programme shared_config: already set")
            return
        self.invoke(
            "programmes.update",
            {"id": row["id"], "shared_config": shared},
            read_action="programmes.get",
            read_input={"id": row["id"]},
            if_match=row.get("version", 1),
        )

    # ── benefit realization records (SPEC_V2 §8.3) ─────────────────────────────────

    def ensure_benefit_transitions(self) -> None:
        """Write one realization record per benefit profile the pack names.

        The benefit-transition and benefit-evaluation standards resolve all seven of their
        sections from `msp.json#/benefit_transitions`; an empty table makes fourteen
        mandatory artefacts absent however good their prose. The row is the measurement
        record: the baseline, the readiness, the enactment, the post reading and the
        legacy retirement, with the evidence behind each date.
        """
        rows = self.content.get("benefit_transitions") or []
        if not rows:
            print("  benefit transitions: none in the content")
            return
        benefits = self._read(
            "benefits.list", {"programme_id": self.programme_id}
        ).get("items") or []
        by_ref = {str(row.get("reference") or ""): row for row in benefits}
        existing = {
            str(row.get("benefit_id")): row
            for row in self._read(
                "benefits.transition_list", {"programme_id": self.programme_id}
            ).get("items") or []
        }
        for item in rows:
            reference = str(item.get("reference") or "").strip()
            benefit = by_ref.get(reference)
            if benefit is None:
                fail(f"benefit transition {reference}: no benefit profile with that reference")
            payload = {
                "benefit_id": benefit["id"],
                "baseline_measured_at": item.get("baseline_measured_at"),
                "readiness": item.get("readiness"),
                "enacted_at": item.get("enacted_at"),
                "post_measured_at": item.get("post_measured_at"),
                "retired_at": item.get("retired_at"),
                "evidence": item.get("evidence"),
            }
            current = existing.get(str(benefit["id"]))
            if current is None:
                self.invoke(
                    "benefits.transition_upsert",
                    payload,
                    read_action="benefits.transition_list",
                    read_input={"programme_id": self.programme_id},
                    read_match=[f"benefit_id={benefit['id']}"],
                )
            else:
                print(f"  benefit transition {reference}: already present")
                self.invoke(
                    "benefits.transition_upsert",
                    payload,
                    read_action="benefits.transition_list",
                    read_input={"programme_id": self.programme_id},
                    read_match=[f"benefit_id={benefit['id']}"],
                    if_match=current.get("version", 1),
                )

    # ── declarative record corrections ─────────────────────────────────────────────

    def ensure_record_fixes(self) -> None:
        """Apply the pack's `record_fixes` through the product actions.

        The vocabulary is small and typed — a register row retired or backfilled, a work
        package completed, a tranche's review date set — because a free-form action list
        would let the pack write anything the factory has not verified. Every write reads
        the row back; the approved change the rework already recorded authorises the WBS
        edits against the frozen scope baseline.
        """
        fixes = self.content.get("record_fixes") or []
        if not fixes:
            print("  record fixes: none in the content")
            return
        project_ids = {
            str(row.get("key")): str(row.get("id"))
            for row in self.snapshot_json("programme.json").get("projects") or []
        }
        change_id: str | None = None
        for fix in fixes:
            kind = str(fix.get("kind") or "")
            match = fix.get("match") or {}
            if kind == "register":
                project_key = str(fix.get("project") or self.project)
                project_id = project_ids.get(project_key)
                if not project_id:
                    fail(f"record fix: no project {project_key}")
                rows = self._read(
                    "registers.list", {"project_id": project_id}
                ).get("items") or []
                row = next(
                    (
                        r for r in rows
                        if all(str(r.get(k)) == str(v) for k, v in match.items())
                    ),
                    None,
                )
                if row is None:
                    fail(f"record fix: no register row matches {match}")
                payload = {"id": row["id"], **(fix.get("set") or {})}
                self.invoke(
                    "registers.update",
                    payload,
                    read_action="registers.get",
                    read_input={"id": row["id"]},
                    if_match=row.get("version", 1),
                )
            elif kind == "wbs":
                rows = self._read(
                    "wbs.list", {"project_id": self.project_id}
                ).get("items") or []
                row = next(
                    (
                        r for r in rows
                        if all(str(r.get(k)) == str(v) for k, v in match.items())
                    ),
                    None,
                )
                if row is None:
                    fail(f"record fix: no WBS node matches {match}")
                if change_id is None:
                    # The WBS gate reads an APPROVED `change` gate whose meta names the change;
                    # a row's own status is never an approval (`change_is_approved_tx`). A
                    # pre-existing change whose approval predates that link authorises nothing
                    # (measured 2026-11-26 on PLATFORM), so the factory resolves a linked
                    # approval and, when there is none, opens and decides one.
                    change_id = self.approved_change()
                payload = {"id": row["id"], "change_id": change_id, **{
                    key: fix[key] for key in ("acceptance", "evidence", "status") if key in fix
                }}
                if str(row.get("kind") or "") == "work_package" and not row.get("card_id"):
                    # Migration 0097: a work_package node owes its card. The row predates the
                    # rule, and an UPDATE re-validates it, so the plan half grows the card it
                    # should have carried from birth.
                    payload["card_id"] = self.ensure_work_package_card(
                        str(row.get("title") or f"{self.project} work package")
                    )
                self.invoke(
                    "wbs.update",
                    payload,
                    read_action="wbs.get",
                    read_input={"id": row["id"]},
                    if_match=row.get("version", 1),
                )
            elif kind == "lesson":
                finding_key = str(fix.get("finding_key") or f"L-{self.project}-1")
                existing = self._read(
                    "programme_lessons.list", {"programme_id": self.programme_id}
                ).get("items") or []
                if any(str(row.get("finding_key")) == finding_key for row in existing):
                    print(f"  lesson {finding_key}: already present")
                    continue
                self.invoke(
                    "programme_lessons.propose",
                    {
                        "programme_id": self.programme_id,
                        "cadence_id": fix.get("cadence_id"),
                        "finding_key": finding_key,
                        "statement": str(fix.get("statement") or ""),
                        "evidence": fix.get("evidence") or [],
                        "meta": fix.get("meta") or {},
                    },
                    read_action="programme_lessons.list",
                    read_input={"programme_id": self.programme_id},
                    read_match=[f"finding_key={finding_key}"],
                )
            elif kind == "tranche":
                rows = self._read(
                    "tranches.list", {"programme_id": self.programme_id}
                ).get("items") or []
                row = next(
                    (r for r in rows if str(r.get("number")) == str(fix.get("number"))),
                    None,
                )
                if row is None:
                    fail(f"record fix: no tranche {fix.get('number')}")
                payload = {"id": row["id"], "review_at": fix.get("review_at")}
                self.invoke(
                    "tranches.update",
                    payload,
                    read_action="tranches.list",
                    read_input={"programme_id": self.programme_id},
                    if_match=row.get("version", 1),
                )
            else:
                fail(f"record fix: unknown kind {kind!r}")

    def approved_change(self) -> str:
        """The change id an approved `change` gate authorises, or a newly approved change.

        The WBS guard joins the change row to an `approval` with `gate='change'`,
        `status='approved'` and `meta.change_id` naming it; a change row's own status is never
        an approval. A change created before that link existed authorises nothing, so the
        factory opens one through `changes.request` — the action writes the link — and decides
        its gate.
        """
        changes = {
            str(row.get("id"))
            for row in self._read(
                "registers.list", {"project_id": self.project_id}
            ).get("items") or []
            if row.get("kind") == "change"
        }
        for approval in self.snapshot_json("work.json").get("approvals") or []:
            meta = approval.get("meta") or {}
            change = str(meta.get("change_id") or "")
            if (
                change
                and change in changes
                and approval.get("gate") == "change"
                and approval.get("status") == "approved"
            ):
                return change
        baseline = next(
            (
                row.get("id")
                for row in self.snapshot_json("msp.json").get("project_baselines") or []
                if row.get("project_id") == self.project_id
                and str(row.get("kind") or "scope") == "scope"
                and row.get("approved_by")
            ),
            None,
        )
        if not baseline:
            fail(f"{self.project}: no approved scope baseline for the record-fix change")
        created = self.invoke(
            "changes.request",
            {
                "project_id": self.project_id,
                "baseline_id": baseline,
                "title": "The retrospective rework's record fixes",
                "body": (
                    "The retrospective rework completes the project's record: its work packages "
                    "carry acceptance and evidence, its register rows are owned and dated, and "
                    "the probe rows are retired. This change authorises those edits against the "
                    "approved scope baseline."
                ),
                "impact": {"scope": "the record fixes' WBS acceptance and evidence"},
            },
            read_action="registers.get",
            read_id_from_output="change_id",
        )
        self.decide(created["approval_id"], "approve the record-fix change")
        return str(created["change_id"])

    def ensure_work_package_card(self, title: str) -> str:
        """Create the card a work_package node owes, on the project board's intake lane.

        Migration 0097's `wbs_node_work_package_card_required` refuses a work_package without a
        card; the plan's older nodes predate it. The card is the executors' handle on the work,
        so the fix is to create it, not to exempt the row.
        """
        boards = [
            board
            for board in self.snapshot_json("work.json").get("boards") or []
            if str(board.get("project_id")) == self.project_id
        ]
        if not boards:
            fail(f"{self.project}: no kanban board for the work-package card")
        board_id = str(boards[0]["id"])
        lanes = [
            lane
            for lane in self.snapshot_json("work.json").get("lists") or []
            if str(lane.get("board_id")) == board_id
            and str(lane.get("kind")) == "intake"
        ]
        if not lanes:
            fail(f"{self.project}: the board has no intake lane for the work-package card")
        lane = next(
            (row for row in lanes if str(row.get("name")) == "Intake"), lanes[0]
        )
        card = self.invoke(
            "cards.create",
            {
                "board_id": board_id,
                "list_id": lane["id"],
                "name": title[:190],
                "description": (
                    f"The work package '{title}' of the standing platform's plan. The card is the "
                    "executor's handle on the work; the WBS node carries the acceptance and the "
                    "evidence."
                ),
                "card_type_code": "development_chore",
                "meta": {"work_package": title},
            },
            read_action="cards.get",
            read_id_from_output="id",
        )
        return str(card["id"])

    def ensure_extra_baselines(self) -> None:
        """Take the schedule and cost baselines the PMP baseline rule reads.

        The plan already exists; the two baselines freeze the same WBS with an empty insert,
        so no duplicate node is written. Each take opens its own approval, decided by the
        distinct approver. A resumed run reads the pre-run snapshot's kinds and skips one
        that already exists.
        """
        existing = {
            str(row.get("kind") or "scope")
            for row in self.snapshot_json("msp.json").get("project_baselines", [])
            if row.get("project_id") == self.project_id
        }
        for kind in ("schedule", "cost"):
            if kind in existing:
                print(f"  {kind} baseline: already taken")
                continue
            taken = self.invoke(
                "projects.baseline",
                {"project_id": self.project_id, "baseline_kind": kind, "wbs": []},
                read_action="baselines.get",
                read_id_from_output="baseline_id",
            )
            self.decide(taken["approval_id"], f"approve the {kind} baseline")

    def ensure_documents(self) -> None:
        """Publish the pack's authored category documents.

        The close gate reads a published document for every mandatory category the output
        standard binds, and four TOGAF categories (benefits-diagram, candidate-solutions,
        project-context-diagram, gap-analysis) resolve their template headings from a document
        of their own category — no other row in the record can satisfy them. The pack authors
        those bodies; this publishes one per (project, category), never a second body, because
        the publisher's own lookup refuses two bodies for one category.
        """
        documents = self.content.get("documents") or []
        for document in documents:
            category = str(document.get("category") or "").strip()
            title = str(document.get("title") or "").strip()
            body = str(document.get("body") or "")
            if not category or not title or not body.strip():
                fail("documents: every document needs a title, a category and a body")
            existing = self._read(
                "documents.list", {"project_id": self.project_id, "category": category}
            ).get("items") or []
            present = next(
                (row for row in existing if str(row.get("category") or "") == category), None
            )
            if present is not None:
                # A row that exists is not a row that is published. The close gate reads
                # `published_at`; an unpublished row left behind by an earlier pass counted as
                # "already present" and made the project unclosable while the pack's own body
                # was never applied (measured on SPEC-V2, 2026-11-26).
                if present.get("published_at"):
                    print(f"  document {category}: already present and published")
                    continue
                # An unpublished row is a draft. The pack is the authority for its body — the
                # publisher refuses a draft that is missing a required section, and the draft an
                # earlier pass left may be stale (SPEC-V2's project-brief had no `Plan`).
                updated = self.invoke(
                    "documents.update",
                    {"id": present["id"], "title": title, "body": body},
                    read_action="documents.get",
                    read_input={"id": present["id"]},
                    if_match=present.get("version", 1),
                )
                self.invoke(
                    "documents.publish",
                    {"id": updated["id"], "publish": True},
                    read_input={"id": updated["id"]},
                    if_match=updated.get("version", 1),
                )
                print(f"  document {category}: published (draft replaced by the pack's body)")
                continue
            doc = self.invoke(
                "documents.compose",
                {
                    "title": title,
                    "project_id": self.project_id,
                    "body": body,
                    "category": category,
                    "publishable": True,
                },
                read_action="documents.get",
                read_id_from_output="id",
            )
            self.invoke(
                "documents.publish",
                {"id": doc["id"], "publish": True},
                read_input={"id": doc["id"]},
                if_match=doc.get("version", 1),
            )
            print(f"  document {category}: published")

    def publish_artefact_document(self, artefact: dict) -> str:
        """Compose and publish the artefact's own document, once.

        The standard's artefact-instance rule reads `document_id` off the artefact row; a
        published body is what makes the metadata inspectable. A resumed run reuses the
        published document of the same title from the pre-run outputs index.
        """
        title = str(artefact["title"])
        for row in self._published_outputs():
            if (
                row.get("title") == title
                and row.get("category") == "architecture-artifact-instance"
            ):
                return str(row["id"])
        body = str(artefact.get("body") or "")
        if not body.strip():
            fail(f"artefact {title}: no body to publish")
        doc = self.invoke(
            "documents.compose",
            {
                "title": title,
                "project_id": self.project_id,
                "body": body,
                "category": "architecture-artifact-instance",
                "publishable": True,
            },
            read_action="documents.get",
            read_id_from_output="id",
        )
        self.invoke(
            "documents.publish",
            {"id": doc["id"], "publish": True},
            read_input={"id": doc["id"]},
            if_match=doc.get("version", 1),
        )
        return str(doc["id"])

    def ensure_artefact_links(self, ids_by_title: dict[str, str]) -> None:
        """Create the pack's declared artefact edges (§8.1); an orphan artefact is refused."""
        for artefact in self.content["artifacts"]:
            source = ids_by_title.get(str(artefact["title"]))
            if not source:
                fail(f"artefact {artefact['title']}: no created id to link from")
            for link in artefact.get("links") or []:
                target_title = str(link.get("to") or "")
                target = ids_by_title.get(target_title)
                if not target:
                    fail(
                        f"artefact {artefact['title']}: link target {target_title!r} "
                        "is not an artefact of this pack"
                    )
                self.invoke(
                    "architecture.artifacts_link",
                    {
                        "from_artifact_id": source,
                        "to_artifact_id": target,
                        "kind": str(link.get("kind") or "derivation"),
                    },
                    read_action="architecture.artifacts_links",
                    read_input={"id": source},
                    read_match=[f"to_artifact_id={target}"],
                )

    def _published_outputs(self) -> list[dict]:
        path = Path(self.snapshot).parent / "docs" / "outputs.js"
        if not path.exists():
            return []
        text = path.read_text(encoding="utf-8")
        document = json.loads(text[text.find("{"): text.rfind("}") + 1])
        return document.get("outputs", [])

    def ensure_output(self, category: str, body: str) -> None:
        """Publish a declared output, or correct a published body that the rework changed.

        A rework must not leave a stale public claim. A project-brief or evidence page written
        for the pre-rework state stays public unless its body is corrected; skipping it because
        a row exists would leave the record contradicting the closure. The product lets an admin
        edit a published row, but unpublish -> update -> publish keeps publication a recorded
        decision and re-runs the category's required-heading gate on the new body.
        """
        rows = [
            row for row in self._published_outputs()
            if row.get("category") == category and row.get("project_id") == self.project_id
        ]
        if not rows:
            doc = self.invoke(
                "documents.compose",
                {
                    "title": f"{category.replace('-', ' ').title()} — {self.project} "
                             f"(retrospective, {self.today})",
                    "project_id": self.project_id,
                    "body": body,
                    "category": category,
                    "publishable": True,
                },
                read_action="documents.get",
                read_id_from_output="id",
            )
            self.invoke(
                "documents.publish",
                {"id": doc["id"], "publish": True},
                read_input={"id": doc["id"]},
                if_match=doc.get("version", 1),
            )
            return
        row = self._read("documents.get", {"id": rows[0]["id"]})
        if (row.get("body") or "").strip() == body.strip():
            print(f"  {category} output: current")
            return
        if row.get("published_at"):
            row = self.invoke(
                "documents.publish",
                {"id": row["id"], "publish": False},
                read_input={"id": row["id"]},
                if_match=row.get("version", 1),
            )
        row = self.invoke(
            "documents.update",
            {"id": row["id"], "body": body},
            read_action="documents.get",
            read_input={"id": row["id"]},
            if_match=row.get("version", 1),
        )
        if not row.get("published_at"):
            self.invoke(
                "documents.publish",
                {"id": row["id"], "publish": True},
                read_input={"id": row["id"]},
                if_match=row.get("version", 1),
            )
        print(f"  {category} output: body corrected and republished")

    def name_change(self, change_id: str) -> None:
        """Name a change row with the code the standard's composer requires.

        `changes.request` writes an empty code by design (a change row is a register row,
        and the composer refuses an empty code: EMPTY_REGISTER_CODE). The factory names a
        change as soon as it creates or finds it, so a first run can close without
        depending on a second pass.
        """
        change_id = str(change_id)
        row = self._read("registers.get", {"id": change_id})
        output = row.get("output") if isinstance(row.get("output"), dict) else row
        if str(output.get("code") or "").strip():
            return
        self.invoke(
            "registers.update",
            {"id": change_id, "code": f"CH-{self.project}-{change_id[-6:]}"},
            read_action="registers.get",
            read_input={"id": change_id},
            if_match=int(output.get("version") or 1),
        )

    def ensure_change(self) -> str | None:
        """The approved change a post-baseline acceptance needs (v1-p5-4, v1-change-1).

        Reads the project's baselines; when an approved scope baseline exists, opens the one
        change that authorises this acceptance on the direct route (the baseline, not a PM job
        binding) and approves it under the delegation. No approved baseline returns None and
        the `wbs.update` needs no change.
        """
        items = self._read("baselines.list", {"project_id": self.project_id}).get("items", [])
        # The direct route's change must name the SCOPE baseline: a schedule or cost baseline
        # (added by this rework) is refused `baseline_not_scope` (measured on the tr4 batch).
        approved = [
            row for row in items
            if row.get("approved_by") and str(row.get("kind") or "scope") == "scope"
        ]
        if not approved:
            return None
        baseline = max(approved, key=lambda row: row.get("taken_at") or "")
        change = self.invoke(
            "changes.request",
            {
                "project_id": self.project_id,
                "baseline_id": baseline["id"],
                "title": f"Accept the retrospective WBS deliverable for {self.project}",
                "body": (
                    "The scope baseline freezes the plan; recording the deliverable's "
                    "acceptance with its evidence is the one change this record makes to it."
                ),
                "impact": {"scope": "the WBS deliverable's acceptance evidence"},
            },
            read_action="registers.get",
            read_id_from_output="change_id",
        )
        # The composer refuses a register row with an empty code (`EMPTY_REGISTER_CODE`), and a
        # change row is a register row: `changes.request` writes none, so the factory names it
        # before the gate decides. Measured on S07/S09, tr4 batch 2026-11-26: `project-registers`
        # and `change-control` were refused on the empty code and the close then refused the two
        # missing mandatory documents.
        self.name_change(str(change["change_id"]))
        self.decide(change["approval_id"], "approve the change that authorises the acceptance")
        return str(change["change_id"])

    def decision_payload(self, approval_id: str, note: str) -> dict:
        """The approvals.decide input, citing the published delegation.

        Without `delegation_ref` the decision writer records the admin CLI's
        actor kind (`service`) — the machine approving its own record. Every
        gate this recorder decides is the assistant's rule decision under the
        owner's published delegation, so the reference is not optional here.
        """
        return {
            "approval_id": approval_id,
            "decision": "approve",
            "note": note,
            "delegation_ref": self.delegation,
        }

    def decide(self, approval_id: str, note: str, close_gate: bool = False) -> dict:
        self.invoke(
            "approvals.decide",
            self.decision_payload(approval_id, note),
            actor=APPROVER,
            read_match=[f"id={approval_id}"],
        )
        return {}

    def approve_contract(self, contract_id: str, note: str, version: int = 1) -> dict:
        decision = self.invoke(
            "architecture.board_decide",
            {
                "body_id": self.board,
                "decision": "approved",
                "evidence": {
                    "delegation_ref": self.delegation,
                    "manifest": f"acceptance/programme/{self.project}.toml",
                    "sha": self.head,
                },
                "subject_kind": "contract",
                "subject_id": contract_id,
            },
            actor=APPROVER,
            read_action="architecture.board_get",
            read_input={"key": "conveyor-architecture-board"},
        )
        return self.invoke(
            "architecture.contracts_approve",
            {
                "id": contract_id,
                "approval_id": decision["approval_id"],
                "delegation_ref": self.delegation,
            },
            actor=APPROVER,
            read_action="architecture.contracts_get",
            read_input={"id": contract_id},
            if_match=version,
        )


def self_test() -> None:
    """The tranche label, the demo cadence and the acts path reach every payload."""

    def runner(**kwargs) -> Runner:
        return Runner(
            "S00",
            {},
            ROOT / "docs" / "evidence" / "rework" / "S00" / "acts.md",
            True,
            **kwargs,
        )

    default = runner()
    if default.demo_cadence != DEMO_CADENCE or "tranche 0" not in default.reopen_reason():
        raise SystemExit("self-test: the default shape is no longer tranche 0")

    second = runner(
        demo_cadence="demo-cadence-1",
        tranche_label="tranche 1",
        acts_path=ROOT / "docs" / "evidence" / "rework" / "tranche-1" / "acts.md",
        today="2026-11-22",
    )
    for text in (second.reopen_reason(), second.close_reason()):
        if "tranche 0" in text or "tranche 1" not in text:
            raise SystemExit(f"self-test: {text!r} names the wrong tranche")
    if second.acts_ref() != "docs/evidence/rework/tranche-1/acts.md":
        raise SystemExit(f"self-test: acts_ref {second.acts_ref()!r} names the wrong tranche")
    if second.today != "2026-11-22":
        raise SystemExit("self-test: the date is not the one passed")
    if not second.closes_project():
        raise SystemExit("self-test: a project without --keep-active must close")
    if runner(keep_active=True).closes_project():
        raise SystemExit("self-test: --keep-active did not stop the closure")
    payload = default.decision_payload("01a2", "approved in the self-test")
    if payload.get("delegation_ref") != default.delegation:
        raise SystemExit(
            "self-test: a gate decision must cite the published delegation, not decide as service"
        )
    if payload.get("decision") != "approve" or payload.get("approval_id") != "01a2":
        raise SystemExit(f"self-test: the decision payload lost its shape: {payload}")

    # The discharge: only a card whose package the fresh register no longer carries moves.
    class FakeRunner(Runner):
        def __init__(self, *, dry_run: bool) -> None:
            super().__init__(
                "S00", {}, ROOT / "docs" / "evidence" / "rework" / "S00" / "acts.md", dry_run
            )
            self.project_id = "p1"
            self.snapshot = "/tmp/self-test-snap/data"
            self.invoked: list[tuple[str, dict]] = []
            self.move_if_matches: list[int | None] = []

        def snapshot_json(self, name: str) -> dict:
            if name != "work.json":
                return {}
            return {
                "boards": [{"id": "b1", "project_id": "p1", "position": 0}],
                "lists": [
                    {"id": "l-intake", "board_id": "b1", "kind": "intake",
                     "name": "Conformance rework"},
                    {"id": "l-active", "board_id": "b1", "kind": "active", "name": "Active"},
                    {"id": "l-done", "board_id": "b1", "kind": "done", "name": "Done"},
                ],
            }

        def board_cards(self, board_id: str) -> list[dict]:
            return [
                {"id": "c-open", "list_id": "l-intake",
                 "meta": {"rework_key": "open:1", "owner_project": "S00"}},
                {"id": "c-clear", "list_id": "l-intake",
                 "meta": {"rework_key": "clear:1", "owner_project": "S00"}},
                {"id": "c-done", "list_id": "l-done",
                 "meta": {"rework_key": "clear:2", "owner_project": "S00"}},
                {"id": "c-other", "list_id": "l-intake",
                 "meta": {"rework_key": "clear:3", "owner_project": "OTHER"}},
            ]

        def snapshot_now(self, label: str) -> str:
            return "/tmp/self-test-snap/data"

        def _read(self, action: str, payload: dict) -> dict:
            if action == "approvals.list":
                return {"items": []}
            raise SystemExit(f"self-test: unexpected read {action}")

        def invoke(self, action: str, payload: dict, **kwargs) -> dict:
            self.invoked.append((action, payload))
            if action == "cards.move":
                self.move_if_matches.append(kwargs.get("if_match"))
            if action == "approvals.request":
                return {"id": "gate-1"}
            if action == "cards.move":
                return {"id": payload.get("id"), "version": 2}
            return {"id": payload.get("id")}

        def decide(self, approval_id: str, note: str, close_gate: bool = False) -> dict:
            self.invoked.append(("approvals.decide", {"approval_id": approval_id}))
            return {}

    original_build = globals()["build_rework_register"]
    globals()["build_rework_register"] = lambda *args, **kwargs: {
        "packages": [{"key": "open:1", "status": "open"}]
    }
    try:
        fake = FakeRunner(dry_run=False)
        cleared = fake.discharge_rework_cards()
        if cleared != 1:
            raise SystemExit(f"self-test: the discharge cleared {cleared} cards, not one")
        actions = [action for action, _ in fake.invoked]
        if actions != [
            "approvals.request", "approvals.decide", "cards.comment",
            "cards.move", "cards.move",
        ]:
            raise SystemExit(f"self-test: the discharge sequence is {actions}")
        request = fake.invoked[0][1]
        if not request.get("evidence_waiver_reason") or not request.get("require_evidence"):
            raise SystemExit("self-test: the discharge waiver is not carried")
        moves = [payload["list_id"] for action, payload in fake.invoked if action == "cards.move"]
        if moves != ["l-active", "l-done"]:
            raise SystemExit(f"self-test: the discharge moved to {moves}")
        if fake.move_if_matches != [1, 2]:
            raise SystemExit(
                f"self-test: the moves did not carry the card version: {fake.move_if_matches}"
            )
        if any(payload.get("id") == "c-open" for _, payload in fake.invoked):
            raise SystemExit("self-test: an open package's card was discharged")

        dry = FakeRunner(dry_run=True)
        if dry.discharge_rework_cards() != 0 or dry.invoked:
            raise SystemExit("self-test: a dry run wrote a card")
    finally:
        globals()["build_rework_register"] = original_build

    # A row that exists is not a row that is published: an unpublished draft takes the pack's
    # body and is published, and a published row is left alone.
    class DocsRunner(Runner):
        def __init__(self, rows: list[dict]) -> None:
            super().__init__(
                "S00", {}, ROOT / "docs" / "evidence" / "rework" / "S00" / "acts.md", False
            )
            self.project_id = "p1"
            self.rows = rows
            self.invoked: list[tuple[str, dict]] = []
            self.content = {
                "documents": [
                    {"category": cat, "title": f"{cat} title", "body": f"# {cat}\n\nbody"}
                    for cat in ("project-brief", "gap-analysis")
                ]
            }

        def _read(self, action: str, payload: dict) -> dict:
            if action == "documents.list":
                return {
                    "items": [row for row in self.rows if row["category"] == payload["category"]]
                }
            raise SystemExit(f"self-test: unexpected read {action}")

        def invoke(self, action: str, payload: dict, **kwargs) -> dict:
            self.invoked.append((action, payload))
            if action == "documents.update":
                return {"id": payload["id"], "version": 5}
            return {"id": payload.get("id")}

    docs = DocsRunner(
        [
            {"id": "d1", "category": "project-brief", "version": 4, "published_at": None},
            {"id": "d2", "category": "gap-analysis", "version": 1,
             "published_at": "2026-11-26T00:00:00Z"},
        ]
    )
    docs.ensure_documents()
    acts = [(action, payload.get("id")) for action, payload in docs.invoked]
    if acts != [("documents.update", "d1"), ("documents.publish", "d1")]:
        raise SystemExit(f"self-test: an unpublished draft was not replaced and published: {acts}")
    if docs.invoked[0][1].get("body") != "# project-brief\n\nbody":
        raise SystemExit("self-test: the pack's body was not applied to the draft")
    if any(payload.get("id") == "d2" for _, payload in docs.invoked):
        raise SystemExit("self-test: an already-published row was written again")
    print("rework-project: self-test ok")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project")
    parser.add_argument("--content", type=Path)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument(
        "--tranche-label",
        default="tranche 0",
        help="the tranche this batch reworks, e.g. 'tranche 1' (issue #526)",
    )
    parser.add_argument(
        "--demo-cadence",
        default=DEMO_CADENCE,
        help="the demo cadence row this tranche's deliverables are credited to",
    )
    parser.add_argument(
        "--acts",
        type=Path,
        help="the batch acts page (default docs/evidence/rework/<KEY>/acts.md)",
    )
    parser.add_argument(
        "--keep-active",
        action="store_true",
        help="rework but do not close: a project with an open issue is never closed (#526)",
    )
    parser.add_argument(
        "--closure-asset",
        help="the approved closure governed_asset a resumed run names as the destructive "
        "gate's typed evidence (the id the earlier run's close_request created)",
    )
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return
    if not args.project or not args.content:
        parser.error("--project and --content are required")
    content = json.loads(args.content.read_text(encoding="utf-8"))
    evidence = ROOT / "docs" / "evidence" / "rework" / args.project / "acts.md"
    evidence.parent.mkdir(parents=True, exist_ok=True)
    if not evidence.exists():
        today = datetime.now(timezone.utc).strftime("%Y-%m-%d")
        evidence.write_text(
            f"# {args.project} — retrospective rework acts ({today})\n\n"
            f"Delegation: `{DELEGATION}`. {args.tranche_label.capitalize()}.\n\n",
            encoding="utf-8",
        )
    Runner(
        args.project,
        content,
        evidence,
        args.dry_run,
        demo_cadence=args.demo_cadence,
        tranche_label=args.tranche_label,
        acts_path=args.acts,
        keep_active=args.keep_active,
        closure_asset=args.closure_asset,
    ).run()


if __name__ == "__main__":
    main()
