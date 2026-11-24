#!/usr/bin/env python3
"""Run the retrospective rework for a tranche (or a key list) through the per-project factory.

`rework-project.py` reworks ONE project and is resumable. This driver is the missing queue: it
reads the roster from a pinned snapshot, refuses a tranche the record has not authorised, checks
every content pack before the first write, runs the factory one project at a time, publishes the
member's artefact documents, re-reads the receipt, and classifies every remaining failure as a
record, code or instrument gap. Every transition is appended to a journal, so a killed run
resumes where it stopped instead of re-running the tranche.

Usage:
    python3 scripts/rework-batch.py --tranche 4 --snapshot /tmp/snap-526/data \
        --content-dir docs/evidence/rework --journal docs/evidence/rework/batch/tranche-4.jsonl
    python3 scripts/rework-batch.py --tranche 4 ... --dry-run
    python3 scripts/rework-batch.py --self-test

Environment (never printed): PROGRAMME_RECORD_BIN, SNAPSHOT_DATABASE_URL, TENANT, APP_KEY,
CONVEYOR_E2E, CONVEYOR_APPROVER. The driver inherits them; it refuses to run without the binary.

A receipt is the only thing that calls a project green. The driver never edits a manifest, never
weakens a check, and never treats a record read as a product proof.
"""

from __future__ import annotations

import argparse
import fcntl
import hashlib
import json
import os
import socket
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FACTORY = ROOT / "scripts" / "rework-project.py"
CHECKER = ROOT / "scripts" / "rework-content-check.py"
PUBLISHER = ROOT / "scripts" / "publish-artefact-bodies.py"
VERIFIER = ROOT / "scripts" / "verify-project.py"
EVIDENCE = ROOT / "docs" / "evidence" / "rework"

# The only states a project may be in. A green receipt is terminal for a snapshot: re-running it
# costs acts and can only move a green member backwards.
STATES = (
    "DISCOVERED",
    "PREFLIGHTED",
    "CONTENT_READY",
    "RUNNING",
    "TRANSCRIBED",
    "RECEIPT_GREEN",
    "RECEIPT_RED",
    "BLOCKED",
    "REFUSED",
)


class Refused(SystemExit):
    """A refusal is a result, not a crash: it names the rule that stopped the run."""


def now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def load_json(path: Path) -> dict:
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def write_json(path: Path, value: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    with tmp.open("w", encoding="utf-8") as handle:
        json.dump(value, handle, indent=2, sort_keys=True)
        handle.write("\n")
        handle.flush()
        os.fsync(handle.fileno())
    tmp.replace(path)


def snapshot_projects(snapshot: Path) -> list[dict]:
    programme = load_json(snapshot / "programme.json")
    projects = programme.get("projects")
    if not isinstance(projects, list):
        raise Refused(f"snapshot {snapshot} carries no projects list")
    return projects


def tranche_members(projects: list[dict], number: int) -> list[dict]:
    members = []
    for project in projects:
        meta = project.get("meta") or {}
        tranche = meta.get("tranche")
        try:
            if tranche is not None and int(tranche) == number:
                members.append(project)
        except (TypeError, ValueError):
            continue
    return sorted(members, key=lambda project: str(project.get("key")))


def tranche_row(msp: dict, number: int) -> dict | None:
    for tranche in msp.get("tranches") or []:
        if int(tranche.get("number", -1)) == number:
            return tranche
    return None


def ensure_authorised(msp: dict, number: int) -> dict:
    """Delivery is authorised per tranche. A planned tranche may not be reworked."""
    row = tranche_row(msp, number)
    if row is None:
        raise Refused(f"the record has no tranche {number}")
    state = row.get("state")
    if state not in ("authorised", "closed", "in_review"):
        raise Refused(
            f"tranche {number} is {state!r}; only authorised/closed/in_review may be reworked"
        )
    return row


def demo_cadence(msp: dict, number: int) -> str:
    for cadence in msp.get("cadences") or []:
        meta = cadence.get("meta") or {}
        if cadence.get("kind") == "demo" and meta.get("tranche_number") == number:
            return str(cadence.get("id"))
    raise Refused(f"tranche {number} has no demo cadence; the tranche readiness step is missing")


def wsjf_for(msp: dict, project_id: str) -> tuple[int, int, int, int]:
    """The PI plan's four integers for the project's deliverable, read from the committed WBS."""
    for node in msp.get("wbs_nodes") or []:
        if node.get("project_id") != project_id or node.get("kind") != "deliverable":
            continue
        try:
            return (
                int(node["business_value"]),
                int(node["time_criticality"]),
                int(node["risk_reduction"]),
                int(node["job_size"]),
            )
        except (KeyError, TypeError, ValueError) as error:
            raise Refused(f"the WBS deliverable for {project_id} has no WSJF: {error}") from error
    raise Refused(f"the PI plan has no WBS deliverable for project {project_id}")


def content_path(content_dir: Path, key: str) -> Path | None:
    matches = sorted(content_dir.rglob(f"rework-content-{key}.json"))
    return matches[-1] if matches else None


def journal_entries(journal: Path) -> dict[str, dict]:
    """The last entry per project. A missing journal is an empty run."""
    latest: dict[str, dict] = {}
    if not journal.exists():
        return latest
    with journal.open(encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            entry = json.loads(line)
            latest[str(entry["key"])] = entry
    return latest


def journal_append(journal: Path, entry: dict) -> None:
    journal.parent.mkdir(parents=True, exist_ok=True)
    entry = {"ts": now(), **entry}
    with journal.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(entry, sort_keys=True) + "\n")
        handle.flush()
        os.fsync(handle.fileno())


def resume_set(members: list[dict], latest: dict[str, dict], snapshot_sha: str) -> list[dict]:
    """Projects that still need work at this snapshot; a green receipt is never re-run."""
    pending = []
    for project in members:
        key = str(project.get("key"))
        last = latest.get(key)
        if last and last.get("state") == "RECEIPT_GREEN" and last.get("snapshot_sha") == snapshot_sha:
            continue
        pending.append(project)
    return pending


def classify(checks: list[dict]) -> list[dict]:
    """Every failing check becomes a typed gap, never prose.

    - a snapshot check whose row is absent is a RECORD gap the factory can close;
    - a command that cannot start (127/126/124) or raises before asserting is an INSTRUMENT gap;
    - a command that runs and fails an assertion is a CODE gap the product must fix.
    """
    gaps: list[dict] = []
    for check in checks:
        if check.get("result") == "pass":
            continue
        detail = str(check.get("detail") or "")
        kind = check.get("kind") or "snapshot"
        if kind == "snapshot":
            if "Traceback" in detail or "no such file" in detail.lower():
                gap = "instrument"
            else:
                gap = "record"
        else:
            lowered = detail.lower()
            if any(marker in detail for marker in ("exit 124", "exit 126", "exit 127")):
                gap = "instrument"
            elif "Traceback (most recent call last)" in detail and "AssertionError" not in detail:
                gap = "instrument"
            elif any(marker in lowered for marker in ("command not found", "no such file")):
                gap = "instrument"
            else:
                gap = "code"
        gaps.append(
            {
                "id": check.get("id"),
                "severity": check.get("severity"),
                "kind": kind,
                "gap": gap,
                "detail": detail[:400],
            }
        )
    return gaps


def lock_journal(journal: Path) -> int:
    """One writer. A second driver on the same journal is refused, not queued."""
    journal.parent.mkdir(parents=True, exist_ok=True)
    fd = os.open(str(journal) + ".lock", os.O_CREAT | os.O_RDWR, 0o600)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError as error:
        os.close(fd)
        raise Refused(f"another batch driver holds {journal}.lock") from error
    os.ftruncate(fd, 0)
    os.write(fd, f"{socket.gethostname()} {os.getpid()} {now()}\n".encode())
    return fd


def dirty_paths() -> list[str]:
    result = subprocess.run(
        ["git", "status", "--porcelain", "--untracked-files=no"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    return [line for line in result.stdout.splitlines() if line.strip()]


def refuse_dirty() -> None:
    dirty = dirty_paths()
    if dirty:
        raise Refused(
            "the lane is dirty; commit or reset before a batch run:\n  " + "\n  ".join(dirty[:12])
        )


def run_command(command: list[str], log: Path | None = None, dry_run: bool = False) -> int:
    if dry_run:
        print("DRY-RUN " + " ".join(command))
        return 0
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, check=False)
    if log is not None:
        log.parent.mkdir(parents=True, exist_ok=True)
        log.write_text(
            "$ " + " ".join(command) + "\n\n" + result.stdout + "\n" + result.stderr,
            encoding="utf-8",
        )
    if result.stdout:
        print(result.stdout.rstrip())
    if result.returncode != 0 and result.stderr:
        print(result.stderr.rstrip(), file=sys.stderr)
    return result.returncode


def verify_member(key: str, snapshot: Path, dry_run: bool) -> tuple[str, list[dict]]:
    """Run the project's own manifest and read the receipt it wrote."""
    code = run_command(
        [
            sys.executable,
            str(VERIFIER),
            "--project",
            key,
            "--snapshot",
            str(snapshot),
            "--evidence-dir",
            str(EVIDENCE),
        ],
        dry_run=dry_run,
    )
    receipt = EVIDENCE / f"{key}.json"
    if dry_run:
        return "DRY", []
    if not receipt.exists():
        return "BLOCKED", [{"id": None, "gap": "instrument", "detail": "no receipt written"}]
    data = load_json(receipt)
    gaps = classify(data.get("checks") or [])
    if data.get("verdict") == "green" and code == 0 and not gaps:
        return "RECEIPT_GREEN", []
    return "RECEIPT_RED", gaps


def plan_member(project: dict, msp: dict, content_dir: Path, number: int) -> dict:
    key = str(project.get("key"))
    meta = project.get("meta") or {}
    path = content_path(content_dir, key)
    plan = {
        "key": key,
        "status": project.get("status"),
        "tranche": number,
        "content": str(path) if path else None,
        "closes": project.get("status") == "closed" and not (meta.get("issues") or []),
    }
    if not path:
        plan["blocked"] = "no content pack"
        return plan
    wsjf = wsjf_for(msp, str(project.get("id")))
    plan["wsjf"] = list(wsjf)
    if project.get("status") == "active" or meta.get("issues"):
        plan["keep_active"] = True
        plan["closes"] = False
    return plan


def preflight(plan: list[dict], snapshot: Path, dry_run: bool) -> list[dict]:
    """Refuse every invalid content pack before the first act of the tranche."""
    ready = []
    for member in plan:
        if member.get("blocked"):
            ready.append(member)
            continue
        checker = [
            sys.executable,
            str(CHECKER),
            "--project",
            member["key"],
            "--content",
            member["content"],
            "--snapshot",
            str(snapshot),
            "--wsjf",
            ",".join(str(part) for part in member["wsjf"]),
            "--closes",
            "true" if member["closes"] else "false",
        ]
        code = run_command(checker, dry_run=dry_run)
        if code != 0:
            member["blocked"] = "content rejected by rework-content-check"
            ready.append(member)
            continue
        ready.append(member)
    return ready


def run_one(member: dict, msp: dict, number: int, snapshot: Path, dry_run: bool, publish: bool) -> dict:
    key = member["key"]
    log = EVIDENCE / "batch" / f"tranche-{number}" / f"{key}.log"
    factory = [
        sys.executable,
        str(FACTORY),
        "--project",
        key,
        "--content",
        member["content"],
        "--tranche-label",
        f"tranche {number}",
        "--demo-cadence",
        demo_cadence(msp, number),
    ]
    if member.get("keep_active"):
        factory.append("--keep-active")
    code = run_command(factory, log=log, dry_run=dry_run)
    if code != 0:
        member["factory_exit"] = code
    if publish and not dry_run:
        run_command(
            [
                sys.executable,
                str(PUBLISHER),
                "--apply",
                "--project",
                key,
            ],
            log=log,
            dry_run=False,
        )
    verdict, gaps = verify_member(key, snapshot, dry_run)
    return {
        "key": key,
        "state": verdict,
        "factory_exit": member.get("factory_exit", 0),
        "gaps": gaps,
        "log": str(log.relative_to(ROOT)),
    }


def load_snapshot_sha(snapshot: Path) -> str:
    manifest = load_json(snapshot / "manifest.json")
    sha = str(manifest.get("git_sha") or "")
    if not sha:
        raise Refused(f"snapshot {snapshot} has no manifest git_sha")
    return sha


def execute(args: argparse.Namespace) -> int:
    snapshot = Path(args.snapshot)
    projects = snapshot_projects(snapshot)
    msp = load_json(snapshot / "msp.json")
    sha = load_snapshot_sha(snapshot)
    journal = Path(args.journal)

    if args.tranche is not None:
        ensure_authorised(msp, args.tranche)
        members = tranche_members(projects, args.tranche)
    else:
        keys = {part.strip() for part in args.projects.split(",") if part.strip()}
        members = [project for project in projects if project.get("key") in keys]
        missing = keys - {project.get("key") for project in members}
        if missing:
            raise Refused(f"unknown project keys: {sorted(missing)}")
        args.tranche = int((members[0].get("meta") or {}).get("tranche") or 0)
    if not members:
        raise Refused(f"no projects in tranche {args.tranche}")
    if not args.dry_run:
        refuse_dirty()

    fd = lock_journal(journal) if not args.dry_run else -1
    try:
        latest = journal_entries(journal)
        pending = members if args.repair else resume_set(members, latest, sha)
        plan = [plan_member(project, msp, Path(args.content_dir), args.tranche) for project in pending]
        plan = preflight(plan, snapshot, args.dry_run)

        for member in plan:
            journal_append(
                journal,
                {
                    "key": member["key"],
                    "tranche": args.tranche,
                    "snapshot_sha": sha,
                    "state": "PREFLIGHTED",
                    "blocked": member.get("blocked"),
                    "content": member.get("content"),
                },
            )
        blocked = [member for member in plan if member.get("blocked")]
        if blocked:
            for member in blocked:
                journal_append(
                    journal,
                    {
                        "key": member["key"],
                        "tranche": args.tranche,
                        "snapshot_sha": sha,
                        "state": "BLOCKED",
                        "reason": member["blocked"],
                    },
                )
            print(
                f"rework-batch: {len(blocked)} of {len(plan)} blocked before the first act; "
                "nothing was written for them",
                file=sys.stderr,
            )

        results = []
        for member in plan:
            if member.get("blocked"):
                results.append({"key": member["key"], "state": "BLOCKED", "gaps": []})
                continue
            journal_append(
                journal,
                {
                    "key": member["key"],
                    "tranche": args.tranche,
                    "snapshot_sha": sha,
                    "state": "RUNNING",
                },
            )
            result = run_one(member, msp, args.tranche, snapshot, args.dry_run, args.publish)
            journal_append(
                journal,
                {
                    "key": member["key"],
                    "tranche": args.tranche,
                    "snapshot_sha": sha,
                    "state": result["state"],
                    "factory_exit": result["factory_exit"],
                    "gaps": result["gaps"],
                    "log": result["log"],
                },
            )
            results.append(result)
            print(f"rework-batch: {member['key']} {result['state']}")

        summary = {
            "tranche": args.tranche,
            "snapshot_sha": sha,
            "generated_at": now(),
            "members": results,
            "green": sum(1 for result in results if result["state"] == "RECEIPT_GREEN"),
            "red": sum(1 for result in results if result["state"] == "RECEIPT_RED"),
            "blocked": sum(1 for result in results if result["state"] == "BLOCKED"),
        }
        if not args.dry_run:
            write_json(EVIDENCE / "batch" / f"tranche-{args.tranche}.json", summary)
        print(json.dumps({key: summary[key] for key in ("green", "red", "blocked")}, sort_keys=True))
        return 0 if summary["red"] == 0 and summary["blocked"] == 0 else 1
    finally:
        if fd >= 0:
            os.close(fd)


def self_test() -> None:
    """The four controls the batch depends on, with no record and no database."""
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        snapshot = root / "data"
        snapshot.mkdir()
        (snapshot / "manifest.json").write_text(json.dumps({"git_sha": "abc123"}))
        (snapshot / "programme.json").write_text(
            json.dumps(
                {
                    "projects": [
                        {"key": "A", "id": "pa", "status": "closed", "meta": {"tranche": 4}},
                        {"key": "B", "id": "pb", "status": "active", "meta": {"tranche": 4}},
                        {"key": "C", "id": "pc", "status": "closed", "meta": {"tranche": 5}},
                    ]
                }
            )
        )
        (snapshot / "msp.json").write_text(
            json.dumps(
                {
                    "tranches": [
                        {"number": 4, "state": "authorised"},
                        {"number": 5, "state": "planned"},
                    ],
                    "cadences": [
                        {"id": "d4", "kind": "demo", "meta": {"tranche_number": 4}}
                    ],
                    "wbs_nodes": [
                        {
                            "project_id": "pa",
                            "kind": "deliverable",
                            "business_value": 8,
                            "time_criticality": 5,
                            "risk_reduction": 3,
                            "job_size": 8,
                        },
                        {
                            "project_id": "pb",
                            "kind": "deliverable",
                            "business_value": 5,
                            "time_criticality": 5,
                            "risk_reduction": 5,
                            "job_size": 13,
                        },
                    ],
                }
            )
        )
        projects = snapshot_projects(snapshot)
        assert [p["key"] for p in tranche_members(projects, 4)] == ["A", "B"]
        try:
            ensure_authorised(load_json(snapshot / "msp.json"), 5)
        except Refused:
            pass
        else:
            raise SystemExit("self-test: a planned tranche was not refused")
        assert demo_cadence(load_json(snapshot / "msp.json"), 4) == "d4"
        assert wsjf_for(load_json(snapshot / "msp.json"), "pa") == (8, 5, 3, 8)

        journal = root / "run.jsonl"
        for entry in (
            {"key": "A", "state": "RECEIPT_GREEN", "snapshot_sha": "abc123"},
            {"key": "B", "state": "RECEIPT_RED", "snapshot_sha": "abc123"},
            {"key": "A", "state": "RECEIPT_GREEN", "snapshot_sha": "abc123"},
        ):
            journal_append(journal, entry)
        latest = journal_entries(journal)
        assert load_snapshot_sha(snapshot) == "abc123"
        assert [p["key"] for p in resume_set(projects[:2], latest, "abc123")] == ["B"]
        assert [p["key"] for p in resume_set(projects[:2], latest, "def456")] == ["A", "B"]

        gaps = classify(
            [
                {"id": "s", "kind": "snapshot", "result": "fail", "detail": "no row"},
                {
                    "id": "i",
                    "kind": "command",
                    "result": "fail",
                    "detail": "exit 127: command not found",
                },
                {
                    "id": "c",
                    "kind": "command",
                    "result": "fail",
                    "detail": "AssertionError: expected a run",
                },
            ]
        )
        assert [gap["gap"] for gap in gaps] == ["record", "instrument", "code"]

        fd = lock_journal(journal)
        try:
            try:
                lock_journal(journal)
            except Refused:
                pass
            else:
                raise SystemExit("self-test: a second writer was not refused")
        finally:
            os.close(fd)

        missing = plan_member(
            {"key": "Z", "id": "pz", "status": "closed", "meta": {"tranche": 4}},
            load_json(snapshot / "msp.json"),
            root / "empty",
            4,
        )
        assert missing.get("blocked") == "no content pack"
    print("rework-batch: self-test ok — roster, refusal, resume, classification, lock")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tranche", type=int)
    parser.add_argument("--projects", default="", help="comma-separated keys when not a tranche")
    parser.add_argument("--snapshot", type=Path)
    parser.add_argument("--content-dir", type=Path, default=EVIDENCE)
    parser.add_argument("--journal", type=Path)
    parser.add_argument("--repair", action="store_true", help="re-run a project whose acts were refused")
    parser.add_argument("--publish", action="store_true", help="publish the member's artefact bodies")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return
    if not args.snapshot:
        parser.error("--snapshot is required")
    if args.tranche is None and not args.projects:
        parser.error("--tranche or --projects is required")
    if not args.journal:
        parser.error("--journal is required")
    if not args.dry_run and not os.environ.get("PROGRAMME_RECORD_BIN"):
        raise Refused("PROGRAMME_RECORD_BIN is required: the driver never writes SQL")
    raise SystemExit(execute(args))


if __name__ == "__main__":
    main()
