#!/usr/bin/env python3
"""build-docs.py — render every tracked Markdown document as a standalone HTML page.

The design pack is published verbatim by `nonagentic_homepage`, so the HTML has to exist
in the pack: there is no runtime Markdown renderer and no fetch (the pack must work from
`file://`). This script is the only generator.

Sources: every `docs/**/*.md` plus the root `README.md`, minus `docs/design/legacy/`
(the retired v1 studies) and `docs/design/docs/` (this output). `docs/design/scripts/`
holds no Markdown. Output mirrors the repo path under `docs/design/docs/`.

Links:
  * a relative `.md` link resolves to the generated `.html` page;
  * a relative link to any other tracked file resolves to the GitHub blob/tree URL;
  * an unresolvable relative link fails the build — a broken document is not published.

Usage:
    python3 docs/design/scripts/build-docs.py            # regenerate + index
    python3 docs/design/scripts/build-docs.py --check    # verify committed output (no pandoc)
    python3 docs/design/scripts/build-docs.py --verify-render  # byte-identical re-render

`--check` is the CI gate: pandoc is not needed there, the committed pages and the index
are compared against the sources.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import posixpath
import re
import shutil
import subprocess
import sys
from pathlib import Path
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[3]
PACK = ROOT / "docs" / "design"
OUT = PACK / "docs"
TEMPLATE = PACK / "scripts" / "doc-template.html"
SCHEMA = "design-docs/v1"
REPO_URL = "https://github.com/nonagenticai/conveyor"
RAW_URL = REPO_URL + "/blob/main/"
TREE_URL = REPO_URL + "/tree/main/"

SKIP_PREFIXES = (
    "docs/design/legacy/",
    "docs/design/docs/",
    "docs/design/scripts/",
    ".claude/",
    "crates/",
    "seed/",
    "scripts/",
    "parity/",
)
SKIP_FILES = {"CLAUDE.md"}

HREF = re.compile(r'(?P<attr>href|src)="(?P<url>[^"]*)"')
HEADING = re.compile(r"^#\s+(.+?)\s*$", re.MULTILINE)
MATCH_SCHEME = re.compile(r"^(?:[a-z][a-z0-9+.\-]*:|//)", re.IGNORECASE)

# The generated page is a public artifact. These are the guard's hard set
# (`scripts/ci/no-private-refs.sh` from #54): the page must never publish an internal
# hostname, a tailnet address, a Tailscale name, a host login or the private CI repo
# path — even when the source document still carries it. `check-docs.cjs` asserts the
# same set, so a generator change cannot silently reintroduce one. The sources are
# scrubbed separately in #56; this keeps the published documents clean now.
REDACTIONS = (
    (re.compile(r"\b([A-Za-z0-9][A-Za-z0-9_-]*)\.internal\b"), r"\1.example.com"),
    (re.compile(r"nonagenticai/gitops-production"), "gitops-production"),
    (re.compile(r"strato-vps"), "deploy-key"),
    (re.compile(r"strato-server"), "deploy-host"),
    (re.compile(r"\bts\.net\b"), "tailnet"),
    (
        re.compile(r"\b100\.(?:6[4-9]|[7-9]\d|1[01]\d|12[0-7])\.\d{1,3}\.\d{1,3}\b"),
        "192.0.2.10",
    ),
)


def redact(text: str) -> str:
    for pattern, replacement in REDACTIONS:
        text = pattern.sub(replacement, text)
    return text


def git_files() -> list[str]:
    out = subprocess.check_output(
        ["git", "-C", str(ROOT), "ls-files"], text=True, errors="replace"
    )
    return sorted(line for line in out.splitlines() if line)


def in_scope(files: list[str]) -> list[str]:
    selected: list[str] = []
    for path in files:
        if path in SKIP_FILES or not path.endswith(".md"):
            continue
        if path != "README.md" and not path.startswith("docs/"):
            continue
        if any(path.startswith(prefix) for prefix in SKIP_PREFIXES):
            continue
        selected.append(path)
    return selected


def output_rel(source: str) -> str:
    """`docs/plans/00-INDEX.md` -> `plans/00-INDEX.html`; root README -> `README.html`."""
    if source == "README.md":
        return "README.html"
    return source[len("docs/") : -len(".md")] + ".html"


def section(source: str) -> str:
    if source == "README.md":
        return "Root"
    for prefix, name in (
        ("docs/design/", "Design"),
        ("docs/evidence/", "Evidence"),
        ("docs/decisions/", "Decisions"),
        ("docs/plans/", "Plans"),
    ):
        if source.startswith(prefix):
            return name
    return "Spec"


def title_of(source: str) -> str:
    text = (ROOT / source).read_text(encoding="utf-8")
    match = HEADING.search(text)
    if not match:
        return Path(source).stem
    title = re.sub(r"[`*_]", "", match.group(1)).strip()
    return re.sub(r"\s+#+$", "", title)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(65536), b""):
            digest.update(chunk)
    return digest.hexdigest()


def rewrite_links(
    html: str,
    source: str,
    out_rel: str,
    published: dict[str, str],
    unresolved: list[str],
) -> str:
    source_dir = posixpath.dirname(source)
    out_dir = posixpath.dirname(out_rel)

    def replace(match: re.Match[str]) -> str:
        attr, url = match.group("attr"), match.group("url")
        if not url or url.startswith("#") or url.startswith("mailto:") or MATCH_SCHEME.match(url):
            return match.group(0)
        target, _, fragment = url.partition("#")
        target = unquote(target).lstrip("/")
        if not target:
            return match.group(0)
        resolved = posixpath.normpath(posixpath.join(source_dir, target))
        if resolved in published:
            link = posixpath.relpath(published[resolved], start=out_dir)
        else:
            disk = ROOT / resolved
            if not disk.exists():
                unresolved.append(f"{out_rel}: {url} -> {resolved}")
                return match.group(0)
            if attr == "src":
                asset = OUT / "_assets" / resolved
                asset.parent.mkdir(parents=True, exist_ok=True)
                if disk.is_file():
                    shutil.copy2(disk, asset)
                link = posixpath.relpath(f"_assets/{resolved}", start=out_dir)
            else:
                link = (TREE_URL if disk.is_dir() else RAW_URL) + resolved
        if fragment:
            link += "#" + fragment
        return f'{attr}="{link}"'

    return HREF.sub(replace, html)


def render(sources: list[str], published: dict[str, str]) -> list[str]:
    pandoc = shutil.which("pandoc")
    if not pandoc:
        raise SystemExit("build-docs: pandoc is not installed (needed to regenerate)")
    template = TEMPLATE.read_text(encoding="utf-8")
    errors: list[str] = []
    for source in sources:
        out_rel = published[source]
        target = OUT / out_rel
        target.parent.mkdir(parents=True, exist_ok=True)
        depth = len(Path(out_rel).parts) - 1
        # OUT is PACK/docs, so reaching the pack root always costs one extra level.
        prefix = "../" * (depth + 1)
        # Pandoc renders the fragment only: the page chrome is the template, so link
        # rewriting can never touch the pack's own asset/studio links.
        fragment = subprocess.run(
            [
                pandoc,
                "--from=gfm",
                "--to=html5",
                "--wrap=none",
                "--syntax-highlighting=none",
                str(ROOT / source),
            ],
            check=True,
            cwd=ROOT,
            capture_output=True,
        ).stdout.decode("utf-8")
        fragment = rewrite_links(fragment, source, out_rel, published, errors)
        fragment = redact(fragment)
        page = (
            template.replace("$title$", redact(title_of(source)))
            .replace("$root$", prefix)
            .replace("$source_url$", RAW_URL + source)
            .replace("$source$", source)
            .replace("$body$", fragment)
        )
        target.write_text(page, encoding="utf-8")
        print(f"  {source} -> {target.relative_to(PACK)}")
    return errors


def index(sources: list[str], published: dict[str, str]) -> dict:
    documents = []
    for source in sources:
        out_rel = published[source]
        source_bytes = (ROOT / source).read_bytes()
        documents.append(
            {
                "title": redact(title_of(source)),
                "path": "docs/" + out_rel,
                "source": source,
                "section": section(source),
                "sha256": hashlib.sha256(source_bytes).hexdigest(),
                "bytes": len(source_bytes),
                "source_url": RAW_URL + source,
            }
        )
    return {"schema": SCHEMA, "documents": documents}


def write_index(payload: dict) -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "documents.json").write_text(
        json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    (OUT / "documents.js").write_text(
        "window.CONVEYOR_DOCUMENTS = "
        + json.dumps(payload, indent=2, ensure_ascii=False)
        + ";\n",
        encoding="utf-8",
    )


def read_index() -> dict | None:
    path = OUT / "documents.json"
    if not path.exists():
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def check(sources: list[str], published: dict[str, str]) -> int:
    failures = 0
    payload = read_index()
    if not payload or payload.get("schema") != SCHEMA:
        print("check-docs: documents.json is absent or has the wrong schema")
        return 1
    listed = {doc["source"]: doc for doc in payload.get("documents", [])}
    if len(listed) != len(payload.get("documents", [])):
        print("check-docs: duplicate source entries in the index")
        failures += 1
    for source in sources:
        doc = listed.get(source)
        if not doc:
            print(f"check-docs: {source} is not indexed")
            failures += 1
            continue
        if doc.get("path") != "docs/" + published[source]:
            print(f"check-docs: {source} has a wrong output path")
            failures += 1
        target = PACK / doc["path"]
        if not target.exists():
            print(f"check-docs: {doc['path']} is missing")
            failures += 1
            continue
        if sha256_file(ROOT / source) != doc.get("sha256"):
            print(f"check-docs: {source} changed since the index was generated")
            failures += 1
    extra = set(listed) - set(sources)
    for source in sorted(extra):
        print(f"check-docs: index lists a source that is no longer in scope: {source}")
        failures += 1
    print(f"check-docs: {len(sources)} sources, {failures} failure(s)")
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify committed output; no pandoc")
    parser.add_argument("--verify-render", action="store_true", help="render twice and compare bytes")
    args = parser.parse_args()

    sources = in_scope(git_files())
    published = {source: output_rel(source) for source in sources}

    if args.check:
        return check(sources, published)

    if OUT.exists():
        shutil.rmtree(OUT)
    OUT.mkdir(parents=True, exist_ok=True)
    errors = render(sources, published)
    if errors:
        print("build-docs: unresolved links:", file=sys.stderr)
        for error in errors:
            print("  " + error, file=sys.stderr)
        return 1
    if args.verify_render:
        snapshot = {
            path.relative_to(OUT): path.read_bytes()
            for path in OUT.rglob("*")
            if path.is_file()
        }
        shutil.rmtree(OUT)
        OUT.mkdir(parents=True, exist_ok=True)
        errors = render(sources, published)
        if errors:
            print("build-docs: unresolved links on re-render: " + "; ".join(errors), file=sys.stderr)
            return 1
        drift = [
            str(rel)
            for rel, data in snapshot.items()
            if (OUT / rel).read_bytes() != data
        ]
        if drift:
            print("build-docs: non-deterministic render: " + ", ".join(drift))
            return 1
    write_index(index(sources, published))
    print(f"build-docs: {len(sources)} documents -> {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
