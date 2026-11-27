#!/usr/bin/env python3
"""Publish every programme project's artefacts, composed from the standard.

`publish-programme-artefacts.py` publishes programme-level artefacts whose bodies are
authored externally.  This tool is the per-project counterpart: it composes each
mandatory project artefact with `compose-artefact-bodies.py` (imported by path, never
copied), decides whether the record already carries it, and publishes the missing or
stale ones through the product's own actions.

Readiness, per programme project x template whose `subject_kind` is `project` and whose
`applies()` holds (generate-standards.py:1209), plus triggered `on_demand` templates:

  present    one published document for (project, category) whose recorded
             ``meta.sha256`` equals the composed ``body_sha256``
  stale      a published document carrying a different hash
  ambiguous  two published documents for the pair carrying different hashes
  due        no published document for the pair

Modes:
  --check     read-only; exit 1 when any due/stale/ambiguous row exists, 0 when all present
  --dry-run   compose every due/stale body and write it under --bodies-out; no actions
  --apply     compose, then for each due pair: documents.compose, documents.publish, then
              trace_links.create (document -> project, kind `satisfies`), each call proven by
              its read action through `programme-record.py`. A stale pair is left in place
              unless --republish-stale is given: it is a published, authored document, not a gap.

Every action payload of an apply is appended to docs/evidence/record-w4/replay.jsonl, one
JSON object per line ({natural_key, action, payload, read_action, read_input,
reads_back_by}), so `promote-record.py` can replay the run into production later.  The w3
journal is never written.  A pair the record already carries is skipped, and a 409
duplicate compose is accepted only when the existing row carries the same body hash and
the trace edge reads back.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Any, NoReturn

ROOT = Path(__file__).resolve().parent.parent
from private_refs import redact  # noqa: E402
COMPOSER_PATH = ROOT / "scripts/compose-artefact-bodies.py"
RECORD = ROOT / "scripts/programme-record.py"
STANDARD_PATH = ROOT / "seed/output-standard.json"
DEFAULT_JOURNAL = ROOT / "docs/evidence/record-w4/replay.jsonl"
DEFAULT_ACTOR = "01a1efd4-c3e5-7b74-b32b-1b15683a8c4e"
SCHEMA = "artefact-publication/v1"
STATES = ("due", "present", "missing-edge", "stale", "ambiguous")
PUBLISHED_STATUSES = {"published", "live", "approved"}
NATURAL_KEY = "artefact:{project}:{category}:{sha16}"


class Refusal(Exception):
    """A refused publication: exit 1, a typed code, and no unverified claim."""

    def __init__(self, code: str, message: str, **context: Any) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.context = context


def refuse(code: str, message: str, **context: Any) -> NoReturn:
    raise Refusal(code, message, **context)


def fail(message: str) -> NoReturn:
    sys.stderr.write(f"publish-artefact-bodies: {message}\n")
    raise SystemExit(2)


def load_composer():
    # The same importlib pattern both neighbours use: a path import of the hyphenated
    # script, so the skeleton/source resolution stays the one authority.
    spec = importlib.util.spec_from_file_location("compose_artefact_bodies", COMPOSER_PATH)
    if spec is None or spec.loader is None:
        fail(f"cannot load {COMPOSER_PATH}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


# ── the snapshot's published documents ──────────────────────────────────────────────────


def is_published(row: dict) -> bool:
    """Whether a document row is a publication of record.

    The product's lifecycle field is named differently across exports; a boolean or a
    timestamp settles it, a status/state word decides when present, and a snapshot row
    with no lifecycle field at all is a publication record (the export is of documents,
    not of drafts).
    """
    for field in ("publish", "published", "is_published"):
        value = row.get(field)
        if isinstance(value, bool):
            return value
    if row.get("published_at"):
        return True
    status = str(row.get("status") or row.get("state") or "").strip().lower()
    if status:
        return status in PUBLISHED_STATUSES
    return True


def document_hash(row: dict) -> str | None:
    meta = row.get("meta")
    if isinstance(meta, str):
        try:
            meta = json.loads(meta)
        except ValueError:
            meta = {}
    if not isinstance(meta, dict):
        meta = {}
    for value in (meta.get("sha256"), row.get("sha256"), meta.get("body_sha256")):
        if value:
            return str(value)
    # The snapshot does not project `document.meta` (a known projection gap, package pending),
    # so the publisher's own natural key is the second source: `artefact:<project>:<category>:
    # <sha16>` carries the first 16 hex of the body hash we wrote.
    unit = str(row.get("unit") or "")
    if unit.startswith("artefact:") and unit.count(":") == 3:
        suffix = unit.rsplit(":", 1)[-1]
        if len(suffix) == 16 and all(ch in "0123456789abcdef" for ch in suffix):
            return suffix
    return None


def hash_matches(stored: str | None, composed_sha: str | None) -> bool:
    if not stored or not composed_sha:
        return False
    return stored == composed_sha or (len(stored) == 16 and composed_sha.startswith(stored))


def snapshot_documents(snapshot: dict) -> list[dict]:
    """Every document row the snapshot carries under a `documents` collection."""
    rows: list[dict] = []
    for stem, document in snapshot.items():
        if isinstance(document, dict):
            candidate = document.get("documents")
            if isinstance(candidate, list):
                rows.extend(row for row in candidate if isinstance(row, dict))
        elif isinstance(document, list) and stem in {"documents", "docs"}:
            rows.extend(row for row in document if isinstance(row, dict))
    return rows


def published_for(documents: list[dict], project_id: str, category: str) -> list[dict]:
    return [
        row for row in documents
        if is_published(row)
        and str(row.get("project_id") or "") == str(project_id)
        and str(row.get("category") or "") == str(category)
    ]


def classify(published: list[dict], composed_sha: str | None) -> str:
    if not published:
        return "due"
    hashes = {value for value in (document_hash(row) for row in published) if value}
    if len(hashes) > 1:
        return "ambiguous"
    if len(hashes) == 1 and hash_matches(next(iter(hashes)), composed_sha):
        return "present"
    return "stale"


def matching_document_id(published: list[dict], composed_sha: str) -> str:
    for row in published:
        if hash_matches(document_hash(row), composed_sha):
            return str(row.get("id") or "")
    return ""


# ── readiness ──────────────────────────────────────────────────────────────────────────


def public_row(row: dict) -> dict:
    return {
        "project_key": row["project_key"],
        "project_id": row["project_id"],
        "category": row["category"],
        "mandatory": row["mandatory"],
        "trigger": row["trigger"],
        "state": row["state"],
    }


def count_states(rows: list[dict]) -> dict:
    counts = {state: 0 for state in STATES}
    for row in rows:
        counts[row["state"]] = counts.get(row["state"], 0) + 1
    return counts


def trigger_of(template: dict) -> Any:
    flag = template.get("on_demand")
    if not flag:
        return None
    return flag if isinstance(flag, str) else True


def readiness(
    gen,
    composer,
    std: dict,
    snapshot: dict,
    snapshot_dir: Path,
    *,
    categories: list[str] | None = None,
    project_keys: set[str] | None = None,
    mandatory_only: bool = False,
    compose_due: bool = True,
    verify_edges: bool = False,
    actor: str = "",
    local_bin: str = "",
) -> list[dict]:
    """One row per (project, applicable category) with its record state.

    With `verify_edges`, a published document whose `document -> project satisfies` trace edge
    does not read back is `missing-edge`, not `present`: the artefact's trace chain is part of
    the record (SPEC_V2 §8.5), and an unpublished link is a gap the check must not hide.
    """
    categories = list(categories or [])
    projects_map = composer.programme_projects(snapshot)
    templates = std.get("templates") or {}
    declared = list(std.get("categories") or [])
    subjects: list[dict] = []
    for row in (snapshot.get("programme") or {}).get("projects") or []:
        if not isinstance(row, dict):
            continue
        key = str(row.get("key") or row.get("id") or "")
        if not key:
            continue
        if project_keys is not None and key not in project_keys and str(row.get("id")) not in project_keys:
            continue
        subject = composer.pick_subject(snapshot, "project", key)
        if subject is not None:
            subjects.append(subject)
    subjects.sort(key=lambda item: item["key"])
    documents = snapshot_documents(snapshot)
    rows: list[dict] = []
    for subject in subjects:
        for category in sorted(templates):
            template = templates[category]
            if str(template.get("subject_kind") or "") != "project":
                continue
            if categories and category not in categories:
                continue
            if declared and category not in declared:
                continue
            trigger = trigger_of(template)
            if mandatory_only and not (bool(template.get("mandatory")) or trigger is not None):
                continue
            if not gen.applies(template, category, subject, snapshot, projects_map):
                continue
            published = published_for(documents, subject["id"], category)
            row: dict[str, Any] = {
                "project_key": subject["key"],
                "project_id": subject["id"],
                "category": category,
                "mandatory": bool(template.get("mandatory")),
                "trigger": trigger,
            }
            if len({document_hash(item) for item in published}) > 1:
                row["state"] = "ambiguous"
                rows.append(row)
                continue
            if not published and not compose_due:
                row["state"] = "due"
                rows.append(row)
                continue
            try:
                summary = composer.compose(
                    gen, std, snapshot, snapshot_dir, category, "project", subject["key"],
                )
            except composer.Refusal as exc:
                row["state"] = "stale" if published else "due"
                row["refusal"] = {"code": exc.code, "message": exc.message}
                rows.append(row)
                continue
            except composer.Abort as exc:
                if exc.code == "NOT_APPLICABLE":
                    continue
                raise
            row["_summary"] = summary
            row["state"] = classify(published, summary["body_sha256"])
            if row["state"] == "present" and verify_edges:
                document_id = matching_document_id(published, summary["body_sha256"])
                row["_document_id"] = document_id
                if not document_id or not trace_edge(
                    subject["id"], document_id, actor=actor, local_bin=local_bin,
                ):
                    row["state"] = "missing-edge"
            rows.append(row)
    rows.sort(key=lambda item: (item["project_key"], item["category"]))
    return rows


# ── the action transport (publish-programme-artefacts.py:220-250) ──────────────────────


def check_action(action: str, payload: dict, *, actor: str, local_bin: str) -> Any:
    command = [
        sys.executable, str(RECORD), "--check",
        "--action", action,
        "--input", json.dumps(payload),
        "--actor-id", actor,
        "--quiet",
    ]
    if local_bin:
        command += ["--local-bin", local_bin]
    completed = subprocess.run(command, capture_output=True, text=True)
    if completed.returncode != 0:
        refuse(
            "READBACK",
            f"{action}: programme-record exited {completed.returncode}: "
            f"{(completed.stderr or completed.stdout).strip()[:400]}",
        )
    try:
        return json.loads(completed.stdout)
    except ValueError:
        refuse("READBACK", f"{action}: transport output is not JSON: {completed.stdout[:300]!r}")


def write_action(
    action: str,
    payload: dict,
    *,
    read_action: str,
    read_input: dict | None,
    evidence: Path,
    actor: str,
    local_bin: str,
    if_match: int | None = None,
    read_id_from_output: str | None = None,
    read_match: tuple[str, ...] = (),
) -> tuple[dict | None, str | None]:
    """One write through programme-record; (output, None) or (None, error text)."""
    command = [
        sys.executable, str(RECORD),
        "--action", action,
        "--input", json.dumps(payload),
        "--read-action", read_action,
        "--evidence", str(evidence),
        "--actor-id", actor,
        "--quiet",
    ]
    if read_input is not None:
        command += ["--read-input", json.dumps(read_input)]
    if if_match is not None:
        command += ["--if-match", str(if_match)]
    if read_id_from_output:
        command += ["--read-id-from-output", read_id_from_output]
    for pair in read_match:
        command += ["--read-match", pair]
    if local_bin:
        command += ["--local-bin", local_bin]
    completed = subprocess.run(command, capture_output=True, text=True)
    if completed.returncode != 0:
        return None, (completed.stderr or "") + (completed.stdout or "")
    try:
        return json.loads(completed.stdout), None
    except ValueError:
        return None, f"{action}: transport output is not JSON: {completed.stdout[:300]!r}"


def is_duplicate(error: str) -> bool:
    text = error.lower()
    return "409" in error or "duplicate" in text or ("natural key" in text and "exists" in text)


def page_items(value: Any) -> list[dict]:
    if isinstance(value, dict):
        for key in ("items", "rows", "results"):
            if isinstance(value.get(key), list):
                return [row for row in value[key] if isinstance(row, dict)]
    if isinstance(value, list):
        return [row for row in value if isinstance(row, dict)]
    return []


def find_existing(
    *, project_id: str, category: str, body_sha: str, actor: str, local_bin: str,
) -> dict | None:
    result = check_action(
        "documents.list", {"project_id": project_id, "category": category},
        actor=actor, local_bin=local_bin,
    )
    matches = [
        row for row in page_items(result)
        if str(row.get("project_id") or "") == str(project_id)
        and str(row.get("category") or "") == str(category)
        and document_hash(row) == body_sha
    ]
    if not matches:
        return None
    matches.sort(key=lambda row: (not is_published(row), str(row.get("id") or "")))
    return matches[0]


def trace_edge(project_id: str, document_id: str, *, actor: str, local_bin: str) -> bool:
    result = check_action(
        "trace_links.list", {"to_type": "project", "to_id": project_id},
        actor=actor, local_bin=local_bin,
    )
    for row in page_items(result):
        if str(row.get("from_id") or "") != document_id:
            continue
        if str(row.get("to_id") or "") != str(project_id):
            continue
        if str(row.get("kind") or "satisfies") != "satisfies":
            continue
        return True
    return False


# ── bodies and the replay journal ─────────────────────────────────────────────────────


def write_summary(bodies_out: Path, project_key: str, category: str, summary: dict) -> Path:
    directory = bodies_out / (re.sub(r"[^A-Za-z0-9._-]+", "-", project_key or "") or "project")
    directory.mkdir(parents=True, exist_ok=True)
    body_path = directory / f"{category}.md"
    body_path.write_text(summary["body"], encoding="utf-8")
    detail = {
        key: summary.get(key) for key in (
            "schema", "category", "standard", "spec", "subject", "title",
            "body_sha256", "skeleton_sha256", "headings",
        )
    }
    (directory / f"{category}.json").write_text(
        json.dumps(detail, indent=2, sort_keys=True) + "\n", encoding="utf-8",
    )
    return body_path


def guard_journal(path: Path) -> None:
    try:
        resolved = path.resolve()
    except OSError:
        resolved = path
    if "record-w3" in resolved.parts:
        fail(f"{path}: the w4 publisher never writes docs/evidence/record-w3")


def journal_line(
    path: Path,
    *,
    natural_key: str,
    action: str,
    payload: dict,
    read_action: str,
    read_input: dict | None,
    reads_back_by: str,
) -> None:
    line = redact(
        {
            "natural_key": natural_key,
            "action": action,
            "payload": payload,
            "read_action": read_action,
            "read_input": read_input,
            "reads_back_by": reads_back_by,
        }
    )
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(line, sort_keys=True) + "\n")


# ── apply ─────────────────────────────────────────────────────────────────────────────


def apply_rows(
    rows: list[dict], *, args: argparse.Namespace, bodies_out: Path, journal_path: Path,
) -> tuple[list[dict], list[dict], list[dict]]:
    ambiguous = [row for row in rows if row["state"] == "ambiguous"]
    if ambiguous:
        refuse(
            "AMBIGUOUS_EXISTING",
            "two published documents carry different hashes for one (project, category)",
            pairs=[f"{row['project_key']}:{row['category']}" for row in ambiguous],
        )
    applied: list[dict] = []
    skipped: list[dict] = []
    refused: list[dict] = []
    actor, local_bin = args.actor_id, args.local_bin
    for row in rows:
        if row["state"] == "missing-edge":
            document_id = str(row.get("_document_id") or "")
            if not document_id:
                refuse(
                    "MISSING_EDGE_NO_DOCUMENT",
                    f"{row['project_key']}:{row['category']}: the published document has no id",
                )
            note = bodies_out / "_edges" / f"{row['project_key']}-{row['category']}.md"
            note.parent.mkdir(parents=True, exist_ok=True)
            note.write_text(
                f"# Trace edge — {row['project_key']} / {row['category']}\n\n"
                f"document {document_id} -> project {row['project_id']} (satisfies)\n",
                encoding="utf-8",
            )
            link = {
                "from_type": "document", "from_id": document_id,
                "to_type": "project", "to_id": row["project_id"], "kind": "satisfies",
            }
            _, error = write_action(
                "trace_links.create", link,
                read_action="trace_links.list",
                read_input={"to_type": "project", "to_id": row["project_id"]},
                read_match=("from_type=document", f"from_id={document_id}", "kind=satisfies"),
                evidence=note, actor=actor, local_bin=local_bin,
            )
            if error is not None and not trace_edge(
                row["project_id"], document_id, actor=actor, local_bin=local_bin,
            ):
                refuse(
                    "TRACE_UNVERIFIED",
                    f"{row['project_key']}:{row['category']}: trace_links.create failed and "
                    f"the edge does not read back: {error.strip()[:400]}",
                )
            journal_line(
                args.journal, natural_key=f"edge:{row['project_key']}:{row['category']}",
                action="trace_links.create", payload=link,
                read_action="trace_links.list",
                read_input={"to_type": "project", "to_id": row["project_id"]},
                reads_back_by=(
                    f"trace_links.list:to_type=project,to_id={row['project_id']}"
                ),
            )
            applied.append({
                "project_key": row["project_key"],
                "project_id": row["project_id"],
                "category": row["category"],
                "document_id": document_id,
                "edge_repaired": True,
            })
            continue
        if row["state"] == "present":
            skipped.append(public_row(row))
            continue
        if row["state"] == "stale" and not getattr(args, "republish_stale", False):
            # A published document with a different body is not a gap: it was authored and
            # accepted. Replacing it with a composed body would lose that content and create a
            # second published row for the same pair. Only an explicit --republish-stale does.
            skipped.append(public_row(row))
            continue
        if row.get("refusal"):
            entry = public_row(row)
            entry["refusal"] = row["refusal"]
            refused.append(entry)
            continue
        summary = row["_summary"]
        body_path = write_summary(bodies_out, row["project_key"], row["category"], summary)
        body_sha = summary["body_sha256"]
        natural_key = NATURAL_KEY.format(
            project=row["project_key"], category=row["category"], sha16=body_sha[:16],
        )
        compose_payload = {
            "title": summary["title"],
            "body": summary["body"],
            "project_id": row["project_id"],
            "category": row["category"],
            "unit": natural_key,
            "publishable": True,
            "meta": {
                "natural_key": natural_key,
                "sha256": body_sha,
                "skeleton_sha256": summary["skeleton_sha256"],
            },
        }
        created, error = write_action(
            "documents.compose", compose_payload,
            read_action="documents.get", read_input=None,
            read_id_from_output="id", evidence=body_path,
            actor=actor, local_bin=local_bin,
        )
        document_id = ""
        version = 1
        publish_needed = True
        if error is None:
            assert created is not None
            document_id = str(created.get("id") or "")
            if not document_id:
                refuse("COMPOSE_NO_ID", f"{natural_key}: documents.compose returned no id: {created}")
            try:
                version = int(created.get("version") or 1)
            except (TypeError, ValueError):
                version = 1
        elif is_duplicate(error):
            existing = find_existing(
                project_id=row["project_id"], category=row["category"], body_sha=body_sha,
                actor=actor, local_bin=local_bin,
            )
            if existing is None or not existing.get("id"):
                refuse(
                    "DUPLICATE_MISMATCH",
                    f"{natural_key}: the duplicate compose names no row carrying this body hash",
                )
            document_id = str(existing["id"])
            try:
                version = int(existing.get("version") or 1)
            except (TypeError, ValueError):
                version = 1
            publish_needed = not is_published(existing)
        else:
            refuse("TRANSPORT", f"documents.compose: {(error or '').strip()[:600]}")
        journal_line(
            journal_path, natural_key=natural_key, action="documents.compose",
            payload=compose_payload, read_action="documents.get", read_input=None,
            reads_back_by=f"documents.get:{document_id}",
        )
        if publish_needed:
            publish_payload = {"id": document_id, "publish": True}
            _, error = write_action(
                "documents.publish", publish_payload,
                read_action="documents.get", read_input={"id": document_id},
                if_match=version, evidence=body_path, actor=actor, local_bin=local_bin,
            )
            if error is not None:
                if not is_duplicate(error):
                    refuse("TRANSPORT", f"documents.publish: {error.strip()[:600]}")
                current = check_action(
                    "documents.get", {"id": document_id}, actor=actor, local_bin=local_bin,
                )
                if not (isinstance(current, dict) and is_published(current)):
                    refuse(
                        "PUBLISH_UNVERIFIED",
                        f"{natural_key}: documents.publish failed and the row does not read "
                        f"back published: {error.strip()[:400]}",
                    )
            journal_line(
                journal_path, natural_key=natural_key, action="documents.publish",
                payload=publish_payload, read_action="documents.get",
                read_input={"id": document_id},
                reads_back_by=f"documents.get:{document_id}",
            )
        link = {
            "from_type": "document",
            "from_id": document_id,
            "to_type": "project",
            "to_id": row["project_id"],
            "kind": "satisfies",
        }
        _, error = write_action(
            "trace_links.create", link,
            read_action="trace_links.list",
            read_input={"to_type": "project", "to_id": row["project_id"]},
            read_match=("from_type=document", f"from_id={document_id}", "kind=satisfies"),
            evidence=body_path, actor=actor, local_bin=local_bin,
        )
        if error is not None and not trace_edge(
            row["project_id"], document_id, actor=actor, local_bin=local_bin,
        ):
            refuse(
                "TRACE_UNVERIFIED",
                f"{natural_key}: trace_links.create failed and the edge does not read back: "
                f"{error.strip()[:400]}",
            )
        journal_line(
            journal_path, natural_key=natural_key, action="trace_links.create",
            payload=link, read_action="trace_links.list",
            read_input={"to_type": "project", "to_id": row["project_id"]},
            reads_back_by=f"trace_links.list:to_type=project,to_id={row['project_id']}",
        )
        applied.append({
            "project_key": row["project_key"],
            "project_id": row["project_id"],
            "category": row["category"],
            "document_id": document_id,
            "natural_key": natural_key,
            "body_sha256": body_sha,
        })
    return applied, skipped, refused


# ── CLI ───────────────────────────────────────────────────────────────────────────────


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Publish each programme project's artefacts from the standard.",
    )
    parser.add_argument("--standard", type=Path, default=STANDARD_PATH)
    parser.add_argument("--snapshot", type=Path, help="the product snapshot directory")
    parser.add_argument("--outputs-dir", type=Path, help="the generator's output directory")
    parser.add_argument("--category", action="append", default=[], help="restrict to this key")
    parser.add_argument("--project", help="one programme project key or id")
    parser.add_argument("--all-projects", action="store_true",
                        help="every project, mandatory and triggered on_demand templates")
    parser.add_argument("--all", action="store_true",
                        help="every project, every applicable project template")
    parser.add_argument("--check", action="store_true", help="read-only readiness report")
    parser.add_argument("--dry-run", action="store_true", help="compose bodies; no actions")
    parser.add_argument("--apply", action="store_true", help="publish the due bodies; stale ones only with --republish-stale")
    parser.add_argument(
        "--republish-stale", action="store_true",
        help="also replace a published document whose body hash differs (default: leave it in place)",
    )
    parser.add_argument("--local-bin", default=os.environ.get("PROGRAMME_RECORD_BIN", ""))
    parser.add_argument("--actor-id", default=os.environ.get("PROGRAMME_ACTOR", DEFAULT_ACTOR))
    parser.add_argument("--journal", type=Path, default=DEFAULT_JOURNAL)
    parser.add_argument("--bodies-out", type=Path, help="where composed bodies are written")
    parser.add_argument("--self-test", action="store_true", help="run the offline fixtures")
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    if args.self_test:
        return self_test()
    if args.snapshot is None:
        parser.error("--snapshot DIR is required")
    if args.outputs_dir is None:
        parser.error("--outputs-dir DIR is required")
    if not (args.project or args.all_projects or args.all):
        parser.error("one of --project KEY, --all-projects or --all is required")
    modes = [
        flag for flag, enabled in (
            ("--check", args.check), ("--dry-run", args.dry_run), ("--apply", args.apply),
        ) if enabled
    ]
    if len(modes) != 1:
        parser.error("exactly one of --check, --dry-run or --apply is required")
    if not args.check and args.bodies_out is None:
        parser.error("--dry-run/--apply require --bodies-out DIR")

    composer = load_composer()
    gen = composer.load_generator()
    try:
        std = json.loads(args.standard.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        fail(f"--standard {args.standard}: {error}")
    try:
        snapshot = composer.load_snapshot(args.snapshot)
    except composer.Abort as error:
        fail(f"--snapshot {args.snapshot}: {error.code}: {error.message}")

    templates = std.get("templates") or {}
    for category in args.category:
        template = templates.get(category)
        if template is None:
            fail(f"--category {category}: not in the standard")
        if str(template.get("subject_kind") or "") != "project":
            fail(f"--category {category}: subject_kind is not project")
    known: set[str] = set()
    for row in (snapshot.get("programme") or {}).get("projects") or []:
        if isinstance(row, dict):
            known.add(str(row.get("key") or ""))
            known.add(str(row.get("id") or ""))
    if args.project and args.project not in known:
        fail(f"--project {args.project}: no such programme project in the snapshot")

    guard_journal(args.journal)
    verify_edges = bool(args.check or args.apply)
    if verify_edges and not args.local_bin:
        fail("--check/--apply verify the trace edge and need --local-bin (or PROGRAMME_RECORD_BIN)")
    rows = readiness(
        gen, composer, std, snapshot, args.snapshot,
        categories=args.category,
        project_keys={args.project} if args.project else None,
        mandatory_only=bool(args.all_projects),
        compose_due=not args.check,
        verify_edges=verify_edges,
        actor=args.actor_id,
        local_bin=args.local_bin or "",
    )
    counts = count_states(rows)

    if args.check:
        print(json.dumps({
            "schema": SCHEMA,
            "counts": counts,
            "rows": [public_row(row) for row in rows[:20]],
        }, indent=2))
        return 1 if any(
            counts[state] for state in ("due", "stale", "ambiguous", "missing-edge")
        ) else 0

    if args.dry_run:
        assert args.bodies_out is not None
        bodies: list[dict] = []
        refused: list[dict] = []
        for row in rows:
            if row["state"] == "present":
                continue
            if row.get("refusal"):
                entry = public_row(row)
                entry["refusal"] = row["refusal"]
                refused.append(entry)
                continue
            path = write_summary(
                args.bodies_out, row["project_key"], row["category"], row["_summary"],
            )
            bodies.append({
                "project_key": row["project_key"],
                "category": row["category"],
                "state": row["state"],
                "body_sha256": row["_summary"]["body_sha256"],
                "path": str(path),
            })
        print(json.dumps({
            "schema": SCHEMA, "dry_run": True, "counts": counts,
            "bodies": bodies, "refused": refused,
        }, indent=2))
        return 1 if refused else 0

    if not args.local_bin:
        fail("--apply needs --local-bin (or PROGRAMME_RECORD_BIN)")
    assert args.bodies_out is not None
    applied, skipped, refused = apply_rows(
        rows, args=args, bodies_out=args.bodies_out, journal_path=args.journal,
    )
    print(json.dumps({
        "schema": SCHEMA, "applied": applied, "skipped": skipped,
        "refused": refused, "counts": counts,
    }, indent=2))
    return 1 if refused else 0


# ── self-test ─────────────────────────────────────────────────────────────────────────

STUB_SOURCE = '''#!/usr/bin/env python3
import json
import os
import sys

argv = sys.argv[1:]


def opt(name):
    return argv[argv.index(name) + 1] if name in argv else None


log_path = os.environ.get("STUB_LOG")
if log_path:
    with open(log_path, "a", encoding="utf-8") as handle:
        handle.write(json.dumps({
            "action": opt("--action"),
            "check": "--check" in argv,
            "input": json.loads(opt("--input") or "{}"),
            "read_action": opt("--read-action"),
            "read_input": json.loads(opt("--read-input")) if opt("--read-input") else None,
            "read_match": [argv[i + 1] for i, part in enumerate(argv) if part == "--read-match"],
            "if_match": opt("--if-match"),
            "local_bin": opt("--local-bin"),
            "evidence": opt("--evidence"),
        }) + "\\n")

action = opt("--action")
if os.environ.get("STUB_CONFLICT") and action == "documents.compose":
    sys.stderr.write(json.dumps({"code": "409", "message": "duplicate natural key"}) + "\\n")
    raise SystemExit(2)
payload = json.loads(opt("--input") or "{}")
if action == "documents.compose":
    print(json.dumps({"id": "doc-new", "version": 1, "meta": payload.get("meta")}))
elif action == "documents.publish":
    print(json.dumps({"id": payload.get("id"), "version": 2, "status": "published"}))
elif action == "documents.get":
    print(json.dumps({"id": payload.get("id"), "version": 2, "status": "published"}))
elif action == "documents.list":
    docs = json.loads(os.environ.get("STUB_DOCS") or "[]")
    project = payload.get("project_id")
    category = payload.get("category")
    rows = [
        row for row in docs
        if row.get("project_id") == project and row.get("category") == category
    ]
    print(json.dumps({"items": rows}))
elif action == "trace_links.list":
    if os.environ.get("STUB_NO_EDGES"):
        print(json.dumps({"items": []}))
    else:
        edges = json.loads(os.environ.get("STUB_EDGES") or "{}")
        from_id = edges.get(str(payload.get("to_id")) or "") or "doc-new"
        print(json.dumps({"items": [{
            "id": "edge-1", "from_type": "document", "from_id": from_id,
            "to_type": "project", "to_id": payload.get("to_id"), "kind": "satisfies",
        }]}))
elif action == "trace_links.create":
    print(json.dumps({"id": "edge-1"}))
else:
    print(json.dumps({"ok": True}))
'''


def self_test() -> int:
    import contextlib
    import io
    import tempfile

    composer = load_composer()
    gen = composer.load_generator()
    base = Path(tempfile.mkdtemp(prefix="publish-artefact-selftest-"))
    failures: list[str] = []

    def expect(name: str, condition: bool, detail: Any = "") -> None:
        if condition:
            sys.stdout.write(f"PASS self-test {name}\n")
        else:
            failures.append(name)
            sys.stdout.write(f"FAIL self-test {name}: {detail}\n")

    def run_cli(argv: list[str]):
        out, err = io.StringIO(), io.StringIO()
        code = None
        refusal = None
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            try:
                code = main(argv)
            except Refusal as exc:
                refusal = exc
                err.write(json.dumps(
                    {"code": exc.code, "message": exc.message}, sort_keys=True,
                ) + "\n")
        return code, out.getvalue(), err.getvalue(), refusal

    def template(mandatory: bool, trigger: str | None = None) -> dict:
        data = {
            "standard": "FIXTURE", "level": "project", "subject_kind": "project",
            "mandatory": mandatory, "spec": "SPEC-fixture",
            "required": ["Scope"], "optional": [],
            "sources": {"Scope": [{
                "kind": "rows", "file": "programme.json", "pointer": "/projects",
            }]},
            "skeleton": (
                "# Fixture artefact\n\n## Scope\n\n"
                "The people and systems the work touches.\n"
            ),
        }
        if trigger is not None:
            data["on_demand"] = trigger
        return data

    standard = {
        "version": 1, "categories": ["A", "B", "D"], "aliases": {}, "classes": [],
        "templates": {
            "A": template(True),
            "B": template(False, "closure"),
            "D": template(True),
        },
    }
    standard_path = base / "standard.json"
    standard_path.write_text(json.dumps(standard, indent=2), encoding="utf-8")

    snapshot_dir = base / "snapshot"
    snapshot_dir.mkdir()
    (snapshot_dir / "manifest.json").write_text(json.dumps({
        "git_sha": "0" * 40, "generated_at": "2026-11-23T00:00:00Z",
    }), encoding="utf-8")
    (snapshot_dir / "programme.json").write_text(json.dumps({
        "programme": [{"id": "prog-1", "key": "PROG", "title": "Fixture Programme"}],
        "projects": [
            {"id": "proj-px", "key": "PX", "title": "Fixture Project X",
             "programme_id": "prog-1", "status": "active",
             "meta": {"expected_outputs": ["A", "B"]}},
            {"id": "proj-py", "key": "PY", "title": "Fixture Project Y",
             "programme_id": "prog-1", "status": "active",
             "meta": {"expected_outputs": ["A", "D"]}},
        ],
    }), encoding="utf-8")
    (snapshot_dir / "msp.json").write_text(json.dumps({"items": []}), encoding="utf-8")
    snapshot = composer.load_snapshot(snapshot_dir)
    px_a = composer.compose(gen, standard, snapshot, snapshot_dir, "A", "project", "PX")
    py_a = composer.compose(gen, standard, snapshot, snapshot_dir, "A", "project", "PY")

    def write_documents(rows: list[dict]) -> None:
        (snapshot_dir / "documents.json").write_text(
            json.dumps({"documents": rows}, indent=2), encoding="utf-8",
        )

    baseline_documents = [
        {"id": "doc-px-a", "project_id": "proj-px", "category": "A", "title": "A",
         "status": "published", "version": 3,
         "meta": {"sha256": px_a["body_sha256"]}},
        {"id": "doc-py-a", "project_id": "proj-py", "category": "A", "title": "A",
         "status": "published", "version": 1, "meta": {"sha256": "ab" * 32}},
        {"id": "doc-px-b1", "project_id": "proj-px", "category": "B", "title": "B one",
         "status": "published", "version": 1, "meta": {"sha256": "00" * 32}},
        {"id": "doc-px-b2", "project_id": "proj-px", "category": "B", "title": "B two",
         "status": "published", "version": 1, "meta": {"sha256": "11" * 32}},
    ]
    write_documents(baseline_documents)
    snapshot = composer.load_snapshot(snapshot_dir)

    stub = base / "record-stub.py"
    stub.write_text(STUB_SOURCE, encoding="utf-8")
    global RECORD
    original_record = RECORD
    RECORD = str(stub)
    out_root = base / "outputs"
    out_root.mkdir()
    journal = base / "replay.jsonl"
    log = base / "calls.jsonl"
    os.environ["STUB_LOG"] = str(log)
    os.environ["STUB_EDGES"] = json.dumps({"proj-px": "doc-px-a", "proj-py": "doc-py-a"})

    def common(extra: list[str]) -> list[str]:
        return [
            "--standard", str(standard_path),
            "--snapshot", str(snapshot_dir),
            "--outputs-dir", str(out_root),
            "--local-bin", "/bin/true",
            "--actor-id", "00000000-0000-0000-0000-000000000001",
            "--journal", str(journal),
        ] + extra

    def calls() -> list[dict]:
        if not log.exists():
            return []
        return [
            json.loads(line) for line in log.read_text(encoding="utf-8").splitlines()
            if line.strip()
        ]

    def reset_log() -> None:
        if log.exists():
            log.unlink()

    rows = readiness(
        gen, composer, standard, snapshot, snapshot_dir,
        categories=[], project_keys=None, mandatory_only=True, compose_due=False,
    )
    by_pair = {(row["project_key"], row["category"]): row for row in rows}
    expect("classification-present",
           by_pair.get(("PX", "A"), {}).get("state") == "present", by_pair.get(("PX", "A")))
    expect("classification-stale",
           by_pair.get(("PY", "A"), {}).get("state") == "stale", by_pair.get(("PY", "A")))
    expect("classification-ambiguous",
           by_pair.get(("PX", "B"), {}).get("state") == "ambiguous", by_pair.get(("PX", "B")))
    expect("classification-due",
           by_pair.get(("PY", "D"), {}).get("state") == "due", by_pair.get(("PY", "D")))
    expect("on-demand-trigger",
           by_pair.get(("PX", "B"), {}).get("trigger") == "closure"
           and by_pair[("PX", "B")]["mandatory"] is False, by_pair.get(("PX", "B")))

    reset_log()
    check_journal = base / "check-journal.jsonl"
    code, stdout, stderr, _ = run_cli(
        common(["--all-projects", "--check", "--journal", str(check_journal)]),
    )
    payload = json.loads(stdout) if stdout.strip().startswith("{") else {}
    expect("check-counts",
           payload.get("counts", {}).get("present") == 1
           and payload.get("counts", {}).get("stale") == 1
           and payload.get("counts", {}).get("ambiguous") == 1
           and payload.get("counts", {}).get("due", 0) >= 1,
           payload.get("counts"))
    expect("check-exit", code == 1, code)
    writes = [call["action"] for call in calls() if call["action"] != "trace_links.list"]
    expect("check-read-only", not check_journal.exists() and not writes, stderr[-300:])

    reset_log()
    _, _, stderr, refusal = run_cli(common([
        "--project", "PX", "--category", "B", "--apply",
        "--bodies-out", str(base / "bodies-ambiguous"),
    ]))
    expect("ambiguity-refusal",
           refusal is not None and refusal.code == "AMBIGUOUS_EXISTING" and not log.exists(),
           (refusal.code if refusal else None, stderr[-300:]))

    reset_log()
    dry_bodies = base / "bodies-dry"
    code, _, stderr, _ = run_cli(common([
        "--project", "PY", "--category", "A", "--dry-run", "--bodies-out", str(dry_bodies),
    ]))
    expect("dry-run-bodies",
           code == 0 and (dry_bodies / "PY" / "A.md").exists() and not log.exists(),
           (code, stderr[-300:]))
    expect("dry-run-body-content",
           (dry_bodies / "PY" / "A.md").read_text(encoding="utf-8") == py_a["body"]
           if (dry_bodies / "PY" / "A.md").exists() else False)

    reset_log()
    if journal.exists():
        journal.unlink()
    default_bodies = base / "bodies-default"
    code, stdout, stderr, _ = run_cli(common([
        "--project", "PY", "--category", "A", "--apply", "--bodies-out", str(default_bodies),
    ]))
    expect("stale-left-in-place-by-default",
           code == 0 and not calls()
           and json.loads(stdout).get("skipped", [{}])[0].get("state") == "stale",
           (code, stderr[-300:], [c["action"] for c in calls()]))

    reset_log()
    if journal.exists():
        journal.unlink()
    bodies = base / "bodies-apply"
    code, _, stderr, _ = run_cli(common([
        "--project", "PY", "--category", "A", "--apply", "--republish-stale",
        "--bodies-out", str(bodies),
    ]))
    recorded = calls()
    natural_key = f"artefact:PY:A:{py_a['body_sha256'][:16]}"
    expect("apply-exit", code == 0, (code, stderr[-400:]))
    expect("apply-three-calls",
           [call["action"] for call in recorded]
           == ["documents.compose", "documents.publish", "trace_links.create"],
           [call["action"] for call in recorded])
    expected_compose = {
        "title": py_a["title"],
        "body": py_a["body"],
        "project_id": "proj-py",
        "category": "A",
        "unit": natural_key,
        "publishable": True,
        "meta": {
            "natural_key": natural_key,
            "sha256": py_a["body_sha256"],
            "skeleton_sha256": py_a["skeleton_sha256"],
        },
    }
    expect("apply-compose-payload",
           len(recorded) > 0 and recorded[0]["input"] == expected_compose,
           recorded[0]["input"] if recorded else None)
    expect("apply-publish-payload",
           len(recorded) > 1 and recorded[1]["input"] == {"id": "doc-new", "publish": True}
           and recorded[1]["read_input"] == {"id": "doc-new"}
           and recorded[1]["if_match"] == "1",
           recorded[1] if len(recorded) > 1 else None)
    expect("apply-trace-payload",
           len(recorded) > 2 and recorded[2]["input"] == {
               "from_type": "document", "from_id": "doc-new",
               "to_type": "project", "to_id": "proj-py", "kind": "satisfies",
           }
           and recorded[2]["read_action"] == "trace_links.list"
           and recorded[2]["read_input"] == {"to_type": "project", "to_id": "proj-py"},
           recorded[2] if len(recorded) > 2 else None)
    journal_lines = [
        json.loads(line) for line in journal.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ] if journal.exists() else []
    expected_keys = {
        "natural_key", "action", "payload", "read_action", "read_input", "reads_back_by",
    }
    expect("journal-shape",
           len(journal_lines) == 3
           and all(set(line) == expected_keys for line in journal_lines)
           and [line["action"] for line in journal_lines]
           == ["documents.compose", "documents.publish", "trace_links.create"]
           and all(line["natural_key"] == natural_key for line in journal_lines)
           and all(line["reads_back_by"] for line in journal_lines),
           journal_lines)

    reset_log()
    skip_documents = [dict(row, meta=dict(row.get("meta") or {})) for row in baseline_documents]
    for row in skip_documents:
        if row["id"] == "doc-py-a":
            row["meta"] = {"sha256": py_a["body_sha256"]}
    write_documents(skip_documents)
    before = len([
        line for line in journal.read_text(encoding="utf-8").splitlines() if line.strip()
    ]) if journal.exists() else 0
    code, stdout, stderr, _ = run_cli(common([
        "--project", "PY", "--category", "A", "--apply", "--bodies-out", str(bodies),
    ]))
    payload = json.loads(stdout) if stdout.strip().startswith("{") else {}
    after = len([
        line for line in journal.read_text(encoding="utf-8").splitlines() if line.strip()
    ]) if journal.exists() else 0
    writes = [call["action"] for call in calls() if call["action"] != "trace_links.list"]
    expect("skip-present",
           code == 0 and not writes and payload.get("applied") == [] and after == before,
           (code, [call["action"] for call in calls()], stderr[-300:]))

    reset_log()
    os.environ["STUB_NO_EDGES"] = "1"
    code, stdout, stderr, _ = run_cli(common([
        "--project", "PY", "--category", "A", "--check",
    ]))
    payload = json.loads(stdout) if stdout.strip().startswith("{") else {}
    writes = [call["action"] for call in calls() if call["action"] != "trace_links.list"]
    expect("check-missing-edge",
           code == 1 and payload.get("counts", {}).get("missing-edge") == 1
           and not writes,
           (code, payload.get("counts"), stderr[-300:]))
    code, stdout, stderr, _ = run_cli(common([
        "--project", "PY", "--category", "A", "--apply", "--bodies-out", str(bodies),
    ]))
    payload = json.loads(stdout) if stdout.strip().startswith("{") else {}
    recorded = calls()
    writes = [call["action"] for call in recorded if call["action"] != "trace_links.list"]
    expect("apply-repairs-edge",
           code == 0
           and writes == ["trace_links.create"]
           and (payload.get("applied") or [{}])[0].get("edge_repaired") is True,
           (code, [call["action"] for call in recorded], stderr[-300:]))
    os.environ.pop("STUB_NO_EDGES", None)

    reset_log()
    write_documents(baseline_documents)
    conflict_journal = base / "conflict-replay.jsonl"
    if conflict_journal.exists():
        conflict_journal.unlink()
    os.environ["STUB_CONFLICT"] = "1"
    os.environ["STUB_DOCS"] = json.dumps([{
        "id": "doc-existing", "project_id": "proj-py", "category": "A", "title": "A",
        "status": "draft", "version": 1, "meta": {"sha256": py_a["body_sha256"]},
    }])
    code, _, stderr, _ = run_cli(common([
        "--project", "PY", "--category", "A", "--apply", "--republish-stale",
        "--bodies-out", str(base / "bodies-conflict"),
        "--journal", str(conflict_journal),
    ]))
    recorded = calls()
    expect("duplicate-409",
           code == 0
           and [call["action"] for call in recorded]
           == ["documents.compose", "documents.list",
               "documents.publish", "trace_links.create"]
           and recorded[2]["input"].get("id") == "doc-existing",
           (code, [call["action"] for call in recorded], stderr[-400:]))
    conflict_lines = [
        json.loads(line)
        for line in conflict_journal.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ] if conflict_journal.exists() else []
    expect("duplicate-journal",
           len(conflict_lines) == 3 and conflict_lines[0]["action"] == "documents.compose",
           conflict_lines)

    RECORD = original_record
    for key in ("STUB_LOG", "STUB_CONFLICT", "STUB_DOCS"):
        os.environ.pop(key, None)
    if failures:
        sys.stdout.write(
            f"SELF-TEST FAILED: {len(failures)} checks: {', '.join(failures)}\n"
        )
        return 1
    sys.stdout.write("PASS self-test\n")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Refusal as exc:
        sys.stderr.write(json.dumps(
            {"code": exc.code, "message": exc.message, **exc.context},
            sort_keys=True, default=str,
        ) + "\n")
        raise SystemExit(1)
