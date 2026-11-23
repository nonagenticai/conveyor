#!/usr/bin/env python3
"""Check the standards-coverage map (WP11): every audit rule and output category is covered.

`governance/standards-coverage.json` is the chain between a governance rule and the thing that
proves it: the output-standard category its artefact is rendered in, the snapshot collection (or
live action) the rule reads, and the check that refuses a gap — a **registry** name (a
`scripts/verify-project.py` CHECKS entry or a `governance/conformance.json` rule) or a
**manifest clause** (the `<project>-<clause>` id every `acceptance/programme/<key>.toml` check
carries).

The file's `categories` object carries the same chain for every output-standard **category**, so
the "all standards are covered" claim holds for the whole output standard and not only where a
rule happens to name a category. A category's check names the committed record's own verifier,
`generate-standards.py --check#<category>` — never an invented instrument.

The checker refuses exactly what the map exists to prevent:

  - an **orphan rule**: a rule in the matrix with no coverage entry, or an entry for no rule;
  - an **unmapped category**: an output-standard category with no `categories` entry, an entry
    for no category, a standard/spec that differs from its template, or a check that is not
    `generate-standards.py --check#<category>`;
  - an **unknown verifier**: a registry name in neither registry, or a clause no manifest carries;
  - a **source absent from the schema it claims**: a committed snapshot pointer that does not
    resolve, an action the registry dump does not serve, a live-only projection that is marked
    so but already resolves (the flag must not outlive its absence), or a live-only source with
    no stated reason;
  - a standard/spec mismatch against the matrix, and a category the output standard does not
    define (a non-SPEC rule must name at least one).

Usage:
    python3 scripts/check-standards-coverage.py [--check]
    python3 scripts/check-standards-coverage.py --self-test
"""

from __future__ import annotations

import argparse
import ast
import copy
import json
import sys
import tomllib
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
COVERAGE = ROOT / "governance" / "standards-coverage.json"
STANDARD = ROOT / "governance" / "conformance.json"
OUTPUT_STANDARD = ROOT / "seed" / "output-standard.json"
SNAPSHOT = ROOT / "docs" / "design" / "data"
ACTIONS = ROOT / "docs" / "design" / "data" / "actions.json"
MANIFESTS = ROOT / "acceptance" / "programme"
VERIFY_PROJECT = ROOT / "scripts" / "verify-project.py"


def read_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def pointer(document: Any, path: str) -> Any:
    """Resolve a JSON pointer; a missing member resolves to `None`."""
    if not path or path == "/":
        return document
    if not path.startswith("/"):
        raise ValueError(f"{path!r} is not a JSON pointer")
    value = document
    for part in path[1:].split("/"):
        part = part.replace("~1", "/").replace("~0", "~")
        if isinstance(value, list):
            try:
                value = value[int(part)]
            except (ValueError, IndexError):
                return None
        elif isinstance(value, dict):
            if part not in value:
                return None
            value = value[part]
        else:
            return None
    return value


def registry_names(standard: dict[str, Any]) -> set[str]:
    """The two registries a verifier may name: verify-project's CHECKS and the rule matrix."""
    names = set((standard.get("rules") or {}).keys())
    tree = ast.parse(VERIFY_PROJECT.read_text(encoding="utf-8"))
    for node in ast.walk(tree):
        target = None
        if isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name):
            target = node.target.id
        elif isinstance(node, ast.Assign) and len(node.targets) == 1 and isinstance(node.targets[0], ast.Name):
            target = node.targets[0].id
        if target != "CHECKS":
            continue
        value = node.value
        if not isinstance(value, ast.Dict):
            raise ValueError("scripts/verify-project.py CHECKS is not a literal dict")
        for key in value.keys:
            if not isinstance(key, ast.Constant) or not isinstance(key.value, str):
                raise ValueError("scripts/verify-project.py CHECKS has a non-string key")
            names.add(key.value)
    return names


def manifest_clauses(directory: Path = MANIFESTS) -> set[str]:
    """The clause suffixes the project manifests carry (`<project>-<clause>`)."""
    clauses: set[str] = set()
    for path in sorted(directory.glob("*.toml")):
        stem = path.stem
        document = tomllib.loads(path.read_text(encoding="utf-8"))
        for entry in document.get("check") or []:
            identifier = str(entry.get("id") or "")
            if identifier.startswith(f"{stem}-"):
                clauses.add(identifier[len(stem) + 1:])
    return clauses


def action_names(path: Path = ACTIONS) -> set[str]:
    document = read_json(path)
    rows = document.get("actions") if isinstance(document, dict) else document
    return {str(row) for row in rows or []}


def check(
    coverage: dict[str, Any],
    standard: dict[str, Any],
    output_standard: dict[str, Any],
    snapshot: Path = SNAPSHOT,
    actions: set[str] | None = None,
    registry: set[str] | None = None,
    clauses: set[str] | None = None,
) -> list[str]:
    problems: list[str] = []
    rules = standard.get("rules") or {}
    templates = output_standard.get("templates") or {}
    template_categories = set(templates)
    actions = actions if actions is not None else action_names()
    registry = registry if registry is not None else registry_names(standard)
    clauses = clauses if clauses is not None else manifest_clauses()

    if coverage.get("version") != 1:
        problems.append("coverage version must be 1")
    entries = coverage.get("entries")
    if not isinstance(entries, dict) or not entries:
        return problems + ["coverage.entries must be a non-empty object"]
    missing = sorted(set(rules) - set(entries))
    unknown = sorted(set(entries) - set(rules))
    if missing:
        problems.append(f"rule(s) with no coverage entry: {missing}")
    if unknown:
        problems.append(f"coverage entr(y/ies) for no rule: {unknown}")

    for name in sorted(set(entries) & set(rules)):
        entry = entries[name]
        rule = rules[name]
        if not isinstance(entry, dict):
            problems.append(f"{name}: the entry is not an object")
            continue
        if entry.get("standard") != rule.get("standard"):
            problems.append(
                f"{name}: standard {entry.get('standard')!r} != matrix {rule.get('standard')!r}"
            )
        if entry.get("spec") != rule.get("spec"):
            problems.append(f"{name}: spec {entry.get('spec')!r} != matrix {rule.get('spec')!r}")
        categories = entry.get("categories")
        if not isinstance(categories, list):
            problems.append(f"{name}: categories must be a list")
        else:
            for category in categories:
                if category not in template_categories:
                    problems.append(f"{name}: unknown category {category!r}")
            if not categories and rule.get("standard") != "SPEC":
                problems.append(f"{name}: a non-SPEC rule must name at least one category")
        source = entry.get("source")
        if not isinstance(source, dict):
            problems.append(f"{name}: source must be an object")
        elif "action" in source:
            if str(source["action"]) not in actions:
                problems.append(f"{name}: unknown live action {source['action']!r}")
        elif "file" in source:
            file = str(source.get("file") or "")
            path = str(source.get("pointer") or "/")
            document = None
            target = snapshot / file
            if not file or not target.is_file():
                problems.append(f"{name}: snapshot file {file!r} does not exist")
            else:
                try:
                    document = pointer(read_json(target), path)
                except (ValueError, json.JSONDecodeError) as error:
                    problems.append(f"{name}: source {file}#{path} is unreadable: {error}")
            if source.get("available") == "live":
                if not str(source.get("reason") or "").strip():
                    problems.append(f"{name}: a live-only source must state its reason")
                if document is not None:
                    problems.append(
                        f"{name}: {file}#{path} is marked live-only but resolves in the "
                        "committed pack; remove the flag"
                    )
            elif document is None:
                problems.append(f"{name}: source {file}#{path} does not resolve in the snapshot")
        else:
            problems.append(f"{name}: source names neither a file+pointer nor an action")
        verifier = entry.get("verifier")
        if not isinstance(verifier, dict):
            problems.append(f"{name}: verifier must be an object")
            continue
        kind = verifier.get("kind")
        if kind == "registry":
            if str(verifier.get("name")) not in registry:
                problems.append(f"{name}: unknown registry check {verifier.get('name')!r}")
        elif kind == "manifest":
            if str(verifier.get("clause")) not in clauses:
                problems.append(f"{name}: unknown manifest clause {verifier.get('clause')!r}")
        else:
            problems.append(f"{name}: verifier kind must be registry or manifest")

    # Every output-standard category is covered in its own right; the rule map above only proves
    # the categories some rule happens to name.
    category_map = coverage.get("categories")
    if not isinstance(category_map, dict) or not category_map:
        problems.append("coverage.categories must be a non-empty object")
        category_map = {}
    unmapped = sorted(template_categories - set(category_map))
    unknown_categories = sorted(set(category_map) - template_categories)
    if unmapped:
        problems.append(f"output-standard categor(y/ies) with no coverage entry: {unmapped}")
    if unknown_categories:
        problems.append(
            f"category coverage entr(y/ies) for no output-standard category: {unknown_categories}"
        )
    for name in sorted(set(category_map) & template_categories):
        entry = category_map[name]
        template = templates[name] if isinstance(templates[name], dict) else {}
        if not isinstance(entry, dict):
            problems.append(f"category {name}: the entry is not an object")
            continue
        if entry.get("standard") != template.get("standard"):
            problems.append(
                f"category {name}: standard {entry.get('standard')!r} "
                f"!= template {template.get('standard')!r}"
            )
        if entry.get("spec") != template.get("spec"):
            problems.append(
                f"category {name}: spec {entry.get('spec')!r} != template {template.get('spec')!r}"
            )
        expected = f"generate-standards.py --check#{name}"
        if entry.get("check") != expected:
            problems.append(
                f"category {name}: check {entry.get('check')!r} != {expected!r} "
                "(the existing verifier)"
            )
    return problems


def load_inputs() -> tuple[dict[str, Any], dict[str, Any], dict[str, Any], Path]:
    return (
        read_json(COVERAGE),
        read_json(STANDARD),
        read_json(OUTPUT_STANDARD),
        SNAPSHOT,
    )


def self_test() -> int:
    coverage, standard, output_standard, snapshot = load_inputs()
    actions = action_names()
    registry = registry_names(standard)
    clauses = manifest_clauses()

    def mutate(change) -> list[str]:
        variant = copy.deepcopy(coverage)
        change(variant)
        return check(variant, standard, output_standard, snapshot, actions, registry, clauses)

    baseline = check(coverage, standard, output_standard, snapshot, actions, registry, clauses)
    assert not baseline, f"the committed coverage must pass: {baseline}"

    def drop(variant):
        variant["entries"].pop(sorted(variant["entries"])[0])

    def add_unknown(variant):
        variant["entries"]["not_a_rule"] = copy.deepcopy(
            next(iter(variant["entries"].values()))
        )

    def unknown_registry(variant):
        variant["entries"]["programme_outputs"]["verifier"] = {
            "kind": "registry", "name": "nope_nope",
        }

    def unknown_clause(variant):
        variant["entries"]["wsjf_inputs_missing"]["verifier"] = {
            "kind": "manifest", "clause": "nope-clause",
        }

    def absent_source(variant):
        variant["entries"]["programme_outputs"]["source"] = {
            "file": "programme.json", "pointer": "/nope",
        }

    def stale_live(variant):
        variant["entries"]["demo_credit_missing"]["source"] = {
            "file": "msp.json", "pointer": "/cadences", "available": "live", "reason": "test",
        }

    def unknown_action(variant):
        variant["entries"]["programme_outputs"]["source"] = {"action": "nope.nope"}

    def unknown_category(variant):
        variant["entries"]["programme_outputs"]["categories"] = ["nope-category"]

    def standard_mismatch(variant):
        variant["entries"]["programme_outputs"]["standard"] = "PMP"

    def spec_mismatch(variant):
        variant["entries"]["programme_outputs"]["spec"] = "§9.9"

    def drop_category(variant):
        variant["categories"].pop("mandate")

    def add_unknown_category(variant):
        variant["categories"]["not-a-category"] = {
            "standard": "SPEC",
            "spec": "§5",
            "check": "generate-standards.py --check#not-a-category",
        }

    def category_standard_mismatch(variant):
        variant["categories"]["mandate"]["standard"] = "PMP"

    def category_spec_mismatch(variant):
        variant["categories"]["mandate"]["spec"] = "§9.9"

    def invented_category_check(variant):
        variant["categories"]["mandate"]["check"] = "scripts/invented-linter.py --all"

    def absent_category_map(variant):
        variant.pop("categories")

    cases = {
        "an orphan rule fails": (drop, "no coverage entry"),
        "an entry for no rule fails": (add_unknown, "for no rule"),
        "an unknown registry check fails": (unknown_registry, "unknown registry check"),
        "an unknown manifest clause fails": (unknown_clause, "unknown manifest clause"),
        "an absent snapshot source fails": (absent_source, "does not resolve"),
        "a stale live-only flag fails": (stale_live, "marked live-only but resolves"),
        "an unknown live action fails": (unknown_action, "unknown live action"),
        "an unknown category fails": (unknown_category, "unknown category"),
        "a standard mismatch fails": (standard_mismatch, "!= matrix"),
        "a spec mismatch fails": (spec_mismatch, "!= matrix"),
        "a missing category fails": (drop_category, "'mandate'"),
        "a category entry for no template fails": (add_unknown_category, "for no output-standard"),
        "a category standard mismatch fails": (category_standard_mismatch, "!= template"),
        "a category spec mismatch fails": (category_spec_mismatch, "!= template"),
        "an invented category verifier fails": (invented_category_check, "--check#mandate"),
        "a missing category map fails": (absent_category_map, "coverage.categories"),
    }
    for label, (change, needle) in cases.items():
        problems = mutate(change)
        assert problems, f"self-test: {label} was accepted"
        assert any(needle in problem for problem in problems), (
            f"self-test: {label} produced {problems}, expected one naming {needle!r}"
        )
        print(f"standards-coverage: self-test case {label} OK")

    print(
        f"standards-coverage: self-test OK ({len(coverage['entries'])} rules, "
        f"{len(coverage['categories'])} categories, {len(clauses)} manifest clauses, "
        f"{len(registry)} registry checks)"
    )
    return 0


def main() -> int:
    global VERIFY_PROJECT, MANIFESTS
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--coverage", type=Path, default=COVERAGE)
    parser.add_argument("--standard", type=Path, default=STANDARD)
    parser.add_argument("--output-standard", type=Path, default=OUTPUT_STANDARD)
    parser.add_argument("--snapshot", type=Path, default=SNAPSHOT)
    parser.add_argument("--actions", type=Path, default=ACTIONS)
    parser.add_argument("--manifests", type=Path, default=MANIFESTS)
    parser.add_argument("--verify-project", type=Path, default=VERIFY_PROJECT)
    parser.add_argument(
        "--check", action="store_true", help="verify the committed map (the default action)"
    )
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    VERIFY_PROJECT = args.verify_project
    MANIFESTS = args.manifests
    if args.self_test:
        return self_test()
    coverage = read_json(args.coverage)
    standard = read_json(args.standard)
    output_standard = read_json(args.output_standard)
    problems = check(
        coverage,
        standard,
        output_standard,
        args.snapshot,
        action_names(args.actions),
        registry_names(standard),
        manifest_clauses(args.manifests),
    )
    if problems:
        print("standards-coverage: FAIL", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1
    live = [name for name, entry in (coverage["entries"]).items()
            if (entry.get("source") or {}).get("available") == "live"]
    print(
        f"standards-coverage: {len(coverage['entries'])} rules covered; "
        f"{len(coverage['categories'])} output-standard categories covered; "
        f"{len(live)} live-only source(s){': ' + ', '.join(sorted(live)) if live else ''}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
