#!/usr/bin/env python3
"""Compose an artefact body from the standard's skeleton and the published snapshot.

`publish-programme-artefacts.py` publishes only externally authored bodies. This factory
authors the body instead: the standard's skeleton supplies the intent paragraph under each
required heading, the snapshot supplies the rows, and an incomplete artefact is refused
with a typed code rather than published with a placeholder. A body with no non-zero fact
under any required heading is refused `NO_EVIDENCE`: a heading-only document is not evidence.

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
import tempfile
from pathlib import Path
from typing import Any, NoReturn

HERE = Path(__file__).resolve().parent
GEN_PATH = HERE / "generate-standards.py"
SCHEMA = "artefact-body/v1"
PLACEHOLDER = re.compile(r"\b(TODO|TBD|PLACEHOLDER|FIXME|LOREM IPSUM)\b", re.IGNORECASE)
UNSUBSTITUTED = re.compile(r"\$subject[A-Za-z0-9_]*")
SOURCE_KINDS = {"rows", "documents", "computed", "register", "absence"}


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


def source_is_self_document(
    source: dict[str, Any],
    template: dict[str, Any],
    subject: dict[str, Any],
    category: str,
    gen,
) -> bool:
    """Does this source cite the artefact's own document category?"""
    if str(source.get("kind") or "") != "documents":
        return False
    own = str(template.get("document_category") or "")
    if not own:
        return False
    wanted = str(
        gen.substitute(
            (source.get("match") or {}).get("category", "$category"),
            subject,
            own or category,
            subject.get("phase"),
        )
    )
    return wanted == own


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
    fact_blocks = 0
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
        # A heading that cites only its own document is a heading without facts — the business
        # case's four headings that published the business case itself. A template whose body is
        # authored directly (project brief, vision, …) declares `document_authored`, and only
        # then may the document carry the heading.
        active = [source for source in declared if not source.get("optional")]
        if (
            active
            and template.get("document_authored") is not True
            and all(
                source_is_self_document(source, template, subject, category, gen)
                for source in active
            )
        ):
            refuse(
                "SELF_REFERENCE",
                f"required heading {heading!r} cites only its own document "
                f"{template.get('document_category')!r}; declare the template "
                "`document_authored` if the document body carries the facts",
                heading=heading,
                category=str(template.get("document_category") or ""),
            )
        absences: list[str] = []
        fallback_absences: list[str] = []
        for source in declared:
            kind = str(source.get("kind") or "")
            if kind not in SOURCE_KINDS:
                abort("UNKNOWN_SOURCE_KIND", f"unknown source kind {kind!r}", heading=heading)
            if source.get("optional"):
                continue
            if kind == "absence":
                record = str(source.get("record") or "").strip()
                if not record:
                    refuse(
                        "EMPTY_ABSENCE",
                        f"required heading {heading!r} declares an absence with no record",
                        heading=heading,
                    )
                (fallback_absences if source.get("fallback") else absences).append(record)
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
            if source.get("optional") or source.get("kind") == "absence":
                continue
            count = source_count(gen, template, category, subject, snapshot, projects, section, source)
            if count <= 0 and not fallback_absences:
                refuse(
                    "EMPTY_POINTER",
                    f"required heading {heading!r} has a source that resolves to zero rows",
                    heading=heading,
                    source=f"{source.get('file') or source.get('name') or source.get('kind')}"
                           f"#{source.get('pointer', '')}",
                )
        body.extend([f"## {heading}", "", intent, ""])
        heading_facts = sum(
            1 for entry in section["sources"] if int(entry.get("count") or 0) > 0
        )
        for record in absences + (fallback_absences if heading_facts == 0 else []):
            body.extend([f"_Explicit absence — {record}._", ""])
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
            fact_blocks += 1
        headings.append({
            "heading": heading,
            "required": True,
            "status": str(section.get("status") or ""),
            "sources": [source_summary(entry, category) for entry in section["sources"]],
        })
    if fact_blocks == 0:
        # Every declared source resolved to zero rows. The skeleton's intent paragraphs are not
        # evidence: publishing this body would put a heading-only document into the record and
        # the artefact would still be a gap. Refuse, and let the missing row be the finding.
        refuse(
            "NO_EVIDENCE",
            "no required heading resolves to a non-zero fact; headings alone are not evidence",
            category=category, subject=subject_key,
        )
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


def self_test_compose(gen) -> None:
    """The mechanism through `compose()` itself, not only its helpers.

    A helper test leaves the refusal and fallback unproven: deleting the guard or reverting the
    fallback kept the old self-test green. These cases fail when the behaviour changes.
    """
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        snapshot = {
            "manifest": {"git_sha": "0" * 40, "generated_at": "2026-11-20T00:00:00Z"},
            "programme": {
                "programme": [{"id": "pg1", "name": "Fixture", "status": "active"}],
                "projects": [{
                    "id": "p1", "key": "P1", "programme_id": "pg1", "status": "active",
                    "meta": {"measured_status": "active", "expected_outputs": ["vision"]},
                }],
            },
            "artefacts": {"documents": [{
                "id": "d1", "category": "vision", "project_id": "p1",
                "publishable": True, "created_at": "2026-11-20T00:00:00Z", "title": "Vision",
            }]},
            "msp": {
                "register_items": [{"id": "i1", "kind": "issue", "title": "One issue", "status": "open"}],
                "project_baselines": [
                    {"id": "b1", "project_id": "p1", "kind": "scope", "approved_by": "u1", "approved_by_name": "a"},
                    {"id": "b2", "project_id": "p1", "kind": "schedule", "approved_by": "u1", "approved_by_name": "a"},
                    {"id": "b3", "project_id": "p1", "kind": "cost", "approved_by": "u1", "approved_by_name": "a"},
                ],
            },
            "work": {"approvals": [
                {"id": "ap1", "project_id": "p1", "gate": "baseline", "status": "approved"},
            ]},
        }
        for name, document in snapshot.items():
            (root / f"{name}.json").write_text(json.dumps(document), encoding="utf-8")
        (root / "output-d1.html").write_text(
            "<h1>Vision</h1><h2>Purpose</h2>", encoding="utf-8"
        )
        standard = {"templates": {
            "probe": {
                "required": ["Alpha", "Beta"],
                "skeleton": "# Probe\n\n## Alpha\n\nA.\n\n## Beta\n\nB.\n",
                "standard": "MSP", "level": "programme", "subject_kind": "programme",
                "mandatory": True, "spec": "§8.3", "document_category": "probe",
                "sources": {
                    "Alpha": [{"kind": "rows", "file": "msp.json",
                               "pointer": "/register_items", "match": {"kind": "issue"}}],
                    "Beta": [{"kind": "absence", "record": "the beta record"}],
                },
            },
            "project-baseline": {
                "required": ["Scope", "Schedule", "Cost", "Approval"],
                "skeleton": (
                    "# Project Baseline\n\n## Scope\n\nS.\n\n## Schedule\n\nS.\n\n"
                    "## Cost\n\nC.\n\n## Approval\n\nA.\n"
                ),
                "standard": "PMP", "level": "project", "subject_kind": "project",
                "mandatory": True, "spec": "§8.4", "document_category": "project-baseline",
                "sources": {
                    "Scope": [{"kind": "rows", "file": "msp.json", "pointer": "/project_baselines",
                               "match": {"project_id": "$subject_id", "kind": "scope"}}],
                    "Schedule": [
                        {"kind": "rows", "file": "msp.json", "pointer": "/project_baselines",
                         "match": {"project_id": "$subject_id", "kind": "schedule"}},
                        {"kind": "absence", "fallback": True, "record": "the schedule baseline"},
                    ],
                    "Cost": [
                        {"kind": "rows", "file": "msp.json", "pointer": "/project_baselines",
                         "match": {"project_id": "$subject_id", "kind": "cost"}},
                        {"kind": "absence", "fallback": True, "record": "the cost baseline"},
                    ],
                    "Approval": [{"kind": "rows", "file": "work.json", "pointer": "/approvals",
                                  "match": {"project_id": "$subject_id", "gate": "baseline"}}],
                },
            },
        }}
        summary = compose(gen, standard, snapshot, root, "probe", "programme", "pg1")
        assert "_Explicit absence — the beta record._" in summary["body"], summary["body"]
        assert "One issue" in summary["body"], summary["body"]
        standard["templates"]["probe"]["sources"] = {
            "Alpha": [{"kind": "absence", "record": "alpha"}],
            "Beta": [{"kind": "absence", "record": "beta"}],
        }
        try:
            compose(gen, standard, snapshot, root, "probe", "programme", "pg1")
        except Refusal as exc:
            assert exc.code == "NO_EVIDENCE", exc.code
        else:
            raise AssertionError("an all-absence body must refuse NO_EVIDENCE")
        # A self-document heading is refused unless the template declares itself authored. The
        # fixture's d1 vision page carries `Purpose`, so the flagged template composes and the
        # unflagged one must refuse: the mutant that deletes the guard dies here.
        vision = {
            "required": ["Purpose"],
            "skeleton": "# Vision\n\n## Purpose\n\nP.\n",
            "standard": "MSP", "level": "project", "subject_kind": "project",
            "mandatory": True, "spec": "§8.3", "document_category": "vision",
            "sources": {"Purpose": [{
                "kind": "documents", "file": "artefacts.json", "pointer": "/documents",
                "match": {"category": "vision"},
            }]},
        }
        standard["templates"]["vision"] = dict(vision)
        try:
            compose(gen, standard, snapshot, root, "vision", "project", "p1")
        except Refusal as exc:
            assert exc.code == "SELF_REFERENCE", exc.code
        else:
            raise AssertionError("an authored template must declare document_authored")
        standard["templates"]["vision"]["document_authored"] = True
        summary = compose(gen, standard, snapshot, root, "vision", "project", "p1")
        assert "## Purpose" in summary["body"], summary["body"]
        def section_text(body: str, heading: str) -> str:
            parts: list[str] = []
            current = ""
            for line in body.splitlines():
                if line.startswith("## "):
                    current = line[3:].strip()
                elif current == heading:
                    parts.append(line)
            return "\n".join(parts)

        body = compose(gen, standard, snapshot, root, "project-baseline", "project", "p1")["body"]
        scope = section_text(body, "Scope")
        schedule = section_text(body, "Schedule")
        cost = section_text(body, "Cost")
        approval = section_text(body, "Approval")
        assert '"b1"' in scope and '"b2"' not in scope and '"b3"' not in scope, scope
        assert '"b2"' in schedule and '"b1"' not in schedule, schedule
        assert '"b3"' in cost and '"b1"' not in cost, cost
        assert '"ap1"' in approval, approval
        snapshot["msp"]["project_baselines"] = [
            row for row in snapshot["msp"]["project_baselines"] if row["kind"] != "schedule"
        ]
        body = compose(gen, standard, snapshot, root, "project-baseline", "project", "p1")["body"]
        assert "_Explicit absence — the schedule baseline._" in body, body
        assert "b1" in body and "b3" in body, body
    sys.stdout.write("PASS self-test compose\n")


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
    self_test_compose(gen)
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
