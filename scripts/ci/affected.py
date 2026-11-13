#!/usr/bin/env python3
"""Map changed paths to the workspace packages they can affect.

The reverse-dependency walk uses `cargo metadata --no-deps`, so a change to a shared crate
selects every member that depends on it, however deep.
"""

import argparse
import json
import os
import subprocess

# A change here can alter any package's build or behaviour.
GLOBAL_FILES = {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "docker/Dockerfile.service"}
GLOBAL_PREFIXES = (".cargo/", "scripts/ci/", "crates/platform/")


def root() -> str:
    out = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True, check=True
    ).stdout
    return os.path.abspath(out.strip())


def changed_files(args, repo: str) -> list[str]:
    if args.files is not None:
        return [p for p in args.files.split() if p]
    base = args.base or "origin/main"
    head = args.head or "HEAD"
    out = subprocess.run(
        ["git", "diff", "--name-only", f"{base}...{head}"],
        cwd=repo,
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    return [p for p in out.splitlines() if p]


def workspace(repo: str):
    out = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=repo,
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    meta = json.loads(out)
    packages = {}
    dirs = []
    for pkg in meta["packages"]:
        rel = os.path.relpath(os.path.dirname(pkg["manifest_path"]), repo).replace(os.sep, "/")
        packages[pkg["name"]] = {"dir": rel, "dependents": []}
        dirs.append((rel, pkg["name"]))
    for pkg in meta["packages"]:
        for dep in pkg["dependencies"]:
            # `path` is present only for a path dependency, i.e. a workspace member.
            if dep.get("path") and dep["name"] in packages:
                packages[dep["name"]]["dependents"].append(pkg["name"])
    return packages, dirs


def owner(path: str, dirs) -> str | None:
    best = None
    for rel, name in dirs:
        if path == rel or path.startswith(rel + "/"):
            if best is None or len(rel) > len(best[0]):
                best = (rel, name)
    return best[1] if best else None


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--files", help="a space-separated file list (skips git)")
    parser.add_argument("--base", help="the diff base (default origin/main)")
    parser.add_argument("--head", help="the diff head (default HEAD)")
    parser.add_argument("--apps", action="store_true", help="only packages under apps/")
    parser.add_argument("--json", action="store_true", help="print a JSON array")
    args = parser.parse_args()

    repo = root()
    files = changed_files(args, repo)
    packages, dirs = workspace(repo)

    if any(f in GLOBAL_FILES or f.startswith(GLOBAL_PREFIXES) for f in files):
        selected = set(packages)
    else:
        selected = {name for name in (owner(f, dirs) for f in files) if name}
        changed = True
        while changed:
            changed = False
            for name in list(selected):
                for dependent in packages[name]["dependents"]:
                    if dependent not in selected:
                        selected.add(dependent)
                        changed = True

    if args.apps:
        selected = {name for name in selected if packages[name]["dir"].startswith("apps/")}

    result = sorted(selected)
    print(json.dumps(result) if args.json else " ".join(result))


if __name__ == "__main__":
    main()
