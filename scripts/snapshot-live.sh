#!/usr/bin/env bash
# Build a snapshot from the live database, read-only, through the branch-built binary.
#
# Usage: scripts/snapshot-live.sh OUT_DIR
#
# The snapshot is the product's own projection of the live rows: `conveyor snapshot` opens a
# READ ONLY transaction and writes deterministic JSON under OUT_DIR/data plus the output pages
# under OUT_DIR/docs. `programme-record.py --snapshot OUT_DIR/data --snapshot-command
# "scripts/snapshot-live.sh OUT_DIR"` runs this after a write so the read-back sees the row.
#
# Environment:
#   SNAPSHOT_DATABASE_URL   required; a read-only URL for the primary (never commit it)
#   PROGRAMME_RECORD_BIN    required; the branch-built `conveyor` binary
#   TENANT                  default nonagentic
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="${1:?usage: snapshot-live.sh OUT_DIR}"
: "${SNAPSHOT_DATABASE_URL:?SNAPSHOT_DATABASE_URL is required}"
: "${PROGRAMME_RECORD_BIN:?PROGRAMME_RECORD_BIN is required}"
tenant="${TENANT:-nonagentic}"

mkdir -p "$out/data" "$out/docs"
exec "$PROGRAMME_RECORD_BIN" snapshot \
  --tenant "$tenant" \
  --out "$out/data" \
  --outputs-dir "$out/docs" \
  --git-sha "$(git -C "$root" rev-parse HEAD)" \
  --issues docs/design/data/issues.json \
  --database-url "$SNAPSHOT_DATABASE_URL"
