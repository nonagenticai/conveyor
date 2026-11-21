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
    python3 scripts/verify-project.py --project P0 --kind static|live

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
import json
import os
import re
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


CHECKS: dict[str, Callable[[Snapshot, str, dict[str, Any]], tuple[bool, str]]] = {
    "project": check_project,
    "project_baseline": check_project_baseline,
    "closure_outputs": check_closure_outputs,
    "contract": check_contract,
    "review": check_review,
    "closure_gate": check_closure_gate,
    "waivers": check_waivers,
    "benefit_baseline": check_benefit_baseline,
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
            print(f"{'PASS' if ok else 'FAIL'}  {identifier}  {detail}")
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
            "path": str(snapshot_dir),
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
                        "status": "closed",
                        "meta": {"closure_outputs": []},
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
    (snapshot / "data" / "msp.json").write_text(json.dumps({"benefit_profiles": []}))
    (snapshot / "data" / "artefacts.json").write_text(json.dumps({"documents": []}))
    (snapshot / "data" / "manifest.json").write_text(
        json.dumps({"git_sha": "self-test", "generated_at": "2026-11-21T00:00:00Z"})
    )
    (snapshot / "docs" / "outputs.js").write_text(
        'window.CONVEYOR_OUTPUTS = {"schema":"v1","outputs":[]};\n'
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
