#!/usr/bin/env python3
"""Validate the programme output standard's own shape.

`seed/output-standard.json` is the one vocabulary and the per-project-class required sets.
The frozen `seed/programme.json` carries each project's `meta.expected_outputs` /
`meta.closure_outputs`; this gate refuses a standard that is internally inconsistent:

  * an unknown category, alias target or matching fact;
  * a class with no facts, an empty set, or a repeated category;
  * a category with no template, an empty or duplicate required heading, a required heading
    absent from the skeleton, or a URL in a skeleton;
  * a mandatory template with no source or no document category;
  * a SPEC_V2 §8.1, §8.2 or §8.3 named artefact with no category or template, or one of the
    stream's new categories missing;
  * a template without its standard, level, subject kind, mandatory flag or spec clause.

The comparisons this gate used to carry — a document's category membership, the seed's recorded
sets against its class, orphan coverage and the console copy — moved to the runs that own them:
`documents.publish` refuses an unknown category, and the declared-vs-evidenced register
(`crates/pack/src/register.rs`) publishes class drift and per-kind gaps on the public record
(#756 P3).

Run it before every push: `python3 scripts/check-output-standard.py`. `--self-test` proves the
gate refuses each failure shape.
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
FACTS = {"key", "track", "status", "measured_status"}
ON_DEMAND = "decision"
DEFAULT_ON_DEMAND = [ON_DEMAND]
STANDARDS = {"MSP", "TOGAF", "SAFe", "PMP", "SPEC", "Delivery"}
LEVELS = {"portfolio", "programme", "project", "work_package"}
SUBJECT_KINDS = {"programme", "project", "tranche", "benefit", "capability", "phase", "artifact", "card"}
SOURCE_KINDS = {"rows", "documents", "computed", "register", "absence"}
# SPEC_V2 §8.1, §8.2 and §8.3 name the artefacts each standard requires. The prose is not a
# parser (the programme seed follows the same rule), so the named lists are data: every named
# artefact must resolve to a category, and the category must have a template.
SPEC_ARTEFACT_CATEGORIES = {
    "§8.1": (
        "architecture-principles",
        "adm-profile",
        "artifact-standard",
        "architecture-capability",
        "architecture-governance-log",
        "waiver",
        "maturity-assessment",
        "skills-assessment",
        "phase-preliminary",
        "phase-a",
        "phase-b",
        "phase-c",
        "phase-d",
        "phase-e",
        "phase-f",
        "phase-g",
        "phase-h",
        "statement-of-architecture-work",
        "requirements-catalog",
        "compliance-review",
        "gap-analysis",
        "candidate-solutions",
        "project-context-diagram",
        "benefits-diagram",
        "architecture-artifact",
        "architecture-artifact-instance",
        "delivery-contract",
        "statement-of-intent",
        "request-for-architecture-work",
        "adr-register",
    ),
    "§8.2": (
        "safe-portfolio",
        "safe-art",
        "safe-cadence",
        "pi-plan",
        "pi-objectives",
        "system-demo",
        "inspect-adapt",
    ),
    "§8.3": (
        "mandate",
        "programme-brief",
        "blueprint",
        "business-case",
        "benefit-profile",
        "benefits-map",
        "benefits-registry",
        "benefit-evaluation",
        "benefits-strategy",
        "realization-plan",
        "projects-dossier",
        "programme-plan",
        "tranche-plan",
        "tranche-review",
        "refined-vision",
        "programme-baselines",
        "programme-strategies",
        "programme-risk-register",
        "stakeholder-profiles",
        "benefit-transition",
    ),
}
# The operated registers, the programme mandate, benefits registry and evaluation, and TOGAF
# phase E's named artefacts: the categories this standard used to omit.
NEW_CATEGORIES = {
    "programme-risk-register",
    "adr-register",
    "benefits-registry",
    "benefit-evaluation",
    "mandate",
    "candidate-solutions",
    "project-context-diagram",
    "benefits-diagram",
    "gap-analysis",
}
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


def validate(std: dict) -> list[str]:
    errors: list[str] = []
    if std.get("version") != 3:
        errors.append(f"standard version is {std.get('version')!r}, expected 3")

    categories = std.get("categories")
    if not isinstance(categories, list) or not categories:
        errors.append("categories must be a non-empty list")
        categories = []
    if len(categories) != len(set(categories)):
        errors.append("categories contains a duplicate")
    for category in categories:
        if not isinstance(category, str) or not category:
            errors.append(f"invalid category {category!r}")

    on_demand = std.get("on_demand", DEFAULT_ON_DEMAND)
    if not isinstance(on_demand, list) or not on_demand:
        errors.append("on_demand must be a non-empty list")
        on_demand = list(DEFAULT_ON_DEMAND)
    if len(on_demand) != len(set(on_demand)):
        errors.append("on_demand contains a duplicate")
    for category in on_demand:
        if category not in categories:
            errors.append(f"on_demand names unknown category {category!r}")

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
                if category in on_demand:
                    errors.append(f"class {name!r} {field} requires {category!r}, which is on demand only")
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
        # The document category the generator falls back to when a template's rows are empty.
        # Bind it before the source checks: a mandatory template without it can never be
        # satisfied, and a self-document heading check reads it below.
        document_category = template.get("document_category")
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

        # The standards metadata is what the artefact generator reads. A template without it is
        # an output category, not a standards artefact; every v3 template must name its place.
        if template.get("standard") not in STANDARDS:
            errors.append(f"template {category!r} has no valid standard")
        if template.get("level") not in LEVELS:
            errors.append(f"template {category!r} has no valid level")
        if template.get("subject_kind") not in SUBJECT_KINDS:
            errors.append(f"template {category!r} has no valid subject_kind")
        if not isinstance(template.get("mandatory"), bool):
            errors.append(f"template {category!r} has no boolean mandatory flag")
        spec = template.get("spec")
        if not isinstance(spec, str) or not spec.strip():
            errors.append(f"template {category!r} has no spec clause")
        sources = template.get("sources", {})
        if not isinstance(sources, dict):
            errors.append(f"template {category!r} sources must be an object")
            sources = {}
        heading_keys = {heading_key(h) for h in required if isinstance(h, str)}
        heading_keys |= {heading_key(h) for h in optional if isinstance(h, str)}
        for heading, source_list in sources.items():
            if not isinstance(heading, str) or heading_key(heading) not in heading_keys:
                errors.append(f"template {category!r} sources a heading it does not declare: {heading!r}")
                continue
            if not isinstance(source_list, list) or not source_list:
                errors.append(f"template {category!r} section {heading!r} has no sources")
                continue
            for source in source_list:
                if not isinstance(source, dict) or source.get("kind") not in SOURCE_KINDS:
                    errors.append(
                        f"template {category!r} section {heading!r} has an invalid source"
                    )
                    continue
                if source["kind"] in {"rows", "documents"}:
                    if not isinstance(source.get("file"), str) or not source["file"]:
                        errors.append(
                            f"template {category!r} section {heading!r} source has no file"
                        )
                    if not isinstance(source.get("pointer"), str) or not source["pointer"].startswith("/"):
                        errors.append(
                            f"template {category!r} section {heading!r} source has no JSON pointer"
                        )
                    if "match" in source and not isinstance(source["match"], dict):
                        errors.append(
                            f"template {category!r} section {heading!r} match must be an object"
                        )
                if source["kind"] == "computed" and not isinstance(source.get("name"), str):
                    errors.append(
                        f"template {category!r} section {heading!r} computed source has no name"
                    )
                if source["kind"] == "absence":
                    if not isinstance(source.get("record"), str) or not source["record"].strip():
                        errors.append(
                            f"template {category!r} section {heading!r} absence source has no record"
                        )
                    if "fallback" in source and not isinstance(source["fallback"], bool):
                        errors.append(
                            f"template {category!r} section {heading!r} absence fallback must be a boolean"
                        )
            # A heading that cites only the artefact's own document is a heading without facts
            # unless the template declares its body authored. The 23 authored templates declare
            # it; a template that loses the declaration is refused.
            active = [s for s in source_list if isinstance(s, dict) and not s.get("optional")]
            if (
                document_category
                and active
                and template.get("document_authored") is not True
                and all(
                    s.get("kind") == "documents"
                    and str((s.get("match") or {}).get("category", "$category"))
                    in {str(document_category), "$category"}
                    for s in active
                )
            ):
                errors.append(
                    f"template {category!r} section {heading!r} cites only its own "
                    f"document category {document_category!r}; declare document_authored"
                )
        if template.get("mandatory") is True:
            # A mandatory artefact must be resolvable: at least one named source, and the
            # document category the generator falls back to when its rows are empty. Without
            # either, the artefact is permanently absent and no record can satisfy it.
            if not sources:
                errors.append(f"mandatory template {category!r} has no sources")
            if not isinstance(document_category, str) or not document_category:
                errors.append(f"mandatory template {category!r} has no document_category")

    for clause, named in SPEC_ARTEFACT_CATEGORIES.items():
        for category in named:
            if category not in categories:
                errors.append(
                    f"SPEC_V2 {clause} names artefact category {category!r}, which does not exist"
                )
            elif category not in templates:
                errors.append(
                    f"SPEC_V2 {clause} names artefact category {category!r}, which has no template"
                )
    for category in sorted(NEW_CATEGORIES):
        if category not in categories:
            errors.append(f"new category {category!r} is missing from categories")
        elif category not in templates:
            errors.append(f"new category {category!r} has no template")

    return errors


def self_test(std: dict) -> int:
    cases: list[tuple[str, dict]] = []

    bad = copy.deepcopy(std)
    bad["classes"][0]["now"].append("not-a-category")
    cases.append(("unknown category in a set", bad))

    bad = copy.deepcopy(std)
    bad["aliases"]["brief"] = "not-a-category"
    cases.append(("alias to an unknown category", bad))

    bad = copy.deepcopy(std)
    bad["templates"].pop("vision")
    cases.append(("a category without a template", bad))

    bad = copy.deepcopy(std)
    bad["templates"]["vision"]["required"] = []
    cases.append(("an empty required list", bad))

    bad = copy.deepcopy(std)
    bad["templates"]["vision"]["required"].append(bad["templates"]["vision"]["required"][0])
    cases.append(("a duplicate required heading", bad))

    bad = copy.deepcopy(std)
    bad["templates"]["vision"]["skeleton"] = "# Vision\n\n## Purpose\n"
    bad["templates"]["vision"]["required"].append("A heading not in the skeleton")
    cases.append(("a required heading absent from the skeleton", bad))

    bad = copy.deepcopy(std)
    bad["templates"]["vision"]["skeleton"] += "\nSee https://example.com for the method.\n"
    cases.append(("a URL in the skeleton", bad))

    bad = copy.deepcopy(std)
    bad["version"] = 2
    cases.append(("the retired standard version", bad))

    bad = copy.deepcopy(std)
    bad["on_demand"].append("not-a-category")
    cases.append(("an on-demand category that does not exist", bad))

    bad = copy.deepcopy(std)
    bad["classes"][1]["now"].append(bad["on_demand"][-1])
    cases.append(("a class that requires an on-demand category", bad))

    bad = copy.deepcopy(std)
    bad["templates"]["vision"].pop("standard", None)
    cases.append(("a template without a standard", bad))

    bad = copy.deepcopy(std)
    bad["templates"]["vision"]["sources"] = {"Not a required heading": [{"kind": "computed", "name": "x"}]}
    cases.append(("a source on an undeclared heading", bad))

    bad = copy.deepcopy(std)
    first_heading = bad["templates"]["vision"]["required"][0]
    bad["templates"]["vision"]["sources"] = {first_heading: [{"kind": "not-a-kind"}]}
    cases.append(("an invalid source kind", bad))

    bad = copy.deepcopy(std)
    first_heading = bad["templates"]["vision"]["required"][0]
    bad["templates"]["vision"]["sources"] = {first_heading: [{"kind": "absence"}]}
    cases.append(("an absence source without a record", bad))

    bad = copy.deepcopy(std)
    bad["templates"]["vision"].pop("document_authored", None)
    cases.append(("a self-document template without document_authored", bad))

    bad = copy.deepcopy(std)
    bad["templates"]["benefit-profile"].pop("subject_kind", None)
    cases.append(("a template without a subject kind", bad))

    bad = copy.deepcopy(std)
    bad["templates"]["tranche-review"].pop("sources", None)
    cases.append(("a mandatory template without sources", bad))

    bad = copy.deepcopy(std)
    bad["templates"]["tranche-review"].pop("document_category", None)
    cases.append(("a mandatory template without a document category", bad))

    bad = copy.deepcopy(std)
    bad["templates"].pop("programme-risk-register", None)
    cases.append(("a new category without a template", bad))

    bad = copy.deepcopy(std)
    bad["templates"].pop("safe-portfolio", None)
    cases.append(("a SPEC_V2 named artefact without a template", bad))

    failures = 0
    for label, case_std in cases:
        errors = validate(case_std)
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
    args = parser.parse_args()

    std = read_json(STANDARD_PATH)

    if args.self_test:
        return self_test(std)

    errors = validate(std)
    if errors:
        for error in errors:
            print(f"check-output-standard: ERROR: {error}")
        return 1
    print(
        f"check-output-standard: {len(std['classes'])} classes, "
        f"{len(std['categories'])} categories"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
