#!/usr/bin/env bash
# Publish the MSP/TOGAF/SAFe/PMP artefact record and one page per artefact from the COMMITTED
# snapshot in docs/design/data. No database and no wall clock: the record's `generated_at` and
# `git_sha` come from the snapshot manifest, so two runs are byte-identical.
#
# This is the committed-pack half of the publish. The live publish (`publish-data.sh`) additionally
# refreshes the snapshot itself from the database and embeds the register in `snapshot.js`.
#
# The REWORK REGISTER is the live publish's object, not this script's: this pack only re-serialises
# `docs/design/data/rework.json` into `rework.js`. Deriving it here from the snapshot would make the
# pack disagree with the published register the moment the issue-driven seed moves (measured on the
# automated refresh #665, 2026-11-24), which reds every refresh's design-pack until a live publish.
#
# Run:
#   docs/design/scripts/publish-standards.sh          # write the pack
#   docs/design/scripts/publish-standards.sh --check  # build in temp and compare; write nothing
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
pack="$root/docs/design"
mode="${1:-}"
case "$mode" in
  ""|"--check") ;;
  *)
    echo "publish-standards: unknown argument '$mode' (only --check is accepted)" >&2
    exit 2
    ;;
esac
export TZ=UTC
export PYTHONHASHSEED=0
cd "$root"

# The same relative arguments in every run: the register records the snapshot label it was built
# with, and an absolute temp path would make two identical builds differ.
snapshot="docs/design/data"
outputs="docs/design/docs"
findings_ledger="docs/evidence/standards-findings.json"

# The record's provenance comes from the snapshot manifest; without it the generator would fall
# back to an epoch timestamp and an empty commit and publish a record that names no snapshot.
test -f "$snapshot/manifest.json" || {
  echo "publish-standards: no snapshot manifest at $snapshot/manifest.json" >&2
  exit 1
}

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/audit" "$tmp/data" "$tmp/standards"

# 1. The conformance gap register on the committed snapshot.
python3 scripts/audit-programme-conformance.py \
  --snapshot "$snapshot" --out "$tmp/audit" --report >/dev/null
test -f "$tmp/audit/conformance-gap-register.json" || {
  echo "publish-standards: the audit produced no gap register" >&2
  exit 1
}

# 2. Pass 1: every unresolved mandatory artefact becomes one finding. The file is an input to the
#    register below, so the record and the register agree about every package.
python3 scripts/generate-standards.py \
  --snapshot "$snapshot" --outputs-dir "$outputs" \
  --gaps "$tmp/audit/conformance-gap-register.json" \
  --findings-out "$tmp/standards-findings.json" >/dev/null

# 3. The register is the LIVE publish's object. `publish-data.sh` writes
#    docs/design/data/rework.json only after its strict gate has audited the live rows; this
#    committed-pack publish re-serialises exactly that object and never derives it again. A derived
#    register would follow the issue-driven seed and disagree with the published one the moment a
#    unit closes.
register="$pack/data/rework.json"
test -f "$register" || {
  echo "publish-standards: no published register at $register; run publish-data.sh" >&2
  exit 1
}

# 4. Pass 2: the record, one page per artefact and the register global. The generator writes
#    `standards.json`, `standards.js` and `rework.js`; the canonical register stays JSON.
python3 scripts/generate-standards.py \
  --snapshot "$snapshot" --outputs-dir "$outputs" \
  --gaps "$tmp/audit/conformance-gap-register.json" \
  --register "$register" \
  --data-out "$tmp/data" --html-out "$tmp/standards" \
  --findings-out "$tmp/standards-findings.json" >/dev/null
test -f "$tmp/data/standards.json" || {
  echo "publish-standards: the generator produced no record" >&2
  exit 1
}
test -f "$tmp/data/rework.js" || {
  echo "publish-standards: the generator produced no rework global" >&2
  exit 1
}
test -f "$tmp/standards/index.html" || {
  echo "publish-standards: the generator produced no artefact index" >&2
  exit 1
}

if [ "$mode" = "--check" ]; then
  stale() {
    echo "publish-standards: $1 is stale or missing; run docs/design/scripts/publish-standards.sh" >&2
    exit 1
  }
  cmp -s "$tmp/data/standards.json" "$pack/data/standards.json" || stale "$pack/data/standards.json"
  cmp -s "$tmp/data/standards.js" "$pack/data/standards.js" || stale "$pack/data/standards.js"
  cmp -s "$tmp/data/rework.js" "$pack/data/rework.js" || stale "$pack/data/rework.js"
  cmp -s "$tmp/standards-findings.json" "$findings_ledger" || stale "$findings_ledger"
  diff -r "$tmp/standards" "$pack/docs/standards" >/dev/null || stale "$pack/docs/standards"
  echo "publish-standards: the committed record, pages and register are current"
  exit 0
fi

# 5. Install only after every step passed. The record and the ledgers go first and the page tree
#    last: a failure while copying the small files leaves the old pages beside the old record (a
#    stale pack the gate refuses), never new pages beside an old register. The standards directory
#    is replaced wholesale: a page for a retired artefact must not survive. The published register
#    (`rework.json`) and its ledger are NOT written here — the live publish owns both.
mkdir -p "$pack/data" "$pack/docs/standards" "$(dirname "$findings_ledger")"
cp "$tmp/data/standards.json" "$tmp/data/standards.js" "$tmp/data/rework.js" "$pack/data/"
cp "$tmp/standards-findings.json" "$findings_ledger"
rsync -a --delete "$tmp/standards/" "$pack/docs/standards/"
echo "publish-standards: record, pages and the register global published from $snapshot"
