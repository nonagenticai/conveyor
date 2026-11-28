#!/usr/bin/env bash
# The published MSP/TOGAF/SAFe/PMP record and the rework register are the LIVE PUBLISH's objects
# (`docs/design/scripts/publish-data.sh`). This test validates the COMMITTED pack those bytes
# describe: it never re-derives the register or the record from the snapshot and compares it to the
# pack, because the snapshot follows the issue-driven seed and a branch that closes one issue would
# redden the pack by construction (measured on the automated refresh #665 and on #670). The
# generators' determinism is checked as a self-test below: two runs over the same inputs must agree,
# and nothing compares that output to the committed pack.
#
# Run: bash scripts/ci/tests/standards_pack.sh
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
export TZ=UTC
export PYTHONHASHSEED=0
cd "$repo"

record="docs/design/data/standards.json"
register="docs/design/data/rework.json"
ledger="docs/evidence/rework-register.json"
findings="docs/evidence/standards-findings.json"

for path in "$record" "$register" "$ledger" "$findings" docs/design/docs/standards/index.html; do
  test -f "$path" || {
    echo "standards_pack: $path is missing; the pack must carry the published record and register" >&2
    exit 1
  }
done

# The record's schema, page hashes and package linkage are checked over the committed tree. The
# register keys are read from the committed register, so every unresolved mandatory artefact must
# name a package the published pack actually carries.
node docs/design/scripts/check-standards.cjs --data docs/design/data --documents docs/design

# The committed register must be consumable at head: a dry run plans cards against the committed
# snapshot, with no database and no write.
python3 scripts/materialise-rework.py \
  --register "$register" --data docs/design/data \
  --receipt "$tmp/receipt.json" --severity P0,P1,P2,P3 | tee "$tmp/materialise.log"
grep -Eq 'materialise-rework: [0-9]+ to create' "$tmp/materialise.log"

python3 - "$record" "$register" "$ledger" "$findings" <<'PY'
import json
import sys


def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


record = load(sys.argv[1])
register = load(sys.argv[2])
ledger = load(sys.argv[3])
findings = load(sys.argv[4])

# The published register travels in two committed carriers; they are the same object. Canonical
# JSON equality, not bytes: `data/rework.json` is the live snapshot's serialisation and the
# evidence ledger is the audit's (sorted keys, escaped non-ASCII); `check-data.cjs` compares them
# the same way.
assert register == ledger, (
    "docs/design/data/rework.json and docs/evidence/rework-register.json differ")

summary = record["summary"]
artefacts = record["artefacts"]
assert record["schema"] == "design-standards/v1", record.get("schema")
assert summary["artefacts"] == len(artefacts)
assert summary["artefacts"] == summary["evidenced"] + summary["partial"] + summary["absent"]
assert findings.get("schema") == "conveyor-v1-findings/v1", findings.get("schema")
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

def missing_required(artefact):
    sections = artefact.get("sections") or []
    if not sections:
        return artefact.get("status") != "evidenced"
    return any(
        section.get("required", True) is not False and section.get("status") != "evidenced"
        for section in sections
    )


# A scheduled or excluded artefact is not work yet: it produces no finding, so it owes no
# package. The page still carries it and its status, so the future tranche stays visible.
unresolved = [
    a for a in artefacts
    if a.get("mandatory") and missing_required(a) and a.get("due_state", "due") == "due"
]
for artefact in unresolved:
    assert artefact.get("package_published") is True, f"{artefact['id']}: no published package"
    assert artefact.get("package_key") in set(keys), (
        f"{artefact['id']}: {artefact.get('package_key')} is not in the register")

print(
    "standards_pack: "
    f"{summary['artefacts']} artefacts "
    f"({summary['evidenced']} evidenced, {summary['partial']} partial, {summary['absent']} absent), "
    f"{summary['findings']} findings, {len(packages)} packages "
    f"({register['summary']['open']} open); the committed record, register and findings agree"
)
PY

# Determinism self-test: the generators must be deterministic over the same inputs. This is the
# only derivation in this test; its output is compared run-to-run, never to the committed pack.
build() {
  local run="$1"
  mkdir -p "$run/audit" "$run/data" "$run/docs/standards"
  python3 scripts/audit-programme-conformance.py \
    --snapshot docs/design/data --out "$run/audit" --report >/dev/null
  python3 scripts/generate-standards.py \
    --snapshot docs/design/data --outputs-dir docs/design/docs \
    --gaps "$run/audit/conformance-gap-register.json" \
    --findings-out "$run/standards-pass1.json" >/dev/null
  # The record is derived with the PUBLISHED register (the live publish's object), never a
  # second register derived from the snapshot.
  python3 scripts/generate-standards.py \
    --snapshot docs/design/data --outputs-dir docs/design/docs \
    --gaps "$run/audit/conformance-gap-register.json" \
    --register "$register" \
    --data-out "$run/data" --html-out "$run/docs/standards" \
    --findings-out "$run/standards-findings.json" >/dev/null
  # The pass-2 record must carry the same findings pass 1 produced.
  cmp "$run/standards-pass1.json" "$run/standards-findings.json"
}

build "$tmp/run1"
build "$tmp/run2"

cmp "$tmp/run1/standards-findings.json" "$tmp/run2/standards-findings.json"
diff -r "$tmp/run1/data" "$tmp/run2/data"
diff -r "$tmp/run1/docs" "$tmp/run2/docs"

echo "standards_pack: OK — the committed pack is a valid published record and register"
