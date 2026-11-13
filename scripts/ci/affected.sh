#!/usr/bin/env bash
# Map changed paths to the workspace packages they can affect.
#
#   bash scripts/ci/affected.sh --base origin/main --head HEAD
#   bash scripts/ci/affected.sh --files "apps/server/src/lib.rs Cargo.lock"
#   bash scripts/ci/affected.sh --files "..." --apps   # only packages under apps/
#   bash scripts/ci/affected.sh --files "..." --json   # JSON array
#
# Prints a space-separated list of package names (default) and exits 0. An empty diff prints
# nothing. A change to the workspace root, Cargo.lock, rust-toolchain.toml, .cargo/,
# docker/Dockerfile.service, scripts/ci/ or crates/platform/ selects every package.
set -euo pipefail
exec python3 "$(dirname "$0")/affected.py" "$@"
