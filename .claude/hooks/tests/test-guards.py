#!/usr/bin/env python3
"""Drive both repo guards through their `decide()` functions with the harness's real shapes.

Run: python3 .claude/hooks/tests/test-guards.py   (exit 0 = every case behaved)

⛔ The PR cases below are Python string data. Never move one into a shell command that runs this
file — a shell argument containing the literal command is read by the global hooks as a command,
which is the lexer trap `block-duplicate-pr.py` documents.
"""

from __future__ import annotations

import importlib.util
import json
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
HOOKS = os.path.dirname(HERE)
ROOT = os.path.dirname(os.path.dirname(HOOKS))


def _common_root(path: str) -> str:
    out = subprocess.run(
        ["git", "-C", path, "rev-parse", "--git-common-dir"],
        capture_output=True,
        text=True,
        check=True,
    )
    common = out.stdout.strip()
    if not os.path.isabs(common):
        common = os.path.join(path, common)
    return os.path.dirname(os.path.realpath(common))


SHARED_ROOT = _common_root(ROOT)


def load(name: str, filename: str):
    path = os.path.join(HOOKS, filename)
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader, f"cannot load {path}"
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


commit_guard = load("block_shared_root_commit", "block-shared-root-commit.py")
pr_guard = load("block_unissued_pr", "block-unissued-pr.py")


def linked_worktree() -> tuple[str, bool]:
    """(path, created_by_us). Prefer an existing linked worktree; create one only if absent."""
    out = subprocess.run(
        ["git", "-C", SHARED_ROOT,"worktree", "list", "--porcelain"],
        capture_output=True,
        text=True,
        check=False,
    )
    if out.returncode == 0:
        for line in out.stdout.splitlines():
            if line.startswith("worktree "):
                path = line.split(" ", 1)[1].strip()
                if os.path.realpath(path) != os.path.realpath(SHARED_ROOT) and os.path.isfile(
                    os.path.join(path, ".git")
                ):
                    return path, False
    tmp = tempfile.mkdtemp(prefix="guard-wt-")
    # The temp dir must be empty for `worktree add`.
    os.rmdir(tmp)
    subprocess.run(
        ["git", "-C", SHARED_ROOT, "worktree", "add", "--detach", tmp, "HEAD"],
        check=True,
        capture_output=True,
    )
    return tmp, True


def main() -> int:
    wt, created = linked_worktree()
    cases: list[tuple[str, object, str, str, bool]] = []

    def add(name, mod, command, cwd, expect_allow):
        cases.append((name, mod, command, cwd, expect_allow))

    # ---- shared-root commit guard -------------------------------------------------------
    add("root-commit-deny", commit_guard, "git commit -m x", SHARED_ROOT,False)
    add("worktree-commit-allow", commit_guard, "git commit --allow-empty -m x", wt, True)
    add("root-commit-via-C-deny", commit_guard, f"git -C {SHARED_ROOT} commit -m x", wt, False)
    add("worktree-commit-via-C-allow", commit_guard, f"git -C {wt} commit -m x", SHARED_ROOT,True)
    add("retry-script-root-deny", commit_guard, "~/.claude/scripts/commit-with-retry.sh msg", SHARED_ROOT,False)
    add("retry-script-worktree-allow", commit_guard, "~/.claude/scripts/commit-with-retry.sh msg", wt, True)
    add("commit-mention-allow", commit_guard, "echo git commit", SHARED_ROOT, True)
    add(
        "commit-heredoc-mention-allow",
        commit_guard,
        "cat > /tmp/doc.md <<'EOF'\ngit commit -m x\nEOF",
        SHARED_ROOT,
        True,
    )
    add(
        "commit-heredoc-then-real-deny",
        commit_guard,
        "cat > /tmp/doc.md <<'EOF'\nx\nEOF\ngit commit -m x",
        SHARED_ROOT,
        False,
    )
    add("commit-abs-git-deny", commit_guard, "/usr/bin/git commit -m x", SHARED_ROOT, False)
    add("commit-unparseable-allow", commit_guard, "git commit 'unterminated", SHARED_ROOT,True)
    add("commit-after-cd-allow", commit_guard, f"cd {wt} && git commit -m x", SHARED_ROOT,True)

    # ---- unissued-PR guard --------------------------------------------------------------
    add("pr-nonissue-deny", pr_guard, "gh pr create --head lane/x --body 'Closes #1'", SHARED_ROOT,False)
    add("pr-no-closes-deny", pr_guard, "gh pr create --head issue/39-x --body 'nothing'", SHARED_ROOT,False)
    add("pr-ok-body-allow", pr_guard, "gh pr create --head issue/39-x --body 'Closes #39'", SHARED_ROOT,True)
    add("pr-ok-fixes-allow", pr_guard, "gh pr create --head issue/39-x --body 'Fixes #39'", SHARED_ROOT,True)
    add("pr-body-stdin-allow", pr_guard, "gh pr create --head issue/39-x --body-file -", SHARED_ROOT,True)
    add("pr-other-repo-allow", pr_guard, "gh pr create -R other/repo --head lane/x --body 'x'", SHARED_ROOT,True)
    add("pr-mention-allow", pr_guard, "echo gh pr create", SHARED_ROOT, True)
    add("pr-body-expansion-allow", pr_guard, 'gh pr create --head issue/39-x -b "$(cat body.md)"', SHARED_ROOT, True)
    add("pr-body-var-allow", pr_guard, 'gh pr create --head issue/39-x -b "$BODY"', SHARED_ROOT, True)
    add("pr-bash-c-deny", pr_guard, "bash -c \"gh pr create --head lane/x --body 'Closes #1'\"", SHARED_ROOT, False)
    add("commit-bash-c-deny", commit_guard, 'bash -c "git commit -m x"', SHARED_ROOT, False)
    add("commit-bash-lc-deny", commit_guard, 'bash -lc "git commit -m x"', SHARED_ROOT, False)
    add("pr-bash-lc-deny", pr_guard, "bash -lc \"gh pr create --head lane/x --body 'Closes #1'\"", SHARED_ROOT, False)
    add("pr-fill-allow", pr_guard, "gh pr create --head issue/39-x --fill", SHARED_ROOT, True)
    add(
        "pr-heredoc-mention-allow",
        pr_guard,
        "cat > /tmp/doc.md <<'EOF'\ngh pr create --head lane/x --body 'x'\nEOF",
        SHARED_ROOT,
        True,
    )
    add(
        "pr-heredoc-then-real-deny",
        pr_guard,
        "cat > /tmp/doc.md <<'EOF'\nx\nEOF\ngh pr create --head lane/x --body 'Closes #1'",
        SHARED_ROOT,
        False,
    )
    add("pr-abs-gh-deny", pr_guard, "/opt/homebrew/bin/gh pr create --head lane/x --body 'Closes #1'", SHARED_ROOT, False)
    add("pr-unparseable-allow", pr_guard, "gh pr create --head \"unterminated", SHARED_ROOT,True)

    with tempfile.NamedTemporaryFile("w", suffix=".md", delete=False) as fh:
        fh.write("Closes #39\n")
        body_file = fh.name
    try:
        add(
            "pr-body-file-allow",
            pr_guard,
            f"gh pr create --head issue/39-x --body-file {body_file}",
            ROOT,
            True,
        )
        add(
            "pr-body-file-missing-allow",
            pr_guard,
            f"gh pr create --head issue/39-x --body-file {body_file}.missing",
            ROOT,
            True,  # unreadable body file = fail open
        )

        failures = 0
        for name, mod, command, cwd, expect_allow in cases:
            ok, reason = mod.decide(command, cwd)
            verdict = "PASS" if ok == expect_allow else "FAIL"
            if verdict == "FAIL":
                failures += 1
            print(f"{verdict:4} {name:34} expected={'allow' if expect_allow else 'deny ':5} "
                  f"got={'allow' if ok else 'deny ':5} {reason}")
    finally:
        os.unlink(body_file)
        if created:
            subprocess.run(
                ["git", "-C", SHARED_ROOT,"worktree", "remove", "--force", wt],
                check=False,
                capture_output=True,
            )

    # ---- registration -------------------------------------------------------------------
    with open(os.path.join(ROOT, ".claude", "settings.json"), encoding="utf-8") as fh:
        settings = json.load(fh)
    hooks = settings["hooks"]["PreToolUse"][0]["hooks"]
    registered = len(hooks) == 2
    print(f"{'PASS' if registered else 'FAIL':4} {'settings-registration':34} "
          f"PreToolUse Bash hooks = {len(hooks)}")
    if not registered:
        failures += 1

    print(f"\n{len(cases) + 1 - failures}/{len(cases) + 1} cases passed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
