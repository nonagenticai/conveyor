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
import tempfile
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HARNESS = ROOT / "scripts" / "programme-record.py"
PROGRAMME = "01a1d039-0d24-7d16-ae52-4841e96ccfac"
DELEGATION = "01a1ff1f-e4ba-72dc-9c1a-13049933cf14"
TRANCHE_ZERO = "01a1e9e0-ced2-7769-b12f-f6fba0997914"
DEMO_CADENCE = "01a1ff21-7354-7777-996b-4c8cb12f25e9"
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
    ) -> None:
        self.project = project
        self.content = content
        self.evidence = evidence
        self.dry_run = dry_run
        self.demo_cadence = demo_cadence
        self.delegation = delegation
        self.transcript = evidence.parent / "transcript.jsonl"
        self.snapshot = ""
        self.board = ""
        self.head = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True
        ).stdout.strip()
        self.acts = 0

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
                    {
                        "at": now(),
                        "act": self.acts,
                        "action": action,
                        "actor": "e2e" if actor == E2E else "approver",
                        "input": payload,
                        "result_id": result.get("id") or result.get("baseline_id"),
                    },
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
        board = self._read("architecture.board_list", {})
        self.board = board["items"][0]["id"]

        # 1. reopen a closed project, citing the delegation the rework runs under
        if project["status"] == "closed":
            self.invoke(
                "projects.reopen",
                {
                    "id": self.project_id,
                    "reason": "the retrospective MSP/TOGAF/SAFe rework of tranche 0 (#526)",
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
        if set(meta.get("closure_outputs") or []) != {"closure-review"}:
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

        # 5. the A–D artefacts
        existing = self._read(
            "architecture.artifacts_list", {"project_id": self.project_id}
        ).get("items", [])
        titles = {row.get("title") for row in existing}
        for artefact in self.content["artifacts"]:
            if artefact["title"] in titles:
                print(f"  artefact: {artefact['title']} already present")
                continue
            created = self.invoke(
                "architecture.artifacts_create",
                dict(
                    artefact,
                    project_id=self.project_id,
                    meta={"retrospective": True, "delegation_ref": self.delegation},
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

        # 6. the delivery contract
        contract = next(
            (c for c in contracts
             if c.get("kind") == "delivery_contract" and c.get("status") in ("live", "draft")),
            None,
        )
        if contract is None:
            for candidate in self._read(
                "architecture.contracts_list", {"project_id": self.project_id}
            ).get("items", []):
                if candidate.get("kind") == "delivery_contract" and candidate.get("status") in (
                    "live",
                    "draft",
                ):
                    contract = candidate
                    break
        if contract is None:
            requested = self.invoke(
                "architecture.contracts_request",
                {
                    "project_id": self.project_id,
                    "kind": "delivery_contract",
                    "phase_code": "f",
                    "title": self.content["delivery_contract"]["title"],
                    "body": self.content["delivery_contract"]["body"],
                },
                read_action="architecture.contracts_get",
                read_id_from_output="id",
            )
            contract = self.approve_contract(
                requested["id"], "approve the delivery contract", version=1
            )
        elif contract.get("status") == "draft":
            contract = self.approve_contract(
                contract["id"], "approve the delivery contract",
                version=contract.get("version", 1),
            )
        else:
            print("  delivery contract: already live")

        # 7. baseline with the project's one WBS deliverable
        baselines = [
            b for b in self.snapshot_json("msp.json").get("project_baselines", [])
            if b.get("project_id") == self.project_id
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
            self.decide(plan["approval_id"], "approve the project baseline")

        # 8. publish the retrospective closure review output
        outputs = self._published_outputs()
        if not any(
            row.get("category") == "closure-review"
            and row.get("project_id") == self.project_id
            and "retrospective" in (row.get("title") or "")
            for row in outputs
        ):
            review = self.invoke(
                "documents.compose",
                {
                    "title": f"Closure review — {self.project} (retrospective, 2026-11-21)",
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
        outputs = self._published_outputs()
        for category in expected:
            if any(
                row.get("category") == category and row.get("project_id") == self.project_id
                for row in outputs
            ):
                print(f"  {category} output: already published")
                continue
            body = self.content.get(
                "spec_output" if category == "spec" else f"{category.replace('-', '_')}_output"
            )
            if not body:
                fail(f"{category}: no body in the content file")
            doc = self.invoke(
                "documents.compose",
                {
                    "title": f"{category.replace('-', ' ').title()} — {self.project} "
                             f"(retrospective, 2026-11-21)",
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
                if_match=1,
            )

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
            self.invoke(
                "wbs.update",
                {
                    "id": deliverable,
                    "status": "accepted",
                    "evidence": [
                        f"acceptance/programme/{self.project}.toml",
                        f"docs/evidence/rework/{self.project}.json",
                        f"docs/evidence/rework/tranche-0/acts.md",
                    ],
                    "business_value": wbs["business_value"],
                    "time_criticality": wbs["time_criticality"],
                    "risk_reduction": wbs["risk_reduction"],
                    "job_size": wbs["job_size"],
                },
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

        # 11. closure request -> decision -> the destructive close gate -> close
        row = self.project_row()
        if row["status"] != "closed":
            approvals = self._read(
                "approvals.list", {"project_id": self.project_id}
            ).get("items", [])
            approved_closure = [
                approval
                for approval in approvals
                if approval.get("gate") == "closure" and approval.get("status") == "approved"
            ]
            if approved_closure:
                print("  closure gate: already approved")
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
            gate = self.invoke(
                "approvals.request",
                {
                    "gate": "projects.close",
                    "kind": "action",
                    "action": "projects.close",
                    "input": {"id": self.project_id},
                    "description": f"Close {self.project} after the retrospective rework",
                },
                read_action="approvals.list",
                read_match=["gate=projects.close", "status=pending"],
            )
            self.decide(gate["id"], "approve the destructive close", close_gate=True)
            closed = self.invoke(
                "projects.close",
                {"id": self.project_id, "reason": "the retrospective rework of tranche 0 (#526)"},
                approval_id=gate["id"],
                read_action="projects.get",
                read_input={"id": self.project_id},
            )
            print(f"  closed {self.project}: {closed.get('id') or closed.get('project', {}).get('id')}")

        print(f"{self.project}: {self.acts} acts recorded")

    def _published_outputs(self) -> list[dict]:
        path = Path(self.snapshot).parent / "docs" / "outputs.js"
        if not path.exists():
            return []
        text = path.read_text(encoding="utf-8")
        document = json.loads(text[text.find("{"): text.rfind("}") + 1])
        return document.get("outputs", [])

    def decide(self, approval_id: str, note: str, close_gate: bool = False) -> dict:
        self.invoke(
            "approvals.decide",
            {"approval_id": approval_id, "decision": "approve", "note": note},
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


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project", required=True)
    parser.add_argument("--content", type=Path, required=True)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    content = json.loads(args.content.read_text(encoding="utf-8"))
    evidence = ROOT / "docs" / "evidence" / "rework" / args.project / "acts.md"
    evidence.parent.mkdir(parents=True, exist_ok=True)
    if not evidence.exists():
        evidence.write_text(
            f"# {args.project} — retrospective rework acts (2026-11-21)\n\n"
            f"Delegation: `{DELEGATION}`. Tranche 0.\n\n",
            encoding="utf-8",
        )
    Runner(args.project, content, evidence, args.dry_run).run()


if __name__ == "__main__":
    main()
