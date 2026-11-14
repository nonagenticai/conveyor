#!/usr/bin/env bash
# Prove the embedded migrations produce the Python service's schema.
#
#   schema-diff.sh --live-dump <file>
#
# `<file>` is `pg_dump --schema-only --no-owner --no-privileges --no-tablespaces
# --exclude-table=alembic_version -d inference_router` taken from the live database.
#
# The script starts a throwaway postgres:18, applies migrations/*.sql in order (each in one
# transaction, as the sqlx migrator does), dumps it with the same options and diffs. Excluded by
# construction: the two migration ledgers (alembic_version on the Python side, _sqlx_migrations on
# the Rust side), owners, privileges, tablespaces and comments. A difference in any table, column,
# type, default, constraint, index or ordering is a failure.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
migrations="$here/../migrations"
live=""

while [ $# -gt 0 ]; do
    case "$1" in
        --live-dump) live="${2:?--live-dump needs a file}"; shift 2 ;;
        *) echo "usage: $0 --live-dump <file>" >&2; exit 2 ;;
    esac
done
[ -n "$live" ] || { echo "usage: $0 --live-dump <file>" >&2; exit 2; }
[ -f "$live" ] || { echo "missing live dump: $live" >&2; exit 1; }
command -v docker >/dev/null || { echo "docker is required" >&2; exit 1; }

image="postgres:18"
container="ir-schema-diff-$$"
cleanup() { docker rm -f "$container" >/dev/null 2>&1 || true; rm -f "$rust_dump"; }
rust_dump=""
trap cleanup EXIT

docker run -d --rm --name "$container" \
    --tmpfs /var/lib/postgresql \
    -e POSTGRES_PASSWORD=postgres \
    -p 127.0.0.1::5432 "$image" -c fsync=off >/dev/null

for _ in $(seq 1 90); do
    docker exec "$container" pg_isready -U postgres -h 127.0.0.1 >/dev/null 2>&1 && break
    sleep 1
done
docker exec "$container" pg_isready -U postgres -h 127.0.0.1 >/dev/null

docker exec "$container" createdb -U postgres ir_schema

for file in "$migrations"/*.sql; do
    echo "==> $(basename "$file")"
    docker exec -i "$container" psql -U postgres -d ir_schema -1 -q -v ON_ERROR_STOP=1 -f - <"$file"
done

rust_dump="$(mktemp)"
docker exec "$container" pg_dump -U postgres -d ir_schema \
    --schema-only --no-owner --no-privileges --no-tablespaces \
    --exclude-table=_sqlx_migrations >"$rust_dump"

# Normalize both dumps: the lines pg_dump emits that carry session state, psql meta-commands or
# comments are not schema.
normalize() {
    grep -v -E '^\\(un)?restrict|^(SET |SELECT pg_catalog\.set_config|--|$)' "$1" || true
}

if diff -u <(normalize "$live") <(normalize "$rust_dump"); then
    echo "schema OK: the migrations produce the live schema"
else
    echo "schema DIFF: the migrations drift from the live schema" >&2
    exit 1
fi
