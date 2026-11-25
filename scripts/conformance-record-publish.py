#!/usr/bin/env python3
"""conformance-record-publish — publish the programme's missing declared outputs.

Every closed project must publish its declared `closure-review`, and every project its declared
`evidence`/`project-brief` (the conformance audit's `closed_without_closure_review` and
`unpublished_declared_output` rules). This script composes each repo document as a text output
(`documents.compose`), publishes it (`documents.publish`, an admin act with If-Match) and reads it
back through the deployed engine's action surface — never SQL. A published output whose body hash
already matches is skipped, so a re-run writes nothing. The T19F evidence/closure-review documents
are updated in place (they are already published or are new).

Transport: `scripts/programme-record.py` (CONVEYOR_SSH_KEY, CONVEYOR_HOST, TENANT), the deployed
engine — documents are byte-backed through MinIO, which only the pod reaches.

Usage:
    python3 scripts/conformance-record-publish.py --apply
    python3 scripts/conformance-record-publish.py --check
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RECORDER = ROOT / "scripts" / "programme-record.py"
SNAPSHOT = ROOT / "docs" / "design" / "data" / "programme.json"
EVIDENCE = ROOT / "docs" / "evidence" / "conformance"

# (project key, category, source path, title). Order is the audit's: closure records first.
DOCUMENTS = [
    ("T5", "closure-review", "docs/plans/T5/closure-review.md", "Closure review — T5 Kata"),
    ("T5", "evidence", "docs/plans/T5/evidence.md", "Evidence — T5 Kata"),
    ("T6", "closure-review", "docs/plans/T6/closure-review.md", "Closure review — T6 Development project"),
    ("T6", "evidence", "docs/plans/T6/evidence.md", "Evidence — T6 Development project"),
    ("T8", "closure-review", "docs/plans/T8/closure-review.md", "Closure review — T8 Organisational memory"),
    ("T8", "project-brief", "docs/plans/T8/project-brief.md", "Project brief — T8 Organisational memory"),
    ("T12", "evidence", "docs/plans/T12/evidence.md", "Evidence — T12 One organisation loop"),
    ("T15", "closure-review", "docs/plans/T15/closure-review.md", "Closure review — T15 Tool authoring and BYO"),
    ("T15", "evidence", "docs/plans/T15/evidence.md", "Evidence — T15 Tool authoring and BYO"),
    ("T17", "closure-review", "docs/plans/T17/closure-review.md", "Closure review — T17 Tenant asset authoring"),
    ("T17", "evidence", "docs/plans/T17/evidence.md", "Evidence — T17 Tenant asset authoring"),
]

# Already-published documents whose bytes moved in the close-out: update in place.
REFRESH = [
    ("T19F", "evidence", "docs/plans/T19F/evidence.md", "Evidence — T19F Forum bindings"),
    ("T19F", "closure-review", "docs/plans/T19F/closure-review.md", "Closure review — T19F Forum bindings"),
]


def run(args: list[str]) -> tuple[int, str, str]:
    completed = subprocess.run(args, capture_output=True, text=True)
    return completed.returncode, completed.stdout, completed.stderr


def parse_sections(text: str) -> dict[str, dict]:
    parts: dict[str, dict] = {}
    current: str | None = None
    buffer: list[str] = []
    for line in text.splitlines():
        if line.startswith("== ") and line.endswith(" =="):
            if current is not None and "\n".join(buffer).strip():
                parts[current] = json.loads("\n".join(buffer))
            current = line[3:-3]
            buffer = []
        else:
            buffer.append(line)
    if current is not None and "\n".join(buffer).strip():
        parts[current] = json.loads("\n".join(buffer))
    return parts


def read(action: str, payload: dict) -> dict:
    code, out, err = run(
        [sys.executable, str(RECORDER), "--action", action, "--input", json.dumps(payload)]
    )
    if code != 0:
        raise SystemExit(f"{action}: recorder exited {code}: {err or out}")
    sections = parse_sections(out)
    return next(iter(sections.values()), {})


def write(action: str, payload: dict, *, name: str, header: str,
          read_action: str, read_input: dict, if_match: int | None = None) -> tuple[dict, dict]:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    path = EVIDENCE / name
    if not path.exists():
        path.write_text(header, encoding="utf-8")
    args = [
        sys.executable, str(RECORDER), "--action", action,
        "--input", json.dumps(payload), "--evidence", str(path),
        "--read-action", read_action, "--read-input", json.dumps(read_input),
    ]
    if if_match is not None:
        args += ["--if-match", str(if_match)]
    code, out, err = run(args)
    with path.open("a", encoding="utf-8") as handle:
        handle.write("\n```\n" + (out or err).rstrip() + "\n```\n")
    if code != 0:
        raise SystemExit(f"{action}: recorder exited {code}: {err or out}")
    sections = parse_sections(out)
    return sections.get(f"write: {action}", {}), sections.get(f"read-back: {read_action}", {})


def project_ids() -> dict[str, str]:
    if not SNAPSHOT.exists():
        raise SystemExit(f"{SNAPSHOT} is missing; run publish-data.sh first")
    snapshot = json.loads(SNAPSHOT.read_text(encoding="utf-8"))
    return {row["key"]: row["id"] for row in snapshot.get("projects", []) if row.get("id")}


def published_document(project_id: str, category: str) -> dict | None:
    items = read("documents.list", {"project_id": project_id, "category": category}).get("items", [])
    return next((row for row in items if row.get("published_at")), None)


def ensure(project_id: str, key: str, category: str, source: str, title: str) -> str:
    path = ROOT / source
    body = path.read_text(encoding="utf-8")
    sha = hashlib.sha256(body.encode()).hexdigest()
    items = read(
        "documents.list", {"project_id": project_id, "category": category}
    ).get("items", [])
    # A body that already exists — published or a composed draft from an interrupted run — is
    # reused; only an unpublished matching row is published.
    matching = next((row for row in items if row.get("content_hash") == sha), None)
    if matching is not None and matching.get("published_at"):
        print(f"SKIP  {key} {category}: published body matches")
        return "skip"
    if matching is not None:
        written, _ = write(
            "documents.publish",
            {"id": matching["id"], "publish": True},
            name=f"{category}-{key}-publish.md",
            header=(
                f"# {key} {category} — publish the composed draft\n\n"
                f"`documents.publish` for `{matching['id']}` (an interrupted earlier run left the "
                f"body composed but unpublished).\n"
            ),
            read_action="documents.get",
            read_input={"id": matching["id"]},
            if_match=int(matching.get("version") or 0) or None,
        )
        if not written.get("published_at"):
            raise SystemExit(f"{key} {category}: the draft did not read back published")
        print(f"PUBLISH {key} {category}: existing draft {matching['id']}")
        return "publish"
    # A published row, or a same-title draft left by an interrupted run, is updated in place —
    # never duplicated.
    existing = next((row for row in items if row.get("published_at")), None) or next(
        (row for row in items if row.get("title") == title), None
    )
    if existing is not None:
        full = read("documents.get", {"id": existing["id"]})
        full = full.get("document", full)
        if full.get("content_hash") == sha:
            print(f"SKIP  {key} {category}: published body matches")
            return "skip"
        updated, _ = write(
            "documents.update",
            {"id": existing["id"], "title": title, "body": body},
            name=f"{category}-{key}.md",
            header=(
                f"# {key} {category} — the published update\n\n"
                f"`documents.update` on the published output `{existing['id']}` "
                f"({source}) so the public record carries the measured bytes.\n"
            ),
            read_action="documents.get",
            read_input={"id": existing["id"]},
            if_match=int(full.get("version") or 0) or None,
        )
        if not updated.get("published_at"):
            _, _ = write(
                "documents.publish",
                {"id": existing["id"], "publish": True},
                name=f"{category}-{key}-publish.md",
                header=f"# {key} {category} — publish the update\n\n`documents.publish`.\n",
                read_action="documents.get",
                read_input={"id": existing["id"]},
                if_match=int(updated.get("version") or 0) or None,
            )
        print(f"UPDATE {key} {category}: output {existing['id']}")
        return "update"
    composed, _ = write(
        "documents.compose",
        {"title": title, "project_id": project_id, "body": body,
         "category": category, "unit": key},
        name=f"{category}-{key}.md",
        header=(
            f"# {key} {category} — the published output\n\n"
            f"`documents.compose` of `{source}` (category `{category}`, unit `{key}`), then "
            f"`documents.publish`; the audit's declared-output rule reads this row.\n"
        ),
        read_action="documents.list",
        read_input={"project_id": project_id, "category": category},
    )
    document_id = composed["id"]
    published, _ = write(
        "documents.publish",
        {"id": document_id, "publish": True},
        name=f"{category}-{key}-publish.md",
        header=(
            f"# {key} {category} — publish\n\n"
            f"`documents.publish` for `{document_id}` (admin act, If-Match). The standard's "
            f"required headings are checked on this transition.\n"
        ),
        read_action="documents.get",
        read_input={"id": document_id},
        if_match=int(composed.get("version") or 0) or None,
    )
    if not published.get("published_at"):
        raise SystemExit(f"{key} {category}: the output did not read back published")
    print(f"CREATE {key} {category}: output {document_id}")
    return "create"


def check() -> int:
    ids = project_ids()
    missing = []
    for key, category, _source, _title in DOCUMENTS + REFRESH:
        project_id = ids.get(key)
        if project_id is None:
            missing.append(f"{key}: no live project id in the snapshot")
            continue
        row = published_document(project_id, category)
        if row is None:
            missing.append(f"{key} {category}: no published output")
    if missing:
        print("\n".join(missing))
        print(f"conformance-record-publish: {len(missing)} missing output(s)")
        return 1
    print("conformance-record-publish: every declared output is published")
    return 0


def apply() -> int:
    ids = project_ids()
    for key, category, source, title in DOCUMENTS + REFRESH:
        project_id = ids.get(key)
        if project_id is None:
            raise SystemExit(f"{key}: no live project id in the snapshot")
        ensure(project_id, key, category, source, title)
    return check()


def main() -> int:
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--apply", action="store_true")
    group.add_argument("--check", action="store_true")
    args = parser.parse_args()
    try:
        return apply() if args.apply else check()
    except RuntimeError as error:
        print(f"FAILED: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
