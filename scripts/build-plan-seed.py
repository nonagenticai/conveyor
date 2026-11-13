#!/usr/bin/env python3
"""Build the plan seed: every programme project's units as conveyor cards.

Sources (read-only):
  * `seed/programme.json` — the project registry (one project per plan) and its issue links.
  * `docs/design/data/issues.json` — the redacted issue registry (state + title).
  * `seed/plans/card-types.json` — the explicit unit -> card type map (T0 is pinned here).

Outputs:
  * `seed/plans.json` — the committed document the seeder embeds.
  * `docs/design/data/plan-seed.js` — the same document as a `window.CONVEYOR_PLAN_SEED`
    assignment, so the design pack renders the DESIGN fallback without fetching.

The unit is the issue: one card per linked issue, plus one `plan-<KEY>` card for a project
with no issue. The card type decides the process stage (`<card_type>.READY`), so every unit
names a real process.

Deterministic: projects keep the programme order; units sort by issue number, plan-level
last. Re-running on unchanged inputs produces identical bytes.

    python3 scripts/build-plan-seed.py            # write both outputs
    python3 scripts/build-plan-seed.py --check    # fail if either is stale
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROGRAMME = ROOT / "seed/programme.json"
ISSUES = ROOT / "docs/design/data/issues.json"
CARD_TYPES = ROOT / "seed/plans/card-types.json"
CATALOGUE = ROOT / "seed/catalogue.json"
MANIFEST = ROOT / "seed/processes/manifest.yaml"
OUT_SEED = ROOT / "seed/plans.json"
OUT_JS = ROOT / "docs/design/data/plan-seed.js"
SCHEMA = "design-plan-seed/v1"


def load_catalogue_types() -> set[str]:
    document = json.loads(CATALOGUE.read_text())
    return {row["code"] for row in document.get("card_types", [])}


def load_process_types() -> set[str]:
    """The card types that own a `<type>.READY` process (the manifest is the index)."""
    text = MANIFEST.read_text()
    keys = set(re.findall(r"\{key:\s*([^,\s]+)", text))
    return {key.split(".", 1)[0] for key in keys if key.endswith(".READY")}


def card_type_for(project_key: str, issue: int | None, mapping: dict) -> str:
    if issue is not None:
        pinned = mapping.get("issues", {}).get(str(issue))
        if pinned:
            return pinned
    return mapping.get("projects", {}).get(project_key) or mapping["default"]


def build() -> dict:
    programme = json.loads(PROGRAMME.read_text())
    registry = {
        row["number"]: row for row in json.loads(ISSUES.read_text()).get("issues", [])
    }
    mapping = json.loads(CARD_TYPES.read_text())
    known_types = load_catalogue_types()
    known_processes = load_process_types()

    problems: list[str] = []
    projects: list[dict] = []
    seen_units: set[str] = set()
    for project in programme["projects"]:
        meta = project.get("meta", {})
        plan = meta.get("plan", "")
        units: list[dict] = []
        for number in sorted(meta.get("issues", [])):
            issue = registry.get(number)
            if issue is None:
                problems.append(f"{project['key']}: issue #{number} is not in the registry")
                continue
            card_type = card_type_for(project["key"], number, mapping)
            unit_key = f"issue-{number}"
            units.append(
                {
                    "unit_key": unit_key,
                    "title": issue["title"],
                    "issue": number,
                    "state": issue["state"],
                    "card_type": card_type,
                    "process_key": f"{card_type}.READY",
                    "plan": plan,
                    **({"gate": meta["gate"]} if meta.get("gate") else {}),
                }
            )
            if unit_key in seen_units:
                problems.append(f"duplicate unit key {unit_key}")
            seen_units.add(unit_key)

        if not units:
            card_type = card_type_for(project["key"], None, mapping)
            units.append(
                {
                    "unit_key": f"plan-{project['key']}",
                    "title": project["name"],
                    "issue": None,
                    "state": "CLOSED" if project.get("status") in ("closed", "archived") else "OPEN",
                    "card_type": card_type,
                    "process_key": f"{card_type}.READY",
                    "plan": plan,
                    **({"gate": meta["gate"]} if meta.get("gate") else {}),
                }
            )

        for unit in units:
            if unit["card_type"] not in known_types:
                problems.append(
                    f"{project['key']}/{unit['unit_key']}: card type {unit['card_type']} is not in seed/catalogue.json"
                )
            if unit["card_type"] not in known_processes:
                problems.append(
                    f"{project['key']}/{unit['unit_key']}: no {unit['card_type']}.READY in seed/processes/manifest.yaml"
                )

        projects.append(
            {
                "project_key": project["key"],
                "plan": plan,
                "track": meta.get("track", ""),
                "plan_status": meta.get("plan_status", ""),
                "depends_on": meta.get("depends_on", []),
                "units": units,
            }
        )

    if problems:
        for problem in problems:
            print(f"build-plan-seed: {problem}", file=sys.stderr)
        sys.exit("build-plan-seed: refusing to write an invalid seed")

    return {
        "version": 1,
        "schema": SCHEMA,
        "generated_from": [
            "seed/programme.json",
            "docs/design/data/issues.json",
            "seed/plans/card-types.json",
        ],
        "projects": projects,
    }


def render(document: dict) -> tuple[str, str]:
    seed = json.dumps(document, indent=2, ensure_ascii=False) + "\n"
    payload = json.dumps(document, ensure_ascii=False, separators=(",", ":"))
    js = (
        "// Generated by scripts/build-plan-seed.py — do not hand-edit.\n"
        "// The same document the seeder embeds; the pack's DESIGN fallback.\n"
        f"window.CONVEYOR_PLAN_SEED = {payload};\n"
    )
    return seed, js


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="fail if an output is stale")
    args = parser.parse_args()
    seed, js = render(build())
    if args.check:
        stale = [
            str(path)
            for path, expected in ((OUT_SEED, seed), (OUT_JS, js))
            if not path.exists() or path.read_text() != expected
        ]
        if stale:
            sys.exit(f"build-plan-seed: stale output(s): {', '.join(stale)}; re-run without --check")
        print("build-plan-seed: both outputs are current")
        return
    OUT_SEED.write_text(seed)
    OUT_JS.write_text(js)
    document = json.loads(seed)
    units = sum(len(project["units"]) for project in document["projects"])
    print(f"build-plan-seed: {len(document['projects'])} projects, {units} units")


if __name__ == "__main__":
    main()
