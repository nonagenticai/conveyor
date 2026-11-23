#!/usr/bin/env python3
"""Compose an artefact body from the standard's skeleton and the published snapshot.

`publish-programme-artefacts.py` publishes only externally authored bodies. This factory
authors the body instead: the standard's skeleton supplies the intent paragraph under each
required heading, the snapshot supplies the rows, and an incomplete artefact is refused
with a typed code rather than published with a placeholder.

The generator is imported by path (never copied), so the source resolution the pack pages
use — `resolve_section`, `skeleton_bodies`, `applies`, `rows_for` — stays the one authority.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import re
import sys
from pathlib import Path
from typing import Any, NoReturn

HERE = Path(__file__).resolve().parent
GEN_PATH = HERE / "generate-standards.py"
SCHEMA = "artefact-body/v1"
PLACEHOLDER = re.compile(r"\b(TODO|TBD|PLACEHOLDER|FIXME|LOREM IPSUM)\b", re.IGNORECASE)
UNSUBSTITUTED = re.compile(r"\$subject[A-Za-z0-9_]*")
SOURCE_KINDS = {"rows", "documents", "computed", "register"}


class Refusal(Exception):
    """A refused composition: exit 1, a typed code, and no body written."""

    def __init__(self, code: str, message: str, **context: Any) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.context = context


class Abort(Exception):
    """A usage or environment failure: exit 2, and no body written."""

    def __init__(self, code: str, message: str, **context: Any) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.context = context


def refuse(code: str, message: str, **context: Any) -> NoReturn:
    raise Refusal(code, message, **context)


def abort(code: str, message: str, **context: Any) -> NoReturn:
    raise Abort(code, message, **context)


def load_generator():
    # The same importlib pattern `publish-programme-artefacts.py` uses (lines 83-90): a path
    # import of the hyphenated script, so both callers execute one copy of the resolver.
    spec = importlib.util.spec_from_file_location("generate_standards", GEN_PATH)
    if spec is None or spec.loader is None:
        abort("GENERATOR", f"cannot load {GEN_PATH}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_snapshot(directory: Path) -> dict[str, Any]:
    if not directory.is_dir():
        abort("SNAPSHOT", f"{directory} is not a directory")
    files = {
        path.stem: json.loads(path.read_text(encoding="utf-8"))
        for path in sorted(directory.glob("*.json"))
    }
    if "manifest" not in files:
        abort("SNAPSHOT", f"{directory} is not a product snapshot: manifest.json is missing")
    return files


def programme_projects(snapshot: dict[str, Any]) -> dict[str, set[str]]:
    out: dict[str, set[str]] = {}
    for project in (snapshot.get("programme") or {}).get("projects") or []:
        out.setdefault(str(project.get("programme_id")), set()).add(str(project.get("id")))
    return out


def pick_subject(snapshot: dict[str, Any], kind: str, key: str) -> dict[str, Any] | None:
    if kind == "programme":
        rows = (snapshot.get("programme") or {}).get("programme") or []
        row = next(
            (item for item in rows
             if isinstance(item, dict)
             and key in {str(item.get("key") or ""), str(item.get("id") or "")}),
            None,
        )
        if row is None or not row.get("id"):
            return None
        return {
            "kind": "programme",
            "id": str(row["id"]),
            "key": str(row.get("key") or row["id"]),
            "title": str(row.get("title") or row.get("name") or row["id"]),
            "programme_id": str(row["id"]),
            "meta": row,
        }
    if kind == "project":
        rows = (snapshot.get("programme") or {}).get("projects") or []
        row = next(
            (item for item in rows
             if isinstance(item, dict)
             and key in {str(item.get("key") or ""), str(item.get("id") or "")}),
            None,
        )
        if row is None or not row.get("id"):
            return None
        meta = row.get("meta") if isinstance(row.get("meta"), dict) else {}
        return {
            "kind": "project",
            "id": str(row["id"]),
            "key": str(row.get("key") or row["id"]),
            "title": str(row.get("title") or row.get("name") or row["id"]),
            "programme_id": str(row.get("programme_id") or ""),
            "project_id": str(row["id"]),
            "project_key": str(row.get("key") or ""),
            "status": str(row.get("status") or ""),
            "meta": meta,
        }
    return None


def sort_rows(rows: list[Any]) -> list[Any]:
    """Rows sorted by id, or by name when the row has no id, ties broken deterministically."""
    def key(row: Any) -> tuple[str, str]:
        if not isinstance(row, dict):
            return ("", json.dumps(row, sort_keys=True, default=str))
        return (
            str(row.get("id") or row.get("name") or ""),
            json.dumps(row, sort_keys=True, default=str),
        )
    return sorted(rows, key=key)


def source_label(entry: dict[str, Any]) -> str:
    kind = str(entry.get("kind") or "source")
    if kind in {"rows", "documents"} and entry.get("file"):
        return f"{entry['file']}#{entry.get('pointer', '')}"
    if kind == "computed":
        return f"computed:{entry.get('name', '')}"
    return kind


def source_summary(entry: dict[str, Any], category: str) -> dict[str, Any]:
    return {
        "kind": str(entry.get("kind") or ""),
        "file": str(entry.get("file") or entry.get("name") or ""),
        "pointer": str(entry.get("pointer") or ""),
        "category": str(entry.get("category") or category),
        "count": int(entry.get("count") or 0),
    }


def source_count(
    gen,
    template: dict[str, Any],
    category: str,
    subject: dict[str, Any],
    snapshot: dict[str, Any],
    projects: dict[str, set[str]],
    section: dict[str, Any],
    source: dict[str, Any],
) -> int:
    """How many rows/documents this one declared source resolves to, right now."""
    kind = str(source.get("kind") or "")
    if kind == "rows":
        return len(gen.rows_for(snapshot, source, subject, category, subject.get("phase")))
    if kind == "computed":
        name = str(source.get("name") or "")
        for entry in section["sources"]:
            if entry.get("kind") == "computed" and str(entry.get("name") or "") == name:
                return int(entry.get("count") or 0)
        return 0
    if kind == "documents":
        wanted = str(gen.substitute(
            (source.get("match") or {}).get("category", "$category"),
            subject, template.get("document_category") or category, subject.get("phase"),
        ))
        for entry in section["sources"]:
            if entry.get("kind") == "documents" and str(entry.get("category") or "") == wanted:
                return int(entry.get("count") or 0)
        return 0
    # `register` is a declared placeholder the snapshot never carries: zero rows.
    return 0


def check_placeholders(text: str, where: str) -> None:
    match = PLACEHOLDER.search(text) or UNSUBSTITUTED.search(text)
    if match:
        refuse(
            "PLACEHOLDER",
            f"{where} carries the placeholder {match.group(0)!r}",
            where=where,
        )


def check_tokens(text: str, where: str) -> None:
    match = UNSUBSTITUTED.search(text)
    if match:
        refuse(
            "PLACEHOLDER",
            f"{where} carries the unsubstituted token {match.group(0)!r}",
            where=where,
        )


def compose(
    gen,
    std: dict[str, Any],
    snapshot: dict[str, Any],
    snapshot_dir: Path,
    category: str,
    subject_kind: str,
    subject_key: str,
) -> dict[str, Any]:
    templates = std.get("templates") or {}
    categories = list(std.get("categories") or [])
    if category not in templates or (categories and category not in categories):
        abort("UNKNOWN_CATEGORY", f"the standard declares no category {category!r}", category=category)
    template = templates[category]
    subject = pick_subject(snapshot, subject_kind, subject_key)
    if subject is None:
        abort(
            "UNKNOWN_SUBJECT",
            f"the snapshot carries no {subject_kind} with key or id {subject_key!r}",
            subject=subject_key,
        )
    projects = programme_projects(snapshot)
    if not gen.applies(template, category, subject, snapshot, projects):
        abort(
            "NOT_APPLICABLE",
            f"{category!r} does not apply to {subject_kind} {subject_key!r}",
            category=category, subject=subject_key,
        )
    required = list(template.get("required") or [])
    if not required:
        abort("NO_REQUIRED_HEADINGS", f"{category!r} declares no required heading", category=category)
    skeleton = gen.skeleton_bodies(template)
    declared_sources = template.get("sources") or {}
    title = f"{category.replace('-', ' ').title()} — {subject['title']}"
    check_placeholders(title, "title")
    body: list[str] = [f"# {title}", ""]
    headings: list[dict[str, Any]] = []
    for heading in required:
        intent = skeleton.get(gen.normalise(heading))
        if intent is None:
            refuse(
                "MISSING_HEADING",
                f"the skeleton carries no '## {heading}' heading",
                heading=heading,
            )
        check_placeholders(intent, f"skeleton heading {heading!r}")
        declared = declared_sources.get(heading) or []
        if not declared:
            refuse(
                "MISSING_SOURCE",
                f"required heading {heading!r} declares no source",
                heading=heading,
            )
        for source in declared:
            kind = str(source.get("kind") or "")
            if kind not in SOURCE_KINDS:
                abort("UNKNOWN_SOURCE_KIND", f"unknown source kind {kind!r}", heading=heading)
            if source.get("optional"):
                continue
            pointer = str(source.get("pointer") or "")
            if kind == "rows" and pointer.strip() in {"", "/"}:
                refuse(
                    "EMPTY_POINTER",
                    f"required heading {heading!r} names an empty pointer {pointer!r}",
                    heading=heading, source=str(source.get("file") or ""),
                )
        try:
            section = gen.resolve_section(
                heading, template, category, subject, snapshot, snapshot_dir, projects,
                register=None, item_limit=None,
            )
        except Exception as exc:  # GenerationError is defined inside the generator module.
            if type(exc).__name__ == "GenerationError":
                abort("GENERATION_ERROR", str(exc), heading=heading)
            raise
        for source in declared:
            if source.get("optional"):
                continue
            count = source_count(gen, template, category, subject, snapshot, projects, section, source)
            if count <= 0:
                refuse(
                    "EMPTY_POINTER",
                    f"required heading {heading!r} has a source that resolves to zero rows",
                    heading=heading,
                    source=f"{source.get('file') or source.get('name') or source.get('kind')}"
                           f"#{source.get('pointer', '')}",
                )
        body.extend([f"## {heading}", "", intent, ""])
        for entry in section["sources"]:
            rows = entry.get("rows")
            if not isinstance(rows, list):
                items = entry.get("items")
                rows = items if isinstance(items, list) else []
            count = int(entry.get("count") or 0)
            if count <= 0:
                continue
            fact = {
                "source": source_label(entry),
                "category": str(entry.get("category") or category),
                "count": count,
                "rows": sort_rows(rows),
            }
            body.extend(["```json", json.dumps(fact, sort_keys=True, indent=2), "```", ""])
        headings.append({
            "heading": heading,
            "required": True,
            "status": str(section.get("status") or ""),
            "sources": [source_summary(entry, category) for entry in section["sources"]],
        })
    text = "\n".join(body).rstrip("\n") + "\n"
    check_tokens(text, "composed body")
    return {
        "schema": SCHEMA,
        "category": category,
        "standard": str(template.get("standard") or ""),
        "spec": str(template.get("spec") or ""),
        "subject": {
            "kind": subject["kind"], "id": subject["id"],
            "key": subject["key"], "title": subject["title"],
        },
        "title": title,
        "body": text,
        "body_sha256": hashlib.sha256(text.encode("utf-8")).hexdigest(),
        "skeleton_sha256": hashlib.sha256(
            str(template.get("skeleton") or "").encode("utf-8")
        ).hexdigest(),
        "headings": headings,
    }


def self_test() -> None:
    gen = load_generator()
    checks: list[tuple[str, bool]] = []
    bodies = gen.skeleton_bodies({
        "skeleton": "# Artefact\n\n## Alpha\n\nOne line.\n\n## Beta\n\nTwo lines,\njoined.\n",
    })
    checks.append((
        "skeleton_bodies",
        bodies == {"alpha": "One line.", "beta": "Two lines, joined."},
    ))
    ordered = sort_rows([{"name": "b"}, {"id": "z"}, {"name": "a"}, {"id": "a"}])
    checks.append((
        "sort_rows",
        [row.get("id") or row.get("name") for row in ordered] == ["a", "a", "b", "z"],
    ))
    checks.append((
        "placeholders",
        bool(PLACEHOLDER.search("TODO")) and bool(UNSUBSTITUTED.search("$subject_id")),
    ))
    checks.append((
        "source_label",
        source_label({"kind": "rows", "file": "programme.json", "pointer": "/projects"})
        == "programme.json#/projects",
    ))
    for name, ok in checks:
        if not ok:
            sys.stderr.write(f"compose-artefact-bodies self-test: FAIL {name}\n")
            raise SystemExit(1)
        sys.stdout.write(f"PASS self-test {name}\n")
    sys.stdout.write("PASS self-test\n")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Compose an artefact body from the standard's skeleton and the snapshot.",
    )
    parser.add_argument("--standard", help="the standards JSON (e.g. seed/output-standard.json)")
    parser.add_argument("--snapshot", help="the product snapshot directory (manifest.json inside)")
    parser.add_argument("--category", help="the artefact category key")
    parser.add_argument("--subject-kind", choices=["project", "programme"],
                        help="what the artefact is about")
    parser.add_argument("--subject", help="the subject's key or id")
    parser.add_argument("--out", help="write the JSON summary here instead of stdout")
    parser.add_argument("--self-test", action="store_true", help="check the local helpers and exit")
    args = parser.parse_args(argv)
    if args.self_test:
        self_test()
        return 0
    missing = [
        flag for flag, value in (
            ("--standard", args.standard),
            ("--snapshot", args.snapshot),
            ("--category", args.category),
            ("--subject-kind", args.subject_kind),
            ("--subject", args.subject),
        ) if not value
    ]
    if missing:
        abort("USAGE", f"missing required option(s): {', '.join(missing)}")
    standard_path = Path(args.standard)
    if not standard_path.is_file():
        abort("STANDARD", f"{standard_path} does not exist")
    std = json.loads(standard_path.read_text(encoding="utf-8"))
    snapshot_dir = Path(args.snapshot)
    snapshot = load_snapshot(snapshot_dir)
    gen = load_generator()
    summary = compose(
        gen, std, snapshot, snapshot_dir, args.category, args.subject_kind, args.subject,
    )
    payload = json.dumps(summary, sort_keys=True, indent=2) + "\n"
    if args.out:
        Path(args.out).write_text(payload, encoding="utf-8")
    else:
        sys.stdout.write(payload)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Refusal as exc:
        sys.stderr.write(json.dumps(
            {"code": exc.code, "message": exc.message, **exc.context}, sort_keys=True,
        ) + "\n")
        raise SystemExit(1)
    except Abort as exc:
        sys.stderr.write(json.dumps(
            {"code": exc.code, "message": exc.message, **exc.context}, sort_keys=True,
        ) + "\n")
        raise SystemExit(2)
