#!/usr/bin/env python3
"""Check the issue board against the label contract in docs/plans/03-DELIVERY.md.

WHY THIS FILE EXISTS
--------------------
`docs/plans/03-DELIVERY.md` § "Labels and states" defines the states as a contract:

    ready   status:ready    open, no `## Claim` block, no live branch, no linked PR
    claimed status:claimed  branch pushed + claim comment, work in progress
    blocked status:blocked  the body carries a plain `Blocker: #N` naming an OPEN issue

Every one of those clauses is machine-checkable, and until now none of them was checked.
The label is written by hand after the branch is pushed, so it is the step that gets
skipped, and a stale `status:ready` is a licence to start a second branch on work that
already exists. Measured on 2026-11-15: #137 and #140 carried `ready` with live branches
(repaired by hand at 15:47Z), then #149 and #151 were created as `ready` at 15:56Z/16:35Z
and claimed without the label moving; #121 and #122 stayed `blocked` on #120 after it
closed at 00:37:54Z.

`reap-stale-claims.sh` cannot catch any of this, by design: *a claim is a REMOTE BRANCH*,
so it reads branches and never labels. This reads the labels and the branches together.

THE THREE RULES, AND WHY ONLY ONE OF THEM ACTS
----------------------------------------------
A (acts)    `status:ready` + a live branch or an open PR -> `status:claimed`, one comment.
            Safe because it moves in ONE direction only. A check that could also move an
            issue back to `ready` would be able to steal a claim, which is a worse failure
            than the drift it repairs.
B (reports) `status:blocked` with no line starting `Blocker: #N`, or whose every named
            blocker is closed or missing. A block is real while any named blocker is open.
C (reports) `status:claimed` with no branch, no PR, and no update in 48 h — the reaper's
            two tests applied to the label. Reporting only: releasing a human's claim
            stays a human decision, which is also why the reaper defaults to report mode.

A rule's evidence is always printed. A wrong fix is silent; a printed verdict is not, and
the job summary is the record a later session reads.

USAGE
    board.py --repo nonagenticai/conveyor            # apply rule A, report B and C
    board.py --repo nonagenticai/conveyor --dry-run  # print the plan, write nothing
    board.py --self-test                             # fixtures only, no network, no git

EXIT CODES
    0  clean, or every rule-A fix applied
    1  --dry-run found a rule-A violation, or a fix failed
    2  the instrument is broken (gh failed, git failed, malformed input)

The fallbacks are deliberately unlike the reaper's. This script fails OPEN on a lookup it
cannot complete, because the alternative -- relabelling on incomplete evidence -- edits a
shared board; it never writes on anything but a positively-identified live branch or PR.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import re
import subprocess
import sys

READY = "status:ready"
CLAIMED = "status:claimed"
BLOCKED = "status:blocked"
NEEDS_HUMAN = "status:needs-human"

# `03-DELIVERY.md`: "A `status:blocked` issue carries a plain line STARTING `Blocker: #N`;
# a `Blocker:` mention inside prose or a checklist is not a blocker line." So the line must
# begin with the token -- that is what excludes the `- [x] ... `Blocker: #N` present.`
# checklists and mid-sentence prose -- and it may continue with a reason, and it may name
# more than one issue.
BLOCKER_LINE_RE = re.compile(r"^[ \t]*Blocker:[ \t]*(.+)$", re.MULTILINE)
ISSUE_REF_RE = re.compile(r"#(\d+)")

STALE_CLAIM_HOURS = 48
LIMIT = "500"


# --------------------------------------------------------------------------- rules


def blockers_of(body: str) -> list[int]:
    """Issue numbers named by the FIRST line that starts with `Blocker:`.

    Empty list means the issue carries no conforming blocker line at all.
    """
    match = BLOCKER_LINE_RE.search(body)
    if match is None:
        return []
    seen: list[int] = []
    for number in ISSUE_REF_RE.findall(match.group(1)):
        value = int(number)
        if value not in seen:
            seen.append(value)
    return seen


def branch_for(number: int, branches) -> str | None:
    """The remote branch that names this issue, or None. `issue/155-board-contract`."""
    exact = f"issue/{number}"
    prefix = f"issue/{number}-"
    for name in sorted(branches):
        if name == exact or name.startswith(prefix):
            return name
    return None


def pr_for(number: int, prs) -> dict | None:
    """The open PR that closes this issue, or whose head branch names it."""
    for pr in prs:
        if number in pr["closes"]:
            return pr
    head_exact = f"issue/{number}"
    head_prefix = f"issue/{number}-"
    for pr in prs:
        if pr["head"] == head_exact or pr["head"].startswith(head_prefix):
            return pr
    return None


def plan(issues, prs, branches, blocker_state, now):
    """Return (fixes, reports). Pure: every input is a plain value.

    `blocker_state(number)` is called at most once per distinct blocker and must return
    "open", "closed" or "missing".
    """
    fixes, reports = [], []
    for issue in issues:
        number = issue["number"]
        labels = set(issue["labels"])
        branch = branch_for(number, branches)
        pr = pr_for(number, prs)
        evidence = []
        if branch:
            evidence.append(f"branch `{branch}`")
        if pr:
            evidence.append(f"open PR #{pr['number']}")

        if READY in labels and evidence:
            fixes.append(
                {
                    "number": number,
                    "title": issue["title"],
                    "branch": branch,
                    "pr": pr["number"] if pr else None,
                    "evidence": evidence,
                }
            )
            continue

        if BLOCKED in labels:
            blockers = blockers_of(issue["body"])
            if not blockers:
                reports.append(
                    {
                        "rule": "B",
                        "number": number,
                        "title": issue["title"],
                        "why": f"`{BLOCKED}` with no line starting `Blocker: #N`",
                    }
                )
                continue
            states = {blocker: blocker_state(blocker) for blocker in blockers}
            # A block is real while ANY named blocker is open; it is stale only when every
            # one of them has closed (or never existed).
            if not any(state == "open" for state in states.values()):
                named = ", ".join(f"#{b} {s}" for b, s in states.items())
                reports.append(
                    {
                        "rule": "B",
                        "number": number,
                        "title": issue["title"],
                        "why": f"every named blocker is closed or missing ({named})",
                    }
                )
            continue

        if CLAIMED in labels and not evidence:
            age = now - issue["updated"]
            if age > dt.timedelta(hours=STALE_CLAIM_HOURS):
                hours = int(age.total_seconds() // 3600)
                reports.append(
                    {
                        "rule": "C",
                        "number": number,
                        "title": issue["title"],
                        "why": (
                            f"`{CLAIMED}` with no branch, no open PR, and no update in {hours} h"
                        ),
                    }
                )
    return fixes, reports


def comment_for(fix) -> str:
    """One comment per fix: what moved, on what evidence, and why it is not a mistake."""
    return (
        "Board contract check: `status:ready` -> `status:claimed`.\n"
        "\n"
        f"This issue was labelled `{READY}` while {' and '.join(fix['evidence'])} "
        "proved the work is live. `docs/plans/03-DELIVERY.md` § \"Labels and states\" "
        f"defines `{READY}` as *open, no live branch, no linked PR — safe to claim*, so the "
        "label invited a second branch on work that already exists.\n"
        "\n"
        "Restored automatically by `scripts/ci/board.py`; no human action is needed. The "
        "issue is not free to claim."
    )


# --------------------------------------------------------------------------- inputs


def gh(*args: str) -> str:
    proc = subprocess.run(["gh", *args], capture_output=True, text=True)
    if proc.returncode != 0:
        raise RuntimeError(f"gh {' '.join(args)} failed: {proc.stderr.strip()}")
    return proc.stdout


def parse_ts(value: str) -> dt.datetime:
    return dt.datetime.fromisoformat(value.replace("Z", "+00:00"))


def load_issues(repo: str):
    raw = json.loads(
        gh(
            "issue", "list", "--repo", repo, "--state", "open", "--limit", LIMIT,
            "--json", "number,title,labels,updatedAt,body",
        )
    )
    return [
        {
            "number": item["number"],
            "title": item["title"],
            "labels": [label["name"] for label in item["labels"]],
            "updated": parse_ts(item["updatedAt"]),
            "body": item.get("body") or "",
        }
        for item in raw
    ]


def load_prs(repo: str):
    raw = json.loads(
        gh(
            "pr", "list", "--repo", repo, "--state", "open", "--limit", LIMIT,
            "--json", "number,headRefName,closingIssuesReferences",
        )
    )
    return [
        {
            "number": item["number"],
            "head": item["headRefName"],
            "closes": [ref["number"] for ref in item.get("closingIssuesReferences") or []],
        }
        for item in raw
    ]


def load_branches() -> list[str]:
    proc = subprocess.run(
        ["git", "ls-remote", "--heads", "origin"], capture_output=True, text=True
    )
    if proc.returncode != 0:
        raise RuntimeError(f"git ls-remote failed: {proc.stderr.strip()}")
    marker = "refs/heads/"
    return [line.split(marker, 1)[1] for line in proc.stdout.splitlines() if marker in line]


def blocker_states(repo: str):
    """One REST lookup per distinct blocker, memoised. REST, never GraphQL: the budget is
    shared per account and `gh`'s GraphQL is what empties it."""
    cache: dict[int, str] = {}

    def state(number: int) -> str:
        if number not in cache:
            proc = subprocess.run(
                ["gh", "api", f"repos/{repo}/issues/{number}", "--jq", ".state"],
                capture_output=True,
                text=True,
            )
            cache[number] = proc.stdout.strip().lower() if proc.returncode == 0 else "missing"
        return cache[number]

    return state


# --------------------------------------------------------------------------- output


def emit(fixes, reports, dry_run: bool, repo: str) -> int:
    lines = [f"# Board contract — {repo}", ""]
    if fixes:
        verb = "would restore" if dry_run else "restored"
        lines.append(f"## Rule A — {verb} {len(fixes)} label(s)")
        for fix in fixes:
            lines.append(
                f"- #{fix['number']} {fix['title']} — {' and '.join(fix['evidence'])}"
            )
    else:
        lines.append("## Rule A — no violation")
    lines.append("")
    if reports:
        lines.append(f"## Reported, not changed ({len(reports)})")
        for report in reports:
            lines.append(f"- [{report['rule']}] #{report['number']} {report['title']} — {report['why']}")
    else:
        lines.append("## Reported, not changed — nothing to report")
    text = "\n".join(lines) + "\n"

    print(text)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as handle:
            handle.write(text)
    return 0


def apply(fixes, repo: str):
    """Apply rule A. Returns (applied, failed) so the summary reports what actually happened --
    a summary that claims a restore it did not make is the same failure as a stale label."""
    applied, failed = [], []
    for fix in fixes:
        try:
            gh(
                "issue", "edit", str(fix["number"]), "--repo", repo,
                "--add-label", CLAIMED, "--remove-label", READY,
            )
            gh("issue", "comment", str(fix["number"]), "--repo", repo, "--body", comment_for(fix))
            applied.append(fix)
            print(f"fixed #{fix['number']}: ready -> claimed")
        except RuntimeError as error:
            failed.append((fix, str(error)))
            print(f"FAILED #{fix['number']}: {error}", file=sys.stderr)
    return applied, failed


# --------------------------------------------------------------------------- self-test


def self_test() -> int:
    """Fixture matrix. No network, no git, no clock."""
    now = dt.datetime(2026, 11, 15, 19, 0, tzinfo=dt.timezone.utc)

    def issue(number, labels, body="", age_hours=0, title=None):
        return {
            "number": number,
            "title": title or f"issue {number}",
            "labels": list(labels),
            "updated": now - dt.timedelta(hours=age_hours),
            "body": body,
        }

    states = {120: "closed", 151: "open"}

    def blocker_state(number):
        return states.get(number, "missing")

    cases = [
        # name, issues, prs, branches, expected fix numbers, expected report keys
        (
            "ready + pushed branch -> fix",
            [issue(149, [READY])],
            [],
            ["issue/149-work-store"],
            [149],
            [],
        ),
        (
            "ready + open PR by closing reference -> fix",
            [issue(151, [READY])],
            [{"number": 152, "head": "feature-x", "closes": [151]}],
            [],
            [151],
            [],
        ),
        (
            "ready + open PR by head branch name -> fix",
            [issue(151, [READY])],
            [{"number": 152, "head": "issue/151-programme-export", "closes": []}],
            [],
            [151],
            [],
        ),
        ("ready + nothing -> no action", [issue(6, [READY])], [], [], [], []),
        (
            "claimed + branch -> no action",
            [issue(137, [CLAIMED])],
            [],
            ["issue/137-router-metrics"],
            [],
            [],
        ),
        (
            "blocked on an open issue -> no report",
            [issue(72, [BLOCKED], "Blocker: #151\n")],
            [], [], [], [],
        ),
        (
            "blocked naming a closed AND an open blocker -> no report",
            [issue(72, [BLOCKED], "Blocker: #120, #151 — the second still gates it.\n")],
            [], [], [], [],
        ),
        (
            "blocked on a closed issue -> report",
            [issue(121, [BLOCKED], "Blocker: #120 — the P0 seed unit.\n")],
            [], [], [], ["B:121"],
        ),
        (
            "blocked on a missing issue -> report",
            [issue(122, [BLOCKED], "Blocker: #999\n")],
            [], [], [], ["B:122"],
        ),
        (
            "blocked with no blocker line -> report",
            [issue(95, [BLOCKED], "prose mentions #120, not a blocker line")],
            [], [], [], ["B:95"],
        ),
        (
            "blocked, blocker not at line start (backticked) -> report",
            [issue(121, [BLOCKED], "`Blocker: #151`\n")],
            [], [], [], ["B:121"],
        ),
        (
            "blocked, blocker only in a checklist -> report",
            [issue(30, [BLOCKED], "- [x] Dependencies typed; `Blocker: #151` present.\n")],
            [], [], [], ["B:30"],
        ),
        (
            "claimed, no branch, stale -> report",
            [issue(6, [CLAIMED], age_hours=60)],
            [], [], [], ["C:6"],
        ),
        (
            "claimed, no branch, fresh -> no report",
            [issue(6, [CLAIMED], age_hours=2)],
            [], [], [], [],
        ),
        ("needs-human -> ignored", [issue(97, [NEEDS_HUMAN])], [], [], [], []),
    ]

    failures = 0
    for name, issues, prs, branches, want_fixes, want_reports in cases:
        fixes, reports = plan(issues, prs, branches, blocker_state, now)
        got_fixes = [fix["number"] for fix in fixes]
        got_reports = [f"{report['rule']}:{report['number']}" for report in reports]
        if got_fixes == want_fixes and got_reports == want_reports:
            print(f"ok: {name}")
        else:
            failures += 1
            print(
                f"FAIL: {name} (fixes {got_fixes} != {want_fixes}, "
                f"reports {got_reports} != {want_reports})"
            )

    # The comment is the only thing a human reads on the issue; it must name its evidence.
    fix = {"number": 149, "branch": "issue/149-work-store", "pr": 150,
           "evidence": ["branch `issue/149-work-store`", "open PR #150"]}
    body = comment_for(fix)
    for needle in ("status:claimed", "#150", "issue/149-work-store", "not free to claim"):
        if needle in body:
            print(f"ok: comment names {needle!r}")
        else:
            failures += 1
            print(f"FAIL: comment omits {needle!r}")

    print(f"\n{len(cases) + 1 - failures} passing, {failures} failing")
    return 1 if failures else 0


# --------------------------------------------------------------------------- main


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--repo", help="owner/name, e.g. nonagenticai/conveyor")
    parser.add_argument("--dry-run", action="store_true", help="print the plan, write nothing")
    parser.add_argument("--self-test", action="store_true", help="run the fixture matrix")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if not args.repo:
        parser.error("--repo is required unless --self-test")

    try:
        issues = load_issues(args.repo)
        prs = load_prs(args.repo)
        branches = load_branches()
    except (RuntimeError, json.JSONDecodeError) as error:
        print(f"board: {error}", file=sys.stderr)
        return 2

    fixes, reports = plan(
        issues, prs, branches, blocker_states(args.repo), dt.datetime.now(dt.timezone.utc)
    )

    if args.dry_run:
        emit(fixes, reports, True, args.repo)
        return 1 if fixes else 0

    applied, failed = apply(fixes, args.repo)
    for fix, error in failed:
        reports.append(
            {"rule": "A", "number": fix["number"], "title": fix["title"],
             "why": f"fix failed: {error}"}
        )
    emit(applied, reports, False, args.repo)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
