#!/usr/bin/env bash
# The public local gate: the same checks the repository's CI runs.
#
#   bash scripts/ci/gate.sh
#   bash scripts/ci/gate.sh --print-route     # print the build route and exit
#
# Requires: the toolchain from rust-toolchain.toml, cargo-nextest, and Docker with a running
# daemon. cargo-audit and betterleaks are optional and reported as skipped when absent.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

need() {
  command -v "$1" >/dev/null 2>&1 || { echo "missing $1: $2" >&2; exit 1; }
}

# --- build route ----------------------------------------------------------------------
# A bare `cargo` gives every worktree its own multi-GB `target/` (48 GB measured on
# `.worktrees/issue-366`, 2026-11-18). The sanctioned local route is `cargo-guard.py`: one
# `<repo>/target/claude` per repository (derived from `--git-common-dir`, so every worktree
# and session shares it), a free-disk floor and bounded build slots. `--print-target` is the
# guard's own way for a repo script to find that target. Without the guard the gate still
# uses the same one and says so — it never silently builds a second cache.
GUARD="${CARGO_GUARD:-$HOME/.claude/scripts/cargo-guard.py}"
guarded=0
if [ -x "$GUARD" ] && TARGET="$("$GUARD" --print-target 2>/dev/null)"; then
  CARGO_RUN=("$GUARD")
  guarded=1
  ROUTE="cargo-guard (free-disk floor + build slots)"
elif common="$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null)"; then
  TARGET="$(dirname "$common")/target/claude"
  CARGO_RUN=(cargo)
  ROUTE="plain cargo — cargo-guard NOT installed: no disk floor, no build slots"
else
  echo "cannot resolve the repository root; run this inside the git checkout" >&2
  exit 1
fi
export CARGO_TARGET_DIR="$TARGET"
echo "==> build route: $ROUTE; target $TARGET"

if [ "${1:-}" = "--print-route" ]; then
  exit 0
fi

need cargo "install Rust from https://rustup.rs"

# Check the guard's floor before any service starts: this is the cheapest guarded command
# and it compiles nothing.
if [ "$guarded" = 1 ]; then
  "${CARGO_RUN[@]}" metadata --no-deps --format-version 1 >/dev/null
fi

need docker "install Docker and start the daemon"
command -v cargo-nextest >/dev/null 2>&1 \
  || { echo "missing cargo-nextest: cargo install cargo-nextest --locked" >&2; exit 1; }

PG=""
RD=""
cleanup() {
  [ -n "$PG" ] && docker rm -f "$PG" >/dev/null 2>&1 || true
  [ -n "$RD" ] && docker rm -f "$RD" >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "==> starting Postgres 18 and Redis on random local ports"
PG=$(docker run -d --rm -e POSTGRES_PASSWORD=postgres --tmpfs /var/lib/postgresql \
  -p 127.0.0.1::5432 postgres:18 -c fsync=off -c max_connections=300)
RD=$(docker run -d --rm --tmpfs /data -p 127.0.0.1::6379 library/redis:8.10.1-alpine)

for _ in $(seq 1 60); do
  docker exec "$PG" pg_isready -h 127.0.0.1 -U postgres >/dev/null 2>&1 && break
  sleep 1
done
docker exec "$PG" pg_isready -h 127.0.0.1 -U postgres >/dev/null

for _ in $(seq 1 60); do
  docker exec "$RD" redis-cli ping >/dev/null 2>&1 && break
  sleep 1
done
docker exec "$RD" redis-cli ping >/dev/null

PGPORT=$(docker port "$PG" 5432/tcp | head -1 | sed 's/.*://')
RDPORT=$(docker port "$RD" 6379/tcp | head -1 | sed 's/.*://')
export TEST_DATABASE_URL="postgres://postgres:postgres@127.0.0.1:${PGPORT}/postgres"
export REDIS_URL="redis://127.0.0.1:${RDPORT}/"
echo "    TEST_DATABASE_URL=$TEST_DATABASE_URL"
echo "    REDIS_URL=$REDIS_URL"

echo "==> cargo fmt"
cargo fmt --all -- --check

echo "==> cargo clippy"
"${CARGO_RUN[@]}" clippy --all-targets --locked -- -D warnings

echo "==> cargo audit"
if command -v cargo-audit >/dev/null 2>&1; then
  cargo audit
else
  echo "    skip: cargo-audit not installed (cargo install cargo-audit --locked)"
fi

echo "==> cargo nextest"
"${CARGO_RUN[@]}" nextest run --no-tests=fail --color never --locked
"${CARGO_RUN[@]}" test --doc --locked

echo "==> egress allowlist (S09 property f)"
python3 scripts/check-egress.py --check

echo "==> no private references"
bash scripts/ci/no-private-refs.sh

echo "==> betterleaks"
if command -v betterleaks >/dev/null 2>&1; then
  # The SAME scanner, config and ignore file as `ci-public.yml`'s secret-scan job. gitleaks was
  # retired: it is a different scanner with different defaults, so a green gitleaks run said
  # nothing about the gate that actually blocks a merge.
  #
  # ⛔ --config IS REQUIRED. betterleaks only auto-loads a target's own config under
  # --unsafe-auto-load-config, so a bare `betterleaks dir` scans with the BUILT-IN defaults — which
  # are not gitleaks' defaults: measured on this tree, 605 findings (600 generic-credential-uri on
  # local dev DSNs, 5 generic-password on compose placeholders), none of them real.
  #
  # ⛔ TRACKED FILES ONLY. CI scans a clean checkout, so it only ever sees what git tracks.
  # Scanning `.` here also walks `target/` — 1.57 GB of build output, 12 findings measured
  # 2026-11-15, every one a binary that happened to match a rule. A local gate that disagrees with
  # CI is worse than no gate: it trains you to ignore the step.
  git ls-files -z | xargs -0 -r betterleaks dir --redact --exit-code 1 --no-banner \
    --config=.betterleaks.toml --gitleaks-ignore-path .betterleaksignore --
else
  echo "    skip: betterleaks not installed (https://github.com/betterleaks/betterleaks)"
fi

echo "gate: OK"
