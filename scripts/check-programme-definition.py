#!/usr/bin/env python3
"""Validate the buildout's MSP definition seed (`seed/programme-definition.json`).

The definition is the programme's own: the six benefit profiles and their map, the fourteen
tranches, and every project's ADM phase/plateau. It is deliberately checked before the loader
runs, so a missing baseline, target, method or owner is refused in the source rather than seeded
as a draft that can never be approved (the T1 rule).

Usage:
    python3 scripts/check-programme-definition.py
    python3 scripts/check-programme-definition.py --file /tmp/mutated.json   # a copy, for tests
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT = ROOT / "seed" / "programme-definition.json"
PROGRAMME_SEED = ROOT / "seed" / "programme.json"

BENEFIT_FIELDS = (
    "reference",
    "description",
    "category",
    "target",
    "baseline",
    "measurement_method",
    "owner_role",
)
LINK_FIELDS = (
    "output_ref",
    "capability_ref",
    "operational_outcome",
    "benefit",
    "strategic_objective",
)
TRANCHE_STATES = {"planned", "authorised", "in_review", "active", "closed", "paused", "stopped"}


def fail(message: str) -> None:
    sys.exit(f"check-programme-definition: {message}")


def non_empty(value: object) -> bool:
    return isinstance(value, str) and value.strip() != ""


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--file", default=str(DEFAULT))
    args = parser.parse_args()
    path = Path(args.file)
    if not path.exists():
        fail(f"{path} does not exist")
    document = json.loads(path.read_text(encoding="utf-8"))
    if document.get("version") != 1:
        fail("version must be 1")
    programme = document.get("programme") or {}
    if not non_empty(programme.get("name")):
        fail("programme.name is required")

    benefits = document.get("benefits")
    if not isinstance(benefits, list) or not benefits:
        fail("benefits must be a non-empty list")
    references: set[str] = set()
    for index, benefit in enumerate(benefits):
        where = f"benefits[{index}]"
        for field in BENEFIT_FIELDS:
            if not non_empty(benefit.get(field)):
                fail(f"{where}.{field} is required")
        reference = benefit["reference"]
        if reference in references:
            fail(f"duplicate benefit reference {reference}")
        references.add(reference)

    links = document.get("links")
    if not isinstance(links, list):
        fail("links must be a list")
    for index, link in enumerate(links):
        where = f"links[{index}]"
        for field in LINK_FIELDS:
            if not non_empty(link.get(field)):
                fail(f"{where}.{field} is required")
        if link["benefit"] not in references:
            fail(f"{where}.benefit {link['benefit']!r} names no benefit")

    tranches = document.get("tranches")
    if not isinstance(tranches, list) or not tranches:
        fail("tranches must be a non-empty list")
    numbers = sorted(tranche.get("number") for tranche in tranches)
    if numbers != list(range(len(numbers))):
        fail(f"tranche numbers must be contiguous from 0; found {numbers}")
    for tranche in tranches:
        where = f"tranche {tranche.get('number')}"
        if tranche.get("state") not in TRANCHE_STATES:
            fail(f"{where}.state must be one of {sorted(TRANCHE_STATES)}")
        for field in ("entry", "exit"):
            if not non_empty(tranche.get(field)):
                fail(f"{where}.{field} is required")

    projects = document.get("projects")
    if not isinstance(projects, dict) or not projects:
        fail("projects must be a non-empty object")
    if PROGRAMME_SEED.exists():
        seed = json.loads(PROGRAMME_SEED.read_text(encoding="utf-8"))
        keys = {project["key"] for project in seed.get("projects", [])}
        missing = sorted(keys - set(projects))
        extra = sorted(set(projects) - keys)
        if missing:
            fail(f"projects has no ADM entry for {missing}")
        if extra:
            fail(f"projects names keys not in seed/programme.json: {extra}")
    for key, entry in projects.items():
        if not non_empty(entry.get("adm_phase")):
            fail(f"projects.{key}.adm_phase is required")
        if not non_empty(entry.get("plateau")):
            fail(f"projects.{key}.plateau is required")

    print(
        f"check-programme-definition: {len(benefits)} benefits, {len(links)} links, "
        f"{len(tranches)} tranches, {len(projects)} projects — OK"
    )


if __name__ == "__main__":
    main()
