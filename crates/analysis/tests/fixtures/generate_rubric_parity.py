#!/usr/bin/env python3
"""Generate `rubric_parity.json` from the source service's scoring law.

The three lines below are copied verbatim from the absorbed service's
`compute_category_score` (`report_generator.py` at the captured revision) — the decay, the
rounding and the floor — with the config defaults it reads (`src/config/scoring.py`):

    decay_k      = 25
    score_floor  = 50.0
    critical/high/medium/low/info = 30/15/7/3/0

The point is *not* to re-derive the law in Rust and compare it with itself. It is to freeze the
Python implementation's output — including `round(x, 1)`'s half-to-even behaviour — into a file a
Rust test reads, so a change to `round_1dp` or to the constants fails against the source's own
numbers rather than against an assumption about them.

Run from this directory:  python3 generate_rubric_parity.py > rubric_parity.json
"""

import json
import math

DECAY_K = 25
SCORE_FLOOR = 50.0
GRADE_THRESHOLDS = ((95.0, "A+"), (90.0, "A"), (80.0, "B"), (70.0, "C"), (60.0, "D"), (0.0, "F"))


def compute_category_score(penalty: int) -> float:
    score = SCORE_FLOOR + (100 - SCORE_FLOOR) * math.exp(-penalty / DECAY_K)
    return max(round(score, 1), SCORE_FLOOR)


def score_to_grade(score: float) -> str:
    for threshold, grade in GRADE_THRESHOLDS:
        if score >= threshold:
            return grade
    return "F"


cases = [{"penalty": penalty, "score": compute_category_score(penalty)} for penalty in range(0, 121)]
grades = [
    {"score": score, "grade": score_to_grade(score)}
    for score in (100.0, 95.0, 94.9, 90.0, 89.9, 80.0, 79.9, 70.0, 69.9, 60.0, 59.9, 50.0, 0.0)
]

print(json.dumps({"cases": cases, "grades": grades}, indent=2))
