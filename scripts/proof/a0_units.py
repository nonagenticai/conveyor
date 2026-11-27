#!/usr/bin/env python3
"""A0: emit unit-model Lean sources for the prover timing spike (conveyor#836).

This is the spike, not the shipped renderer. It emits the exact template
`crates/actions/src/proof/render.rs` will emit for a `unit_ledger` facts document:
the model text verbatim, then a `List UnitObs` literal and one kernel theorem.
A1's golden test pins the real renderer against this template for three units.

Run: python3 scripts/proof/a0_units.py --out /tmp/a0
Every emitted source is expected to exit 0 except the `negative_*`, `empty` and
(until proven otherwise) `startsWith` probes, which measure the model's failure shape.
"""

from __future__ import annotations

import argparse
import pathlib

ROOT = pathlib.Path(__file__).resolve().parents[2]
MODEL = ROOT / "docs/spec/lean/DeliveryModel.lean"


def opt(value, render):
    return "none" if value is None else f"some {render(value)}"


def unit(
    *,
    issue="closed",
    branches=1,
    deleted=True,
    pulls=1,
    pull="merged",
    ci=2,
) -> str:
    """One `UnitObs` literal, one line — the unit of the byte budget."""
    return (
        "  { issue := "
        + opt(issue, lambda v: f".{v}")
        + ", branches := "
        + opt(branches, str)
        + ", branchDeleted := "
        + opt(deleted, lambda v: "true" if v else "false")
        + ", pulls := "
        + opt(pulls, str)
        + ", pull := "
        + opt(pull, lambda v: f".{v}")
        + ", ciRuns := "
        + opt(ci, str)
        + " }"
    )


def ledger(units: list[str], theorem: str) -> str:
    body = ",\n".join(units)
    return f"def a0_facts : List UnitObs := [\n{body}\n]\n\ntheorem {theorem} : checkLedger a0_facts = true := by decide\n"


def render(model: str, tail: str) -> str:
    return f"{model}\n{tail}"


def synthetic(count: int) -> list[str]:
    """A deterministic, realistic mix; every unit satisfies the five rules."""
    out = []
    for i in range(count):
        if i % 2 == 0:
            out.append(unit(ci=i % 3))
        else:
            out.append(unit(issue="pending", deleted=False, pull="ready", ci=i % 3))
    return out


NEGATIVES = {
    "negative_two_branches": {"branches": 2},
    "negative_two_pulls": {"pulls": 2},
    "negative_draft": {"pull": "draft"},
    "negative_three_ci": {"ci": 3},
    "negative_merged_issue_pending": {"issue": "pending"},
    "negative_merged_branch_live": {"deleted": False},
    "negative_unknown_branches": {"branches": None},
}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", default="/tmp/a0", type=pathlib.Path)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)

    model = MODEL.read_text()
    manifest = []

    for count in (40, 80, 200, 400, 444):
        text = render(model, ledger(synthetic(count), "a0_ok"))
        path = args.out / f"ledger_{count}.lean"
        path.write_text(text)
        manifest.append((path.name, count, len(text)))

    for name, overrides in NEGATIVES.items():
        body = unit(**overrides)
        text = render(
            model,
            f"def a0_bad : UnitObs := {body.strip()}\n\n"
            "theorem a0_negative : checkLedger [a0_bad] = true := by decide\n",
        )
        path = args.out / f"{name}.lean"
        path.write_text(text)
        manifest.append((path.name, 1, len(text)))

    empty = render(
        model,
        "def a0_empty : List UnitObs := []\n\n"
        "theorem a0_empty_ok : checkLedger a0_empty = true := by decide\n",
    )
    (args.out / "empty.lean").write_text(empty)
    manifest.append(("empty.lean", 0, len(empty)))

    starts = render(
        model,
        'def a0_probe : Bool := "issue/836-unit-proof".startsWith "issue/836-"\n\n'
        "theorem a0_startsWith : a0_probe = true := by decide\n",
    )
    (args.out / "startsWith.lean").write_text(starts)
    manifest.append(("startsWith.lean", 0, len(starts)))

    for name, count, size in manifest:
        print(f"{name}\t{count}\t{size}")


if __name__ == "__main__":
    main()
