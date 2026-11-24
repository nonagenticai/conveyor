#!/usr/bin/env python3
"""t19-live-gate-check — assert T19's live acceptance rows through the deployed read actions.

The T19 live gate (docs/plans/T19-decision-forums.md §10) is the owner deciding one agenda item
in the web room and one over MCP, both recorded `via.surface='meeting'`, followed by the meeting's
own DoD (minutes published, meeting closed). This script reads those rows back through the
deployed engine's read actions — never SQL and never a captured fixture — and exits non-zero when
any invariant fails.

Transport: `scripts/programme-record.py` (the same read path the programme record uses), so the
operator environment is `CONVEYOR_SSH_KEY`, `CONVEYOR_HOST` and `TENANT`.

Usage:
    python3 scripts/t19-live-gate-check.py --state docs/evidence/t19/live-gate.json
    python3 scripts/t19-live-gate-check.py --state docs/evidence/t19/live-gate.json --seed

`--seed` also checks the live programme row installed by `migrate --seed`: the T19 project and its
board, the active `meeting` card type, and benefit B7.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RECORDER = ROOT / "scripts" / "programme-record.py"


def read(action: str, payload: dict) -> dict:
    """Invoke one read action through the recorder and return its JSON body."""
    completed = subprocess.run(
        [sys.executable, str(RECORDER), "--action", action, "--input", json.dumps(payload)],
        capture_output=True,
        text=True,
    )
    if completed.returncode != 0:
        sys.stderr.write(completed.stderr or completed.stdout)
        raise SystemExit(f"{action}: transport exited {completed.returncode}")
    body = "\n".join(
        line for line in completed.stdout.splitlines() if not line.startswith("== ")
    )
    try:
        return json.loads(body)
    except ValueError:
        raise SystemExit(f"{action}: output is not JSON: {body[:300]!r}")


class Checks:
    def __init__(self) -> None:
        self.failures: list[str] = []

    def check(self, condition: bool, message: str) -> None:
        print(f"{'PASS' if condition else 'FAIL'} {message}")
        if not condition:
            self.failures.append(message)


def seed_checks(state: dict, checks: Checks) -> None:
    project = read("projects.get", {"id": state["project_id"]})
    body = project.get("project", project)
    checks.check(body.get("key") == "T19", f"live project is T19 (got {body.get('key')})")
    boards = [b for b in project.get("boards", body.get("boards", [])) if b.get("id")]
    checks.check(
        any(b.get("id") == state["board_id"] for b in boards),
        "the T19 board is a live row",
    )

    types = read("card_types.list", {})
    meeting_types = [
        t for t in types.get("items", []) if t.get("code") == "meeting" and t.get("is_active")
    ]
    checks.check(len(meeting_types) == 1, f"one active `meeting` card type (got {len(meeting_types)})")

    benefits = read("benefits.list", {"programme_id": state["programme_id"]})
    b7 = [b for b in benefits.get("items", []) if b.get("reference") == "B7"]
    checks.check(len(b7) == 1, f"benefit B7 is a live row (got {len(b7)})")


def gate_checks(state: dict, checks: Checks) -> None:
    detail = read("meetings.get", {"meeting_id": state["meeting_id"]})
    meeting = detail.get("meeting", detail)
    approvals = read("approvals.list", {"project_id": state["project_id"]})
    by_id = {a.get("id"): a for a in approvals.get("items", [])}

    checks.check(
        meeting.get("status") == "closed",
        f"meeting is closed (got {meeting.get('status')!r})",
    )
    deciding = [a for a in detail.get("agenda", []) if a.get("outcome") == "decision"]
    checks.check(len(deciding) == 2, f"two agenda items closed as decision (got {len(deciding)})")

    approvers: list[str] = []
    for channel, approval_id in (
        ("web", state["approvals"]["web"]),
        ("mcp", state["approvals"]["mcp"]),
    ):
        approval = by_id.get(approval_id, {})
        rows = [
            d
            for d in approval.get("decisions", [])
            if (d.get("via") or {}).get("surface") == "meeting"
        ]
        checks.check(len(rows) == 1, f"{channel}: one meeting decision row (got {len(rows)})")
        checks.check(
            approval.get("status") == "approved",
            f"{channel}: linked approval is approved (got {approval.get('status')!r})",
        )
        if rows:
            row = rows[0]
            checks.check(
                (row.get("via") or {}).get("channel") == channel,
                f"{channel}: via.channel is {channel} (got {(row.get('via') or {}).get('channel')!r})",
            )
            approvers.append(row.get("approver_id"))

    checks.check(
        len(approvers) == 2 and len(set(approvers)) == 1,
        f"both rows name one approver (got {approvers})",
    )
    if state.get("approver_id") and approvers:
        checks.check(
            all(a == state["approver_id"] for a in approvers),
            f"the approver is the expected human seat ({state['approver_id']})",
        )

    minutes = by_id.get(state["approvals"]["minutes"], {})
    checks.check(
        minutes.get("status") == "approved",
        f"minutes approval is approved (got {minutes.get('status')!r})",
    )
    # `meetings.close` refuses without a published minutes governed asset, so a closed
    # meeting is the row-level proof that the minutes exist.


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--state", required=True)
    parser.add_argument("--seed", action="store_true")
    args = parser.parse_args()
    state = json.loads(Path(args.state).read_text(encoding="utf-8"))

    checks = Checks()
    if args.seed:
        seed_checks(state, checks)
    gate_checks(state, checks)

    if checks.failures:
        print(f"{len(checks.failures)} check(s) failed", file=sys.stderr)
        return 1
    print("all T19 live-gate checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
