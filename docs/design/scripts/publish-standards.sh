#!/usr/bin/env bash
# Publish the MSP/TOGAF/SAFe/PMP artefact record and one page per artefact from the COMMITTED
# snapshot in docs/design/data. No database and no wall clock: the record's `generated_at` and
# `git_sha` come from the snapshot manifest, so two runs are byte-identical.
#
# Write mode rehashes the installed manifest after it installs the record (`$CONVEYOR_BIN
# snapshot --manifest-only`, which reads no database and no clock); `--check` needs no binary.
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

# 3b. WRITE GUARD — the record must not outrun the published register. A snapshot whose
#     issue-driven seed has moved past the last live publish derives a different register; writing
#     the record here would ship a record that names packages the register lacks. `--check` skips
#     this on purpose: the committed pack belongs to the live publish, so a moved seed is not pack
#     staleness (proven on the refresh #665 tree).
if [ "$mode" = "" ]; then
  python3 scripts/audit-programme-conformance.py \
    --snapshot "$snapshot" --out "$tmp/audit" \
    --findings docs/evidence/v1-conformance-findings.json \
    --findings "$tmp/standards-findings.json" \
    --deep-review docs/evidence/record-w3/deep-review-findings.json \
    --deep-review docs/evidence/record-w4/deep-review-findings.json \
    --deep-review docs/evidence/record-w5/deep-review-findings.json \
    --rework-out "$tmp/derived-rework.json" >/dev/null
  # The guard's contract is the one the message names: the record must not name a package the
  # published register lacks. Exact equality cannot hold — the live register embeds the temp
  # snapshot it was derived from and is taken before this publish's own seed/coverage fixpoint —
  # so the check is the subset that actually protects the record. Extra published packages are
  # the live object's, not this build's business.
  python3 - "$tmp/derived-rework.json" "$register" <<'PY'
import json
import sys

derived = json.load(open(sys.argv[1], encoding="utf-8"))
published = json.load(open(sys.argv[2], encoding="utf-8"))
derived_keys = {p.get("key") for p in derived.get("packages") or []}
published_keys = {p.get("key") for p in published.get("packages") or []}
missing = sorted(derived_keys - published_keys)
if missing:
    print(
        "publish-standards: the snapshot derives packages the published register lacks: "
        + ", ".join(missing[:10]),
        file=sys.stderr,
    )
    sys.exit(1)
PY
fi

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
# The files just installed change `data/standards.json`, `data/standards.js` and `data/rework.js`;
# the manifest the live publish hashed now names the old bytes, and check-data refuses a data file
# whose hash differs. Rehash the installed pack. `snapshot --manifest-only` reads no database and
# no clock — it only walks the pack — so the write stays deterministic; the binary is the one
# publish-data.sh already required.
test -n "${CONVEYOR_BIN:-}" || {
  echo "publish-standards: set CONVEYOR_BIN so the installed manifest can be rehashed" >&2
  exit 1
}
"$CONVEYOR_BIN" snapshot --manifest-only --out "$pack/data"
echo "publish-standards: record, pages and the register global published from $snapshot"
