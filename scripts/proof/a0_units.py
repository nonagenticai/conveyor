#!/usr/bin/env python3
"""A0/A1: emit unit-model Lean sources for the prover checks (conveyor#836).

This is the template generator, not the shipped renderer. It emits the exact template
`crates/actions/src/proof/render.rs` emits for a `unit_ledger` facts document: the model text
verbatim, then a `List UnitObs` literal, the kernel-checked per-rule results, and the aggregate
theorem. A1's golden test pins the Rust renderer's literal format against this file.

Run: python3 scripts/proof/a0_units.py --out /tmp/a0
Every emitted source is expected to exit 0 except the `negative_*`, `empty` and `startsWith`
probes, which measure the model's failure shape. A negative control's `_rules` theorem is always
true (the kernel proves the reported outcome); its aggregate theorem is what the kernel refuses.
"""

from __future__ import annotations

import argparse
import pathlib
from dataclasses import dataclass, replace

ROOT = pathlib.Path(__file__).resolve().parents[2]
MODEL = ROOT / "docs/spec/lean/DeliveryModel.lean"


@dataclass(frozen=True)
class Unit:
    """One observation, exactly the nine fields of `UnitObs`."""

    issue: str | None = "closed"
    branches: int | None = 1
    deleted: bool | None = True
    pulls: int | None = 1
    pull: str | None = "merged"
    ci: int | None = 2
    closes: bool | None = True
    verify: bool | None = True
    unit: bool | None = True


def opt(value, render) -> str:
    return "none" if value is None else f"some {render(value)}"


def lean_unit(unit: Unit) -> str:
    """One `UnitObs` literal, one line — the unit of the byte budget."""
    return (
        "  { issue := "
        + opt(unit.issue, lambda v: f".{v}")
        + ", branches := "
        + opt(unit.branches, str)
        + ", branchDeleted := "
        + opt(unit.deleted, lambda v: "true" if v else "false")
        + ", pulls := "
        + opt(unit.pulls, str)
        + ", pull := "
        + opt(unit.pull, lambda v: f".{v}")
        + ", ciRuns := "
        + opt(unit.ci, str)
        + ", closesKeyword := "
        + opt(unit.closes, lambda v: "true" if v else "false")
        + ", verifyParts := "
        + opt(unit.verify, lambda v: "true" if v else "false")
        + ", unit := "
        + opt(unit.unit, lambda v: "true" if v else "false")
        + " }"
    )


def one_branch(unit: Unit) -> bool:
    return unit.branches == 1


def one_pull(unit: Unit) -> bool:
    return unit.pulls == 1


def pull_usable(unit: Unit) -> bool:
    return unit.pull in ("ready", "merged")


def ci_budget(unit: Unit) -> bool:
    return unit.ci is not None and unit.ci <= 2


def merge_complete(unit: Unit) -> bool:
    if unit.pull == "merged":
        return unit.issue == "closed" and unit.deleted is True
    return True


def closes_stated(unit: Unit) -> bool:
    return unit.closes is True


def parts_verified(unit: Unit) -> bool:
    return unit.unit is False or unit.verify is True


RULES = [
    ("oneBranch", one_branch),
    ("onePull", one_pull),
    ("pullUsable", pull_usable),
    ("ciBudget", ci_budget),
    ("mergeComplete", merge_complete),
    ("closesKeyword", closes_stated),
    ("verifyParts", parts_verified),
]


def rules_literal(units: list[Unit]) -> str:
    """The per-rule outcomes, in the model's `ruleResults` order."""
    inner = ", ".join(
        f'("{name}", {"true" if all(rule(unit) for unit in units) else "false"})'
        for name, rule in RULES
    )
    return f"[{inner}]"


def ledger(units: list[Unit], theorem: str) -> str:
    body = ",\n".join(lean_unit(unit) for unit in units)
    return (
        f"def a0_facts : List UnitObs := [\n{body}\n]\n\n"
        f"theorem a0_rules : ruleResults a0_facts = {rules_literal(units)} := by decide\n\n"
        f"theorem {theorem} : checkLedger a0_facts = true := by decide\n"
    )


def control(unit: Unit, theorem: str) -> str:
    """One bad unit: the rules theorem is true, the aggregate theorem is refused."""
    return (
        f"def a0_bad : UnitObs := {lean_unit(unit).strip()}\n\n"
        f"theorem a0_control_rules : ruleResults [a0_bad] = {rules_literal([unit])} := by decide\n\n"
        f"theorem {theorem} : checkLedger [a0_bad] = true := by decide\n"
    )


def render(tail: str) -> str:
    return f"{MODEL.read_text()}\n{tail}"


def synthetic(count: int) -> list[Unit]:
    """A deterministic, realistic mix; every unit satisfies the eight rules."""
    out = []
    for index in range(count):
        if index % 2 == 0:
            out.append(Unit(ci=index % 3))
        else:
            out.append(Unit(issue="pending", deleted=False, pull="ready", ci=index % 3))
    return out


NEGATIVES = {
    "negative_two_branches": {"branches": 2},
    "negative_two_pulls": {"pulls": 2},
    "negative_draft": {"pull": "draft"},
    "negative_three_ci": {"ci": 3},
    "negative_merged_issue_pending": {"issue": "pending"},
    "negative_merged_branch_live": {"deleted": False},
    "negative_unknown_branches": {"branches": None},
    "negative_no_closes": {"closes": False},
    "negative_missing_verify": {"verify": False},
    "negative_unknown_unit": {"unit": None, "verify": False},
}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", default="/tmp/a0", type=pathlib.Path)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)

    manifest = []

    for count in (40, 80, 200, 400, 444):
        text = render(ledger(synthetic(count), "a0_ok"))
        path = args.out / f"ledger_{count}.lean"
        path.write_text(text)
        manifest.append((path.name, count, len(text.encode())))

    for name, overrides in NEGATIVES.items():
        text = render(control(replace(Unit(), **overrides), name))
        path = args.out / f"{name}.lean"
        path.write_text(text)
        manifest.append((path.name, 1, len(text)))

    empty = render(ledger([], "a0_empty_ok"))
    (args.out / "empty.lean").write_text(empty)
    manifest.append(("empty.lean", 0, len(empty)))

    starts = render(
        'def a0_probe : Bool := "issue/836-unit-proof".startsWith "issue/836-"\n\n'
        "theorem a0_startsWith : a0_probe = true := by decide\n",
    )
    (args.out / "startsWith.lean").write_text(starts)
    manifest.append(("startsWith.lean", 0, len(starts)))

    for name, count, size in manifest:
        print(f"{name}\t{count}\t{size}")


if __name__ == "__main__":
    main()
