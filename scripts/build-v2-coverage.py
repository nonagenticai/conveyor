#!/usr/bin/env python3
"""V1 → SPEC_V2 coverage: what exists, what must be extended, what is new.

Reads the authoritative spec tables and the code, and writes:

  * `docs/evidence/v2-coverage.json` — one row per §10.3 table, §10.4 action family and
    §10.5 destination, with the verdict and the evidence the verdict rests on.
  * `docs/evidence/v2-coverage.md` — the same record as a rendered document (the design
    site's Documents destination picks it up through `build-docs.py`).

Verdicts are mechanical where a mechanical answer is honest (a table exists or it does not)
and curated where it is not (a table exists but the concept is only an extension). The
destinations are measured against the **real app**: `apps/web/src/app.rs` and the Playwright
specs in `apps/web/e2e/`, never the design pack's page registry. A mock page is cited as
design reference only (SPEC_V2 D65). The curated answers live in
`scripts/v2-coverage-overrides.json` and are always shown with their note.

    python3 scripts/build-v2-coverage.py            # write both files
    python3 scripts/build-v2-coverage.py --check    # fail if either is stale
    python3 scripts/build-v2-coverage.py --self-test  # the parser/verdict fixtures
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
APP = ROOT / "apps/web/src/app.rs"
E2E = ROOT / "apps/web/e2e"
OVERRIDES = ROOT / "scripts/v2-coverage-overrides.json"
OUT_JSON = ROOT / "docs/evidence/v2-coverage.json"
OUT_MD = ROOT / "docs/evidence/v2-coverage.md"

BACKTICK = re.compile(r"`([^`]+)`")
ROUTE = re.compile(r'path!\(\s*"([^"]+)"\s*\)')
ROUTE_VIEW = re.compile(
    r'<Route\s+path=path!\(\s*"([^"]+)"\s*\)\s+view=([A-Za-z_][A-Za-z0-9_]*)\s*/>'
)


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


# The design pack's page id per destination, cited as a design reference only. It never
# decides a verdict (SPEC_V2 D65).
DESTINATION_PAGES = {
    "Organisation": "organisation",
    "Procedures": "procedures",
    "Authoring": "authoring",
    "Portfolio": "portfolio",
    "Programme": "programme",
    "Architecture": "adm",
    "Plan": "plan",
    "Improve": "improve",
    "Operations": "operations",
    "Tools": "toolkits",
}


def app_route_paths(text: str) -> set[str]:
    """The route patterns declared in `apps/web/src/app.rs`."""
    return set(ROUTE.findall(text))


def app_route_views(text: str) -> dict[str, str]:
    """The component each `<Route … view=…/>` renders, by route pattern.

    A route whose view ends in `Gate` is the honest "not yet live" page for a destination whose
    owning slice is absent; it keeps the route honest but it is not the destination (SPEC_V2
    §10.5.2 and D65). The spec file alone must not turn a gate into `exists`.
    """
    return {path: view for path, view in ROUTE_VIEW.findall(text)}


def app_routes() -> set[str]:
    return app_route_paths(APP.read_text())


def e2e_specs() -> set[str]:
    return {path.name for path in sorted(E2E.glob("*.spec.ts"))}


def destination_rows(body: str) -> list[dict]:
    """The §10.5 destination table as route/spec rows.

    The first backticked path in the route cell is the V2 route; a path after the literal
    `V1:` is the ancestor the destination extends. The spec cell names one Playwright file.
    """
    rows: list[dict] = []
    for cells in table_rows(body):
        if len(cells) < 5:
            continue
        destination = cells[0].replace("*", "").strip()
        v2_part, _, v1_part = cells[1].partition("V1:")
        v2_routes = BACKTICK.findall(v2_part)
        if not v2_routes:
            continue
        v1_routes = BACKTICK.findall(v1_part)
        specs = BACKTICK.findall(cells[4])
        rows.append(
            {
                "destination": destination,
                "v2": v2_routes,
                "v1": v1_routes,
                "owner": cells[3].replace("*", "").replace("`", "").strip(),
                "spec": specs[0] if specs else "",
            }
        )
    return rows


def destination_verdict(
    row: dict, routes: set[str], specs: set[str], views: dict[str, str]
) -> tuple[str, list[str]]:
    """`exists` needs the real route and its spec; an ancestor or a gate is only `extend`."""
    v2_found = [path for path in row["v2"] if path in routes]
    v1_found = [path for path in row["v1"] if path in routes]
    spec = row["spec"]
    spec_present = bool(spec) and spec in specs
    evidence = [
        f"apps/web/src/app.rs: route {path!r} {'found' if path in routes else 'absent'}"
        for path in row["v2"]
    ]
    if spec:
        evidence.append(f"apps/web/e2e/{spec}: {'present' if spec_present else 'absent'}")
    if row["v1"]:
        evidence.append(
            "apps/web/src/app.rs: V1 ancestor "
            f"{', '.join(repr(path) for path in row['v1'])} {'found' if v1_found else 'absent'}"
        )
    page = DESTINATION_PAGES.get(row["destination"])
    if page:
        evidence.append(f"docs/design/v2.js: page {page!r} — design reference, not implementation")
    v2_all = len(v2_found) == len(row["v2"])
    # A gate keeps a deep link honest, but it is not the destination. Even with its own spec it
    # cannot be `exists`: the spec asserts the gate, not the screen (SPEC_V2 §10.5.2/D65).
    gate_views = [views.get(path, "") for path in v2_found]
    all_gates = v2_all and bool(gate_views) and all(view.endswith("Gate") for view in gate_views)
    if all_gates:
        for path, view in zip(v2_found, gate_views):
            evidence.append(
                f"apps/web/src/app.rs: route {path!r} renders {view} "
                f"(a gate; owner: {row['owner'] or 'unassigned'})"
            )
        return "extend", evidence
    if v2_all and spec_present:
        return "exists", evidence
    if v2_all or v1_found:
        return "extend", evidence
    return "new", evidence


def load_overrides() -> dict[str, dict]:
    if not OVERRIDES.exists():
        return {}
    document = json.loads(OVERRIDES.read_text())
    return document.get("overrides", {})


def build() -> dict:
    text = SPEC.read_text()
    migrations = migration_text()
    names = action_names()
    routes = app_routes()
    views = app_route_views(APP.read_text())
    specs = e2e_specs()
    overrides = load_overrides()

    rows: list[dict] = []

    def add(row_id: str, spec_ref: str, kind: str, concept: str, verdict: str, evidence: list[str], note: str = "") -> None:
        override = overrides.get(row_id)
        if override:
            if "verdict" in override:
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
    for row in destination_rows(ui_body):
        verdict, evidence = destination_verdict(row, routes, specs, views)
        add(f"destination:{row['destination']}", "§10.5", "destination", row["destination"], verdict, evidence)

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
            "apps/web/src/app.rs",
            "apps/web/e2e/*.spec.ts",
        ],
        "verdicts": {
            "exists": "the real route and its Playwright spec exist",
            "extend": "a V1 ancestor exists, the V2 route has no spec, or the route renders a gate naming its owner",
            "new": "neither the route nor an ancestor exists",
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


def self_test() -> None:
    """The parser and verdict fixtures; no repository state is read."""
    table = """
| Destination | Route (app) | Screens | Owning slice | Spec |
|---|---|---|---|---|
| **Alpha** | `/alpha`, `/alpha/:id` | one | T0 | `alpha.spec.ts` |
| **Beta** | `/beta` (V1: `/old-beta`) | two | T1 | `beta.spec.ts` |
| **Gamma** | `/gamma` | three | T2 | `gamma.spec.ts` |
"""
    rows = {row["destination"]: row for row in destination_rows(table)}
    assert set(rows) == {"Alpha", "Beta", "Gamma"}, rows
    assert rows["Alpha"]["v2"] == ["/alpha", "/alpha/:id"], rows["Alpha"]
    assert rows["Beta"]["v1"] == ["/old-beta"], rows["Beta"]
    assert rows["Gamma"]["spec"] == "gamma.spec.ts", rows["Gamma"]
    routes = {"/alpha", "/alpha/:id", "/old-beta"}
    cases = [
        ("Alpha", {"alpha.spec.ts"}, "exists"),
        ("Alpha", set(), "extend"),
        ("Beta", {"beta.spec.ts"}, "extend"),
        ("Gamma", {"gamma.spec.ts"}, "new"),
    ]
    for destination, specs, expected in cases:
        verdict, evidence = destination_verdict(rows[destination], routes, specs, {})
        assert verdict == expected, (destination, verdict, evidence)
    app = 'view! { <Route path=path!("/alpha") view=A/> <Route path=path!("/alpha/:id") view=B/> }'
    assert app_route_paths(app) == {"/alpha", "/alpha/:id"}, app_route_paths(app)
    assert app_route_views(app) == {"/alpha": "A", "/alpha/:id": "B"}, app_route_views(app)
    gate_app = 'view! { <Route path=path!("/gamma") view=GammaGate/> }'
    gate_views = app_route_views(gate_app)
    verdict, evidence = destination_verdict(rows["Gamma"], {"/gamma"}, {"gamma.spec.ts"}, gate_views)
    assert verdict == "extend", (verdict, evidence)
    assert any("a gate; owner: T2" in line for line in evidence), evidence
    print("build-v2-coverage: self-test OK")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="fail if an output is stale")
    parser.add_argument("--self-test", action="store_true", help="run the parser/verdict fixtures")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return
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
