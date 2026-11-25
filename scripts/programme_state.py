#!/usr/bin/env python3
"""One derivation for a programme project's delivery status.

`seed/programme.json` (and therefore the tenant's project rows, the public snapshot and the
programme page) carries one status per project. Before this module there were three rules for
it — the seed generator's, the design pack's coverage measurement, and the plan frontmatter — and
they disagreed: T11's unit issue #84 is closed and merged (#404) while the seed still said
`draft` because the coverage row said `proposed`.

The precedence, highest first:

1. the coverage row says `archived`, or the plan says `superseded` → `archived`;
2. `PLATFORM` is a standing project and is never done → `active`;
3. `closure_deferred_to_tranche` is declared (a positive integer) → `active`: the programme has
   recorded that closure waits for that tranche review, and a project closed inside a planned
   tranche manufactures a `project_in_unauthorised_tranche` P0 (T19F's closure review);
4. every unit issue is closed, and there is at least one → `closed`;
5. any unit issue is open → `active`;
6. the coverage row says `built` or `standing` → `closed`;
7. the plan frontmatter says `built*` or `active` → `closed`;
8. otherwise → `draft`.

Rules 4 and 5 are the change: **GitHub issue state outranks every measurement.** The coverage row
answers "does the mock UI cover this project", not "is the work delivered", so it may never keep a
project open whose units are all closed. A plan frontmatter field never outranks a closed issue
either.

Run from any directory:

    python3 scripts/programme_state.py --self-test
    python3 scripts/programme_state.py --project T11
    python3 scripts/programme_state.py --check        # exit 1 when the seed is stale
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SEED_FILE = ROOT / "seed" / "programme.json"
ISSUES_FILE = ROOT / "docs" / "design" / "data" / "issues.json"
COVERAGE_FILE = ROOT / "docs" / "design" / "data" / "coverage.json"

STATUS_SOURCE = "github issue state > measured coverage > plan frontmatter"


def derive_status(project: dict, issues: list[dict], coverage: dict[str, dict]) -> str:
    """Return the project's delivery status. See the module docstring for the precedence."""
    key = project["key"]
    measured = coverage.get(key, {}).get("status", "")
    if measured == "archived" or project.get("plan_status") == "superseded":
        return "archived"
    if key == "PLATFORM":
        return "active"  # a standing project: it is never done
    # A deferred closure is a recorded programme decision, not a measurement. The declaration
    # travels in the seed's `meta` (from `seed/programme-definition.json`); `build-programme-seed`
    # also passes it at the top level, before the meta exists.
    deferred = project.get("closure_deferred_to_tranche")
    if deferred is None:
        deferred = (project.get("meta") or {}).get("closure_deferred_to_tranche")
    if isinstance(deferred, int) and not isinstance(deferred, bool) and deferred > 0:
        return "active"
    if issues and all(issue.get("state") != "OPEN" for issue in issues):
        return "closed"
    if any(issue.get("state") == "OPEN" for issue in issues):
        return "active"
    if measured in ("built", "standing"):
        return "closed"
    plan_status = project.get("plan_status", "")
    if plan_status.startswith("built") or plan_status == "active":
        return "closed"
    return "draft"


def load_json(path: Path, what: str) -> dict:
    if not path.exists():
        sys.exit(f"programme_state: {what} {path} does not exist")
    return json.loads(path.read_text(encoding="utf-8"))


def load_seed() -> dict:
    return load_json(SEED_FILE, "seed")


def load_issues() -> list[dict]:
    document = load_json(ISSUES_FILE, "issue registry")
    rows = document.get("issues")
    if not isinstance(rows, list):
        sys.exit(f"programme_state: {ISSUES_FILE} has no `issues` list")
    return rows


def load_coverage() -> dict[str, dict]:
    if not COVERAGE_FILE.exists():
        return {}
    document = json.loads(COVERAGE_FILE.read_text(encoding="utf-8"))
    return {row["key"]: row for row in document.get("projects", [])}


def project_units(project: dict, by_number: dict[int, dict]) -> list[dict]:
    """The registry rows the project's `meta.issues` names, in number order."""
    numbers = project.get("meta", {}).get("issues", [])
    return [by_number[number] for number in sorted(numbers) if number in by_number]


def statuses(seed: dict, issues: list[dict], coverage: dict[str, dict]) -> dict[str, str]:
    by_number = {issue["number"]: issue for issue in issues}
    return {
        project["key"]: derive_status(project, project_units(project, by_number), coverage)
        for project in seed["projects"]
    }


def self_test() -> None:
    """The precedence, case by case. A rule that is not exercised here does not exist."""
    coverage = {
        "T11": {"status": "proposed"},
        "T14": {"status": "built"},
        "S00": {"status": "archived"},
    }

    def project(key: str, plan_status: str = "") -> dict:
        return {"key": key, "plan_status": plan_status}

    closed = [{"number": 84, "state": "CLOSED"}]
    opened = [{"number": 77, "state": "OPEN"}, {"number": 76, "state": "CLOSED"}]

    cases = [
        # A closed issue outranks a stale measurement — the T11 defect this module exists for.
        ("closed units beat a proposed measurement", project("T11"), closed, "closed"),
        ("an open unit keeps the project active", project("T5"), opened, "active"),
        ("no units and a built measurement", project("T14"), [], "closed"),
        ("no units and a proposed measurement", project("T15"), [], "draft"),
        ("archived coverage wins", project("S00"), closed, "archived"),
        ("PLATFORM stands forever", project("PLATFORM"), closed, "active"),
        # T19F: a closure the programme deferred to the tranche-6 review stays active even
        # though every unit issue is closed (a closed project in a planned tranche is a P0).
        (
            "a declared closure deferral keeps the project active",
            {**project("T19F"), "closure_deferred_to_tranche": 6},
            closed,
            "active",
        ),
        (
            "the deferral reads from the seed meta too",
            {**project("T19F"), "meta": {"closure_deferred_to_tranche": 6}},
            closed,
            "active",
        ),
        (
            "archived beats a deferral",
            {**project("S00"), "closure_deferred_to_tranche": 6},
            closed,
            "archived",
        ),
        ("plan frontmatter is the last resort", project("X", "built-steps-1-3"), [], "closed"),
    ]
    failures = 0
    for name, proj, units, expected in cases:
        actual = derive_status(proj, units, coverage)
        if actual != expected:
            failures += 1
            print(f"FAIL {name}: expected {expected!r}, got {actual!r}")
    if failures:
        sys.exit(1)
    print(f"programme_state: self-test passed ({len(cases)} cases)")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run the precedence cases")
    parser.add_argument("--project", metavar="KEY", help="print one project's derived status")
    parser.add_argument(
        "--check",
        action="store_true",
        help="compare the seed's stored statuses with a fresh derivation; exit 1 on drift",
    )
    args = parser.parse_args()

    if args.self_test:
        self_test()
        return

    seed = load_seed()
    issues = load_issues()
    coverage = load_coverage()

    if args.project:
        by_number = {issue["number"]: issue for issue in issues}
        for project in seed["projects"]:
            if project["key"] == args.project:
                print(derive_status(project, project_units(project, by_number), coverage))
                return
        sys.exit(f"programme_state: no project {args.project!r} in the seed")

    if args.check:
        fresh = statuses(seed, issues, coverage)
        stale = [
            (project["key"], project["status"], fresh[project["key"]])
            for project in seed["projects"]
            if project["status"] != fresh.get(project["key"])
        ]
        for key, stored, derived in stale:
            print(f"{key}: seed={stored} derived={derived}")
        if stale:
            sys.exit(f"programme_state: {len(stale)} stale status(es); regenerate the seed")
        print(f"programme_state: {len(fresh)} statuses agree with the derivation")
        return

    parser.print_help()


if __name__ == "__main__":
    main()
