#!/usr/bin/env python3
"""Build and verify `docs/evidence/v2-gap-ledger.json` against its three sources.

The ledger unifies: absent artefacts from the generated artefact record, stale
`conformance-baseline.json` entries with no live gap, and the open v1 code findings.

    python3 scripts/check-v2-gap-ledger.py --build   # one-command refresh
    python3 scripts/check-v2-gap-ledger.py           # verify the committed ledger
    python3 scripts/check-v2-gap-ledger.py --self-test

The check proves: row counts and key sets equal the sources, every row has an owner
(possibly `unassigned`) and a fix card, no key is duplicated, and every open v1 finding id
appears exactly once as a `ledger:` row.

Defaults are the lane's retained inputs; pass the flags for another run.
Exit 0 = all proofs hold; exit 1 = at least one proof failed.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import re
import subprocess
import sys
import tempfile
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_LEDGER = ROOT / "docs/evidence/v2-gap-ledger.json"
DEFAULT_FINDINGS = ROOT / "docs/evidence/v1-conformance-findings.json"
DEFAULT_BASELINE = ROOT / "docs/evidence/conformance-baseline.json"
DEFAULT_RULES = ROOT / "governance/conformance.json"
DEFAULT_PROGRAMME = ROOT / "seed/programme.json"
DEFAULT_SNAPSHOT = Path("/tmp/snap-526-0738/data")
DEFAULT_OUTPUTS = Path("/tmp/snap-526-0738/docs")
DEFAULT_REGISTER = Path("/tmp/conf-0738/rework.json")
DEFAULT_GAPS = Path("/tmp/conf-0738/conformance-gap-register.json")
KINDS = ("artefact", "baseline", "ledger")
STATUS = {"artefact": "absent", "baseline": "stale", "ledger": "open"}
DEPLOY = re.compile(
    r"\b(authoring\.intake|authoring\.draft|authoring\.review|authoring\.publish|"
    r"authoring\.assets|00(?:62|63|64|65|66))\b"
)
# Severity is a rank, not a label: a lower number outranks a higher one. The standard's
# generated severity is authoritative; a fix package may raise it, never lower it. Before
# 2026-11-22 the package won outright, which carried 18 of the 21 standards P0s into the
# allocation board as P2.
SEVERITY_RANK = {"P0": 0, "P1": 1, "P2": 2}
UNRESOLVED = "unresolved"
RESOLUTIONS = {
    "published_package", "covered_package", "artefact_package", "finding_id",
    "declared_package_absent", "baseline_key", UNRESOLVED,
}
PACKAGE_RESOLUTIONS = {"published_package", "covered_package", "artefact_package"}


def severity_for(package: dict | None, finding: dict) -> str:
    """The more severe of the generated finding and its fix package."""
    candidates = []
    if finding:
        candidates.append(str(finding.get("severity") or ""))
    if package:
        candidates.append(str(package.get("severity") or ""))
    candidates = [value for value in candidates if value in SEVERITY_RANK]
    if not candidates:
        return "unassigned"
    return min(candidates, key=SEVERITY_RANK.__getitem__)


def read(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def tree_digest(directory: Path) -> str:
    checksum = hashlib.sha256()
    for path in sorted(directory.iterdir()):
        if path.is_file():
            checksum.update(path.name.encode())
            checksum.update(b"\0")
            checksum.update(digest(path).encode())
            checksum.update(b"\n")
    return checksum.hexdigest()


def lane_pin() -> str:
    return subprocess.run(
        ["git", "-C", str(ROOT), "rev-parse", "--short=8", "HEAD"],
        check=True, capture_output=True, text=True,
    ).stdout.strip()


def regenerate(args, out: Path) -> dict:
    """Rebuild the artefact record and findings with the unchanged generator."""
    out.mkdir(parents=True, exist_ok=True)
    command = [
        sys.executable,
        str(ROOT / "scripts/generate-standards.py"),
        "--snapshot", str(args.snapshot),
        "--outputs-dir", str(args.outputs),
        "--register", str(args.register),
        "--data-out", str(out),
        "--findings-out", str(out / "findings.json"),
    ]
    subprocess.run(command, check=True, capture_output=True, text=True)
    return {
        "standards": read(out / "standards.json"),
        "findings": read(out / "findings.json")["findings"],
        "register": read(args.register),
        "baseline": read(args.baseline),
        "gaps": read(args.gaps)["gaps"],
        "v1": read(args.findings)["findings"],
        "rules": read(DEFAULT_RULES)["rules"],
        "phase": {
            project["key"]: str((project.get("meta") or {}).get("adm_phase") or "")
            for project in read(DEFAULT_PROGRAMME)["projects"]
        },
        # `generate-standards.py` emits no finding for an archived project, so its absent
        # artefacts are not gaps. The ledger must use the same predicate or it invents work.
        "archived": {
            str(project["key"])
            for project in read(DEFAULT_PROGRAMME)["projects"]
            if str((project.get("meta") or {}).get("measured_status") or "") == "archived"
        },
        # The exact inputs this run read, so `check` can refuse a ledger built from others.
        # Without this, any snapshot with the same key set "verifies" any ledger.
        "inputs": [
            {"path": str(args.snapshot), "sha256": tree_digest(args.snapshot),
             "role": "post-close snapshot; absent set"},
            {"path": str(args.outputs), "sha256": tree_digest(args.outputs),
             "role": "published pages the record resolves document headings from"},
            {"path": str(args.register), "sha256": digest(args.register),
             "role": "rework register; fix cards and owners"},
            {"path": str(args.gaps), "sha256": digest(args.gaps),
             "role": "live gaps; baseline liveness"},
            {"path": str(args.baseline), "sha256": digest(args.baseline),
             "role": "baseline entries; stale set"},
            {"path": str(args.findings), "sha256": digest(args.findings),
             "role": "v1 findings; open ledger"},
            {"path": str(DEFAULT_RULES), "sha256": digest(DEFAULT_RULES),
             "role": "rule standard and spec refs for baseline rows"},
            {"path": str(DEFAULT_PROGRAMME), "sha256": digest(DEFAULT_PROGRAMME),
             "role": "project ADM phase and archived set"},
        ],
        "command": " ".join(command),
    }


def expected(source: dict) -> dict:
    gap_keys = {(gap["rule"], gap.get("subject")) for gap in source["gaps"]}
    archived = source.get("archived") or set()
    return {
        # The standard holds an artefact to account only when it is MANDATORY and its project
        # is not archived — the same two filters `build_findings` applies before it emits a
        # finding. An on-demand artefact with no trigger is not a gap (SPEC_V2 §8 / plan v6
        # W3.1); charging it produced 65 phantom gaps and 88 rows with no severity.
        "absent": {
            artefact["id"]: artefact
            for artefact in source["standards"]["artefacts"]
            if artefact["status"] == "absent"
            and artefact.get("mandatory")
            and str(artefact["subject"].get("project_key") or "") not in archived
        },
        "stale": {
            (entry["rule"], entry["subject"]): entry
            for entry in source["baseline"]["entries"]
            if (entry["rule"], entry["subject"]) not in gap_keys
        },
        "open": {f["id"]: f for f in source["v1"] if not f.get("resolved_by")},
        "register": {package["key"]: package for package in source["register"]["packages"]},
        "register_keys": {package["key"] for package in source["register"]["packages"]},
    }


# ── build ───────────────────────────────────────────────────────────────────────────


def build(source: dict, args, pin: str) -> dict:
    exp = expected(source)
    packages = source["register"]["packages"]
    by_key = {package["key"]: package for package in packages}
    by_rule: dict[str, dict] = {}
    for package in packages:
        by_rule.setdefault(str(package["rule"]), package)
    generated = {finding["id"]: finding for finding in source["findings"]}
    phase_of = source["phase"]
    resolution: Counter = Counter()
    # Artefact records that declare a package key the register does not contain. The row's
    # finding still gives it a real fix card, so the work is allocatable, but the declaration
    # itself is a defect in the artefact record and must stay visible.
    declared_absent: list[str] = []

    def phase_for_project(project: str) -> str:
        return phase_of.get(project) or "unassigned"

    rows: list[dict] = []
    for artefact in exp["absent"].values():
        finding = generated.get(artefact["id"]) or {}
        covered = str(artefact.get("covered_key") or "")
        declared = str(artefact.get("package_key") or "")
        if declared and declared not in by_key:
            declared_absent.append(declared)
        if artefact.get("package_published") and declared in by_key:
            card, package, card_resolution = declared, by_key[declared], "published_package"
        elif covered in by_key:
            card, package, card_resolution = covered, by_key[covered], "covered_package"
        elif artefact["id"] in by_rule:
            package = by_rule[artefact["id"]]
            card, card_resolution = str(package["key"]), "artefact_package"
        elif finding.get("id"):
            card, package, card_resolution = str(finding["id"]), None, "finding_id"
        elif declared:
            card, package, card_resolution = declared, None, "declared_package_absent"
        else:
            card, package, card_resolution = "", None, UNRESOLVED
        resolution[card_resolution] += 1
        row_phase = re.fullmatch(r"phase-([a-h])", str(artefact["category"]))
        rows.append({
            "key": f"artefact:{artefact['id']}",
            "kind": "artefact",
            "standard": artefact["standard"],
            "spec": artefact["spec"],
            "subject": str(artefact["subject"].get("key") or artefact["subject"].get("id") or ""),
            "project": artefact["owner_project"],
            "title": artefact["title"],
            "status": "absent",
            "severity": severity_for(package, finding),
            "owner": (
                str(package.get("apply_to") or "")
                if package
                else str(finding.get("owner_project") or artefact["owner_project"])
            ) or "unassigned",
            "fix_card": card,
            "fix_card_resolution": card_resolution,
            "standard_id": str(finding.get("id") or ""),
            "declared_package_key": declared,
            "target_phase": (
                row_phase.group(1).upper() if row_phase
                else phase_for_project(str(artefact["owner_project"]))
            ),
            "deploy_bound": bool(DEPLOY.search(
                str(artefact.get("category", "")) + " " + " ".join(
                    str(finding.get(key, "")) for key in
                    ("title", "problem", "acceptance", "failure_scenario")
                )
            )),
        })

    for (rule, subject), entry in exp["stale"].items():
        row = source["rules"].get(rule) or {}
        package = by_key.get(f"{rule}:{subject}")
        rows.append({
            "key": f"baseline:{rule}:{subject}",
            "kind": "baseline",
            "standard": str(row.get("standard") or "governance/conformance.json"),
            "spec": str(row.get("spec") or "unassigned"),
            "subject": str(subject),
            "project": str(subject) if subject in phase_of else "unassigned",
            "title": f"Stale baseline entry: {rule} ({subject})",
            "status": "stale",
            "severity": str(entry["severity"]),
            "owner": str(package.get("apply_to") or "unassigned") if package else "unassigned",
            "fix_card": f"baseline:{rule}:{subject}",
            "fix_card_resolution": "baseline_key",
            "standard_id": "",
            "declared_package_key": "",
            "target_phase": phase_for_project(str(subject)),
            "deploy_bound": False,
        })

    for finding in exp["open"].values():
        rows.append({
            "key": f"ledger:{finding['id']}",
            "kind": "ledger",
            "standard": str(finding["standard"]),
            "spec": str(finding["spec"]),
            "subject": str(finding.get("area") or ""),
            "project": str(finding["owner_project"]),
            "title": str(finding["title"]),
            "status": "open",
            "severity": str(finding["severity"]),
            "owner": str(finding["owner_project"]),
            "fix_card": str(finding["id"]),
            "fix_card_resolution": "finding_id",
            "standard_id": str(finding["id"]),
            "declared_package_key": "",
            "target_phase": phase_for_project(str(finding["owner_project"])),
            "deploy_bound": bool(DEPLOY.search(" ".join(
                str(finding.get(key, "")) for key in
                ("title", "problem", "acceptance", "failure_scenario")
            ))),
        })

    order = {kind: position for position, kind in enumerate(KINDS)}
    rows.sort(key=lambda row: (order[row["kind"]], row["key"]))
    return {
        "schema": "v2-gap-ledger/v1",
        "lane_pin": pin,
        "generated_from": {
            "artefact_record": {
                "command": source["command"],
                "summary": source["standards"]["summary"],
            },
            "sources": source["inputs"],
        },
        "row_schema": {
            "key": "artefact:<record id> | baseline:<rule>:<subject> | ledger:<finding id>",
            "kind": "artefact | baseline | ledger",
            "standard": "the artefact/finding standard, or the conformance rule's standard",
            "spec": "the artefact/finding spec clause, or the conformance rule's spec",
            "subject": "the artefact subject key, the baseline subject, or the finding area",
            "project": "the owning project key; 'unassigned' when the source names none",
            "title": "the record title; baseline rows render '<rule> (<subject>)'",
            "status": "absent (artefact), stale (baseline), open (ledger)",
            "severity": "the more severe of the generated finding and its fix package; 'unassigned' is refused for an artefact row",
            "owner": "fix package apply_to, else finding owner_project, else 'unassigned'",
            "fix_card": "the register package key that owns the work, else the generated finding id",
            "fix_card_resolution": "published_package | covered_package | artefact_package | finding_id | declared_package_absent | baseline_key | unresolved",
            "standard_id": "the generated finding id for this artefact, empty when the standard emits no finding",
            "declared_package_key": "the package_key the artefact record declares, kept so a dangling declaration stays visible",
            "target_phase": "owner project ADM phase; a phase-* artefact uses its letter; else 'unassigned'",
            "deploy_bound": "true only when the row text names an action from migrations 0062-0066 or the migration itself",
        },
        "derivation": {
            "absent": "status == 'absent', mandatory, and the owner project not archived — the same predicate build_findings uses",
            "stale": "a baseline (rule, subject) absent from the live gap register",
            "open": "a v1 finding with empty resolved_by",
            "fixed_artefacts": "register package key, else covered key, else the generated finding id; a declaration absent from the register is recorded, never silently accepted",
            "severity": "the more severe of the generated finding and the fix package; a package may raise severity, never lower it",
            "deploy_bound": "the not-yet-deployed set is migrations 0062-0066 and the authoring.* family 0066 creates",
        },
        "counts": counts(rows, resolution, declared_absent),
        "rows": rows,
    }


def counts(rows: list[dict], resolution: Counter, declared_absent: list[str] | None = None) -> dict:
    return {
        "rows": len(rows),
        "by_kind": dict(Counter(row["kind"] for row in rows)),
        "by_severity": dict(Counter(row["severity"] for row in rows)),
        "by_kind_severity": {
            kind: dict(Counter(row["severity"] for row in rows if row["kind"] == kind))
            for kind in KINDS
        },
        "by_deploy_bound": dict(Counter("true" if row["deploy_bound"] else "false" for row in rows)),
        "by_status": dict(Counter(row["status"] for row in rows)),
        "by_fix_card_resolution": dict(Counter(row.get("fix_card_resolution") for row in rows)),
        "declared_package_keys_absent_from_register": sorted(set(declared_absent or [])),
        "artefact_fix_card_resolution": dict(resolution),
    }


# ── verify ─────────────────────────────────────────────────────────────────────────


def row_text(row: dict, source: dict, exp: dict) -> str:
    if row["kind"] == "ledger":
        finding = exp["open"].get(row["fix_card"], {})
        return " ".join(str(finding.get(key, "")) for key in
                        ("title", "problem", "acceptance", "failure_scenario"))
    if row["kind"] == "artefact":
        artefact = exp["absent"].get(row["key"].split(":", 1)[1], {})
        generated = next((f for f in source["findings"] if f["id"] == artefact.get("id")), {})
        return " ".join(
            [str(artefact.get("category", ""))]
            + [str(generated.get(key, "")) for key in
               ("title", "problem", "acceptance", "failure_scenario")]
        )
    return ""


def check(ledger: dict, source: dict) -> list[str]:
    problems: list[str] = []
    exp = expected(source)
    if ledger.get("schema") != "v2-gap-ledger/v1":
        problems.append(f"schema is {ledger.get('schema')!r}, expected v2-gap-ledger/v1")
    if not ledger.get("lane_pin"):
        problems.append("lane_pin is missing")
    recorded = {
        str(entry.get("path")): str(entry.get("sha256") or "")
        for entry in (ledger.get("generated_from") or {}).get("sources") or []
        if isinstance(entry, dict)
    }
    for entry in source.get("inputs") or []:
        path, live = str(entry["path"]), str(entry["sha256"])
        if path in recorded and recorded[path] != live:
            problems.append(
                f"input changed since the ledger was built: {path} "
                f"({recorded[path][:12]} -> {live[:12]}); rebuild it"
            )
    rows = ledger.get("rows")
    if not isinstance(rows, list) or not rows:
        return problems + ["rows is missing or empty"]

    keys = [str(row.get("key") or "") for row in rows]
    duplicates = sorted(key for key, count in Counter(keys).items() if count > 1)
    if duplicates:
        problems.append(f"duplicate keys: {duplicates[:5]}")
    for row in rows:
        kind = row.get("kind")
        if kind not in KINDS:
            problems.append(f"{row.get('key')}: kind {kind!r} is not one of {KINDS}")
            continue
        for field in ("key", "standard", "spec", "subject", "project", "title",
                      "status", "severity", "owner", "fix_card", "target_phase"):
            if not str(row.get(field) or "").strip():
                problems.append(f"{row.get('key')}: field {field!r} is empty")
        if row.get("status") != STATUS[kind]:
            problems.append(f"{row.get('key')}: status {row.get('status')!r} != {STATUS[kind]!r}")
        if not isinstance(row.get("deploy_bound"), bool):
            problems.append(f"{row.get('key')}: deploy_bound is not a boolean")
        elif row["deploy_bound"] != bool(DEPLOY.search(row_text(row, source, exp))):
            problems.append(f"{row.get('key')}: deploy_bound disagrees with its source text")

    got = {kind: {str(r["key"]) for r in rows if r.get("kind") == kind} for kind in KINDS}
    want = {
        "artefact": {f"artefact:{key}" for key in exp["absent"]},
        "baseline": {f"baseline:{rule}:{subject}" for rule, subject in exp["stale"]},
        "ledger": {f"ledger:{key}" for key in exp["open"]},
    }
    for kind in KINDS:
        if got[kind] != want[kind]:
            missing = sorted(want[kind] - got[kind])[:3]
            extra = sorted(got[kind] - want[kind])[:3]
            problems.append(
                f"{kind}: key set mismatch (missing {missing}, extra {extra}, "
                f"want {len(want[kind])}, got {len(got[kind])})"
            )

    open_keys = [row["key"] for row in rows if row.get("kind") == "ledger"]
    if len(open_keys) != len(exp["open"]):
        problems.append(f"open findings: {len(open_keys)} rows, source has {len(exp['open'])}")
    if len(set(open_keys)) != len(open_keys):
        problems.append("open findings: a ledger row is repeated")
    for identifier in exp["open"]:
        if open_keys.count(f"ledger:{identifier}") != 1:
            problems.append(f"open finding {identifier} does not appear exactly once")

    for row in rows:
        if row.get("owner") != "unassigned" and not str(row.get("owner") or "").strip():
            problems.append(f"{row.get('key')}: owner is empty and not 'unassigned'")
        if row.get("kind") == "ledger":
            finding = exp["open"].get(row.get("fix_card"))
            if finding is None:
                problems.append(f"{row.get('key')}: fix_card is not an open finding id")
            else:
                if row.get("owner") != finding["owner_project"]:
                    problems.append(f"{row.get('key')}: owner != finding owner_project")
                if row.get("severity") != finding["severity"]:
                    problems.append(f"{row.get('key')}: severity != finding severity")
        elif row.get("kind") == "baseline":
            if row.get("fix_card") != row.get("key"):
                problems.append(f"{row.get('key')}: baseline fix_card is not its key")
        else:
            artefact = exp["absent"].get(str(row.get("key")).split(":", 1)[1])
            if artefact is None:
                problems.append(f"{row.get('key')}: not an absent mandatory artefact in the record")
                continue
            finding = next((f for f in source["findings"] if f["id"] == artefact["id"]), {})
            card = str(row.get("fix_card") or "")
            resolution = str(row.get("fix_card_resolution") or "")
            declared = str(artefact.get("package_key") or "")
            if resolution not in RESOLUTIONS:
                problems.append(
                    f"{row.get('key')}: fix_card_resolution {resolution!r} "
                    f"is not one of {sorted(RESOLUTIONS)}"
                )
            if row.get("severity") == "unassigned":
                problems.append(
                    f"{row.get('key')}: severity is unassigned, but the standard holds "
                    f"this artefact to account"
                )
            if str(row.get("standard_id") or "") != str(finding.get("id") or ""):
                problems.append(f"{row.get('key')}: standard_id disagrees with the generated finding")
            # A claimed package resolution must be a real register key. The old check compared
            # the fix card against the artefact's own declared `package_key`, so any dangling
            # declaration accepted itself.
            if resolution in PACKAGE_RESOLUTIONS and card not in exp["register_keys"]:
                problems.append(
                    f"{row.get('key')}: fix_card_resolution {resolution!r} claims a package "
                    f"key {card!r} that is not in the register"
                )
            known = (
                card in exp["register_keys"]
                or card == str(finding.get("id") or "")
                or card == str(artefact.get("covered_key") or "")
                or (resolution == "declared_package_absent" and card == declared and card)
            )
            if not known:
                problems.append(
                    f"{row.get('key')}: fix_card {card!r} resolves to no register package, "
                    f"generated finding or covered key"
                )
            # Severity is derived, so it can be re-derived. This is the assertion that makes a
            # package's demotion of a rule-derived P0 a refusal rather than a silent edit.
            package = exp["register"].get(card) if resolution in PACKAGE_RESOLUTIONS else None
            want_severity = severity_for(package, finding)
            if str(row.get("severity") or "") != want_severity:
                problems.append(
                    f"{row.get('key')}: severity {row.get('severity')!r} != the derived "
                    f"{want_severity!r} (finding {finding.get('severity')!r}, "
                    f"package {(package or {}).get('severity')!r})"
                )

    stated = ledger.get("counts") or {}
    if stated.get("rows") != len(rows):
        problems.append(f"counts.rows {stated.get('rows')} != {len(rows)}")
    if stated.get("by_kind") != {kind: len(want[kind]) for kind in KINDS}:
        problems.append(f"counts.by_kind {stated.get('by_kind')} != source counts")
    return problems


def self_test(ledger: dict, source: dict) -> list[str]:
    """Prove the checker refuses each defect the card names."""
    failures: list[str] = []

    def artefact_row(doc: dict) -> dict:
        return next(row for row in doc["rows"] if row["kind"] == "artefact")

    def demote(doc: dict) -> None:
        row = artefact_row(doc)
        row["severity"] = "P2" if row["severity"] != "P2" else "P1"

    def unassigned(doc: dict) -> None:
        artefact_row(doc)["severity"] = "unassigned"

    def dangling(doc: dict) -> None:
        row = artefact_row(doc)
        row["fix_card"] = "no-such-package:nowhere"
        row["fix_card_resolution"] = "declared_package"

    def fabricated(doc: dict) -> None:
        row = artefact_row(doc)
        row["fix_card"] = "no-such-package:nowhere"
        row["fix_card_resolution"] = "published_package"

    mutations = [
        ("duplicate key", lambda doc: doc["rows"].append(copy.deepcopy(doc["rows"][0]))),
        ("missing owner", lambda doc: doc["rows"][0].update(owner="")),
        ("missing fix card", lambda doc: doc["rows"][0].update(fix_card="")),
        ("missing open finding", lambda doc: doc["rows"].pop(
            next(i for i, row in enumerate(doc["rows"]) if row["kind"] == "ledger"))),
        ("missing artefact row", lambda doc: doc["rows"].pop(
            next(i for i, row in enumerate(doc["rows"]) if row["kind"] == "artefact"))),
        ("severity demoted below its finding", demote),
        ("severity unassigned", unassigned),
        ("fix card dangling without a disposition", dangling),
        ("package resolution that is not in the register", fabricated),
    ]
    for name, mutate in mutations:
        broken = copy.deepcopy(ledger)
        mutate(broken)
        if not check(broken, source):
            failures.append(f"negative control not refused: {name}")
        else:
            print(f"  self-test refused: {name}")
    print(f"  self-test: {len(mutations)} negative controls")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--build", action="store_true", help="regenerate the ledger")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--ledger", type=Path, default=DEFAULT_LEDGER)
    parser.add_argument("--snapshot", type=Path, default=DEFAULT_SNAPSHOT)
    parser.add_argument("--outputs", type=Path, default=DEFAULT_OUTPUTS)
    parser.add_argument("--register", type=Path, default=DEFAULT_REGISTER)
    parser.add_argument("--gaps", type=Path, default=DEFAULT_GAPS)
    parser.add_argument("--baseline", type=Path, default=DEFAULT_BASELINE)
    parser.add_argument("--findings", type=Path, default=DEFAULT_FINDINGS)
    args = parser.parse_args()

    with tempfile.TemporaryDirectory(prefix="v2-gap-ledger-") as temporary:
        source = regenerate(args, Path(temporary))
        if args.build:
            ledger = build(source, args, lane_pin())
            args.ledger.parent.mkdir(parents=True, exist_ok=True)
            args.ledger.write_text(
                json.dumps(ledger, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
            )
            print(f"v2-gap-ledger: wrote {args.ledger} at lane pin {ledger['lane_pin']}")
        else:
            ledger = read(args.ledger)
        problems = check(ledger, source)
        if args.self_test:
            problems += self_test(ledger, source)

    if problems:
        print("v2-gap-ledger: FAIL", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1
    rows = ledger["rows"]
    severity = Counter(row["severity"] for row in rows)
    owners = Counter(row["owner"] for row in rows)
    deploy = Counter("true" if row["deploy_bound"] else "false" for row in rows)
    kinds = Counter(row["kind"] for row in rows)
    print("v2-gap-ledger: PASS")
    print(f"  rows {len(rows)} = artefact {kinds['artefact']} + "
          f"baseline {kinds['baseline']} + ledger {kinds['ledger']}")
    print("  severity " + " · ".join(f"{name} {severity[name]}" for name in sorted(severity)))
    print(f"  deploy_bound false {deploy['false']} · true {deploy['true']}")
    print(f"  owners non-empty {len(rows)} (unassigned {owners['unassigned']})")
    print(f"  fix cards non-empty {sum(1 for row in rows if row['fix_card'])}")
    print(f"  duplicate keys 0 · open ledger ids {kinds['ledger']}, each exactly once")
    resolution = Counter(row.get("fix_card_resolution") for row in rows)
    print("  fix cards " + " · ".join(f"{name} {count}" for name, count in sorted(resolution.items())))
    dangling = (ledger.get("counts") or {}).get("declared_package_keys_absent_from_register") or []
    print(f"  declared package keys absent from the register: {len(dangling)}")
    if args.self_test:
        print("  self-test: PASS (every negative control refused)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
