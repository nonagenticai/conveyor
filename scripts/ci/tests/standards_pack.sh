#!/usr/bin/env bash
# The published MSP/TOGAF/SAFe/PMP record and the rework register are built from the COMMITTED
# pack, not from a fixture: every unresolved mandatory section must name a package the register
# actually carries, every package must be individually acceptable, and two runs must be
# byte-identical. This is the gate the pack never had: without it, an absent record passed CI.
#
# Run: bash scripts/ci/tests/standards_pack.sh
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
export TZ=UTC
export PYTHONHASHSEED=0
cd "$repo"

# The same relative --snapshot argument in both runs: the register records the string it was
# built with, so an absolute path would make two identical builds differ.
build() {
  local run="$1"
  mkdir -p "$run/audit" "$run/data" "$run/docs/standards"
  python3 scripts/audit-programme-conformance.py \
    --snapshot docs/design/data --out "$run/audit" --report >/dev/null
  python3 scripts/generate-standards.py \
    --snapshot docs/design/data --outputs-dir docs/design/docs \
    --gaps "$run/audit/conformance-gap-register.json" \
    --findings-out "$run/standards-pass1.json" >/dev/null
  python3 scripts/audit-programme-conformance.py \
    --snapshot docs/design/data --out "$run/audit" \
    --findings docs/evidence/v1-conformance-findings.json \
    --findings "$run/standards-pass1.json" \
    --rework-out "$run/rework.json" >/dev/null
  python3 scripts/generate-standards.py \
    --snapshot docs/design/data --outputs-dir docs/design/docs \
    --gaps "$run/audit/conformance-gap-register.json" \
    --register "$run/rework.json" \
    --data-out "$run/data" --html-out "$run/docs/standards" \
    --findings-out "$run/standards-findings.json" >/dev/null
  # The pass-2 record must carry the same findings pass 1 fed to the register.
  cmp "$run/standards-pass1.json" "$run/standards-findings.json"
}

build "$tmp/run1"
build "$tmp/run2"

# Determinism: the same committed pack produces the same record, pages and register.
cmp "$tmp/run1/standards-findings.json" "$tmp/run2/standards-findings.json"
cmp "$tmp/run1/rework.json" "$tmp/run2/rework.json"
diff -r "$tmp/run1/data" "$tmp/run2/data"
diff -r "$tmp/run1/docs" "$tmp/run2/docs"

# The committed ledger is the record of this snapshot, not a stale side note.
cmp "$tmp/run1/standards-findings.json" docs/evidence/standards-findings.json

# The pack must CARRY the record, not merely be able to produce it. Without these comparisons a
# green run proves only that a temp root could be populated, which is exactly how the artefact set
# stayed absent from the public pack while CI passed.
stale() {
  echo "standards_pack: $1 is stale or missing; run docs/design/scripts/publish-standards.sh" >&2
  exit 1
}
cmp "$tmp/run1/data/standards.json" docs/design/data/standards.json || stale "docs/design/data/standards.json"
cmp "$tmp/run1/data/standards.js" docs/design/data/standards.js || stale "docs/design/data/standards.js"
cmp "$tmp/run1/data/rework.js" docs/design/data/rework.js || stale "docs/design/data/rework.js"
cmp "$tmp/run1/rework.json" docs/evidence/rework-register.json || stale "docs/evidence/rework-register.json"
diff -r "$tmp/run1/docs/standards" docs/design/docs/standards >/dev/null || stale "docs/design/docs/standards"

# The record's schema, page hashes and package linkage are checked over the generated tree.
node docs/design/scripts/check-standards.cjs --data "$tmp/run1/data" --documents "$tmp/run1"

# The register must be consumable at head: a dry run plans cards against the committed snapshot,
# with no database and no write.
python3 scripts/materialise-rework.py \
  --register "$tmp/run1/rework.json" --data docs/design/data \
  --receipt "$tmp/receipt.json" --severity P0,P1,P2,P3 | tee "$tmp/materialise.log"
grep -Eq 'materialise-rework: [0-9]+ to create' "$tmp/materialise.log"

python3 - "$tmp/run1" <<'PY'
import json
import pathlib
import sys

run = pathlib.Path(sys.argv[1])
record = json.loads((run / "data" / "standards.json").read_text(encoding="utf-8"))
register = json.loads((run / "rework.json").read_text(encoding="utf-8"))
findings = json.loads((run / "standards-findings.json").read_text(encoding="utf-8"))

summary = record["summary"]
artefacts = record["artefacts"]
assert record["schema"] == "design-standards/v1", record.get("schema")
assert summary["artefacts"] == len(artefacts)
assert summary["artefacts"] == summary["evidenced"] + summary["partial"] + summary["absent"]
assert summary["findings"] == len(findings["findings"]), (
    summary["findings"], len(findings["findings"]))

packages = register.get("packages") or []
keys = [str(package.get("key")) for package in packages]
assert register.get("schema") == "conveyor-rework/v1", register.get("schema")
assert len(keys) == len(set(keys)), "the register repeats a package key"
assert register["summary"]["packages"] == len(packages)
for package in packages:
    key = str(package.get("key"))
    for field in ("key", "rule", "title", "acceptance_command"):
        assert str(package.get(field) or "").strip(), f"{key}: empty {field}"
    assert package.get("severity") in {"P0", "P1", "P2", "P3"}, f"{key}: {package.get('severity')}"
    assert package.get("dod"), f"{key}: no Definition of Done"
    assert key in str(package["acceptance_command"]), f"{key}: acceptance does not name its package"
    assert package.get("evidence"), f"{key}: no evidence"

unresolved = [a for a in artefacts if a.get("mandatory") and a.get("status") != "evidenced"]
for artefact in unresolved:
    assert artefact.get("package_published") is True, f"{artefact['id']}: no published package"
    assert artefact.get("package_key") in set(keys), (
        f"{artefact['id']}: {artefact.get('package_key')} is not in the register")

print(
    "standards_pack: "
    f"{summary['artefacts']} artefacts "
    f"({summary['evidenced']} evidenced, {summary['partial']} partial, {summary['absent']} absent), "
    f"{summary['findings']} findings, {len(packages)} packages "
    f"({register['summary']['open']} open); the record and the register are deterministic"
)
PY

echo "standards_pack: OK"
