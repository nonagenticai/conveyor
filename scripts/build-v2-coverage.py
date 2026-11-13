#!/usr/bin/env python3
"""V1 → SPEC_V2 coverage: what exists, what must be extended, what is new.

Reads the authoritative spec tables and the code, and writes:

  * `docs/evidence/v2-coverage.json` — one row per §10.3 table, §10.4 action family and
    §10.5 destination, with the verdict and the evidence the verdict rests on.
  * `docs/evidence/v2-coverage.md` — the same record as a rendered document (the design
    site's Documents destination picks it up through `build-docs.py`).

Verdicts are mechanical where a mechanical answer is honest (a table exists or it does not)
and curated where it is not (a page exists but still renders fixtures). The curated answers
live in `scripts/v2-coverage-overrides.json` and are always shown with their note.

    python3 scripts/build-v2-coverage.py            # write both files
    python3 scripts/build-v2-coverage.py --check    # fail if either is stale
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = ROOT / "docs/SPEC_V2.md"
MIGRATIONS = ROOT / "migrations"
ACTIONS = ROOT / "crates/actions/src"
V2_JS = ROOT / "docs/design/v2.js"
OVERRIDES = ROOT / "scripts/v2-coverage-overrides.json"
OUT_JSON = ROOT / "docs/evidence/v2-coverage.json"
OUT_MD = ROOT / "docs/evidence/v2-coverage.md"

BACKTICK = re.compile(r"`([^`]+)`")


def section(text: str, start: str, end: str) -> str:
    body = text.split(start, 1)
    if len(body) != 2:
        sys.exit(f"build-v2-coverage: section {start!r} not found")
    return body[1].split(end, 1)[0]


def table_rows(body: str) -> list[list[str]]:
    rows = []
    for line in body.splitlines():
        if not line.lstrip().startswith("|"):
            continue
        cells = [cell.strip().replace("\\|", "|") for cell in re.split(r"(?<!\\)\|", line.strip().strip("|"))]
        if len(cells) < 2 or set(cells[0]) <= set("-: "):
            continue
        if cells[0].lower() in ("table", "family", "destination", "#", "seam"):
            continue
        rows.append(cells)
    return rows


def migration_text() -> str:
    return "\n".join(path.read_text() for path in sorted(MIGRATIONS.glob("*.sql")))


def action_names() -> list[str]:
    names = []
    for path in sorted(ACTIONS.rglob("*.rs")):
        names.extend(re.findall(r'name = "([a-z][\w]*\.[\w*.]+)"', path.read_text()))
    return sorted(set(names))


def page_ids() -> set[str]:
    text = V2_JS.read_text()
    body = section(text, "const pages = {", "};")
    return set(re.findall(r"^\s{4}([a-z][\w-]*)\s*[,:]", body, re.M))


def table_verdict(name: str, migrations: str) -> tuple[str, list[str]]:
    created = re.search(rf"CREATE TABLE\s+{re.escape(name)}\b", migrations)
    altered = re.search(rf"ALTER TABLE\s+{re.escape(name)}\b", migrations)
    if created:
        return "exists", [f"migrations: CREATE TABLE {name}"]
    if altered:
        return "extend", [f"migrations: ALTER TABLE {name} (the table exists, the V2 columns do not)"]
    return "new", [f"migrations: no CREATE/ALTER TABLE {name}"]


def action_prefixes(cell: str) -> set[str]:
    """The action families a spec cell names.

    `jobs.create\\|update\\|retire` means `jobs.create`, `jobs.update`, `jobs.retire`: the
    variants after the first inherit the first one's family. Only the family is needed here.
    """
    prefixes = set()
    for token in BACKTICK.findall(cell):
        variants = [part.strip().strip("*") for part in re.split(r"\\?\|", token)]
        for variant in variants:
            if "." not in variant:
                continue
            prefix = variant.split(".", 1)[0]
            if re.fullmatch(r"[a-z][\w]*", prefix):
                prefixes.add(prefix)
    return prefixes


def action_verdict(cell: str, names: list[str]) -> tuple[str, list[str]]:
    prefixes = action_prefixes(cell)
    present = {prefix: [n for n in names if n.startswith(prefix + ".")] for prefix in prefixes}
    found = {prefix: rows for prefix, rows in present.items() if rows}
    evidence = [
        f"actions: {prefix}.* {'found' if rows else 'absent'}"
        + (f" ({', '.join(rows[:3])}{'…' if len(rows) > 3 else ''})" if rows else "")
        for prefix, rows in sorted(present.items())
    ]
    if prefixes and len(found) == len(prefixes):
        return "exists", evidence
    if found:
        return "extend", evidence
    return "new", evidence


DESTINATION_PAGES = {
    "Organisation": "organisation",
    "Procedures": "procedures",
    "Authoring": "authoring",
    "Portfolio": "portfolio",
    "Plan": "plan",
    "Improve": "improve",
    "Operations": "operations",
    "Tools": "toolkits",
}


def load_overrides() -> dict[str, dict]:
    if not OVERRIDES.exists():
        return {}
    document = json.loads(OVERRIDES.read_text())
    return document.get("overrides", {})


def build() -> dict:
    text = SPEC.read_text()
    migrations = migration_text()
    names = action_names()
    pages = page_ids()
    overrides = load_overrides()

    rows: list[dict] = []

    def add(row_id: str, spec_ref: str, kind: str, concept: str, verdict: str, evidence: list[str], note: str = "") -> None:
        override = overrides.get(row_id)
        if override:
            verdict = override["verdict"]
            note = override.get("note", "")
            evidence = evidence + override.get("evidence", [])
        rows.append(
            {
                "id": row_id,
                "spec_ref": spec_ref,
                "kind": kind,
                "concept": concept,
                "verdict": verdict,
                "evidence": evidence,
                **({"note": note} if note else {}),
            }
        )

    data_body = section(text, "### 10.3 Data model additions", "### 10.4 Action families")
    for cells in table_rows(data_body):
        concept = cells[0].replace("*", "").strip()
        names_in_cell = BACKTICK.findall(cells[0])
        for name in names_in_cell:
            verdict, evidence = table_verdict(name, migrations)
            add(f"table:{name}", "§10.3", "table", name, verdict, evidence)

    action_body = section(text, "### 10.4 Action families", "### 10.5 UI additions")
    for cells in table_rows(action_body):
        family = cells[0].replace("*", "").strip()
        verdict, evidence = action_verdict(cells[1], names)
        add(f"actions:{family}", "§10.4", "actions", family, verdict, evidence)

    ui_body = section(text, "### 10.5 UI additions", "### 10.6")
    for cells in table_rows(ui_body):
        destination = cells[0].replace("*", "").strip()
        page = DESTINATION_PAGES.get(destination)
        if page is None:
            continue
        verdict = "exists" if page in pages else "new"
        evidence = [f"docs/design/v2.js: page {page!r} {'registered' if page in pages else 'absent'}"]
        add(f"destination:{destination}", "§10.5", "destination", destination, verdict, evidence)
    # "Tools" is prose in §10.5, not a table row: a Tools screen under Organisation.
    tools_page = "toolkits"
    add(
        "destination:Tools",
        "§10.5",
        "destination",
        "Tools",
        "exists" if tools_page in pages else "new",
        [f"docs/design/v2.js: page {tools_page!r} {'registered' if tools_page in pages else 'absent'}"],
    )

    counts: dict[str, int] = {}
    for row in rows:
        counts[row["verdict"]] = counts.get(row["verdict"], 0) + 1

    return {
        "version": 1,
        "schema": "v2-coverage/v1",
        "generated_from": [
            "docs/SPEC_V2.md",
            "migrations/*.sql",
            "crates/actions/src/**",
            "docs/design/v2.js",
        ],
        "verdicts": {
            "exists": "implemented in code and backed by real rows",
            "extend": "a partial implementation exists (a page, a table, or a prefix) that must be extended",
            "new": "absent",
        },
        "counts": dict(sorted(counts.items())),
        "rows": rows,
    }


def render_markdown(document: dict) -> str:
    lines = [
        "# V1 → SPEC_V2 coverage",
        "",
        "Generated by `scripts/build-v2-coverage.py` from `docs/SPEC_V2.md` §10.3–§10.5 and the",
        "code. The JSON beside this file is the record; this page is its rendered form.",
        "",
        "| Verdict | Meaning | Count |",
        "|---|---|---|",
    ]
    for verdict, meaning in document["verdicts"].items():
        lines.append(f"| `{verdict}` | {meaning} | {document['counts'].get(verdict, 0)} |")
    lines += ["", "## Rows", "", "| Id | Spec | Kind | Concept | Verdict | Evidence |", "|---|---|---|---|---|---|"]
    for row in document["rows"]:
        evidence = "<br>".join(row["evidence"])
        note = f"<br>*{row['note']}*" if row.get("note") else ""
        lines.append(
            f"| `{row['id']}` | {row['spec_ref']} | {row['kind']} | {row['concept']} | "
            f"`{row['verdict']}` | {evidence}{note} |"
        )
    lines.append("")
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="fail if an output is stale")
    args = parser.parse_args()
    document = build()
    rendered = json.dumps(document, indent=2, ensure_ascii=False) + "\n"
    markdown = render_markdown(document)
    if args.check:
        stale = [
            str(path)
            for path, expected in ((OUT_JSON, rendered), (OUT_MD, markdown))
            if not path.exists() or path.read_text() != expected
        ]
        if stale:
            sys.exit(f"build-v2-coverage: stale output(s): {', '.join(stale)}; re-run without --check")
        print("build-v2-coverage: both outputs are current")
        return
    OUT_JSON.write_text(rendered)
    OUT_MD.write_text(markdown)
    print(f"build-v2-coverage: {len(document['rows'])} rows {document['counts']}")


if __name__ == "__main__":
    main()
