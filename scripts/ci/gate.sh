#!/usr/bin/env bash
# The public local gate: the same checks the repository's CI runs.
#
#   bash scripts/ci/gate.sh
#
# Requires: the toolchain from rust-toolchain.toml, cargo-nextest, and Docker with a running
# daemon. cargo-audit and gitleaks are optional and reported as skipped when absent.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

need() {
  command -v "$1" >/dev/null 2>&1 || { echo "missing $1: $2" >&2; exit 1; }
}
need docker "install Docker and start the daemon"
need cargo "install Rust from https://rustup.rs"
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
cargo clippy --all-targets --locked -- -D warnings

echo "==> cargo audit"
if command -v cargo-audit >/dev/null 2>&1; then
  cargo audit
else
  echo "    skip: cargo-audit not installed (cargo install cargo-audit --locked)"
fi

echo "==> cargo nextest"
cargo nextest run --no-tests=fail --color never --locked
cargo test --doc --locked

echo "==> no private references"
bash scripts/ci/no-private-refs.sh

echo "==> gitleaks"
if command -v gitleaks >/dev/null 2>&1; then
  gitleaks dir --redact --exit-code 1 --no-banner --gitleaks-ignore-path .gitleaksignore .
else
  echo "    skip: gitleaks not installed (https://github.com/gitleaks/gitleaks)"
fi

echo "gate: OK"
