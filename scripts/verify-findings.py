#!/usr/bin/env python3
"""Verify the v1 conformance findings ledger against the tree it cites.

Every finding is an evidence-backed claim about the code. This gate re-reads each
`evidence[].path` line and refuses the ledger unless the quoted text is exactly that line.
A stale quote is a stale finding: the fix has landed, the line moved, or the claim was never
true. Either way it must not reach the rework register.

Usage:
    python3 scripts/verify-findings.py [docs/evidence/v1-conformance-findings.json]
    python3 scripts/verify-findings.py --stdout      # verify, print a summary, write nothing

Exit 0 = every finding verified; exit 1 = at least one defect (listed on stderr).
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
import tempfile
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT = ROOT / "docs" / "evidence" / "v1-conformance-findings.json"
SCHEMA = "conveyor-v1-findings/v1"
STANDARDS = {"MSP", "TOGAF", "SAFe", "PMP", "SPEC"}
SEVERITIES = {"P0", "P1", "P2", "P3"}
SHA = re.compile(r"^[0-9a-f]{40}$")
REQUIRED = (
    "id",
    "area",
    "standard",
    "spec",
    "severity",
    "title",
    "problem",
    "evidence",
    "failure_scenario",
    "acceptance",
    "owner_project",
)


def validate_resolution(finding: dict) -> list[str]:
    """A resolved finding is an object: an ancestor commit, a real test path and its run."""
    label = str(finding.get("id", "?"))
    value = finding.get("resolved_by")
    if not isinstance(value, dict):
        return [
            f"{label}: resolved_by must be an object with commit, test and evidence; "
            "prose is not evidence"
        ]
    errors: list[str] = []
    commit = value.get("commit")
    test = value.get("test")
    evidence = value.get("evidence")
    if not isinstance(commit, str) or not SHA.match(commit):
        errors.append(f"{label}: resolved_by.commit must be a 40-character lowercase hex commit")
    else:
        completed = subprocess.run(
            ["git", "-C", str(ROOT), "merge-base", "--is-ancestor", commit, "HEAD"],
            capture_output=True,
        )
        if completed.returncode != 0:
            errors.append(f"{label}: resolved_by.commit {commit[:8]} is not an ancestor of HEAD")
    if not isinstance(test, str) or not test.strip():
        errors.append(f"{label}: resolved_by.test must name a test path or path::name")
    else:
        path = test.split("::", 1)[0]
        if not (ROOT / path).exists():
            errors.append(f"{label}: resolved_by.test path does not exist: {path}")
    if not isinstance(evidence, str) or not evidence.strip():
        errors.append(f"{label}: resolved_by.evidence must name the run, command or result")
    return errors


def verify(path: Path) -> list[str]:
    errors: list[str] = []
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        return [f"{path}: cannot read the ledger: {error}"]
    if not isinstance(document, dict):
        return [f"{path}: the ledger must be an object"]
    if document.get("schema") != SCHEMA:
        errors.append(f"schema is {document.get('schema')!r}, expected {SCHEMA!r}")
    if not isinstance(document.get("audited_sha"), str) or not SHA.match(document["audited_sha"]):
        errors.append("audited_sha must be a 40-character lowercase hex commit")
    findings = document.get("findings")
    if not isinstance(findings, list) or not findings:
        return errors + ["findings must be a non-empty array"]

    seen: set[str] = set()
    for index, finding in enumerate(findings):
        label = f"finding[{index}]"
        if not isinstance(finding, dict):
            errors.append(f"{label} is not an object")
            continue
        missing = [key for key in REQUIRED if key not in finding]
        if missing:
            errors.append(f"{label} is missing {missing}")
            continue
        resolved = finding.get("resolved_by")
        resolution_errors = validate_resolution(finding) if resolved is not None else []
        identifier = finding["id"]
        label = str(identifier)
        if not isinstance(identifier, str) or not re.match(r"^v1-[a-z0-9-]+$", identifier):
            errors.append(f"{label}: id must look like v1-<area>-<n>")
        elif identifier in seen:
            errors.append(f"{label}: duplicate id")
        seen.add(identifier)
        if finding["standard"] not in STANDARDS:
            errors.append(f"{label}: standard {finding['standard']!r} is not one of {sorted(STANDARDS)}")
        if finding["severity"] not in SEVERITIES:
            errors.append(f"{label}: severity {finding['severity']!r} is not one of {sorted(SEVERITIES)}")
        for field in ("title", "problem", "failure_scenario", "acceptance", "owner_project", "area", "spec"):
            value = finding[field]
            if not isinstance(value, str) or not value.strip():
                errors.append(f"{label}: {field} must be a non-empty string")
        if resolved is not None:
            # A fixed finding is history: its quoted line may have moved or disappeared BECAUSE
            # the fix landed. The ledger keeps the claim and names the commit, test and run.
            errors.extend(resolution_errors)
            seen.add(identifier)
            continue
        evidence = finding["evidence"]
        if not isinstance(evidence, list) or not evidence:
            errors.append(f"{label}: evidence must be a non-empty array")
            continue
        for number, item in enumerate(evidence):
            where = f"{label} evidence[{number}]"
            if not isinstance(item, dict):
                errors.append(f"{where}: not an object")
                continue
            relative, line, quote = item.get("path"), item.get("line"), item.get("quote")
            if not isinstance(relative, str) or not relative:
                errors.append(f"{where}: path must be a non-empty string")
                continue
            if not isinstance(line, int) or line < 1:
                errors.append(f"{where}: line must be a positive integer")
                continue
            if not isinstance(quote, str) or not quote:
                errors.append(f"{where}: quote must be a non-empty string")
                continue
            target = (ROOT / relative).resolve()
            try:
                target.relative_to(ROOT)
            except ValueError:
                errors.append(f"{where}: path {relative!r} escapes the repository")
                continue
            if not target.is_file():
                errors.append(f"{where}: path {relative!r} does not exist")
                continue
            lines = target.read_text(encoding="utf-8", errors="replace").splitlines()
            if line > len(lines):
                errors.append(f"{where}: line {line} is past the end of {relative} ({len(lines)} lines)")
                continue
            actual = lines[line - 1]
            if actual != quote:
                errors.append(
                    f"{where}: quote does not match {relative}:{line}\n"
                    f"    expected: {quote!r}\n"
                    f"    actual:   {actual!r}"
                )
    return errors


def self_test() -> None:
    """The gate refuses prose, a non-ancestor commit, a missing test and empty evidence."""
    first_line = (ROOT / "scripts" / "verify-findings.py").read_text(encoding="utf-8").splitlines()[0]
    base = {
        "id": "v1-self-1",
        "area": "§1",
        "standard": "SPEC",
        "spec": "§1",
        "severity": "P2",
        "title": "self test",
        "problem": "self test",
        "evidence": [{"path": "scripts/verify-findings.py", "line": 1, "quote": first_line}],
        "failure_scenario": "self test",
        "acceptance": "self test",
        "owner_project": "PLATFORM",
        "resolved_by": {
            "commit": "92e6b223e2b6de6a005e8e8ec1e984abbbc3071b",
            "test": "scripts/verify-findings.py",
            "evidence": "self test",
        },
    }

    def check(mutate, expected: str | None) -> None:
        finding = json.loads(json.dumps(base))
        mutate(finding)
        document = {
            "schema": SCHEMA,
            "audited_sha": "92e6b223e2b6de6a005e8e8ec1e984abbbc3071b",
            "audited_at": "2026-11-21",
            "findings": [finding],
        }
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as handle:
            json.dump(document, handle)
            name = handle.name
        try:
            errors = verify(Path(name))
        finally:
            Path(name).unlink()
        if expected is None:
            if errors:
                raise SystemExit(f"self-test: the valid ledger failed: {errors}")
        elif not any(expected in error for error in errors):
            raise SystemExit(f"self-test: expected {expected!r}, got {errors}")

    def noop(finding: dict) -> None:
        pass

    check(noop, None)
    check(lambda finding: finding.update(resolved_by="fixed in a PR"), "must be an object")
    check(lambda finding: finding["resolved_by"].update(commit="0" * 40), "not an ancestor")
    check(
        lambda finding: finding["resolved_by"].update(test="scripts/does-not-exist.py"),
        "does not exist",
    )
    check(lambda finding: finding["resolved_by"].pop("evidence"), "must name the run")
    print("verify-findings: self-test OK")


def main(argv: list[str]) -> int:
    if "--self-test" in argv:
        self_test()
        return 0
    arguments = [item for item in argv if item != "--stdout"]
    if len(arguments) > 1:
        print("usage: verify-findings.py [ledger.json]", file=sys.stderr)
        return 2
    path = Path(arguments[0]) if arguments else DEFAULT
    errors = verify(path)
    if errors:
        for error in errors:
            print(f"FAIL {error}", file=sys.stderr)
        return 1
    document = json.loads(path.read_text(encoding="utf-8"))
    findings = document["findings"]
    by_standard = Counter(finding["standard"] for finding in findings)
    by_severity = Counter(finding["severity"] for finding in findings)
    resolved = sum(1 for finding in findings if finding.get("resolved_by"))
    print(
        f"verify-findings: OK — {len(findings)} findings "
        f"({', '.join(f'{k}={v}' for k, v in sorted(by_severity.items()))}; "
        f"{resolved} resolved) "
        f"across {', '.join(f'{k}={v}' for k, v in sorted(by_standard.items()))} "
        f"at {document['audited_sha'][:8]}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
