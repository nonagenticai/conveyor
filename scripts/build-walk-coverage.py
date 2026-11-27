#!/usr/bin/env python3
"""The Gemba Walk parity instrument (W0).

The walk's parity claim is machine-checked, never prose. This script has two jobs:

* `--freeze` builds `docs/evidence/walk-census.json` from the sources the parity gate
  measures — the `apps/web` route table (through the shared parser in
  `scripts/build-v2-coverage.py`) and the action registry (the `#[crate::action]` attributes
  plus the manual `inventory::submit!` registrations). The freeze is a signed commit; a
  re-baseline is another signed commit, never a silent edit.
* `--check` validates the committed record with **no server, database or network**: every
  claimed state has its evidence, no exemption is expired, and the committed
  `docs/evidence/walk-coverage.json` is the record the instrument would emit today.

`--gate=P6` is the strict completeness mode wired at S4: an absent census row is a failure.
`--check` deliberately accepts absence — a destination owned by a not-yet-started sibling is
`absent`, not a gap. The four failure modes are fixture-tested by `--self-test`.

The census is a frozen comparator. When actions or routes land after the freeze, the live
drift is recorded in the coverage (`drift`) and fails `--gate=P6`; it does not fail `--check`,
because the census is re-baselined deliberately, not on every unrelated merge.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import importlib.util
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "apps/web/src/app.rs"
ACTIONS_SRC = ROOT / "crates/actions/src"
ACTIONS_JSON = ROOT / "docs/design/data/actions.json"
SPEC_RS = ROOT / "crates/actions/src/spec.rs"
EVIDENCE = ROOT / "docs/evidence"
CENSUS = EVIDENCE / "walk-census.json"
EXEMPTIONS = EVIDENCE / "walk-exemptions.json"
COVERAGE = EVIDENCE / "walk-coverage.json"
WALK = ROOT / "apps/walk"
ROUTE_DIR = WALK / "src/router/routes"
SPEC_DIR = WALK / "e2e"
WORLD = WALK / "data/world.json"

# The eight wings of SPEC §4. The world's rooms are the wings; the walls are the routes.
ROOMS: tuple[dict[str, str], ...] = (
    {"id": "portfolio", "label": "Portfolio & plan"},
    {"id": "organisation", "label": "Organisation"},
    {"id": "procedures", "label": "Procedures & authoring"},
    {"id": "governance", "label": "Govern & design"},
    {"id": "knowledge", "label": "Knowledge & memory"},
    {"id": "measure", "label": "Measure & improve"},
    {"id": "operations", "label": "Operations"},
    {"id": "utilities", "label": "Utilities"},
)

# One row per `apps/web` route pattern: (room, owning issue, the walk spec that asserts it).
# The owner is the delivery sibling that fills the route file row (plan §8). A pattern not in
# this table fails the freeze — a new route is a deliberate decision, never a silent default.
ROUTES: dict[str, tuple[str, str, str]] = {
    # G0 — the entrance and the Portfolio exemplar.
    "/": ("portfolio", "G0", "e2e/shell/entrance.spec.ts"),
    "/portfolio": ("portfolio", "G0", "e2e/portfolio/portfolio.spec.ts"),
    "/portfolio/:programme_id": ("portfolio", "G0", "e2e/portfolio/portfolio.spec.ts"),
    # S1 — Portfolio & plan (except portfolio.*) + Organisation.
    "/programme": ("portfolio", "S1", "e2e/portfolio-organisation/programme.spec.ts"),
    "/plan": ("portfolio", "S1", "e2e/portfolio-organisation/plan.spec.ts"),
    "/board/:board_id": ("portfolio", "S1", "e2e/portfolio-organisation/plan.spec.ts"),
    "/validation": ("portfolio", "S1", "e2e/portfolio-organisation/validation.spec.ts"),
    "/execution": ("portfolio", "S1", "e2e/portfolio-organisation/execution.spec.ts"),
    "/decisions": ("portfolio", "S1", "e2e/portfolio-organisation/decisions.spec.ts"),
    "/runs": ("portfolio", "S1", "e2e/portfolio-organisation/runs.spec.ts"),
    "/runs/:run_id": ("portfolio", "S1", "e2e/portfolio-organisation/runs.spec.ts"),
    "/meetings": ("portfolio", "S1", "e2e/portfolio-organisation/meetings.spec.ts"),
    "/meetings/:id": ("portfolio", "S1", "e2e/portfolio-organisation/meetings.spec.ts"),
    "/organisation": ("organisation", "S1", "e2e/portfolio-organisation/organisation.spec.ts"),
    "/jobs": ("organisation", "S1", "e2e/portfolio-organisation/jobs.spec.ts"),
    "/agents": ("organisation", "S1", "e2e/portfolio-organisation/agents.spec.ts"),
    "/tools": ("organisation", "S1", "e2e/portfolio-organisation/tools.spec.ts"),
    "/project-types": ("organisation", "S1", "e2e/portfolio-organisation/project-types.spec.ts"),
    # S2 — Procedures & authoring + Govern & design.
    "/procedures": ("procedures", "S2", "e2e/procedures-governance/procedures.spec.ts"),
    "/build/processes": ("procedures", "S2", "e2e/procedures-governance/processes.spec.ts"),
    "/build/processes/:process_id": ("procedures", "S2", "e2e/procedures-governance/processes.spec.ts"),
    "/build/process-map": ("procedures", "S2", "e2e/procedures-governance/process-map.spec.ts"),
    "/build/process-map/:map_id": ("procedures", "S2", "e2e/procedures-governance/process-map.spec.ts"),
    "/authoring": ("procedures", "S2", "e2e/procedures-governance/authoring.spec.ts"),
    "/design/adm": ("governance", "S2", "e2e/procedures-governance/adm.spec.ts"),
    "/architecture": ("governance", "S2", "e2e/procedures-governance/architecture.spec.ts"),
    "/design/requirements": ("governance", "S2", "e2e/procedures-governance/requirements.spec.ts"),
    "/design/requirements/:id": ("governance", "S2", "e2e/procedures-governance/requirements.spec.ts"),
    "/design/models": ("governance", "S2", "e2e/procedures-governance/models.spec.ts"),
    "/design/models/:model_id": ("governance", "S2", "e2e/procedures-governance/models.spec.ts"),
    "/copilot": ("governance", "S2", "e2e/procedures-governance/copilot.spec.ts"),
    # S3 — Knowledge + Measure & improve + Operations + Utilities.
    "/knowledge": ("knowledge", "S3", "e2e/measure-operations-utilities/knowledge.spec.ts"),
    "/improve": ("measure", "S3", "e2e/measure-operations-utilities/improve.spec.ts"),
    "/evals": ("measure", "S3", "e2e/measure-operations-utilities/evals.spec.ts"),
    "/evals/:eval_id": ("measure", "S3", "e2e/measure-operations-utilities/evals.spec.ts"),
    "/evals/:eval_id/:tab": ("measure", "S3", "e2e/measure-operations-utilities/evals.spec.ts"),
    "/evals/improvement": ("measure", "S3", "e2e/measure-operations-utilities/improve.spec.ts"),
    "/evals/benchmarks": ("measure", "S3", "e2e/measure-operations-utilities/benchmarks.spec.ts"),
    "/evals/benchmarks/run/:run_id": ("measure", "S3", "e2e/measure-operations-utilities/benchmarks.spec.ts"),
    "/measure/usage": ("measure", "S3", "e2e/measure-operations-utilities/usage.spec.ts"),
    "/measure/prompt-audit": ("measure", "S3", "e2e/measure-operations-utilities/prompt-audit.spec.ts"),
    "/metering": ("measure", "S3", "e2e/measure-operations-utilities/metering.spec.ts"),
    "/operations": ("operations", "S3", "e2e/measure-operations-utilities/operations.spec.ts"),
    "/search": ("utilities", "S3", "e2e/measure-operations-utilities/search.spec.ts"),
    "/platform": ("utilities", "S3", "e2e/measure-operations-utilities/platform.spec.ts"),
    "/settings/admin": ("utilities", "S3", "e2e/measure-operations-utilities/settings.spec.ts"),
    "/settings/inference": ("utilities", "S3", "e2e/measure-operations-utilities/settings.spec.ts"),
    "/settings/members": ("utilities", "S3", "e2e/measure-operations-utilities/settings.spec.ts"),
    "/settings/roles": ("utilities", "S3", "e2e/measure-operations-utilities/settings.spec.ts"),
    "/approvals": ("utilities", "S3", "e2e/measure-operations-utilities/approvals.spec.ts"),
    # S4 — retained V1 surfaces with no owning destination module.
    "/boards": ("portfolio", "S4", "e2e/v1/board.spec.ts"),
    "/card/:card_id": ("portfolio", "S4", "e2e/v1/cockpit.spec.ts"),
}

ATTR_RE = re.compile(r"#\[crate::action\((.*?)\)\]", re.S)
ATTR_NAME = re.compile(r'\bname\s*=\s*"([^"]+)"')
ATTR_STR = re.compile(r'\b{key}\s*=\s*"([^"]*)"')
ATTR_BOOL = re.compile(r"\b{key}\s*=\s*(true|false)")
ATTR_INT = re.compile(r"\b{key}\s*=\s*(\d[\d_]*)")
ATTR_LIST = re.compile(r"\bscopes\s*=\s*\[(.*?)\]", re.S)

SUBMIT_RE = re.compile(
    r"inventory::submit!\s*\{\s*(?:crate::registry::)?ActionRegistration::new\((\w+)\)\s*\}"
)
FN_RE = re.compile(r"fn\s+{fn}\s*\(.*?\)\s*->\s*ActionSpec\s*\{{(.*?)\n\}}", re.S)
CONST_RE = re.compile(r'const\s+{name}\s*:\s*&str\s*=\s*"([^"]+)"')

ROUTE_BLOCK_RE = re.compile(r"\{([^{}]*)\}", re.S)
ROUTE_FIELD = {
    key: re.compile(r'\b{key}\s*:\s*"([^"]*)"'.format(key=key))
    for key in ("walk", "app", "room", "spec")
}

SCOPE_MAP: dict[str, str] = {}


def git(*args: str) -> str:
    """Run git in the repository and return stdout, or the empty string off-repo."""
    try:
        return subprocess.run(
            ["git", *args], cwd=ROOT, check=True, capture_output=True, text=True
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return ""


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else ""


def sha256_tree(paths: list[Path]) -> str:
    h = hashlib.sha256()
    for path in sorted(paths):
        rel = path.relative_to(ROOT).as_posix()
        h.update(rel.encode() + b"\0" + path.read_bytes() + b"\0")
    return h.hexdigest()


def walk_path(pattern: str) -> str:
    """The walk path is the app path on the game host (ADR 0024).

    The `/walk` prefix was the sub-site shape; the game host serves at the root, so parity is
    an equality. The no-host dev origin still serves this same tree under `/walk/`, but the
    census and the route registry use the production path.
    """
    return pattern


def load_route_parser() -> Any:
    """Import the shared parser rather than re-implementing the router grammar (plan §6)."""
    path = ROOT / "scripts/build-v2-coverage.py"
    spec = importlib.util.spec_from_file_location("build_v2_coverage", path)
    if spec is None or spec.loader is None:
        sys.exit("build-walk-coverage: cannot load scripts/build-v2-coverage.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def parse_routes(app_text: str) -> dict[str, str]:
    """pattern -> view, from the same parser the V2 coverage record uses."""
    parser = load_route_parser()
    views = parser.app_route_views(app_text)
    if not views:
        sys.exit("build-walk-coverage: the apps/web route parser found no routes")
    return views


def parse_scope_map(spec_text: str) -> dict[str, str]:
    """Scope::Variant -> `<resource>:<verb>`, parsed from the enum's own as_str match."""
    mapping: dict[str, str] = {}
    impl = re.search(r"impl Scope \{(.*?)\n\}", spec_text, re.S)
    block = impl.group(1) if impl else spec_text
    match = re.search(r"pub fn as_str\(self\).*?\{(.*?)\n\s{4}\}", block, re.S)
    if not match:
        return mapping
    for variant, wire in re.findall(r'Self::(\w+)\s*=>\s*"([^"]+)"', match.group(1)):
        mapping[variant] = wire
    return mapping


def rust_kind(value: str | None) -> str | None:
    if not value:
        return None
    return {
        "Internal": "internal",
        "External": "external",
        "InternalRead": "internal",
    }.get(value, value.lower())


def rust_effect(value: str | None) -> str | None:
    return value.lower() if value else None


def rust_confirm(value: str | None) -> str | None:
    if not value:
        return None
    return {"None": "none", "Card": "card", "CardTyped": "card-typed"}.get(
        value, value.lower()
    )


def rust_tenancy(value: str | None) -> str | None:
    if not value:
        return None
    return {"Tenant": "tenant", "Global": "global", "Self_": "self"}.get(
        value, value.lower()
    )


def scopes_from_attr(block: str) -> list[str] | None:
    m = ATTR_LIST.search(block)
    if not m:
        return None
    return sorted(set(re.findall(r'"([^"]+)"', m.group(1))))


def action_meta_from_attr(block: str) -> dict[str, Any]:
    def field(key: str) -> str | None:
        m = re.search(ATTR_STR.pattern.format(key=key), block)
        return m.group(1) if m else None

    def flag(key: str) -> bool | None:
        m = re.search(ATTR_BOOL.pattern.format(key=key), block)
        return (m.group(1) == "true") if m else None

    def number(key: str) -> int | None:
        m = re.search(ATTR_INT.pattern.format(key=key), block)
        return int(m.group(1).replace("_", "")) if m else None

    return {
        "kind": field("kind"),
        "effect": field("effect"),
        "confirm": field("confirm"),
        "tenancy": field("tenancy"),
        "idempotent": flag("idempotent"),
        "timeout_ms": number("timeout_ms"),
        "scopes": scopes_from_attr(block),
    }


def parse_attrs(files: list[Path]) -> list[dict[str, Any]]:
    rows = []
    for path in files:
        text = path.read_text()
        for block in ATTR_RE.findall(text):
            m = ATTR_NAME.search(block)
            if not m:
                sys.exit(f"build-walk-coverage: attribute without a name in {path}")
            rows.append(
                {
                    "name": m.group(1),
                    "source": "attr",
                    "metadata_complete": True,
                    **action_meta_from_attr(block),
                }
            )
    return rows


def resolve_const(text: str, name: str) -> str | None:
    m = re.search(CONST_RE.pattern.format(name=re.escape(name)), text)
    return m.group(1) if m else None


def manual_spec_body(text: str, fn: str) -> tuple[str, str] | None:
    """The fn's body and the file; the body is the ActionSpec literal or a helper call."""
    m = re.search(FN_RE.pattern.format(fn=re.escape(fn)), text, re.S)
    if not m:
        return None
    return m.group(1), text


def manual_name(body: str, text: str) -> str | None:
    m = re.search(r'name:\s*"([^"]+)"', body)
    if m:
        return m.group(1)
    m = re.search(r"name:\s*([A-Z][A-Z0-9_]+)", body)
    if m:
        return resolve_const(text, m.group(1))
    # A helper's first string literal is the action name (`spec("lean4.check", ...)`,
    # `chat_spec("completions.create")`).
    m = re.search(r'\b\w+\s*\(\s*"([^"]+)"', body)
    return m.group(1) if m else None


def literal_for_helper(body: str, text: str) -> str | None:
    """If the fn delegates to a helper, return the helper's ActionSpec body."""
    m = re.search(r"\b(\w+)\s*\(", body)
    if not m:
        return None
    helper = m.group(1)
    if helper in ("ActionSpec",):
        return body
    hm = re.search(
        r"fn\s+" + re.escape(helper) + r"\s*\(.*?\)\s*->\s*ActionSpec\s*\{(.*?)\n\}",
        text,
        re.S,
    )
    return hm.group(1) if hm else None


def manual_meta(body: str, text: str) -> dict[str, Any]:
    # The metadata may be in the fn's own struct (a `Target` literal) or in the helper it
    # delegates to (`spec(...)`, `chat_spec(...)`). Search the fn first, then the helper.
    literal = literal_for_helper(body, text) or ""
    haystack = body + "\n" + literal

    def field(key: str, pattern: str) -> str | None:
        m = re.search(pattern.format(key=key), haystack)
        return m.group(1) if m else None

    scopes: list[str] | None = None
    m = re.search(r"scopes:\s*&?\[(.*?)\]", haystack, re.S)
    if m:
        variants = re.findall(r"Scope::(\w+)", m.group(1))
        scopes = sorted({SCOPE_MAP.get(v, f"Scope::{v}") for v in variants})
    else:
        m = re.search(r"scopes:\s*([A-Z][A-Z0-9_]+)", haystack)
        if m:
            const_name = m.group(1)
            cm = re.search(
                r"const\s+" + re.escape(const_name) + r"\s*:\s*&\[Scope\]\s*=\s*&\[(.*?)\];",
                text,
                re.S,
            )
            if cm:
                variants = re.findall(r"Scope::(\w+)", cm.group(1))
                scopes = sorted({SCOPE_MAP.get(v, f"Scope::{v}") for v in variants})

    timeout = field("timeout_ms", r"timeout_ms:\s*(\d[\d_]*)")
    return {
        "kind": rust_kind(field("kind", r"kind:\s*ActionKind::(\w+)")),
        "effect": rust_effect(field("effect", r"effect:\s*Effect::(\w+)")),
        "confirm": rust_confirm(field("confirm", r"confirm:\s*Confirm::(\w+)")),
        "tenancy": rust_tenancy(field("tenancy", r"tenancy:\s*Tenancy::(\w+)")),
        "idempotent": (field("idempotent", r"idempotent:\s*(true|false)") == "true")
        if re.search(r"idempotent:\s*(true|false)", haystack)
        else None,
        "timeout_ms": int(timeout.replace("_", "")) if timeout else None,
        "scopes": scopes,
    }


def parse_manual(files: list[Path]) -> list[dict[str, Any]]:
    rows = []
    for path in files:
        text = path.read_text()
        for fn in SUBMIT_RE.findall(text):
            found = manual_spec_body(text, fn)
            if not found:
                sys.exit(
                    f"build-walk-coverage: {path}: submit of {fn} has no ActionSpec constructor"
                )
            body, _ = found
            name = manual_name(body, text)
            if not name:
                sys.exit(
                    f"build-walk-coverage: {path}: submit of {fn} has no resolvable action name"
                )
            meta = manual_meta(body, text)
            rows.append(
                {
                    "name": name,
                    "source": "manual",
                    "metadata_complete": all(
                        meta[key] is not None
                        for key in ("kind", "effect", "confirm", "tenancy", "idempotent", "scopes")
                    ),
                    **meta,
                }
            )
    return rows


def check_unique(rows: list[dict[str, Any]], what: str) -> None:
    seen: dict[str, int] = {}
    for row in rows:
        seen[row["name"]] = seen.get(row["name"], 0) + 1
    dupes = sorted(name for name, count in seen.items() if count > 1)
    if dupes:
        sys.exit(f"build-walk-coverage: duplicate {what} names: {dupes[:10]}")


def freeze(baseline: str | None) -> dict[str, Any]:
    action_files = sorted(ACTIONS_SRC.rglob("*.rs"))
    attrs = parse_attrs(action_files)
    manual = parse_manual(action_files)
    check_unique(attrs, "attribute action")
    check_unique(manual, "manual action")
    actions = attrs + manual
    check_unique(actions, "action")

    actual = set(json.loads(ACTIONS_JSON.read_text())["actions"])
    parsed = {row["name"] for row in actions}
    missing = sorted(actual - parsed)
    extra = sorted(parsed - actual)
    if missing or extra:
        sys.exit(
            "build-walk-coverage: the parsed action set does not equal actions.json\n"
            f"  missing: {missing[:20]}\n  extra: {extra[:20]}"
        )

    views = parse_routes(APP.read_text())
    unknown = sorted(set(views) - set(ROUTES))
    stale = sorted(set(ROUTES) - set(views))
    if unknown or stale:
        sys.exit(
            "build-walk-coverage: the route map and apps/web disagree\n"
            f"  unmapped routes: {unknown[:20]}\n  stale map rows: {stale[:20]}"
        )

    git_sha = baseline or git("rev-parse", "origin/main") or git("rev-parse", "HEAD")
    routes = []
    for pattern in sorted(views):
        room, owner, spec = ROUTES[pattern]
        routes.append(
            {
                "pattern": pattern,
                "walk": walk_path(pattern),
                "view": views[pattern],
                "room": room,
                "owner": owner,
                "spec": spec,
                "status": "absent",
            }
        )
    actions = [
        {
            "name": row["name"],
            "source": row["source"],
            "kind": row["kind"],
            "effect": row["effect"],
            "confirm": row["confirm"],
            "tenancy": row["tenancy"],
            "idempotent": row["idempotent"],
            "timeout_ms": row.get("timeout_ms"),
            "scopes": row["scopes"],
            "metadata_complete": row["metadata_complete"],
            "status": "absent",
        }
        for row in sorted(actions, key=lambda r: r["name"])
    ]

    census = {
        "schema": 1,
        "baseline": {
            "date": dt.date.today().isoformat(),
            "git_sha": git_sha,
            "prior": {"routes": 52, "actions": 597, "source": "inception measurement 2026-11-26"},
        },
        "counts": {
            "routes": len(routes),
            "actions": len(actions),
            "attrs": len(attrs),
            "manual": len(manual),
        },
        "provenance": {
            "app_sha256": sha256_file(APP),
            "actions_sha256": sha256_tree(action_files),
            "actions_json_sha256": sha256_file(ACTIONS_JSON),
            "scope_map_entries": len(SCOPE_MAP),
        },
        "routes": routes,
        "actions": actions,
    }
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    CENSUS.write_text(json.dumps(census, indent=2, sort_keys=False) + "\n")
    if not EXEMPTIONS.exists():
        EXEMPTIONS.write_text(json.dumps({"schema": 1, "exemptions": []}, indent=2) + "\n")
    write_world(census)
    return census


def write_world(census: dict[str, Any]) -> None:
    WORLD.parent.mkdir(parents=True, exist_ok=True)
    world = {
        "schema": 1,
        "baseline": census["baseline"],
        "rooms": list(ROOMS),
        "walls": [
            {
                "route": row["pattern"],
                "walk": row["walk"],
                "room": row["room"],
                "view": row["view"],
                "owner": row["owner"],
            }
            for row in census["routes"]
        ],
    }
    WORLD.write_text(json.dumps(world, indent=2) + "\n")


def load_census() -> dict[str, Any]:
    if not CENSUS.is_file():
        sys.exit(f"build-walk-coverage: {CENSUS.relative_to(ROOT)} is missing; run --freeze")
    return json.loads(CENSUS.read_text())


def load_exemptions() -> list[dict[str, Any]]:
    if not EXEMPTIONS.is_file():
        return []
    data = json.loads(EXEMPTIONS.read_text())
    return data.get("exemptions", [])


def parse_registry() -> dict[str, dict[str, str]]:
    """walk path -> {app, room, spec}, from the route files' constrained row syntax."""
    rows: dict[str, dict[str, str]] = {}
    if not ROUTE_DIR.is_dir():
        return rows
    for path in sorted(ROUTE_DIR.glob("*.js")):
        if path.name == "registry.js":
            continue
        text = path.read_text()
        for block in ROUTE_BLOCK_RE.findall(text):
            walk = ROUTE_FIELD["walk"].search(block)
            if not walk:
                continue
            row = {"file": path.relative_to(WALK).as_posix(), "walk": walk.group(1)}
            for key in ("app", "room", "spec"):
                m = ROUTE_FIELD[key].search(block)
                if not m:
                    sys.exit(
                        f"build-walk-coverage: {path.relative_to(ROOT)}: a route row has no {key}"
                    )
                row[key] = m.group(1)
            if row["walk"] in rows:
                sys.exit(f"build-walk-coverage: duplicate walk path {row['walk']}")
            rows[row["walk"]] = row
    return rows


CAPABILITIES: dict[str, re.Pattern[str]] = {
    # The capability, not a guessed file path (SPEC §6.1 fixes the seam's name and
    # responsibility, not the implementation's module layout). A rename must not flip the
    # status; the per-destination specs remain the proof that a destination is wired.
    "manifest": re.compile(r"\bmanifest\s*\("),
    "sheet": re.compile(r"\bopenAction\s*\("),
    "form": re.compile(r"\brenderSchemaForm\s*\("),
}


def detect_modules(sources: dict[str, str]) -> dict[str, bool]:
    """Capability detection over the walk's JavaScript sources (path -> text)."""
    text = "\n".join(sources.values())
    return {name: bool(pattern.search(text)) for name, pattern in CAPABILITIES.items()}


def modules_present() -> dict[str, bool]:
    src = WALK / "src"
    if not src.is_dir():
        return {name: False for name in CAPABILITIES}
    return detect_modules({path.as_posix(): path.read_text() for path in sorted(src.rglob("*.js"))})


def load_report(path: str | None) -> set[str]:
    """The Playwright JSON report's passing spec files, relative to apps/walk."""
    if not path:
        return set()
    p = Path(path)
    if not p.is_file():
        sys.exit(f"build-walk-coverage: no Playwright report at {path}")
    data = json.loads(p.read_text())
    passed: set[str] = set()

    def walk(suite: dict[str, Any], inherited: str) -> None:
        file = suite.get("file") or inherited
        for spec in suite.get("specs", []):
            if spec.get("ok") and file:
                passed.add(file)
        for child in suite.get("suites", []):
            walk(child, file)

    for suite in data.get("suites", []):
        walk(suite, "")
    return passed


def build_coverage(
    census: dict[str, Any],
    registry: dict[str, dict[str, str]],
    exemptions: list[dict[str, Any]],
    passed: set[str] | None = None,
    committed: dict[str, Any] | None = None,
    modules: dict[str, bool] | None = None,
) -> dict[str, Any]:
    passed = passed or set()
    modules = modules if modules is not None else modules_present()
    today = dt.date.today()
    gaps: list[str] = []

    routes = []
    for row in census["routes"]:
        walk = row["walk"]
        reg = registry.get(walk)
        spec_present = (WALK / row["spec"]).is_file()
        if reg is None:
            status = "absent"
        else:
            if reg.get("app") != row["pattern"]:
                gaps.append(f"route {walk}: registry app {reg.get('app')!r} != {row['pattern']!r}")
            if reg.get("room") != row["room"]:
                gaps.append(f"route {walk}: registry room {reg.get('room')!r} != {row['room']!r}")
            if not spec_present:
                gaps.append(f"route {walk}: claims a spec but {row['spec']} is missing")
            status = "verified" if spec_present and row["spec"] in passed else "registered"
        routes.append(
            {**row, "status": status, "spec_present": spec_present, "registered": reg is not None}
        )

    # A committed claim the instrument cannot substantiate is the parity lie the census exists
    # to catch: `registered` with no row, `reachable` with no sheet.
    if committed:
        prior = {row["walk"]: row.get("status") for row in committed.get("routes", [])}
        for row in routes:
            claimed = prior.get(row["walk"], "absent")
            if claimed != "absent" and not row["registered"]:
                gaps.append(f"route {row['walk']}: committed status {claimed!r} has no registry row")
        prior_actions = {row["name"]: row.get("status") for row in committed.get("actions", [])}
        for row in census["actions"]:
            claimed = prior_actions.get(row["name"], "absent")
            if claimed in ("reachable", "schema-rendered") and not (
                modules["manifest"] and modules["sheet"] and modules["form"]
            ):
                gaps.append(
                    f"action {row['name']}: committed status {claimed!r} has no action-sheet module"
                )

    sheet_ready = modules["manifest"] and modules["sheet"]
    form_ready = modules["manifest"] and modules["form"]
    actions = []
    for row in census["actions"]:
        if not sheet_ready:
            status = "absent"
        elif not form_ready:
            status = "reachable"
        else:
            status = "schema-rendered"
        actions.append({**row, "status": status})

    unknown_rows = sorted(set(registry) - {row["walk"] for row in census["routes"]})
    for walk in unknown_rows:
        gaps.append(f"route {walk}: registry row is not in the frozen census")

    known_surfaces = {row["pattern"] for row in census["routes"]} | {
        row["name"] for row in census["actions"]
    } | {row["walk"] for row in census["routes"]}
    expired = []
    for ex in exemptions:
        surface = ex.get("surface", "")
        expires = ex.get("expires", "")
        if not ex.get("signer"):
            gaps.append(f"exemption {surface!r}: no signer")
        if surface not in known_surfaces:
            gaps.append(f"exemption {surface!r}: not a census surface")
        try:
            when = dt.date.fromisoformat(expires)
        except ValueError:
            gaps.append(f"exemption {surface!r}: expires is not an ISO date")
            continue
        if when < today:
            expired.append(surface)
            gaps.append(f"exemption {surface!r}: expired {expires}")

    # Live drift: the census is the comparator, so a changed source is reported and fails P6,
    # never silently absorbed.
    drift: dict[str, Any] = {}
    try:
        live_views = parse_routes(APP.read_text())
        action_files = sorted(ACTIONS_SRC.rglob("*.rs"))
        live_actions = {r["name"] for r in parse_attrs(action_files) + parse_manual(action_files)}
        census_routes = {row["pattern"] for row in census["routes"]}
        census_actions = {row["name"] for row in census["actions"]}
        live_routes = set(live_views)
        drift = {
            "live_routes": len(live_routes),
            "census_routes": len(census_routes),
            "live_actions": len(live_actions),
            "census_actions": len(census_actions),
            "missing_routes": sorted(live_routes - census_routes),
            "extra_routes": sorted(census_routes - live_routes),
            "missing_actions": sorted(live_actions - census_actions),
            "extra_actions": sorted(census_actions - live_actions),
        }
    except SystemExit as exc:
        drift = {"error": str(exc)}

    summary = {
        "routes_total": len(routes),
        "routes_absent": sum(1 for r in routes if r["status"] == "absent"),
        "routes_registered": sum(1 for r in routes if r["status"] == "registered"),
        "routes_verified": sum(1 for r in routes if r["status"] == "verified"),
        "actions_total": len(actions),
        "actions_absent": sum(1 for a in actions if a["status"] == "absent"),
        "actions_reachable": sum(1 for a in actions if a["status"] == "reachable"),
        "actions_schema_rendered": sum(1 for a in actions if a["status"] == "schema-rendered"),
    }
    return {
        "schema": 1,
        "baseline": census["baseline"],
        "counts": census["counts"],
        "summary": summary,
        "routes": routes,
        "actions": actions,
        "drift": drift,
        "exemptions": {"count": len(exemptions), "expired": expired},
        "gaps": gaps,
    }


def strict_gaps(coverage: dict[str, Any]) -> list[str]:
    gaps = []
    for row in coverage["routes"]:
        if row["status"] != "verified":
            gaps.append(f"route {row['walk']}: {row['status']} at P6; every route must be verified")
    for row in coverage["actions"]:
        if row["status"] != "schema-rendered":
            gaps.append(
                f"action {row['name']}: {row['status']} at P6; every action must be schema-rendered"
            )
    drift = coverage.get("drift") or {}
    if drift.get("error"):
        gaps.append(f"drift check failed: {drift['error']}")
    else:
        for key in ("missing_routes", "extra_routes", "missing_actions", "extra_actions"):
            for item in drift.get(key, []):
                gaps.append(f"{key}: {item}")
    return gaps


def write_coverage(coverage: dict[str, Any]) -> None:
    COVERAGE.write_text(json.dumps(coverage, indent=2) + "\n")


def self_test() -> int:
    """The rule matrix, with synthetic fixtures. No repo state, no clock dependence."""
    failures: list[str] = []

    def ok(cond: bool, label: str) -> None:
        if not cond:
            failures.append(label)

    # The parser shapes: an attribute, a direct manual struct, a helper call, a const name.
    attr = """
    #[crate::action(
        name = "lessons.propose", version = 1, kind = "internal",
        scopes = ["lessons:write"], idempotent = false, timeout_ms = 30000,
        effect = "write", confirm = "none", tenancy = "tenant"
    )]
    """
    meta = action_meta_from_attr(ATTR_RE.findall(attr)[0])
    ok(meta["effect"] == "write" and meta["scopes"] == ["lessons:write"], "attr meta")
    manual = """
    fn echo_spec() -> ActionSpec {
        ActionSpec { name: "fixtures.echo", kind: ActionKind::External,
            scopes: &[Scope::ActionsRead], idempotent: true, timeout_ms: 30_000,
            effect: Effect::Read, confirm: Confirm::None, tenancy: Tenancy::Tenant }
    }
    inventory::submit! { ActionRegistration::new(echo_spec) }
    """
    ok(manual_name(manual, manual) == "fixtures.echo", "manual struct name")
    helper = """
    fn check() -> ActionSpec { spec("lean4.check", "/api/check", 125_000, check_input) }
    inventory::submit! { ActionRegistration::new(check) }
    """
    ok(manual_name("spec(\"lean4.check\", \"/api/check\", 125_000, check_input)", helper)
       == "lean4.check", "manual helper name")
    ok(resolve_const('const TOOLS_CALL: &str = "tools.call";', "TOOLS_CALL") == "tools.call",
       "manual const name")

    census = {
        "baseline": {"date": "2026-01-01", "git_sha": "0" * 40},
        "counts": {"routes": 1, "actions": 1, "attrs": 1, "manual": 0},
        "routes": [{"pattern": "/x", "walk": "/x", "view": "X", "room": "portfolio",
                    "owner": "G0", "spec": "e2e/x/x.spec.ts", "status": "absent"}],
        "actions": [{"name": "x.read", "source": "attr", "kind": "internal",
                     "effect": "read", "confirm": "none", "tenancy": "tenant",
                     "idempotent": True, "timeout_ms": 1, "scopes": ["x:read"],
                     "metadata_complete": True, "status": "absent"}],
    }
    registry = {"/x": {"walk": "/x", "app": "/x", "room": "portfolio",
                            "spec": "e2e/x/x.spec.ts", "file": "src/router/routes/core.js"}}
    no_caps = {"manifest": False, "sheet": False, "form": False}
    all_caps = {"manifest": True, "sheet": True, "form": True}
    base = build_coverage(census, {}, [], committed=None, modules=no_caps)
    ok(base["gaps"] == [], "clean baseline has no gaps")
    ok(any("absent at P6" in g for g in strict_gaps(base)), "P6 fails an absent route")

    claim = {"routes": [{"walk": "/x", "status": "registered"}], "actions": []}
    gaps = build_coverage(census, {}, [], committed=claim, modules=no_caps)["gaps"]
    ok(any("has no registry row" in g for g in gaps), "failure 1: claim without a row")

    gaps = build_coverage(census, registry, [], committed=None, modules=no_caps)["gaps"]
    ok(any("claims a spec" in g for g in gaps), "failure 3: registered route without its spec")

    action_claim = {"routes": [], "actions": [{"name": "x.read", "status": "reachable"}]}
    gaps = build_coverage(census, {}, [], committed=action_claim, modules=no_caps)["gaps"]
    ok(any("has no action-sheet module" in g for g in gaps), "failure 2: action claim without a sheet")

    # Capability detection: the seam's capability, wherever the implementation puts it.
    caps = detect_modules({
        "src/api/client.js": "  async manifest() {\n    return this.request(\"/api/actions\");\n  }",
        "src/shell/shell.js": "  async openAction(name, options = {}) {",
        "src/forms/schema.js": "export function renderSchemaForm(schema, options) {",
    })
    ok(caps == all_caps, "capability: the delivered shapes are detected")
    ok(detect_modules({"src/shell/renamed-sheet.js": "openAction(name)"})["sheet"],
       "capability: a renamed module is not a false negative")
    ok(not detect_modules({"src/api/client.js": "function manifestThing() {"})["manifest"],
       "capability: a lookalike token is not a false positive")
    ok(build_coverage(census, {}, [], committed=None, modules=all_caps)["actions"][0]["status"]
       == "schema-rendered", "capability: actions are schema-rendered when the sheet exists")
    ok(build_coverage(census, {}, [], committed=None, modules=no_caps)["actions"][0]["status"]
       == "absent", "capability: actions are absent without the sheet")

    expired = [{"surface": "/x", "reason": "r", "signer": "s", "expires": "2000-01-01"}]
    gaps = build_coverage(census, registry, expired, modules=no_caps)["gaps"]
    ok(any("expired" in g for g in gaps), "failure 4: expired exemption")

    a = json.dumps(base, sort_keys=True)
    b = json.dumps(build_coverage(census, {}, [], committed=None, modules=no_caps), sort_keys=True)
    ok(a == b, "deterministic output")

    for label in failures:
        print(f"FAIL: {label}")
    print(f"{len(failures)} failing")
    return 1 if failures else 0


def main(argv: list[str] | None = None) -> int:
    global SCOPE_MAP
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--freeze", action="store_true", help="write the census from sources")
    mode.add_argument("--check", action="store_true", help="validate the committed record")
    mode.add_argument("--write", action="store_true", help="write the coverage record")
    mode.add_argument("--print", action="store_true", help="print the coverage record")
    mode.add_argument("--self-test", action="store_true", help="run the rule matrix")
    parser.add_argument("--gate", choices=["P6"], help="the strict completeness gate")
    parser.add_argument("--baseline", help="the code baseline sha for --freeze")
    parser.add_argument("--report", help="a Playwright JSON report; marks passing specs verified")
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()

    SCOPE_MAP = parse_scope_map(SPEC_RS.read_text())

    if args.freeze:
        census = freeze(args.baseline)
        coverage = build_coverage(census, parse_registry(), load_exemptions(), load_report(args.report))
        write_coverage(coverage)
        print_summary(coverage)
        return 1 if coverage["gaps"] else 0

    census = load_census()
    coverage = build_coverage(
        census,
        parse_registry(),
        load_exemptions(),
        load_report(args.report),
        committed=json.loads(COVERAGE.read_text()) if COVERAGE.is_file() else None,
    )
    gaps = list(coverage["gaps"])
    if args.gate == "P6":
        gaps += strict_gaps(coverage)

    if args.print:
        print(json.dumps({**coverage, "gaps": gaps}, indent=2))
    elif args.write:
        write_coverage({**coverage, "gaps": gaps})
        print_summary({**coverage, "gaps": gaps})
    else:
        print_summary({**coverage, "gaps": gaps})

    for gap in gaps:
        print(f"gap: {gap}", file=sys.stderr)
    return 1 if gaps else 0


def print_summary(coverage: dict[str, Any]) -> None:
    s = coverage["summary"]
    print(
        "walk coverage: "
        f"routes {s['routes_total']} (absent {s['routes_absent']}, registered {s['routes_registered']}, "
        f"verified {s['routes_verified']}); "
        f"actions {s['actions_total']} (absent {s['actions_absent']}, reachable {s['actions_reachable']}, "
        f"schema-rendered {s['actions_schema_rendered']}); "
        f"gaps {len(coverage['gaps'])}"
    )


if __name__ == "__main__":
    sys.exit(main())
