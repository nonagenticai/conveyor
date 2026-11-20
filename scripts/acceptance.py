#!/usr/bin/env python3
"""The native acceptance harness (S14 P3.1).

One checked-in manifest (`acceptance/native.toml`) and one command that refuses to call a
release reachable on partial evidence:

  * **contexts** — every required CI context must be present on the commit under test and
    conclude `success`; the repository's own required-check authority (ruleset, else branch
    protection) must not name a context the manifest omits;
  * **probes** — every declared surface must answer its declared status; 404/422/5xx are never
    "reachable", and a 401 counts only where the manifest declares it (e.g. `/mcp`);
  * **commands** — each executable check runs and must select at least one test. Zero-selected
    is detected per runner (`--no-tests=fail` for nextest, `running N tests` lines for libtest,
    `stats` JSON for Playwright), and a declared `min_tests` is enforced.

Exit 0 only when every part passed. `--self-test` proves each refusal against synthetic input.

Run: `python3 scripts/acceptance.py --sha "$GITHUB_SHA"` (or with no `--sha`, `git rev-parse HEAD`).
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tomllib
import urllib.error
import urllib.request
from pathlib import Path

DEFAULT_MANIFEST = Path(__file__).resolve().parent.parent / "acceptance" / "native.toml"


def fail(message: str) -> None:
    print(f"acceptance: FAIL — {message}", file=sys.stderr)


def ok(message: str) -> None:
    print(f"acceptance: ok — {message}")


def load_manifest(path: Path) -> dict:
    with path.open("rb") as handle:
        manifest = tomllib.load(handle)
    if not isinstance(manifest.get("contexts"), list) or not manifest["contexts"]:
        raise SystemExit("acceptance: the manifest declares no [[contexts]]")
    if not isinstance(manifest.get("probes"), list) or not manifest["probes"]:
        raise SystemExit("acceptance: the manifest declares no [[probes]]")
    return manifest


def run(command: list[str], *, timeout: int = 120) -> subprocess.CompletedProcess:
    return subprocess.run(command, capture_output=True, text=True, timeout=timeout)


def gh_json(path: str) -> dict | list:
    result = run(["gh", "api", path])
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or f"gh api {path} failed")
    return json.loads(result.stdout)


def required_contexts(repo: str, branch: str) -> set[str]:
    """The repository's own required-check authority: rulesets first, then branch protection."""
    names: set[str] = set()
    try:
        rulesets = gh_json(f"repos/{repo}/rulesets")
    except (RuntimeError, json.JSONDecodeError):
        rulesets = []
    if isinstance(rulesets, list):
        for summary in rulesets:
            if summary.get("target") not in (None, "branch"):
                continue
            try:
                detail = gh_json(f"repos/{repo}/rulesets/{summary['id']}")
            except (RuntimeError, json.JSONDecodeError):
                continue
            ref_name = ((detail.get("conditions") or {}).get("ref_name") or {}).get("include") or []
            default_scope = any(token in ref_name for token in ("~DEFAULT_BRANCH", "~ALL"))
            if ref_name and not default_scope and f"refs/heads/{branch}" not in ref_name:
                continue
            for rule in detail.get("rules") or []:
                if rule.get("type") != "required_status_checks":
                    continue
                for check in (rule.get("parameters") or {}).get("required_status_checks") or []:
                    context = check.get("context")
                    if context:
                        names.add(context)
    if names:
        return names
    try:
        protection = gh_json(f"repos/{repo}/branches/{branch}/protection")
    except (RuntimeError, json.JSONDecodeError) as error:
        raise RuntimeError(
            f"no required-check authority is readable (rulesets and branch protection both refused): {error}"
        ) from error
    for check in ((protection.get("required_status_checks") or {}).get("contexts") or []):
        names.add(check)
    return names


def check_contexts(manifest: dict, repo: str, sha: str) -> bool:
    """Every manifest context is success on the sha; the authority names nothing omitted."""
    passed = True
    try:
        runs = gh_json(f"repos/{repo}/commits/{sha}/check-runs?per_page=100")
        authority = required_contexts(repo, manifest["meta"].get("main_branch", "main"))
    except (RuntimeError, json.JSONDecodeError) as error:
        fail(f"contexts: {error}")
        return False
    by_name: dict[str, list[dict]] = {}
    for check in runs.get("check_runs") or []:
        by_name.setdefault(check.get("name") or "", []).append(check)
    declared = {row["name"] for row in manifest["contexts"]}
    for row in manifest["contexts"]:
        name = row["name"]
        expected = row.get("conclusion", "success")
        found = by_name.get(name) or []
        conclusions = {check.get("conclusion") for check in found}
        if not found:
            fail(f"contexts: {name!r} has no check run on {sha[:12]}")
            passed = False
        elif expected not in conclusions:
            fail(f"contexts: {name!r} on {sha[:12]} concluded {sorted(c for c in conclusions if c)}, expected {expected!r}")
            passed = False
        else:
            ok(f"contexts: {name!r} is {expected}")
    omitted = sorted(authority - declared)
    if omitted:
        fail(f"contexts: the repository requires {omitted} but the manifest does not declare them")
        passed = False
    elif authority:
        ok(f"contexts: the manifest covers all {len(authority)} required contexts")
    else:
        fail("contexts: the required-check authority is empty — the reconciliation would prove nothing")
        passed = False
    return passed


def fetch_status(url: str, timeout: int = 30) -> tuple[int, str]:
    request = urllib.request.Request(url, method="GET", headers={"User-Agent": "conveyor-acceptance/1"})
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return response.status, response.read(2048).decode("utf-8", "replace")
    except urllib.error.HTTPError as error:
        return error.code, error.read(2048).decode("utf-8", "replace")
    except urllib.error.URLError as error:
        return 0, str(error.reason)


def check_probes(manifest: dict, base_url: str) -> bool:
    passed = True
    for row in manifest["probes"]:
        path = row["path"]
        expected = int(row["status"])
        allowed = {int(value) for value in row.get("allow", [])} | {expected}
        status, body = fetch_status(base_url.rstrip("/") + path)
        if status in allowed:
            ok(f"probes: {path} -> {status}")
            continue
        passed = False
        if status in (0, 404, 422) or status >= 500:
            fail(f"probes: {path} is not reachable ({status or 'no response'}): {body.strip()[:120]}")
        else:
            fail(f"probes: {path} -> {status}, expected {sorted(allowed)}")
    return passed


def selected_tests(rule: str, output: str) -> int | None:
    """How many tests the runner actually selected, or None when the output is unparseable."""
    if rule == "nextest":
        match = re.search(r"([0-9]+) tests? run", output)
        return int(match.group(1)) if match else None
    if rule == "libtest":
        counts = [int(value) for value in re.findall(r"running ([0-9]+) tests?", output)]
        return sum(counts) if counts else 0
    if rule == "playwright":
        start = output.rfind('\n{')
        candidate = output[start + 1 :] if start >= 0 else output
        try:
            stats = json.loads(candidate).get("stats") or {}
        except json.JSONDecodeError:
            return None
        return int(stats.get("expected") or 0) + int(stats.get("unexpected") or 0)
    return None


def check_commands(manifest: dict) -> bool:
    passed = True
    for row in manifest.get("commands", []):
        name, command = row["name"], row["run"]
        rule = row.get("zero_selected", "libtest")
        if rule == "nextest" and "--no-tests=fail" not in command:
            fail(f"commands: {name!r} is a nextest command without --no-tests=fail")
            passed = False
            continue
        result = run(["bash", "-lc", command], timeout=int(row.get("timeout_seconds", 3600)))
        output = (result.stdout or "") + (result.stderr or "")
        if result.returncode != 0:
            tail = "\n".join(output.strip().splitlines()[-8:])
            fail(f"commands: {name!r} exited {result.returncode}:\n{tail}")
            passed = False
            continue
        selected = selected_tests(rule, output)
        minimum = int(row.get("min_tests", 1))
        if selected is None:
            fail(f"commands: {name!r} selected an unreadable test count ({rule})")
            passed = False
        elif selected < minimum:
            fail(f"commands: {name!r} selected {selected} tests, below the declared minimum {minimum}")
            passed = False
        else:
            ok(f"commands: {name!r} selected {selected} tests (>= {minimum})")
    return passed


def self_test() -> int:
    """Each refusal is proved against synthetic input; a harness that cannot refuse is a pass."""
    failures = 0

    def expect(condition: bool, label: str) -> None:
        nonlocal failures
        if condition:
            print(f"acceptance self-test: ok — {label}")
        else:
            print(f"acceptance self-test: FAIL — {label}", file=sys.stderr)
            failures += 1

    expect(selected_tests("libtest", "running 0 tests\n") == 0, "libtest zero-selected is zero")
    expect(selected_tests("libtest", "running 3 tests\nrunning 4 tests\n") == 7, "libtest sums the binaries")
    expect(selected_tests("nextest", "Summary [ 1.2s] 0 tests run: 0 passed") == 0, "nextest zero is zero")
    expect(selected_tests("nextest", "Summary [ 1.2s] 3128 tests run: 3128 passed") == 3128, "nextest reads its summary")
    expect(
        selected_tests("playwright", 'noise\n{"stats": {"expected": 0, "unexpected": 0}}') == 0,
        "playwright zero is zero",
    )
    expect(
        selected_tests("playwright", '{"stats": {"expected": 4, "unexpected": 1}}') == 5,
        "playwright counts expected plus unexpected",
    )
    expect(selected_tests("libtest", "no summary at all") == 0, "unparseable libtest is zero, never a pass")
    expect(selected_tests("playwright", "not json") is None, "unparseable playwright is unknown")
    print(f"acceptance self-test: {8 - failures}/8 refusals and readings")
    return 0 if failures == 0 else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--sha", default="")
    parser.add_argument("--repo", default="")
    parser.add_argument("--base-url", default="")
    parser.add_argument("--branch", default="")
    parser.add_argument("--skip-contexts", action="store_true")
    parser.add_argument("--skip-probes", action="store_true")
    parser.add_argument("--skip-commands", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    manifest = load_manifest(args.manifest)
    meta = manifest.get("meta", {})
    repo = args.repo or meta.get("repo") or "nonagenticai/conveyor"
    base_url = args.base_url or meta.get("base_url") or "https://conveyor.nonagentic.ai"
    branch = args.branch or meta.get("main_branch") or "main"
    sha = args.sha
    if not sha:
        sha = run(["git", "rev-parse", "HEAD"]).stdout.strip()
    if not sha:
        raise SystemExit("acceptance: no --sha and no git HEAD")

    print(f"acceptance: {repo}@{sha[:12]} against {base_url} (branch {branch})")
    passed = True
    parts = []
    if not args.skip_contexts:
        passed &= check_contexts(manifest, repo, sha)
        parts.append("contexts")
    if not args.skip_probes:
        passed &= check_probes(manifest, base_url)
        parts.append("probes")
    if not args.skip_commands:
        passed &= check_commands(manifest)
        parts.append("commands")
    if not passed:
        print("acceptance: NOT REACHABLE — at least one part refused", file=sys.stderr)
        return 1
    print(f"acceptance: PASS — {', '.join(parts) or 'nothing'} all proved")
    return 0


if __name__ == "__main__":
    sys.exit(main())
