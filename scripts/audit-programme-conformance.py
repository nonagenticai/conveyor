#!/usr/bin/env python3
"""Audit MSP/TOGAF conformance against the programme's own record.

The audit is read-only. It compares four sources — the checked-in seed (the plan), the public
snapshot (the live record at publish time), the output standard and the repository — and emits a
stable, severity-ranked gap register. It never edits the programme and never treats a seed row as
evidence that a gate happened.

Usage:
    python3 scripts/audit-programme-conformance.py --self-test
    python3 scripts/audit-programme-conformance.py --check-standard
    python3 scripts/audit-programme-conformance.py --snapshot docs/design/data --report
    python3 scripts/audit-programme-conformance.py --snapshot <fresh> --strict --fail-on P0,P1

The snapshot is frozen at publish. CI audits the checked-in snapshot as a report; the publish
script audits a fresh temp snapshot before it mirrors anything.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]


def acceptance_status(key: str) -> str | None:
    """The acceptance manifest's pinned target status for `<KEY>-project`, when it has one.

    A manifest that pins `active` holds the project open by decision (S05 until its six gate
    properties execute; T18/T14 by their acceptance), so the seed's historical status is not the
    plan and its difference is not a drift. Measured 2026-11-25: the held projects carried P1
    drift packages the record had settled by decision.
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
DEFAULT_SNAPSHOT = ROOT / "docs" / "design" / "data"
DEFAULT_STANDARD = ROOT / "governance" / "conformance.json"
DEFAULT_BASELINE = ROOT / "docs" / "evidence" / "conformance-baseline.json"
DEFAULT_OUT = ROOT / "docs" / "evidence"
# The two findings ledgers are part of the register's provenance: a --rework-check that omits
# one proves nothing about the findings it carries. They are the default inputs when the caller
# passes no --findings at all.
DEFAULT_FINDINGS = (
    ROOT / "docs" / "evidence" / "v1-conformance-findings.json",
    ROOT / "docs" / "evidence" / "standards-findings.json",
)
# A standard finding whose artifact is one of these is a code defect, not only a template
# wording defect. It must also produce a CODE sibling with an executable acceptance test, so a
# record-only repair cannot close the work package while the code change is open.
CODE_OWNED_ARTIFACT_PREFIXES = (
    "seed/output-standard.json#",
    "crates/store/src/snapshot.rs",
    "scripts/compose-artefact-bodies.py",
    "scripts/generate-standards.py",
)
# An acceptance_test is a command, not prose. The first token must be one of the runners this
# repository actually uses, so a review cannot satisfy the code loop with a sentence.
EXECUTABLE_TEST_PREFIXES = (
    "cargo ",
    "cargo-guard.py ",
    "python3 ",
    "gh ",
    "bash ",
    "sh ",
    "./",
)
SEVERITY_ORDER = {"P0": 0, "P1": 1, "P2": 2, "P3": 3}
REGISTER_SCHEMA = "conveyor-rework/v1"
REGISTER_STANDARDS = ("MSP", "TOGAF", "SAFe", "PMP", "SPEC")
REGISTER_SCOPES = ("project", "tranche", "programme")
REGISTER_MAX_SUBJECTS = 20
# No disposition suppresses a package (ADR 0012): a baseline row recorded as a waiver is
# still an open gap. The tuple is kept as the one extension point so a future legal
# suppression has a single place to be declared.
REGISTER_SUPPRESSED = ()

# A test row is not a programme project. The marker is `meta.test_row = true`; the two known
# keys predate the marker and are named explicitly rather than matched by prefix, so a future
# unmarked row stays visible as a finding (`v1-e2e-1`) instead of disappearing silently.
TEST_ROW_MARKER = "test_row"
LEGACY_TEST_PROJECT_KEYS = (
    "AAA-UI-PROOF-285-2026-11-19T07-3-2",
    "AAA-UI-PROOF-285-2026-11-19T07-4",
)


def test_row_reason(row: dict[str, Any]) -> str | None:
    """Why this project row is test residue, or None when it is a real project."""
    meta = row.get("meta")
    if isinstance(meta, dict) and meta.get(TEST_ROW_MARKER) is True:
        return f"meta.{TEST_ROW_MARKER} is true"
    key = str(row.get("key") or "")
    if key in LEGACY_TEST_PROJECT_KEYS:
        return "a known pre-marker e2e project key"
    return None
def register_acceptance(
    key: str,
    snapshot: str = "docs/design/data",
    findings: list[str] | None = None,
    deep_review: list[str] | None = None,
    acceptance_test: str | None = None,
) -> str:
    """The command that proves one package is closed, not a whole-programme report.

    Rebuilding the register from the ledgers is the check: a finding marked `resolved_by` and a
    gap that no longer exists in the snapshot both drop the package, and `--rework-check` exits
    1 while the key is still open. The command must mirror the inputs the register was built
    from — the snapshot and every findings ledger and deep-review sidecar. A sidecar omitted
    here makes its keys invisible to the check, which then exits 0 without proving anything.

    A code sibling carries its own executable test instead: prose is refused, because a record
    repair alone cannot evidence a code change.
    """
    if acceptance_test is not None:
        test = acceptance_test.strip()
        if not test.startswith(EXECUTABLE_TEST_PREFIXES):
            fail(
                f"the code-owned package {key!r} has an acceptance_test that is not an "
                f"executable command: {test!r}"
            )
        return test
    findings = findings or [
        "docs/evidence/v1-conformance-findings.json",
        "docs/evidence/standards-findings.json",
    ]
    deep_review = deep_review or [
        "docs/evidence/record-w3/deep-review-findings.json",
        "docs/evidence/record-w4/deep-review-findings.json",
    ]
    parts = ["python3 scripts/audit-programme-conformance.py", f"--snapshot {snapshot}"]
    parts.extend(f"--findings {path}" for path in findings)
    parts.extend(f"--deep-review {path}" for path in deep_review)
    parts.append(f"--rework-check {key}")
    return " ".join(parts)
PLACEHOLDER = re.compile(
    r"(not measured|no instrument|unknown|tbd|to be determined|n/?a|placeholder|none)",
    re.IGNORECASE,
)
SCRIPT_REF = re.compile(r"(scripts/[A-Za-z0-9_./-]+\.(?:py|sh|cjs))")


def fail(message: str) -> None:
    sys.exit(f"audit-programme-conformance: {message}")


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
        fail(f"{path} is missing")
    # A clean checkout carries the placeholder (`= null`) until `conveyor snapshot
    # --outputs-dir` writes the live index. "No output published yet" is an empty index, not a
    # parse failure — the committed placeholder is the repository's normal state (#481).
    if re.search(r"window\.CONVEYOR_OUTPUTS\s*=\s*null\s*;", text):
        return {}
    match = re.search(r"window\.CONVEYOR_OUTPUTS\s*=\s*(\{.*\});\s*$", text, re.S)
    if not match:
        fail(f"{path} has no CONVEYOR_OUTPUTS assignment")
    # The checked-in placeholder is `null`: the pack carries no published-output index until the
    # publish path overwrites it. Treat that as an empty index; the output checks then report
    # every declared output as unpublished (the publish path runs strict with the real index).
    if match.group(1) == "null":
        return {}
    return json.loads(match.group(1))


def parse_time(value: Any) -> datetime | None:
    if not isinstance(value, str) or not value:
        return None
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None


def non_empty(value: Any) -> bool:
    if isinstance(value, str):
        return bool(value.strip())
    return value is not None and value != [] and value != {}


def profile_core(profile: dict[str, Any]) -> dict[str, Any]:
    return {
        key: profile.get(key)
        for key in ("key", "phase_set", "artifact_requirements", "contract_kinds")
    }


def profile_hash(profile: dict[str, Any]) -> str:
    text = json.dumps(profile_core(profile), sort_keys=True, separators=(",", ":"))
    return "sha256:" + hashlib.sha256(text.encode("utf-8")).hexdigest()


class Dataset:
    """One audit's inputs, with small lookup helpers."""

    def __init__(
        self,
        *,
        programme: dict[str, Any],
        msp: dict[str, Any],
        architecture: dict[str, Any],
        manifest: dict[str, Any],
        outputs: dict[str, Any],
        coverage: dict[str, Any],
        issues: dict[str, Any],
        seed: dict[str, Any],
        definition: dict[str, Any],
        output_standard: dict[str, Any],
        standard: dict[str, Any],
        repo_root: Path,
    ) -> None:
        self.programme = programme
        self.msp = msp
        self.architecture = architecture
        self.manifest = manifest
        self.outputs = outputs
        self.coverage = coverage
        self.issues = issues
        self.seed = seed
        self.definition = definition
        self.output_standard = output_standard
        self.standard = standard
        self.repo_root = repo_root
        all_projects = [
            row for row in programme.get("projects", []) if isinstance(row, dict)
        ]
        # Test residue is excluded from every rule's subject set, and named in the report, so
        # the exclusion is visible rather than a silent filter.
        self.excluded_test_projects = [
            {"key": row.get("key"), "reason": test_row_reason(row)}
            for row in all_projects
            if test_row_reason(row)
        ]
        self.projects = [row for row in all_projects if not test_row_reason(row)]
        self.by_key = {row.get("key"): row for row in self.projects if row.get("key")}
        self.by_id = {row.get("id"): row for row in self.projects if row.get("id")}
        self.seed_by_key = {
            row.get("key"): row
            for row in seed.get("projects", [])
            if isinstance(row, dict) and row.get("key")
        }
        self.tranches = [
            row for row in msp.get("tranches", []) if isinstance(row, dict)
        ]
        self.tranche_by_number = {
            row.get("number"): row for row in self.tranches if row.get("number") is not None
        }
        self.tranche_by_id = {
            row.get("id"): row for row in self.tranches if row.get("id")
        }
        self.reviews = [
            row for row in msp.get("tranche_reviews", []) if isinstance(row, dict)
        ]
        self.benefits = [
            row for row in msp.get("benefit_profiles", []) if isinstance(row, dict)
        ]
        self.links = [row for row in msp.get("benefit_links", []) if isinstance(row, dict)]
        self.transitions = [
            row for row in msp.get("benefit_transitions", []) if isinstance(row, dict)
        ]
        self.contracts = [
            row for row in architecture.get("contracts", []) if isinstance(row, dict)
        ]
        self.reviews_arch = [
            row
            for row in architecture.get("compliance_reviews", [])
            if isinstance(row, dict)
        ]
        self.waivers = [
            row for row in architecture.get("waivers", []) if isinstance(row, dict)
        ]
        self.artefacts = [
            row for row in architecture.get("artifacts", []) if isinstance(row, dict)
        ]
        self.artifact_types = {
            row.get("id"): row
            for row in architecture.get("standard", [])
            if isinstance(row, dict) and row.get("id")
        }
        self.published: dict[str, set[str]] = {}
        for row in outputs.get("outputs", []):
            if not isinstance(row, dict):
                continue
            key = row.get("project_key")
            category = row.get("category")
            if isinstance(key, str) and isinstance(category, str):
                self.published.setdefault(key, set()).add(category)
        self.coverage_by_key = {
            row.get("key"): row
            for row in coverage.get("projects", [])
            if isinstance(row, dict) and row.get("key")
        }
        self.profile = next(
            (
                row
                for row in architecture.get("profiles", [])
                if isinstance(row, dict)
                and row.get("key") == standard.get("sources", {}).get("profile_key")
            ),
            None,
        )

    def class_for(self, project: dict[str, Any]) -> dict[str, Any] | None:
        facts = {
            "key": project.get("key"),
            "track": project.get("meta", {}).get("track"),
            "status": project.get("status"),
            "measured_status": project.get("meta", {}).get("measured_status"),
        }
        for cls in self.output_standard.get("classes", []):
            if all(facts.get(key) == value for key, value in cls.get("when", {}).items()):
                return cls
        return None

    def declared(self, key: str, include_closure: bool = True) -> list[str]:
        project = self.by_key.get(key, {})
        meta = project.get("meta", {}) or {}
        out: list[str] = []
        fields = ("expected_outputs", "closure_outputs") if include_closure else ("expected_outputs",)
        for field in fields:
            for category in meta.get(field) or []:
                if category not in out:
                    out.append(category)
        return out

    def phase(self, key: str) -> tuple[str | None, str | None]:
        entry = self.definition.get("projects", {}).get(key) or {}
        meta = self.by_key.get(key, {}).get("meta", {})
        phase = entry.get("adm_phase") or meta.get("adm_phase")
        plateau = entry.get("plateau") or meta.get("plateau")
        return (str(phase).lower() if phase else None, plateau)

    def projection(self, name: str) -> list[dict[str, Any]] | None:
        """A published MSP projection: None when the section is absent, a list otherwise."""
        value = self.msp.get(name)
        if value is None:
            return None
        if not isinstance(value, list):
            return []
        return [row for row in value if isinstance(row, dict)]


class Audit:
    def __init__(self, data: Dataset, standard: dict[str, Any]) -> None:
        self.data = data
        self.standard = standard
        self.gaps: list[dict[str, Any]] = []

    def add(
        self,
        rule: str,
        severity: str,
        subject: str,
        detail: str,
        *,
        disposition: str = "rework",
        evidence: Any = None,
    ) -> None:
        self.gaps.append(
            {
                "rule": rule,
                "severity": severity,
                "subject": subject,
                "detail": detail,
                "disposition": disposition,
                "evidence": evidence,
            }
        )

    def unprovable(self, rule: str, subject: str, detail: str) -> None:
        self.add(rule, "P2", subject, detail, disposition="unprovable")

    # ── Generated-artefact checks (P7) ───────────────────────────────────────────────

    def dependencies(self) -> None:
        """The declared dependency graph: unknown targets, untyped edges, cycles.

        The edges are the programme's DECLARED meta (`meta.depends_on`), not a derived section:
        a declaration that names no project, or carries no kind, is a finding about the declaration.
        """
        typed = {"blocks", "needs", "supersedes", "informs"}
        edges: list[tuple[str, str]] = []
        untyped = 0
        for project in self.data.projects:
            key = str(project.get("key"))
            for item in (project.get("meta") or {}).get("depends_on") or []:
                if isinstance(item, str):
                    target, kind = item, None
                elif isinstance(item, dict):
                    target, kind = item.get("to") or item.get("project"), item.get("kind")
                else:
                    continue
                if not isinstance(target, str) or not target:
                    continue
                if target not in self.data.by_key:
                    self.add(
                        "dependency_unknown_target",
                        "P1",
                        key,
                        f"depends_on names {target!r}, which is not a project in the programme",
                    )
                if kind not in typed:
                    untyped += 1
                if target in self.data.by_key:
                    edges.append((key, target))
        if untyped:
            self.add(
                "dependency_untyped",
                "P2",
                "programme",
                f"{untyped} declared dependency edge(s) carry no blocks|needs|supersedes|informs kind",
            )
        for cycle in self._cycles(edges):
            self.add(
                "dependency_cycle",
                "P0",
                ",".join(cycle),
                f"the declared dependency graph contains the cycle {' -> '.join(cycle)}",
            )

    @staticmethod
    def _cycles(edges: list[tuple[str, str]]) -> list[list[str]]:
        graph: dict[str, list[str]] = {}
        for source, target in edges:
            graph.setdefault(source, []).append(target)
        colour: dict[str, int] = {}
        stack: list[str] = []
        found: list[list[str]] = []

        def visit(node: str) -> None:
            colour[node] = 1
            stack.append(node)
            for nxt in graph.get(node, []):
                if colour.get(nxt, 0) == 1:
                    start = stack.index(nxt)
                    cycle = stack[start:] + [nxt]
                    if cycle not in found:
                        found.append(cycle)
                elif colour.get(nxt, 0) == 0:
                    visit(nxt)
            stack.pop()
            colour[node] = 2

        for node in sorted(graph):
            if colour.get(node, 0) == 0:
                visit(node)
        return found

    def cadence(self) -> None:
        """A published cadence section with no row is a SAFe gap; an absent section is not."""
        cadences = self.data.msp.get("cadences")
        if cadences is None:
            return
        if not cadences:
            self.add(
                "cadence_absent",
                "P1",
                "programme",
                "the snapshot publishes no programme_cadence row (pi|demo|retro|refinement)",
            )

    def wbs(self) -> None:
        """A published WBS section with no node for an active project is a §5.1 gap."""
        nodes = self.data.msp.get("wbs_nodes")
        if nodes is None:
            return
        planned = {
            str(node.get("project_id"))
            for node in nodes
            if isinstance(node, dict) and node.get("project_id")
        }
        for project in self.data.projects:
            if project.get("status") == "active" and str(project.get("id")) not in planned:
                self.add(
                    "wbs_absent",
                    "P2",
                    str(project.get("key")),
                    "no WBS node is published for the active project",
                )

    # ── SAFe §8.2 ────────────────────────────────────────────────────────────────────

    def safe(self) -> None:
        """The SAFe planning, demonstration and learning controls on the tranche record."""
        authorising = set(
            self.standard.get("programme", {}).get("tranche_states_authorising_delivery", [])
        )
        cadences = self.data.projection("cadences")
        carrying = self.carrying_tranches()

        for tranche in carrying:
            subject = self.tranche_subject(tranche)
            # cadence_absent owns the globally-empty case; these rules are per tranche.
            if cadences and not self.cadence_for(cadences, tranche, "pi"):
                self.add(
                    "pi_cadence_missing",
                    "P1",
                    subject,
                    "the tranche carries projects but has no PI cadence for its period",
                )
            if cadences and not self.cadence_for(cadences, tranche, "refinement"):
                self.add(
                    "backlog_refinement_missing",
                    "P1",
                    subject,
                    "the tranche has no refinement cadence for its period",
                )

        for tranche in carrying:
            if tranche.get("state") not in authorising:
                continue
            subject = self.tranche_subject(tranche) + " PI"
            if not self.pi_output("pi-plan", tranche):
                self.add(
                    "pi_plan_missing",
                    "P0",
                    subject,
                    "the PI carried by an authorised tranche has no published pi-plan output",
                )
            if not self.pi_output("pi-objectives", tranche):
                self.add(
                    "pi_objectives_missing",
                    "P1",
                    subject,
                    "the PI carried by an authorised tranche has no published pi-objectives output",
                )

        self.demo_credits(authorising)
        self.retro_lessons(cadences)
        self.wsjf_inputs()

    def pi_output(self, category: str, tranche: dict[str, Any]) -> bool:
        """The PI artefact must name this tranche, not merely exist somewhere."""
        programme_key = self.standard.get("programme", {}).get("project_key", "P0")
        needle = self.tranche_subject(tranche).replace(" ", "-").lower()
        rows = self.data.outputs.get("outputs", []) if isinstance(self.data.outputs, dict) else []
        for row in rows:
            if not isinstance(row, dict):
                continue
            if row.get("category") != category or row.get("project_key") != programme_key:
                continue
            label = f"{row.get('title') or ''} {row.get('unit') or ''}".lower()
            if needle in label:
                return True
        return False

    def carrying_tranches(self) -> list[dict[str, Any]]:
        declared = set()
        for project in self.data.projects:
            try:
                declared.add(int((project.get("meta") or {}).get("tranche")))
            except (TypeError, ValueError):
                continue
        carrying = []
        for tranche in self.data.tranches:
            try:
                number = int(tranche.get("number"))
            except (TypeError, ValueError):
                continue
            if number in declared:
                carrying.append(tranche)
        return carrying

    @staticmethod
    def tranche_subject(tranche: dict[str, Any]) -> str:
        return f"tranche {tranche.get('number')}"

    @staticmethod
    def cadence_kind(cadence: dict[str, Any]) -> str:
        return str(cadence.get("kind") or "").lower()

    @staticmethod
    def tranche_periods(tranche: dict[str, Any]) -> set[str]:
        """The tranche projection has no period column today; accept one, plus its number."""
        values: set[str] = set()
        meta = tranche.get("meta") if isinstance(tranche.get("meta"), dict) else {}
        for source in (tranche, meta):
            for field in ("period", "pi", "pi_number"):
                value = source.get(field)
                if value not in (None, ""):
                    values.add(str(value))
        if tranche.get("number") is not None:
            values.add(str(tranche.get("number")))
        return values

    def cadence_covers(self, cadence: dict[str, Any], tranche: dict[str, Any]) -> bool:
        """The cadence projection carries a period and optional meta, not a tranche column."""
        meta = cadence.get("meta") if isinstance(cadence.get("meta"), dict) else {}
        identifiers = {str(tranche.get("id")), str(tranche.get("number"))}
        periods = self.tranche_periods(tranche)
        for source in (cadence, meta):
            for field in ("tranche_id", "tranche_number", "tranche"):
                value = source.get(field)
                if value not in (None, "") and str(value) in identifiers:
                    return True
            for field in ("period", "pi", "pi_number"):
                value = source.get(field)
                if value not in (None, "") and str(value) in periods:
                    return True
        return False

    def cadence_for(
        self,
        cadences: list[dict[str, Any]] | None,
        tranche: dict[str, Any],
        kind: str,
    ) -> bool:
        if not cadences:
            return False
        return any(
            self.cadence_kind(row) == kind and self.cadence_covers(row, tranche)
            for row in cadences
        )

    def demo_credits(self, authorising: set[str]) -> None:
        nodes = self.data.projection("wbs_nodes")
        if nodes is None:
            self.unprovable(
                "demo_credit_missing",
                "msp.wbs_nodes",
                "the snapshot does not carry the WBS projection",
            )
            return
        credits = self.data.projection("demo_credits")
        if credits is None:
            self.unprovable(
                "demo_credit_missing",
                "msp.demo_credits",
                "the snapshot does not carry the demo-credit projection",
            )
            return
        for node in nodes:
            if node.get("kind") != "deliverable" or node.get("status") != "accepted":
                continue
            tranche = self.node_tranche(node)
            if tranche is None or tranche.get("state") not in authorising:
                continue
            subject = self.node_project_key(node)
            linked = [row for row in credits if row.get("deliverable_id") == node.get("id")]
            if not linked:
                self.add(
                    "demo_credit_missing",
                    "P1",
                    subject,
                    f"accepted deliverable {self.wbs_subject(node)} in an authorised tranche has no demo credit",
                )
            for credit in linked:
                if not non_empty(credit.get("evidence")):
                    self.add(
                        "demo_credit_missing",
                        "P1",
                        subject,
                        f"the demo credit for {self.wbs_subject(node)} carries empty evidence",
                    )

    def retro_lessons(self, cadences: list[dict[str, Any]] | None) -> None:
        # cadence_absent owns the globally-empty case; these rules are per tranche.
        if not cadences:
            return
        lessons = self.data.projection("lessons")
        if lessons is None:
            self.unprovable(
                "retro_lesson_missing",
                "msp.lessons",
                "the snapshot does not carry the lessons projection",
            )
            return
        for tranche in self.data.tranches:
            if tranche.get("state") != "closed":
                continue
            subject = self.tranche_subject(tranche)
            retro = [
                row
                for row in cadences
                if self.cadence_kind(row) == "retro" and self.cadence_covers(row, tranche)
            ]
            if not retro:
                self.add(
                    "retro_lesson_missing",
                    "P1",
                    subject,
                    "the closed tranche has no retro cadence for its period",
                )
                continue
            retro_ids = {str(row.get("id")) for row in retro if row.get("id") is not None}
            taught = any(
                str(row.get("cadence_id")) in retro_ids and non_empty(row.get("statement"))
                for row in lessons
            )
            if not taught:
                self.add(
                    "retro_lesson_missing",
                    "P1",
                    subject,
                    "the tranche's retro produced no lesson",
                )

    def wsjf_inputs(self) -> None:
        nodes = self.data.projection("wbs_nodes")
        if nodes is None:
            self.unprovable(
                "wsjf_inputs_missing",
                "msp.wbs_nodes",
                "the snapshot does not carry the WBS projection",
            )
            return
        for node in nodes:
            if node.get("kind") != "deliverable":
                continue
            problems = []
            size = self.as_number(node.get("job_size"))
            if size is None or size < 1:
                problems.append("job_size is missing or below 1")
            values = [
                self.as_number(node.get(field))
                for field in ("business_value", "time_criticality", "risk_reduction")
            ]
            if all(value is None or value <= 0 for value in values):
                problems.append("business_value, time_criticality and risk_reduction are all zero")
            if problems:
                self.add(
                    "wsjf_inputs_missing",
                    "P1",
                    self.node_project_key(node),
                    f"{self.wbs_subject(node)}: " + "; ".join(problems),
                )

    def node_tranche(self, node: dict[str, Any]) -> dict[str, Any] | None:
        project = self.data.by_id.get(node.get("project_id"))
        if project is None:
            return None
        try:
            number = int((project.get("meta") or {}).get("tranche"))
        except (TypeError, ValueError):
            return None
        return self.data.tranche_by_number.get(number)

    def node_project_key(self, node: dict[str, Any]) -> str:
        project = self.data.by_id.get(node.get("project_id"))
        if project is not None and project.get("key"):
            return str(project.get("key"))
        return self.wbs_subject(node)

    @staticmethod
    def wbs_subject(node: dict[str, Any]) -> str:
        return str(node.get("id") or node.get("title") or "?")

    @staticmethod
    def as_number(value: Any) -> float | None:
        if value is None or isinstance(value, bool):
            return None
        try:
            return float(value)
        except (TypeError, ValueError):
            return None

    def run(self) -> list[dict[str, Any]]:
        self.snapshot_schema()
        self.programme_outputs()
        self.tranche_plan()
        self.tranche_authorisation()
        self.project_tranche()
        self.closure()
        self.declared_outputs()
        self.benefits()
        self.programme_baselines()
        self.profile_and_board()
        self.contracts()
        self.compliance()
        self.waivers()
        self.capability_artefacts()
        self.artefacts()
        self.drift()
        self.derivation()
        self.dependencies()
        self.cadence()
        self.wbs()
        self.safe()
        self.gaps.sort(
            key=lambda gap: (
                SEVERITY_ORDER.get(gap["severity"], 9),
                gap["rule"],
                str(gap["subject"]),
            )
        )
        return self.gaps

    # ── MSP ───────────────────────────────────────────────────────────────────────────

    def snapshot_schema(self) -> None:
        expected = self.standard.get("sources", {}).get("snapshot_schema")
        schema = self.data.manifest.get("schema")
        if schema != expected:
            self.add(
                "snapshot_schema_stale",
                "P2",
                "manifest",
                f"snapshot schema is {schema!r}, expected {expected!r}",
                disposition="unprovable",
            )
        tranches = self.data.msp.get("tranches")
        if isinstance(tranches, list) and tranches and not isinstance(tranches[0], dict):
            self.unprovable("snapshot_schema_stale", "msp.tranches", "tranche rows are not objects")
        elif isinstance(tranches, list) and tranches and all(
            "authorised_at" not in row for row in tranches if isinstance(row, dict)
        ):
            self.unprovable(
                "snapshot_schema_stale",
                "msp.tranches",
                "tranche authorisation columns are absent; the snapshot predates the conformance projection",
            )
        if "compliance_reviews" not in self.data.architecture:
            self.unprovable(
                "snapshot_schema_stale",
                "architecture.compliance_reviews",
                "the snapshot predates the compliance-review projection",
            )
        if "waivers" not in self.data.architecture:
            self.unprovable(
                "snapshot_schema_stale",
                "architecture.waivers",
                "the snapshot predates the waiver projection",
            )

    def programme_outputs(self) -> None:
        key = self.standard.get("programme", {}).get("project_key", "P0")
        project = self.data.by_key.get(key)
        if project is None:
            self.add("programme_outputs", "P1", key, "the programme-definition project is absent")
            return
        cls = self.data.class_for(project)
        if cls is None:
            self.add("programme_outputs", "P1", key, "no output-standard class matches the project")
            return
        published = self.data.published.get(key, set())
        for category in cls.get("now", []):
            if category not in published:
                self.add(
                    "programme_outputs",
                    "P1",
                    f"{key}:{category}",
                    "declared programme output is not published",
                )

    def tranche_plan(self) -> None:
        for tranche in self.data.tranches:
            number = tranche.get("number")
            subject = f"tranche {number}"
            for field in ("entry_criteria", "exit_criteria"):
                if not non_empty(tranche.get(field)):
                    self.add("tranche_plan", "P2", subject, f"{field} is missing")
            if tranche.get("state") == "closed":
                reviews = [
                    row for row in self.data.reviews if row.get("tranche_id") == tranche.get("id")
                ]
                if not reviews:
                    self.add("tranche_plan", "P2", subject, "closed without a review record")
                if not non_empty(tranche.get("review_at")):
                    self.add("tranche_plan", "P2", subject, "closed without a review date")

    def tranche_authorisation(self) -> None:
        authorising = set(
            self.standard.get("programme", {}).get("tranche_states_authorising_delivery", [])
        )
        for review in self.data.reviews:
            if review.get("decision") != "continue":
                continue
            tranche = self.data.tranche_by_id.get(review.get("tranche_id"))
            if tranche is None:
                self.add(
                    "tranche_review_without_authorisation",
                    "P1",
                    str(review.get("tranche_id")),
                    "review names a tranche that is not in the register",
                )
                continue
            number = tranche.get("number")
            next_tranche = self.data.tranche_by_number.get((number or 0) + 1)
            subject = f"tranche {number} review"
            if next_tranche is None:
                self.add(
                    "tranche_review_without_authorisation",
                    "P1",
                    subject,
                    "continue has no next tranche to authorise",
                )
                continue
            state = next_tranche.get("state")
            if state not in authorising:
                self.add(
                    "tranche_review_without_authorisation",
                    "P0",
                    subject,
                    f"continue did not authorise tranche {next_tranche.get('number')}: state is {state!r}",
                )
                continue
            if "authorisation_review_id" in next_tranche:
                linked = next_tranche.get("authorisation_review_id")
                if linked is None:
                    self.add(
                        "tranche_review_without_authorisation",
                        "P1",
                        subject,
                        f"tranche {next_tranche.get('number')} is {state} but names no review",
                    )
                elif linked != review.get("id"):
                    self.add(
                        "tranche_review_without_authorisation",
                        "P0",
                        subject,
                        f"tranche {next_tranche.get('number')} names a different review ({linked})",
                    )
            else:
                self.unprovable(
                    "tranche_review_without_authorisation",
                    subject,
                    "the snapshot does not carry the authorisation link",
                )

    def project_tranche(self) -> None:
        authorising = set(
            self.standard.get("programme", {}).get("tranche_states_authorising_delivery", [])
        )
        for project in self.data.projects:
            key = project.get("key", "?")
            status = project.get("status")
            if status not in {"active", "closed", "archived"}:
                continue
            number = project.get("meta", {}).get("tranche")
            try:
                number = int(number)
            except (TypeError, ValueError):
                self.add("project_in_unauthorised_tranche", "P2", key, "no tranche is declared")
                continue
            tranche = self.data.tranche_by_number.get(number)
            if tranche is None:
                self.add(
                    "project_in_unauthorised_tranche",
                    "P1",
                    key,
                    f"tranche {number} is not in the register",
                )
                continue
            if tranche.get("state") in authorising:
                continue
            severity = "P0" if status == "closed" else "P1"
            if status == "archived":
                severity = "P2"
            self.add(
                "project_in_unauthorised_tranche",
                severity,
                key,
                f"project is {status} in tranche {number} whose state is {tranche.get('state')!r}",
            )

    def closure(self) -> None:
        closure_category = (
            self.standard.get("programme", {}).get("closure_review_category", "closure-review")
        )
        programme_key = self.standard.get("programme", {}).get("project_key", "P0")
        for project in self.data.projects:
            if project.get("status") != "closed":
                continue
            key = project.get("key", "?")
            meta = project.get("meta") or {}
            if not (meta.get("closure_outputs") or []):
                self.add(
                    "closure_outputs_undeclared",
                    "P1",
                    key,
                    "project is closed but declares no closure_outputs",
                )
            declared = self.data.declared(key)
            published = self.data.published.get(key, set())
            if closure_category in published:
                continue
            if key == programme_key and "tranche-review" in published:
                continue
            severity = "P0" if closure_category in declared else "P1"
            self.add(
                "closed_without_closure_review",
                severity,
                key,
                "project is closed but its closure review is not published",
            )

    def declared_outputs(self) -> None:
        for project in self.data.projects:
            key = project.get("key", "?")
            if project.get("status") == "archived":
                continue
            declared = self.data.declared(
                key, include_closure=project.get("status") == "closed"
            )
            if not declared:
                continue
            missing = sorted(set(declared) - self.data.published.get(key, set()))
            if not missing:
                continue
            severity = "P1" if project.get("status") in {"active", "closed"} else "P2"
            self.add(
                "unpublished_declared_output",
                severity,
                key,
                "declared outputs are not published: " + ", ".join(missing),
            )

    def benefits(self) -> None:
        linked = {row.get("benefit_id") for row in self.data.links}
        for benefit in self.data.benefits:
            ref = benefit.get("reference", "?")
            missing = []
            for field in ("target", "measurement_method", "operational_area"):
                if not non_empty(benefit.get(field)):
                    missing.append(field)
            if not (benefit.get("owner_job_id") or benefit.get("benefit_owner_job_id")):
                missing.append("owner")
            if benefit.get("id") not in linked:
                missing.append("benefits-map edge")
            if missing:
                self.add(
                    "benefit_profile_incomplete",
                    "P1",
                    ref,
                    "missing " + ", ".join(missing),
                )
            baseline = benefit.get("baseline")
            if not non_empty(baseline):
                self.add(
                    "benefit_baseline_placeholder",
                    "P1",
                    ref,
                    "no baseline is recorded",
                )
            elif isinstance(baseline, str):
                if PLACEHOLDER.search(baseline):
                    self.add(
                        "benefit_baseline_placeholder",
                        "P1",
                        ref,
                        f"baseline is a placeholder: {baseline!r}",
                    )
                else:
                    self.add(
                        "benefit_baseline_placeholder",
                        "P2",
                        ref,
                        "baseline is a free string, not a measurement record",
                    )
            elif isinstance(baseline, dict):
                absent = [
                    field
                    for field in ("value", "unit", "measured_at", "method")
                    if not non_empty(baseline.get(field))
                ]
                if absent:
                    self.add(
                        "benefit_baseline_placeholder",
                        "P1",
                        ref,
                        "measurement record is missing " + ", ".join(absent),
                    )
            else:
                self.add(
                    "benefit_baseline_placeholder",
                    "P1",
                    ref,
                    f"baseline has unexpected type {type(baseline).__name__}",
                )
            if benefit.get("status") == "realized":
                rows = [
                    row
                    for row in self.data.transitions
                    if row.get("benefit_id") == benefit.get("id")
                ]
                ok = any(
                    row.get("baseline_measured_at")
                    and row.get("post_measured_at")
                    and non_empty(row.get("evidence"))
                    for row in rows
                )
                if not ok:
                    self.add(
                        "realized_without_transition",
                        "P0",
                        ref,
                        "realized without a measured baseline, post-measurement and evidence",
                    )
        for transition in self.data.transitions:
            benefit = next(
                (
                    row
                    for row in self.data.benefits
                    if row.get("id") == transition.get("benefit_id")
                ),
                {},
            )
            subject = benefit.get("reference", str(transition.get("benefit_id")))
            baseline = parse_time(transition.get("baseline_measured_at"))
            enacted = parse_time(transition.get("enacted_at"))
            post = parse_time(transition.get("post_measured_at"))
            if post and not baseline:
                self.add(
                    "transition_order",
                    "P0",
                    subject,
                    "post-transition measurement without a measured baseline",
                )
            if baseline and enacted and baseline > enacted:
                self.add(
                    "transition_order",
                    "P1",
                    subject,
                    "baseline measured after the transition was enacted",
                )
            if enacted and post and enacted > post:
                self.add(
                    "transition_order",
                    "P1",
                    subject,
                    "post-measurement predates enactment",
                )
            if "baseline_value" not in transition or "post_value" not in transition:
                self.unprovable(
                    "transition_order",
                    subject,
                    "the snapshot carries timestamps but not the measured values",
                )

    def programme_baselines(self) -> None:
        present = {
            row.get("kind")
            for row in self.data.msp.get("programme_baselines", [])
            if isinstance(row, dict)
        }
        for kind in self.standard.get("programme", {}).get("required_baselines", []):
            if kind not in present:
                self.add(
                    "programme_baselines",
                    "P1",
                    kind,
                    f"the programme has no {kind} baseline",
                )

    # ── TOGAF ────────────────────────────────────────────────────────────────────────

    def profile_and_board(self) -> None:
        profile = self.data.profile
        if profile is None:
            self.add(
                "profile_adopted",
                "P0",
                self.standard.get("sources", {}).get("profile_key", "profile"),
                "the adopted ADM profile is absent from the snapshot",
            )
        else:
            if profile.get("status") != "adopted":
                self.add(
                    "profile_adopted",
                    "P0",
                    profile.get("key", "profile"),
                    f"profile status is {profile.get('status')!r}, not 'adopted'",
                )
            expected = self.standard.get("sources", {}).get("profile_hash")
            actual = profile_hash(profile)
            if expected and expected != actual:
                self.add(
                    "profile_adopted",
                    "P0",
                    profile.get("key", "profile"),
                    f"profile hash is {actual}, standard pins {expected}",
                )
        boards = [
            row
            for row in self.data.architecture.get("board", [])
            if isinstance(row, dict) and row.get("kind") == "architecture_board"
        ]
        if not boards:
            self.add("board_not_approved", "P1", "board", "no architecture board is recorded")
        elif not any(row.get("status") == "active" for row in boards):
            self.add(
                "board_not_approved",
                "P1",
                "board",
                "the architecture board is not active",
            )

    def contracts(self) -> None:
        for project in self.data.projects:
            key = project.get("key", "?")
            if key == "T0" or not key.startswith("T"):
                continue
            track = (project.get("meta") or {}).get("track")
            if track and track != "v2":
                continue
            if project.get("status") not in {"active", "closed"}:
                continue
            live = [
                row
                for row in self.data.contracts
                if row.get("project_id") == project.get("id") and row.get("status") == "live"
            ]
            if not live:
                self.add(
                    "contract_missing",
                    "P1",
                    key,
                    "delivery project has no live architecture contract",
                )
                continue
            for contract in live:
                if not non_empty(contract.get("approved_at")):
                    self.add(
                        "contract_unapproved",
                        "P1",
                        key,
                        f"live contract {contract.get('kind')} has no approval",
                    )
                if "requested_by" not in contract:
                    self.unprovable(
                        "requester_is_approver",
                        key,
                        "the snapshot does not carry the contract requester",
                    )
                elif (
                    contract.get("requested_by")
                    and contract.get("requested_by") == contract.get("approved_by")
                ):
                    self.add(
                        "requester_is_approver",
                        "P0",
                        key,
                        "the contract's requester approved it",
                    )

    def compliance(self) -> None:
        if "compliance_reviews" not in self.data.architecture:
            return
        for project in self.data.projects:
            key = project.get("key", "?")
            if project.get("status") != "closed":
                continue
            if key == "T0" or not key.startswith("T"):
                continue
            track = (project.get("meta") or {}).get("track")
            if track and track != "v2":
                continue
            contracts = [
                row
                for row in self.data.contracts
                if row.get("project_id") == project.get("id")
                and row.get("status") == "live"
                # A Phase-G compliance review is of the DELIVERY contract. The SoAW is a live
                # contract too, and demanding a review of it red every v2 project whose SoAW
                # has none (measured on T1/T11, 2026-11-26).
                and (
                    str(row.get("kind") or "") == "delivery_contract"
                    or str(row.get("phase_code") or "").lower() == "f"
                )
            ]
            for contract in contracts:
                reviews = [
                    row
                    for row in self.data.reviews_arch
                    if row.get("project_id") == project.get("id")
                    and row.get("contract_id") == contract.get("id")
                ]
                if not reviews:
                    self.add(
                        "compliance_review_missing",
                        "P1",
                        key,
                        "closed delivery project has no Phase-G compliance review",
                    )
                    continue
                for review in reviews:
                    if review.get("level") != "non_conformant":
                        continue
                    active_waiver = any(
                        row.get("review_id") == review.get("id")
                        and row.get("status") == "active"
                        and str(
                            row.get("kind")
                            or row.get("waiver_type")
                            or "architecture_waiver"
                        )
                        == "architecture_waiver"
                        and parse_time(row.get("reassessment_at"))
                        and parse_time(row.get("reassessment_at")) > datetime.now(timezone.utc)
                        for row in self.data.waivers
                    )
                    if not active_waiver:
                        self.add(
                            "compliance_review_missing",
                            "P0",
                            key,
                            "a non-conformant review has no active waiver",
                        )

    def waivers(self) -> None:
        if "waivers" not in self.data.architecture:
            return
        review_ids = {str(row.get("id")) for row in self.data.reviews_arch}
        for waiver in self.data.waivers:
            subject = str(waiver.get("id"))
            kind = str(
                waiver.get("kind") or waiver.get("waiver_type") or "architecture_waiver"
            )
            if kind != "architecture_waiver" or str(
                waiver.get("review_id") or ""
            ) not in review_ids:
                # Only an architecture_waiver — one attached to a compliance review — is a
                # legal TOGAF deviation. A programme waiver is a finding, never a permit.
                self.add(
                    "waiver_not_permitted",
                    "P1",
                    subject,
                    "the waiver is a programme waiver or is not attached to an architecture "
                    "compliance review; programme work permits no waiver",
                )
                continue
            missing = [
                field
                for field in ("owner_user_id", "reason", "reassessment_at")
                if not non_empty(waiver.get(field))
            ]
            if missing:
                self.add(
                    "waiver_invalid",
                    "P1",
                    subject,
                    "architecture waiver is missing " + ", ".join(missing),
                )
                continue
            if waiver.get("status") == "active":
                expires = parse_time(waiver.get("reassessment_at"))
                if expires and expires < datetime.now(timezone.utc):
                    self.add(
                        "waiver_invalid",
                        "P2",
                        subject,
                        "active architecture waiver is past its reassessment date",
                    )

    def capability_artefacts(self) -> None:
        """The Preliminary phase is a capability obligation, checked once, owned by no project."""
        preliminary = {
            row.get("key")
            for row in self.data.architecture.get("standard", [])
            if isinstance(row, dict)
            and row.get("phase_code") == "preliminary"
            and row.get("mandatory", True)
            and row.get("key")
        }
        if not preliminary:
            return
        approved: set[str] = set()
        for artefact in self.data.artefacts:
            if artefact.get("status") != "approved":
                continue
            artifact_type = self.data.artifact_types.get(artefact.get("artifact_type_id"))
            if artifact_type and artifact_type.get("key"):
                approved.add(artifact_type["key"])
            elif artefact.get("artifact_kind"):
                approved.add(artefact["artifact_kind"])
        for key in sorted(preliminary - approved):
            self.add(
                "capability_artefacts",
                "P1",
                key,
                "the Preliminary architecture capability has no approved artefact of this type",
            )

    def architecture_track(self, project: dict) -> bool:
        """The programme (architecture) track, from the live meta — never a guess.

        Mirrors `generate-standards.py`: `meta.adm_profile` wins when it names an architecture
        profile; otherwise the programme seed's `meta.track == "programme"`. The owner's
        tailoring (2026-11-24) gives delivery projects A–D and the Phase-G review; E–H are the
        architecture track's.
        """
        meta = project.get("meta") or {}
        named = str(meta.get("adm_profile") or "").strip().lower()
        if named:
            return "architecture" in named or named in {"programme", "a-h", "ah"}
        return str(meta.get("track") or "").strip().lower() == "programme"

    def artefacts(self) -> None:
        profile = self.data.profile or {}
        requirements = profile.get("artifact_requirements") or {}
        # A project's own mandatory set starts at phase A; Preliminary is the capability check
        # above (§8.1). There is no pre-adoption discount: every live project is held to its set.
        phases = ["a", "b", "c", "d", "e", "f", "g", "h"]
        for project in self.data.projects:
            key = project.get("key", "?")
            if (project.get("meta") or {}).get("viability") in {"fold", "close-by-decision"}:
                continue
            phase, _plateau = self.data.phase(key)
            if phase not in phases:
                continue
            limit = phases.index(phase)
            # Delivery projects carry A–D and the Phase-G review; only the programme
            # (architecture) track carries E–H (owner tailoring 2026-11-24).
            applicable = phases if self.architecture_track(project) else [
                item for item in phases[: limit + 1] if item in {"a", "b", "c", "d"}
            ]
            required: set[str] = set()
            for ph in applicable:
                for plateau, kinds in (requirements.get(ph) or {}).items():
                    if isinstance(kinds, list):
                        required.update(kinds)
            if not required:
                continue
            actual = set()
            for artefact in self.data.artefacts:
                if artefact.get("project_id") != project.get("id"):
                    continue
                if artefact.get("status") != "approved":
                    continue
                artifact_type = self.data.artifact_types.get(artefact.get("artifact_type_id"))
                if artifact_type and artifact_type.get("key"):
                    actual.add(artifact_type["key"])
                elif artefact.get("artifact_kind"):
                    actual.add(artefact["artifact_kind"])
            missing = sorted(required - actual)
            if not missing:
                continue
            self.add(
                "mandatory_artefacts",
                "P1",
                key,
                "missing approved artefacts: " + ", ".join(missing),
            )

    # ── Cross-source ─────────────────────────────────────────────────────────────────

    def drift(self) -> None:
        missing_meta = []
        for project in self.data.projects:
            key = project.get("key", "?")
            seed = self.data.seed_by_key.get(key)
            pinned = acceptance_status(key)
            if (
                seed
                and seed.get("status") != project.get("status")
                and not (pinned and pinned == project.get("status"))
            ):
                severity = (
                    "P1"
                    if seed.get("status") == "closed" and project.get("status") != "closed"
                    else "P2"
                )
                self.add(
                    "seed_live_drift",
                    severity,
                    key,
                    f"seed status {seed.get('status')!r} != live status {project.get('status')!r}",
                )
            coverage = self.data.coverage_by_key.get(key)
            if coverage:
                measured = coverage.get("status")
                live = project.get("status")
                if measured == "proposed" and live in {"active", "closed"}:
                    self.add(
                        "status_source_drift",
                        "P2",
                        key,
                        f"coverage says {measured!r}, the register says {live!r}",
                    )
                elif measured == "built" and live == "draft":
                    self.add(
                        "status_source_drift",
                        "P2",
                        key,
                        "coverage says built, the register says draft",
                    )
            phase, _ = self.data.phase(key)
            if phase and not project.get("meta", {}).get("adm_phase"):
                missing_meta.append(key)
        if missing_meta:
            self.unprovable(
                "seed_live_drift",
                "live project meta",
                "adm_phase is absent from the live meta for " + ", ".join(sorted(missing_meta)),
            )

    def derivation(self) -> None:
        issues = self.data.issues.get("issues", self.data.issues)
        if not isinstance(issues, list):
            self.unprovable("derivation_missing", "#435", "the issue registry is not readable")
            return
        issue = next(
            (row for row in issues if isinstance(row, dict) and row.get("number") == 435),
            None,
        )
        if issue is None:
            self.unprovable("derivation_missing", "#435", "issue #435 is absent from the registry")
            return
        body = issue.get("body") or ""
        if not isinstance(body, str) or not body.strip():
            # The public issue registry redacts bodies by construction (`fetch-issues.sh` never
            # writes them), so this claim has no subject an offline instrument can read. The
            # audit records the limitation instead of manufacturing a P2 no record can discharge.
            # A registry that carries the body (a private export) still takes the check below.
            return
        for reference in sorted(set(SCRIPT_REF.findall(body))):
            if not (self.data.repo_root / reference).exists():
                self.add(
                    "derivation_missing",
                    "P1",
                    "#435",
                    f"the tracking issue names a script that does not exist: {reference}",
                )


def load_dataset(
    snapshot: Path,
    seed_dir: Path,
    standard: dict[str, Any],
    repo_root: Path,
    outputs_path: Path | None,
) -> Dataset:
    data_dir = snapshot
    programme_doc = read_json(data_dir / "programme.json")
    # The snapshot embeds the issue registry in programme.json; a publish also writes it as
    # its own file. Read whichever the snapshot carries, so a rule about an issue does not
    # report "the registry is not readable" on every live snapshot.
    issues_doc = (
        read_json(data_dir / "issues.json")
        if (data_dir / "issues.json").exists()
        else programme_doc.get("issue_registry") or {}
    )
    return Dataset(
        programme=programme_doc,
        msp=read_json(data_dir / "msp.json"),
        architecture=read_json(data_dir / "architecture.json"),
        manifest=read_json(data_dir / "manifest.json") if (data_dir / "manifest.json").exists() else {},
        outputs=read_outputs(outputs_path or data_dir.parent / "docs" / "outputs.js"),
        coverage=read_json(data_dir / "coverage.json") if (data_dir / "coverage.json").exists() else {},
        issues=issues_doc,
        seed=read_json(seed_dir / "programme.json"),
        definition=read_json(seed_dir / "programme-definition.json"),
        output_standard=read_json(
            repo_root / standard.get("sources", {}).get("output_standard", "seed/output-standard.json")
        ),
        standard=standard,
        repo_root=repo_root,
    )


def render_markdown(
    gaps: list[dict[str, Any]], dataset: Dataset, snapshot_label: str
) -> str:
    counts = {severity: 0 for severity in SEVERITY_ORDER}
    for gap in gaps:
        counts[gap["severity"]] = counts.get(gap["severity"], 0) + 1
    manifest = dataset.manifest
    lines = [
        "# MSP/TOGAF conformance gap register",
        "",
        f"Snapshot: `{manifest.get('git_sha', 'unknown')}` generated `{manifest.get('generated_at', 'unknown')}` "
        f"from `{snapshot_label}`.",
        "",
        "| Severity | Count |",
        "|---|---:|",
    ]
    for severity in SEVERITY_ORDER:
        lines.append(f"| {severity} | {counts.get(severity, 0)} |")
    lines.extend(["", "## P0 and P1", "", "| Rule | Subject | Severity | Detail |", "|---|---|---|---|"])
    important = [gap for gap in gaps if gap["severity"] in {"P0", "P1"}]
    if not important:
        lines.append("| — | — | — | none |")
    for gap in important:
        detail = gap["detail"].replace("|", "\\|")
        if len(detail) > 300:
            detail = detail[:297] + "…"
        lines.append(
            f"| `{gap['rule']}` | `{gap['subject']}` | {gap['severity']} | {detail} |"
        )
    if dataset.excluded_test_projects:
        lines.extend(
            [
                "",
                "## Excluded test rows",
                "",
                "These rows are test residue, not programme projects. They are excluded by the",
                "predicate in `test_row_reason` and are listed here so the exclusion is visible.",
                "",
                "| Key | Reason |",
                "|---|---|",
            ]
        )
        for row in dataset.excluded_test_projects:
            lines.append(f"| `{row.get('key')}` | {row.get('reason')} |")
    lines.extend(
        [
            "",
            "## Projects",
            "",
            "| Project | Status | Tranche | P0 | P1 | P2 | P3 |",
            "|---|---|---:|---:|---:|---:|---:|",
        ]
    )
    by_project: dict[str, dict[str, int]] = {}
    for gap in gaps:
        subject = str(gap["subject"]).split(":")[0]
        by_project.setdefault(subject, {})
        by_project[subject][gap["severity"]] = by_project[subject].get(gap["severity"], 0) + 1
    for key in sorted(dataset.by_key):
        project = dataset.by_key[key]
        row = by_project.get(key, {})
        lines.append(
            f"| {key} | {project.get('status', '')} | {project.get('meta', {}).get('tranche', '')} | "
            f"{row.get('P0', 0)} | {row.get('P1', 0)} | {row.get('P2', 0)} | {row.get('P3', 0)} |"
        )
    lines.append("")
    return "\n".join(lines)


def gap_key(gap: dict[str, Any]) -> tuple[str, str]:
    return (str(gap["rule"]), str(gap["subject"]))


DISPOSITIONS = ("rework", "fold", "waiver", "correct-the-claim", "unprovable")


def baseline_disposition(gap: dict[str, Any]) -> str:
    """The ledger's one disposition vocabulary; the auditor's own unprovable verdict wins."""
    if gap.get("disposition") == "unprovable":
        return "unprovable"
    if gap["rule"] == "derivation_missing":
        return "correct-the-claim"
    if str(gap["subject"]).split(":")[0] == "S06a":
        return "fold"
    return "rework"


def baseline_entries(gaps: list[dict[str, Any]], standard: dict[str, Any]) -> dict[str, Any]:
    expires = "2027-02-18"
    entries = [
        {
            "rule": gap["rule"],
            "subject": gap["subject"],
            "severity": gap["severity"],
            "disposition": baseline_disposition(gap),
            "owner": "programme owner",
            "expires": expires,
            "reason": "proposed at the U1 baseline; accepted or amended in the authority-repair unit",
        }
        for gap in gaps
        if gap["severity"] in {"P0", "P1"}
    ]
    entries.sort(key=lambda row: (SEVERITY_ORDER[row["severity"]], row["rule"], row["subject"]))
    return {
        "version": 1,
        "standard": "governance/conformance.json",
        "entries": entries,
    }


def accepted_baseline(
    gaps: list[dict[str, Any]],
    standard: dict[str, Any],
    dataset: Dataset,
    snapshot_dir: Path,
    *,
    accepted_by: str,
    approver: str,
    acceptance_ref: str,
    delegation_ref: str,
    accepted_at: str | None = None,
) -> dict[str, Any]:
    """A proposal plus the record of who accepted it, and of what snapshot.

    The strict gate refuses a ledger without this header, so an accepted ledger can only be
    produced by naming the decision that accepted it and the bytes it was taken from.  A
    proposal is produced by ``baseline_entries`` and must never be copied over the accepted
    file: that is how a report silently becomes an acceptance.
    """
    document = baseline_entries(gaps, standard)
    manifest_path = Path(snapshot_dir) / "manifest.json"
    if not manifest_path.exists():
        fail(f"cannot accept a baseline: no manifest at {manifest_path}")
    document["accepted_at"] = accepted_at or datetime.now(timezone.utc).isoformat()
    document["accepted_by"] = accepted_by
    document["approver"] = approver
    document["acceptance_ref"] = acceptance_ref
    document["delegation_ref"] = delegation_ref
    document["snapshot"] = {
        "git_sha": dataset.manifest.get("git_sha"),
        "generated_at": dataset.manifest.get("generated_at"),
        "manifest_sha256": hashlib.sha256(manifest_path.read_bytes()).hexdigest(),
        "outputs": len(dataset.outputs.get("outputs") or []),
    }
    return document


def check_strict(
    gaps: list[dict[str, Any]], baseline: dict[str, Any], fail_on: set[str]
) -> list[str]:
    failures: list[str] = []
    # The header is the acceptance: a baseline without one is a list, not an accepted record.
    for field in (
        "version", "accepted_at", "accepted_by", "approver", "acceptance_ref", "delegation_ref",
    ):
        if not baseline.get(field):
            failures.append(f"baseline header is missing {field}")
    snapshot = baseline.get("snapshot") or {}
    for field in ("git_sha", "generated_at", "manifest_sha256", "outputs"):
        if not snapshot.get(field):
            failures.append(f"baseline snapshot is missing {field}")
    current = {gap_key(gap): gap for gap in gaps}
    recorded: dict[tuple[str, str], dict[str, Any]] = {}
    today = datetime.now(timezone.utc).date()
    for entry in baseline.get("entries", []):
        if not isinstance(entry, dict):
            continue
        key = (str(entry.get("rule")), str(entry.get("subject")))
        recorded[key] = entry
        name = f"{entry.get('rule')} {entry.get('subject')}"
        if not str(entry.get("owner") or "").strip():
            failures.append(f"baseline entry has no owner: {name}")
        if entry.get("disposition") not in DISPOSITIONS:
            failures.append(
                f"baseline entry has no valid disposition: {name} ({entry.get('disposition')!r})"
            )
        expires = entry.get("expires")
        try:
            expired = datetime.fromisoformat(expires).date() < today
        except (TypeError, ValueError):
            expired = True
        if expired:
            failures.append(f"baseline expired: {name} (expires {expires!r})")
    for key, gap in current.items():
        if gap["severity"] not in fail_on:
            continue
        entry = recorded.get(key)
        if entry is None:
            failures.append(
                f"new {gap['severity']}: {gap['rule']} {gap['subject']} — {gap['detail']}"
            )
            continue
        if SEVERITY_ORDER[gap["severity"]] < SEVERITY_ORDER.get(entry.get("severity"), 9):
            failures.append(
                f"worsened to {gap['severity']}: {gap['rule']} {gap['subject']} — {gap['detail']}"
            )
    for key, entry in recorded.items():
        if key not in current:
            failures.append(
                f"ledger grew: {entry.get('rule')} {entry.get('subject')} has no live gap"
            )
    return failures


def standard_checks(standard: dict[str, Any], seed_dir: Path) -> None:
    if standard.get("version") != 1:
        fail("standard version must be 1")
    if "projects" in standard:
        fail("the standard must not duplicate per-project obligations; it is a rule matrix")
    rules = standard.get("rules")
    if not isinstance(rules, dict) or not rules:
        fail("standard.rules must be a non-empty object")
    known = {
        "snapshot_schema_stale",
        "programme_outputs",
        "tranche_plan",
        "tranche_review_without_authorisation",
        "project_in_unauthorised_tranche",
        "closed_without_closure_review",
        "unpublished_declared_output",
        "benefit_profile_incomplete",
        "benefit_baseline_placeholder",
        "realized_without_transition",
        "transition_order",
        "programme_baselines",
        "profile_adopted",
        "board_not_approved",
        "contract_missing",
        "contract_unapproved",
        "requester_is_approver",
        "compliance_review_missing",
        "waiver_invalid",
        "waiver_not_permitted",
        "capability_artefacts",
        "mandatory_artefacts",
        "closed_without_closure_review",
        "closure_outputs_undeclared",
        "seed_live_drift",
        "derivation_missing",
        "status_source_drift",
        "dependency_cycle",
        "dependency_unknown_target",
        "dependency_untyped",
        "cadence_absent",
        "wbs_absent",
        "pi_plan_missing",
        "pi_cadence_missing",
        "pi_objectives_missing",
        "demo_credit_missing",
        "retro_lesson_missing",
        "wsjf_inputs_missing",
        "backlog_refinement_missing",
    }
    unknown = sorted(set(rules) - known)
    missing = sorted(known - set(rules))
    if unknown:
        fail(f"unknown rules: {unknown}")
    if missing:
        fail(f"rules missing from the matrix: {missing}")
    for rule, value in rules.items():
        row = value if isinstance(value, dict) else {"severity": value}
        severity = row.get("severity")
        if severity not in SEVERITY_ORDER:
            fail(f"rule {rule} has unknown severity {severity!r}")
        if isinstance(value, dict):
            # A rule is a governance record, not only a severity: it names the standard it
            # enforces, the SPEC clause, the artefact the rework produces, and its DoD.
            if row.get("standard") not in REGISTER_STANDARDS:
                fail(f"rule {rule} has unknown standard {row.get('standard')!r}")
            if not isinstance(row.get("spec"), str) or not row["spec"].strip():
                fail(f"rule {rule} must name a spec clause")
            if not isinstance(row.get("artefact"), str) or not row["artefact"].strip():
                fail(f"rule {rule} must name an artefact")
            if row.get("scope") not in REGISTER_SCOPES:
                fail(f"rule {rule} has unknown scope {row.get('scope')!r}")
            if "owner_project" not in row:
                fail(f"rule {rule} must declare owner_project (a project key or null)")
            rework = row.get("rework")
            if not isinstance(rework, dict):
                fail(f"rule {rule} must carry a rework object")
            if not isinstance(rework.get("title"), str) or not rework["title"].strip():
                fail(f"rule {rule} must carry rework.title")
            dod = rework.get("dod")
            if not isinstance(dod, list) or not dod or not all(
                isinstance(item, str) and item.strip() for item in dod
            ):
                fail(f"rule {rule} must carry a non-empty rework.dod list of strings")
    sources = standard.get("sources", {})
    for field in ("output_standard", "programme_seed", "programme_definition"):
        path = sources.get(field)
        if not path or not (ROOT / path).exists():
            fail(f"source {field} does not resolve: {path!r}")
    expected = sources.get("profile_hash", "")
    if not expected.startswith("sha256:") or len(expected) != 71:
        fail("sources.profile_hash must be sha256:<64 hex>")
    output_standard = read_json(ROOT / sources["output_standard"])
    p0 = next(
        (cls for cls in output_standard.get("classes", []) if cls.get("name") == "p0"), None
    )
    if p0 is None or len(p0.get("now", [])) != 21:
        fail("seed/output-standard.json must define the p0 class with 21 categories")
    seed = read_json(ROOT / sources["programme_seed"])
    definition = read_json(ROOT / sources["programme_definition"])
    seed_keys = {row.get("key") for row in seed.get("projects", [])}
    definition_keys = set(definition.get("projects", {}))
    if seed_keys != definition_keys:
        fail("seed/programme.json and seed/programme-definition.json disagree on project keys")


# ── Rework register ─────────────────────────────────────────────────────────────────


def rule_row(standard: dict[str, Any], rule: str) -> dict[str, Any]:
    """A rule's full record. The pre-P7 string form is read as severity-only."""
    value = standard.get("rules", {}).get(rule, "P2")
    if isinstance(value, dict):
        return value
    return {"severity": str(value)}


def _scope_of(gap: dict[str, Any], row: dict[str, Any], dataset: Dataset) -> str:
    """The package scope a gap belongs to: a project key, `tranche-<n>`, or `programme`."""
    subject = str(gap.get("subject", ""))
    head = subject.split(":", 1)[0]
    if head in dataset.by_key:
        # A gap about a specific project belongs to that project, whatever the rule's
        # artefact scope: only the project (or its acting owner) can close its own missing
        # record. Grouping a project's gap under its tranche hands the card to whichever
        # member is alphabetically first and makes it unfixable by that member. The rule's
        # `scope: tranche` still governs the standard's applicability and the roll-up.
        return head
    scope = row.get("scope", "programme")
    if scope == "tranche":
        match = re.match(r"tranche\s+(\d+)", subject)
        if match:
            return f"tranche-{match.group(1)}"
        return "programme"
    return "programme"


def _snapshot_file(row: dict[str, Any]) -> str:
    """The published section an artefact is read from, for the package's evidence links."""
    artefact = str(row.get("artefact", ""))
    if row.get("standard") == "TOGAF":
        return "architecture.json"
    if artefact == "wbs_node":
        return "work.json"
    if artefact == "programme_cadence":
        return "msp.json"
    if artefact.startswith("benefit"):
        return "msp.json"
    if artefact in {"tranche_plan", "tranche_review", "programme_baseline"}:
        return "msp.json"
    return "programme.json"


def _owner_of(
    row: dict[str, Any], scope: str, dataset: Dataset, *, finding: bool
) -> tuple[str, str]:
    """(apply_to, owner_project): where the card is created, and the project that owns the fix."""
    hint = row.get("owner_project")
    if finding:
        owner = dataset.by_key.get(hint) if hint else None
        if owner is not None and owner.get("status") == "active":
            return str(hint), str(hint)
        return "PLATFORM", str(hint or "PLATFORM")
    if row.get("scope") == "project" and scope in dataset.by_key:
        project = dataset.by_key[scope]
        # A closed or archived project's board is history: new work must not be written into it.
        if project.get("status") == "active":
            return scope, scope
        return "PLATFORM", scope
    if row.get("scope") == "tranche" and scope.startswith("tranche-"):
        number = scope.split("-", 1)[1]
        members = sorted(
            str(project.get("key"))
            for project in dataset.projects
            if str((project.get("meta") or {}).get("tranche")) == number
            and project.get("key")
            and project.get("status") == "active"
        )
        if members:
            return members[0], str(hint or members[0])
    return "PLATFORM", str(hint or "PLATFORM")


def _subject_record(
    gap: dict[str, Any], entry: dict[str, Any] | None
) -> dict[str, Any]:
    disposition = (entry or {}).get("disposition") or gap.get("disposition") or "rework"
    evidence: list[Any] = []
    raw = gap.get("evidence")
    if raw:
        evidence.append({"source": "audit", "rule": gap["rule"], "evidence": raw})
    return {
        "subject": str(gap.get("subject", "")),
        "detail": str(gap.get("detail", "")),
        "severity": str(gap.get("severity", "P2")),
        "disposition": str(disposition),
        "owner": (entry or {}).get("owner"),
        "expires": (entry or {}).get("expires"),
        "reason": (entry or {}).get("reason"),
        "evidence": evidence,
    }


# A deep-review finding names the artefact it reviewed, not a ledger id: the converter prefixes
# its key, maps its three kinds onto the register's package kinds, and takes the section from
# the review. An unknown kind is refused rather than guessed.
DEEP_REVIEW_KINDS = {
    "code": "CODE",
    "record+code": "CODE",
    "record": "RECORD",
    "standard": "STANDARD",
}
# `record+code` names a finding whose code capability may already exist. At this head
# `programmes.update` accepts `shared_config` and `programme_lessons.propose`/`retros.*` exist, so the
# blocker is the record write, not new code. The override is per finding and evidence-backed;
# it never widens an unknown kind.
DEEP_REVIEW_KIND_OVERRIDES = {
    "w3-shared-config-1": "RECORD",
    "w3-lessons-1": "RECORD",
}
DEEP_REVIEW_SPEC = {
    "w3-action-registry-projection-1": "§8.5",
    "w3-licensing-frame-1": "§8.5",
    "w3-verdict-t18-1": "§12",
}


def deep_review_findings(path: Path, output_standard: dict[str, Any]) -> list[dict[str, Any]]:
    """Convert a deep-review sidecar into the findings-ledger shape the register reads.

    The sidecar's `evidence` is one prose paragraph, so it becomes both the ledger `problem`
    and the single evidence quote. `standard` comes from the artefact's template in
    `seed/output-standard.json`; an artefact with no template is SPEC. The section derives
    from the template's standard (MSP → §8.3), except where the review names it, and is §8
    otherwise.
    """
    document = read_json(path)
    findings = document.get("findings")
    if not isinstance(findings, list):
        fail(f"the deep-review sidecar {path} has no findings list")
    try:
        evidence_path = path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        evidence_path = str(path)
    converted: list[dict[str, Any]] = []
    for finding in findings:
        if not isinstance(finding, dict):
            fail(f"the deep-review sidecar {path} contains a non-object finding")
        key = str(finding.get("key") or "").strip()
        if not key:
            fail(f"the deep-review sidecar {path} contains a finding with no key")
        raw_kind = str(finding.get("kind") or "").strip()
        kind = DEEP_REVIEW_KINDS.get(raw_kind)
        if kind is None:
            fail(
                f"the deep-review finding {key!r} has unknown kind {raw_kind!r}; "
                f"expected one of {', '.join(sorted(DEEP_REVIEW_KINDS))}"
            )
        kind = DEEP_REVIEW_KIND_OVERRIDES.get(key, kind)
        _, _, template_key = str(finding.get("artifact") or "").partition("#")
        template = (output_standard.get("templates") or {}).get(template_key) or {}
        standard = str(template.get("standard") or "SPEC")
        spec = DEEP_REVIEW_SPEC.get(key) or ("§8.3" if standard == "MSP" else "§8")
        evidence = str(finding.get("evidence") or "")
        converted_finding = (
            {
                "id": f"dr-{key}",
                "standard": standard,
                "spec": spec,
                "severity": finding.get("severity"),
                "title": finding.get("title"),
                "problem": evidence,
                "evidence": [
                    {"source": "deep-review", "path": evidence_path, "quote": evidence}
                ],
                "acceptance": finding.get("acceptance"),
                "owner_project": finding.get("owner_project"),
                "kind": kind,
                # A deep-review finding closes by the same rule as a ledger finding: the
                # sidecar names the commit, the test and the evidence, and the register
                # counts it resolved instead of emitting a package nobody can discharge.
                "resolved_by": finding.get("resolved_by"),
            }
        )
        artifact = str(finding.get("artifact") or "")
        if (
            kind == "STANDARD"
            and artifact.startswith(CODE_OWNED_ARTIFACT_PREFIXES)
            and not finding.get("resolved_by")
        ):
            acceptance_test = str(finding.get("acceptance_test") or "").strip()
            if not acceptance_test.startswith(EXECUTABLE_TEST_PREFIXES):
                fail(
                    f"the code-owned standard finding {key!r} must declare an executable "
                    f"acceptance_test; a record-only repair cannot close it"
                )
            sibling = dict(converted_finding)
            sibling["id"] = f"dr-{key}-code"
            sibling["kind"] = "CODE"
            sibling["title"] = f"code: {converted_finding['title']}"
            sibling["problem"] = (
                f"the code artefact {artifact} must carry the fix, not only the record: {evidence}"
            )
            sibling["acceptance"] = acceptance_test
            sibling["acceptance_test"] = acceptance_test
            sibling["depends_on"] = []
            converted_finding["depends_on"] = [sibling["id"]]
            converted.append(converted_finding)
            converted.append(sibling)
        else:
            converted.append(converted_finding)
    return converted


def build_register(
    gaps: list[dict[str, Any]],
    baseline: dict[str, Any],
    findings_doc: dict[str, Any],
    standard: dict[str, Any],
    dataset: Dataset,
    snapshot_label: str = "docs/design/data",
    findings_paths: list[str] | None = None,
    deep_review_paths: list[str] | None = None,
) -> dict[str, Any]:
    """Project the gap register into labelled rework packages, deterministically.

    P0 is never grouped (one package per rule and subject). P1–P3 are grouped by the rule's
    scope (project / tranche / programme) and split at [`REGISTER_MAX_SUBJECTS`]. No
    disposition suppresses a package: every open row computes as an open gap (ADR 0012). A
    checked-in v1 finding is dropped only when it names the data gap it duplicates with
    `duplicate_of`.
    """
    entries = {
        (str(entry.get("rule")), str(entry.get("subject"))): entry
        for entry in baseline.get("entries", [])
        if isinstance(entry, dict)
    }
    groups: dict[str, dict[str, Any]] = {}

    def add(key: str, rule: str, row: dict[str, Any], scope: str, gap: dict[str, Any], finding: bool) -> None:
        group = groups.setdefault(
            key,
            {"key": key, "rule": rule, "row": row, "scope": scope, "gaps": [], "finding": finding},
        )
        group["gaps"].append(gap)

    data_keys: set[str] = set()
    for gap in gaps:
        data_keys.add(f"{gap['rule']}:{gap['subject']}")
        row = rule_row(standard, str(gap["rule"]))
        scope = _scope_of(gap, row, dataset)
        # The package is the remediation unit, not the individual row: a project-scoped rule
        # is naturally one package per project; a tranche rule is one package per tranche.
        add(f"{gap['rule']}:{scope}", str(gap["rule"]), row, scope, gap, False)

    duplicates: list[str] = []
    resolved_findings: list[str] = []
    reported: set[str] = set()
    for finding in findings_doc.get("findings", []):
        if not isinstance(finding, dict):
            fail("the findings ledger contains a non-object finding")
        for field in (
            "id",
            "standard",
            "spec",
            "severity",
            "title",
            "problem",
            "evidence",
            "acceptance",
            "owner_project",
        ):
            if not finding.get(field):
                fail(f"the finding {finding.get('id', '?')!r} has no {field}")
        identifier = str(finding["id"])
        if identifier in reported:
            fail(f"the findings ledger repeats {identifier}")
        reported.add(identifier)
        if finding.get("resolved_by"):
            resolved_findings.append(identifier)
            continue
        duplicate_of = finding.get("duplicate_of")
        if duplicate_of and str(duplicate_of) in data_keys:
            duplicates.append(identifier)
            continue
        # The residual-waiver legality check survives: a waiver names who accepted it, the
        # decision that records it, why, and when it must be reassessed, and anything less is
        # refused. A legal programme waiver is still not a disposition (ADR 0012): the finding
        # stays open and the waiver itself becomes a `waiver_not_permitted` finding.
        disposition = "rework"
        waiver = finding.get("waiver")
        if waiver is not None:
            if not isinstance(waiver, dict):
                fail(f"the finding {identifier} has a non-object waiver")
            missing = [
                field
                for field in ("accepted_by", "accepted_at", "decision_ref", "expires", "reason")
                if not str(waiver.get(field) or "").strip()
            ]
            if missing:
                fail(f"the finding {identifier} waiver is missing " + ", ".join(missing))
            try:
                expires = datetime.fromisoformat(
                    str(waiver["expires"]).replace("Z", "+00:00")
                ).date()
            except ValueError:
                fail(
                    f"the finding {identifier} waiver expires is not a date: "
                    f"{waiver.get('expires')!r}"
                )
            if expires < datetime.now(timezone.utc).date():
                fail(f"the finding {identifier} waiver expired at {waiver['expires']}")
        owner = str(finding["owner_project"])
        scope = owner if owner in dataset.by_key else "programme"
        if waiver is not None:
            add(
                f"waiver_not_permitted:{scope}",
                "waiver_not_permitted",
                rule_row(standard, "waiver_not_permitted"),
                scope,
                {
                    "rule": "waiver_not_permitted",
                    "severity": str(finding["severity"]),
                    "subject": identifier,
                    "detail": (
                        f"finding {identifier} carries an owner-accepted waiver; "
                        "programme work permits no waiver"
                    ),
                    "disposition": "rework",
                    "evidence": [
                        {"source": "code", **item}
                        for item in finding["evidence"]
                        if isinstance(item, dict)
                    ],
                },
                False,
            )
        row = {
            "severity": str(finding["severity"]),
            "standard": str(finding["standard"]),
            "spec": str(finding["spec"]),
            "artefact": str(finding.get("area", "code")),
            # R11's package kind. A ledger finding is a product defect (CODE); an artefact
            # finding states its own cause (STANDARD when the template cannot resolve the
            # section, RECORD when the rows or documents are missing). A finding written by an
            # older generator carries no kind and reads as RECORD.
            "kind": str(
                finding.get("kind")
                or ("CODE" if identifier.startswith("v1-") else "RECORD")
            ),
            "depends_on": list(finding.get("depends_on") or []),
            "acceptance_test": finding.get("acceptance_test"),
            "scope": "project" if owner in dataset.by_key else "programme",
            "owner_project": owner,
            "rework": {
                "title": str(finding["title"]),
                "dod": [str(finding["acceptance"])],
            },
        }
        gap = {
            "rule": identifier,
            "severity": str(finding["severity"]),
            "subject": owner,
            "detail": str(finding["problem"]),
            "disposition": disposition,
            "evidence": [
                {"source": "code", **item}
                for item in finding["evidence"]
                if isinstance(item, dict)
            ],
        }
        add(f"{identifier}:{scope}", identifier, row, scope, gap, True)

    packages: list[dict[str, Any]] = []
    ordered = sorted(
        groups.values(),
        key=lambda group: (
            SEVERITY_ORDER.get(str(group["gaps"][0].get("severity")), 9),
            str(group["rule"]),
            str(group["key"]),
        ),
    )
    for group in ordered:
        row = group["row"]
        rule = group["rule"]
        records = [
            _subject_record(gap, entries.get((rule, str(gap["subject"]))))
            for gap in sorted(
                group["gaps"],
                key=lambda gap: (SEVERITY_ORDER.get(str(gap.get("severity")), 9), str(gap.get("subject"))),
            )
        ]
        severity = min(
            (record["severity"] for record in records),
            key=lambda value: SEVERITY_ORDER.get(value, 9),
        )
        chunks = (
            [records]
            if severity == "P0"
            else [
                records[index : index + REGISTER_MAX_SUBJECTS]
                for index in range(0, len(records), REGISTER_MAX_SUBJECTS)
            ]
        )
        for position, chunk in enumerate(chunks):
            key = group["key"] if len(chunks) == 1 else f"{group['key']}#{position + 1}"
            dispositions = {record["disposition"] for record in chunk}
            status = "suppressed" if dispositions <= set(REGISTER_SUPPRESSED) else "open"
            disposition = next(iter(dispositions)) if len(dispositions) == 1 else "mixed"
            apply_to, owner = _owner_of(row, group["scope"], dataset, finding=group["finding"])
            evidence: list[Any] = []
            for record in chunk:
                evidence.extend(record["evidence"])
            if not evidence:
                evidence.append(
                    {
                        "source": "snapshot",
                        "file": _snapshot_file(row),
                        "rule": rule,
                        "subjects": [record["subject"] for record in chunk],
                    }
                )
            owners = sorted({str(record["owner"]) for record in chunk if record.get("owner")})
            expiries = sorted({str(record["expires"]) for record in chunk if record.get("expires")})
            dod = row.get("rework", {}).get("dod") or [
                "The next audit reports no gap for this rule and subject"
            ]
            packages.append(
                {
                    "key": key,
                    "rule": rule,
                    "kind": str(row.get("kind") or "RECORD"),
                    "standard": str(row.get("standard", "SPEC")),
                    "spec": str(row.get("spec", "")),
                    "artefact": str(row.get("artefact", "")),
                    "title": str(row.get("rework", {}).get("title", rule)),
                    "severity": severity,
                    "disposition": disposition,
                    "status": status,
                    "scope": group["scope"],
                    "subjects": chunk,
                    "subject_count": len(chunk),
                    "evidence": evidence[:REGISTER_MAX_SUBJECTS],
                    "dod": [{"item": str(item), "met": False} for item in dod],
                    "owner_project": owner,
                    "apply_to": apply_to,
                    "depends_on": list(row.get("depends_on") or []),
                    "acceptance_command": register_acceptance(
                        key,
                        snapshot_label,
                        findings_paths,
                        deep_review_paths,
                        row.get("acceptance_test"),
                    ),
                    "baseline": None
                    if group["finding"]
                    else {
                        "disposition": disposition,
                        "entries": len(chunk),
                        "owners": owners,
                        "earliest_expires": expiries[0] if expiries else None,
                    },
                }
            )

    packages.sort(
        key=lambda package: (
            SEVERITY_ORDER.get(package["severity"], 9),
            package["standard"],
            package["key"],
        )
    )
    return {
        "schema": REGISTER_SCHEMA,
        "snapshot": str(snapshot_label),
        "generated_at": dataset.manifest.get("generated_at"),
        "git_sha": dataset.manifest.get("git_sha"),
        "scorecard": _scorecard(gaps, packages, standard, entries),
        "summary": _summary(packages, duplicates, resolved_findings),
        "packages": packages,
    }


def _scorecard(
    gaps: list[dict[str, Any]],
    packages: list[dict[str, Any]],
    standard: dict[str, Any],
    entries: dict[tuple[str, str], dict[str, Any]],
) -> dict[str, Any]:
    """Per-standard counts whose denominators come from the standard, never from the view."""
    card: dict[str, Any] = {}
    for name in REGISTER_STANDARDS:
        rules = [
            rule
            for rule in standard.get("rules", {})
            if rule_row(standard, rule).get("standard") == name
        ]
        with_gaps = {
            gap["rule"]
            for gap in gaps
            if gap["rule"] in rules and rule_row(standard, str(gap["rule"])).get("standard") == name
        }
        matching = [
            gap for gap in gaps if rule_row(standard, str(gap["rule"])).get("standard") == name
        ]
        suppressed = sum(
            1
            for gap in matching
            if (entries.get((str(gap["rule"]), str(gap["subject"]))) or {}).get("disposition")
            in REGISTER_SUPPRESSED
        )
        card[name] = {
            "rules": len(rules),
            "rules_with_gaps": len(with_gaps),
            "rules_clean": len(rules) - len(with_gaps),
            "gaps": len(matching),
            "p0": sum(1 for gap in matching if gap.get("severity") == "P0"),
            "p1": sum(1 for gap in matching if gap.get("severity") == "P1"),
            "p2": sum(1 for gap in matching if gap.get("severity") == "P2"),
            "suppressed": suppressed,
            "packages": sum(1 for package in packages if package["standard"] == name),
            "findings": sum(
                1
                for package in packages
                if package["standard"] == name and package["rule"].startswith("v1-")
            ),
        }
    return card


def _summary(
    packages: list[dict[str, Any]],
    duplicates: list[str],
    resolved_findings: list[str],
) -> dict[str, Any]:
    by_standard: dict[str, int] = {}
    by_severity: dict[str, int] = {}
    by_status: dict[str, int] = {}
    by_kind: dict[str, int] = {}
    for package in packages:
        by_standard[package["standard"]] = by_standard.get(package["standard"], 0) + 1
        by_severity[package["severity"]] = by_severity.get(package["severity"], 0) + 1
        by_status[package["status"]] = by_status.get(package["status"], 0) + 1
        kind = str(package.get("kind") or "RECORD")
        by_kind[kind] = by_kind.get(kind, 0) + 1
    return {
        "packages": len(packages),
        "open": by_status.get("open", 0),
        "suppressed": by_status.get("suppressed", 0),
        "subjects": sum(package["subject_count"] for package in packages),
        "duplicates": len(duplicates),
        "resolved": len(resolved_findings),
        "by_standard": dict(sorted(by_standard.items())),
        "by_severity": {key: by_severity[key] for key in sorted(by_severity)},
        "by_status": dict(sorted(by_status.items())),
        "by_kind": dict(sorted(by_kind.items())),
    }


# ── Self-test ────────────────────────────────────────────────────────────────────────


def self_test() -> None:
    standard = read_json(DEFAULT_STANDARD)
    standard_checks(standard, ROOT / "seed")
    output_standard = read_json(ROOT / standard["sources"]["output_standard"])

    def base() -> Dataset:
        profile = {
            "key": standard["sources"]["profile_key"],
            "status": "adopted",
            "phase_set": ["preliminary", "a", "b", "c", "d", "e", "f", "g", "h"],
            "artifact_requirements": {
                "preliminary": {"psa": ["principles_catalog"]},
                "a": {"psa": ["solution_concept"]},
            },
            "contract_kinds": {"a": "statement_of_architecture_work"},
        }
        local_standard = json.loads(json.dumps(standard))
        local_standard["sources"]["profile_hash"] = profile_hash(profile)
        p0 = next(cls for cls in output_standard["classes"] if cls.get("name") == "p0")
        projects = [
            {
                "id": "p0",
                "key": "P0",
                "status": "closed",
                "created_at": "2026-11-10T00:00:00+00:00",
                "meta": {
                    "track": "programme",
                    "tranche": 0,
                    "adm_phase": "preliminary",
                    "plateau": "psa",
                    "expected_outputs": p0["now"],
                    "closure_outputs": ["closure-review"],
                    "measured_status": "built",
                },
            },
            {
                "id": "t1",
                "key": "T1",
                "status": "active",
                "created_at": "2026-11-20T00:00:00+00:00",
                "meta": {
                    "track": "v2",
                    "tranche": 1,
                    "adm_phase": "a",
                    "plateau": "psa",
                    "expected_outputs": ["project-brief", "evidence"],
                    "closure_outputs": ["closure-review"],
                    "measured_status": "built",
                },
            },
        ]
        outputs = {
            "outputs": [
                {"project_key": "P0", "category": category} for category in p0["now"]
            ]
            + [
                {"project_key": "P0", "category": "pi-plan", "title": "tranche-0 PI plan"},
                {"project_key": "P0", "category": "pi-objectives", "title": "tranche-0 PI objectives"},
                {"project_key": "P0", "category": "pi-plan", "title": "tranche-1 PI plan"},
                {"project_key": "P0", "category": "pi-objectives", "title": "tranche-1 PI objectives"},
                {"project_key": "P0", "category": "closure-review"},
                {"project_key": "T1", "category": "project-brief"},
                {"project_key": "T1", "category": "evidence"},
            ]
        }
        tranches = [
            {"id": "tr0", "number": 0, "state": "closed", "entry_criteria": "x",
             "exit_criteria": "y", "review_at": "2026-11-18T00:00:00+00:00"},
            {"id": "tr1", "number": 1, "state": "authorised",
             "authorised_at": "2026-11-18T00:00:00+00:00",
             "authorisation_review_id": "rev0", "entry_criteria": "x", "exit_criteria": "y"},
        ]
        msp = {
            "tranches": tranches,
            "tranche_reviews": [
                {"id": "rev0", "tranche_id": "tr0", "decision": "continue",
                 "reviewed_at": "2026-11-18T00:00:00+00:00",
                 "evidence": {"baselines": {}, "benefit_status": {}, "risk_register": {}}}
            ],
            "benefit_profiles": [
                _benefit("B1", "b1")
            ],
            "benefit_links": [{"benefit_id": "b1"}],
            "benefit_transitions": [],
            "programme_baselines": [
                {"kind": "management"}, {"kind": "governance"}, {"kind": "boundary"}
            ],
            "cadences": [
                {"id": "cd0-pi", "kind": "pi", "tranche_number": "0"},
                {"id": "cd0-ref", "kind": "refinement", "tranche_number": "0"},
                {"id": "cd0-retro", "kind": "retro", "tranche_number": "0"},
                {"id": "cd1-pi", "kind": "pi", "tranche_number": "1"},
                {"id": "cd1-ref", "kind": "refinement", "tranche_number": "1"},
            ],
            "wbs_nodes": [
                {"id": "w0", "project_id": "t1", "kind": "epic", "status": "open"}
            ],
            "demo_credits": [],
            "lessons": [
                {"id": "l0", "cadence_id": "cd0-retro", "statement": "keep the gate green"}
            ],
        }
        architecture = {
            "profiles": [profile],
            "standard": [
                {"id": "type-principles", "key": "principles_catalog", "phase_code": "preliminary", "plateau": "psa"},
                {"id": "type-solution", "key": "solution_concept", "phase_code": "a", "plateau": "psa"},
            ],
            "artifacts": [
                {"id": "a1", "project_id": "p0", "artifact_type_id": "type-principles", "status": "approved"},
                {"id": "a2", "project_id": "t1", "artifact_type_id": "type-principles", "status": "approved"},
                {"id": "a3", "project_id": "t1", "artifact_type_id": "type-solution", "status": "approved"},
            ],
            "links": [],
            "contracts": [
                {"id": "c1", "project_id": "t1", "kind": "statement_of_architecture_work",
                 "status": "live", "approved_at": "2026-11-20T01:00:00+00:00",
                 "approved_by": "u1", "requested_by": "u2"},
                # A v2 project carries both; the compliance review names the delivery contract.
                {"id": "c2", "project_id": "t1", "kind": "delivery_contract",
                 "phase_code": "f", "status": "live",
                 "approved_at": "2026-11-20T01:30:00+00:00",
                 "approved_by": "u1", "requested_by": "u2"},
            ],
            "compliance_reviews": [
                {"id": "r1", "project_id": "t1", "contract_id": "c2", "level": "compliant",
                 "reviewed_at": "2026-11-20T02:00:00+00:00"}
            ],
            "waivers": [],
            "board": [{"kind": "architecture_board", "status": "active"}],
        }
        programme = {"programme": [{"name": "P"}], "projects": projects}
        seed = {
            "projects": [
                {"key": "P0", "status": "closed"},
                {"key": "T1", "status": "active"},
            ]
        }
        definition = {
            "projects": {
                "P0": {"adm_phase": "preliminary", "plateau": "psa"},
                "T1": {"adm_phase": "a", "plateau": "psa"},
            }
        }
        coverage = {
            "projects": [
                {"key": "P0", "status": "built"},
                {"key": "T1", "status": "built"},
            ]
        }
        issues = {
            "issues": [
                {"number": 435, "body": "generated by scripts/build-coverage.py"}
            ]
        }
        manifest = {"schema": "design-snapshot/v1", "git_sha": "test", "generated_at": "test"}
        return Dataset(
            programme=programme,
            msp=msp,
            architecture=architecture,
            manifest=manifest,
            outputs=outputs,
            coverage=coverage,
            issues=issues,
            seed=seed,
            definition=definition,
            output_standard=output_standard,
            standard=local_standard,
            repo_root=ROOT,
        )

    def _benefit(reference: str, ident: str) -> dict[str, Any]:
        return {
            "id": ident,
            "reference": reference,
            "target": "target",
            "measurement_method": "method",
            "operational_area": "area",
            "owner_job_id": "job",
            "baseline": {
                "value": 1,
                "unit": "count",
                "measured_at": "2026-11-19T00:00:00+00:00",
                "method": "method",
            },
            "status": "active",
        }

    def audit(data: Dataset) -> list[dict[str, Any]]:
        return Audit(data, data.standard).run()

    # A conformant base emits nothing.
    base_gaps = audit(base())
    if base_gaps:
        fail(f"the self-test base is not conformant: {base_gaps}")

    # A report is evidence, not an acceptance: the proposal carries no header and strict
    # refuses it; only the accepted form, which names the decision and the snapshot bytes,
    # passes. This is the trap that let `--report` overwrite an accepted ledger.
    proposal = baseline_entries(base_gaps, base().standard)
    if "accepted_at" in proposal or "snapshot" in proposal:
        fail("a proposal must not carry an acceptance header")
    proposal_failures = check_strict(base_gaps, proposal, {"P0", "P1"})
    if not any("header is missing accepted_at" in item for item in proposal_failures):
        fail(f"strict accepted a proposal: {proposal_failures}")
    with tempfile.TemporaryDirectory() as snapshot_dir:
        fixture = base()
        Path(snapshot_dir, "manifest.json").write_text(
            json.dumps(fixture.manifest), encoding="utf-8"
        )
        accepted = accepted_baseline(
            base_gaps,
            fixture.standard,
            fixture,
            Path(snapshot_dir),
            accepted_by="self-test",
            approver="self-test-approver",
            acceptance_ref="ref",
            delegation_ref="delegation",
            accepted_at="2026-11-22T00:00:00Z",
        )
        accepted_failures = check_strict(base_gaps, accepted, {"P0", "P1"})
        if accepted_failures:
            fail(f"strict refused the accepted ledger: {accepted_failures}")
        if accepted["snapshot"]["manifest_sha256"] != hashlib.sha256(
            Path(snapshot_dir, "manifest.json").read_bytes()
        ).hexdigest():
            fail("the accepted ledger does not pin the manifest it was taken from")

    # Test residue is excluded by a named predicate and stays visible in the report.
    if not test_row_reason({"key": "AAA-UI-PROOF-285-2026-11-19T07-3-2"}):
        fail("a known pre-marker e2e project was not recognised as test residue")
    if not test_row_reason({"key": "X", "meta": {"test_row": True}}):
        fail("meta.test_row was not recognised")
    if test_row_reason({"key": "PLATFORM", "meta": {"test_row": False}}):
        fail("a real project was excluded as test residue")

    def expect(mutate, rule: str, severity: str | None = None) -> None:
        data = base()
        mutate(data)
        found = audit(data)
        matches = [gap for gap in found if gap["rule"] == rule]
        if not matches:
            fail(f"mutating for {rule} produced {sorted({gap['rule'] for gap in found})}")
        if severity and not any(gap["severity"] == severity for gap in matches):
            fail(f"{rule} fired at {[gap['severity'] for gap in matches]}, expected {severity}")

    def expect_absent(mutate, rule: str) -> None:
        data = base()
        mutate(data)
        if any(gap["rule"] == rule for gap in audit(data)):
            fail(f"{rule} fired where it must not")

    def expect_clean(mutate) -> None:
        data = base()
        mutate(data)
        found = audit(data)
        if found:
            fail(f"a conformant fixture produced {found}")

    def noop(data: Dataset) -> None:
        pass

    def drop_artefact(data: Dataset, artifact_type_id: str) -> None:
        data.artefacts[:] = [
            row for row in data.artefacts if row.get("artifact_type_id") != artifact_type_id
        ]

    def drop_authorisation(data: Dataset) -> None:
        data.tranche_by_number[1]["state"] = "planned"
        data.tranche_by_number[1]["authorisation_review_id"] = None

    expect(drop_authorisation, "tranche_review_without_authorisation")
    expect(drop_authorisation, "project_in_unauthorised_tranche")

    def drop_output(data: Dataset) -> None:
        data.published["P0"].discard("vision")

    expect(drop_output, "programme_outputs")

    def close_without_review(data: Dataset) -> None:
        data.by_key["T1"]["status"] = "closed"
        data.published["T1"].discard("closure-review")

    expect(close_without_review, "closed_without_closure_review")

    def placeholder(data: Dataset) -> None:
        data.benefits[0]["baseline"] = "not measured - no instrument exists"

    expect(placeholder, "benefit_baseline_placeholder")

    def missing_area(data: Dataset) -> None:
        data.benefits[0]["operational_area"] = ""

    expect(missing_area, "benefit_profile_incomplete")

    def realized(data: Dataset) -> None:
        data.benefits[0]["status"] = "realized"

    expect(realized, "realized_without_transition")

    def self_approval(data: Dataset) -> None:
        data.contracts[0]["requested_by"] = data.contracts[0]["approved_by"]

    expect(self_approval, "requester_is_approver")

    def no_contract(data: Dataset) -> None:
        data.contracts.clear()

    expect(no_contract, "contract_missing")

    def delivery_contract_reviewed(data: Dataset) -> None:
        data.by_key["T1"]["status"] = "closed"
        if not any(
            row.get("kind") == "delivery_contract" and row.get("project_id") == "t1"
            for row in data.contracts
        ):
            data.contracts.append(
                {
                    "id": "c2",
                    "project_id": "t1",
                    "kind": "delivery_contract",
                    "phase_code": "f",
                    "status": "live",
                    "approved_at": "2026-11-20T01:30:00+00:00",
                    "approved_by": "u1",
                    "requested_by": "u2",
                }
            )
        data.reviews_arch[0]["contract_id"] = "c2"

    # A Phase-G review of the delivery contract satisfies the rule; the live SoAW is not a
    # compliance subject, so its having no review is not a gap.
    expect_absent(delivery_contract_reviewed, "compliance_review_missing")

    def delivery_contract_unreviewed(data: Dataset) -> None:
        delivery_contract_reviewed(data)
        data.reviews_arch[0]["contract_id"] = "c1"

    expect(delivery_contract_unreviewed, "compliance_review_missing")

    def soaw_only(data: Dataset) -> None:
        # A v2 project whose only live contract is the SoAW: the delivery contract is what a
        # Phase-G review is of, so with none live and no reviews the rule must stay silent.
        data.by_key["T1"]["status"] = "closed"
        data.reviews_arch.clear()
        data.contracts[:] = [
            row for row in data.contracts if row.get("kind") != "delivery_contract"
        ]

    expect_absent(soaw_only, "compliance_review_missing")

    def draft_board(data: Dataset) -> None:
        data.architecture["board"][0]["status"] = "draft"

    expect(draft_board, "board_not_approved")

    def draft_profile(data: Dataset) -> None:
        data.profile["status"] = "draft"

    expect(draft_profile, "profile_adopted")

    def no_baselines(data: Dataset) -> None:
        data.msp["programme_baselines"] = []

    expect(no_baselines, "programme_baselines")

    # SAFe §8.2: the PI controls are per tranche and must refuse, not merely report.
    def drop_pi_output(data: Dataset, category: str, needle: str) -> None:
        text = lambda row: f"{row.get('title') or ''} {row.get('unit') or ''}".lower()
        data.outputs["outputs"][:] = [
            row
            for row in data.outputs["outputs"]
            if not (row.get("category") == category and needle in text(row))
        ]

    expect(lambda data: drop_pi_output(data, "pi-plan", "tranche-1"),
           "pi_plan_missing", "P0")
    expect(lambda data: drop_pi_output(data, "pi-objectives", "tranche-0"),
           "pi_objectives_missing", "P1")

    def drop_cadence_for(data: Dataset, kind: str, tranche: str) -> None:
        data.msp["cadences"][:] = [
            row
            for row in data.msp["cadences"]
            if not (row.get("kind") == kind and row.get("tranche_number") == tranche)
        ]

    expect(lambda data: drop_cadence_for(data, "pi", "1"),
           "pi_cadence_missing", "P1")
    expect(lambda data: drop_cadence_for(data, "refinement", "1"),
           "backlog_refinement_missing", "P1")

    def add_deliverable(data: Dataset, **overrides: Any) -> None:
        node = {
            "id": "w1",
            "project_id": "t1",
            "kind": "deliverable",
            "status": "accepted",
            "job_size": 1,
            "business_value": 5,
            "time_criticality": 3,
            "risk_reduction": 2,
        }
        node.update(overrides)
        data.msp["wbs_nodes"].append(node)

    expect(add_deliverable, "demo_credit_missing", "P1")

    def empty_credit(data: Dataset) -> None:
        add_deliverable(data)
        data.msp["demo_credits"].append(
            {"id": "cr1", "deliverable_id": "w1", "evidence": []}
        )

    expect(empty_credit, "demo_credit_missing", "P1")

    def no_lessons(data: Dataset) -> None:
        data.msp["lessons"].clear()

    expect(no_lessons, "retro_lesson_missing", "P1")

    def empty_lesson(data: Dataset) -> None:
        data.msp["lessons"][0]["statement"] = ""

    expect(empty_lesson, "retro_lesson_missing", "P1")

    def zeroed_wsjf(data: Dataset) -> None:
        add_deliverable(data, job_size=0, business_value=0,
                        time_criticality=0, risk_reduction=0)

    expect(zeroed_wsjf, "wsjf_inputs_missing", "P1")

    def seed_drift(data: Dataset) -> None:
        data.seed_by_key["T1"]["status"] = "closed"

    expect(seed_drift, "seed_live_drift")

    def missing_derivation(data: Dataset) -> None:
        # The case reads issue #435 alone, so the fixture must mutate THAT row — and it must name
        # a path no commit can contain; naming a script that exists (as the base fixture's
        # `scripts/build-coverage.py` does) made the "missing script" case produce nothing.
        rows = data.issues["issues"]
        row = next((row for row in rows if row.get("number") == 435), None)
        if row is None:
            row = {"number": 435, "state": "CLOSED", "labels": [], "title": "", "url": ""}
            rows.append(row)
        row["body"] = "generated by scripts/definitely-missing-derivation.py"

    expect(missing_derivation, "derivation_missing")

    def transition_without_baseline(data: Dataset) -> None:
        row = {"benefit_id": "b1", "post_measured_at": "2026-11-20T00:00:00+00:00"}
        data.msp["benefit_transitions"].append(row)
        data.transitions.append(row)

    expect(transition_without_baseline, "transition_order")

    def invalid_waiver(data: Dataset) -> None:
        row = {"id": "w1", "status": "active", "review_id": "r1"}
        data.architecture["waivers"].append(row)
        data.waivers.append(row)

    expect(invalid_waiver, "waiver_invalid")

    def programme_waiver(data: Dataset) -> None:
        # A waiver not attached to an architecture compliance review is programme work; it is
        # a finding, not a legal deviation.
        row = {"id": "w2", "status": "active", "review_id": "no-such-review"}
        data.architecture["waivers"].append(row)
        data.waivers.append(row)

    expect(programme_waiver, "waiver_not_permitted")

    # A Phase-G review of the delivery contract satisfies the rule; the live SoAW is not a
    # compliance subject, so its having no review is not a gap.
    expect_absent(lambda data: None, "compliance_review_missing")

    def review_waiver(kind: str):
        def mutate(data: Dataset) -> None:
            data.by_key["T1"]["status"] = "closed"
            data.published["T1"].add("closure-review")
            data.reviews_arch[0]["level"] = "non_conformant"
            row = {
                "id": f"w-{kind}",
                "status": "active",
                "review_id": "r1",
                "kind": kind,
                "owner_user_id": "u1",
                "reason": "self-test",
                "reassessment_at": "2099-01-01T00:00:00+00:00",
            }
            data.architecture["waivers"].append(row)
            data.waivers.append(row)

        return mutate

    # A programme waiver never suppresses a non-conformant compliance review; only an
    # architecture_waiver on that review may.
    expect(review_waiver("programme"), "waiver_not_permitted")
    expect(review_waiver("programme"), "compliance_review_missing", "P0")
    expect_absent(review_waiver("architecture_waiver"), "compliance_review_missing")
    expect_absent(review_waiver("architecture_waiver"), "waiver_not_permitted")

    # capability_artefacts: the Preliminary set is checked once, at capability level.
    expect(lambda data: drop_artefact(data, "type-principles"), "capability_artefacts", "P1")

    def unowned_principles(data: Dataset) -> None:
        data.artefacts[:] = [row for row in data.artefacts if row.get("id") != "a1"]

    # Pass: a Preliminary-phase project need not own the capability artefacts; an unowned
    # approved artefact of the type satisfies the capability set.
    expect_clean(unowned_principles)

    # mandatory_artefacts: the per-project set starts at A, at P1 with no pre-adoption discount.
    def missing_solution(data: Dataset) -> None:
        drop_artefact(data, "type-solution")

    expect(missing_solution, "mandatory_artefacts", "P1")

    def pre_adoption_missing(data: Dataset) -> None:
        data.by_key["T1"]["created_at"] = "2020-01-01T00:00:00+00:00"
        drop_artefact(data, "type-solution")

    expect(pre_adoption_missing, "mandatory_artefacts", "P1")

    def folded_missing(data: Dataset) -> None:
        data.by_key["T1"]["meta"]["viability"] = "fold"
        drop_artefact(data, "type-solution")

    expect_absent(folded_missing, "mandatory_artefacts")

    def close_by_decision_missing(data: Dataset) -> None:
        data.by_key["T1"]["meta"]["viability"] = "close-by-decision"
        drop_artefact(data, "type-solution")

    expect_absent(close_by_decision_missing, "mandatory_artefacts")

    # closure_outputs_undeclared, and the vacuous skip removed from the review rule.
    def empty_closure(data: Dataset) -> None:
        data.by_key["T1"]["status"] = "closed"
        data.by_key["T1"]["meta"]["closure_outputs"] = []
        data.published["T1"].discard("closure-review")

    expect(empty_closure, "closure_outputs_undeclared", "P1")
    expect(empty_closure, "closed_without_closure_review", "P1")
    expect_absent(noop, "closure_outputs_undeclared")
    expect_absent(noop, "closed_without_closure_review")

    # Strict mode: the ledger is a ratchet with one disposition vocabulary.
    sample = base()
    drop_artefact(sample, "type-solution")
    sample_gaps = audit(sample)
    ledger = baseline_entries(sample_gaps, standard)
    accepted_ledger = dict(ledger)
    accepted_ledger.update(
        {
            "accepted_at": "2026-11-21T00:00:00Z",
            "accepted_by": "conveyor-e2e",
            "approver": "conveyor-approver",
            "acceptance_ref": "01a1ff54-eb5c-7581-91c6-4e1ea2cde8c7",
            "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
            "snapshot": {
                "git_sha": "d7293f96",
                "generated_at": "2026-11-21T18:17:55Z",
                "manifest_sha256": "ba8129",
                "outputs": 116,
            },
        }
    )
    if check_strict(sample_gaps, accepted_ledger, {"P0", "P1"}):
        fail("strict rejects its own generated ledger")
    if [entry["disposition"] for entry in ledger["entries"]] != ["rework"]:
        fail(f"the base ledger carries {ledger['entries']}")

    def accepted(entries: list[dict[str, Any]]) -> dict[str, Any]:
        return {
            "version": 1,
            "accepted_at": "2026-11-21T00:00:00Z",
            "accepted_by": "conveyor-e2e",
            "approver": "conveyor-approver",
            "acceptance_ref": "01a1ff54-eb5c-7581-91c6-4e1ea2cde8c7",
            "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
            "snapshot": {
                "git_sha": "d7293f96",
                "generated_at": "2026-11-21T18:17:55Z",
                "manifest_sha256": "ba8129",
                "outputs": 116,
            },
            "entries": entries,
        }

    def strict_failures(entries: list[dict[str, Any]]) -> str:
        found = check_strict(sample_gaps, accepted(entries), {"P0", "P1"})
        if not found:
            fail(f"strict accepted {entries}")
        return " | ".join(found)

    headerless = check_strict(
        sample_gaps, {"version": 1, "entries": ledger["entries"]}, {"P0", "P1"}
    )
    if not any("header is missing accepted_by" in failure for failure in headerless):
        fail("strict accepted a baseline without its acceptance header")
    ownerless = [dict(entry) for entry in ledger["entries"]]
    ownerless[0]["owner"] = ""
    if "has no owner" not in strict_failures(ownerless):
        fail("strict accepted an entry without an owner")

    missing_disposition = [dict(entry) for entry in ledger["entries"]]
    del missing_disposition[0]["disposition"]
    if "no valid disposition" not in strict_failures(missing_disposition):
        fail("strict did not name the entry without a disposition")

    expired = [dict(entry) for entry in ledger["entries"]]
    expired[0]["expires"] = "2020-01-01"
    if "baseline expired" not in strict_failures(expired):
        fail("strict did not refuse the expired entry")

    if "new P1" not in strict_failures([]):
        fail("strict did not refuse a new fingerprint")

    grown = [dict(entry) for entry in ledger["entries"]] + [
        {
            "rule": "mandatory_artefacts",
            "subject": "Z9",
            "severity": "P1",
            "disposition": "rework",
            "owner": "programme owner",
            "expires": "2027-02-18",
            "reason": "added without a live gap",
        }
    ]
    if "ledger grew: mandatory_artefacts Z9" not in strict_failures(grown):
        fail("strict did not refuse ledger growth")

    if baseline_disposition(
        {"rule": "mandatory_artefacts", "subject": "S06a", "severity": "P1"}
    ) != "fold":
        fail("S06a is not folded in the ledger")
    if baseline_disposition(
        {"rule": "derivation_missing", "subject": "#435", "severity": "P1"}
    ) != "correct-the-claim":
        fail("derivation_missing is not a correct-the-claim")
    if baseline_disposition(
        {"rule": "waiver_invalid", "subject": "w", "severity": "P1", "disposition": "unprovable"}
    ) != "unprovable":
        fail("an unprovable gap is not marked unprovable")

    # The derived-status assertion: a closed project with a new closure gap fails strict with
    # the project named, not only the rule, so a publish refusal is actionable.
    closed_gap = base()
    closed_gap.by_key["T1"]["status"] = "closed"
    closed_gap.by_key["T1"]["meta"]["closure_outputs"] = []
    closed_gap.published["T1"].discard("closure-review")
    closed_failure = " | ".join(
        check_strict(audit(closed_gap), {"version": 1, "entries": []}, {"P0", "P1"})
    )
    if "closed_without_closure_review T1" not in closed_failure:
        fail(f"strict did not name the closed project: {closed_failure}")

    # The run is deterministic: same inputs, same bytes.
    first = json.dumps([gap for gap in audit(base())], sort_keys=True)
    second = json.dumps([gap for gap in audit(base())], sort_keys=True)
    if first != second:
        fail("two audits of the same input differ")
    # The rework register: P0 is never grouped, P1 splits deterministically at the cap, a
    # waiver is refused as a disposition, a duplicate finding is folded, and a re-run is
    # byte-identical.
    register_data = base()
    register_gaps = [
        {
            "rule": "contract_missing",
            "severity": "P1",
            "subject": f"T1:{number}",
            "detail": "d",
            "disposition": "rework",
            "evidence": None,
        }
        for number in range(1, 22)
    ]
    register_gaps.append(
        {
            "rule": "closed_without_closure_review",
            "severity": "P0",
            "subject": "P0",
            "detail": "d",
            "disposition": "rework",
            "evidence": None,
        }
    )
    register_gaps.append(
        {
            "rule": "benefit_baseline_placeholder",
            "severity": "P1",
            "subject": "B1",
            "detail": "d",
            "disposition": "rework",
            "evidence": None,
        }
    )
    register_gaps.append(
        {
            "rule": "mandatory_artefacts",
            "severity": "P1",
            "subject": "tranche 1",
            "detail": "d",
            "disposition": "rework",
            "evidence": None,
        }
    )
    register_baseline = {
        "version": 1,
        "entries": [
            {
                "rule": "benefit_baseline_placeholder",
                "subject": "B1",
                "severity": "P1",
                "disposition": "waiver",
                "owner": "o",
                "expires": "2099-01-01",
                "reason": "r",
            }
        ],
    }
    register_findings = {
        "findings": [
            {
                "id": "v1-x-1",
                "area": "§9",
                "standard": "SAFe",
                "spec": "§9.4",
                "severity": "P1",
                "title": "t",
                "problem": "p",
                "evidence": [
                    {
                        "path": "scripts/audit-programme-conformance.py",
                        "line": 1,
                        "quote": "#!/usr/bin/env python3",
                    }
                ],
                "acceptance": "a",
                "owner_project": "T1",
            },
            {
                "id": "v1-x-2",
                "area": "§9",
                "standard": "SAFe",
                "spec": "§9.4",
                "severity": "P1",
                "title": "dup",
                "problem": "p",
                "evidence": [
                    {
                        "path": "scripts/audit-programme-conformance.py",
                        "line": 1,
                        "quote": "#!/usr/bin/env python3",
                    }
                ],
                "acceptance": "a",
                "owner_project": "T1",
                "duplicate_of": "contract_missing:T1:1",
            },
        ]
    }
    first = build_register(
        register_gaps, register_baseline, register_findings, standard, register_data, "fixture"
    )
    second = build_register(
        json.loads(json.dumps(register_gaps)),
        register_baseline,
        register_findings,
        standard,
        register_data,
        "fixture",
    )
    if json.dumps(first, sort_keys=True) != json.dumps(second, sort_keys=True):
        fail("the rework register is not deterministic")
    keys = [package["key"] for package in first["packages"]]
    # A project-subject gap is the project's own card, keyed by the project, and the cap
    # splits it deterministically.
    if "contract_missing:T1#1" not in keys or "contract_missing:T1#2" not in keys:
        fail(f"the 21-subject project rule did not split at the cap: {keys}")
    # A gap whose subject is the tranche itself stays tranche-scoped.
    if "mandatory_artefacts:tranche-1" not in keys:
        fail(f"a tranche-subject gap lost its tranche scope: {keys}")
    if not any(
        package["key"] == "closed_without_closure_review:P0" and package["subject_count"] == 1
        for package in first["packages"]
    ):
        fail("a P0 gap was grouped")
    if not any(
        package["key"].startswith("benefit_baseline_placeholder")
        and package["status"] == "open"
        for package in first["packages"]
    ):
        fail("a baseline waiver suppressed its package; no disposition may suppress a gap")
    # A finding's waiver is refused as a disposition: the finding stays open and the waiver is
    # itself a `waiver_not_permitted` finding. A malformed waiver is still refused outright.
    def with_waiver(waiver: dict[str, Any]) -> dict[str, Any]:
        document = json.loads(json.dumps(register_findings))
        document["findings"][0]["waiver"] = waiver
        return document

    valid_waiver = {
        "accepted_by": "programme owner",
        "accepted_at": "2026-11-22",
        "decision_ref": "fixture-decision",
        "expires": "2027-02-18",
        "reason": "self-test residual",
    }
    waived = build_register(
        register_gaps,
        register_baseline,
        with_waiver(valid_waiver),
        standard,
        register_data,
        "fixture",
    )
    if not any(
        package["rule"] == "v1-x-1" and package["status"] == "open"
        for package in waived["packages"]
    ):
        fail("a finding waiver suppressed its package")
    waiver_packages = [
        package for package in waived["packages"] if package["rule"] == "waiver_not_permitted"
    ]
    if not waiver_packages:
        fail("a programme waiver did not become a waiver_not_permitted finding")
    if "v1-x-1" not in {
        str(subject.get("subject"))
        for package in waiver_packages
        for subject in package["subjects"]
    }:
        fail("the waiver_not_permitted finding does not name the waivered finding")

    def refused_waiver(waiver: dict[str, Any], why: str) -> None:
        try:
            build_register(
                register_gaps,
                register_baseline,
                with_waiver(waiver),
                standard,
                register_data,
                "fixture",
            )
        except SystemExit:
            return
        fail(f"an invalid finding waiver was accepted ({why})")

    ownerless = dict(valid_waiver)
    del ownerless["accepted_by"]
    refused_waiver(ownerless, "no owner")
    expired_waiver = dict(valid_waiver)
    expired_waiver["expires"] = "2020-01-01"
    refused_waiver(expired_waiver, "expired")

    if first["summary"]["duplicates"] != 1:
        fail("duplicate_of did not fold the finding")
    if not any(package["rule"] == "v1-x-1" for package in first["packages"]):
        fail("the v1 finding is not a package")
    # R11's package kind: a ledger finding is CODE, a conformance gap is RECORD, and a kind the
    # generator states (a template defect) is carried through as STANDARD.
    kinds = {package["rule"]: package["kind"] for package in first["packages"]}
    if kinds.get("v1-x-1") != "CODE":
        fail(f"a v1 ledger finding is not a CODE package: {kinds.get('v1-x-1')}")
    if not any(package["kind"] == "RECORD" for package in first["packages"]):
        fail("a conformance gap is not a RECORD package")
    if first["summary"]["by_kind"].get("CODE", 0) < 1:
        fail("the summary does not count package kinds")
    kinded = json.loads(json.dumps(register_findings))
    kinded["findings"][0]["kind"] = "STANDARD"
    standard_register = build_register(
        register_gaps, register_baseline, kinded, standard, register_data, "fixture"
    )
    if not any(
        package["rule"] == "v1-x-1" and package["kind"] == "STANDARD"
        for package in standard_register["packages"]
    ):
        fail("a generator-declared STANDARD finding did not become a STANDARD package")

    # A deep-review sidecar converts into the ledger shape the register reads: the template
    # names the standard, the section derives from it, and the sidecar's kind becomes the
    # package kind. A kind the converter does not know is refused, never guessed.
    with tempfile.TemporaryDirectory() as sidecar_dir:
        sidecar = Path(sidecar_dir) / "deep-review-findings.json"

        def write_sidecar(kind: str, **extra: object) -> None:
            sidecar.write_text(
                json.dumps(
                    {
                        "schema": "w3-deep-review/v1",
                        "findings": [
                            {
                                "key": "x-1",
                                "kind": kind,
                                "severity": "P1",
                                "owner_project": "T1",
                                "title": "a deep-review title",
                                "evidence": "a deep-review paragraph",
                                "acceptance": "a deep-review acceptance",
                                "artifact": "seed/output-standard.json#mandate",
                                **extra,
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )

        write_sidecar("record+code")
        converted = deep_review_findings(sidecar, output_standard)
        if len(converted) != 1:
            fail(f"a one-finding deep-review sidecar converted to {len(converted)} findings")
        finding = converted[0]
        if (
            finding["id"] != "dr-x-1"
            or finding["standard"] != "MSP"
            or finding["spec"] != "§8.3"
            or finding["kind"] != "CODE"
            or finding["problem"] != "a deep-review paragraph"
            or finding["evidence"]
            != [
                {
                    "source": "deep-review",
                    "path": str(sidecar),
                    "quote": "a deep-review paragraph",
                }
            ]
        ):
            fail(f"the deep-review conversion is wrong: {finding}")
        deep_register = build_register(
            register_gaps,
            register_baseline,
            {"findings": converted},
            standard,
            register_data,
            "fixture",
        )
        if not any(
            package["rule"] == "dr-x-1"
            and package["kind"] == "CODE"
            and package["standard"] == "MSP"
            and package["spec"] == "§8.3"
            for package in deep_register["packages"]
        ):
            fail("a deep-review finding did not become its package with its kind")
        # A standard finding on a code-owned artefact also produces a CODE sibling with an
        # executable test; the record package depends on it. A code-owned finding with no
        # acceptance_test is refused: a record-only repair cannot close a code change.
        write_sidecar(
            "standard",
            acceptance_test=(
                "cargo-guard.py test -p conveyor-actions --test registry "
                "registers_list_accepts_its_filters"
            ),
        )
        converted_code = deep_review_findings(sidecar, output_standard)
        by_id = {finding["id"]: finding for finding in converted_code}
        if set(by_id) != {"dr-x-1", "dr-x-1-code"}:
            fail(f"a code-owned standard finding did not produce its sibling: {sorted(by_id)}")
        if by_id["dr-x-1"]["kind"] != "STANDARD" or by_id["dr-x-1-code"]["kind"] != "CODE":
            fail("the code-owned standard finding or its sibling has the wrong kind")
        if "dr-x-1-code" not in by_id["dr-x-1"].get("depends_on", []):
            fail("the record package does not depend on its code sibling")
        code_register = build_register(
            register_gaps,
            register_baseline,
            {"findings": converted_code},
            standard,
            register_data,
            "fixture",
        )
        code_packages = {package["rule"]: package for package in code_register["packages"]}
        if code_packages.get("dr-x-1", {}).get("kind") != "STANDARD":
            fail("the code-owned standard finding is not a STANDARD package")
        if code_packages.get("dr-x-1", {}).get("depends_on") != ["dr-x-1-code"]:
            fail("the STANDARD package does not carry the sibling dependency")
        sibling_package = code_packages.get("dr-x-1-code") or {}
        if sibling_package.get("kind") != "CODE":
            fail("the code sibling is not a CODE package")
        if sibling_package.get("acceptance_command") != (
            "cargo-guard.py test -p conveyor-actions --test registry "
            "registers_list_accepts_its_filters"
        ):
            fail("the code sibling's acceptance command is not its executable test")
        write_sidecar("standard")
        try:
            deep_review_findings(sidecar, output_standard)
        except SystemExit:
            pass
        else:
            fail("a code-owned standard finding without acceptance_test was admitted")
        # A resolved deep-review finding is closed by the same rule as a ledger finding:
        # its receipt survives the conversion and the register neither emits a package for
        # it nor loses the resolution count.
        receipt = {
            "commit": "0" * 40,
            "test": "crates/x/tests/y.rs::z",
            "evidence": "the test proves it",
        }
        write_sidecar("record+code", resolved_by=receipt)
        converted_resolved = deep_review_findings(sidecar, output_standard)
        if converted_resolved[0].get("resolved_by") != receipt:
            fail("a resolved deep-review finding lost its resolved_by")
        resolved_register = build_register(
            register_gaps,
            register_baseline,
            {"findings": converted_resolved},
            standard,
            register_data,
            "fixture",
        )
        if any(
            package["rule"] == "dr-x-1"
            for package in resolved_register["packages"]
        ):
            fail("a resolved deep-review finding still produced a package")
        if resolved_register["summary"]["resolved"] != 1:
            fail("a resolved deep-review finding was not counted as resolved")
        write_sidecar("record")
        converted_record = deep_review_findings(sidecar, output_standard)
        if converted_record[0]["kind"] != "RECORD":
            fail("a deep-review record finding did not convert to RECORD")
        write_sidecar("prose")
        try:
            deep_review_findings(sidecar, output_standard)
        except SystemExit:
            pass
        else:
            fail("an unknown deep-review kind was accepted")
    command = register_acceptance("k:s", "snap", ["v1.json"], ["w3.json", "w4.json"])
    for fragment in (
        "--snapshot snap",
        "--findings v1.json",
        "--deep-review w3.json",
        "--deep-review w4.json",
        "--rework-check k:s",
    ):
        if fragment not in command:
            fail(f"register_acceptance omits {fragment!r}")
    if first["scorecard"]["MSP"]["rules"] < 1:
        fail("the scorecard has no MSP rules")

    # WP11: every standard rule is covered by exactly one category, source and verifier. The
    # coverage instrument owns that property; this self-test proves that instrument's offline
    # mechanism. The repository-wide `--check` runs in the repository gate — never here, because
    # a self-test that reads the generated pack turns a stale pack into an instrument failure.
    coverage = ROOT / "scripts" / "check-standards-coverage.py"
    if not coverage.is_file():
        raise SystemExit(
            "audit-programme-conformance: scripts/check-standards-coverage.py is missing"
        )
    completed = subprocess.run(
        [sys.executable, str(coverage), "--self-test"], capture_output=True, text=True
    )
    if completed.returncode != 0:
        tail = (completed.stdout + completed.stderr).strip().splitlines()[-3:]
        raise SystemExit(
            "audit-programme-conformance: standards coverage failed: " + " ".join(tail)
        )
    print("audit-programme-conformance: self-test OK")


# ── CLI ──────────────────────────────────────────────────────────────────────────────


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--snapshot", type=Path, default=DEFAULT_SNAPSHOT)
    parser.add_argument(
        "--path-label",
        action="append",
        default=[],
        metavar="OLD=NEW",
        help="name a working copy's file by its installed path inside the register's "
        "acceptance commands; repeatable",
    )
    parser.add_argument(
        "--snapshot-label",
        default=None,
        help="the path the register's acceptance commands and the page name, so a "
        "temp working copy does not leak into the public pack; defaults to --snapshot",
    )
    parser.add_argument("--outputs", type=Path, default=None)
    parser.add_argument("--seed-dir", type=Path, default=ROOT / "seed")
    parser.add_argument("--standard", type=Path, default=DEFAULT_STANDARD)
    parser.add_argument("--repo-root", type=Path, default=ROOT)
    parser.add_argument("--baseline", type=Path, default=DEFAULT_BASELINE)
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--report", action="store_true")
    parser.add_argument("--strict", action="store_true")
    parser.add_argument("--fail-on", default="P0,P1")
    parser.add_argument("--write-baseline", type=Path, default=None)
    parser.add_argument(
        "--accept-baseline",
        action="store_true",
        help=(
            "write the accepted ledger to --baseline; requires --accepted-by, --approver, "
            "--acceptance-ref and --delegation-ref, and records the snapshot it was taken from"
        ),
    )
    parser.add_argument("--accepted-by", default=None)
    parser.add_argument("--approver", default=None)
    parser.add_argument("--acceptance-ref", default=None)
    parser.add_argument("--delegation-ref", default=None)
    parser.add_argument(
        "--findings",
        type=Path,
        action="append",
        default=None,
        help=(
            "a findings ledger to merge into the rework register; repeatable, so the checked-in "
            "v1 ledger and the generated artefact findings are merged together"
        ),
    )
    parser.add_argument(
        "--deep-review",
        type=Path,
        action="append",
        default=None,
        help=(
            "a deep-review sidecar whose findings are converted to ledger findings and merged "
            "into the rework register; repeatable"
        ),
    )
    parser.add_argument(
        "--rework-out",
        type=Path,
        default=None,
        help="write the rework register (one package per gap) to this path",
    )
    parser.add_argument(
        "--rework-check",
        metavar="KEY",
        default=None,
        help=(
            "rebuild the register and exit 1 while this package key is still open; this is the "
            "per-package acceptance command the register publishes"
        ),
    )
    parser.add_argument("--check-standard", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    # One label for the register's acceptance commands and the page: the publish runs against a
    # temp copy, and that path must not leak into the committed pack.
    snapshot_label = args.snapshot_label or str(args.snapshot)
    path_labels: dict[str, str] = {}
    for item in args.path_label:
        old, sep, new = item.partition("=")
        if not sep or not old or not new:
            parser.error(f"--path-label wants OLD=NEW, got {item!r}")
        path_labels[old] = new

    def labelled(path: object) -> str:
        raw = str(path)
        return path_labels.get(raw, raw)
    # The two findings ledgers are the register's default provenance. An explicit --findings
    # list replaces them; omission must not silently drop the standards findings, or a
    # --rework-check built from this command proves nothing about the standards packages.
    findings_paths = args.findings or [path for path in DEFAULT_FINDINGS if path.exists()]
    if (args.findings or args.deep_review) and not (args.rework_out or args.rework_check):
        parser.error(
            "--findings and --deep-review are only meaningful with --rework-out or --rework-check"
        )
    if args.rework_check and not (findings_paths or args.deep_review):
        parser.error("--rework-check needs the findings ledgers via --findings or --deep-review")

    if args.self_test:
        self_test()
        return
    standard = read_json(args.standard)
    standard_checks(standard, args.seed_dir)
    if args.check_standard:
        print("audit-programme-conformance: standard OK")
        return

    dataset = load_dataset(
        args.snapshot.resolve(), args.seed_dir.resolve(), standard, args.repo_root.resolve(), args.outputs
    )
    gaps = Audit(dataset, standard).run()

    register = None
    if args.rework_out or args.rework_check:
        baseline_doc = (
            read_json(args.baseline) if args.baseline.exists() else {"version": 1, "entries": []}
        )
        merged_findings: list[Any] = []
        for path in findings_paths:
            ledger = read_json(path)
            merged_findings.extend(ledger.get("findings", []))
        for path in args.deep_review or []:
            merged_findings.extend(deep_review_findings(path, dataset.output_standard))
        findings_doc = {"findings": merged_findings}
        register = build_register(
            gaps,
            baseline_doc,
            findings_doc,
            standard,
            dataset,
            snapshot_label,
            [labelled(path) for path in findings_paths],
            [labelled(path) for path in (args.deep_review or [])],
        )
    if args.rework_out and register is not None:
        args.rework_out.parent.mkdir(parents=True, exist_ok=True)
        # sort_keys + the package sort make a re-run byte-identical, so the published register
        # can be diffed like any other generated artefact.
        args.rework_out.write_text(
            json.dumps(register, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        print(
            f"audit-programme-conformance: wrote {register['summary']['packages']} rework packages "
            f"({register['summary']['open']} open, {register['summary']['suppressed']} suppressed, "
            f"{register['summary']['duplicates']} duplicates folded)"
        )
    if args.rework_check:
        open_keys = {str(package.get("key")) for package in register.get("packages") or []}
        # A key that is not open must still be a key the inputs know: a finding's rule and its
        # owner project, or a baseline entry. Before this set an unknown key printed "cleared"
        # and exited 0, so a typo in an acceptance command proved nothing.
        known_keys = set(open_keys)
        for entry in baseline_doc.get("entries", []) if isinstance(baseline_doc, dict) else []:
            if isinstance(entry, dict) and entry.get("rule") and entry.get("subject"):
                known_keys.add(f"{entry['rule']}:{entry['subject']}")
        for finding in merged_findings:
            if not isinstance(finding, dict):
                continue
            rule = str(finding.get("id") or "")
            subject = str(
                finding.get("owner_project") or finding.get("project")
                or finding.get("scope") or ""
            )
            if rule and subject:
                known_keys.add(f"{rule}:{subject}")
        if args.rework_check in open_keys:
            print(
                f"audit-programme-conformance: {args.rework_check} is still open",
                file=sys.stderr,
            )
            sys.exit(1)
        if args.rework_check not in known_keys:
            print(
                f"audit-programme-conformance: {args.rework_check} matches no open package and "
                "no finding or baseline rule; an unknown key is not cleared",
                file=sys.stderr,
            )
            sys.exit(1)
        print(f"audit-programme-conformance: {args.rework_check} is cleared")
        return

    if args.report or args.write_baseline or args.strict:
        args.out.mkdir(parents=True, exist_ok=True)
        try:
            snapshot_path = str(args.snapshot.resolve().relative_to(ROOT))
        except ValueError:
            snapshot_path = args.snapshot.name
        report = {
            "version": 1,
            "snapshot": {
                "git_sha": dataset.manifest.get("git_sha"),
                "generated_at": dataset.manifest.get("generated_at"),
                "tenant": dataset.manifest.get("tenant"),
                "path": snapshot_path,
            },
            "counts": {
                severity: sum(1 for gap in gaps if gap["severity"] == severity)
                for severity in SEVERITY_ORDER
            },
            "excluded_test_projects": dataset.excluded_test_projects,
            "gaps": gaps,
        }
        (args.out / "conformance-gap-register.json").write_text(
            json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        (args.out / "conformance-gap-register.md").write_text(
            render_markdown(gaps, dataset, snapshot_label), encoding="utf-8"
        )
        # A report is evidence, not an acceptance: it must never write the accepted ledger.
        # The accepted file is produced only by --accept-baseline, which names the decision.

    if args.accept_baseline:
        missing = [
            flag
            for flag, value in (
                ("--accepted-by", args.accepted_by),
                ("--approver", args.approver),
                ("--acceptance-ref", args.acceptance_ref),
                ("--delegation-ref", args.delegation_ref),
            )
            if not str(value or "").strip()
        ]
        if missing:
            parser.error("--accept-baseline requires " + ", ".join(missing))
        accepted = accepted_baseline(
            gaps,
            standard,
            dataset,
            args.snapshot,
            accepted_by=args.accepted_by,
            approver=args.approver,
            acceptance_ref=args.acceptance_ref,
            delegation_ref=args.delegation_ref,
        )
        args.baseline.parent.mkdir(parents=True, exist_ok=True)
        args.baseline.write_text(
            json.dumps(accepted, indent=2, sort_keys=False) + "\n", encoding="utf-8"
        )
        print(
            f"audit-programme-conformance: accepted {len(accepted['entries'])} entries from "
            f"{args.snapshot} into {args.baseline}"
        )

    if args.write_baseline:
        args.write_baseline.parent.mkdir(parents=True, exist_ok=True)
        baseline = baseline_entries(gaps, standard)
        # Marked, so nobody can mistake a proposal for the accepted ledger.
        baseline["status"] = "proposed"
        args.write_baseline.write_text(
            json.dumps(baseline, indent=2, sort_keys=False) + "\n", encoding="utf-8"
        )
        print(
            f"audit-programme-conformance: wrote {len(baseline['entries'])} proposed entries "
            "(not an acceptance)"
        )

    if args.strict:
        if not args.baseline.exists():
            fail(f"strict audit requires a baseline at {args.baseline}")
        baseline = read_json(args.baseline)
        fail_on = {item.strip() for item in args.fail_on.split(",") if item.strip()}
        failures = check_strict(gaps, baseline, fail_on)
        if failures:
            for failure in failures:
                print(f"FAIL {failure}", file=sys.stderr)
            sys.exit(1)
        print("audit-programme-conformance: strict audit OK")


if __name__ == "__main__":
    main()
