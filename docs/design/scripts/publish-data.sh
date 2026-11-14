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

# 0. Markdown -> standalone HTML. The pack is published verbatim, so the pages must exist
#    here; check-docs.cjs gates the committed output without needing pandoc.
python3 "$pack/scripts/build-docs.py"

# 1. The issue registry is the only GitHub-derived input the snapshot needs. The programme seed
#    is an IMAGE INPUT (`include_str!`), so it changes only in a repository PR and never during a
#    publish. The plan seed is retired: execution state (boards, cards, runs) is never seeded, so
#    there is no plan document to regenerate. Refresh the registry only.
"$root/scripts/fetch-issues.sh" "$data/issues.json"

# 2. The rows come from the live database, not from a fixture.
mkdir -p "$data"
snapshot_args=(snapshot --tenant "$tenant" --out "$data" --issues "$data/issues.json")
snapshot_args+=(--outputs-dir "$pack/docs")
snapshot_args+=(--git-sha "$(git -C "$root" rev-parse HEAD)")
if [ -n "${SNAPSHOT_DATABASE_URL:-}" ]; then
  snapshot_args+=(--database-url "$SNAPSHOT_DATABASE_URL")
fi
if [ -n "${GENERATED_AT:-}" ]; then
  snapshot_args+=(--generated-at "$GENERATED_AT")
fi
"$conveyor" "${snapshot_args[@]}"

# 3. Mirror the pack (including data/ and docs/) and let the homepage's own gate decide.
# `issues.json` is an INPUT to the snapshot (the redacted registry is inside programme.json).
# It is not mirrored: the published data is the snapshot, not the raw GitHub export.
# `--delete` removes files that are no longer part of the pack (the retired v1 studies, stale
# generated pages); excluded paths are protected by rsync and survive the delete.
rsync -a --delete --exclude '*.md' --exclude 'scripts/' --exclude 'issues.json' --exclude 'legacy/' "$pack/" "$homepage/design/"
echo "publish-data: mirrored $pack → $homepage/design (runtime files + data/)"
echo "publish-data: now run (cd $homepage && make verify), review the diff, and open the homepage PR"
