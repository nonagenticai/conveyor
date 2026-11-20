#!/usr/bin/env python3
"""Build `seed/programme.json` — the conveyor buildout programme and its projects.

The programme is the whole SPEC_V2 buildout. Every current plan is a project in it; the
already-registered GitHub issues are the units and are linked from each project's `meta.issues`.

Sources (all read-only):
  * `docs/plans/slices/S*.md` frontmatter — slice id, title, status, depends_on.
  * `docs/SPEC_V2.md` §12 when present — the T0–T17 slice list; a checked-in fallback is used
    until SPEC_V2 lands on main (issue #24).
  * `docs/design/data/issues.json` — the exported unit registry the public site ships. When it
    is present it is the ONLY issue source, so the seed and the registry agree by construction.
    `gh issue list --state all` is the fallback for a checkout that has not fetched it. An issue
    maps to a project by its `S##` label, its leading `S##`/`T##` title, or one of the explicit
    cross-cutting assignments.

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
# The buildout's own MSP definition (benefits, map, tranches, project ADM). The generator merges
# each project's ADM phase/plateau into its meta; the loader seeds the benefits/links/tranches.
DEFINITION_PATH = ROOT / "seed" / "programme-definition.json"
STANDARD_PATH = ROOT / "seed" / "output-standard.json"
CONSOLE_STANDARD_PATH = ROOT / "docs" / "design" / "data" / "output-standard.js"
# The exported registry (`scripts/fetch-issues.sh`). It is the workflow's first step, so reading
# this file — never a second live `gh` call — is what keeps the seed and the registry identical.
ISSUES_FILE = ROOT / "docs" / "design" / "data" / "issues.json"


def load_standard() -> dict:
    return json.loads(STANDARD_PATH.read_text(encoding="utf-8"))

def definition_project_meta() -> dict[str, dict[str, str]]:
    """The ADM placement per project, from the checked-in definition (empty when absent)."""
    if not DEFINITION_PATH.exists():
        return {}
    document = json.loads(DEFINITION_PATH.read_text(encoding="utf-8"))
    out: dict[str, dict[str, str]] = {}
    for key, entry in (document.get("projects") or {}).items():
        phase = (entry or {}).get("adm_phase", "")
        plateau = (entry or {}).get("plateau", "")
        if phase and plateau:
            out[key] = {"adm_phase": phase, "plateau": plateau}
    return out


def standard_class(std: dict, project: dict, measured_status: str) -> dict:
    """Resolve the project's standard class from its facts; fail closed on no match."""
    facts = {
        "key": project["key"],
        "track": project["track"],
        "status": project.get("status", ""),
        "measured_status": measured_status,
    }
    for cls in std["classes"]:
        if all(facts.get(key) == value for key, value in cls["when"].items()):
            return cls
    raise SystemExit(
        f"build-programme-seed: no output-standard class for {project['key']} "
        f"(track {project['track']!r}, measured {measured_status!r})"
    )


def render_standard_js(std: dict) -> str:
    return "window.CONVEYOR_OUTPUT_STANDARD = " + json.dumps(std, indent=2, ensure_ascii=False) + ";\n"

# The cross-cutting units that are not a `S##`/`T##` plan. Their issues are assigned by
# number — these are the public repository's numbers after the 2026-11-12 recreation.
CROSS_CUTTING = {
    "P0": {
        "name": "Programme definition — vision, blueprint, plan and project briefs",
        "plan": "docs/plans/P0-programme-definition.md",
        "track": "programme",
        # #120 is the seed/plan/gate unit; #121 the definition set; #122 the reuse map,
        # architecture/analysis pack and the per-project inception standard.
        "issues": [120, 121, 122],
    },
    "SPEC-V2": {
        "name": "SPEC V2 — the organisational operating model",
        "plan": "docs/SPEC_V2.md",
        "track": "programme",
        "issues": [24],
    },
    "DESIGN-PUBLISH": {
        "name": "Publish the SPEC_V2 studio design pack, live",
        "plan": "docs/design/STUDIO-SPEC.md",
        "track": "programme",
        # #89 (the V2 registration in the seed/index), #97 (the console's live engine proof)
        # and #103 (the measured programme truth) are the pack/console surface. #149 is the
        # work store: expected outputs and the project checklist on the same console.
        "issues": [32, 33, 36, 37, 89, 97, 103, 149],
    },
    "PLATFORM": {
        "name": "CI, runners and platform",
        "plan": "docs/plans/development-process-speed.md",
        "track": "programme",
        # #43–#46 are the estate absorptions, #55 betterleaks, #57 the programme spine,
        # #71 the inference-router cutover, #93 public collaboration, #95 the licences
        # cutover and #102 the CI action pinning.
        "issues": [17, 22, 43, 44, 45, 46, 55, 57, 71, 93, 95, 102],
    },
}

# The output standard is data, not constants: `seed/output-standard.json` owns the vocabulary
# and the per-class required sets. The builder resolves one class per project and emits
# `meta.expected_outputs` / `meta.closure_outputs`; `scripts/check-output-standard.py` is the gate.

# SPEC_V2 §12 (the dependency sentence, kept as data because prose parsing is not a parser).
# "T0 → T1 → T2 → T3; T4 after T3; T5 after T2 and V1's S16–S20; T6 after T5 and T13;
#  T7 after T2/T4 and T13; T9/T10 after T1/T3; T8 after T5; T11 continuous; T12 after all;
#  T13 after T2; T14 after T13; T15 after T13 and T6; T16 after T2 and T13; T17 after T16 and T3."
#
# P0 (the programme definition) gates every delivery slice T1–T17: the programme plan and the
# project briefs are what a delivery project is held to. T0 is the already-merged organisation
# spine — history, not a P0 dependency.
T_DEPENDS_ON = {
    "T0": [],
    "T1": ["T0", "P0"],
    "T2": ["T1", "P0"],
    "T3": ["T2", "P0"],
    "T4": ["T3", "P0"],
    "T5": ["T2", "S16", "S17", "S18", "S19", "S20", "P0"],
    "T6": ["T5", "T13", "P0"],
    "T7": ["T2", "T4", "T13", "P0"],
    "T8": ["T5", "P0"],
    "T9": ["T1", "T3", "P0"],
    "T10": ["T1", "T3", "P0"],
    "T11": ["P0"],
    "T12": [f"T{n}" for n in range(18) if n != 12] + ["P0"],
    "T13": ["T2", "P0"],
    "T14": ["T13", "P0"],
    "T15": ["T13", "T6", "P0"],
    "T16": ["T2", "T13", "P0"],
    "T17": ["T16", "T3", "P0"],
}

# SPEC_V2 §12, kept here until `docs/SPEC_V2.md` is on main (issue #24). The parser below
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
COMMENT = re.compile(r"\s+#.*$")


def parse_yaml_subset(block: str) -> dict[str, object]:
    """The slice frontmatter is a small, regular YAML subset.

    It is parsed here rather than with PyYAML so the generator has no dependency; the two
    shapes that used to corrupt `seed/programme.json` are handled explicitly: an inline
    `#` comment after a flow sequence (`S06a`) and a block sequence (`S06d`, `S19`).
    """
    fields: dict[str, object] = {}
    key: str | None = None
    items: list[str] | None = None
    for raw in block.splitlines():
        line = raw.rstrip()
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        head = re.match(r"^([A-Za-z_][\w-]*):\s*(.*)$", line)
        if head:
            if key is not None and items is not None:
                fields[key] = items
            key, value = head.group(1), COMMENT.sub("", head.group(2)).strip()
            items = None
            if value.startswith("[") and value.endswith("]"):
                fields[key] = [
                    part.strip().strip('"').strip("'")
                    for part in value[1:-1].split(",")
                    if part.strip()
                ]
                key = None
            elif value == "":
                items = []
            else:
                fields[key] = value.strip('"').strip("'")
                key = None
        elif line.lstrip().startswith("- ") and items is not None:
            items.append(line.lstrip()[2:].strip().strip('"').strip("'"))
    if key is not None and items is not None:
        fields[key] = items
    return fields


def read_frontmatter(path: Path) -> dict[str, object]:
    match = FRONTMATTER.match(path.read_text())
    if not match:
        return {}
    return parse_yaml_subset(match.group(1))


def inline_fields(path: Path, key: str) -> dict[str, object]:
    """S21 predates the frontmatter convention: its status/depends live in bold prose."""
    text = path.read_text()
    fields: dict[str, object] = {}
    status = re.search(r"^\*\*Status:\*\*\s*`([^`]+)`", text, re.M)
    if status:
        fields["status"] = status.group(1).strip()
    depends = re.search(r"^\*\*Depends on:\*\*\s*(.+)$", text, re.M)
    if depends:
        fields["depends_on"] = sorted(set(re.findall(r"S\d{2}[a-z]?", depends.group(1))))
    fields.setdefault("slice", key)
    return fields


def slice_projects() -> list[dict]:
    projects = []
    for path in sorted((ROOT / "docs/plans/slices").glob("S*.md")):
        match = SLICE_FILE.match(path.name)
        if not match:
            continue
        key = match.group(1)
        fields = read_frontmatter(path) or inline_fields(path, key)
        depends = fields.get("depends_on", [])
        if isinstance(depends, str):
            depends = [part.strip() for part in depends.strip("[]").split(",") if part.strip()]
        projects.append(
            {
                "key": fields.get("slice", key),
                "name": fields.get("title", key),
                "description": f"Plan: docs/plans/slices/{path.name}",
                "plan": f"docs/plans/slices/{path.name}",
                "track": "v1",
                "plan_status": fields.get("status", ""),
                "depends_on": depends,
                "issues": [],
            }
        )
    return projects


def t_slice_projects() -> list[dict]:
    """Prefer SPEC_V2 §12's table; fall back to the checked-in list.

    The table has three columns — key, title, gate — and the gate is the slice's acceptance
    text. Dependencies come from the §12 sentence, kept in `T_DEPENDS_ON`.
    """
    spec = ROOT / "docs/SPEC_V2.md"
    slices: list[tuple[str, str, str]] = [(code, title, "") for code, title in T_SLICES]
    if spec.exists():
        text = spec.read_text()
        section = text.split("## 12.", 1)
        if len(section) == 2:
            body = section[1].split("## 13.", 1)[0]
            parsed: list[tuple[str, str, str]] = []
            for line in body.splitlines():
                if not line.lstrip().startswith("|"):
                    continue
                cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
                if len(cells) < 3:
                    continue
                match = re.match(r"\*\*(T\d+)\*\*", cells[0])
                if not match:
                    continue
                title = cells[1].removeprefix("**")
                title = title.split("**", 1)[0]
                # Drop the slice's "— goal" dash; the table's first sentence is the title.
                title = re.sub(r"\s+", " ", title).strip()
                gate = cells[2].strip()
                parsed.append((match.group(1), title, gate))
            if parsed:
                slices = parsed
    return [
        {
            "key": code,
            "name": title,
            "description": f"SPEC_V2 slice {code} (docs/SPEC_V2.md §12)",
            "plan": "docs/SPEC_V2.md#12",
            "track": "v2",
            "plan_status": "draft",
            "depends_on": T_DEPENDS_ON.get(code, []),
            "gate": gate,
            "issues": [],
        }
        for code, title, gate in slices
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


def load_issues(path: Path | None) -> list[dict]:
    """Read the unit registry ONCE.

    An explicit `--issues-file` must exist (a typo is an error, not a silent live read). With
    no argument the exported registry is used when present; only a checkout that has never
    fetched it falls back to `gh issue list`. The workflow fetches the file immediately before
    this script, so its seed and its registry are the same read.
    """
    if path is None:
        path = ISSUES_FILE if ISSUES_FILE.exists() else None
    if path is not None:
        if not path.exists():
            sys.exit(f"build-programme-seed: issue registry {path} does not exist")
        document = json.loads(path.read_text(encoding="utf-8"))
        rows = document.get("issues")
        if not isinstance(rows, list):
            sys.exit(f"build-programme-seed: {path} has no `issues` list")
        print(f"build-programme-seed: reading {len(rows)} issues from {path}")
        return rows
    print("build-programme-seed: no exported issue registry; reading `gh issue list`")
    return gh_issues()


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


def coverage_statuses() -> dict[str, dict]:
    """The measured coverage rows, keyed by project. Absent means `status_source` falls back."""
    path = ROOT / "docs/design/data/coverage.json"
    if not path.exists():
        return {}
    document = json.loads(path.read_text())
    return {row["key"]: row for row in document.get("projects", [])}


def derive_status(project: dict, issues: list[dict], coverage: dict[str, dict]) -> str:
    """Prefer the measured status over plan prose; the GitHub issue state still decides active.

    A built/archived/standing project with no open unit is closed (or archived); any project
    with an open unit is active; a measured-unbuilt project with no unit stays draft. Without a
    coverage row the old plan-frontmatter rule applies, so the generator still works alone.
    """
    if project["key"] == "PLATFORM":
        return "active"  # a standing project: it is never "done"
    measured = coverage.get(project["key"], {}).get("status", "")
    if measured == "archived" or project.get("plan_status") == "superseded":
        return "archived"
    open_issues = [issue for issue in issues if issue["state"] == "OPEN"]
    if measured in ("built", "standing"):
        return "active" if open_issues else "closed"
    if open_issues:
        return "active"
    if measured in ("partial", "planned", "absent", "proposed"):
        return "draft"
    if project.get("plan_status", "").startswith("built") or project.get("plan_status") == "active":
        return "closed"
    return "draft"


def build(issues: list[dict]) -> dict:
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

    coverage = coverage_statuses()
    std = load_standard()
    adm = definition_project_meta()
    out_projects = []
    for project in sorted(projects, key=project_order):
        project_issues = sorted(issues_by_project.get(project["key"], []), key=lambda i: i["number"])
        numbers = [issue["number"] for issue in project_issues]
        for number in project["issues"]:
            if number not in numbers:
                numbers.append(number)
        numbers.sort()
        measured = coverage.get(project["key"], {})
        status = derive_status(project, project_issues, coverage)
        cls = standard_class(std, project, measured.get("status", ""))
        out_projects.append(
            {
                "key": project["key"],
                "name": project["name"],
                "description": project["description"],
                "status": status,
                "repo_url": REPO_URL,
                "meta": {
                    "plan": project["plan"],
                    "track": project["track"],
                    "issues": numbers,
                    "depends_on": project["depends_on"],
                    "plan_status": project.get("plan_status", ""),
                    "status_source": "measured coverage + open GitHub issues + plan frontmatter",
                    "measured_status": measured.get("status", ""),
                    "tranche": measured.get("tranche", 0),
                    "expected_outputs": list(cls["now"]),
                    "closure_outputs": list(cls["closure"]),
                    **adm.get(project["key"], {}),
                    **({"gate": project["gate"]} if project.get("gate") else {}),
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
    parser.add_argument(
        "--issues-file",
        default=None,
        help=(
            "the exported issue registry (default: docs/design/data/issues.json when present, "
            "else `gh issue list`)"
        ),
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail when the committed document differs from a fresh build (no write)",
    )
    args = parser.parse_args()

    issues = load_issues(Path(args.issues_file) if args.issues_file else None)
    programme = build(issues)
    std = load_standard()
    console_rendered = render_standard_js(std)
    out = Path(args.out)
    rendered = json.dumps(programme, indent=2, ensure_ascii=False) + "\n"
    if args.check:
        current = out.read_text() if out.exists() else ""
        if current != rendered:
            sys.exit(f"build-programme-seed: {out} is stale; re-run without --check")
        current_console = (
            CONSOLE_STANDARD_PATH.read_text() if CONSOLE_STANDARD_PATH.exists() else ""
        )
        if current_console != console_rendered:
            sys.exit(
                f"build-programme-seed: {CONSOLE_STANDARD_PATH} is stale; re-run without --check"
            )
        print(f"build-programme-seed: {out} is current")
        print(f"build-programme-seed: {CONSOLE_STANDARD_PATH} is current")
        return
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(rendered)
    CONSOLE_STANDARD_PATH.parent.mkdir(parents=True, exist_ok=True)
    CONSOLE_STANDARD_PATH.write_text(console_rendered)
    print(
        f"build-programme-seed: wrote {out} — {len(programme['projects'])} projects, "
        f"{sum(len(p['meta']['issues']) for p in programme['projects'])} issue links"
    )
    print(f"build-programme-seed: wrote {CONSOLE_STANDARD_PATH}")
    if args.write_docs:
        write_doc_headers(programme["projects"])
        print("build-programme-seed: plan headers updated")


if __name__ == "__main__":
    main()
