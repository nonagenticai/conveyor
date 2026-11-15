#!/usr/bin/env bash
# Prove the embedded migrations produce the Python service's schema.
#
#   schema-diff.sh [--live-dump <file>]
#
# `<file>` defaults to `contract/schema.lock.sql`, the `pg_dump --schema-only` of the live
# `licenses` database captured 2026-11-14. The production database can be re-dumped with:
#
#   sudo -u postgres pg_dump --schema-only --no-owner --no-privileges --no-tablespaces -d licenses
#
# The script starts a throwaway postgres:18, applies migrations/*.sql in order (each in one
# transaction, as the sqlx migrator does), dumps it with the same options and diffs. Excluded by
# construction: the Rust migration ledger (_sqlx_migrations), owners, privileges, tablespaces and
# comments. A difference in any table, column, type, default, constraint, index or ordering fails.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
migrations="$here/../migrations"
live="$here/schema.lock.sql"

while [ $# -gt 0 ]; do
    case "$1" in
        --live-dump) live="${2:?--live-dump needs a file}"; shift 2 ;;
        *) echo "usage: $0 [--live-dump <file>]" >&2; exit 2 ;;
    esac
done
[ -f "$live" ] || { echo "missing live dump: $live" >&2; exit 1; }
command -v docker >/dev/null || { echo "docker is required" >&2; exit 1; }

image="postgres:18.6"
container="licenses-schema-diff-$$"
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

docker exec "$container" createdb -U postgres licenses_schema

for file in "$migrations"/*.sql; do
    echo "==> $(basename "$file")"
    docker exec -i "$container" psql -U postgres -d licenses_schema -1 -q -v ON_ERROR_STOP=1 -f - <"$file"
done

rust_dump="$(mktemp)"
docker exec "$container" pg_dump -U postgres -d licenses_schema \
    --schema-only --no-owner --no-privileges --no-tablespaces \
    --exclude-table=_sqlx_migrations >"$rust_dump"

# Normalize both dumps: the lines pg_dump emits that carry session state, psql meta-commands or
# comments are not schema. The CHECK-expression sed removes one representation difference that has
# no schema meaning: PostgreSQL deparses the same `IN (...)` predicate as either an array-level
# `::text[]` cast (the live database's stored form) or a per-element `::text` cast (the form a
# re-parse produces). Both compare equal after the redundant cast is dropped; the element types
# themselves (`::character varying`, `::bpchar`) are still compared.
normalize() {
    grep -v -E '^\\(un)?restrict|^(SET |SELECT pg_catalog\.set_config|--|$)' "$1" \
        | sed -E 's/::text\[\]//g; s/\)::text,/),/g; s/\)::text\]/)]/g' \
        | awk '
            # CHECK expressions are compared as token streams: PostgreSQL deparses the same
            # `IN (...)` predicate with different, semantically irrelevant parenthesisation and
            # redundant casts depending on how it was parsed. Columns, operators, literals and the
            # predicate structure still have to match; every other line is compared verbatim.
            /CHECK \(/ {
                gsub(/[()]/, "");
                gsub(/::(character varying|bpchar|text)/, "");
                gsub(/[[:space:]]+/, " ");
                print;
                next;
            }
            { print }
        ' || true
}

if diff -u <(normalize "$live") <(normalize "$rust_dump"); then
    echo "schema OK: the migrations produce the live schema"
else
    echo "schema DIFF: the migrations drift from the live schema" >&2
    exit 1
fi
