#!/usr/bin/env python3
"""Build `docs/design/data/coverage.json` — the measured programme coverage.

Every status is measured from a committed artefact, never guessed:

* a V1 slice (`S##`) from its plan frontmatter — `adds_tables` against `migrations/*.sql`
  (rename-aware: `procedure` -> `process`, `procedure_step` -> `process_step`), `adds_actions`
  against the compiled registry dump `docs/design/data/actions.json`, `adds_modules` against
  the tree (with an explicit, evidenced alias table for a module absorbed under another name);
* a V2 slice (`T##`) is `proposed`: no code exists, its unit is the evidence;
* a screen from `docs/design/v2.js` — the page map, the render body's `ribbon`/`wrapLegacy`,
  and every `C.action(...)` call;
* a tranche from `seed/programme.json` `meta.depends_on` (longest path), with `blocked_by`
  measured against the dependency's own coverage status.

The output is deterministic: sorted arrays, no timestamps. `--check` recomputes and compares
both committed files without writing; a drift is exit 1.

Regenerate the action dump first, then the coverage, then the seed (the seed carries each
project's measured status and tranche; the coverage itself never reads the seed's status):

    conveyor admin actions dump --json --out docs/design/data/actions.json
    python3 scripts/build-coverage.py
    python3 scripts/build-programme-seed.py
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import sys

STATUSES = ("built", "partial", "planned", "absent", "proposed", "archived", "standing")
SCREEN_STATUSES = ("live", "mixed", "proposed", "legacy")

# A declared module absorbed under another name. The evidence is the code that carries it;
# the gate verifies every path exists.
MODULE_ALIASES = {
    "realtime": ["crates/api/src/routes/events.rs", "apps/web/src/sse.rs"],
}

# A slice with no `adds_*` declarations: its deliverable is a named artefact, so presence is
# measured directly and the evidence path is checked. No status here is inferred from prose.
SLICE_EVIDENCE = {
    # T0's plan is `docs/SPEC_V2.md#12`, not a file, so the frontmatter measurement cannot see
    # its artefacts. The organisation spine merged as #65 and its issue #34 is closed; the
    # evidence is the migration and the three modules it added.
    "T0": {
        "status": "built",
        "evidence": [
            "migrations/0040_organisation.sql",
            "crates/actions/src/organisation.rs",
            "crates/domain/src/organisation.rs",
            "crates/store/src/organisation.rs",
        ],
        "gap": None,
    },
    # P0 is the programme-definition project: its plan declares no tables/actions/modules, so
    # its status is the plan's existence plus the definition set's publication (#121/#122).
    "P0": {
        "status": "planned",
        "evidence": ["docs/plans/P0-programme-definition.md"],
        "gap": "the definition set (#121) and the reuse/architecture pack (#122) are not yet authored",
    },
    "S00": {
        "status": "built",
        "evidence": ["parity/parity.yaml", "parity/SUMMARY.md"],
        "gap": None,
    },
    "S01": {
        "status": "built",
        "evidence": ["docs/decisions/0001-ui-stack.md"],
        "gap": None,
    },
    "S06a": {
        "status": "archived",
        "evidence": ["docs/plans/slices/S06a-mcp-server-channel.md"],
        "gap": "superseded by S06c/S06d",
    },
    "S12": {
        "status": "built",
        "evidence": ["docs/design/v2.js", "apps/web/src"],
        "gap": "the eight V2 destinations are their own projects, not S12 work",
    },
    "S14": {
        "status": "partial",
        "evidence": ["docs/plans/slices/S14-cutover.md"],
        "gap": "the fr/RM/Planka decommission is open (#8, #29); the app itself is deployed",
    },
    "S21": {
        "status": "partial",
        "evidence": ["docs/plans/slices/S21-message-construction.md"],
        "gap": "the v1 seam is built; the 2026-2027 design is unbuilt (#25)",
    },
}

# A page whose status is not a single ribbon/no-ribbon fact. Every non-obvious claim carries
# the evidence and the gap; the eight ribboned destinations and the legacy wrappers are
# derived from the source, not from this table.
SCREEN_OVERRIDES = {
    "portfolio": {
        "status": "mixed",
        "evidence": ["docs/design/v2.js"],
        "gap": "project rows are snapshot-filled; programmes, backlog, WBS, baseline and registers are fixture (T1)",
    },
    "plan": {
        "status": "mixed",
        "evidence": ["docs/design/v2.js"],
        "gap": "in-flight boards are live; WBS, baseline and registers are fixture (T1)",
    },
    "projects": {"status": "live", "evidence": ["docs/design/v2.js"], "gap": None},
    "contribute": {
        "status": "live",
        "evidence": ["docs/design/v2.js"],
        "gap": "read-only; claims and code stay on GitHub (#93)",
    },
    "execution": {
        "status": "live",
        "evidence": ["docs/design/v2.js"],
        "gap": None,
    },
    "documents": {
        "status": "live",
        "evidence": ["docs/design/docs.js"],
        "gap": "Markdown rendered at build time",
    },
    "architecture": {
        "status": "live",
        "evidence": ["docs/design/architecture.js"],
        "gap": "a drawn study, not a V2 aggregate",
    },
    "metering": {
        "status": "mixed",
        "evidence": ["docs/design/v2.js"],
        "gap": "aggregates are fixture (T11)",
    },
}

FRONT = re.compile(r"^---\n(.*?)\n---\n", re.S)
CREATE_TABLE = re.compile(
    r"CREATE TABLE\s+(?:IF NOT EXISTS\s+)?([a-z_][a-z0-9_]*)", re.I
)
RENAME_TABLE = re.compile(
    r"ALTER TABLE\s+(?:IF EXISTS\s+)?([a-z_][a-z0-9_]*)\s+RENAME TO\s+([a-z_][a-z0-9_]*)",
    re.I,
)
ACTION_CALL = re.compile(r'C\.action\("([a-z][a-z0-9_]*(?:\.[a-z0-9_]+)+)"')
PAGE_KEY = re.compile(r"^    ([a-z][a-z0-9_]*)\s*[:,]")


def read(path: pathlib.Path) -> str:
    return path.read_text(encoding="utf-8")


def parse_frontmatter(text: str) -> dict[str, object]:
    match = FRONT.match(text)
    if not match:
        return {}
    fields: dict[str, object] = {}
    key: str | None = None
    items: list[str] | None = None
    for raw in match.group(1).splitlines():
        line = raw.rstrip()
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        head = re.match(r"^([A-Za-z_][\w-]*):\s*(.*)$", line)
        if head:
            if key is not None and items is not None:
                fields[key] = items
            name, value = head.group(1), re.sub(r"\s+#.*$", "", head.group(2)).strip()
            if value.startswith("[") and value.endswith("]"):
                fields[name] = [
                    part.strip().strip('"').strip("'")
                    for part in value[1:-1].split(",")
                    if part.strip()
                ]
                key, items = None, None
            elif value == "":
                key, items = name, []
            else:
                fields[name] = value.strip('"').strip("'")
                key, items = None, None
        elif line.lstrip().startswith("- ") and items is not None:
            items.append(line.lstrip()[2:].strip().strip('"').strip("'"))
    if key is not None and items is not None:
        fields[key] = items
    return fields


def field_list(fields: dict[str, object], name: str) -> list[str]:
    value = fields.get(name, [])
    if isinstance(value, str):
        return [part.strip() for part in value.strip("[]").split(",") if part.strip()]
    return list(value or [])


def table_index(root: pathlib.Path) -> dict[str, str]:
    """Every table name -> the migration that created or renamed it."""
    index: dict[str, str] = {}
    for path in sorted((root / "migrations").glob("*.sql")):
        text = read(path)
        rel = path.relative_to(root).as_posix()
        for match in CREATE_TABLE.finditer(text):
            index.setdefault(match.group(1), rel)
        for match in RENAME_TABLE.finditer(text):
            index.setdefault(match.group(1), rel)
            index.setdefault(match.group(2), rel)
    return index


def module_path(root: pathlib.Path, name: str) -> str | None:
    """The first `src` tree that carries the declared module, as a repo-relative path.

    Declared modules live in any crate or app (`jobs`/`search`/`copilot` are store modules,
    `interaction` is store, `board` is actions), so the search walks every `src` tree rather
    than guessing one crate. The path it returns is the evidence the gate checks.
    """
    # A crate or app whose NAME is the module (`domain`, `store`): the crate root is the
    # evidence, not a directory under some other crate's `src`.
    for parent in (root / "crates", root / "apps"):
        candidate = parent / name
        if candidate.is_dir() and (candidate / "Cargo.toml").exists():
            return candidate.relative_to(root).as_posix()
    relative = pathlib.Path(*name.split("::"))
    bases = sorted((root / "crates").glob("*/src")) + sorted(
        (root / "apps").glob("*/src")
    )
    for base in bases:
        candidate = base / relative
        if candidate.is_dir():
            return candidate.relative_to(root).as_posix()
        file = base / (relative.as_posix() + ".rs")
        if file.exists():
            return file.relative_to(root).as_posix()
    for alias in MODULE_ALIASES.get(name, []):
        if (root / alias).exists():
            return alias
    return None


def extract_object(text: str, marker: str) -> str | None:
    start = text.find(marker)
    if start < 0:
        return None
    brace = text.find("{", start)
    if brace < 0:
        return None
    depth = 0
    for index in range(brace, len(text)):
        if text[index] == "{":
            depth += 1
        elif text[index] == "}":
            depth -= 1
            if depth == 0:
                return text[brace : index + 1]
    return None


def function_body(text: str, name: str) -> str:
    match = re.search(rf"function {re.escape(name)}\s*\(", text)
    if match is None:
        match = re.search(rf"const {re.escape(name)}\s*=", text)
        if match is None:
            return ""
    brace = text.find("{", match.end())
    if brace < 0:
        return ""
    depth = 0
    for index in range(brace, len(text)):
        if text[index] == "{":
            depth += 1
        elif text[index] == "}":
            depth -= 1
            if depth == 0:
                return text[brace : index + 1]
    return ""


def page_map(text: str) -> dict[str, str]:
    block = extract_object(text, "const pages = {")
    if block is None:
        raise SystemExit("coverage: v2.js has no `const pages = {` map")
    pages: dict[str, str] = {}
    lines = block.splitlines()
    for number, line in enumerate(lines):
        match = PAGE_KEY.match(line)
        if match is None:
            continue
        key = match.group(1)
        rest = line.split(":", 1)[1].strip() if ":" in line else ""
        if rest:
            pages[key] = rest.rstrip(",").strip()
        else:
            pages[key] = key
    return pages


def titles_map(text: str) -> dict[str, str]:
    block = extract_object(text, "const titles = {")
    if block is None:
        return {}
    titles: dict[str, str] = {}
    for match in re.finditer(r'^\s*([a-z][a-z0-9_]*):\s*"([^"]+)"', block, re.M):
        titles[match.group(1)] = match.group(2)
    return titles


def screens(root: pathlib.Path) -> tuple[list[dict], list[dict]]:
    text = read(root / "docs" / "design" / "v2.js")
    pages = page_map(text)
    titles = titles_map(text)
    rows: list[dict] = []
    for key in sorted(pages):
        expr = pages[key]
        identifier = re.match(r"^([a-z_][a-z0-9_]*)$", expr)
        body = function_body(text, identifier.group(1)) if identifier else ""
        override = SCREEN_OVERRIDES.get(key)
        if override is not None:
            status = override["status"]
            evidence = list(override["evidence"])
            gap = override["gap"]
        elif expr.startswith("wrapLegacy") or "wrapLegacy(" in body:
            status, evidence, gap = "legacy", ["docs/design/v2.js"], "V1 surface retained behind a banner"
        elif "ribbon(" in body or "consoleRibbon(" in body:
            status, evidence, gap = "proposed", ["docs/design/v2.js"], "V2 destination; no backend call"
        else:
            status, evidence, gap = "live", ["docs/design/v2.js"], None
        if status not in SCREEN_STATUSES:
            raise SystemExit(f"coverage: screen {key} has unknown status {status}")
        rows.append(
            {
                "key": key,
                "title": titles.get(key, key.replace("_", " ").title()),
                "status": status,
                "evidence": evidence,
                "gap": gap,
            }
        )
    invoked = [
        {"name": match.group(1), "line": text[: match.start()].count("\n") + 1}
        for match in ACTION_CALL.finditer(text)
    ]
    return rows, invoked


def units(project: dict, registry: dict[int, dict]) -> dict:
    numbers = sorted(set(project.get("meta", {}).get("issues", []) or []))
    counts = {"open": 0, "claimed": 0, "blocked": 0, "ready": 0, "unknown": 0}
    for number in numbers:
        issue = registry.get(number)
        if issue is None:
            counts["unknown"] += 1
            continue
        if issue.get("state") != "OPEN":
            continue
        counts["open"] += 1
        labels = {label["name"] for label in issue.get("labels", [])}
        for label, key in (
            ("status:claimed", "claimed"),
            ("status:blocked", "blocked"),
            ("status:ready", "ready"),
        ):
            if label in labels:
                counts[key] += 1
    counts["numbers"] = numbers
    return counts


def measure_slice(
    root: pathlib.Path,
    project: dict,
    tables: dict[str, str],
    actions: set[str],
) -> dict:
    key = project["key"]
    plan = project.get("meta", {}).get("plan", "")
    plan_path = root / plan if plan else None
    fields = parse_frontmatter(read(plan_path)) if plan_path and plan_path.exists() else {}
    declared_tables = field_list(fields, "adds_tables")
    declared_actions = field_list(fields, "adds_actions")
    declared_modules = field_list(fields, "adds_modules")
    total = len(declared_tables) + len(declared_actions) + len(declared_modules)
    evidence: list[str] = []
    missing: list[str] = []
    present = 0
    for table in declared_tables:
        if table in tables:
            present += 1
            evidence.append(tables[table])
        else:
            missing.append(f"table {table}")
    for action in declared_actions:
        if action in actions:
            present += 1
        else:
            missing.append(f"action {action}")
    for module in declared_modules:
        found = module_path(root, module)
        if found is not None:
            present += 1
            evidence.append(found)
        else:
            missing.append(f"module {module}")
    if total == 0:
        override = SLICE_EVIDENCE.get(key)
        if override is None:
            return {
                "status": "planned",
                "measured": {"tables": [0, 0], "actions": [0, 0], "modules": [0, 0]},
                "evidence": [plan] if plan else [],
                "gap": "no declared artefacts and no evidence row; add one",
            }
        return {
            "status": override["status"],
            "measured": {"tables": [0, 0], "actions": [0, 0], "modules": [0, 0]},
            "evidence": list(override["evidence"]),
            "gap": override["gap"],
        }
    status = "built" if present == total else ("partial" if present else "absent")
    return {
        "status": status,
        "measured": {
            "tables": [sum(1 for t in declared_tables if t in tables), len(declared_tables)],
            "actions": [sum(1 for a in declared_actions if a in actions), len(declared_actions)],
            "modules": [
                sum(1 for m in declared_modules if module_path(root, m) is not None),
                len(declared_modules),
            ],
        },
        "evidence": sorted(set(evidence))[:6],
        "gap": ", ".join(missing[:6]) if missing else None,
    }


def tranches(projects: list[dict]) -> tuple[dict[str, int], list[list[str]]]:
    keys = {project["key"] for project in projects}
    deps = {
        project["key"]: [
            dep
            for dep in project.get("meta", {}).get("depends_on", []) or []
            if dep in keys
        ]
        for project in projects
    }
    level: dict[str, int] = {}
    visiting: list[str] = []
    cycles: list[list[str]] = []

    def visit(key: str) -> int:
        if key in level:
            return level[key]
        if key in visiting:
            cycles.append(visiting[visiting.index(key) :] + [key])
            return 0
        visiting.append(key)
        value = 0 if not deps[key] else 1 + max(visit(dep) for dep in deps[key])
        visiting.pop()
        level[key] = value
        return value

    for key in sorted(keys):
        visit(key)
    return level, cycles


def build(root: pathlib.Path) -> dict:
    seed = json.loads(read(root / "seed" / "programme.json"))
    registry_path = root / "docs" / "design" / "data" / "issues.json"
    registry = {issue["number"]: issue for issue in json.loads(read(registry_path))["issues"]}
    actions_path = root / "docs" / "design" / "data" / "actions.json"
    if not actions_path.exists():
        raise SystemExit(
            "coverage: docs/design/data/actions.json is missing; run "
            "`conveyor admin actions dump --json --out docs/design/data/actions.json`"
        )
    actions = set(json.loads(read(actions_path))["actions"])
    tables = table_index(root)
    projects = seed["projects"]
    levels, cycles = tranches(projects)

    rows: list[dict] = []
    for project in sorted(projects, key=lambda p: p["key"]):
        meta = project.get("meta", {})
        track = meta.get("track", "")
        key = project["key"]
        if track == "v1" and re.match(r"^S\d{2}[a-z]?$", key):
            measured = measure_slice(root, project, tables, actions)
        elif track == "v2":
            override = SLICE_EVIDENCE.get(key)
            if override is not None:
                measured = {
                    "status": override["status"],
                    "measured": {"tables": [0, 0], "actions": [0, 0], "modules": [0, 0]},
                    "evidence": list(override["evidence"]),
                    "gap": override["gap"],
                }
            else:
                measured = {
                    "status": "proposed",
                    "measured": {"tables": [0, 0], "actions": [0, 0], "modules": [0, 0]},
                    "evidence": ["docs/SPEC_V2.md"],
                    "gap": "V2 slice unbuilt; its unit is the plan",
                }
        elif key == "SPEC-V2":
            measured = {
                "status": "built",
                "measured": {"tables": [0, 0], "actions": [0, 0], "modules": [0, 0]},
                "evidence": ["docs/SPEC_V2.md"],
                "gap": None,
            }
        elif key == "DESIGN-PUBLISH":
            measured = {
                "status": "built",
                "measured": {"tables": [0, 0], "actions": [0, 0], "modules": [0, 0]},
                "evidence": ["docs/design/README.md", "docs/design/studio.html"],
                "gap": "published by PR today; P4 makes it live from the tenant rows",
            }
        elif key == "PLATFORM":
            measured = {
                "status": "standing",
                "measured": {"tables": [0, 0], "actions": [0, 0], "modules": [0, 0]},
                "evidence": [".github/workflows/ci-public.yml", "Dockerfile"],
                "gap": "standing project; CI and platform work continues",
            }
        elif key in SLICE_EVIDENCE:
            # A cross-cutting project whose deliverable is a named artefact and whose plan
            # declares no tables/actions/modules (P0 today).
            override = SLICE_EVIDENCE[key]
            measured = {
                "status": override["status"],
                "measured": {"tables": [0, 0], "actions": [0, 0], "modules": [0, 0]},
                "evidence": list(override["evidence"]),
                "gap": override["gap"],
            }
        else:
            measured = {
                "status": "planned",
                "measured": {"tables": [0, 0], "actions": [0, 0], "modules": [0, 0]},
                "evidence": [meta.get("plan", "")] if meta.get("plan") else [],
                "gap": "unknown project shape; add an evidence row",
            }
        row = {
            "key": key,
            "name": project.get("name", key),
            "track": track,
            "plan": meta.get("plan"),
            "status": measured["status"],
            "measured": measured["measured"],
            "evidence": [item for item in measured["evidence"] if item],
            "gap": measured["gap"],
            "tranche": levels.get(key, 0),
            "units": units(project, registry),
        }
        rows.append(row)

    by_key = {row["key"]: row for row in rows}
    for row in rows:
        project = next(p for p in projects if p["key"] == row["key"])
        deps = project.get("meta", {}).get("depends_on", []) or []
        row["blocked_by"] = sorted(
            dep
            for dep in deps
            if dep in by_key and by_key[dep]["status"] not in ("built", "archived", "standing")
        )
        row["parallel_with"] = sorted(
            other["key"]
            for other in rows
            if other["key"] != row["key"] and other["tranche"] == row["tranche"]
        )

    page_rows, invoked = screens(root)
    action_names = {item["name"] for item in invoked}
    summary_projects = {
        "total": len(rows),
        "by_status": {
            status: sum(1 for row in rows if row["status"] == status)
            for status in STATUSES
        },
        "with_open_units": sum(1 for row in rows if row["units"]["open"] > 0),
        "without_open_units": sum(1 for row in rows if row["units"]["open"] == 0),
        "uncovered": sorted(
            row["key"]
            for row in rows
            if row["status"] not in ("built", "archived", "standing")
            and row["units"]["open"] == 0
        ),
    }
    summary_screens = {
        "total": len(page_rows),
        "by_status": {
            status: sum(1 for row in page_rows if row["status"] == status)
            for status in SCREEN_STATUSES
        },
        "actions_invoked": len(action_names),
    }
    return {
        "version": 1,
        "summary": {"projects": summary_projects, "screens": summary_screens},
        "cycles": cycles,
        "projects": rows,
        "screens": page_rows,
        "actions_invoked": sorted(invoked, key=lambda item: (item["name"], item["line"])),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=pathlib.Path(__file__).resolve().parents[1])
    parser.add_argument(
        "--check",
        action="store_true",
        help="recompute and compare both committed files; write nothing",
    )
    args = parser.parse_args()
    root = pathlib.Path(args.root).resolve()
    document = build(root)
    pretty = json.dumps(document, indent=2, sort_keys=True) + "\n"
    wrapper = "window.CONVEYOR_COVERAGE = " + json.dumps(
        document, sort_keys=True, separators=(",", ":")
    ) + ";\n"
    data_dir = root / "docs" / "design" / "data"
    json_path = data_dir / "coverage.json"
    js_path = data_dir / "coverage.js"
    if args.check:
        failures = 0
        for path, expected in ((json_path, pretty), (js_path, wrapper)):
            actual = read(path) if path.exists() else ""
            if actual == expected:
                print(f"coverage: {path.relative_to(root)} is current")
                continue
            failures += 1
            print(f"coverage: {path.relative_to(root)} is STALE", file=sys.stderr)
            committed = {}
            try:
                committed = json.loads(actual) if path.suffix == ".json" else {}
            except json.JSONDecodeError:
                pass
            for row in document["projects"]:
                old = next(
                    (p for p in committed.get("projects", []) if p["key"] == row["key"]),
                    None,
                )
                if old is None or old.get("status") != row["status"]:
                    print(
                        f"  {row['key']}: committed={old and old.get('status')} measured={row['status']}",
                        file=sys.stderr,
                    )
                    break
            print(
                "  regenerate with `python3 scripts/build-coverage.py`",
                file=sys.stderr,
            )
        sys.exit(1 if failures else 0)
    data_dir.mkdir(parents=True, exist_ok=True)
    json_path.write_text(pretty, encoding="utf-8")
    js_path.write_text(wrapper, encoding="utf-8")
    summary = document["summary"]
    print(
        f"coverage: {summary['projects']['total']} projects "
        f"({summary['projects']['by_status']}), "
        f"{summary['screens']['total']} screens "
        f"({summary['screens']['by_status']}), "
        f"cycles={len(document['cycles'])}, "
        f"uncovered={len(summary['projects']['uncovered'])}"
    )


if __name__ == "__main__":
    main()
