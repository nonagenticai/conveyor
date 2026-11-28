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
Q (writes with --apply)
            the full issue content contract for one unit, on the event that changes the body:
            every contract section present, the Programme rows filled, and at least one
            `Verify:` line. For a unit created on/after STORY_SINCE (minus STORY_EXEMPT) also
            the `## User story`, a Gherkin `Scenario:` with `@verify` per part, and one DoD row
            per scenario. The `issue-dor` workflow runs it on `issues: opened, edited`; the
            heading parser accepts `##` and `###`, because the GitHub issue form renders h3.

A rule's evidence is always printed. A wrong fix is silent; a printed verdict is not, and
the job summary is the record a later session reads.

USAGE
    board.py --repo nonagenticai/conveyor            # apply A and stale B, report C and P
    board.py --repo nonagenticai/conveyor --dry-run  # print the plan, write nothing
    board.py --repo nonagenticai/conveyor --dor N    # check issue N's Definition of ready
    board.py --repo nonagenticai/conveyor --contract N [--apply]
                                                     # the full issue content contract; --apply
                                                     # writes status:not-ready and one comment
    board.py --contract-body PATH                    # the same check over a body file (offline)
    board.py --self-test                             # fixtures only, no network, no git

EXIT CODES
    0  clean, or every fix applied; `--dor`/`--contract` found no gap
    1  --dry-run found a fix pending, a fix failed, or `--dor`/`--contract` found a gap
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
from pathlib import Path

# The pure parser half of this instrument. A sibling module, so other readers (the proof
# collector, the readiness derivation) parse a body exactly as the claim gate does.
sys.path.insert(0, str(Path(__file__).resolve().parent))
import issue_contract as contract  # noqa: E402 — after the path insert

READY = "status:ready"
CLAIMED = "status:claimed"
BLOCKED = "status:blocked"
NEEDS_HUMAN = "status:needs-human"
UNIT = "unit"

# The contract constants and pure parsers live in `issue_contract.py`. These aliases keep the
# names this file, its CLI and its existing readers (`scripts/proof/collect_units.py`) already
# use; the rules, their docstrings and the forward-only dates are in that module.
PROGRAMME_SINCE = contract.PROGRAMME_SINCE
PROGRAMME_FIELDS = contract.PROGRAMME_FIELDS
INVEST_SINCE = contract.INVEST_SINCE
INVEST_CRITERIA = contract.INVEST_CRITERIA
STORY_SINCE = contract.STORY_SINCE
STORY_EXEMPT = contract.STORY_EXEMPT
CONTRACT_SECTIONS = contract.CONTRACT_SECTIONS
SECTION_RE = contract.SECTION_RE
VERIFY_RE = contract.VERIFY_RE
KNOWN_SECTIONS = contract.KNOWN_SECTIONS
section_lines = contract.section_lines
programme_gaps = contract.programme_gaps
invest_gaps = contract.invest_gaps
story_gaps = contract.story_gaps
scenario_gaps = contract.scenario_gaps
dod_gaps = contract.dod_gaps
contract_gaps = contract.contract_gaps
story_required = contract.story_required

# The label and the comment the event workflow applies (#780 P2). The marker makes the comment a
# single, updatable record rather than one new comment per edit.
DOR_LABEL = "status:not-ready"
DOR_MARKER = "<!-- issue-dor -->"

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
                reports.append(
                    {
                        "rule": "B",
                        "number": number,
                        "title": issue["title"],
                        "why": (
                            "not released; a named blocker's state could not be "
                            f"determined ({named})"
                        ),
                    }
                )
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
            # closed. Rule B releases only on a positively closed set; a missing or unknown
            # blocker is reported and never released.
            cache[number] = proc.stdout.strip().lower() if proc.returncode == 0 else "unknown"
        return cache[number]

    return state


def check_dor(repo: str, number: int) -> int:
    """`--dor N`: the Definition-of-ready half of the contract, for the pickup gate.

    Exit 1 names the missing section/fields; exit 0 means the issue may be claimed on this
    rule. Forward-only: an issue created before PROGRAMME_SINCE is exempt, the INVEST/DoD rows
    start at INVEST_SINCE, and the story/Gherkin shape starts at STORY_SINCE (minus
    STORY_EXEMPT); a non-unit issue has no requirement.
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
    body = raw.get("body") or ""
    gaps = programme_gaps(body)
    if created >= INVEST_SINCE:
        gaps.extend(invest_gaps(body))
        gaps.extend(dod_gaps(body))
    if story_required(created, number):
        gaps.extend(story_gaps(body))
        gaps.extend(scenario_gaps(body))
    if gaps:
        print(f"#{number}: not ready — {', '.join(gaps)}")
        return 1
    print(f"#{number}: the Programme, INVEST, acceptance and story contract is complete; ready.")
    return 0


def contract_message(gaps: list[str]) -> str:
    """The one comment the event workflow posts (and later updates) on a not-ready body."""
    lines = [
        DOR_MARKER,
        'The issue body does not yet meet `docs/plans/03-DELIVERY.md` § "Issue content contract":',
        "",
        *[f"- {gap}" for gap in gaps],
        "",
        "Edit the issue; this comment is updated in place, and `status:not-ready` comes off when",
        "every item is present.",
    ]
    return "\n".join(lines)


def marked_comment(repo: str, number: int) -> int | None:
    """The id of the contract comment already on the issue, if any."""
    try:
        comments = json.loads(gh("api", f"repos/{repo}/issues/{number}/comments?per_page=100"))
    except (RuntimeError, json.JSONDecodeError):
        return None
    for comment in comments:
        if (comment.get("body") or "").startswith(DOR_MARKER):
            return int(comment["id"])
    return None


def mark_not_ready(repo: str, number: int, body: str) -> None:
    """Apply `status:not-ready` and upsert the one marked comment."""
    try:
        gh(
            "label", "create", DOR_LABEL, "--repo", repo, "--color", "B60205",
            "--description", "The issue body does not yet meet the content contract",
        )
    except RuntimeError:
        pass  # it already exists; creating it is an idempotent convenience
    gh("issue", "edit", str(number), "--repo", repo, "--add-label", DOR_LABEL)
    existing = marked_comment(repo, number)
    if existing is None:
        gh("issue", "comment", str(number), "--repo", repo, "--body", body)
    else:
        gh("api", "-X", "PATCH", f"repos/{repo}/issues/comments/{existing}", "-f", f"body={body}")


def clear_not_ready(repo: str, number: int) -> None:
    """Remove the label and the marked comment once the body is complete."""
    try:
        gh("issue", "edit", str(number), "--repo", repo, "--remove-label", DOR_LABEL)
    except RuntimeError:
        pass  # the label was not set
    existing = marked_comment(repo, number)
    if existing is not None:
        try:
            gh("api", "-X", "DELETE", f"repos/{repo}/issues/comments/{existing}")
        except RuntimeError:
            pass


def check_contract(repo: str, number: int, apply: bool) -> int:
    """`--contract N`: the full issue content contract, on the event that changes the body.

    Exit 1 names every missing section/field; exit 0 means the body satisfies the contract. With
    `--apply` it also sets `status:not-ready` and one marked comment on a gap, and clears both
    when the body is complete. Forward-only like `--dor`: an issue created before PROGRAMME_SINCE
    is exempt, the story/Gherkin and INVEST halves start at their SINCE dates, and a non-unit
    issue has no contract requirement.
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
    if UNIT not in labels:
        print(f"#{number}: not a `{UNIT}` issue; no contract requirement.")
        return 0
    created = parse_ts(raw["createdAt"]).date()
    if created < PROGRAMME_SINCE:
        print(f"#{number}: created {created}, before {PROGRAMME_SINCE}; exempt (forward-only).")
        return 0
    gaps = contract_gaps(
        raw.get("body") or "",
        require_invest=created >= INVEST_SINCE,
        require_story=story_required(created, number),
    )
    if not gaps:
        if apply:
            clear_not_ready(repo, number)
        print(f"#{number}: the issue content contract is complete.")
        return 0
    if apply:
        mark_not_ready(repo, number, contract_message(gaps))
        print(f"#{number}: not ready — {len(gaps)} gap(s) named in the issue comment.")
    else:
        print(f"#{number}: not ready — missing {', '.join(gaps)}")
    return 1


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
            "stale block, closed + unknown -> report only",
            [issue(124, [BLOCKED], "Blocker: #120, #777\n")],
            [], [], [], ["B:124"],
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
            "blocker lookup failed -> report only",
            [issue(13, [BLOCKED], "Blocker: #777\n")],
            [], [], [], ["B:13"],
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

    # The issue content contract (#780 P2). The GitHub form renders every field as `### Label`,
    # so a conforming body must parse at h3 as well as h2, and a part's `### P1` subheading must
    # not end its section. A missing section or Verify line is named field by field.
    programme = (
        "| Field | Value |\n|---|---|\n"
        "| Tranche | 1 |\n"
        "| Brief | project-brief-p0 |\n"
        "| ADM phase/plateau | — (no architecture change) |\n"
        "| Benefits | enabling — no direct benefit |\n"
    )

    def contract_body(level: str) -> str:
        sections = [
            ("Goal", "One outcome that is true after this lands."),
            ("Programme", programme),
            ("References", "| Kind | Reference | Resolves at |\n|---|---|---|"),
            ("Baseline", "- Code: `main@abc1234`, measured 2026-11-26"),
            (
                "User story",
                "As a delivery session, I want the unit contract enforced, so that a unit is "
                "ready before it is claimed.",
            ),
            (
                "Parts and acceptance criteria",
                "### P1 — the part\n"
                "- **P1.1** Given a state, when an act, then an outcome.\n"
                "  @verify=P1.1\n"
                "  Scenario: the part does its job\n"
                "    Given a state\n"
                "    When an act\n"
                "    Then an outcome\n"
                "  Verify: `true`",
            ),
            ("Dependencies", "| Relation | Issue | Why | State |\n|---|---|---|---|"),
            (
                "Definition of ready",
                "- [x] **Independent** — no `blocked-by`; the parser is one new file.\n"
                "- [x] **Negotiable** — the card fixes the checker's shape.\n"
                "- [x] **Valuable** — removes gap F2 in `07-BDD-DELIVERY.md`.\n"
                "- [x] **Estimable** — four files named in the card; baseline `main@a037cd90`.\n"
                "- [x] **Small** — four files and one PR for #934.\n"
                "- [x] **Testable** — `P1.1` runs `board.py --self-test`.",
            ),
            (
                "Definition of done",
                "| Acceptance scenario | Evidence at completion | Verify: |\n|---|---|---|\n"
                "| P1.1 | to fill | `true` |",
            ),
        ]
        return "\n\n".join(f"{level} {name}\n{text}" for name, text in sections)

    contract_cases = [
        ("conforming h2 body", contract_body("##"), []),
        ("conforming issue-form body (h3)", contract_body("###"), []),
        (
            "the template's fenced Gherkin and @verify-keyed DoD",
            contract_body("###")
            .replace(
                "- **P1.1** Given a state, when an act, then an outcome.\n"
                "  @verify=P1.1\n"
                "  Scenario: the part does its job\n"
                "    Given a state\n"
                "    When an act\n"
                "    Then an outcome\n"
                "  Verify: `true`",
                "```gherkin\n"
                "Feature: the part\n\n"
                "  @verify=P1.1\n"
                "  Scenario: the part does its job\n"
                "    Given a state\n"
                "    When an act\n"
                "    Then an outcome\n"
                "```\n\n"
                "Verify: `true` — P1.1",
            )
            .replace(
                "| P1.1 | to fill | `true` |",
                "| `@verify=P1.1` | to fill | `true` |",
            ),
            [],
        ),
        (
            "two parts, one Verify line",
            contract_body("###").replace(
                "  Verify: `true`",
                "  Verify: `true`\n\n### P2 — the second part\n"
                "- **P2.1** Given a state, when an act, then an outcome.",
            ),
            ["a `Verify:` line in every part"],
        ),
        (
            "two parts, each with a Verify line and a DoD row",
            contract_body("###")
            .replace(
                "  Verify: `true`",
                "  Verify: `true`\n\n### P2 — the second part\n"
                "- **P2.1** Given a state, when an act, then an outcome.\n"
                "  @verify=P2.1\n"
                "  Scenario: the second part does its job\n"
                "    Given a state\n"
                "    When an act\n"
                "    Then an outcome\n"
                "  Verify: `true`",
            )
            .replace(
                "| P1.1 | to fill | `true` |",
                "| P1.1 | to fill | `true` |\n| P2.1 | to fill | `true` |",
            ),
            [],
        ),
        (
            "a missing section",
            contract_body("###").replace("### Baseline", "### Base line"),
            ["`Baseline` section"],
        ),
        (
            "no Verify line",
            contract_body("###").replace("Verify:", "verify later"),
            ["a `Verify:` line"],
        ),
        (
            "an empty Programme value",
            contract_body("###").replace("| Tranche | 1 |", "| Tranche | — |"),
            ["`Programme` rows: Tranche"],
        ),
        (
            "an empty section",
            contract_body("###").replace(
                "### Baseline\n- Code: `main@abc1234`, measured 2026-11-26", "### Baseline"
            ),
            ["`Baseline` section is empty"],
        ),
        (
            "a DoR without the INVEST criteria",
            contract_body("###").replace("**Small**", "**Sized**"),
            ["the DoR does not name INVEST: Small"],
        ),
        (
            "a generic INVEST restatement",
            # The template's Small phrase keeps `PR` uppercase: the comparison is
            # case-insensitive, so this fixture catches a case-sensitive regression.
            contract_body("###").replace(
                "- [x] **Small** — four files and one PR for #934.",
                "- [x] **Small** — one issue, one branch, one PR.",
            ),
            ["the DoR row for Small is the template's restatement, not evidence"],
        ),
        (
            "a copied template placeholder in the DoR",
            contract_body("###").replace(
                "- [x] **Independent** — no `blocked-by`; the parser is one new file.",
                "- [x] **Independent** — <why this unit is independent: name its files, its "
                "typed dependency, if any.>",
            ),
            ["the DoR row for Independent is the template's restatement, not evidence"],
        ),
        (
            "a DoD with no acceptance rows",
            contract_body("###").replace(
                "| Acceptance scenario | Evidence at completion | Verify: |\n|---|---|---|\n"
                "| P1.1 | to fill | `true` |",
                "- [ ] One PR `Closes #N` merged",
            ),
            ["the DoD table names no `Verify:` column"],
        ),
        (
            "a missing user story",
            contract_body("###").replace("### User story", "### Story"),
            ["`User story` section"],
        ),
        (
            "a scenario with no @verify tag",
            contract_body("###").replace("  @verify=P1.1\n", ""),
            ["the P1 part has no Gherkin scenario with @verify"],
        ),
        (
            "a DoD without the scenario's row",
            contract_body("###").replace("\n| P1.1 | to fill | `true` |", ""),
            ["the DoD has no row for `P1.1`"],
        ),
    ]
    for name, text, wanted in contract_cases:
        total += 1
        gaps = contract_gaps(text, require_invest=True, require_story=True)
        joined = "; ".join(gaps)
        if all(item in joined for item in wanted) and (wanted or not gaps):
            print(f"ok: contract — {name}")
        else:
            failures += 1
            print(f"FAIL: contract — {name}: gaps {gaps!r}")

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
    parser.add_argument(
        "--contract",
        type=int,
        metavar="N",
        help="check issue N against the full issue content contract",
    )
    parser.add_argument(
        "--contract-body",
        metavar="PATH",
        help="the same contract check over a body file (offline; tests and previews)",
    )
    parser.add_argument(
        "--apply",
        action="store_true",
        help="with --contract: set status:not-ready and one marked comment, or clear them",
    )
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if args.contract_body is not None:
        try:
            with open(args.contract_body, encoding="utf-8") as handle:
                # Offline there is no creation date to apply the forward-only exemptions to, so
                # the preview checks the full current contract: the shape a new unit must carry.
                gaps = contract_gaps(
                    handle.read(), require_invest=True, require_story=True
                )
        except OSError as error:
            print(f"board: {error}", file=sys.stderr)
            return 2
        if gaps:
            print(f"not ready — missing {', '.join(gaps)}")
            return 1
        print("the issue content contract is complete")
        return 0
    if not args.repo:
        parser.error("--repo is required unless --self-test or --contract-body")
    if args.contract is not None:
        return check_contract(args.repo, args.contract, args.apply)
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
