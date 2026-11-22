#!/usr/bin/env python3
"""Check the three programme registers against the snapshot and the sources.

The registers are mechanisms, not documents (SPEC_V2 §8.3–§8.4): a risk is a row with
probability, impact, an owner and a review date; an ADR is a governed asset whose content
hash matches its source file and whose supersession graph is acyclic; a benefit has the six
field groups, a baseline measured before its transition, a post-transition measurement, and
a five-level map. This checker reads the *snapshot* (never the live database) and the
repository sources, and returns findings — it never writes a row.

Usage:
    python3 scripts/check-registers.py --snapshot docs/design/data
    python3 scripts/check-registers.py --snapshot /tmp/snap/data --out /tmp/registers.json
    python3 scripts/check-registers.py --check          # exit 1 on any P0/P1 finding
    python3 scripts/check-registers.py --self-test

Verdicts are honest about what the projection cannot show: the governed-asset projection
currently omits `meta`, so a content hash or decider it cannot see is reported as
`adr_meta_not_projected` (a P1), never as a pass.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import tempfile
from datetime import date, datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SNAPSHOT = ROOT / "docs" / "design" / "data"

ADR_FILE = re.compile(r"^(?P<number>\d{4})-(?P<slug>.+)\.md$")
ADR_KEY = re.compile(r"^adr-(?P<number>\d{4})$")
OPEN_RISK_STATES = {"open", "assessed", "mitigating", "accepted"}
ADR_STATES = {"proposed", "accepted", "approved", "superseded", "rejected", "deprecated"}

SEVERITY_ORDER = {"P0": 0, "P1": 1, "P2": 2}


def finding(check: str, severity: str, subject: str, detail: str) -> dict[str, str]:
    return {"check": check, "severity": severity, "subject": subject, "detail": detail}


def non_empty(value: Any) -> bool:
    if value is None:
        return False
    if isinstance(value, str):
        return value.strip() != ""
    if isinstance(value, (list, tuple, dict, set)):
        return len(value) > 0
    return True


def as_date(value: Any) -> date | None:
    if not isinstance(value, str) or not value.strip():
        return None
    try:
        return date.fromisoformat(value.strip()[:10])
    except ValueError:
        return None


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


# ── risk register ───────────────────────────────────────────────────────────────


def check_risks(items: list[dict[str, Any]], today: date) -> list[dict[str, str]]:
    out: list[dict[str, str]] = []
    risks = [row for row in items if str(row.get("kind")) == "risk"]
    by_id = {str(row.get("id")): row for row in risks}
    for row in risks:
        if str(row.get("status")) not in OPEN_RISK_STATES:
            continue
        subject = str(row.get("code") or row.get("title") or row.get("id"))
        missing = [
            name
            for name in ("probability", "impact", "owner_job_id", "review_at")
            if not non_empty(row.get(name))
        ]
        if not non_empty(row.get("links")):
            missing.append("links")
        if missing:
            out.append(
                finding(
                    "risk_incomplete",
                    "P1",
                    subject,
                    f"open risk missing {', '.join(missing)}",
                )
            )
        reviewed = as_date(row.get("review_at"))
        if reviewed is not None and reviewed < today:
            out.append(
                finding(
                    "risk_review_overdue",
                    "P1",
                    subject,
                    f"review_at {reviewed.isoformat()} is before {today.isoformat()}",
                )
            )
        if str(row.get("status")) == "accepted" and not non_empty(row.get("response")):
            out.append(
                finding(
                    "risk_acceptance_unjustified",
                    "P1",
                    subject,
                    "an accepted risk carries no response/rationale",
                )
            )
        if non_empty(row.get("programme_id")):
            escalations = [
                link
                for link in (row.get("links") or [])
                if isinstance(link, dict) and link.get("kind") == "escalated_from"
            ]
            if not escalations:
                out.append(
                    finding(
                        "risk_escalation_unlinked",
                        "P1",
                        subject,
                        "a programme risk has no escalated_from link to a project risk",
                    )
                )
                continue
            for link in escalations:
                target = by_id.get(str(link.get("item_id")))
                if target is None:
                    out.append(
                        finding(
                            "risk_escalation_unresolved",
                            "P1",
                            subject,
                            f"escalated_from item {link.get('item_id')} is not a risk row",
                        )
                    )
                elif str(target.get("status")) not in OPEN_RISK_STATES:
                    out.append(
                        finding(
                            "risk_escalation_closed",
                            "P1",
                            subject,
                            f"escalated_from risk {link.get('item_id')} is closed",
                        )
                    )
    return out


# ── ADR register ────────────────────────────────────────────────────────────────


def check_adrs(
    rows: list[dict[str, Any]], decisions_dir: Path, contracts: list[dict[str, Any]]
) -> list[dict[str, str]]:
    out: list[dict[str, str]] = []
    files: dict[str, Path] = {}
    for path in sorted(decisions_dir.glob("*.md")):
        match = ADR_FILE.match(path.name)
        if not match:
            continue
        number = match.group("number")
        if number in files:
            out.append(
                finding(
                    "adr_duplicate_file",
                    "P0",
                    f"adr-{number}",
                    f"two source files claim ADR {number}: {files[number].name} and {path.name}",
                )
            )
        files[number] = path

    adr_rows = [row for row in rows if str(row.get("kind")) == "adr"]
    row_by_number: dict[str, list[dict[str, Any]]] = {}
    for row in adr_rows:
        match = ADR_KEY.match(str(row.get("key") or ""))
        if not match:
            out.append(
                finding(
                    "adr_bad_key",
                    "P1",
                    str(row.get("key") or row.get("id")),
                    "an ADR row's key is not adr-NNNN",
                )
            )
            continue
        row_by_number.setdefault(match.group("number"), []).append(row)
    for number, entries in row_by_number.items():
        if len(entries) > 1:
            out.append(
                finding(
                    "adr_duplicate_row",
                    "P0",
                    f"adr-{number}",
                    f"{len(entries)} governed-asset rows claim ADR {number}",
                )
            )

    for number, path in files.items():
        if number not in row_by_number:
            out.append(
                finding(
                    "adr_missing_row",
                    "P1",
                    f"adr-{number}",
                    f"{path.name} has no governed_asset(kind='adr') row",
                )
            )
    for number in row_by_number:
        if number not in files:
            out.append(
                finding(
                    "adr_orphan_row",
                    "P1",
                    f"adr-{number}",
                    "an ADR row has no source file under docs/decisions",
                )
            )

    projected_meta = any(isinstance(row.get("meta"), dict) for row in adr_rows)
    if adr_rows and not projected_meta:
        out.append(
            finding(
                "adr_meta_not_projected",
                "P1",
                "snapshot.governed_assets",
                "the governed-asset projection omits meta, so content hash, decider and "
                "decided_at cannot be assessed (crates/store/src/snapshot.rs:338)",
            )
        )
    if projected_meta:
        for number, path in files.items():
            for row in row_by_number.get(number, []):
                meta = row.get("meta") or {}
                expected = hashlib.sha256(path.read_bytes()).hexdigest()
                if meta.get("content_sha256") != expected:
                    out.append(
                        finding(
                            "adr_content_mismatch",
                            "P1",
                            f"adr-{number}",
                            "meta.content_sha256 does not match the source file",
                        )
                    )
                status = str(row.get("status") or "")
                if status not in ADR_STATES:
                    out.append(
                        finding(
                            "adr_status_invalid",
                            "P1",
                            f"adr-{number}",
                            f"status {status!r} is not a register status",
                        )
                    )
                if status in {"accepted", "approved"}:
                    if not non_empty(meta.get("decided_at")) or not non_empty(
                        meta.get("deciders")
                    ):
                        out.append(
                            finding(
                                "adr_decider_missing",
                                "P1",
                                f"adr-{number}",
                                "an accepted ADR records no decider or decided_at",
                            )
                        )

    # Supersession graph: every edge resolves, and the graph is acyclic.
    id_to_number = {
        str(row.get("id")): number
        for number, entries in row_by_number.items()
        for row in entries
    }
    edges: dict[str, str] = {}
    for number, entries in row_by_number.items():
        for row in entries:
            for field, reverse in (("supersedes_id", False), ("superseded_by_id", True)):
                target_id = row.get(field)
                if not non_empty(target_id):
                    continue
                if str(target_id) not in id_to_number:
                    out.append(
                        finding(
                            "adr_supersession_dangling",
                            "P1",
                            f"adr-{number}",
                            f"{field} {target_id} is not an ADR row",
                        )
                    )
                    continue
                source, target = (
                    (number, id_to_number[str(target_id)])
                    if not reverse
                    else (id_to_number[str(target_id)], number)
                )
                edges[source] = target
    state: dict[str, int] = {}

    def visit(node: str) -> bool:
        state[node] = 1
        nxt = edges.get(node)
        if nxt is not None:
            if state.get(nxt) == 1:
                return True
            if state.get(nxt, 0) == 0 and visit(nxt):
                return True
        state[node] = 2
        return False

    for node in list(edges):
        if state.get(node, 0) == 0 and visit(node):
            out.append(
                finding(
                    "adr_supersession_cycle",
                    "P0",
                    f"adr-{node}",
                    "the supersession graph contains a cycle",
                )
            )
            break

    # Contracts must cite the ADRs they rest on — or the projection must say it cannot see them.
    if contracts and not any("links" in contract or "meta" in contract for contract in contracts):
        out.append(
            finding(
                "contract_citations_not_projected",
                "P2",
                "snapshot.contracts",
                "the contract projection omits links/meta, so ADR citations cannot be assessed",
            )
        )
    return out


# ── benefits register ───────────────────────────────────────────────────────────


def profile_groups(profile: dict[str, Any]) -> dict[str, bool]:
    baseline = profile.get("baseline")
    placeholder = isinstance(baseline, str) and baseline.strip().lower().startswith(
        "not measured"
    )
    return {
        "identification": all(
            non_empty(profile.get(field))
            for field in ("reference", "description", "category", "objectives",
                          "observable_outcome")
        ),
        "performance": (
            not placeholder
            and all(
                non_empty(profile.get(field))
                for field in ("kpis", "baseline", "target", "measurement_method")
            )
        ),
        "cost": non_empty(profile.get("cost")),
        "conditions": all(
            non_empty(profile.get(field)) for field in ("capabilities", "preconditions")
        ),
        "uncertainty": non_empty(profile.get("risks")),
        "responsibility": all(
            non_empty(profile.get(field))
            for field in ("owner_job_id", "benefit_owner_job_id", "operational_area")
        ),
    }


def check_benefits(
    profiles: list[dict[str, Any]],
    links: list[dict[str, Any]],
    measurements: list[dict[str, Any]],
    transitions: list[dict[str, Any]],
    realisation: list[dict[str, Any]],
) -> list[dict[str, str]]:
    out: list[dict[str, str]] = []
    links_by_benefit: dict[str, list[dict[str, Any]]] = {}
    for link in links:
        links_by_benefit.setdefault(str(link.get("benefit_id")), []).append(link)
    measurements_by_benefit: dict[str, list[dict[str, Any]]] = {}
    for row in measurements:
        measurements_by_benefit.setdefault(str(row.get("benefit_id")), []).append(row)
    transitions_by_benefit: dict[str, list[dict[str, Any]]] = {}
    for row in transitions:
        transitions_by_benefit.setdefault(str(row.get("benefit_id")), []).append(row)
    realisation_by_reference = {str(row.get("reference")): row for row in realisation}

    for profile in profiles:
        reference = str(profile.get("reference") or profile.get("id"))
        benefit_id = str(profile.get("id"))
        subject = reference
        groups = profile_groups(profile)
        missing = [name for name, ok in groups.items() if not ok]
        if missing:
            out.append(
                finding(
                    "benefit_group_incomplete",
                    "P1",
                    subject,
                    f"missing field group(s): {', '.join(missing)}",
                )
            )
        baseline = profile.get("baseline")
        if isinstance(baseline, str) and baseline.strip().lower().startswith("not measured"):
            out.append(
                finding(
                    "benefit_baseline_placeholder",
                    "P1",
                    subject,
                    "the baseline is a placeholder, not a measurement",
                )
            )

        mapped = links_by_benefit.get(benefit_id, [])
        complete = [
            link
            for link in mapped
            if all(
                non_empty(link.get(field))
                for field in (
                    "output_ref",
                    "capability_ref",
                    "operational_outcome",
                    "benefit_id",
                    "strategic_objective",
                )
            )
        ]
        if not complete:
            out.append(
                finding(
                    "benefit_map_incomplete",
                    "P1",
                    subject,
                    "no benefit_link completes output → capability → outcome → benefit → objective",
                )
            )

        rows = measurements_by_benefit.get(benefit_id, [])
        baseline_rows = [row for row in rows if str(row.get("kind")) == "baseline"]
        post_rows = [row for row in rows if str(row.get("kind")) == "post"]
        if not isinstance(baseline, str) or not baseline.strip().lower().startswith(
            "not measured"
        ):
            if not baseline_rows:
                out.append(
                    finding(
                        "benefit_baseline_unmeasured",
                        "P1",
                        subject,
                        "a stated baseline has no baseline measurement row",
                    )
                )
        for transition in transitions_by_benefit.get(benefit_id, []):
            enacted = as_date(transition.get("enacted_at"))
            if enacted is None:
                continue
            posts = [row for row in post_rows if as_date(row.get("measured_at")) is not None]
            if not posts:
                out.append(
                    finding(
                        "benefit_post_missing",
                        "P1",
                        subject,
                        "the transition was enacted and no post measurement exists",
                    )
                )
            elif all(as_date(row.get("measured_at")) < enacted for row in posts):
                out.append(
                    finding(
                        "benefit_post_before_enact",
                        "P1",
                        subject,
                        "every post measurement predates the transition's enacted_at",
                    )
                )

        state = str((realisation_by_reference.get(reference) or {}).get("state") or "")
        if state == "realised" and not post_rows:
            out.append(
                finding(
                    "benefit_realised_without_post",
                    "P0",
                    subject,
                    "the benefit is recorded realised with no post-transition measurement",
                )
            )

    if profiles and not any(
        "token" in json.dumps(profile).lower()
        and "deliverable" in json.dumps(profile).lower()
        for profile in profiles
    ):
        out.append(
            finding(
                "benefit_kpi_unrecorded",
                "P1",
                "B7",
                "no benefit profile records tokens per accepted deliverable",
            )
        )
    return out


# ── driver ─────────────────────────────────────────────────────────────────────


def run_checks(snapshot: Path, repo_root: Path, today: date | None = None) -> dict[str, Any]:
    today = today or datetime.now(timezone.utc).date()
    msp = load_json(snapshot / "msp.json")
    artefacts = load_json(snapshot / "artefacts.json")
    architecture = load_json(snapshot / "architecture.json")
    findings: list[dict[str, str]] = []
    findings += check_risks(msp.get("register_items") or [], today)
    findings += check_adrs(
        artefacts.get("governed_assets") or [],
        repo_root / "docs" / "decisions",
        architecture.get("contracts") or [],
    )
    findings += check_benefits(
        msp.get("benefit_profiles") or [],
        msp.get("benefit_links") or [],
        msp.get("benefit_measurements") or [],
        msp.get("benefit_transitions") or [],
        msp.get("benefit_realisation") or [],
    )
    findings.sort(key=lambda item: (SEVERITY_ORDER[item["severity"]], item["check"], item["subject"]))
    counts = {
        "findings": len(findings),
        "P0": sum(1 for item in findings if item["severity"] == "P0"),
        "P1": sum(1 for item in findings if item["severity"] == "P1"),
        "P2": sum(1 for item in findings if item["severity"] == "P2"),
        "risks": len([row for row in (msp.get("register_items") or []) if row.get("kind") == "risk"]),
        "adr_rows": len(
            [row for row in (artefacts.get("governed_assets") or []) if row.get("kind") == "adr"]
        ),
        "benefit_profiles": len(msp.get("benefit_profiles") or []),
        "benefit_measurements": len(msp.get("benefit_measurements") or []),
    }
    return {
        "schema": "check-registers/v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "snapshot": str(snapshot),
        "summary": counts,
        "findings": findings,
    }


def human(report: dict[str, Any]) -> str:
    counts = report["summary"]
    lines = [
        f"check-registers: {counts['findings']} finding(s) — P0 {counts['P0']} / "
        f"P1 {counts['P1']} / P2 {counts['P2']}",
        f"  risks {counts['risks']} · adr rows {counts['adr_rows']} · "
        f"benefits {counts['benefit_profiles']} · measurements {counts['benefit_measurements']}",
    ]
    for item in report["findings"][:40]:
        lines.append(
            f"  {item['severity']}  {item['check']}  {item['subject']}: {item['detail']}"
        )
    if len(report["findings"]) > 40:
        lines.append(f"  … {len(report['findings']) - 40} more")
    return "\n".join(lines)


# ── self-test ──────────────────────────────────────────────────────────────────


def self_test() -> int:
    passed = 0

    def expect(condition: bool, label: str) -> None:
        nonlocal passed
        if not condition:
            raise SystemExit(f"self-test: FAIL — {label}")
        passed += 1

    def checks(report: dict[str, Any]) -> set[str]:
        return {item["check"] for item in report["findings"]}

    today = date(2026, 11, 22)
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        snapshot = root / "snap"
        snapshot.mkdir()
        (root / "docs" / "decisions").mkdir(parents=True)
        (root / "docs" / "decisions" / "0001-first.md").write_text("# ADR 1\n", encoding="utf-8")
        digest = hashlib.sha256((root / "docs" / "decisions" / "0001-first.md").read_bytes()).hexdigest()

        good_risk = {
            "id": "r1", "kind": "risk", "code": "R1", "status": "open",
            "probability": "high", "impact": 4, "owner_job_id": "j1",
            "review_at": "2027-01-01", "links": ["https://example/1"], "response": None,
            "programme_id": None, "project_id": "p1",
        }
        good_profile = {
            "id": "b1", "reference": "B1", "description": "Auditable decisions",
            "category": "strategic", "objectives": ["o"], "observable_outcome": "out",
            "kpis": ["k"], "baseline": "10 decisions", "target": "every choice",
            "measurement_method": "count", "cost": {"amount": 1}, "capabilities": ["c"],
            "preconditions": ["p"], "risks": ["r"], "owner_job_id": "j1",
            "benefit_owner_job_id": "j2", "operational_area": "ops",
        }
        good_link = {
            "benefit_id": "b1", "output_ref": "o", "capability_ref": "c",
            "operational_outcome": "out", "strategic_objective": "s",
        }
        good_measure = {"benefit_id": "b1", "kind": "baseline", "actual_value": "10",
                        "measured_at": "2026-11-01", "measurement_method": "count"}
        good_kpi = {
            **good_profile, "id": "b7", "reference": "B7",
            "description": "Tokens per accepted deliverable",
            "objectives": ["reduce token spend per accepted deliverable"],
            "observable_outcome": "token spend per accepted deliverable falls",
            "baseline": "788206 tokens per deliverable",
            "target": "20% below the baseline",
            "measurement_method": "inference ledger over accepted deliverables",
        }
        good_kpi_link = {**good_link, "benefit_id": "b7"}
        good_kpi_measure = {**good_measure, "benefit_id": "b7"}
        adr_row = {
            "id": "a1", "kind": "adr", "key": "adr-0001", "status": "approved",
            "meta": {"content_sha256": digest, "decided_at": "2026-11-01", "deciders": ["x"]},
            "supersedes_id": None, "superseded_by_id": None,
        }

        def write(risks=None, profiles=None, links=None, measures=None,
                  transitions=None, realisation=None, adrs=None, contracts=None):
            (snapshot / "msp.json").write_text(json.dumps({
                "register_items": risks or [],
                "benefit_profiles": profiles or [],
                "benefit_links": links or [],
                "benefit_measurements": measures or [],
                "benefit_transitions": transitions or [],
                "benefit_realisation": realisation or [],
            }), encoding="utf-8")
            (snapshot / "artefacts.json").write_text(json.dumps({
                "governed_assets": adrs if adrs is not None else [adr_row],
            }), encoding="utf-8")
            (snapshot / "architecture.json").write_text(json.dumps({
                "contracts": contracts or [],
            }), encoding="utf-8")

        write(risks=[good_risk], profiles=[good_profile, good_kpi],
              links=[good_link, good_kpi_link],
              measures=[good_measure, good_kpi_measure],
              realisation=[{"reference": "B1", "state": "on_track"},
                           {"reference": "B7", "state": "on_track"}])
        report = run_checks(snapshot, root, today)
        expect(not checks(report), "the good fixture passes")

        write(risks=[{**good_risk, "owner_job_id": None}])
        report = run_checks(snapshot, root, today)
        expect("risk_incomplete" in checks(report), "a risk without an owner is refused")

        write(risks=[{**good_risk, "review_at": "2026-01-01"}])
        report = run_checks(snapshot, root, today)
        expect("risk_review_overdue" in checks(report), "an overdue review is refused")

        write(risks=[{**good_risk, "programme_id": "p", "links": ["x"]}])
        report = run_checks(snapshot, root, today)
        expect("risk_escalation_unlinked" in checks(report), "a programme risk needs its escalation")

        write(adrs=[])
        report = run_checks(snapshot, root, today)
        expect("adr_missing_row" in checks(report), "a source ADR without a row is refused")

        write(adrs=[{**adr_row, "meta": {**adr_row["meta"], "content_sha256": "0" * 64}}])
        report = run_checks(snapshot, root, today)
        expect("adr_content_mismatch" in checks(report), "a content-hash mismatch is refused")

        write(adrs=[{**adr_row, "supersedes_id": "a1"}])
        report = run_checks(snapshot, root, today)
        expect("adr_supersession_cycle" in checks(report), "a supersession cycle is refused")

        write(profiles=[{**good_profile, "target": ""}], links=[good_link],
              measures=[good_measure], realisation=[{"reference": "B1", "state": "on_track"}])
        report = run_checks(snapshot, root, today)
        expect("benefit_group_incomplete" in checks(report), "a benefit without a target is refused")

        write(profiles=[{**good_profile, "baseline": "not measured - placeholder"}],
              links=[good_link], measures=[good_measure],
              realisation=[{"reference": "B1", "state": "unbaselined"}])
        report = run_checks(snapshot, root, today)
        expect("benefit_baseline_placeholder" in checks(report), "a placeholder baseline is refused")

        write(profiles=[good_profile], links=[good_link],
              measures=[{"benefit_id": "b1", "kind": "post", "actual_value": "1",
                         "measured_at": "2026-10-01"}],
              transitions=[{"benefit_id": "b1", "enacted_at": "2026-11-01"}],
              realisation=[{"reference": "B1", "state": "realised"}])
        report = run_checks(snapshot, root, today)
        expect("benefit_post_before_enact" in checks(report), "a post before enact is refused")
        expect("benefit_realised_without_post" not in checks(report),
               "a post row exists, so realised is not the no-post finding")

        write(profiles=[good_profile], links=[good_link], measures=[good_measure],
              realisation=[{"reference": "B1", "state": "realised"}])
        report = run_checks(snapshot, root, today)
        expect("benefit_realised_without_post" in checks(report),
               "a realised benefit with no post row is refused")

    print(f"check-registers: self-test OK ({passed} checks)")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--snapshot", default=str(DEFAULT_SNAPSHOT))
    parser.add_argument("--repo-root", default=str(ROOT))
    parser.add_argument("--out")
    parser.add_argument("--check", action="store_true", help="exit 1 on any P0/P1 finding")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    snapshot = Path(args.snapshot)
    if not snapshot.exists():
        print(f"check-registers: snapshot {snapshot} does not exist", file=sys.stderr)
        return 2
    report = run_checks(snapshot, Path(args.repo_root))
    if args.out:
        Path(args.out).write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(human(report), file=sys.stderr)
    print(json.dumps(report, indent=2))
    if args.check and (report["summary"]["P0"] or report["summary"]["P1"]):
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
