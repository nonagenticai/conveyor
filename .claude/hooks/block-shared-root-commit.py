#!/usr/bin/env python3
"""PreToolUse hook: deny `git commit` in conveyor's shared root checkout.

The delivery protocol (docs/plans/03-DELIVERY.md § Claim) says: work only in
`.worktrees/issue-<n>`; never commit in the shared root. Measured 2026-11-11: the shared root was
parked on a merged lane branch, so a commit there would land on a dead branch — the shape that
produced PR #35 (one SPEC change, two PRs, two CI runs).

DENIED: `git commit` whose target is THIS repo's shared root — the toplevel whose `.git` is a
DIRECTORY. A linked worktree has `.git` as a FILE and is allowed. `git -C <worktree> commit` is
allowed too, because the target is resolved, not the session cwd.

ALSO COVERED: `~/.claude/scripts/commit-with-retry.sh`, the sanctioned commit route — a hook that
reads only the literal string `git commit` would miss it.

FAILS OPEN on anything it cannot resolve: an unparseable command, an unknown `cd`, a detached
script, a missing git. A wrong deny here blocks commits for every session, which is far worse than
one missed deny; the protocol text remains authoritative.

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

# A heredoc BODY is data. `cat > doc.md <<'EOF' … git commit … EOF` writes a document; it does not
# run a commit. Without this the lexer reads the body lines as commands and denies document writes.
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
    """Split on && || ; | and newlines outside quotes. ValueError on an unbalanced quote."""
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
            ["git", "-C", cwd, *args],
            capture_output=True,
            text=True,
            timeout=3,
            check=False,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    if out.returncode != 0:
        return None
    return out.stdout.splitlines()


def _repo_common_root(here: str) -> str | None:
    """The shared root of the repo this hook belongs to, resolved even from a worktree."""
    lines = _git(["rev-parse", "--git-common-dir"], here)
    if not lines:
        return None
    common = lines[0].strip()
    if not os.path.isabs(common):
        common = os.path.join(here, common)
    return os.path.dirname(os.path.realpath(common))


def _git_commit_target(tokens: list[str], cwd: str) -> tuple[str | None, bool]:
    """(target_dir, is_commit). target None means 'cannot resolve' -> fail open later.

    `git` must be the COMMAND WORD (or an absolute path ending in /git). Searching anywhere in the
    token list would deny `echo git commit`, which is not a commit.
    """
    if not tokens or not (tokens[0] == "git" or tokens[0].endswith("/git")):
        return None, False
    rest = tokens[1:]
    target = cwd
    i = 0
    while i < len(rest):
        tok = rest[i]
        if tok in ("-C", "--git-dir", "--work-tree"):
            if i + 1 >= len(rest):
                return None, False
            if tok == "-C":
                target = rest[i + 1]
            else:
                return None, False  # an explicit git-dir is outside this hook's model
            i += 2
            continue
        if tok in ("-c", "--namespace"):
            i += 2
            continue
        if tok.startswith("-"):
            i += 1
            continue
        return target, tok == "commit"
    return target, False


def _shell_command_flag(tokens: list[str]) -> int | None:
    """The index of `-c` even when clustered (`-lc`, `-ec`); None when the shell has no -c."""
    for i, tok in enumerate(tokens[1:], start=1):
        if tok.startswith("--"):
            continue
        if tok.startswith("-") and "c" in tok[1:]:
            return i
    return None


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

        # `bash -c '<script>'` / `eval <script>` run the inner command; stopping at the wrapper
        # would be a bypass. Decide the inner script with the same cwd (two levels max).
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

        if "commit-with-retry.sh" in toks[0]:
            target, is_commit = current or cwd, True
        else:
            target, is_commit = _git_commit_target(toks, current or cwd)
        if not is_commit:
            continue
        if current is None or target is None:
            return True, "target directory cannot be resolved"
        target_abs = os.path.abspath(os.path.join(current, os.path.expanduser(target)))
        lines = _git(["rev-parse", "--show-toplevel", "--git-common-dir"], target_abs)
        if not lines or len(lines) < 2:
            return True, "not a git repository"
        toplevel = os.path.realpath(lines[0].strip())
        common = lines[1].strip()
        if not os.path.isabs(common):
            common = os.path.join(target_abs, common)
        shared_root = os.path.realpath(os.path.dirname(common))
        if toplevel == shared_root and os.path.realpath(repo_root) == shared_root:
            return False, (
                f"Commit denied: `{shared_root}` is the shared root checkout, and another "
                "session's branch may be checked out there.\n\n"
                "Claim an issue and work in a worktree:\n"
                "  git worktree add -b issue/<n>-<slug> .worktrees/issue-<n> origin/main\n"
                "  cd .worktrees/issue-<n> && git commit ...\n\n"
                "Protocol: docs/plans/03-DELIVERY.md § Claim."
            )
    return True, "no shared-root commit found"


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
    except Exception:  # a crash is "cannot read", never a verdict
        allow()
    if ok:
        allow()
    deny(reason)


if __name__ == "__main__":
    main()
