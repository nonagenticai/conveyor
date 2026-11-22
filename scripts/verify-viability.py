#!/usr/bin/env python3
"""The programme's viability verdict: one command, non-zero on any miss.

This is the gate §U7 of the #526 plan describes. It does not re-implement the
instruments that already own their truth; it composes them and adds the two
programme-level assertions no manifest can make on its own:

  A. manifests      `verify-project.py --all` exits 0 (every project's own
                    acceptance, run against the same snapshot).
  B. conformance    `audit-programme-conformance.py --strict --fail-on P0,P1`
                    exits 0 (every mandatory artefact evidenced or deferred).
  C. findings       `verify-findings.py` exits 0, and no open finding is a P0;
                    every other open finding carries a valid deferral (named
                    owner, dated acceptance, decision ref, future expiry).
  D. contracts      every active or closed project has a live Statement of
                    Architecture Work AND a live delivery contract, each with a
                    requester, an approving actor and the approval row that
                    decided it, the approver distinct from the requester.
  E. benefits       at least one benefit realisation is measured end to end
                    (baseline, post, non-empty delta, a method that is not a
                    placeholder) and the owner's KPI — token spend per accepted
                    deliverable — is recorded as an instrument, with its current
                    value printed from the inference ledger and the demo credits.

Usage:
  python3 scripts/verify-viability.py --snapshot /tmp/u7-snapshot/data
  python3 scripts/verify-viability.py --snapshot DIR --skip-commands   # D and E only
  python3 scripts/verify-viability.py --self-test

The snapshot is the product's own projection of the live rows (see
docs/evidence/rework/full-fidelity-snapshot.md): run it against the restored,
branch-migrated copy, never the deployed engine's older projection.

Exit: 0 all pass; 1 any miss; 2 usage or an unreadable input.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, NoReturn

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_SNAPSHOT = ROOT / "docs" / "design" / "data"
DEFAULT_FINDINGS = ROOT / "docs" / "evidence" / "v1-conformance-findings.json"
PLACEHOLDER = re.compile(
    r"(not measured|no instrument|unknown|tbd|to be determined|n/?a|placeholder|none)",
    re.IGNORECASE,
)
LIVE = "live"
OPEN_PROJECT_STATES = ("active", "closed")
SEVERITY_ORDER = ("P0", "P1", "P2")


def fail(message: str, code: int = 2) -> NoReturn:
    print(f"verify-viability: {message}", file=sys.stderr)
    raise SystemExit(code)


def read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        fail(f"{path} is missing")
    except json.JSONDecodeError as error:
        fail(f"{path} is not JSON: {error}")


# ── C. the code-findings ledger ──────────────────────────────────────────────


def valid_deferral(finding: dict[str, Any], today: dt.date) -> list[str]:
    """Why an open finding carries no valid deferral, or an empty list."""
    waiver = finding.get("waiver")
    if not isinstance(waiver, dict):
        return ["open with no waiver"]
    problems = []
    if not str(waiver.get("accepted_by") or "").strip():
        problems.append("waiver names no accepting owner")
    if not str(waiver.get("decision_ref") or "").strip():
        problems.append("waiver names no published decision")
    if not str(waiver.get("reason") or "").strip():
        problems.append("waiver gives no reason")
    expires = str(waiver.get("expires") or "").strip()
    try:
        if dt.date.fromisoformat(expires) <= today:
            problems.append(f"waiver expired {expires}")
    except ValueError:
        problems.append("waiver names no valid expiry")
    return problems


def check_findings(ledger: dict[str, Any], today: dt.date) -> tuple[bool, str]:
    findings = ledger.get("findings")
    if not isinstance(findings, list):
        return False, "the ledger has no findings array"
    open_rows = [f for f in findings if not f.get("resolved_by")]
    undeferred = []
    for row in open_rows:
        if row.get("severity") not in SEVERITY_ORDER:
            undeferred.append(f"{row.get('id')} (no severity)")
            continue
        problems = valid_deferral(row, today)
        if problems:
            undeferred.append(f"{row.get('id')} ({'; '.join(problems)})")
    # A P0 is acceptable only when an owner has accepted it as a dated residual;
    # an unwaived one is never deferrable (the plan's "zero unwaived P0" bar).
    p0 = [
        row
        for row in open_rows
        if row.get("severity") == "P0" and valid_deferral(row, today)
    ]
    if p0:
        ids = ", ".join(str(row.get("id")) for row in p0)
        return False, f"{len(p0)} open P0 finding(s) with no accepted residual: {ids}"
    if undeferred:
        preview = "; ".join(undeferred[:6])
        more = "" if len(undeferred) <= 6 else f"; +{len(undeferred) - 6} more"
        return False, f"{len(undeferred)} open finding(s) without a valid deferral: {preview}{more}"
    return True, f"{len(findings)} findings, {len(open_rows)} open, all deferred with a dated owner"


# ── D. every live project has its architecture spine ─────────────────────────


def check_contracts(snapshot_dir: Path) -> tuple[bool, str]:
    programme = read_json(snapshot_dir / "programme.json")
    architecture = read_json(snapshot_dir / "architecture.json")
    projects = programme.get("projects")
    contracts = architecture.get("contracts")
    if not isinstance(projects, list) or not isinstance(contracts, list):
        return False, "the snapshot has no projects or contracts array"
    by_project: dict[str, dict[str, list[dict[str, Any]]]] = {}
    for row in contracts:
        if row.get("status") != LIVE:
            continue
        by_project.setdefault(str(row.get("project_id")), {}).setdefault(
            str(row.get("kind")), []
        ).append(row)
    problems = []
    checked = 0
    for project in projects:
        if project.get("status") not in OPEN_PROJECT_STATES:
            continue
        checked += 1
        key = str(project.get("key"))
        live = by_project.get(str(project.get("id")), {})
        for kind in ("statement_of_architecture_work", "delivery_contract"):
            rows = live.get(kind) or []
            if not rows:
                problems.append(f"{key}: no live {kind}")
                continue
            row = rows[0]
            requester = row.get("requested_by")
            approver = row.get("approved_by")
            if not requester:
                problems.append(f"{key}: {kind} has no requester")
            if not approver:
                problems.append(f"{key}: {kind} has no approving actor")
            if not row.get("approval_id"):
                problems.append(f"{key}: {kind} names no approval row")
            if requester and approver and requester == approver:
                problems.append(f"{key}: {kind} requested and approved by the same actor")
    if problems:
        preview = "; ".join(problems[:8])
        more = "" if len(problems) <= 8 else f"; +{len(problems) - 8} more"
        return False, f"{len(problems)} contract gap(s) over {checked} live projects: {preview}{more}"
    return True, f"{checked} live projects each carry a live SoAW and delivery contract"


# ── E. benefits, with the owner's token-spend KPI ────────────────────────────


def measured_row(row: dict[str, Any]) -> bool:
    """A realisation measured end to end: a real baseline, a post and a delta."""
    baseline = row.get("baseline")
    post = row.get("latest_post")
    delta = row.get("delta")
    if not isinstance(baseline, dict) or baseline.get("placeholder"):
        return False
    if baseline.get("value") in (None, ""):
        return False
    if not isinstance(post, dict) or post.get("value") in (None, ""):
        return False
    if delta in (None, {}, []):
        return False
    method = str(row.get("measurement_method") or "")
    return not PLACEHOLDER.search(method)


def token_kpi(measure: dict[str, Any], msp: dict[str, Any]) -> tuple[bool, str]:
    inference = measure.get("inference")
    if not isinstance(inference, dict):
        return False, "the snapshot has no inference ledger"
    totals = inference.get("totals")
    if not isinstance(totals, dict) or not isinstance(totals.get("total_tokens"), int):
        return False, "the inference ledger carries no total token count"
    accepted = {
        str(row.get("deliverable_id"))
        for row in msp.get("demo_credits") or []
        if row.get("deliverable_id")
    }
    if not accepted:
        return False, "no accepted deliverable (demo credit) exists to divide by"
    credits = len(accepted)
    value = totals["total_tokens"] / credits
    recorded = any(
        "token" in str(profile.get("measurement_method") or "").lower()
        and "deliverable" in str(profile.get("measurement_method") or "").lower()
        for profile in msp.get("benefit_profiles") or []
    )
    detail = (
        f"{totals['total_tokens']} tokens over {credits} accepted deliverable(s) "
        f"= {value:,.0f} per deliverable"
    )
    if not recorded:
        return False, f"{detail}; no benefit profile records this KPI as its instrument"
    return True, detail


def check_benefits(snapshot_dir: Path) -> tuple[bool, str]:
    msp = read_json(snapshot_dir / "msp.json")
    measure = read_json(snapshot_dir / "measure.json")
    realisations = msp.get("benefit_realisation")
    if not isinstance(realisations, list):
        return False, "the snapshot has no benefit_realisation rows"
    measured = [row for row in realisations if measured_row(row)]
    kpi_ok, kpi_detail = token_kpi(measure, msp)
    if not measured:
        unbaselined = sum(
            1 for row in realisations if row.get("state") == "unbaselined"
        )
        return False, (
            f"no benefit is measured end to end ({unbaselined}/{len(realisations)} "
            f"unbaselined); token KPI: {kpi_detail}"
        )
    if not kpi_ok:
        return False, f"{len(measured)} measured benefit(s); {kpi_detail}"
    refs = ", ".join(str(row.get("reference")) for row in measured[:4])
    return True, f"{len(measured)} measured benefit(s) ({refs}); {kpi_detail}"


# ── A/B. the instruments that own their own truth ────────────────────────────


def run_command(command: list[str]) -> tuple[int, str]:
    completed = subprocess.run(
        command, capture_output=True, text=True, cwd=str(ROOT)
    )
    output = (completed.stdout or "") + (completed.stderr or "")
    return completed.returncode, output


def red_projects(output: str) -> list[str]:
    reds = []
    for line in output.splitlines():
        match = re.match(r"^== (\S+) ==$", line.strip())
        if match:
            current = match.group(1)
            continue
        match = re.match(r"^([A-Za-z0-9_-]+): red — (\d+) checks, (\d+) failures", line)
        if match:
            reds.append(f"{match.group(1)}({match.group(3)}/{match.group(2)})")
    return reds


def check_gap_counts(register_path: Path) -> str:
    try:
        register = read_json(register_path)
    except SystemExit:
        return "no gap register written"
    counts = register.get("counts") or {}
    parts = [f"{key} {counts.get(key, 0)}" for key in ("P0", "P1", "P2") if key in counts]
    return "gaps " + " / ".join(parts) if parts else "no counts"


def check_manifests(snapshot_dir: Path) -> tuple[bool, str]:
    code, output = run_command(
        [
            sys.executable,
            str(ROOT / "scripts" / "verify-project.py"),
            "--all",
            "--snapshot",
            str(snapshot_dir),
        ]
    )
    if code == 0:
        return True, "every project manifest is green"
    reds = red_projects(output)
    preview = ", ".join(reds[:10])
    more = "" if len(reds) <= 10 else f", +{len(reds) - 10} more"
    return False, f"{len(reds)} red manifest(s): {preview}{more}"


def check_conformance(snapshot_dir: Path) -> tuple[bool, str]:
    with tempfile.TemporaryDirectory() as workdir:
        out = Path(workdir)
        code, _ = run_command(
            [
                sys.executable,
                str(ROOT / "scripts" / "audit-programme-conformance.py"),
                "--snapshot",
                str(snapshot_dir),
                "--strict",
                "--fail-on",
                "P0,P1",
                "--out",
                str(out),
            ]
        )
        detail = check_gap_counts(out / "conformance-gap-register.json")
    if code == 0:
        return True, f"strict audit passes; {detail}"
    return False, f"strict audit fails; {detail}"


def check_findings_instrument(ledger: Path) -> tuple[bool, str]:
    code, output = run_command(
        [sys.executable, str(ROOT / "scripts" / "verify-findings.py"), "--findings", str(ledger)]
    )
    if code != 0:
        tail = " ".join(output.strip().splitlines()[-3:])
        return False, f"verify-findings exits {code}: {tail[:300]}"
    return True, "verify-findings passes"


# ── the verdict ──────────────────────────────────────────────────────────────


def render(rows: list[tuple[str, str, bool, str]]) -> bool:
    width = max(len(name) for name, _, _, _ in rows)
    for key, label, ok, detail in rows:
        print(f"{'PASS' if ok else 'FAIL'}  {key:<12} {label:<{width}}  {detail}")
    failures = [key for key, _, ok, _ in rows if not ok]
    if failures:
        print(f"viability: NOT VIABLE — {len(failures)} of {len(rows)} checks miss: {', '.join(failures)}")
        return False
    print(f"viability: VIABLE — {len(rows)} checks pass")
    return True


def self_test() -> None:
    today = dt.date(2026, 11, 22)
    valid_waiver = {
        "accepted_by": "programme owner",
        "accepted_at": "2026-11-22",
        "decision_ref": "01a20208-5261-7085-a4a7-74f061e4842b",
        "expires": "2027-02-18",
        "reason": "residual accepted",
    }

    def finding(fid: str, severity: str, waiver: dict[str, Any] | None = None) -> dict[str, Any]:
        row: dict[str, Any] = {"id": fid, "severity": severity}
        if waiver is not None:
            row["waiver"] = waiver
        return row

    def ledger(*rows: dict[str, Any]) -> dict[str, Any]:
        return {"findings": list(rows)}

    ok, _ = check_findings(ledger(finding("a", "P1", dict(valid_waiver))), today)
    assert ok, "a validly deferred P1 must pass"
    ok, _ = check_findings(ledger(finding("p0", "P0", dict(valid_waiver))), today)
    assert ok, "an owner-accepted P0 residual is a dated deferral, not an open miss"
    ok, detail = check_findings(ledger(finding("p0", "P0")), today)
    assert not ok and "P0" in detail, "an unwaived P0 always fails"
    for label, waiver in (
        ("ownerless", {**valid_waiver, "accepted_by": ""}),
        ("undecided", {**valid_waiver, "decision_ref": ""}),
        ("expired", {**valid_waiver, "expires": "2026-11-21"}),
        ("dateless", {**valid_waiver, "expires": "soon"}),
    ):
        ok, detail = check_findings(ledger(finding(label, "P1", waiver)), today)
        assert not ok, f"a {label} waiver must be refused: {detail}"
    ok, _ = check_findings(
        ledger({"id": "done", "severity": "P0", "resolved_by": {"commit": "x"}}), today
    )
    assert ok, "a resolved P0 is history, not an open miss"

    def workspace(projects: list[dict[str, Any]], contracts: list[dict[str, Any]]) -> Path:
        directory = Path(tempfile.mkdtemp(prefix="viability-self-test-"))
        (directory / "programme.json").write_text(json.dumps({"projects": projects}))
        (directory / "architecture.json").write_text(json.dumps({"contracts": contracts}))
        return directory

    def contract(kind: str, **overrides: Any) -> dict[str, Any]:
        row = {
            "kind": kind,
            "status": "live",
            "project_id": "p1",
            "requested_by": "requester",
            "approved_by": "approver",
            "approval_id": "01a2",
        }
        row.update(overrides)
        return row

    project = {"id": "p1", "key": "S99", "status": "active"}
    both = [contract("statement_of_architecture_work"), contract("delivery_contract")]
    ok, detail = check_contracts(workspace([project], both))
    assert ok, detail
    ok, detail = check_contracts(workspace([project], both[1:]))
    assert not ok and "statement_of_architecture_work" in detail, detail
    ok, detail = check_contracts(
        workspace([project], [both[0], contract("delivery_contract", approved_by="requester")])
    )
    assert not ok and "same actor" in detail, detail
    ok, detail = check_contracts(
        workspace([project], [both[0], contract("delivery_contract", requested_by=None)])
    )
    assert not ok and "no requester" in detail, detail
    ok, detail = check_contracts(
        workspace([{**project, "status": "archived"}], [])
    )
    assert ok, "an archived project needs no contract"
    ok, detail = check_contracts(
        workspace([project], [{**both[0], "status": "superseded"}, both[1]])
    )
    assert not ok and "statement_of_architecture_work" in detail, "only a live SoAW counts"

    measure = {"inference": {"totals": {"total_tokens": 1000}}}
    msp_measured = {
        "demo_credits": [{"deliverable_id": "d1"}, {"deliverable_id": "d1"}],
        "benefit_realisation": [
            {
                "reference": "B1",
                "state": "measured",
                "baseline": {"value": 10, "placeholder": False},
                "latest_post": {"value": 20},
                "delta": {"value": 10},
                "measurement_method": "count rows",
            }
        ],
        "benefit_profiles": [
            {"measurement_method": "total tokens from the inference ledger per accepted deliverable"}
        ],
    }
    directory = Path(tempfile.mkdtemp(prefix="viability-self-test-"))
    (directory / "msp.json").write_text(json.dumps(msp_measured))
    (directory / "measure.json").write_text(json.dumps(measure))
    ok, detail = check_benefits(directory)
    assert ok, detail
    msp_unmeasured = {
        **msp_measured,
        "benefit_realisation": [
            {
                "reference": "B1",
                "state": "unbaselined",
                "baseline": {"value": None, "placeholder": True},
                "latest_post": None,
                "delta": None,
                "measurement_method": "not measured",
            }
        ],
    }
    (directory / "msp.json").write_text(json.dumps(msp_unmeasured))
    ok, detail = check_benefits(directory)
    assert not ok and "unbaselined" in detail, detail
    (directory / "msp.json").write_text(json.dumps({**msp_measured, "benefit_profiles": []}))
    ok, detail = check_benefits(directory)
    assert not ok and "KPI" in detail, detail

    rows = [("a", "x", True, ""), ("b", "y", False, "")]
    assert not render(rows)
    assert render(rows[:1])
    print("verify-viability: self-test OK")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--snapshot", metavar="DIR", default=str(DEFAULT_SNAPSHOT))
    parser.add_argument("--findings", metavar="PATH", default=str(DEFAULT_FINDINGS))
    parser.add_argument(
        "--skip-commands",
        action="store_true",
        help="run only the pure-data checks (D, E); skip A, B, C",
    )
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)

    if args.self_test:
        self_test()
        return 0

    snapshot = Path(args.snapshot)
    if not snapshot.is_dir():
        fail(f"the snapshot directory does not exist: {snapshot}")
    ledger_path = Path(args.findings)
    ledger = read_json(ledger_path)
    today = dt.date.today()

    rows: list[tuple[str, str, bool, str]] = []
    if not args.skip_commands:
        rows.append(("manifests", "every project manifest", *check_manifests(snapshot)))
        rows.append(("conformance", "strict audit", *check_conformance(snapshot)))
        rows.append(
            (
                "findings",
                "ledger instrument",
                *check_findings_instrument(ledger_path),
            )
        )
    ok, detail = check_findings(ledger, today)
    rows.append(("deferrals", "no open P0, every open finding deferred", ok, detail))
    rows.append(("contracts", "every live project's SoAW and contract", *check_contracts(snapshot)))
    rows.append(("benefits", "one measured benefit and the token KPI", *check_benefits(snapshot)))

    return 0 if render(rows) else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
