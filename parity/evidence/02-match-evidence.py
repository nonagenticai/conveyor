#!/usr/bin/env python3
"""S00 evidence matcher — maps Loki ingress-log series to inventory surfaces.

WHY THIS EXISTS
  Day 1 matched concrete request paths to surface templates by hand and kept no program, so
  the only record of the method is prose (00-preflight.txt §8b). A second sample cannot be
  compared against the first unless the same mapping is applied to both. This program is that
  mapping, and `--reproduce` proves it against sample 1: for every B-class row it recomputes
  the count from the raw JSON and asserts it equals the count day 1 recorded in the row.

  --reproduce is the guard. If it disagrees, the day-2 numbers are not comparable and the run
  must stop rather than sign.

METHOD (identical for both samples)
  * A surface is `METHOD /template`. A path segment is literal, `{param}`/`:param` (one
    segment), or `(.*)`/`*` (suffix). Trailing-slash variants are accepted (FastAPI/Nest 307).
  * Query strings are stripped from observed paths.
  * Each observed (method, path) contributes to EXACTLY ONE surface: the matching template
    with the most literal segments wins (ties: longer literal prefix, then the template that
    is not a suffix wildcard). That is what stops `/api/projects/{id}` swallowing
    `/api/projects/new`, and it stops one request being counted twice.
  * Host -> app: `api.nonagentic.ai` (and `api.example.com`) rewrite before matching
    (`/requirements/*` -> rm path `/*`, `/kanban/*` -> planka path `/*`, else fr).

USAGE
  python3 02-match-evidence.py --reproduce <parity.yaml.day1.bak> <evidence-dir>
  python3 02-match-evidence.py --counts    <parity.yaml>           <evidence-dir> <sample-prefix>
    prints TSV: id <TAB> surface <TAB> host <TAB> count
"""
import json
import glob
import os
import re
import sys

HOST_APPS = {
    "fr.nonagentic.ai": ["fr"],
    "fr.example.com": ["fr"],
    "fr-backend.example.com": ["fr"],
    "requirements.example.com": ["rm"],
    "kanban.nonagentic.ai": ["planka"],
    "kanban.example.com": ["planka"],
    "n8n.nonagentic.ai": ["n8n"],
    "n8n.example.com": ["n8n"],
    "api.nonagentic.ai": ["fr", "rm", "planka"],
    "api.example.com": ["fr", "rm", "planka"],
}
# Rewrite prefixes applied to concrete paths on the shared api hosts.
REWRITES = [("/requirements/", "rm"), ("/kanban/", "planka")]


def split_surface(surface):
    parts = surface.split(" ", 1)
    if len(parts) == 2 and parts[0].isalpha() and parts[0].isupper():
        return parts[0], parts[1]
    return None, surface


def compile_template(path):
    path = path.split("?")[0]
    if not path.startswith("/"):
        path = "/" + path
    out, literals, wildcard = [], 0, False
    for seg in path.split("/")[1:]:
        if seg == "":
            # A trailing "/" (or the root path) is a separator, NOT a wildcard. Treating it as
            # one makes `GET /` compile to ^/.*/?$ and swallow every request on the host.
            continue
        if seg in ("(.*)", "*"):
            out.append(".*")
            wildcard = True
        elif seg.startswith("{") or seg.startswith(":"):
            out.append("[^/]+")
        elif "(.*)" in seg:
            out.append(re.escape(seg.split("(.*)")[0]) + ".*")
            wildcard = True
        else:
            out.append(re.escape(seg))
            literals += 1
    return re.compile("^/" + "/".join(out) + "/?$"), literals, wildcard


def build_templates(rows):
    by_app = {}
    for row in rows:
        app = row.get("app")
        method, template = split_surface(row.get("surface", ""))
        if method is None or not template.startswith("/"):
            continue
        rx, literals, wildcard = compile_template(template)
        by_app.setdefault(app, []).append((row["id"], method, rx, literals, wildcard, len(template)))
    return by_app


def count_file(by_app, host, data):
    """Return {(id): count} for one host's Loki dump, one surface per observed path."""
    apps = HOST_APPS.get(host, [])
    totals = {}
    unmatched = []
    for series in data.get("data", {}).get("result", []):
        metric = series.get("metric", {})
        method = (metric.get("method") or "").upper()
        raw = (metric.get("path") or "").split("?")[0]
        if not method or not raw:
            continue
        try:
            count = int(float(series["value"][1]))
        except (KeyError, IndexError, ValueError):
            continue
        candidates = []
        for app in apps:
            path = raw
            if host in ("api.nonagentic.ai", "api.example.com"):
                for prefix, owner in REWRITES:
                    if path.startswith(prefix) and owner == app:
                        path = "/" + path[len(prefix):]
                        break
                else:
                    if app != "fr":
                        continue
            for tid, tmethod, rx, literals, wildcard, tlen in by_app.get(app, []):
                if tmethod != method:
                    continue
                if rx.match(path):
                    candidates.append((literals, tlen, 0 if wildcard else 1, tid))
        if not candidates:
            unmatched.append((method, raw))
            continue
        candidates.sort(reverse=True)
        totals[candidates[0][3]] = totals.get(candidates[0][3], 0) + count
    return totals, unmatched


def load_samples(evidence_dir, prefix):
    files = sorted(glob.glob(os.path.join(evidence_dir, prefix + "-*.json")))
    out = {}
    for path in files:
        name = os.path.basename(path)[len(prefix) + 1: -len(".json")]
        with open(path) as fh:
            out[name] = json.load(fh)
    return out


def main():
    mode = sys.argv[1]
    inventory = sys.argv[2]
    evidence_dir = sys.argv[3]

    with open(inventory) as fh:
        rows = json.load(fh)
    by_app = build_templates(rows)
    id_row = {r["id"]: r for r in rows}

    if mode == "--reproduce":
        samples = load_samples(evidence_dir, "loki-sample1-20261107T221746Z")
        totals = {}
        for host, data in samples.items():
            if host == "positive-control":
                continue
            got, _ = count_file(by_app, host, data)
            for tid, n in got.items():
                totals[tid] = totals.get(tid, 0) + n
        bad = []
        checked = 0
        for row in rows:
            if row.get("evidence_class") != "B":
                continue
            m = re.search(r"^B1 (\d+) in 24h", row.get("evidence", ""))
            if not m:
                continue
            checked += 1
            want, got = int(m.group(1)), totals.get(row["id"], 0)
            if want != got:
                bad.append((row["id"], row["surface"], want, got))
        print(f"B rows checked: {checked}  mismatches: {len(bad)}")
        for tid, surface, want, got in bad:
            print(f"  MISMATCH {tid}\t{surface}\twant={want} got={got}")
        sys.exit(0 if not bad else 1)

    if mode == "--counts":
        sample_prefix = sys.argv[4]
        samples = load_samples(evidence_dir, sample_prefix)
        totals = {}
        for host, data in samples.items():
            if host == "positive-control":
                continue
            got, unmatched = count_file(by_app, host, data)
            for tid, n in got.items():
                totals[tid] = totals.get(tid, 0) + n
            if unmatched:
                print(f"# unmatched on {host}: {len(unmatched)}", file=sys.stderr)
        for tid, n in sorted(totals.items()):
            row = id_row.get(tid, {})
            print(f"{tid}\t{row.get('app','')}\t{row.get('surface','')}\t{n}")
        sys.exit(0)

    print("usage: --reproduce <inventory> <evidence-dir> | --counts <inventory> <dir> <prefix>", file=sys.stderr)
    sys.exit(2)


if __name__ == "__main__":
    main()
