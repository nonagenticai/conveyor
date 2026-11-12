#!/usr/bin/env python3
"""Build `seed/programme.json` — the conveyor buildout programme and its projects.

The programme is the whole SPEC_V2 buildout. Every current plan is a project in it; the
already-registered GitHub issues are the units and are linked from each project's `meta.issues`.

Sources (all read-only):
  * `docs/plans/slices/S*.md` frontmatter — slice id, title, status, depends_on.
  * `docs/SPEC_V2.md` §12 when present — the T0–T17 slice list; a checked-in fallback is used
    until SPEC_V2 lands on main (issue #38).
  * `gh issue list --state all` — the unit registry. An issue maps to a project by its `S##`
    label, its leading `S##`/`T##` title, or one of the explicit cross-cutting assignments.

Run from any directory:

    python3 scripts/build-programme-seed.py            # writes seed/programme.json
    python3 scripts/build-programme-seed.py --write-docs  # also adds the plan→project header

The output is deterministic: projects are sorted by key, issue numbers ascending, two-space
indent, trailing newline. Re-running against unchanged inputs produces an identical file.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROGRAMME_NAME = "conveyor buildout"
PROGRAMME_KEY = "conveyor-buildout"
REPO_URL = "https://github.com/nonagenticai/conveyor"

# The cross-cutting units that are not a `S##`/`T##` plan. Their issues are assigned by number.
CROSS_CUTTING = {
    "SPEC-V2": {
        "name": "SPEC V2 — the organisational operating model",
        "plan": "docs/SPEC_V2.md",
        "track": "programme",
        "issues": [38],
    },
    "DESIGN-PUBLISH": {
        "name": "Publish the SPEC_V2 studio design pack, live",
        "plan": "docs/design/STUDIO-SPEC.md",
        "track": "programme",
        "issues": [49, 51],
    },
    "PLATFORM": {
        "name": "CI, runners and platform",
        "plan": "docs/plans/development-process-speed.md",
        "track": "programme",
        "issues": [27, 32],
    },
}

# SPEC_V2 §12, kept here until `docs/SPEC_V2.md` is on main (issue #38). The parser below
# prefers the live table; this list is the deterministic fallback.
T_SLICES = [
    ("T0", "Model and contracts"),
    ("T1", "Plan spine"),
    ("T2", "Decision ladder"),
    ("T3", "PM job"),
    ("T4", "Kanban coach"),
    ("T5", "Kata"),
    ("T6", "Development project"),
    ("T7", "Operations"),
    ("T8", "Organisational memory"),
    ("T9", "TOGAF profile"),
    ("T10", "SAFe cadence"),
    ("T11", "Metrics, metering and maturity"),
    ("T12", "One organisation loop"),
    ("T13", "Tool platform"),
    ("T14", "Estate toolkits"),
    ("T15", "Tool authoring and BYO"),
    ("T16", "SOP, RDF/OWL and process-document ingestion"),
    ("T17", "Tenant asset authoring"),
]

FRONTMATTER = re.compile(r"^---\n(.*?)\n---\n", re.S)
SLICE_FILE = re.compile(r"^(S\d{2}[a-z]?)-.+\.md$")


def read_frontmatter(path: Path) -> dict[str, str]:
    match = FRONTMATTER.match(path.read_text())
    if not match:
        return {}
    fields: dict[str, str] = {}
    for line in match.group(1).splitlines():
        if ":" in line and not line.startswith(" "):
            key, value = line.split(":", 1)
            fields[key.strip()] = value.strip().strip('"')
    return fields


def slice_projects() -> list[dict]:
    projects = []
    for path in sorted((ROOT / "docs/plans/slices").glob("S*.md")):
        match = SLICE_FILE.match(path.name)
        if not match:
            continue
        fields = read_frontmatter(path)
        key = fields.get("slice", match.group(1))
        projects.append(
            {
                "key": key,
                "name": fields.get("title", key),
                "description": f"Plan: docs/plans/slices/{path.name}",
                "plan": f"docs/plans/slices/{path.name}",
                "track": "v1",
                "plan_status": fields.get("status", ""),
                "depends_on": [
                    value.strip()
                    for value in fields.get("depends_on", "[]").strip("[]").split(",")
                    if value.strip()
                ],
                "issues": [],
            }
        )
    return projects


def t_slice_projects() -> list[dict]:
    """Prefer SPEC_V2 §12's table; fall back to the checked-in list."""
    spec = ROOT / "docs/SPEC_V2.md"
    slices = list(T_SLICES)
    if spec.exists():
        text = spec.read_text()
        section = text.split("## 12.", 1)
        if len(section) == 2:
            body = section[1].split("## 13.", 1)[0]
            parsed = re.findall(r"\|\s*\*\*(T\d+)\*\*\s*\|\s*\*\*(.+?)\*\*", body)
            if parsed:
                slices = [(code, re.sub(r"\s+", " ", title).strip()) for code, title in parsed]
    return [
        {
            "key": code,
            "name": title,
            "description": f"SPEC_V2 slice {code} (docs/SPEC_V2.md §12)",
            "plan": "docs/SPEC_V2.md#12",
            "track": "v2",
            "plan_status": "draft",
            "depends_on": [],
            "issues": [],
        }
        for code, title in slices
    ]


def gh_issues() -> list[dict]:
    try:
        raw = subprocess.run(
            [
                "gh",
                "issue",
                "list",
                "--state",
                "all",
                "--limit",
                "500",
                "--json",
                "number,title,state,labels",
            ],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    except (subprocess.CalledProcessError, FileNotFoundError) as error:
        sys.exit(f"build-programme-seed: gh issue list failed: {error}")
    return json.loads(raw)


def explicit_project(issue_number: int) -> str | None:
    """A cross-cutting issue belongs to exactly one project, by number."""
    for key, crossed in CROSS_CUTTING.items():
        if issue_number in crossed["issues"]:
            return key
    return None


def project_for_issue(issue: dict, projects_by_key: dict[str, dict]) -> str | None:
    explicit = explicit_project(issue["number"])
    if explicit is not None:
        return explicit
    labels = [label["name"] for label in issue.get("labels", [])]
    for label in labels:
        if label in projects_by_key:
            return label
    match = re.match(r"\s*(S\d{2}[a-z]?|T\d{1,2})\b", issue["title"])
    if match and match.group(1) in projects_by_key:
        return match.group(1)
    return None


def derive_status(project: dict, issues: list[dict]) -> str:
    if project["key"] == "PLATFORM":
        return "active"  # a standing project: it is never "done"
    open_issues = [issue for issue in issues if issue["state"] == "OPEN"]
    if open_issues:
        return "active"
    if project.get("plan_status", "").startswith("built") or project.get("plan_status") == "active":
        return "closed"
    return "draft"


def build() -> dict:
    projects = slice_projects() + t_slice_projects()
    projects_by_key = {project["key"]: project for project in projects}

    for key, crossed in CROSS_CUTTING.items():
        project = {
            "key": key,
            "name": crossed["name"],
            "description": f"Programme-level unit (plan: {crossed['plan']})",
            "plan": crossed["plan"],
            "track": crossed["track"],
            "plan_status": "active",
            "depends_on": [],
            "issues": list(crossed["issues"]),
        }
        projects.append(project)
        projects_by_key[key] = project

    issues = gh_issues()
    issues_by_number = {issue["number"]: issue for issue in issues}
    issues_by_project: dict[str, list[dict]] = {key: [] for key in projects_by_key}
    unmapped: list[int] = []
    for issue in issues:
        key = project_for_issue(issue, projects_by_key)
        if key is None:
            # A closed historical issue stays in the registry (`issues.json`) but not in a
            # project: only current work belongs in the programme's issue links.
            if issue["state"] == "OPEN":
                key = "PLATFORM"
                unmapped.append(issue["number"])
            else:
                continue
        issues_by_project[key].append(issue)

    # A cross-cutting project may name an issue that has no label of its own.
    for key, project in projects_by_key.items():
        for number in project["issues"]:
            issue = issues_by_number.get(number)
            if issue is not None and issue not in issues_by_project[key]:
                issues_by_project[key].append(issue)

    def project_order(project: dict) -> tuple:
        match = re.match(r"([ST])(\d+)([a-z]?)", project["key"])
        if match:
            return (match.group(1), int(match.group(2)), match.group(3))
        return ("Z", 0, project["key"])

    out_projects = []
    for project in sorted(projects, key=project_order):
        project_issues = sorted(issues_by_project.get(project["key"], []), key=lambda i: i["number"])
        numbers = [issue["number"] for issue in project_issues]
        for number in project["issues"]:
            if number not in numbers:
                numbers.append(number)
        numbers.sort()
        out_projects.append(
            {
                "key": project["key"],
                "name": project["name"],
                "description": project["description"],
                "status": derive_status(project, project_issues),
                "repo_url": REPO_URL,
                "meta": {
                    "plan": project["plan"],
                    "track": project["track"],
                    "issues": numbers,
                    "depends_on": project["depends_on"],
                    "plan_status": project.get("plan_status", ""),
                    "status_source": "open GitHub issues + plan frontmatter",
                },
            }
        )

    if unmapped:
        print(f"build-programme-seed: {len(unmapped)} unmapped issue(s) → PLATFORM: {unmapped}")

    return {
        "version": 1,
        "programme": {"name": PROGRAMME_NAME, "status": "active"},
        "projects": out_projects,
    }


def write_doc_headers(projects: list[dict]) -> None:
    """Add one idempotent `Programme project:` line to every S## plan."""
    for project in projects:
        if not project["key"].startswith("S"):
            continue
        path = ROOT / project["meta"]["plan"]
        if not path.exists():
            continue
        text = path.read_text()
        line = (
            f"Programme project: {project['key']} · issues "
            + (", ".join(f"#{n}" for n in project["meta"]["issues"]) if project["meta"]["issues"] else "none yet")
        )
        if "Programme project:" in text:
            text = re.sub(r"^Programme project:.*$", line, text, count=1, flags=re.M)
        else:
            # Insert after the first markdown heading, before the next blank line.
            text = re.sub(r"^(# .*\n)", rf"\1\n{line}\n", text, count=1, flags=re.M)
        path.write_text(text)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--write-docs", action="store_true", help="also write the plan→project header")
    parser.add_argument("--out", default=str(ROOT / "seed/programme.json"))
    args = parser.parse_args()

    programme = build()
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(programme, indent=2, ensure_ascii=False) + "\n")
    print(
        f"build-programme-seed: wrote {out} — {len(programme['projects'])} projects, "
        f"{sum(len(p['meta']['issues']) for p in programme['projects'])} issue links"
    )
    if args.write_docs:
        write_doc_headers(programme["projects"])
        print("build-programme-seed: plan headers updated")


if __name__ == "__main__":
    main()
