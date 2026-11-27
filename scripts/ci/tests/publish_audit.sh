#!/usr/bin/env bash
# The publish path audits the fresh snapshot before it replaces the published pack.
#
# The test runs the real publish-data.sh in a temp root with a stub conveyor binary, a stub
# issue fetch and a minimal snapshot. It proves two things:
#   1. CONFORMANCE_STRICT=1 with an empty baseline refuses a P0 and leaves the pack untouched;
#   2. report-only mode publishes the snapshot and the conformance register.
#
# Run: bash scripts/ci/tests/publish_audit.sh
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/docs/design/scripts" "$tmp/docs/design/docs" "$tmp/docs/design/data" \
  "$tmp/docs/evidence" "$tmp/scripts" "$tmp/governance" "$tmp/seed" "$tmp/homepage/design" \
  "$tmp/fixture" "$tmp/fixture-docs" "$tmp/bin"

cp "$repo/docs/design/scripts/publish-data.sh" "$tmp/docs/design/scripts/publish-data.sh"
# Order-proving stand-ins: the real generators read the whole tree, so the test proves the
# ORDER — the docs index hashes the INSTALLED conformance.md — with a stub that records the
# hash it sees. A stub that cannot find its input fails the publish, which is the assertion.
cat > "$tmp/docs/design/scripts/build-docs.py" <<'PY'
#!/usr/bin/env python3
import hashlib, json, pathlib
root = pathlib.Path(__file__).resolve().parents[3]
installed = root / "docs/design/data/conformance.md"
digest = hashlib.sha256(installed.read_bytes()).hexdigest() if installed.exists() else None
out = root / "docs/design/docs"
out.mkdir(parents=True, exist_ok=True)
document = {"conformance_sha256": digest}
(out / "documents.json").write_text(json.dumps(document, sort_keys=True) + "\n")
(out / "documents.js").write_text(
    "window.CONVEYOR_DOCUMENTS = " + json.dumps(document, sort_keys=True) + ";\n"
)
PY
cat > "$tmp/scripts/build-coverage.py" <<'PY'
#!/usr/bin/env python3
import hashlib, json, pathlib, sys
root = pathlib.Path(__file__).resolve().parents[1]
if "--check" in sys.argv:
    sys.exit(0)
registry = (root / "docs/design/data/issues.json").read_bytes()
document = {"registry_sha256": hashlib.sha256(registry).hexdigest()}
(root / "docs/design/data/coverage.json").write_text(json.dumps(document, sort_keys=True) + "\n")
PY
cat > "$tmp/scripts/build-v2-coverage.py" <<'PY'
#!/usr/bin/env python3
import sys
sys.exit(0)
PY
cat > "$tmp/scripts/fetch-issues.sh" <<'SH'
#!/usr/bin/env bash
printf '%s' '{"issues": [{"number": 9001, "title": "fetched registry", "state": "OPEN", "labels": []}]}' > "$1"
SH
cp "$repo/scripts/audit-programme-conformance.py" "$tmp/scripts/audit-programme-conformance.py"
cp "$repo/scripts/generate-standards.py" "$tmp/scripts/generate-standards.py"
cp "$repo/governance/conformance.json" "$tmp/governance/conformance.json"
cp "$repo/seed/programme.json" "$tmp/seed/programme.json"
cp "$repo/seed/programme-definition.json" "$tmp/seed/programme-definition.json"
cp "$repo/seed/output-standard.json" "$tmp/seed/output-standard.json"
chmod +x "$tmp/scripts/fetch-issues.sh" "$tmp/docs/design/scripts/build-docs.py"

cat > "$tmp/fixture/programme.json" <<'JSON'
{"programme": [{"id": "pg1", "name": "fixture programme", "status": "active"}], "projects": [
  {"id": "p1", "key": "T1", "programme_id": "pg1", "status": "closed", "created_at": "2026-11-20T00:00:00Z",
   "meta": {"track": "v2", "tranche": 1, "expected_outputs": [], "closure_outputs": [],
            "measured_status": "built", "adm_phase": "g", "plateau": "delivery"}}
]}
JSON
cat > "$tmp/fixture/msp.json" <<'JSON'
{"tranches": [{"id": "tr1", "number": 1, "state": "planned"}], "tranche_reviews": [],
 "benefit_profiles": [], "benefit_links": [], "benefit_transitions": [],
 "programme_baselines": []}
JSON
cat > "$tmp/fixture/architecture.json" <<'JSON'
{"profiles": [], "standard": [], "artifacts": [], "links": [], "contracts": [],
 "compliance_reviews": [], "waivers": [], "board": []}
JSON
cat > "$tmp/fixture/manifest.json" <<'JSON'
{"schema": "design-snapshot/v1", "git_sha": "test", "generated_at": "2026-11-20T00:00:00Z", "tenant": "test"}
JSON
cat > "$tmp/fixture/coverage.json" <<'JSON'
{"projects": []}
JSON
# No fixture issues.json: the fetched registry is the pack's copy, and the stub snapshot must not
# overwrite it (the order assertion below reads the installed file).
cat > "$tmp/fixture-docs/outputs.js" <<'JSON'
window.CONVEYOR_OUTPUTS = {"schema": "design-outputs/v1", "generated_at": "2026-11-20T00:00:00Z", "outputs": []};
JSON
cat > "$tmp/docs/evidence/conformance-baseline.json" <<'JSON'
{"version": 1, "entries": []}
JSON
# The checked-in v1 ledger is an INPUT to the register, not a side note. The finding below is
# distinctive; the published register must carry it, or publish-data.sh dropped the ledger.
cat > "$tmp/docs/evidence/v1-conformance-findings.json" <<'JSON'
{"schema": "conveyor-v1-findings/v1", "audited_sha": "0000000000000000000000000000000000000000",
 "audited_at": "2026-11-20", "method": "publish audit fixture",
 "findings": [{"id": "fixture-p1", "area": "§5.4", "standard": "SPEC", "spec": "§5.4",
  "severity": "P1", "title": "publish fixture finding",
  "problem": "the fixture proves the checked-in ledger reaches the published register",
  "evidence": [{"path": "scripts/ci/tests/publish_audit.sh", "line": 1,
                "quote": "#!/usr/bin/env bash"}],
  "failure_scenario": "publish-data.sh runs the audit without --findings and the register omits every audited finding",
  "acceptance": "the published register carries this package",
  "owner_project": "PLATFORM"}]}
JSON

cat > "$tmp/bin/conveyor" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
out=""
docs=""
conf=""
while [ $# -gt 0 ]; do
  case "$1" in
    --out) out="$2"; shift 2 ;;
    --outputs-dir) docs="$2"; shift 2 ;;
    --conformance) conf="$2"; shift 2 ;;
    *) shift ;;
  esac
done
mkdir -p "$out" "$docs"
cp "$FIXTURE"/*.json "$out/"
cp "$FIXTURE_DOCS/outputs.js" "$docs/"
# The real binary embeds the register the --conformance file names; the stub copies it so the
# register's publication is exercised end to end.
[ -z "$conf" ] || cp "$conf" "$out/rework.json"
SH
chmod +x "$tmp/bin/conveyor"
# publish-data.sh stamps the snapshot with the root's HEAD; the temp root is its own repo.
git -C "$tmp" init -q
git -C "$tmp" -c user.name=test -c user.email=test@dev.invalid -c core.hooksPath=/dev/null \
  commit --allow-empty -q -m "publish audit fixture"

export FIXTURE="$tmp/fixture" FIXTURE_DOCS="$tmp/fixture-docs"
export CONVEYOR_BIN="$tmp/bin/conveyor" HOMEPAGE="$tmp/homepage" TENANT=test

if CONFORMANCE_STRICT=1 "$tmp/docs/design/scripts/publish-data.sh" > "$tmp/strict.log" 2>&1; then
  echo "FAIL: strict publish succeeded with an empty baseline" >&2
  cat "$tmp/strict.log" >&2
  exit 1
fi
if [ -e "$tmp/docs/design/data/programme.json" ]; then
  echo "FAIL: strict publish wrote the pack before the audit passed" >&2
  exit 1
fi
if [ -e "$tmp/docs/design/data/rework.json" ]; then
  echo "FAIL: strict publish wrote the rework register before the audit passed" >&2
  exit 1
fi
if [ -e "$tmp/docs/design/data/standards.json" ]; then
  echo "FAIL: strict publish wrote the standards record before the audit passed" >&2
  exit 1
fi
grep -q 'FAIL new P0' "$tmp/strict.log" || {
  echo "FAIL: the strict failure did not name the new P0" >&2
  cat "$tmp/strict.log" >&2
  exit 1
}

CONFORMANCE_STRICT=0 "$tmp/docs/design/scripts/publish-data.sh" > "$tmp/report.log" 2>&1
test -f "$tmp/docs/design/data/programme.json" || {
  echo "FAIL: report-only publish did not copy the snapshot" >&2
  cat "$tmp/report.log" >&2
  exit 1
}
test -f "$tmp/docs/design/data/conformance.json" || {
  echo "FAIL: report-only publish did not copy the conformance register" >&2
  cat "$tmp/report.log" >&2
  exit 1
}
test -f "$tmp/docs/design/data/rework.json" || {
  echo "FAIL: report-only publish did not carry the rework register" >&2
  cat "$tmp/report.log" >&2
  exit 1
}
test -f "$tmp/docs/design/data/standards.json" || {
  echo "FAIL: report-only publish did not carry the standards record" >&2
  cat "$tmp/report.log" >&2
  exit 1
}
test -f "$tmp/docs/design/docs/standards/index.html" || {
  echo "FAIL: report-only publish did not carry the standards index" >&2
  cat "$tmp/report.log" >&2
  exit 1
}
python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); assert d["schema"]=="conveyor-rework/v1", d.get("schema")' \
  "$tmp/docs/design/data/rework.json" || {
  echo "FAIL: the published rework register is not conveyor-rework/v1" >&2
  exit 1
}
python3 - "$tmp/docs/design/data/rework.json" <<'PY' || {
import json, sys
register = json.load(open(sys.argv[1]))
titles = [package.get("title") for package in register.get("packages", [])]
assert "publish fixture finding" in titles, titles
PY
  echo "FAIL: the published register dropped the checked-in v1 findings ledger" >&2
  exit 1
}
# P1 — the docs index is derived from the INSTALLED conformance.md. The stub build-docs records
# the hash it read; rendering before the install would leave the pre-publish hash here.
python3 - "$tmp" <<'PY' || {
import hashlib, json, pathlib, sys
root = pathlib.Path(sys.argv[1])
index = json.loads((root / "docs/design/docs/documents.json").read_text(encoding="utf-8"))
installed = (root / "docs/design/data/conformance.md").read_bytes()
assert index.get("conformance_sha256") == hashlib.sha256(installed).hexdigest(), index
PY
  echo "FAIL: the docs index did not hash the installed conformance.md" >&2
  exit 1
}
echo "publish_audit: OK"
