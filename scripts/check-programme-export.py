#!/usr/bin/env python3
"""check-programme-export — the committed `programme/` tree matches its manifest and the rules.

The database is the store; `programme/` is its one-way public copy. This gate runs on the tree
alone: every file is in the manifest, every manifest hash matches, every frontmatter parses, a
brief carries the mandatory fields, and no private infrastructure or TODO is published.

Usage: python3 scripts/check-programme-export.py programme/ [--self-test]
"""
from __future__ import annotations

import hashlib
import json
import re
import sys
import tempfile
from pathlib import Path

SCHEMA = "conveyor-programme-export/v1"
REQUIRED_FRONTMATTER = {
    "document_id",
    "version",
    "title",
    "project",
    "unit",
    "category",
    "published_at",
}
BRIEF_HEADINGS = [
    "## Outcome",
    "## Boundary",
    "## Reuse sources",
    "## Inception units",
    "## Dependencies",
    "## Acceptance",
    "## Capabilities",
    "## Gaps",
]
IPV4 = re.compile(r"\b(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})\b")
TAILNET = re.compile(r"\b100\.(6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7])\.\d{1,3}\.\d{1,3}\b")
UUID = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$")


def fail(errors: list[str], message: str) -> None:
    errors.append(message)


def parse_frontmatter(text: str) -> tuple[dict[str, str], str] | None:
    if not text.startswith("---\n"):
        return None
    end = text.find("\n---\n", 4)
    if end < 0:
        return None
    fields: dict[str, str] = {}
    for line in text[4:end].splitlines():
        if not line.strip():
            continue
        key, sep, value = line.partition(":")
        if not sep:
            return None
        value = value.strip()
        if value.startswith('"') and value.endswith('"'):
            value = json.loads(value)
        fields[key.strip()] = value
    return fields, text[end + 5 :]


def private_reference(text: str) -> str | None:
    for match in IPV4.finditer(text):
        octets = [int(part) for part in match.groups()]
        if octets[0] == 127 or octets == [0, 0, 0, 0]:
            continue
        return match.group(0)
    for pattern, label in (
        (TAILNET, "tailnet address"),
        (re.compile(r"/Users/[A-Za-z]"), "home path"),
        (re.compile(r"~/\.ssh|~/Movies"), "home path"),
    ):
        match = pattern.search(text)
        if match:
            return f"{label}: {match.group(0)}"
    return None


def check(root: Path) -> list[str]:
    errors: list[str] = []
    readme = root / "README.md"
    manifest_path = root / "manifest.json"
    if not readme.is_file():
        fail(errors, "README.md is missing")
    if not manifest_path.is_file():
        return errors + ["manifest.json is missing"]
    try:
        manifest = json.loads(manifest_path.read_text())
    except json.JSONDecodeError as error:
        return errors + [f"manifest.json is not JSON: {error}"]
    if manifest.get("schema") != SCHEMA:
        fail(errors, f"manifest schema is {manifest.get('schema')!r}, expected {SCHEMA!r}")
    if not str(manifest.get("generated_at") or "").strip():
        fail(errors, "manifest generated_at is empty")
    entries = manifest.get("files")
    if not isinstance(entries, list) or not entries:
        return errors + ["manifest files is empty"]

    listed: set[str] = set()
    for entry in entries:
        path = entry.get("path", "")
        listed.add(path)
        file = root.parent / path if path.startswith("programme/") else None
        if file is None:
            fail(errors, f"{path}: path must start with programme/")
            continue
        if not file.is_file():
            fail(errors, f"{path}: listed but missing")
            continue
        content = file.read_bytes()
        digest = hashlib.sha256(content).hexdigest()
        if digest != entry.get("sha256"):
            fail(errors, f"{path}: sha256 {digest} != manifest {entry.get('sha256')}")
        parsed = parse_frontmatter(content.decode())
        if parsed is None:
            fail(errors, f"{path}: frontmatter is missing or malformed")
            continue
        fields, body = parsed
        missing = REQUIRED_FRONTMATTER - fields.keys()
        if missing:
            fail(errors, f"{path}: frontmatter missing {sorted(missing)}")
        if fields.get("document_id") != entry.get("document_id"):
            fail(errors, f"{path}: document_id disagrees with the manifest")
        if not UUID.match(fields.get("document_id", "")):
            fail(errors, f"{path}: document_id is not a UUID")
        try:
            if int(fields.get("version", "")) != int(entry.get("version", -1)):
                fail(errors, f"{path}: version disagrees with the manifest")
        except ValueError:
            fail(errors, f"{path}: version is not an integer")
        if not body.strip():
            fail(errors, f"{path}: body is empty")
        if fields.get("category") == "project-brief":
            for heading in BRIEF_HEADINGS:
                if heading not in body:
                    fail(errors, f"{path}: brief is missing {heading}")
            if "Verify:" not in body:
                fail(errors, f"{path}: brief has no Verify: line")
        for label, text in (("file", content.decode(errors="replace")),):
            found = private_reference(text)
            if found:
                fail(errors, f"{path}: private reference in {label}: {found}")
        if "TODO" in body:
            fail(errors, f"{path}: body contains TODO")

    actual = {
        str(path.relative_to(root.parent)).replace("\\", "/")
        for path in root.rglob("*")
        if path.is_file()
    }
    for path in sorted(actual - listed - {"programme/README.md", "programme/manifest.json"}):
        fail(errors, f"{path}: not in the manifest")
    for path in sorted(listed - actual):
        fail(errors, f"{path}: in the manifest but absent")
    if "TODO" in readme.read_text(errors="replace"):
        fail(errors, "README.md contains TODO")
    found = private_reference(readme.read_text(errors="replace"))
    if found:
        fail(errors, f"README.md: private reference: {found}")
    return errors


def self_test() -> int:
    """A valid tree passes; a hash change, a missing frontmatter and a private ref each refuse."""
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp) / "programme"
        root.mkdir()
        doc = root / "T1" / "project-brief" / "t1-01234567.md"
        doc.parent.mkdir(parents=True)
        body = "# T1 — Plan spine\n\n" + "\n".join(f"{h}\n\nx\n" for h in BRIEF_HEADINGS) + "\nVerify: `x`\n"
        content = (
            "---\n"
            'document_id: "01234567-89ab-7cde-8123-456789abcdef"\n'
            "version: 1\n"
            'title: "T1"\n'
            'project: "T1"\n'
            'unit: "T1"\n'
            'category: "project-brief"\n'
            'published_at: "2026-11-15T00:00:00Z"\n'
            "---\n\n" + body
        )
        doc.write_text(content)
        (root / "README.md").write_text("# The conveyor programme\n")
        manifest = {
            "schema": SCHEMA,
            "generated_at": "2026-11-15T00:00:00Z",
            "files": [
                {
                    "path": "programme/T1/project-brief/t1-01234567.md",
                    "document_id": "01234567-89ab-7cde-8123-456789abcdef",
                    "version": 1,
                    "sha256": hashlib.sha256(content.encode()).hexdigest(),
                }
            ],
        }
        (root / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
        cases = []
        cases.append(("valid", check(root)))
        bad_hash = json.loads(json.dumps(manifest))
        bad_hash["files"][0]["sha256"] = "0" * 64
        (root / "manifest.json").write_text(json.dumps(bad_hash) + "\n")
        cases.append(("hash", check(root)))
        (root / "manifest.json").write_text(json.dumps(manifest) + "\n")
        doc.write_text(content.replace("---\n\n# T1", "---\n\n# T1", 1).replace("## Boundary", "Boundary"))
        cases.append(("missing-heading", check(root)))
        doc.write_text(content.replace("# T1 — Plan spine", "# T1 — Plan spine\n\n10.0.0.1"))
        cases.append(("private-ref", check(root)))
        failures = []
        if cases[0][1]:
            failures.append(f"valid tree refused: {cases[0][1][:2]}")
        for name, errors in cases[1:]:
            if not errors:
                failures.append(f"{name} case was accepted")
        if failures:
            print("check-programme-export self-test FAILED")
            for failure in failures:
                print(" -", failure)
            return 1
        print("check-programme-export self-test: 4/4 refusals correct")
        return 0


def main() -> int:
    args = [arg for arg in sys.argv[1:]]
    if "--self-test" in args:
        return self_test()
    if len(args) != 1:
        print("usage: check-programme-export.py programme/ [--self-test]")
        return 2
    root = Path(args[0])
    if not root.is_dir():
        print(f"check-programme-export: {root} is not a directory")
        return 2
    errors = check(root)
    if errors:
        print(f"check-programme-export: FAILED ({len(errors)})")
        for error in errors[:40]:
            print(" -", error)
        if len(errors) > 40:
            print(f" … {len(errors) - 40} more")
        return 1
    manifest = json.loads((root / "manifest.json").read_text())
    print(
        f"check-programme-export: {len(manifest['files'])} documents, "
        f"generated_at {manifest['generated_at']} — ok"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
