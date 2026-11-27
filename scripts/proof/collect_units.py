#!/usr/bin/env python3
"""Collect the delivery units' observed facts and certify the ledger (conveyor#836).

The unit model lives in `docs/spec/lean/DeliveryModel.lean`; this script is the **trusted
collector** that reads GitHub and git, writes a hashed snapshot, renders the facts into chunks,
and (with a tenant bearer) has the engine render, check and record one certificate per chunk.

Three modes, and the split is deliberate:

* `fetch`   — network only. Writes `snapshot.json`: the raw issue/PR/CI/head observations and
              their digests. A fresh fetch is a **new** snapshot; the world moves.
* `render`  — offline and deterministic. Reads a snapshot and writes `units.json` plus the
              `facts/chunk-*.json` documents. The same snapshot always renders the same facts
              hash, so a reviewer can re-derive every certificate from the committed artefact.
* `certify` — calls the deployed engine: `proofs.render` (the exact source), `lean4.check`, then
              `proofs.record`. A failing chunk is recorded `unproven`; that is the deliverable.

`report` writes the Markdown section for the evidence document from a run directory.

The facts carry `model_sha256` and `spec_sha256` computed from the checkout; a stale checkout is
refused by the engine's `proof_model_mismatch`, never silently rendered. The collector never
fetches the model digest from the engine because a fact must name the model it was built for
before a certificate can exist.

No fact is defaulted: a field the snapshot cannot establish is `null`, and the model's `none`
fails closed. Every unit's provenance names the collector digest and the digest of the raw
observation slice it was read from.

Run: scripts/proof/collect-units.sh --help
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
import subprocess
import sys
import urllib.error
import urllib.request
import uuid
from datetime import datetime, timezone

ROOT = pathlib.Path(__file__).resolve().parents[2]
DEFAULT_REPO = "nonagenticai/conveyor"
DEFAULT_BASELINE = "2026-11-11"
SPEC_PATH = "docs/plans/03-DELIVERY.md"
MODEL_PATH = "docs/spec/lean/DeliveryModel.lean"
CHUNK_SIZE = 200
COLLECTOR_VERSION = "collect-units/v1"

AUTOMATION_AUTHORS = {"github-actions[bot]", "dependabot[bot]", "renovate[bot]"}
AUTOMATION_BRANCH_PREFIX = "automation/"
CLOSES_RE = re.compile(r"\b(?:closes|fixes|resolves)\s+#(\d+)\b", re.IGNORECASE)

RULES = {
    "issue": "issue.state/v1",
    "branches": "branch.claim_names/v1",
    "branchDeleted": "branch.head_liveness/v1",
    "pulls": "pull.closing_references/v1",
    "pull": "pull.state/v1",
    "ciRuns": "ci.pull_request_runs/v1",
    "closesKeyword": "pull.closing_keyword/v1",
    "verifyParts": "issue.contract_verify/v2",
}

# Public-repo rule: raw GitHub bodies can quote estate identifiers, and this tree must not carry
# them even as a quotation. The patterns are assembled from pieces so the guard's own scan of this
# file does not match them; redaction is deterministic and covers the strings the facts need
# nothing of (hostnames, tailnet addresses, the private repository path).
REDACTIONS = [
    (re.compile(r"\." + "internal"), ".redacted"),
    (
        re.compile(
            "100" + r"\.(?:6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7])\.[0-9]{1,3}\.[0-9]{1,3}"
        ),
        "[tailnet]",
    ),
    (re.compile(r"\b" + "ts" + r"\." + "net" + r"\b"), "[tailnet]"),
    (re.compile("strato" + "-(?:server|vps)"), "[host]"),
    (re.compile("nonagenticai/" + "gitops-production"), "[private-repo]"),
]


def redact_text(text: str) -> str:
    for pattern, replacement in REDACTIONS:
        text = pattern.sub(replacement, text)
    return text


def redact_value(value):
    if isinstance(value, str):
        return redact_text(value)
    if isinstance(value, list):
        return [redact_value(item) for item in value]
    if isinstance(value, dict):
        return {key: redact_value(item) for key, item in value.items()}
    return value


def canonical(value) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True)


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def sha256_file(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def collector_sha256() -> str:
    return sha256_file(pathlib.Path(__file__).resolve())


# ── gh / git ────────────────────────────────────────────────────────────────────────────


def run(command: list[str], stdin: str | None = None) -> str:
    result = subprocess.run(
        command, input=stdin, text=True, capture_output=True, check=False
    )
    if result.returncode != 0:
        raise SystemExit(f"{' '.join(command)}: {result.stderr.strip()}")
    return result.stdout


def gh_pages(path: str, params: dict[str, str]) -> list:
    """One paginated `gh api` read, slurped into a flat list."""
    command = ["gh", "api", "-X", "GET", path]
    for key, value in params.items():
        command += ["-f", f"{key}={value}"]
    command += ["--paginate", "--slurp"]
    pages = json.loads(run(command))
    flat: list = []
    for page in pages:
        if isinstance(page, list):
            flat.extend(page)
        elif isinstance(page, dict) and isinstance(page.get("items"), list):
            flat.extend(page["items"])
        else:
            flat.append(page)
    return flat


def gh_object(path: str, params: dict[str, str]) -> dict:
    command = ["gh", "api", "-X", "GET", path]
    for key, value in params.items():
        command += ["-f", f"{key}={value}"]
    return json.loads(run(command))


def label_names(issue: dict) -> list[str]:
    return sorted(
        label.get("name", "")
        for label in issue.get("labels", [])
        if isinstance(label, dict)
    )


def trim_issue(issue: dict) -> dict:
    return {
        "number": issue["number"],
        "title": issue.get("title", ""),
        "state": issue.get("state", "open"),
        "created_at": issue.get("created_at"),
        "closed_at": issue.get("closed_at"),
        "labels": label_names(issue),
        "body": issue.get("body") or "",
        "user": (issue.get("user") or {}).get("login"),
    }


def trim_pull(pull: dict) -> dict:
    return {
        "number": pull["number"],
        "title": pull.get("title", ""),
        "state": pull.get("state"),
        "draft": bool(pull.get("draft")),
        "merged_at": pull.get("merged_at"),
        "merge_commit_sha": pull.get("merge_commit_sha"),
        "head": {
            "ref": (pull.get("head") or {}).get("ref"),
            "sha": (pull.get("head") or {}).get("sha"),
        },
        "base": {"ref": (pull.get("base") or {}).get("ref")},
        "user": (pull.get("user") or {}).get("login"),
        "body": pull.get("body") or "",
    }


def live_heads(repo_dir: pathlib.Path) -> list[str]:
    output = run(["git", "-C", str(repo_dir), "ls-remote", "--heads", "origin"])
    heads = []
    for line in output.splitlines():
        _, _, ref = line.partition("\t")
        if ref.startswith("refs/heads/"):
            heads.append(ref[len("refs/heads/") :])
    return sorted(heads)


def redact_existing(path: pathlib.Path) -> None:
    """Apply the current redaction rules to a snapshot fetched before they existed.

    The raw payload is rewritten in place and the artifact digests recomputed; the facts the
    certificate uses (state, closing keywords, `Verify:` lines) do not depend on the redacted
    strings. A snapshot redacted twice is byte-identical to one redacted once.
    """
    snapshot = json.loads(path.read_text())
    snapshot["issues"] = redact_value(snapshot["issues"])
    snapshot["pulls"] = redact_value(snapshot["pulls"])
    snapshot["ci_runs"] = redact_value(snapshot["ci_runs"])
    snapshot["live_heads"] = redact_value(snapshot["live_heads"])
    snapshot["redacted"] = "private-refs/v1 (public GitHub quotations of estate identifiers)"
    snapshot["artifacts"] = {
        "issues": sha256_text(canonical(snapshot["issues"])),
        "pulls": sha256_text(canonical(snapshot["pulls"])),
        "heads": sha256_text(canonical(snapshot["live_heads"])),
    }
    path.write_text(json.dumps(snapshot, indent=2, sort_keys=True) + "\n")
    print(f"redacted: {path}")


def fetch(repo: str, baseline: str, repo_dir: pathlib.Path, out: pathlib.Path) -> None:
    spec = (repo_dir / SPEC_PATH).read_text()
    model = (repo_dir / MODEL_PATH).read_text()
    commit = run(["git", "-C", str(repo_dir), "rev-parse", "HEAD"]).strip()

    issues_raw = gh_pages(
        f"repos/{repo}/issues",
        {"state": "all", "since": f"{baseline}T00:00:00Z", "per_page": "100"},
    )
    issues = redact_value(
        [
            trim_issue(issue)
            for issue in issues_raw
            if "pull_request" not in issue
        ]
    )
    pulls_raw = gh_pages(
        f"repos/{repo}/pulls",
        {"state": "closed", "per_page": "100", "sort": "updated", "direction": "desc"},
    )
    pulls = redact_value(
        [
            trim_pull(pull)
            for pull in pulls_raw
            if pull.get("merged_at") and pull["merged_at"][:10] >= baseline
        ]
    )

    kept = [pull for pull in pulls if not is_automation(pull)]
    shas = sorted(
        {
            sha
            for pull in kept
            for sha in (pull["head"]["sha"], pull["merge_commit_sha"])
            if sha
        }
    )
    runs: dict[str, list] = {}
    for sha in shas:
        body = gh_object(
            f"repos/{repo}/actions/runs", {"head_sha": sha, "per_page": "100"}
        )
        runs[sha] = [
            {
                "name": run_row.get("name"),
                "event": run_row.get("event"),
                "conclusion": run_row.get("conclusion"),
                "head_sha": run_row.get("head_sha"),
                "created_at": run_row.get("created_at"),
            }
            for run_row in body.get("workflow_runs", [])
        ]

    heads = redact_value(live_heads(repo_dir))
    snapshot = {
        "schema": COLLECTOR_VERSION,
        "repo": repo,
        "baseline": baseline,
        "collected_at": datetime.now(timezone.utc).isoformat(),
        "redacted": "private-refs/v1 (public GitHub quotations of estate identifiers)",
        "commit": commit,
        "spec_sha256": sha256_text(spec),
        "model_sha256": sha256_text(model),
        "collector_sha256": collector_sha256(),
        "live_heads": heads,
        "issues": issues,
        "pulls": pulls,
        "ci_runs": runs,
        "artifacts": {
            "issues": sha256_text(canonical(issues)),
            "pulls": sha256_text(canonical(pulls)),
            "heads": sha256_text(canonical(heads)),
        },
    }
    out.mkdir(parents=True, exist_ok=True)
    (out / "snapshot.json").write_text(json.dumps(snapshot, indent=2, sort_keys=True) + "\n")
    print(
        f"snapshot: {len(issues)} issue(s), {len(pulls)} merged pull(s) "
        f"({len(kept)} non-automation), {len(heads)} live head(s), "
        f"{sum(len(v) for v in runs.values())} run(s) at {len(runs)} sha(s)"
    )


# ── the unit corpus (pure; the self-test drives this offline) ──────────────────────────


def is_automation(pull: dict) -> bool:
    login = pull.get("user") or ""
    ref = (pull.get("head") or {}).get("ref") or ""
    return login in AUTOMATION_AUTHORS or ref.startswith(AUTOMATION_BRANCH_PREFIX)


def closing_issues(pull: dict) -> list[int]:
    return sorted({int(match) for match in CLOSES_RE.findall(pull.get("body") or "")})


def contract_verify(body: str) -> bool:
    """Whether every `###` part in the body's Parts section carries a `Verify:` line.

    The line rule is `scripts/ci/board.py`'s `VERIFY_RE`, the instrument `issue-dor` runs. The
    section locator is deliberately looser than that instrument's exact heading: issues written
    before the template settled say `## Parts`, and the model's rule is about the parts, not the
    heading. The report's cross-check names the units where the two disagree, rather than
    silently adopting either answer.
    """
    sys.path.insert(0, str(ROOT / "scripts" / "ci"))
    import board  # noqa: PLC0415 — the repo's contract instrument

    match = re.search(r"(?im)^#+\s*Parts\b.*$", body)
    if not match:
        return False
    heading = match.group(0)
    level = len(heading) - len(heading.lstrip("#"))
    scoped = [heading]
    for line in body[match.start() :].splitlines()[1:]:
        trimmed = line.lstrip()
        if trimmed.startswith("#"):
            depth = len(trimmed) - len(trimmed.lstrip("#"))
            if depth <= level:
                break
        scoped.append(line)
    section = "\n".join(scoped)
    parts = re.split(r"(?m)^###\s+", section)[1:]
    if not parts:
        parts = [section]
    return all(
        any(board.VERIFY_RE.match(line) for line in part.splitlines()) for part in parts
    )


def unit_facts(
    issue: dict | None,
    pulls: list[dict],
    runs: dict[str, list],
    heads: set[str],
) -> tuple[dict, dict]:
    """One unit's eight observations and the artifact they were read from."""
    issue_number = issue["number"] if issue else None
    merged = [pull for pull in pulls if pull.get("merged_at")]
    chosen = min(merged, key=lambda pull: pull["number"]) if merged else (
        min(pulls, key=lambda pull: pull["number"]) if pulls else None
    )

    claim_branches: set[str] = set()
    if issue_number is not None:
        prefix = f"issue/{issue_number}-"
        claim_branches.update(head for head in heads if head.startswith(prefix))
    for pull in pulls:
        ref = (pull.get("head") or {}).get("ref")
        if ref:
            claim_branches.add(ref)

    branch_deleted = None
    if chosen is not None and chosen.get("merged_at"):
        ref = (chosen.get("head") or {}).get("ref")
        branch_deleted = ref not in heads if ref else None

    pull_state = None
    if merged:
        pull_state = "merged"
    elif pulls:
        pull_state = "draft" if any(pull.get("draft") for pull in pulls) else "ready"

    ci_runs = None
    if chosen is not None:
        sha = (chosen.get("head") or {}).get("sha")
        if sha and sha in runs:
            ci_runs = sum(
                1
                for run in runs[sha]
                if run.get("name") == "ci" and run.get("event") == "pull_request"
            )

    closes_keyword = None
    if issue_number is not None and pulls:
        closes_keyword = any(
            issue_number in closing_issues(pull) for pull in pulls
        )

    verify_parts = contract_verify(issue.get("body") or "") if issue else None

    facts = {
        "issue": None
        if issue is None
        else ("closed" if issue.get("state") == "closed" else "pending"),
        "branches": len(claim_branches) if (pulls or issue_number is not None) else None,
        "branchDeleted": branch_deleted,
        "pulls": len(pulls) if pulls else 0,
        "pull": pull_state,
        "ciRuns": ci_runs,
        "closesKeyword": closes_keyword,
        "verifyParts": verify_parts,
    }
    artifact = sha256_text(
        canonical(
            {
                "issue": issue,
                "pulls": sorted(pulls, key=lambda pull: pull["number"]),
                "ci": {
                    (pull.get("head") or {}).get("sha"): runs.get(
                        (pull.get("head") or {}).get("sha"), []
                    )
                    for pull in pulls
                },
                "heads": sorted(claim_branches),
            }
        )
    )
    return facts, {"artifact": artifact, "rules": dict(RULES)}


def build_units(snapshot: dict) -> dict:
    """Pair the window's issues and merged PRs into completed units, in-flight units and
    exclusions. Pure: the snapshot is the only input."""
    issues = {issue["number"]: issue for issue in snapshot["issues"]}
    heads = set(snapshot["live_heads"])
    runs = snapshot["ci_runs"]
    baseline = snapshot["baseline"]

    exclusions: list[dict] = []
    by_issue: dict[int, list[dict]] = {}
    orphans: list[dict] = []
    for pull in snapshot["pulls"]:
        if is_automation(pull):
            exclusions.append(
                {
                    "rule": "automation.author_or_branch/v1",
                    "pull": pull["number"],
                    "reason": f"{pull.get('user')} on {(pull.get('head') or {}).get('ref')}",
                }
            )
            continue
        closing = closing_issues(pull)
        if closing:
            for number in closing:
                by_issue.setdefault(number, []).append(pull)
        else:
            orphans.append(pull)

    units: list[dict] = []
    linked_issues: set[int] = set()

    # Every issue a merged window PR closes is a unit, even when the issue predates the window.
    for number in sorted(by_issue):
        issue = issues.get(number)
        if issue is None:
            continue  # handled below, where the missing issue fails closed as `null`
        linked_issues.add(number)
        pulls = by_issue[number]
        if not any(pull.get("merged_at") for pull in pulls):
            continue
        facts, provenance = unit_facts(issue, pulls, runs, heads)
        key = f"{snapshot['repo']}#{number}"
        units.append(
            {
                "key": key,
                "issue_number": number,
                "unit_label": bool(issue and "unit" in issue.get("labels", [])),
                "facts": facts,
                "provenance": provenance,
            }
        )

    # A merged non-automation PR that closes an issue outside the fetched set is still a unit
    # with an issue the collector could not read: fail closed with `issue = null`, never guess.
    outside = sorted(number for number in by_issue if number not in issues)
    for number in outside:
        pulls = by_issue[number]
        if not any(pull.get("merged_at") for pull in pulls):
            continue
        facts, provenance = unit_facts(None, pulls, runs, heads)
        key = f"{snapshot['repo']}#{number}"
        units.append(
            {
                "key": key,
                "issue_number": number,
                "unit_label": False,
                "facts": facts,
                "provenance": provenance,
            }
        )

    # An orphan is a unit with no issue: the PR is the whole record.
    for pull in orphans:
        if not pull.get("merged_at"):
            continue
        facts, provenance = unit_facts(None, [pull], runs, heads)
        key = f"{snapshot['repo']}#pr-{pull['number']}"
        units.append(
            {
                "key": key,
                "issue_number": None,
                "unit_label": False,
                "facts": facts,
                "provenance": provenance,
            }
        )

    in_flight: list[dict] = []
    for issue in snapshot["issues"]:
        number = issue["number"]
        if "unit" not in issue.get("labels", []):
            continue
        if (issue.get("created_at") or "")[:10] < baseline:
            continue
        if number in by_issue or number in linked_issues:
            continue
        in_flight.append(
            {
                "key": f"{snapshot['repo']}#{number}",
                "issue_number": number,
                "state": issue.get("state"),
                "reason": "no merged pull request in the window",
            }
        )

    units.sort(key=lambda unit: unit["key"])
    corpus = {
        "repo": snapshot["repo"],
        "baseline": snapshot["baseline"],
        "artifacts": snapshot["artifacts"],
        "units": [unit["key"] for unit in units],
    }
    corpus_sha256 = sha256_text(canonical(corpus))
    return {
        "corpus_sha256": corpus_sha256,
        "repo": snapshot["repo"],
        "baseline": snapshot["baseline"],
        "artifacts": snapshot["artifacts"],
        "collected_at": snapshot["collected_at"],
        "commit": snapshot.get("commit"),
        "spec_sha256": snapshot["spec_sha256"],
        "model_sha256": snapshot["model_sha256"],
        "collector_sha256": snapshot.get("collector_sha256"),
        "counts": {
            "completed": len(units),
            "in_flight": len(in_flight),
            "orphans": len(orphans),
            "excluded_automation": len(exclusions),
        },
        "exclusions": exclusions,
        "in_flight": in_flight,
        "units": units,
    }


# ── facts documents ────────────────────────────────────────────────────────────────────


def chunks(units: list[dict], size: int) -> list[list[dict]]:
    return [units[start : start + size] for start in range(0, len(units), size)]


def facts_document(snapshot_meta: dict, units: list[dict]) -> dict:
    """The document `proofs.render` and `proofs.record` see, provenance included.

    The renderer reads only `kind`, `model_sha256`, `spec_sha256` and `units[*]`'s eight fields;
    `provenance` is carried verbatim because the certificate stores the facts, and a reviewer
    needs the rule and artefact behind every field. It is deterministic: no clock, no paths.
    """
    return {
        "kind": "unit_ledger",
        "model_sha256": snapshot_meta["model_sha256"],
        "spec_sha256": snapshot_meta["spec_sha256"],
        "provenance": {
            "collector_sha256": snapshot_meta["collector_sha256"],
            "units": {
                unit["key"]: unit["provenance"] for unit in units
            },
        },
        "units": [
            {"key": unit["key"], **unit["facts"]} for unit in units
        ],
    }


def board_verify(body: str) -> bool:
    """The stricter CI instrument's answer: the exact heading plus one `Verify:` in it."""
    sys.path.insert(0, str(ROOT / "scripts" / "ci"))
    import board  # noqa: PLC0415

    gaps = board.contract_gaps(body)
    return "a `Verify:` line in `Parts and acceptance criteria`" not in gaps


def local_rules(facts: dict) -> dict[str, bool]:
    """The model's rules, mirrored for the report only. The kernel proves the truth."""
    issue = facts["issue"]
    pull = facts["pull"]
    merged = pull == "merged"
    merge_complete = True if not merged else issue == "closed" and facts["branchDeleted"] is True
    return {
        "oneBranch": facts["branches"] == 1,
        "onePull": facts["pulls"] == 1,
        "pullUsable": pull in ("ready", "merged"),
        "ciBudget": facts["ciRuns"] is not None and facts["ciRuns"] <= 2,
        "mergeComplete": merge_complete,
        "closesKeyword": facts["closesKeyword"] is True,
        "verifyParts": facts["verifyParts"] is True,
    }


def render(snapshot_path: pathlib.Path, out: pathlib.Path) -> dict:
    snapshot = json.loads(snapshot_path.read_text())
    # The provenance names the instrument that produced the facts, not the one that fetched the
    # snapshot: the rule implementations are what a reviewer re-runs here.
    snapshot["collector_sha256"] = collector_sha256()
    corpus = build_units(snapshot)
    documents = []
    for index, chunk in enumerate(chunks(corpus["units"], CHUNK_SIZE)):
        the_chunk = facts_document(corpus, chunk)
        size = len(canonical(the_chunk))
        per_unit = [local_rules(unit["facts"]) for unit in chunk]
        documents.append(
            {
                "index": index,
                "name": f"chunk-{index:03d}",
                "units": len(chunk),
                "bytes": size,
                "facts": the_chunk,
                "rules": {
                    name: all(result[name] for result in per_unit)
                    for name in per_unit[0]
                },
            }
        )

    totals = {
        name: sum(
            1
            for unit in corpus["units"]
            if not local_rules(unit["facts"])[name]
        )
        for name in (
            "oneBranch",
            "onePull",
            "pullUsable",
            "ciBudget",
            "mergeComplete",
            "closesKeyword",
            "verifyParts",
        )
    }
    issues = {issue["number"]: issue for issue in snapshot["issues"]}
    cross = {"board_pass_model_fail": 0, "model_pass_board_fail": 0, "agree": 0}
    for unit in corpus["units"]:
        if unit["issue_number"] is None:
            continue
        body = issues.get(unit["issue_number"], {}).get("body", "")
        by_board = board_verify(body)
        by_model = unit["facts"]["verifyParts"] is True
        if by_board and not by_model:
            cross["board_pass_model_fail"] += 1
        elif by_model and not by_board:
            cross["model_pass_board_fail"] += 1
        else:
            cross["agree"] += 1
    corpus["verify_cross_check"] = cross
    corpus["chunks"] = [
        {key: doc[key] for key in ("index", "name", "units", "bytes", "rules")}
        for doc in documents
    ]
    corpus["rule_failures"] = totals
    corpus["units"] = [
        {
            **unit,
            "rules": local_rules(unit["facts"]),
            "failed_rules": sorted(
                name for name, passed in local_rules(unit["facts"]).items() if not passed
            ),
        }
        for unit in corpus["units"]
    ]

    out.mkdir(parents=True, exist_ok=True)
    (out / "units.json").write_text(
        json.dumps(corpus, indent=2, sort_keys=True) + "\n"
    )
    facts_dir = out / "facts"
    facts_dir.mkdir(exist_ok=True)
    for document in documents:
        (facts_dir / f"{document['name']}.json").write_text(
            json.dumps(document["facts"], indent=2, sort_keys=True) + "\n"
        )
    print(
        f"corpus {corpus['corpus_sha256'][:16]}…: {corpus['counts']['completed']} completed, "
        f"{corpus['counts']['in_flight']} in flight, "
        f"{corpus['counts']['orphans']} orphan, "
        f"{corpus['counts']['excluded_automation']} automation excluded; "
        f"{len(documents)} chunk(s)"
    )
    for document in documents:
        print(f"  {document['name']}: {document['units']} units, {document['bytes']} bytes")
    return corpus


# ── the engine path ───────────────────────────────────────────────────────────────────


class Engine:
    def __init__(self, url: str, token: str) -> None:
        self.url = url.rstrip("/")
        self.token = token

    def call(self, action: str, input_value: dict, key: str) -> dict:
        request = urllib.request.Request(
            f"{self.url}/api/actions/{action}/invocations",
            data=json.dumps({"input": input_value}).encode("utf-8"),
            headers={
                "Authorization": f"Bearer {self.token}",
                "Content-Type": "application/json",
                "Idempotency-Key": key,
            },
            method="POST",
        )
        try:
            with urllib.request.urlopen(request, timeout=180) as response:
                body = json.loads(response.read().decode("utf-8"))
                return body
        except urllib.error.HTTPError as error:
            detail = error.read().decode("utf-8", "replace")
            raise SystemExit(f"{action} answered {error.code}: {detail}") from None


def token_from_file(path: pathlib.Path) -> str:
    raw = path.read_text().strip()
    if raw.startswith("{"):
        return json.loads(raw)["token"]
    return raw


def certify(run_dir: pathlib.Path, url: str, token_file: pathlib.Path) -> dict:
    corpus = json.loads((run_dir / "units.json").read_text())
    engine = Engine(url, token_from_file(token_file))
    documents = sorted((run_dir / "facts").glob("chunk-*.json"))
    if not documents:
        raise SystemExit(f"no facts chunks under {run_dir / 'facts'}")

    results = []
    for path in documents:
        facts = json.loads(path.read_text())
        facts_text = canonical(facts)
        name = path.stem
        chunk = next(
            row for row in corpus["chunks"] if row["name"] == name
        )
        base = f"unit-ledger:{corpus['corpus_sha256']}:{chunk['index']}"

        rendered = engine.call(
            "proofs.render",
            {"facts_canonical": facts_text, "theorem": "unit_ledger_ok"},
            f"{base}:render",
        )["output"]
        check_input = {"source": rendered["lean_source"], "timeout": 120}
        checked = engine.call("lean4.check", check_input, f"{base}:check")["output"]
        if checked.get("valid") is None:
            raise SystemExit(f"{name}: the prover answered no verdict: {checked}")
        recorded = engine.call(
            "proofs.record",
            {
                "action": "lean4.check",
                "idempotency_key": f"{base}:check",
                "invocation_input": check_input,
                "subject_type": "unit_ledger",
                "subject_id": str(
                    uuid.uuid5(
                        uuid.NAMESPACE_OID,
                        f"conveyor-ledger:{corpus['corpus_sha256']}:{chunk['index']}",
                    )
                ),
                "spec_name": SPEC_PATH,
                "spec_sha256": corpus["spec_sha256"],
                "facts_canonical": facts_text,
                "theorem": "unit_ledger_ok",
                "trust": "kernel",
            },
            f"{base}:record",
        )["output"]
        results.append(
            {
                "name": name,
                "units": chunk["units"],
                "facts_sha256": rendered["facts_sha256"],
                "model_sha256": rendered["model_sha256"],
                "valid": checked.get("valid"),
                "computation_time_ms": checked.get("computation_time_ms"),
                "certificate_id": recorded["id"],
                "certificate_sha256": recorded["certificate_sha256"],
                "verdict": recorded["verdict"],
            }
        )
        print(f"  {name}: {recorded['verdict']} {recorded['id']}")

    leaves = [[result["name"], result["certificate_sha256"]] for result in results]
    programme_facts = {
        "kind": "programme",
        "model_sha256": corpus["model_sha256"],
        "spec_sha256": corpus["spec_sha256"],
        "expected": [result["name"] for result in results],
        "leaves": leaves,
    }
    programme_text = canonical(programme_facts)
    rendered = engine.call(
        "proofs.render",
        {"facts_canonical": programme_text, "theorem": "unit_programme_ok"},
        f"unit-ledger:{corpus['corpus_sha256']}:programme:render",
    )["output"]
    check_input = {"source": rendered["lean_source"], "timeout": 120}
    checked = engine.call(
        "lean4.check",
        check_input,
        f"unit-ledger:{corpus['corpus_sha256']}:programme:check",
    )["output"]
    recorded = engine.call(
        "proofs.record",
        {
            "action": "lean4.check",
            "idempotency_key": f"unit-ledger:{corpus['corpus_sha256']}:programme:check",
            "invocation_input": check_input,
            "subject_type": "programme",
            "subject_id": str(
                uuid.uuid5(
                    uuid.NAMESPACE_OID,
                    f"conveyor-ledger-programme:{corpus['corpus_sha256']}",
                )
            ),
            "spec_name": SPEC_PATH,
            "spec_sha256": corpus["spec_sha256"],
            "facts_canonical": programme_text,
            "theorem": "unit_programme_ok",
            "trust": "kernel",
        },
        f"unit-ledger:{corpus['corpus_sha256']}:programme:record",
    )["output"]

    certificates = {
        "corpus_sha256": corpus["corpus_sha256"],
        "chunks": results,
        "programme": {
            "certificate_id": recorded["id"],
            "certificate_sha256": recorded["certificate_sha256"],
            "verdict": recorded["verdict"],
            "valid": checked.get("valid"),
            "leaves": leaves,
        },
    }
    (run_dir / "certificates.json").write_text(
        json.dumps(certificates, indent=2, sort_keys=True) + "\n"
    )
    print(f"programme: {recorded['verdict']} {recorded['id']}")
    return certificates


# ── the report ────────────────────────────────────────────────────────────────────────


def report(run_dir: pathlib.Path) -> pathlib.Path:
    corpus = json.loads((run_dir / "units.json").read_text())
    certificates_path = run_dir / "certificates.json"
    certificates = (
        json.loads(certificates_path.read_text()) if certificates_path.exists() else None
    )
    lines: list[str] = []
    lines.append(f"### Evaluation — corpus `{corpus['corpus_sha256'][:16]}…`")
    lines.append("")
    lines.append(
        f"Window: {corpus['repo']} merged PRs and unit issues from {corpus['baseline']}; "
        f"collected {corpus['collected_at']} at `{corpus['commit']}`. "
        f"Spec `{SPEC_PATH}` `{corpus['spec_sha256'][:16]}…`; "
        f"model `{corpus['model_sha256'][:16]}…`."
    )
    lines.append("")
    counts = corpus["counts"]
    lines.append(
        f"Completed units **{counts['completed']}**; in flight {counts['in_flight']} "
        f"(not certified — the delivery has not happened); orphan PRs {counts['orphans']}; "
        f"automation excluded {counts['excluded_automation']}."
    )
    lines.append("")
    lines.append("| Rule | Failing units |")
    lines.append("|---|---|")
    for name, count in corpus["rule_failures"].items():
        lines.append(f"| `{name}` | {count} |")
    lines.append("")
    if certificates:
        lines.append("| Chunk | Units | Verdict | Certificate |")
        lines.append("|---|---|---|---|")
        for chunk in certificates["chunks"]:
            lines.append(
                f"| `{chunk['name']}` | {chunk['units']} | {chunk['verdict']} | "
                f"`{chunk['certificate_id']}` |"
            )
        lines.append(
            f"\nProgramme bundle `{certificates['programme']['certificate_id']}` "
            f"({certificates['programme']['verdict']})."
        )
        lines.append("")
    failing = [unit for unit in corpus["units"] if unit["failed_rules"]]
    lines.append(f"**{len(failing)} completed unit(s) fail the model.**")
    lines.append("")
    lines.append("| Unit | Failed rules |")
    lines.append("|---|---|")
    for unit in failing[:400]:
        lines.append(f"| `{unit['key']}` | {', '.join(unit['failed_rules'])} |")
    if len(failing) > 400:
        lines.append(f"| … | {len(failing) - 400} more in `units.json` |")
    lines.append("")
    cross = corpus.get("verify_cross_check") or {}
    lines.append(
        "Rules the snapshot cannot reconstruct are named, not dropped: claim timing/comments "
        "(the four pre-branch checks are not a property of the finished unit) and "
        "`Blocker:` iff `status:blocked` (label state is not historical)."
    )
    lines.append("")
    lines.append(
        "**`Verify:` cross-check.** The fact reuses `scripts/ci/board.py`'s `VERIFY_RE` (the line "
        "rule `issue-dor` runs) and requires one per `###` part, which is what 03-DELIVERY's DoR "
        "says; the CI checker requires only one in the exact `Parts and acceptance criteria` "
        "section. Disagreements: "
        f"{cross.get('board_pass_model_fail', 0)} pass the checker but fail the model, "
        f"{cross.get('model_pass_board_fail', 0)} the reverse, "
        f"{cross.get('agree', 0)} agree. Both answers are recorded; neither is silently adopted."
    )
    lines.append("")
    lines.append(
        "The kernel certifies *facts ⇒ verdict*; the collector's snapshot, rule ids and "
        "artefact digests are what a reviewer checks against GitHub. The snapshot is redacted "
        "per `scripts/ci/no-private-refs.sh` (public GitHub quotations of estate identifiers "
        "become placeholders; the facts need none of them). No unit is attributed to a person."
    )
    path = run_dir / "report.md"
    path.write_text("\n".join(lines) + "\n")
    print(f"report: {path}")
    return path


# ── the self-test (no network) ────────────────────────────────────────────────────────


def self_test() -> int:
    snapshot = {
        "schema": COLLECTOR_VERSION,
        "repo": "x/y",
        "baseline": "2026-11-11",
        "collected_at": "2026-11-27T00:00:00+00:00",
        "commit": "deadbeef",
        "spec_sha256": "a" * 64,
        "model_sha256": "b" * 64,
        "collector_sha256": "c" * 64,
        "live_heads": ["issue/1-a"],
        "issues": [
            {
                "number": 1,
                "state": "closed",
                "created_at": "2026-11-12T00:00:00Z",
                "labels": ["unit"],
                "body": "## Parts and acceptance criteria\n\n### P1\nVerify: `true`\n",
                "title": "unit one",
            },
            {
                "number": 2,
                "state": "open",
                "created_at": "2026-11-13T00:00:00Z",
                "labels": ["unit"],
                "body": "## Parts and acceptance criteria\n\n### P1\nVerify: `true`\n",
                "title": "unit two",
            },
            {
                "number": 3,
                "state": "closed",
                "created_at": "2026-11-14T00:00:00Z",
                "labels": ["unit"],
                "body": "## Parts and acceptance criteria\n\n### P1\nno acceptance\n",
                "title": "unit three",
            },
        ],
        "pulls": [
            {
                "number": 10,
                "merged_at": "2026-11-15T00:00:00Z",
                "merge_commit_sha": "m10",
                "draft": False,
                "head": {"ref": "issue/1-a", "sha": "s10"},
                "user": "agent[bot]",
                "body": "Closes #1",
            },
            {
                "number": 11,
                "merged_at": "2026-11-15T00:00:00Z",
                "merge_commit_sha": "m11",
                "draft": False,
                "head": {"ref": "automation/x", "sha": "s11"},
                "user": "github-actions[bot]",
                "body": "",
            },
            {
                "number": 12,
                "merged_at": "2026-11-16T00:00:00Z",
                "merge_commit_sha": "m12",
                "draft": False,
                "head": {"ref": "issue/9-x", "sha": "s12"},
                "user": "agent[bot]",
                "body": "no closing reference",
            },
            {
                "number": 13,
                "merged_at": "2026-11-17T00:00:00Z",
                "merge_commit_sha": "m13",
                "draft": False,
                "head": {"ref": "issue/3-x", "sha": "s13"},
                "user": "agent[bot]",
                "body": "Closes #3",
            },
        ],
        "ci_runs": {
            "s10": [
                {"name": "ci", "event": "pull_request", "conclusion": "success"},
                {"name": "ci", "event": "merge_group", "conclusion": "success"},
            ],
            "s13": [
                {"name": "ci", "event": "pull_request", "conclusion": "success"},
                {"name": "ci", "event": "pull_request", "conclusion": "success"},
                {"name": "ci", "event": "pull_request", "conclusion": "success"},
            ],
        },
        "artifacts": {"issues": "i" * 64, "pulls": "p" * 64, "heads": "h" * 64},
    }
    corpus = build_units(snapshot)
    units = {unit["key"]: unit for unit in corpus["units"]}

    assert corpus["counts"]["completed"] == 3, corpus["counts"]
    assert corpus["counts"]["in_flight"] == 1, corpus["counts"]
    assert corpus["counts"]["excluded_automation"] == 1, corpus["counts"]
    assert corpus["counts"]["orphans"] == 1, corpus["counts"]

    one = units["x/y#1"]["facts"]
    assert one["issue"] == "closed" and one["branches"] == 1
    assert one["branchDeleted"] is False, "the live head is the PR head"
    assert one["pulls"] == 1 and one["pull"] == "merged"
    assert one["ciRuns"] == 1, "only the pull_request run counts"
    assert one["closesKeyword"] is True and one["verifyParts"] is True

    three = units["x/y#3"]["facts"]
    assert three["verifyParts"] is False, "the body carries no Verify line"
    assert three["ciRuns"] == 3

    orphan = units["x/y#pr-12"]["facts"]
    assert orphan["issue"] is None and orphan["pull"] == "merged"
    assert orphan["closesKeyword"] is None and orphan["verifyParts"] is None

    assert [unit["key"] for unit in corpus["units"]] == sorted(units)
    assert len(chunks(corpus["units"], 2)) == 2, "chunking is by size"
    print("self-test: ok")
    return 0


# ── CLI ───────────────────────────────────────────────────────────────────────────────


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="mode")

    fetch_parser = sub.add_parser("fetch", help="read GitHub/git and write snapshot.json")
    fetch_parser.add_argument("--repo", default=DEFAULT_REPO)
    fetch_parser.add_argument("--baseline", default=DEFAULT_BASELINE)
    fetch_parser.add_argument("--repo-dir", type=pathlib.Path, default=ROOT)
    fetch_parser.add_argument("--out", type=pathlib.Path, required=True)

    render_parser = sub.add_parser("render", help="render the facts offline from a snapshot")
    render_parser.add_argument("--snapshot", type=pathlib.Path, required=True)
    render_parser.add_argument("--out", type=pathlib.Path, required=True)

    certify_parser = sub.add_parser("certify", help="render, check and record through the engine")
    certify_parser.add_argument("--run", type=pathlib.Path, required=True)
    certify_parser.add_argument("--url", default="https://conveyor.nonagentic.ai")
    certify_parser.add_argument(
        "--token-file", type=pathlib.Path, default=pathlib.Path.home() / ".conveyor/nonagentic.token"
    )

    report_parser = sub.add_parser("report", help="write report.md from a run directory")
    report_parser.add_argument("--run", type=pathlib.Path, required=True)

    redact_parser = sub.add_parser(
        "redact", help="re-apply the public-repo redaction rules to a snapshot"
    )
    redact_parser.add_argument("--snapshot", type=pathlib.Path, required=True)

    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if not args.mode:
        parser.error("a mode is required: fetch, render, certify or report")
    if args.mode == "fetch":
        fetch(args.repo, args.baseline, args.repo_dir, args.out)
        return 0
    if args.mode == "render":
        snapshot_path = (
            args.snapshot / "snapshot.json"
            if args.snapshot.is_dir()
            else args.snapshot
        )
        render(snapshot_path, args.out)
        return 0
    if args.mode == "certify":
        certify(args.run, args.url, args.token_file)
        return 0
    if args.mode == "report":
        report(args.run)
        return 0
    if args.mode == "redact":
        snapshot_path = (
            args.snapshot / "snapshot.json"
            if args.snapshot.is_dir()
            else args.snapshot
        )
        redact_existing(snapshot_path)
        return 0
    parser.error(f"unknown mode {args.mode}")
    return 2


if __name__ == "__main__":
    sys.exit(main())
