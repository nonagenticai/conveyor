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
import re
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


def wsjf_for(msp: dict, project: dict, number: int) -> tuple[int, int, int, int]:
    """The PI's four integers: the tranche refinement's re-estimate, else the committed WBS.

    The refinement cadence is the PI's re-estimate of record; the WBS deliverable is created
    from the content pack by the factory, so a member may not have one before its chain runs.
    Reading only the WBS refused every member whose deliverable the plan had not materialised
    yet (five of tranche 4's ten). The refinement wins when it names the project; the WBS is
    the fallback for a tranche that has no refinement row.
    """
    key = str(project.get("key") or "")
    project_id = str(project.get("id") or "")
    for cadence in msp.get("cadences") or []:
        meta = cadence.get("meta") or {}
        if cadence.get("kind") != "refinement" or meta.get("tranche_number") != number:
            continue
        inputs = ((meta.get("refinement") or {}).get("wsjf_inputs") or {}).get(key)
        if not inputs:
            continue
        try:
            return (
                int(inputs["business_value"]),
                int(inputs["time_criticality"]),
                int(inputs["risk_reduction"]),
                int(inputs["job_size"]),
            )
        except (KeyError, TypeError, ValueError) as error:
            raise Refused(
                f"the tranche {number} refinement for {key} has no WSJF: {error}"
            ) from error
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
    raise Refused(
        f"neither the tranche {number} refinement nor the WBS has a WSJF for "
        f"project {key or project_id}"
    )


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
        # Append, never overwrite: one member runs several commands, and the failing one's
        # output must survive the later ones (measured: the publisher's usage error erased
        # the factory's real refusal).
        with log.open("a", encoding="utf-8") as handle:
            handle.write(
                "$ " + " ".join(command) + "\n\n" + result.stdout + "\n" + result.stderr
                + f"\n[exit {result.returncode}]\n\n"
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


def plan_member(project: dict, msp: dict, content_dir: Path, number: int,
                states: dict[int, str]) -> dict:
    key = str(project.get("key"))
    path = content_path(content_dir, key)
    content = load_json(path) if path else None
    closes = closes_project(project, states, content)
    plan = {
        "key": key,
        "status": project.get("status"),
        "tranche": number,
        "content": str(path) if path else None,
        "closes": closes,
    }
    if not path:
        plan["blocked"] = "no content pack"
        return plan
    wsjf = wsjf_for(msp, project, number)
    plan["wsjf"] = list(wsjf)
    if not closes:
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
        # The checker is read-only: a dry run must still validate every pack, or the dry run
        # proves nothing about the content it is about to publish.
        code = run_command(checker, dry_run=False)
        if code != 0:
            member["blocked"] = "content rejected by rework-content-check"
            ready.append(member)
            continue
        ready.append(member)
    return ready


def crash_detail(log: Path, code: int) -> str:
    """The factory's own last word: its first `rework:` line, else the log's last line.

    A non-zero factory exit is a stop, not a measurement. The run either refused before its first
    act (usually the environment) or halted on the act it names; either way nothing the publisher
    or the verifier would read afterwards is the project's state.
    """
    try:
        lines = [
            line.strip()
            for line in log.read_text(encoding="utf-8", errors="replace").splitlines()
        ]
    except OSError:
        lines = []
    for line in lines:
        if line.startswith("rework:"):
            return line[:300]
    tail = [line for line in lines if line]
    return tail[-1][:300] if tail else f"the factory exited {code} without a message"


def fresh_snapshot(sha: str, label: str) -> tuple[Path | None, str | None]:
    """Take a post-run snapshot so the receipt sees what the last write put in the record.

    The verifier derives its whole package register from the snapshot it is handed
    (`verify-project.py` shells the conformance audit against it), so a receipt rendered from a
    pre-run snapshot reports the previous state with full confidence — and a tranche run would
    judge member *n* on a snapshot taken before members 1..n-1 wrote. The factory needs the same
    two variables.
    """
    database = os.environ.get("SNAPSHOT_DATABASE_URL")
    binary = os.environ.get("PROGRAMME_RECORD_BIN")
    if not database or not binary:
        return None, (
            "SNAPSHOT_DATABASE_URL and PROGRAMME_RECORD_BIN are required to re-render a receipt "
            "after a write"
        )
    directory = Path(tempfile.mkdtemp(prefix=f"rework-{label}-snap-"))
    (directory / "data").mkdir()
    (directory / "docs").mkdir()
    completed = subprocess.run(
        [
            binary,
            "snapshot",
            "--tenant",
            os.environ.get("TENANT") or "nonagentic",
            "--out",
            str(directory / "data"),
            "--outputs-dir",
            str(directory / "docs"),
            "--git-sha",
            sha,
            "--issues",
            "docs/design/data/issues.json",
            "--database-url",
            database,
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if completed.returncode != 0:
        return None, "the post-run snapshot failed: " + (
            completed.stderr.strip() or completed.stdout.strip()
        )[:300]
    return directory / "data", None


def publish_member(key: str, number: int, snapshot: Path, log: Path) -> int:
    """Compose and publish the standard's project documents from the record's own rows."""
    return run_command(
        [
            sys.executable,
            str(PUBLISHER),
            "--apply",
            "--project",
            key,
            "--snapshot",
            str(snapshot),
            "--outputs-dir",
            str(snapshot.parent / "docs"),
            "--bodies-out",
            str(EVIDENCE / "batch" / f"tranche-{number}" / f"{key}-bodies"),
            "--local-bin",
            os.environ.get("PROGRAMME_RECORD_BIN", ""),
        ],
        log=log,
        dry_run=False,
    )


def run_one(
    member: dict,
    msp: dict,
    number: int,
    snapshot: Path,
    sha: str,
    dry_run: bool,
    publish: bool,
) -> dict:
    key = member["key"]
    log = EVIDENCE / "batch" / f"tranche-{number}" / f"{key}.log"
    if not dry_run:
        log.unlink(missing_ok=True)
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
        # Measured 2026-11-26: a three-second environment abort was published as "the factory ran
        # and closed one package", because the publisher and the receipt ran anyway. A crash is
        # BLOCKED with the factory's own line; nothing downstream may read the untouched rows.
        detail = crash_detail(log, code)
        # One exception, and it is a dependency rather than a retry: the close gate reads
        # PUBLISHED documents (`project-registers`, `architecture-artifact`) that only the
        # publisher composes from the rows the factory has just written. Publishing once and
        # resuming the factory is the only order in which a standard-bound project can close.
        if not (publish and not dry_run and "closure_mandatory_artefacts_missing" in detail):
            return {
                "key": key,
                "state": "BLOCKED",
                "factory_exit": code,
                "publish_exit": None,
                "gaps": [{"id": None, "gap": "instrument", "detail": detail}],
                "log": str(log.relative_to(ROOT)),
            }
        mid, error = fresh_snapshot(sha, key)
        if error:
            return {
                "key": key,
                "state": "BLOCKED",
                "factory_exit": code,
                "publish_exit": None,
                "gaps": [{"id": None, "gap": "instrument", "detail": error}],
                "log": str(log.relative_to(ROOT)),
            }
        member["publish_exit"] = publish_member(key, number, mid, log)
        code = run_command(factory, log=log, dry_run=dry_run)
        if code != 0:
            return {
                "key": key,
                "state": "BLOCKED",
                "factory_exit": code,
                "publish_exit": member["publish_exit"],
                "gaps": [{"id": None, "gap": "instrument", "detail": crash_detail(log, code)}],
                "log": str(log.relative_to(ROOT)),
            }
    receipt_snapshot = snapshot
    if not dry_run:
        fresh, error = fresh_snapshot(sha, key)
        if error:
            return {
                "key": key,
                "state": "BLOCKED",
                "factory_exit": 0,
                "publish_exit": None,
                "gaps": [{"id": None, "gap": "instrument", "detail": error}],
                "log": str(log.relative_to(ROOT)),
            }
        receipt_snapshot = fresh
    if publish and not dry_run:
        # The publisher refuses without its snapshot, its outputs directory and the record
        # binary. A publish that cannot run is a recorded failure, never a silently skipped
        # step: measured, the batch ran ten members with the publisher erroring out each time.
        code = publish_member(key, number, receipt_snapshot, log)
        if code != 0:
            member["publish_exit"] = code
        # The publisher writes documents too; the receipt must see them.
        after, error = fresh_snapshot(sha, key)
        if error:
            return {
                "key": key,
                "state": "BLOCKED",
                "factory_exit": 0,
                "publish_exit": member.get("publish_exit", 0),
                "gaps": [{"id": None, "gap": "instrument", "detail": error}],
                "log": str(log.relative_to(ROOT)),
            }
        receipt_snapshot = after
    verdict, gaps = verify_member(key, receipt_snapshot, dry_run)
    return {
        "key": key,
        "state": verdict,
        "factory_exit": member.get("factory_exit", 0),
        "publish_exit": member.get("publish_exit", 0),
        "gaps": gaps,
        "log": str(log.relative_to(ROOT)),
    }


def load_snapshot_sha(snapshot: Path) -> str:
    manifest = load_json(snapshot / "manifest.json")
    sha = str(manifest.get("git_sha") or "")
    if not sha:
        raise Refused(f"snapshot {snapshot} has no manifest git_sha")
    return sha


def issue_states(snapshot: Path) -> dict[int, str]:
    """The linked issues' states at the snapshot's own sha, never today's registry."""
    sha = load_snapshot_sha(snapshot)
    data: dict = {}
    try:
        shown = subprocess.run(
            ["git", "-C", str(ROOT), "show", f"{sha}:docs/design/data/issues.json"],
            capture_output=True, text=True, check=True,
        ).stdout
        data = json.loads(shown)
    except (subprocess.CalledProcessError, json.JSONDecodeError):
        path = ROOT / "docs" / "design" / "data" / "issues.json"
        if path.exists():
            data = load_json(path)
    states: dict[int, str] = {}
    for issue in data.get("issues") or []:
        number = issue.get("number")
        if number is not None:
            states[int(number)] = str(issue.get("state") or "").upper()
    return states


def manifest_status(key: str) -> str | None:
    """The acceptance manifest's pinned target status for `<KEY>-project`, when it has one.

    The project's acceptance is the authored statement of what closing it means; a manifest
    that pins `active` holds the project open whatever the pack's closure content says
    (measured: S05, held open until its six gate properties execute), and one that pins
    `closed` requires the pack to supply closure content.
    """
    path = ROOT / "acceptance" / "programme" / f"{key}.toml"
    if not path.exists():
        return None
    text = path.read_text(encoding="utf-8")
    match = re.search(
        r'\[\[check\]\]\s*id\s*=\s*"' + re.escape(key) + r'-project".*?status\s*=\s*"(\w+)"',
        text,
        re.S,
    )
    return match.group(1) if match else None


def closes_project(project: dict, states: dict[int, str], content: dict | None) -> bool:
    """The pack declares the close; every linked issue must be closed.

    The live status is not the plan. A retrospective that was refused its close on a red
    manifest leaves the row active (measured: T9) while the programme's seed and its closed
    issues still say closed; planning from the live row demanded a stay-active pack for a
    project that must close once green, and the checker refused the authored pack. The pack
    states this tranche's intent — closure and closure_review present — and an open linked
    issue vetoes it. `meta.issues` lists numbers, not states, so the registry at the
    snapshot's sha supplies the states. A manifest that pins `<KEY>-project`'s status wins.
    """
    key = str(project.get("key") or "")
    pinned = manifest_status(key)
    if pinned == "active":
        return False
    if pinned in {"closed", "archived"}:
        return True
    if not content or not content.get("closure") or not content.get("closure_review"):
        return False
    numbers = (project.get("meta") or {}).get("issues") or []
    return all(states.get(int(number)) == "CLOSED" for number in numbers)


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
        states = issue_states(snapshot)
        plan = [
            plan_member(project, msp, Path(args.content_dir), args.tranche, states)
            for project in pending
        ]
        plan = preflight(plan, snapshot, args.dry_run)

        def record(entry: dict) -> None:
            # A dry run writes nothing: its journal entries would make the real run resume-skip
            # members it never acted on.
            if not args.dry_run:
                journal_append(journal, entry)

        for member in plan:
            record(
                {
                    "key": member["key"],
                    "tranche": args.tranche,
                    "snapshot_sha": sha,
                    "state": "PREFLIGHTED",
                    "blocked": member.get("blocked"),
                    "content": member.get("content"),
                }
            )
        blocked = [member for member in plan if member.get("blocked")]
        if blocked:
            for member in blocked:
                record(
                    {
                        "key": member["key"],
                        "tranche": args.tranche,
                        "snapshot_sha": sha,
                        "state": "BLOCKED",
                        "reason": member["blocked"],
                    }
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
            record(
                {
                    "key": member["key"],
                    "tranche": args.tranche,
                    "snapshot_sha": sha,
                    "state": "RUNNING",
                }
            )
            result = run_one(member, msp, args.tranche, snapshot, sha, args.dry_run, args.publish)
            record(
                {
                    "key": member["key"],
                    "tranche": args.tranche,
                    "snapshot_sha": sha,
                    "state": result["state"],
                    "factory_exit": result["factory_exit"],
                    "gaps": result["gaps"],
                    "log": result["log"],
                }
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
                        {"id": "d4", "kind": "demo", "meta": {"tranche_number": 4}},
                        {
                            "id": "r4",
                            "kind": "refinement",
                            "meta": {
                                "tranche_number": 4,
                                "refinement": {
                                    "wsjf_inputs": {
                                        "A": {
                                            "business_value": 7,
                                            "time_criticality": 6,
                                            "risk_reduction": 4,
                                            "job_size": 9,
                                        }
                                    }
                                },
                            },
                        },
                    ],
                    "wbs_nodes": [
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
        msp = load_json(snapshot / "msp.json")
        assert wsjf_for(msp, {"key": "A", "id": "pa"}, 4) == (7, 6, 4, 9)
        assert wsjf_for(msp, {"key": "B", "id": "pb"}, 4) == (5, 5, 5, 13)
        try:
            wsjf_for(msp, {"key": "Z", "id": "pz"}, 4)
        except Refused:
            pass
        else:
            raise SystemExit("self-test: a member with no WSJF was not refused")
        closing = {"closure": {"decision": "close"}, "closure_review": {"decision": "approve"}}
        # The pinned defect: T9's live row is active because its close was refused on a red
        # manifest, while the pack and the closed issues still plan the close.
        assert closes_project({"status": "active", "meta": {"issues": [1]}}, {1: "CLOSED"}, closing)
        assert not closes_project({"status": "active", "meta": {"issues": [2]}}, {2: "OPEN"}, closing)
        assert not closes_project({"status": "active", "meta": {"issues": [3]}}, {}, closing)
        assert not closes_project({"status": "closed", "meta": {"issues": []}}, {}, {})
        assert not closes_project(
            {"status": "closed", "meta": {"issues": [4]}}, {4: "CLOSED"}, {"closure": {}}
        )

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

        # A crash is BLOCKED with the factory's own line — never a receipt over untouched rows.
        crash_log = root / "crash.log"
        crash_log.write_text(
            "rework: SNAPSHOT_DATABASE_URL is required to build the review read-back snapshot\n",
            encoding="utf-8",
        )
        assert crash_detail(crash_log, 1).startswith("rework: SNAPSHOT_DATABASE_URL is required")
        assert crash_detail(root / "absent.log", 7).endswith("without a message")

        # A receipt without a post-run snapshot would measure the previous run's rows.
        saved = {
            name: os.environ.pop(name, None)
            for name in ("SNAPSHOT_DATABASE_URL", "PROGRAMME_RECORD_BIN")
        }
        try:
            path, error = fresh_snapshot("abc123", "A")
            assert path is None and error and "SNAPSHOT_DATABASE_URL" in error
        finally:
            for name, value in saved.items():
                if value is not None:
                    os.environ[name] = value

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
            {},
        )
        assert missing.get("blocked") == "no content pack"
    print("rework-batch: self-test ok — roster, refusal, resume, classification, crash, snapshot, lock")


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
