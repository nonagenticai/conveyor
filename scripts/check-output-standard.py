#!/usr/bin/env python3
"""Validate the programme output standard, the seed's completeness meta and the console copy.

`seed/output-standard.json` is the one vocabulary and the per-project-class required sets.
`scripts/build-programme-seed.py` reads it, emits `meta.expected_outputs` / `meta.closure_outputs`
and writes `docs/design/data/output-standard.js` for the console. This gate refuses drift:

  * an unknown category, alias target or matching fact;
  * a project whose resolved class or seed meta disagrees with the standard;
  * a class that matches no project;
  * a category with no template, an empty or duplicate required heading, a required heading
    absent from the skeleton, or a URL in a skeleton;
  * a console copy that is not byte-for-byte the standard's JSON.

Run it before every push: `python3 scripts/check-output-standard.py`. `--self-test` proves the
gate refuses each failure shape; `--check-copy` checks only the generated console copy.
"""

from __future__ import annotations

import argparse
import copy
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STANDARD_PATH = ROOT / "seed" / "output-standard.json"
SEED_PATH = ROOT / "seed" / "programme.json"
CONSOLE_PATH = ROOT / "docs" / "design" / "data" / "output-standard.js"
FACTS = {"key", "track", "status", "measured_status"}
ON_DEMAND = "decision"
FENCE = re.compile(r"^\s*(```+|~~~+)")
ATX = re.compile(r"^(#{1,6})\s+(.*?)\s*#*\s*$")


def heading_key(value: str) -> str:
    """Case-insensitive, whitespace-collapsed: the action layer compares the same way."""
    return " ".join(value.split()).strip().lower()


def headings(markdown: str) -> set[str]:
    """ATX headings outside fenced code, normalised."""
    found: set[str] = set()
    fence: str | None = None
    for line in markdown.splitlines():
        opener = FENCE.match(line)
        if opener:
            marker = opener.group(1)[0]
            if fence is None:
                fence = marker
            elif fence == marker:
                fence = None
            continue
        if fence is not None:
            continue
        match = ATX.match(line)
        if match:
            found.add(heading_key(match.group(2)))
    return found


def read_json(path: Path) -> dict:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        raise SystemExit(f"check-output-standard: {path} is missing")
    except json.JSONDecodeError as error:
        raise SystemExit(f"check-output-standard: {path} is not JSON: {error}")


def console_standard(path: Path) -> dict:
    try:
        text = path.read_text(encoding="utf-8")
    except FileNotFoundError:
        raise SystemExit(f"check-output-standard: {path} is missing")
    match = re.search(r"window\.CONVEYOR_OUTPUT_STANDARD\s*=\s*(\{.*\});\s*$", text, re.S)
    if not match:
        raise SystemExit(f"check-output-standard: {path} has no CONVEYOR_OUTPUT_STANDARD assignment")
    try:
        return json.loads(match.group(1))
    except json.JSONDecodeError as error:
        raise SystemExit(f"check-output-standard: {path} is not JSON: {error}")


def facts(project: dict) -> dict:
    meta = project.get("meta") or {}
    return {
        "key": project.get("key", ""),
        "track": meta.get("track", ""),
        "status": project.get("status", ""),
        "measured_status": meta.get("measured_status", ""),
    }


def resolve(std: dict, project: dict) -> dict | None:
    project_facts = facts(project)
    for cls in std["classes"]:
        if all(project_facts.get(key) == value for key, value in cls["when"].items()):
            return cls
    return None


def validate(std: dict, seed: dict, console: dict) -> list[str]:
    errors: list[str] = []
    if std.get("version") != 2:
        errors.append(f"standard version is {std.get('version')!r}, expected 2")

    categories = std.get("categories")
    if not isinstance(categories, list) or not categories:
        errors.append("categories must be a non-empty list")
        categories = []
    if len(categories) != len(set(categories)):
        errors.append("categories contains a duplicate")
    for category in categories:
        if not isinstance(category, str) or not category:
            errors.append(f"invalid category {category!r}")

    aliases = std.get("aliases")
    if not isinstance(aliases, dict):
        errors.append("aliases must be an object")
        aliases = {}
    for alias, target in aliases.items():
        if alias in categories:
            errors.append(f"alias {alias!r} shadows a canonical category")
        if target not in categories:
            errors.append(f"alias {alias!r} points at unknown category {target!r}")
        if target in aliases:
            errors.append(f"alias {alias!r} chains through alias {target!r}")

    classes = std.get("classes")
    if not isinstance(classes, list) or not classes:
        errors.append("classes must be a non-empty list")
        classes = []
    names = [cls.get("name") for cls in classes]
    if len(names) != len(set(names)):
        errors.append("classes contains a duplicate name")
    for cls in classes:
        name = cls.get("name", "?")
        when = cls.get("when")
        if not isinstance(when, dict) or not when:
            errors.append(f"class {name!r} has no matching facts")
        else:
            unknown = set(when) - FACTS
            if unknown:
                errors.append(f"class {name!r} matches unknown facts {sorted(unknown)}")
        for field in ("now", "closure"):
            values = cls.get(field)
            if not isinstance(values, list):
                errors.append(f"class {name!r} {field} must be a list")
                continue
            if len(values) != len(set(values)):
                errors.append(f"class {name!r} {field} contains a duplicate")
            for category in values:
                if category not in categories:
                    errors.append(f"class {name!r} {field} has unknown category {category!r}")
                if category == ON_DEMAND:
                    errors.append(f"class {name!r} {field} requires {ON_DEMAND!r}, which is on demand only")
        now = cls.get("now") or []
        closure = cls.get("closure") or []
        if set(now) & set(closure):
            errors.append(f"class {name!r} repeats a category in now and closure")

    templates = std.get("templates")
    if not isinstance(templates, dict):
        errors.append("templates must be an object keyed by category")
        templates = {}
    for unknown in sorted(set(templates) - set(categories)):
        errors.append(f"template for unknown category {unknown!r}")
    for category in categories:
        template = templates.get(category)
        if not isinstance(template, dict):
            errors.append(f"category {category!r} has no template")
            continue
        required = template.get("required")
        optional = template.get("optional", [])
        if not isinstance(required, list) or not required:
            errors.append(f"template {category!r} required must be a non-empty list")
            required = []
        if not isinstance(optional, list):
            errors.append(f"template {category!r} optional must be a list")
            optional = []
        seen: set[str] = set()
        for heading in required:
            if not isinstance(heading, str) or not heading.strip():
                errors.append(f"template {category!r} has an empty required heading")
                continue
            key = heading_key(heading)
            if key in seen:
                errors.append(f"template {category!r} repeats required heading {heading!r}")
            seen.add(key)
        optional_keys = {heading_key(h) for h in optional if isinstance(h, str)}
        if seen & optional_keys:
            errors.append(f"template {category!r} lists a heading as both required and optional")
        skeleton = template.get("skeleton")
        if not isinstance(skeleton, str) or not skeleton.strip():
            errors.append(f"template {category!r} has no skeleton")
            continue
        present = headings(skeleton)
        for heading in required:
            if isinstance(heading, str) and heading_key(heading) not in present:
                errors.append(
                    f"template {category!r} skeleton lacks required heading {heading!r}"
                )
        if "://" in skeleton or "www." in skeleton.lower():
            errors.append(f"template {category!r} skeleton carries a URL")

    if not isinstance(seed.get("projects"), list) or not seed["projects"]:
        errors.append("seed has no projects")
        projects = []
    else:
        projects = seed["projects"]
    matched: set[str] = set()
    for project in projects:
        key = project.get("key", "?")
        cls = resolve(std, project)
        if cls is None:
            errors.append(f"project {key!r} matches no class")
            continue
        matched.add(cls["name"])
        meta = project.get("meta") or {}
        if meta.get("expected_outputs") != cls.get("now"):
            errors.append(
                f"project {key!r} expected_outputs {meta.get('expected_outputs')!r} "
                f"!= class {cls['name']!r} now {cls.get('now')!r}"
            )
        if meta.get("closure_outputs") != cls.get("closure"):
            errors.append(
                f"project {key!r} closure_outputs {meta.get('closure_outputs')!r} "
                f"!= class {cls['name']!r} closure {cls.get('closure')!r}"
            )
    for cls in classes:
        if cls.get("name") not in matched:
            errors.append(f"class {cls.get('name')!r} matches no project")

    # Every category is either required by some class or explicitly on demand. A new category
    # that fits nowhere is a vocabulary orphan, not a checklist slot.
    required: set[str] = set()
    for cls in classes:
        required.update(cls.get("now") or [])
        required.update(cls.get("closure") or [])
    orphans = set(categories) - required - {ON_DEMAND}
    if orphans:
        errors.append(f"categories covered by no class and not on demand: {sorted(orphans)}")

    if console != std:
        errors.append("docs/design/data/output-standard.js does not equal seed/output-standard.json")

    return errors


def self_test(std: dict, seed: dict, console: dict) -> int:
    cases: list[tuple[str, dict, dict, dict]] = []

    bad = copy.deepcopy(std)
    bad["classes"][0]["now"].append("not-a-category")
    cases.append(("unknown category in a set", bad, seed, console))

    bad = copy.deepcopy(std)
    bad["aliases"]["brief"] = "not-a-category"
    cases.append(("alias to an unknown category", bad, seed, console))

    bad = copy.deepcopy(seed)
    bad["projects"][0].setdefault("meta", {})["expected_outputs"] = ["not-a-category"]
    cases.append(("seed disagrees with its class", std, bad, console))

    bad = copy.deepcopy(console)
    bad["categories"].append("not-a-category")
    cases.append(("console copy drifts", std, seed, bad))

    bad = copy.deepcopy(std)
    bad["templates"].pop("vision")
    cases.append(("a category without a template", bad, seed, console))

    bad = copy.deepcopy(std)
    bad["templates"]["vision"]["required"] = []
    cases.append(("an empty required list", bad, seed, console))

    bad = copy.deepcopy(std)
    bad["templates"]["vision"]["required"].append(bad["templates"]["vision"]["required"][0])
    cases.append(("a duplicate required heading", bad, seed, console))

    bad = copy.deepcopy(std)
    bad["templates"]["vision"]["skeleton"] = "# Vision\n\n## Purpose\n"
    bad["templates"]["vision"]["required"].append("A heading not in the skeleton")
    cases.append(("a required heading absent from the skeleton", bad, seed, console))

    bad = copy.deepcopy(std)
    bad["templates"]["vision"]["skeleton"] += "\nSee https://example.com for the method.\n"
    cases.append(("a URL in the skeleton", bad, seed, console))

    failures = 0
    for label, case_std, case_seed, case_console in cases:
        errors = validate(case_std, case_seed, case_console)
        if errors:
            print(f"check-output-standard: self-test refused {label}")
        else:
            failures += 1
            print(f"check-output-standard: self-test FAILED to refuse {label}")
    print(f"check-output-standard: self-test {len(cases) - failures}/{len(cases)} refusals")
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--check-copy", action="store_true")
    args = parser.parse_args()

    std = read_json(STANDARD_PATH)
    seed = read_json(SEED_PATH)
    console = console_standard(CONSOLE_PATH)

    if args.self_test:
        return self_test(std, seed, console)

    if args.check_copy:
        if console != std:
            print("check-output-standard: console copy differs from seed/output-standard.json")
            return 1
        print("check-output-standard: console copy is current")
        return 0

    errors = validate(std, seed, console)
    if errors:
        for error in errors:
            print(f"check-output-standard: ERROR: {error}")
        return 1
    print(
        f"check-output-standard: {len(seed['projects'])} projects, "
        f"{len(std['classes'])} classes, {len(std['categories'])} categories"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
