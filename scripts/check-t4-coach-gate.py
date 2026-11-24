#!/usr/bin/env python3
"""The T4 coach gate as an executable instrument.

docs/SPEC_V2.md §12 says T4 is done when "an injected failure yields exactly one
coach card and one remedy; a repeat signature escalates; no coach card about a
coach card; a systemic class opens a kata".  The proof lives in
``crates/actions/tests/coach.rs``; this script binds every clause to the tests
that prove it, can run exactly those tests, and can prove that a recorded run
belongs to the current HEAD.

Modes:
  --self-test  assert every named test exists and every clause is mapped (no cargo)
  --run        run the named tests and write the sha-bound evidence file
  --check      validate the evidence against HEAD and the current test file

Dependency-free (standard library only).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TEST_FILE = ROOT / "crates" / "actions" / "tests" / "coach.rs"
EVIDENCE = ROOT / "docs" / "evidence" / "rework" / "T4-coach-gate.json"
PACKAGE = "conveyor-actions"
TARGET = "coach"

#: SPEC_V2 §12's T4 gate, clause by clause, and the tests that prove each one.
CLAUSES: dict[str, tuple[str, ...]] = {
    "an injected failure yields exactly one coach card and one remedy": (
        "an_interruption_yields_one_coach_card_one_triage_and_one_remedy",
        "the_seeded_coach_process_yields_one_card_one_triage_and_one_remedy",
        "a_duplicate_remedy_is_refused",
    ),
    "a repeat signature escalates": (
        "a_repeat_signature_escalates_instead_of_a_second_card",
        "an_escalated_signature_is_not_retried",
    ),
    "no coach card about a coach card": (
        "a_failed_coach_run_never_opens_a_coach_card",
        "a_nested_coach_run_is_refused",
        "a_coach_card_cannot_be_created_through_the_public_path",
        "the_coach_chain_settles_at_one_card",
    ),
    "a systemic class opens a kata": (
        "a_systemic_class_opens_one_measured_kata",
        "a_systemic_class_without_a_pinned_instrument_escalates",
        "a_transient_class_opens_no_kata",
    ),
}

TESTS: tuple[str, ...] = tuple(name for names in CLAUSES.values() for name in names)


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def defined_tests(source: str) -> set[str]:
    """Every ``async fn`` name in the coach test file."""
    return set(re.findall(r"(?m)^async fn (\w+)\(", source))


def digest(blob: bytes) -> str:
    return hashlib.sha256(blob).hexdigest()


def current_head() -> str:
    completed = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if completed.returncode != 0:
        raise SystemExit(f"t4-coach-gate: git rev-parse HEAD failed: {completed.stderr.strip()}")
    return completed.stdout.strip()


def self_test() -> int:
    """The gate's shape: every clause mapped, every named test defined, no duplicates."""
    source = read_text(TEST_FILE)
    defined = defined_tests(source)
    problems: list[str] = []
    seen: dict[str, str] = {}
    for clause, names in CLAUSES.items():
        if not names:
            problems.append(f"clause {clause!r} maps to no test")
        for name in names:
            if name not in defined:
                problems.append(f"{name} is not defined in {TEST_FILE.relative_to(ROOT)}")
            if name in seen:
                problems.append(f"{name} is mapped to both {seen[name]!r} and {clause!r}")
            seen[name] = clause
    if problems:
        for problem in problems:
            print(f"t4-coach-gate: {problem}", file=sys.stderr)
        return 1
    print(f"t4-coach-gate: {len(CLAUSES)} clauses and {len(TESTS)} tests are mapped")
    return 0


def run() -> int:
    """Run exactly the named tests and write the sha-bound evidence."""
    if self_test() != 0:
        return 1
    command = [
        "cargo",
        "test",
        "-p",
        PACKAGE,
        "--test",
        TARGET,
        "--",
        *TESTS,
        "--exact",
    ]
    try:
        completed = subprocess.run(
            command, cwd=ROOT, capture_output=True, text=True, timeout=3600
        )
    except FileNotFoundError:
        print("t4-coach-gate: cargo is not on PATH", file=sys.stderr)
        return 127
    except subprocess.TimeoutExpired:
        print("t4-coach-gate: the test run timed out after 3600s", file=sys.stderr)
        return 124
    output = completed.stdout + completed.stderr
    passed = [
        name
        for name in TESTS
        if re.search(rf"(?m)^test {re.escape(name)} \.\.\. ok$", completed.stdout)
    ]
    evidence = {
        "slice": "T4",
        "gate": "coach",
        "commit": current_head(),
        "command": command,
        "filters": list(TESTS),
        "exit": completed.returncode,
        "tests_passed": passed,
        "test_file_digest": f"sha256:{digest(TEST_FILE.read_bytes())}",
        "output_digest": f"sha256:{digest(output.encode('utf-8', 'replace'))}",
        "ran_at": datetime.now(timezone.utc).isoformat(),
    }
    EVIDENCE.parent.mkdir(parents=True, exist_ok=True)
    EVIDENCE.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
    print(f"t4-coach-gate: wrote {EVIDENCE.relative_to(ROOT)} (exit {completed.returncode})")
    if completed.returncode != 0:
        print("\n".join(output.strip().splitlines()[-20:]), file=sys.stderr)
        return completed.returncode
    missing = [name for name in TESTS if name not in passed]
    if missing:
        print(
            "t4-coach-gate: the run did not exercise: " + ", ".join(missing),
            file=sys.stderr,
        )
        return 1
    return 0


def check() -> int:
    """The evidence must belong to HEAD, the current test file and the named tests."""
    if not EVIDENCE.is_file():
        print(f"t4-coach-gate: no evidence at {EVIDENCE.relative_to(ROOT)}; run --run", file=sys.stderr)
        return 1
    try:
        evidence = json.loads(read_text(EVIDENCE))
    except json.JSONDecodeError as error:
        print(f"t4-coach-gate: the evidence is not JSON: {error}", file=sys.stderr)
        return 1
    problems: list[str] = []
    head = current_head()
    if evidence.get("commit") != head:
        problems.append(f"the evidence is bound to {evidence.get('commit')!r}, not HEAD {head}")
    if evidence.get("filters") != list(TESTS):
        problems.append("the evidence's filters are not the gate's named tests")
    if evidence.get("exit") != 0:
        problems.append(f"the recorded run exited {evidence.get('exit')!r}, not 0")
    if evidence.get("tests_passed") != list(TESTS):
        problems.append("the recorded run did not pass every named test")
    expected_test_file = f"sha256:{digest(TEST_FILE.read_bytes())}"
    if evidence.get("test_file_digest") != expected_test_file:
        problems.append("the test file has changed since the recorded run")
    if not re.fullmatch(r"sha256:[0-9a-f]{64}", str(evidence.get("output_digest") or "")):
        problems.append("the evidence carries no sha256 output digest")
    missing = [name for name in TESTS if name not in defined_tests(read_text(TEST_FILE))]
    if missing:
        problems.append("the gate names undefined tests: " + ", ".join(missing))
    if problems:
        for problem in problems:
            print(f"t4-coach-gate: {problem}", file=sys.stderr)
        return 1
    print(f"t4-coach-gate: evidence at {evidence['commit']} covers all {len(TESTS)} named tests")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--self-test", action="store_true", help="check the mapping without cargo")
    mode.add_argument("--run", action="store_true", help="run the named tests and record evidence")
    mode.add_argument("--check", action="store_true", help="validate the recorded evidence")
    args = parser.parse_args(argv)
    if args.self_test:
        return self_test()
    if args.run:
        return run()
    return check()


if __name__ == "__main__":
    raise SystemExit(main())
