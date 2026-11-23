#!/usr/bin/env python3
"""Audit every programme slice against its executable gate.

The universe is SPEC_V2 section 12's T0-T18 rows plus every
``docs/plans/slices/*.md`` plan's ``slice:`` key (S00-S21, including S06a-d).
For a key K the sources are:

* plan      ``docs/plans/slices/K-*.md`` (S keys only)
* manifest  ``acceptance/programme/K.toml``
* receipt   ``docs/evidence/rework/K.json``
* gate text the SPEC section 12 cell, plus the plan body's ``**Gate (...):**``

Gates are executed by the existing ``scripts/verify-project.py`` with its
evidence directory redirected to a temporary directory.  The committed
receipt is read only for provenance (``snapshot.git_sha``, ``generated_at``)
and its verdict.  The live database is never touched.

Statuses
--------
implemented  a non-record check (code/probe) exercises the gate text and
             passes, the gate checks resolve and pass, and the receipt is green.
partial      the receipt is green and the gate passes, but the manifest offers
             only record/generic checks -- ids passing is not SPEC coverage.
absent       no manifest, no receipt, a failed/unresolved gate, no gate text,
             or a probe that the gate names but the manifest does not carry.
broken       the manifest is unreadable, the snapshot is missing, a command
             crashed (import/syntax/usage/timeout), or verify-project produced
             no evidence.

Known limits, encoded not hidden: all manifests share the generic programme
gate_checks; ``gate_problem`` proves ids pass, not that a SPEC gate was
exercised; only a minority of manifests have receipts, so a slice without one
is always ``absent`` with that reason and is never a false green.

Usage
-----
    python3 -I scripts/verify-slices.py --slice S01
    python3 -I scripts/verify-slices.py --all --snapshot /tmp/snap/data \
        --json-out /tmp/slices.json
    python3 -I scripts/verify-slices.py --self-test

JSON (schema ``verify-slices/v1``) is written to stdout; the human table goes
to stderr, so ``... --all > out.json`` still shows the table.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path
from typing import Any, Callable

ROOT = Path(__file__).resolve().parents[1]
SPEC = ROOT / "docs" / "SPEC_V2.md"
SLICE_DIR = ROOT / "docs" / "plans" / "slices"
MANIFEST_DIR = ROOT / "acceptance" / "programme"
RECEIPT_DIR = ROOT / "docs" / "evidence" / "rework"
VERIFY_PROJECT = ROOT / "scripts" / "verify-project.py"
COMPLETED_SNAPSHOT = Path("/tmp/conveyor-w0-526-snap/data")
FALLBACK_SNAPSHOT = ROOT / "docs" / "design" / "data"

SCHEMA = "verify-slices/v1"

# A command that reads the programme through the record tool is an evidence
# read, not an execution of the slice's gate text.
RECORD_MARKER = "programme-record.py --check"

# Exit codes and messages that mean the check itself could not run, as opposed
# to an assertion that legitimately failed.  An AssertionError is a gate
# failure (absent), not a crash (broken).
CRASH_EXITS = {124, 126, 127}
CRASH_MARKERS = (
    "command timed out",
    "ModuleNotFoundError",
    "ImportError",
    "SyntaxError",
    "command not found",
    "Permission denied",
    "No such file or directory",
    # The record transport is deliberately not supplied: the audit must never
    # reach the live database.  A gate that needs it did not execute.
    "is required for the real transport",
)

STOPWORDS = {
    "that", "this", "with", "from", "must", "each", "every", "their", "there",
    "which", "when", "what", "into", "over", "only", "also", "than", "then",
    "they", "them", "been", "being", "does", "have", "will", "shall", "more",
    "less", "such", "some", "same", "other", "where", "while", "within",
    "without", "before", "after", "through", "under", "about", "against",
    "test", "tests", "check", "checks", "project", "programme", "slice",
    "gate", "gates", "exists", "exist", "passes", "pass", "green", "closes",
}


class AuditError(RuntimeError):
    pass


def read_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def parse_spec_gates(path: Path) -> dict[str, str]:
    """SPEC section 12: ``| **T0** | title | gate |`` rows, last cell is the gate."""
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()
    start = None
    for index, line in enumerate(lines):
        if re.match(r"^##\s+12\.\s", line):
            start = index
            break
    if start is None:
        return {}
    gates: dict[str, str] = {}
    for line in lines[start + 1:]:
        if re.match(r"^##\s+", line):
            break
        match = re.match(r"^\|\s*\*\*(T\d+)\*\*\s*\|(.*)\|\s*$", line)
        if not match:
            continue
        cells = [cell.strip() for cell in match.group(2).split("|")]
        if cells and cells[-1]:
            gates[match.group(1)] = cells[-1]
    return gates


def parse_plans(directory: Path) -> dict[str, dict[str, Any]]:
    """``slice:`` frontmatter plus the first ``**Gate (...):**`` body line."""
    plans: dict[str, dict[str, Any]] = {}
    if not directory.is_dir():
        return plans
    for path in sorted(directory.glob("*.md")):
        text = path.read_text(encoding="utf-8")
        front = re.search(r"^slice:\s*(\S+)\s*$", text, re.MULTILINE)
        key = front.group(1) if front else path.stem.split("-", 1)[0]
        gate_match = re.search(r"\*\*Gate[^*]*:\*\*\s*(.+)", text)
        gate = gate_match.group(1).strip() if gate_match else ""
        plans[key] = {"plan": path.name, "gate": gate}
    return plans


def universe(spec_gates: dict[str, str], plans: dict[str, dict[str, Any]]) -> list[str]:
    def t_key(value: str) -> int:
        return int(value[1:])

    ts = sorted((key for key in spec_gates if re.fullmatch(r"T\d+", key)), key=t_key)
    ss = list(plans)  # parse_plans iterates sorted filenames, so S order is natural.
    return ts + [key for key in ss if key not in spec_gates]


def load_manifest(path: Path) -> dict[str, Any]:
    try:
        return tomllib.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as error:
        raise AuditError(f"no manifest at {path}") from error
    except tomllib.TOMLDecodeError as error:
        raise AuditError(f"{path} is not TOML: {error}") from error


def classify(check: dict[str, Any], gate_text: str) -> str:
    """record / code / probe / generic, inferred from the manifest's own shape."""
    kind = str(check.get("kind") or ("command" if "command" in check else "snapshot"))
    command = str(check.get("command") or "")
    if kind == "command":
        if RECORD_MARKER in command:
            return "record"
        haystack = " ".join(
            str(check.get(field) or "") for field in ("id", "description", "command")
        ).lower()
        if gate_text.lower().find("probe") >= 0 and (
            "probe" in haystack or "live call" in haystack
        ):
            return "probe"
        return "code"
    if kind == "snapshot":
        return "generic"
    return "unknown"


def stem(word: str) -> str:
    """Light stemming so ``record``/``recorded`` and ``gate``/``gates`` meet."""
    for suffix in ("ing", "ed", "es", "s"):
        if word.endswith(suffix) and len(word) - len(suffix) >= 3:
            return word[: -len(suffix)]
    return word


def tokens(text: str) -> set[str]:
    words = re.findall(r"[a-z0-9_]{4,}", text.lower())
    return {stem(word) for word in words if word not in STOPWORDS}


def exercises(gate_text: str, check: dict[str, Any]) -> bool:
    """Whether a non-record check's own words meet the gate's words.

    Two distinctive shared tokens are required so a generic programme id such
    as ``project_baseline`` cannot claim a slice gate by accident.
    """
    if not gate_text:
        return False
    haystack = " ".join(
        str(check.get(field) or "") for field in ("id", "description", "command")
    )
    return len(tokens(gate_text) & tokens(haystack)) >= 2


def crashed(record: dict[str, Any]) -> bool:
    detail = str(record.get("detail") or "")
    if record.get("result") == "pass":
        return False
    if record.get("exit") in CRASH_EXITS:
        return True
    stdout = str(record.get("stdout") or "")
    stderr = str(record.get("stderr") or "")
    text = f"{stderr}\n{stdout}\n{detail}"
    return any(marker in text for marker in CRASH_MARKERS)


def run_verify_project(
    key: str,
    manifest_path: Path,
    snapshot_dir: Path,
    evidence_dir: Path,
    timeout: int,
) -> tuple[dict[str, Any] | None, str | None]:
    command = [
        sys.executable,
        "-I",
        str(VERIFY_PROJECT),
        "--project",
        key,
        "--manifest",
        str(manifest_path),
        "--snapshot",
        str(snapshot_dir),
        "--evidence-dir",
        str(evidence_dir),
    ]
    try:
        completed = subprocess.run(
            command,
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        return None, f"verify-project timed out after {timeout}s"
    output = evidence_dir / f"{key}.json"
    if not output.is_file():
        tail = (completed.stderr or completed.stdout or "no output").strip()[-400:]
        return None, f"verify-project produced no evidence (exit {completed.returncode}): {tail}"
    try:
        return read_json(output), None
    except json.JSONDecodeError as error:
        return None, f"verify-project wrote unreadable evidence: {error}"


Runner = Callable[
    [str, Path, Path, Path, int], tuple[dict[str, Any] | None, str | None]
]


def structural_findings(
    manifest: dict[str, Any] | None,
    receipt: dict[str, Any] | None,
    gate_text: str,
    probe_named: bool,
    probe_present: bool,
) -> tuple[int, int]:
    """Obligations beyond individual checks: manifest, receipt, gate, probe."""
    obligations = 4 if probe_named else 3
    unmet = 0
    if manifest is None:
        unmet += 1
    if receipt is None:
        unmet += 1
    if not gate_text:
        unmet += 1
    if probe_named and not probe_present:
        unmet += 1
    return obligations, unmet


def audit_slice(
    key: str,
    root: Path,
    snapshot_dir: Path,
    spec_gates: dict[str, str],
    plans: dict[str, dict[str, Any]],
    runner: Runner,
    timeout: int,
    workdir: Path,
) -> dict[str, Any]:
    plan = plans.get(key)
    manifest_path = root / "acceptance" / "programme" / f"{key}.toml"
    receipt_path = root / "docs" / "evidence" / "rework" / f"{key}.json"
    spec_gate = spec_gates.get(key, "")
    plan_gate = plan.get("gate", "") if plan else ""
    gate_text = " | ".join(part for part in (spec_gate, plan_gate) if part)

    receipt = None
    if receipt_path.is_file():
        try:
            receipt = read_json(receipt_path)
        except json.JSONDecodeError:
            receipt = None
    evidence = {
        "path": str(receipt_path.relative_to(root)) if receipt_path.is_file() else None,
        "git_sha": ((receipt or {}).get("snapshot") or {}).get("git_sha"),
        "generated_at": (receipt or {}).get("generated_at"),
    }

    result: dict[str, Any] = {
        "id": key,
        "spec_gate": spec_gate or None,
        "plan": plan["plan"] if plan else None,
        "manifest": str(manifest_path.relative_to(root)) if manifest_path.is_file() else None,
        "gate_checks": [],
        "checks": [],
        "status": "absent",
        "reason": "",
        "evidence": evidence,
    }

    def finish(status: str, reason: str, checks: list[dict[str, Any]], manifest_obj: dict[str, Any] | None) -> dict[str, Any]:
        probe_named = "probe" in gate_text.lower() or "live call" in gate_text.lower()
        probe_present = any(check.get("kind") == "probe" for check in checks)
        total, unmet = structural_findings(
            manifest_obj, receipt, gate_text, probe_named, probe_present
        )
        result.update(
            {
                "status": status,
                "reason": reason,
                "checks": checks,
                "findings": {
                    "total": len(checks) + total,
                    "unresolved": sum(1 for check in checks if check.get("result") != "pass") + unmet,
                },
            }
        )
        return result

    if not manifest_path.is_file():
        return finish("absent", f"no manifest at acceptance/programme/{key}.toml", [], None)
    try:
        manifest = load_manifest(manifest_path)
    except AuditError as error:
        return finish("broken", str(error), [], None)

    declared = manifest.get("project", {})
    gate_checks = [str(item) for item in declared.get("gate_checks") or []]
    result["gate_checks"] = gate_checks
    manifest_checks = manifest.get("check", [])
    by_id = {str(check.get("id")): check for check in manifest_checks}

    if receipt is None:
        return finish(
            "absent",
            f"no receipt at docs/evidence/rework/{key}.json; the gate is not evidenced",
            [],
            manifest,
        )
    if not snapshot_dir.is_dir():
        return finish("broken", f"snapshot directory {snapshot_dir} is missing", [], manifest)

    evidence_dir = workdir / key
    evidence_dir.mkdir(parents=True, exist_ok=True)
    fresh, error = runner(key, manifest_path, snapshot_dir, evidence_dir, timeout)
    if fresh is None:
        return finish("broken", error or "verify-project produced no evidence", [], manifest)

    fresh_by_id = {str(item.get("id")): item for item in fresh.get("checks", [])}
    checks: list[dict[str, Any]] = []
    for check in manifest_checks:
        identifier = str(check.get("id") or "")
        record = fresh_by_id.get(identifier)
        kind = classify(check, gate_text)
        checks.append(
            {
                "id": identifier,
                "kind": kind,
                "result": record.get("result") if record else "unresolved",
                "detail": record.get("detail") if record else "did not run",
            }
        )
        if record and crashed(record):
            return finish("broken", f"{identifier} crashed: {record.get('detail')}", checks, manifest)

    # Gate resolution: every declared id must exist and pass.
    for identifier in gate_checks:
        if identifier not in by_id:
            return finish(
                "absent",
                f"gate check {identifier!r} resolves to no check in the manifest",
                checks,
                manifest,
            )
        record = fresh_by_id.get(identifier)
        if record is None:
            return finish("absent", f"gate check {identifier!r} did not run", checks, manifest)
        if record.get("result") != "pass":
            return finish(
                "absent",
                f"gate check {identifier!r} did not pass: {record.get('detail')}",
                checks,
                manifest,
            )

    if not gate_text:
        return finish(
            "absent",
            "gate text unresolved: no SPEC section 12 cell and no plan Gate",
            checks,
            manifest,
        )

    probe_named = "probe" in gate_text.lower() or "live call" in gate_text.lower()
    probe_present = any(check["kind"] == "probe" for check in checks)
    if probe_named and not probe_present:
        return finish(
            "absent",
            "the gate names a probe/live call but the manifest carries no probe check",
            checks,
            manifest,
        )

    if str(receipt.get("verdict")) != "green":
        return finish(
            "absent",
            f"committed receipt verdict is {receipt.get('verdict')!r}, not green",
            checks,
            manifest,
        )

    failed = [check for check in checks if check["result"] != "pass"]
    if failed:
        names = ", ".join(check["id"] for check in failed)
        return finish("absent", f"check(s) did not pass: {names}", checks, manifest)

    executed = [check for check in checks if check["kind"] in ("code", "probe")]
    exercising = [
        check
        for check in executed
        if exercises(gate_text, by_id.get(check["id"], {}))
    ]
    if exercising:
        names = ", ".join(check["id"] for check in exercising)
        return finish(
            "implemented",
            f"non-record check(s) {names} exercise the gate text and pass; receipt green",
            checks,
            manifest,
        )
    if not executed:
        kinds = sorted({check["kind"] for check in checks}) or ["none"]
        return finish(
            "partial",
            f"receipt green with only record/generic checks ({', '.join(kinds)}); "
            "ids passing is not SPEC coverage",
            checks,
            manifest,
        )
    names = ", ".join(check["id"] for check in executed)
    return finish(
        "absent",
        f"non-record check(s) {names} do not exercise the gate text",
        checks,
        manifest,
    )


def audit(
    root: Path,
    keys: list[str],
    snapshot_dir: Path,
    runner: Runner = run_verify_project,
    timeout: int = 180,
) -> dict[str, Any]:
    spec_gates = parse_spec_gates(root / "docs" / "SPEC_V2.md")
    plans = parse_plans(root / "docs" / "plans" / "slices")
    known = universe(spec_gates, plans)
    if keys == known:
        ordered = known
    else:
        unknown = [key for key in keys if key not in known]
        if unknown:
            raise AuditError(
                "unknown slice(s): " + ", ".join(unknown) + "; known: " + ", ".join(known)
            )
        ordered = [key for key in known if key in keys]
    audited_sha = "unknown"
    try:
        completed = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=root,
            capture_output=True,
            text=True,
            timeout=10,
        )
        if completed.returncode == 0:
            audited_sha = completed.stdout.strip()
    except (OSError, subprocess.TimeoutExpired):
        pass
    snapshot_manifest = snapshot_dir / "manifest.json"
    snapshot = {"path": str(snapshot_dir), "git_sha": None, "generated_at": None}
    if snapshot_manifest.is_file():
        try:
            declared = read_json(snapshot_manifest)
            snapshot.update(
                {
                    "git_sha": declared.get("git_sha"),
                    "generated_at": declared.get("generated_at"),
                }
            )
        except (OSError, json.JSONDecodeError):
            pass
    slices: list[dict[str, Any]] = []
    with tempfile.TemporaryDirectory(prefix="verify-slices-") as temporary:
        workdir = Path(temporary)
        for key in ordered:
            slices.append(
                audit_slice(
                    key, root, snapshot_dir, spec_gates, plans, runner, timeout, workdir
                )
            )
    summary = {
        status: sum(1 for item in slices if item["status"] == status)
        for status in ("implemented", "partial", "absent", "broken")
    }
    return {
        "schema": SCHEMA,
        "audited_sha": audited_sha,
        "snapshot": snapshot,
        "slices": slices,
        "summary": summary,
    }


def print_table(document: dict[str, Any], stream: Any) -> None:
    rows = [
        (
            "Slice",
            "SPEC gate",
            "Manifest",
            "Checks pass/total",
            "Status",
            "Evidence SHA",
            "Unresolved findings",
        )
    ]
    for item in document["slices"]:
        passed = sum(1 for check in item["checks"] if check["result"] == "pass")
        sha = (item["evidence"] or {}).get("git_sha") or "—"
        rows.append(
            (
                item["id"],
                "yes" if item["spec_gate"] else "—",
                "yes" if item["manifest"] else "—",
                f"{passed}/{len(item['checks'])}",
                item["status"],
                sha[:8],
                str(item["findings"]["unresolved"]),
            )
        )
    widths = [max(len(row[column]) for row in rows) for column in range(len(rows[0]))]
    for row in rows:
        print(
            " | ".join(cell.ljust(widths[index]) for index, cell in enumerate(row)),
            file=stream,
        )
    summary = document["summary"]
    print(
        f"summary: implemented={summary['implemented']} partial={summary['partial']} "
        f"absent={summary['absent']} broken={summary['broken']} "
        f"audited_sha={document['audited_sha']} snapshot_sha={document['snapshot'].get('git_sha')}",
        file=stream,
    )


def _fixture(root: Path, key: str, check: dict[str, Any], gate: str) -> None:
    manifest_dir = root / "acceptance" / "programme"
    slice_dir = root / "docs" / "plans" / "slices"
    receipt_dir = root / "docs" / "evidence" / "rework"
    for directory in (manifest_dir, slice_dir, receipt_dir, root / "docs"):
        directory.mkdir(parents=True, exist_ok=True)
    command = check.pop("_command", "")
    lines = [
        "[project]",
        f'key = "{key}"',
        f'gate = "{gate}"',
        'gate_checks = ["' + check["id"] + '"]',
        "",
        "[[check]]",
        f'id = "{check["id"]}"',
    ]
    if check.get("kind") == "command":
        lines += ['kind = "command"', f"command = '{command}'"]
    else:
        lines.append(f'check = "{check["check"]}"')
    lines.append('severity = "P0"')
    lines.append(f'description = "{check.get("description", "")}"')
    (manifest_dir / f"{key}.toml").write_text("\n".join(lines) + "\n", encoding="utf-8")
    (slice_dir / f"{key}-fixture.md").write_text(
        f"slice: {key}\n\n**Gate (SPEC §8):** {gate}\n", encoding="utf-8"
    )
    (root / "docs" / "SPEC_V2.md").write_text("## 12. Delivery plan — fixtures\n", encoding="utf-8")
    (receipt_dir / f"{key}.json").write_text(
        json.dumps(
            {
                "verdict": "green",
                "generated_at": "2026-11-22T00:00:00+00:00",
                "snapshot": {"git_sha": "f" * 40},
            }
        )
        + "\n",
        encoding="utf-8",
    )


def self_test() -> int:
    failures: list[str] = []
    with tempfile.TemporaryDirectory(prefix="verify-slices-selftest-") as temporary:
        base = Path(temporary)
        snapshot = base / "snapshot"
        (snapshot / "data").mkdir(parents=True)
        (snapshot / "data" / "manifest.json").write_text(
            json.dumps({"git_sha": "f" * 40, "generated_at": "2026-11-22T00:00:00+00:00"}),
            encoding="utf-8",
        )
        (snapshot / "data" / "programme.json").write_text('{"projects": []}\n', encoding="utf-8")

        def fixture_runner(
            key: str,
            manifest_path: Path,
            snapshot_dir: Path,
            evidence_dir: Path,
            timeout: int,
        ) -> tuple[dict[str, Any] | None, str | None]:
            manifest = load_manifest(manifest_path)
            checks = []
            for check in manifest.get("check", []):
                command = str(check.get("command") or "")
                failed = command.strip() == "exit 1"
                checks.append(
                    {
                        "id": check.get("id"),
                        "kind": check.get("kind") or "snapshot",
                        "result": "fail" if failed else "pass",
                        "detail": "exit 1: fixture failure" if failed else "exit 0",
                        "exit": 1 if failed else 0,
                    }
                )
            return {"verdict": "green", "checks": checks}, None

        # Fixture A: a gate whose only check is a command that exits 1 -> absent.
        fixture_a = base / "fixture-a"
        _fixture(
            fixture_a,
            "S00",
            {"id": "x", "kind": "command", "_command": "exit 1", "description": "fixture"},
            "a fixture gate that must fail",
        )
        document_a = audit(fixture_a, ["S00"], snapshot / "data", runner=fixture_runner)
        item_a = document_a["slices"][0]
        if item_a["status"] != "absent":
            failures.append(f"fixture A: status {item_a['status']!r}, expected 'absent'")
        if "did not pass" not in item_a["reason"]:
            failures.append(f"fixture A: reason {item_a['reason']!r} does not name the failed gate")

        # Fixture B: only a record/generic check, green -> partial.
        fixture_b = base / "fixture-b"
        _fixture(
            fixture_b,
            "S01",
            {"id": "y", "check": "project", "description": "a generic programme assertion"},
            "a fixture gate with no exercising check",
        )
        document_b = audit(fixture_b, ["S01"], snapshot / "data", runner=fixture_runner)
        item_b = document_b["slices"][0]
        if item_b["status"] != "partial":
            failures.append(f"fixture B: status {item_b['status']!r}, expected 'partial'")
        if "record/generic" not in item_b["reason"]:
            failures.append(f"fixture B: reason {item_b['reason']!r} does not name record/generic")

    if failures:
        print("verify-slices: SELF-TEST FAILED")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print("verify-slices: self-test OK (fixture A absent, fixture B partial)")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--slice", dest="slice_key", metavar="KEY")
    parser.add_argument("--all", action="store_true")
    parser.add_argument("--snapshot", type=Path)
    parser.add_argument("--json-out", type=Path)
    parser.add_argument("--timeout", type=int, default=180)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    if args.self_test:
        return self_test()
    if not args.slice_key and not args.all:
        parser.error("--slice KEY or --all is required (or --self-test)")
    if args.slice_key and args.all:
        parser.error("--slice and --all are mutually exclusive")
    snapshot = args.snapshot
    if snapshot is None:
        snapshot = COMPLETED_SNAPSHOT if COMPLETED_SNAPSHOT.is_dir() else FALLBACK_SNAPSHOT
    if args.slice_key:
        keys = [args.slice_key]
    else:
        keys = universe(parse_spec_gates(SPEC), parse_plans(SLICE_DIR))
    try:
        document = audit(ROOT, keys, snapshot, timeout=args.timeout)
    except AuditError as error:
        print(f"verify-slices: {error}", file=sys.stderr)
        return 2
    print_table(document, sys.stderr)
    payload = json.dumps(document, indent=2) + "\n"
    if args.json_out:
        args.json_out.write_text(payload, encoding="utf-8")
    else:
        sys.stdout.write(payload)
    return 0


if __name__ == "__main__":
    sys.exit(main())
