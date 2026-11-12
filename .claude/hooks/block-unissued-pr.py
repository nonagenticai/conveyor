#!/usr/bin/env python3
"""PreToolUse hook: deny `gh pr create` in conveyor unless it is an issue PR.

The delivery protocol (docs/plans/03-DELIVERY.md § The PR — exactly one) says: the PR head is
`issue/<n>-<slug>` and the body carries `Closes #<n>`. Measured 2026-11-11: PR #35/#36 were opened
from `lane/*` with no issue and no closing reference — two PRs and two CI runs for one commit, and
the queue could not see the work.

DENIED in this repo:
  * a head branch that does not match `issue/<n>-<slug>`,
  * a body with no `Closes`/`Fixes`/`Resolves #<n>`.

ALLOWED: everything else, including PRs in other repos (`-R`), and `--body-file -` (stdin), which
this hook cannot read without consuming it — fail open, and the protocol still requires the line.

FAILS OPEN on anything it cannot resolve: an unparseable command, a detached HEAD, an unreadable
body file, a git error. A wrong deny blocks PR creation for every session in this repo.

Tested by `.claude/hooks/tests/test-guards.py` through the same JSON payload the harness sends.
"""

from __future__ import annotations

import json
import os
import re
import shlex
import subprocess
import sys
from typing import NoReturn

REPO = "nonagenticai/conveyor"
HEAD_RE = re.compile(r"^issue/[0-9]+-.+")
CLOSES_RE = re.compile(r"(?i)\b(?:closes|fixes|resolves)\s*:?\s*#\d+\b")


def allow() -> NoReturn:
    sys.exit(0)


def deny(reason: str) -> NoReturn:
    print(
        json.dumps(
            {
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": reason,
                }
            }
        )
    )
    sys.exit(0)


WRAPPERS = {"env", "command", "exec", "nohup", "time", "nice", "sudo", "caffeinate"}

# A heredoc BODY is data: writing a document that mentions a PR command must not be denied.
HEREDOC = re.compile(r"<<-?\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\1")


def strip_heredocs(command: str) -> str:
    lines = command.split("\n")
    kept: list[str] = []
    i = 0
    while i < len(lines):
        line = lines[i]
        kept.append(line)
        match = HEREDOC.search(line)
        if match:
            delim = match.group(2)
            i += 1
            while i < len(lines) and lines[i].strip() != delim:
                i += 1
        i += 1
    return "\n".join(kept)


def split_simple(command: str) -> list[str]:
    parts: list[str] = []
    buf: list[str] = []
    quote: str | None = None
    i = 0
    while i < len(command):
        ch = command[i]
        if quote:
            buf.append(ch)
            if ch == "\\" and quote == '"':
                i += 1
                if i < len(command):
                    buf.append(command[i])
            elif ch == quote:
                quote = None
        elif ch in "'\"":
            quote = ch
            buf.append(ch)
        elif command.startswith("&&", i) or command.startswith("||", i):
            parts.append("".join(buf))
            buf = []
            i += 2
            continue
        elif ch in ";|\n":
            parts.append("".join(buf))
            buf = []
            i += 1
            continue
        else:
            buf.append(ch)
        i += 1
    if quote:
        raise ValueError("unbalanced quote")
    parts.append("".join(buf))
    return [p.strip() for p in parts if p.strip()]


def tokens_of(part: str) -> list[str] | None:
    try:
        return shlex.split(part, comments=True, posix=True)
    except ValueError:
        return None


def strip_wrappers(tokens: list[str]) -> list[str]:
    i = 0
    while i < len(tokens):
        tok = tokens[i]
        if "=" in tok and tok.split("=", 1)[0].isidentifier():
            i += 1
            continue
        if tok in WRAPPERS:
            i += 1
            continue
        break
    return tokens[i:]


def _git(args: list[str], cwd: str) -> list[str] | None:
    try:
        out = subprocess.run(
            ["git", "-C", cwd, *args], capture_output=True, text=True, timeout=3, check=False
        )
    except (OSError, subprocess.SubprocessError):
        return None
    if out.returncode != 0:
        return None
    return out.stdout.splitlines()


def _repo_common_root(here: str) -> str | None:
    lines = _git(["rev-parse", "--git-common-dir"], here)
    if not lines:
        return None
    common = lines[0].strip()
    if not os.path.isabs(common):
        common = os.path.join(here, common)
    return os.path.dirname(os.path.realpath(common))


def _shell_command_flag(tokens: list[str]) -> int | None:
    """The index of `-c` even when clustered (`-lc`, `-ec`); None when the shell has no -c."""
    for i, tok in enumerate(tokens[1:], start=1):
        if tok.startswith("--"):
            continue
        if tok.startswith("-") and "c" in tok[1:]:
            return i
    return None


def _flag(tokens: list[str], *names: str) -> str | None:
    for i, tok in enumerate(tokens):
        for name in names:
            if tok == name:
                return tokens[i + 1] if i + 1 < len(tokens) else None
            if tok.startswith(name + "="):
                return tok.split("=", 1)[1]
    return None


def _pr_create(tokens: list[str]) -> bool:
    """`gh` must be the COMMAND WORD (or an absolute path ending in /gh), not a quoted mention."""
    if not tokens or not (tokens[0] == "gh" or tokens[0].endswith("/gh")):
        return False
    rest = tokens[1:]
    return len(rest) >= 2 and rest[0] == "pr" and rest[1] == "create"


def decide(command: str, cwd: str, _depth: int = 0) -> tuple[bool, str]:
    """(allow, reason). Pure enough to test; no stdin, no process exit."""
    here = os.path.dirname(os.path.abspath(__file__))
    repo_root = _repo_common_root(here)
    if not repo_root:
        return True, "cannot resolve this hook's repo"

    try:
        parts = split_simple(strip_heredocs(command))
    except ValueError:
        return True, "unparseable command"

    current = cwd
    for part in parts:
        toks = tokens_of(part)
        if toks is None:
            return True, "unparseable simple command"
        toks = strip_wrappers(toks)
        if not toks:
            continue
        if toks[0] in ("cd", "pushd"):
            if len(toks) < 2 or toks[1] == "-" or "$" in toks[1]:
                current = None
            else:
                nxt = os.path.expanduser(toks[1])
                current = nxt if os.path.isabs(nxt) else os.path.join(current or cwd, nxt)
                if not os.path.isdir(current):
                    current = None
            continue
        # `bash -c '<script>'` / `eval <script>` run the inner command; a guard that stops at the
        # wrapper is a bypass, so the inner script is decided with the same cwd (two levels max).
        if toks[0] in ("bash", "sh", "zsh") and _depth < 2:
            idx = _shell_command_flag(toks)
            if idx is not None and idx + 1 < len(toks):
                ok, reason = decide(toks[idx + 1], current or cwd, _depth + 1)
                if not ok:
                    return ok, reason
            continue
        if toks[0] == "eval" and _depth < 2:
            ok, reason = decide(" ".join(toks[1:]), current or cwd, _depth + 1)
            if not ok:
                return ok, reason
            continue
        if not _pr_create(toks):
            continue

        repo_arg = _flag(toks, "-R", "--repo")
        if repo_arg is not None:
            if repo_arg.lower() != REPO:
                continue  # another repo: out of scope
        else:
            if current is None:
                return True, "cwd cannot be resolved"
            lines = _git(["rev-parse", "--show-toplevel", "--git-common-dir"], current)
            if not lines or len(lines) < 2:
                continue  # not a repo we can resolve: fail open
            common = lines[1].strip()
            if not os.path.isabs(common):
                common = os.path.join(current, common)
            if os.path.realpath(os.path.dirname(common)) != os.path.realpath(repo_root):
                continue  # a different repository

        head = _flag(toks, "-H", "--head")
        if head is None:
            if current is None:
                return True, "cwd cannot be resolved"
            lines = _git(["rev-parse", "--abbrev-ref", "HEAD"], current)
            if not lines or lines[0].strip() == "HEAD":
                return True, "detached HEAD cannot open a PR"
            head = lines[0].strip()

        if "--fill" in toks or "-f" in toks:
            return True, "a filled body cannot be read by the hook"
        body: str | None = _flag(toks, "-b", "--body")
        if body is not None and ("$" in body or "`" in body):
            return True, "a shell-expanded body cannot be read by the hook"
        if body is None:
            body_file = _flag(toks, "-F", "--body-file")
            if body_file is None:
                body = ""
            elif body_file == "-":
                return True, "body on stdin cannot be read without consuming it"
            else:
                path = body_file if os.path.isabs(body_file) else os.path.join(current or cwd, body_file)
                try:
                    with open(path, encoding="utf-8") as fh:
                        body = fh.read()
                except OSError:
                    return True, "body file cannot be read"

        if not HEAD_RE.match(head):
            return False, (
                f"PR denied: head `{head}` must be `issue/<n>-<slug>`.\n"
                "The unit is a GitHub issue (issue-pickup creates the branch); a `lane/*` or "
                "`slice/*` PR is outside the delivery model.\n"
                "Protocol: docs/plans/03-DELIVERY.md § The PR — exactly one."
            )
        if not CLOSES_RE.search(body or ""):
            return False, (
                "PR denied: the body carries no `Closes #<n>` (or `Fixes`/`Resolves`).\n"
                "Every PR closes exactly one issue; the issue is the claim and the PR is the "
                "finish line.\n"
                "Protocol: docs/plans/03-DELIVERY.md § The PR — exactly one."
            )
        return True, "issue PR with a closing reference"
    return True, "no gh pr create found"


def main() -> None:
    try:
        payload = json.load(sys.stdin)
    except (json.JSONDecodeError, ValueError):
        allow()
    if not isinstance(payload, dict) or payload.get("tool_name") != "Bash":
        allow()
    command = (payload.get("tool_input") or {}).get("command") or ""
    if not isinstance(command, str):
        allow()
    cwd = payload.get("cwd") or os.getcwd()
    try:
        ok, reason = decide(command, cwd)
    except Exception:
        allow()
    if ok:
        allow()
    deny(reason)


if __name__ == "__main__":
    main()
