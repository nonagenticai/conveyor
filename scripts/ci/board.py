#!/usr/bin/env python3
"""Check the issue board against the label contract in docs/plans/03-DELIVERY.md.

WHY THIS FILE EXISTS
--------------------
`docs/plans/03-DELIVERY.md` § "Labels and states" defines the states as a contract:

    ready   status:ready    open, no `## Claim` block, no live branch, no linked PR
    claimed status:claimed  branch pushed + claim comment, work in progress
    blocked status:blocked  the body carries a plain `Blocker: #N` naming an OPEN issue

Three of those clauses are checkable without reading a sentence of prose -- the branch, the
open PR, and the blocker line -- and until now none of them was checked. The `## Claim` block is
deliberately NOT read: a released claim usually leaves a stale body cell behind, and re-claiming
on that would be a worse failure than missing it.
The label is written by hand after the branch is pushed, so it is the step that gets
skipped, and a stale `status:ready` is a licence to start a second branch on work that
already exists. Measured on 2026-11-15: #137 and #140 carried `ready` with live branches
(repaired by hand at 15:47Z), then #149 and #151 were created as `ready` at 15:56Z/16:35Z
and claimed without the label moving; #121 and #122 stayed `blocked` on #120 after it
closed at 00:37:54Z.

`reap-stale-claims.sh` cannot catch any of this, by design: *a claim is a REMOTE BRANCH*,
so it reads branches and never labels. This reads the labels and the branches together.

THE FOUR RULES, AND WHICH OF THEM ACT
-------------------------------------
A (acts)    `status:ready` + a live branch or an open PR -> `status:claimed`, one comment.
            Safe because it moves in ONE direction only. A check that could also move an
            issue back to `ready` would be able to steal a claim, which is a worse failure
            than the drift it repairs.
B (acts,    `status:blocked` whose EVERY named blocker is positively `closed` ->
   narrow)  `status:ready`, one comment. The contract's own clause is "naming an OPEN
            issue", so a block whose blockers are all closed is factually over. It never
            acts on a claimed issue, and never on a blocker whose state is `missing` or
            `unknown`: a typo or a failed lookup is not evidence of closure. Those, and a
            body with no conforming blocker line, are reported only.
C (reports) `status:claimed` with no branch, no PR, and no update in 48 h — the reaper's
            two tests applied to the label. Reporting only: releasing a human's claim
            stays a human decision, which is also why the reaper defaults to report mode.
P (reports) a `unit` issue created on or after PROGRAMME_SINCE with no `## Programme` section,
            or with a field row empty. The delivery contract makes the four programme fields
            (tranche, brief, ADM phase/plateau, benefits) part of the Definition of ready for
            issues created on/after that date; the report is how a skipped field is seen.
            Issues created before the change are exempt until claimed (forward-only rule).

A rule's evidence is always printed. A wrong fix is silent; a printed verdict is not, and
the job summary is the record a later session reads.

USAGE
    board.py --repo nonagenticai/conveyor            # apply A and stale B, report C and P
    board.py --repo nonagenticai/conveyor --dry-run  # print the plan, write nothing
    board.py --repo nonagenticai/conveyor --dor N    # check issue N's Definition of ready
    board.py --self-test                             # fixtures only, no network, no git

EXIT CODES
    0  clean, or every fix applied; `--dor` found no gap
    1  --dry-run found a fix pending, a fix failed, or `--dor` found a Programme gap
    2  the instrument is broken (gh failed, git failed, malformed input)

The fallbacks are deliberately unlike the reaper's. This script fails OPEN on a lookup it
cannot complete, because the alternative -- relabelling on incomplete evidence -- edits a
shared board; it writes only on a positively-identified live branch/PR (rule A) or a
positively-closed blocker set (rule B).
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
UNIT = "unit"

# `03-DELIVERY.md` § "Issue content contract": the programme context is required for issues
# created on/after this date; an earlier issue gains it at its next claim. Keep the two in step.
PROGRAMME_SINCE = dt.date(2026, 11, 18)
PROGRAMME_HEADING = "## Programme"
PROGRAMME_FIELDS = ("Tranche", "Brief", "ADM phase/plateau", "Benefits")

# `03-DELIVERY.md`: "A `status:blocked` issue carries a plain line STARTING `Blocker: #N`;
# a `Blocker:` mention inside prose or a checklist is not a blocker line." So the line must
# begin with the token -- that is what excludes the `- [x] ... `Blocker: #N` present.`
# checklists and mid-sentence prose.
#
# Only the RUN of issue references immediately after the colon are blockers. A reason follows
# them and routinely cites other issues as evidence ("the crate is on main (#45 merged), the
# publish mechanism is live (#98)"); counting those would keep a block alive on a number that
# was never a blocker, or report a stale block on one that only appeared in prose.
BLOCKER_LINE_RE = re.compile(
    r"^[ \t]*Blocker:[ \t]*((?:#\d+(?:[ \t]*[,/][ \t]*)?[ \t]*)+)", re.MULTILINE
)
ISSUE_REF_RE = re.compile(r"#(\d+)")

STALE_CLAIM_HOURS = 48
LIMIT = "500"


# --------------------------------------------------------------------------- rules


def blockers_of(body: str) -> list[int]:
    """Issue numbers named by every line that starts with `Blocker:`.

    Union, not first-line-only: a body that carries two blocker lines has been edited, and
    whichever line is newer is the one a reader would believe -- so keeping the block alive
    while ANY named blocker is open is the direction that cannot invent a stale block.

    Empty list means the issue carries no conforming blocker line at all.
    """
    seen: list[int] = []
    for match in BLOCKER_LINE_RE.finditer(body):
        for number in ISSUE_REF_RE.findall(match.group(1)):
            value = int(number)
            if value not in seen:
                seen.append(value)
    return seen


def programme_gaps(body: str) -> list[str]:
    """The programme fields this body is missing or leaves empty.

    `[]` means the section is present and all four rows carry a value. A body with no
    `## Programme` heading returns every field name, so the report says what to add rather than
    only that something is absent. A row value of `—` (the template's ready-state placeholder)
    counts as empty: the template's own blank must not pass as a filled field.
    """
    lines = body.splitlines()
    try:
        start = next(i for i, line in enumerate(lines) if line.strip() == PROGRAMME_HEADING)
    except StopIteration:
        return list(PROGRAMME_FIELDS)
    gaps: list[str] = []
    found: set[str] = set()
    for line in lines[start + 1 :]:
        if line.startswith("## "):
            break
        # Template rows: `| Field | Value |`. The heading row and the separator are skipped by
        # the field-name match; a value that is the placeholder or blank is a gap.
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) != 2:
            continue
        field, value = cells
        if field not in PROGRAMME_FIELDS:
            continue
        found.add(field)
        if not value or value in {"—", "-"}:
            gaps.append(field)
    for field in PROGRAMME_FIELDS:
        if field not in found:
            gaps.append(field)
    return gaps


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
                    # `status:blocked` is not claimable. When an issue somehow carries both, the
                    # dangerous label is `ready`: remove it, and leave the block exactly as it is
                    # rather than producing an issue with two state labels.
                    "blocked": BLOCKED in labels,
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
            named = ", ".join(f"#{b} {s}" for b, s in states.items())
            # A block is real while ANY named blocker is open.
            if any(state == "open" for state in states.values()):
                continue
            # A missing number is a typo or a transferred issue, not proof of closure; an
            # unknown state is a lookup that failed. Neither may release work, and the
            # missing case is reported because its fix is a body edit, not a wait.
            if any(state == "missing" for state in states.values()):
                reports.append(
                    {
                        "rule": "B",
                        "number": number,
                        "title": issue["title"],
                        "why": f"not released; a named blocker cannot be proven closed ({named})",
                    }
                )
                continue
            if any(state == "unknown" for state in states.values()):
                continue
            # Every named blocker is positively closed. Releasing a stale block is mechanical;
            # releasing a claim is still a human decision.
            if CLAIMED in labels:
                reports.append(
                    {
                        "rule": "B",
                        "number": number,
                        "title": issue["title"],
                        "why": (
                            f"every named blocker is closed ({named}), but the issue is "
                            f"`{CLAIMED}`; not released"
                        ),
                    }
                )
                continue
            fixes.append(
                {
                    "rule": "B",
                    "release": True,
                    "number": number,
                    "title": issue["title"],
                    "evidence": [
                        f"#{blocker} {state}" for blocker, state in states.items()
                    ],
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

        # Rule P: the programme context. Forward-only: an issue created before the contract
        # changed is exempt (it gains the section at its next claim), and this rule never edits.
        if UNIT in labels and issue["created"].date() >= PROGRAMME_SINCE:
            gaps = programme_gaps(issue["body"])
            if gaps:
                reports.append(
                    {
                        "rule": "P",
                        "number": number,
                        "title": issue["title"],
                        "why": "no `## Programme` section or empty field(s): " + ", ".join(gaps),
                    }
                )
    return fixes, reports


def comment_for(fix) -> str:
    """One comment per fix: what moved, on what evidence, and why it is not a mistake."""
    if fix.get("blocked"):
        head = (
            "Board contract check: the `status:ready` label is removed; the issue keeps "
            "`status:blocked`."
        )
    else:
        head = "Board contract check: `status:ready` -> `status:claimed`."
    return (
        f"{head}\n"
        "\n"
        f"This issue was labelled `{READY}` while {' and '.join(fix['evidence'])} "
        "proved the work is live. `docs/plans/03-DELIVERY.md` § \"Labels and states\" "
        f"defines `{READY}` as *open, no live branch, no linked PR — safe to claim*, so the "
        "label invited a second branch on work that already exists.\n"
        "\n"
        "Restored automatically by `scripts/ci/board.py`; no human action is needed. The "
        "issue is not free to claim."
    )


def release_comment_for(fix) -> str:
    """The release comment names the evidence that retired the block, not just the move."""
    named = ", ".join(fix["evidence"])
    return (
        f"Board contract check: `{BLOCKED}` -> `{READY}`.\n"
        "\n"
        f"Every issue named on this issue's `Blocker:` line is closed: {named}. "
        f"`docs/plans/03-DELIVERY.md` § \"Labels and states\" defines `{BLOCKED}` as the body "
        "carrying a plain `Blocker: #N` naming an OPEN issue, so the block no longer holds.\n"
        "\n"
        "Released automatically by `scripts/ci/board.py`; no human action is needed. The "
        "issue is free to claim."
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
            "--json", "number,title,labels,createdAt,updatedAt,body",
        )
    )
    return [
        {
            "number": item["number"],
            "title": item["title"],
            "labels": [label["name"] for label in item["labels"]],
            "created": parse_ts(item["createdAt"]),
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
            # "unknown" is not "missing": a lookup that failed is not evidence that the blocker
            # closed, and reporting a stale block on a failed lookup would be a lie that costs
            # a human a trip to the issue. Rule B only reports on closed or missing.
            cache[number] = proc.stdout.strip().lower() if proc.returncode == 0 else "unknown"
        return cache[number]

    return state


def check_dor(repo: str, number: int) -> int:
    """`--dor N`: the Programme half of the Definition of ready, for the pickup gate.

    Exit 1 names the missing section/fields; exit 0 means the issue may be claimed on this
    rule. Forward-only: an issue created before PROGRAMME_SINCE is exempt, and a non-unit
    issue has no Programme requirement.
    """
    try:
        raw = json.loads(
            gh(
                "issue", "view", str(number), "--repo", repo,
                "--json", "number,labels,createdAt,body",
            )
        )
    except (RuntimeError, json.JSONDecodeError) as error:
        print(f"board: {error}", file=sys.stderr)
        return 2
    labels = {label["name"] for label in raw["labels"]}
    created = parse_ts(raw["createdAt"]).date()
    if UNIT not in labels:
        print(f"#{number}: not a `{UNIT}` issue; no Programme requirement.")
        return 0
    if created < PROGRAMME_SINCE:
        print(f"#{number}: created {created}, before {PROGRAMME_SINCE}; exempt (forward-only).")
        return 0
    gaps = programme_gaps(raw.get("body") or "")
    if gaps:
        print(f"#{number}: not ready — `## Programme` missing or empty: {', '.join(gaps)}")
        return 1
    print(f"#{number}: Programme complete; ready on this rule.")
    return 0


# --------------------------------------------------------------------------- output


def emit(fixes, reports, dry_run: bool, repo: str) -> int:
    lines = [f"# Board contract — {repo}", ""]
    claims = [fix for fix in fixes if not fix.get("release")]
    releases = [fix for fix in fixes if fix.get("release")]
    if claims:
        verb = "would restore" if dry_run else "restored"
        lines.append(f"## Rule A — {verb} {len(claims)} label(s)")
        for fix in claims:
            lines.append(
                f"- #{fix['number']} {fix['title']} — {' and '.join(fix['evidence'])}"
            )
    else:
        lines.append("## Rule A — no violation")
    lines.append("")
    if releases:
        verb = "would release" if dry_run else "released"
        lines.append(f"## Rule B — {verb} {len(releases)} stale block(s)")
        for fix in releases:
            lines.append(
                f"- #{fix['number']} {fix['title']} — {' and '.join(fix['evidence'])}"
            )
    else:
        lines.append("## Rule B — no stale block")
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
    """Apply rule A and a stale-block rule B. Returns (applied, failed) so the summary reports
    what actually happened -- a summary that claims a move it did not make is the same failure
    as a stale label."""
    applied, failed = [], []
    for fix in fixes:
        try:
            if fix.get("release"):
                gh(
                    "issue", "edit", str(fix["number"]), "--repo", repo,
                    "--remove-label", BLOCKED, "--add-label", READY,
                )
                gh(
                    "issue", "comment", str(fix["number"]), "--repo", repo,
                    "--body", release_comment_for(fix),
                )
                outcome = "blocked -> ready"
            else:
                edit = [
                    "issue", "edit", str(fix["number"]), "--repo", repo,
                    "--remove-label", READY,
                ]
                if not fix.get("blocked"):
                    edit += ["--add-label", CLAIMED]
                gh(*edit)
                gh(
                    "issue", "comment", str(fix["number"]), "--repo", repo,
                    "--body", comment_for(fix),
                )
                outcome = "ready removed, block kept" if fix.get("blocked") else "ready -> claimed"
            applied.append(fix)
            print(f"fixed #{fix['number']}: {outcome}")
        except RuntimeError as error:
            failed.append((fix, str(error)))
            print(f"FAILED #{fix['number']}: {error}", file=sys.stderr)
    return applied, failed


# --------------------------------------------------------------------------- self-test


def self_test() -> int:
    """Fixture matrix. No network, no git, no clock."""
    now = dt.datetime(2026, 11, 15, 19, 0, tzinfo=dt.timezone.utc)

    def issue(number, labels, body="", age_hours=0, title=None, created=None):
        return {
            "number": number,
            "title": title or f"issue {number}",
            "labels": list(labels),
            "created": created or now - dt.timedelta(days=30),
            "updated": now - dt.timedelta(hours=age_hours),
            "body": body,
        }

    # 777 is a lookup that could not be completed; anything absent never existed.
    states = {120: "closed", 151: "open", 777: "unknown"}

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
            # Two state labels is already invalid; resolve it without creating a third shape.
            "ready AND blocked + branch -> fix that keeps the block",
            [issue(72, [READY, BLOCKED], "Blocker: #151\n")],
            [], ["issue/72-plan-spine"],
            [72], [],
        ),
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
            "stale block, every named blocker closed -> release",
            [issue(121, [BLOCKED], "Blocker: #120 — the P0 seed unit.\n")],
            [], [], [121], [],
        ),
        (
            "stale block, closed + missing -> report only",
            [issue(123, [BLOCKED], "Blocker: #120, #999\n")],
            [], [], [], ["B:123"],
        ),
        (
            "stale block, closed + unknown -> no report",
            [issue(124, [BLOCKED], "Blocker: #120, #777\n")],
            [], [], [], [],
        ),
        (
            "stale block on a claimed issue -> report only",
            [issue(125, [CLAIMED, BLOCKED], "Blocker: #120\n")],
            [], [], [], ["B:125"],
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
            "blocker's reason cites other issues -> only the leading run counts",
            # #120 is closed; #999/#151 are cited in the reason. The block is #120 only, so it
            # releases even though an issue named in the prose is open.
            [issue(95, [BLOCKED], "Blocker: #120 — superseded by #999, see #151 too.\n")],
            [], [], [95], [],
        ),
        (
            "two blocker lines -> union, one open keeps the block",
            [issue(72, [BLOCKED], "Blocker: #120\nBlocker: #151 — corrected.\n")],
            [], [], [], [],
        ),
        (
            "blocker lookup failed -> no stale report",
            [issue(13, [BLOCKED], "Blocker: #777\n")],
            [], [], [], [],
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
        (
            # The contract is forward-only: an issue created before PROGRAMME_SINCE keeps its
            # historical body; it gains the section at its next claim.
            "unit created before the programme contract -> no report",
            [issue(280, [UNIT, READY])],
            [], [], [], [],
        ),
        (
            "unit created after the programme contract, no section -> report P",
            [issue(280, [UNIT, READY], created=now + dt.timedelta(days=4))],
            [], [], [], ["P:280"],
        ),
        (
            "unit created after the programme contract, section filled -> no report",
            [
                issue(
                    280,
                    [UNIT, READY],
                    body=(
                        "## Programme\n\n"
                        "| Field | Value |\n|---|---|\n"
                        "| Tranche | 1 |\n"
                        "| Brief | project-brief-p0 |\n"
                        "| ADM phase/plateau | A / PSA |\n"
                        "| Benefits | benefit-personal-throughput |\n"
                    ),
                    created=now + dt.timedelta(days=4),
                )
            ],
            [], [], [], [],
        ),
        (
            "unit created after the programme contract, empty Tranche -> report P",
            [
                issue(
                    280,
                    [UNIT, READY],
                    body=(
                        "## Programme\n\n"
                        "| Field | Value |\n|---|---|\n"
                        "| Tranche | — |\n"
                        "| Brief | pending #122 |\n"
                        "| ADM phase/plateau | — (no architecture change) |\n"
                        "| Benefits | enabling — no direct benefit |\n"
                    ),
                    created=now + dt.timedelta(days=4),
                )
            ],
            [], [], [], ["P:280"],
        ),
    ]

    failures = 0
    total = 0
    for name, issues, prs, branches, want_fixes, want_reports in cases:
        total += 1
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
        total += 1
        if needle in body:
            print(f"ok: comment names {needle!r}")
        else:
            failures += 1
            print(f"FAIL: comment omits {needle!r}")

    # The release comment must name the closed blockers, not just the move.
    release = {"number": 121, "title": "the P0 seed unit", "evidence": ["#120 closed"]}
    body = release_comment_for(release)
    for needle in ("status:ready", "#120 closed", "free to claim"):
        total += 1
        if needle in body:
            print(f"ok: release comment names {needle!r}")
        else:
            failures += 1
            print(f"FAIL: release comment omits {needle!r}")

    # `apply()` is the half that moves the label; a planner-only fixture would not catch a
    # release fix that forgot to remove `status:blocked`. Record the gh calls, write nothing.
    global gh
    recorded = []
    original = gh

    def recorder(*args):
        recorded.append(list(args))
        return ""

    gh = recorder
    try:
        applied, failed = apply(
            [{"rule": "B", "release": True, "number": 121, "title": "the P0 seed unit",
              "evidence": ["#120 closed"]}],
            "owner/repo",
        )
    finally:
        gh = original
    edit_args = recorded[0] if recorded else []
    total += 1
    if (
        not failed
        and applied
        and "--remove-label" in edit_args
        and BLOCKED in edit_args
        and "--add-label" in edit_args
        and READY in edit_args
        and any("free to claim" in part for row in recorded for part in row)
    ):
        print("ok: release fix removes blocked and adds ready")
    else:
        failures += 1
        print(f"FAIL: release fix did not move the label (recorded {recorded!r})")

    # A ready+blocked issue must lose `ready` without being claimed: `blocked` is not claimable,
    # and a fix that added `claimed` would leave two state labels on the issue.
    total += 1
    conflict, _ = plan(
        [issue(72, [READY, BLOCKED], "Blocker: #151\n")],
        [], ["issue/72-plan-spine"], blocker_state, now,
    )
    if conflict and conflict[0]["blocked"] and "keeps `status:blocked`" in comment_for(conflict[0]):
        print("ok: ready+blocked keeps the block instead of claiming it")
    else:
        failures += 1
        print("FAIL: ready+blocked would claim an issue that is still blocked")

    print(f"\n{total - failures} passing, {failures} failing")
    return 1 if failures else 0


# --------------------------------------------------------------------------- main


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--repo", help="owner/name, e.g. nonagenticai/conveyor")
    parser.add_argument("--dry-run", action="store_true", help="print the plan, write nothing")
    parser.add_argument("--self-test", action="store_true", help="run the fixture matrix")
    parser.add_argument(
        "--dor", type=int, metavar="N", help="check issue N's Programme Definition of ready"
    )
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if not args.repo:
        parser.error("--repo is required unless --self-test")
    if args.dor is not None:
        return check_dor(args.repo, args.dor)

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
