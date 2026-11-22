#!/usr/bin/env bash
# Regenerate the design-site data and mirror the pack into the homepage repo.
#
# This is the publish path, not a build: it assumes a `conveyor` binary built from this tree
# (`CONVEYOR_BIN`, default `conveyor`), a read-only database URL (`SNAPSHOT_DATABASE_URL`, or
# `DATABASE_URL`; the query transaction is READ ONLY) and a checkout of `nonagentic_homepage`
# (`HOMEPAGE`, default ~/Movies/nonagentic_homepage). It never commits or pushes; it leaves the
# homepage tree staged for `make verify` and a human review.
#
# Usage:
#   CONVEYOR_BIN=/path/to/conveyor SNAPSHOT_DATABASE_URL=postgres://… docs/design/scripts/publish-data.sh
#   TENANT=nonagentic GENERATED_AT=2026-11-12T00:00:00Z docs/design/scripts/publish-data.sh
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
pack="$root/docs/design"
homepage="${HOMEPAGE:-$HOME/Movies/nonagentic_homepage}"
conveyor="${CONVEYOR_BIN:-conveyor}"
tenant="${TENANT:-nonagentic}"
data="$pack/data"

if ! command -v "$conveyor" >/dev/null 2>&1 && [ ! -x "$conveyor" ]; then
  echo "publish-data: no conveyor binary ($conveyor); set CONVEYOR_BIN" >&2
  exit 1
fi
if [ ! -d "$homepage/design" ]; then
  echo "publish-data: no homepage design dir at $homepage/design; set HOMEPAGE" >&2
  exit 1
fi

# The snapshot's evidence index is relative to the repo root, so run from there.
cd "$root"

# The pack is assembled in a temp directory and copied in only after the conformance audit.
# A failed audit therefore leaves the published data and output pages untouched.
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/data" "$tmp/docs" "$tmp/conformance"

# 1. The issue registry is the only GitHub-derived input the snapshot needs. The programme seed
#    is an IMAGE INPUT (`include_str!`), so it changes only in a repository PR and never during a
#    publish. The plan seed is retired: execution state (boards, cards, runs) is never seeded, so
#    there is no plan document to regenerate. Refresh the registry only.
"$root/scripts/fetch-issues.sh" "$tmp/data/issues.json"

# 2. The rows come from the live database, not from a fixture.
snapshot_args=(snapshot --tenant "$tenant" --out "$tmp/data" --issues "$tmp/data/issues.json")
snapshot_args+=(--outputs-dir "$tmp/docs")
snapshot_args+=(--git-sha "$(git -C "$root" rev-parse HEAD)")
if [ -n "${SNAPSHOT_DATABASE_URL:-}" ]; then
  snapshot_args+=(--database-url "$SNAPSHOT_DATABASE_URL")
fi
if [ -n "${GENERATED_AT:-}" ]; then
  snapshot_args+=(--generated-at "$GENERATED_AT")
fi
"$conveyor" "${snapshot_args[@]}"

# 3. The conformance gap register runs on the draft BEFORE anything is published. It is the
#    authorisation ledger the artefact generator dedupes against: a gap owns its package, so the
#    same defect never becomes a second card.
audit_args=(--snapshot "$tmp/data" --out "$tmp/conformance")
python3 "$root/scripts/audit-programme-conformance.py" "${audit_args[@]}" --report
test -f "$tmp/conformance/conformance-gap-register.json" || {
  echo "publish-data: the audit produced no gap register" >&2
  exit 1
}

# 3a. Pass 1 of the standards generator: every mandatory MSP/TOGAF/SAFe/PMP artefact is resolved
#     against the draft snapshot, its documents and the gap register; only an unresolved section
#     becomes a finding. This file is an INPUT to the register below.
python3 "$root/scripts/generate-standards.py" \
  --snapshot "$tmp/data" --outputs-dir "$tmp/docs" \
  --gaps "$tmp/conformance/conformance-gap-register.json" \
  --findings-out "$tmp/conformance/standards-findings.json" >/dev/null
test -f "$tmp/conformance/standards-findings.json" || {
  echo "publish-data: the standards generator produced no findings" >&2
  exit 1
}

# 3b. The register merges the live gaps, the checked-in v1 findings ledger and the artefact
#     findings. A failed audit leaves the published data and output pages untouched.
python3 "$root/scripts/audit-programme-conformance.py" "${audit_args[@]}" \
  --findings "$root/docs/evidence/v1-conformance-findings.json" \
  --findings "$tmp/conformance/standards-findings.json" \
  --report --rework-out "$tmp/conformance/rework.json"
test -f "$tmp/conformance/rework.json" || {
  echo "publish-data: the audit produced no rework register" >&2
  exit 1
}

# The live register is public: evidence quotes code and plans, and must never carry a private
# address. Redact before it is embedded in the snapshot, so the pack and the committed register
# are the same document. publish-standards.sh does the same for the committed-pack half.
python3 - "$tmp/conformance/rework.json" <<'PY'
import json, pathlib, re, sys
path = pathlib.Path(sys.argv[1])
register = json.loads(path.read_text(encoding="utf-8"))
def redact(value):
    if isinstance(value, str):
        return re.sub(r"\b(?:\d{1,3}\.){3}\d{1,3}\b", "[redacted-ip]", value)
    if isinstance(value, list):
        return [redact(item) for item in value]
    if isinstance(value, dict):
        return {key: redact(item) for key, item in value.items()}
    return value
path.write_text(
    json.dumps(redact(register), indent=2, sort_keys=True, ensure_ascii=False) + "\n",
    encoding="utf-8",
)
PY

# 3b. The FINAL snapshot carries that register (`rework.json` and `snapshot.js.rework`), and the
#     strict gate below audits THESE bytes — not the draft's.
"$conveyor" "${snapshot_args[@]}" --conformance "$tmp/conformance/rework.json"
test -f "$tmp/data/rework.json" || {
  echo "publish-data: the snapshot did not publish the rework register" >&2
  exit 1
}

# 3c. Strict is the default: a new or worsened P0/P1, an expired ledger entry, or a ledger entry
#     with no live gap fails the publish. The final gap register is what gets published.
if [ "${CONFORMANCE_STRICT:-1}" = "1" ]; then
  python3 "$root/scripts/audit-programme-conformance.py" "${audit_args[@]}" \
    --strict --fail-on P0,P1 --baseline "$root/docs/evidence/conformance-baseline.json"
else
  python3 "$root/scripts/audit-programme-conformance.py" "${audit_args[@]}" --report
  echo "publish-data: conformance audit is report-only (CONFORMANCE_STRICT=0 was set)"
fi
cp "$tmp/conformance/conformance-gap-register.json" "$tmp/data/conformance.json"
cp "$tmp/conformance/conformance-gap-register.md" "$tmp/data/conformance.md"

# 3d. Pass 2 of the standards generator writes the published artefact record and one page per
#     artefact. It runs only after the strict gate, so the register it names is the one the gate
#     saw and a failed publish never writes a standards page.
python3 "$root/scripts/generate-standards.py" \
  --snapshot "$tmp/data" --outputs-dir "$tmp/docs" \
  --gaps "$tmp/conformance/conformance-gap-register.json" \
  --register "$tmp/data/rework.json" \
  --data-out "$tmp/data" --html-out "$tmp/docs/standards" \
  --findings-out "$root/docs/evidence/standards-findings.json" >/dev/null
test -f "$tmp/data/standards.json" || {
  echo "publish-data: the standards generator produced no record" >&2
  exit 1
}
test -f "$tmp/docs/standards/index.html" || {
  echo "publish-data: the standards generator produced no index" >&2
  exit 1
}

# 3e. Markdown -> standalone HTML. This writes the pack's document pages, so it runs only after
#     the gate: a failed publish leaves both the data and the documents untouched.
python3 "$pack/scripts/build-docs.py"

# 3f. The snapshot wrote its manifest before every generator above, so their files (the
#     conformance register, the standards record, the documents) are not hashed in it. Finalise
#     the finished pack: rehash the directory as it now stands and rewrite manifest.json.
#     Nothing is regenerated, and no database is read.
"$conveyor" snapshot --manifest-only --out "$tmp/data"

# 4. Only now does the fresh pack replace the published one. The standards directory is replaced
#    wholesale: a page for an artefact that no longer exists must not survive the publish.
rsync -a --delete "$tmp/docs/standards/" "$pack/docs/standards/"
# The data directory is replaced wholesale too: a file a later generator stopped writing must
# not survive as an unlisted (therefore unhashed) file beside the manifest.
rsync -a --delete "$tmp/data/" "$data/"
rsync -a "$tmp/docs/" "$pack/docs/"
# The canonical register is an evidence ledger as well as a published row: the committed-pack
# publish (`publish-standards.sh`) compares its build against this copy.
cp "$tmp/conformance/rework.json" "$root/docs/evidence/rework-register.json"

# 5. Mirror the pack (including data/ and docs/) and let the homepage's own gate decide.
# `issues.json` is an INPUT to the snapshot (the redacted registry is inside programme.json).
# It is not mirrored: the published data is the snapshot, not the raw GitHub export.
# `--delete` removes files that are no longer part of the pack (the retired v1 studies, stale
# generated pages); excluded paths are protected by rsync and survive the delete.
rsync -a --delete --exclude '*.md' --exclude 'scripts/' --exclude 'issues.json' --exclude 'legacy/' "$pack/" "$homepage/design/"
echo "publish-data: mirrored $pack → $homepage/design (runtime files + data/)"
echo "publish-data: now run (cd $homepage && make verify), review the diff, and open the homepage PR"
