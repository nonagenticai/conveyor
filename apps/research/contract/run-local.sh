#!/usr/bin/env bash
# Boot the OLD (Python) media-discovery from a checkout so `media/capture.sh` and
# `media/capture-goldens.sh` have a reference to read.
#
#   MEDIA_DISCOVERY_SRC=/path/to/media-discovery ./run-local.sh [PORT]
#
# Why a local instance at all: the deployed service answers from a live customer database and its
# adapters reach YouTube/Vimeo/podcast APIs, so its replies are neither synthetic nor stable. This
# boots the same source against a scratch Postgres with every network adapter disabled, which is
# what makes the golden set reproducible.
#
# Three things are load-bearing and were each measured the hard way (2026-11-15):
#   * `alembic.ini` commits a production `sqlalchemy.url`; `MEDIA_DISCOVERY_DATABASE_URL` must be
#     set or the migrations run against the cluster.
#   * The auth modules read the UNPREFIXED `ENABLE_AUTH` (os.getenv), not the `MEDIA_DISCOVERY_`
#     settings prefix, so both spellings are exported.
#   * Without `ALLOW_TRUSTED_NETWORK_DEFAULT_TENANT=true` a loopback caller with no licence token
#     gets 401 on every /api route.
#
# The pinned capture commit is recorded in `media/manifest.json`. This script never writes into the
# old checkout beyond its own `.venv` (uv sync).
set -euo pipefail

port="${1:-18809}"
src="${MEDIA_DISCOVERY_SRC:-}"
if [ -z "$src" ] || [ ! -f "$src/pyproject.toml" ]; then
    echo "set MEDIA_DISCOVERY_SRC to a checkout of the old service" >&2
    exit 2
fi

if [ -z "${MEDIA_DISCOVERY_DATABASE_URL:-}" ]; then
    pg=$(docker run -d --rm -e POSTGRES_PASSWORD=postgres \
        -p 127.0.0.1::5432 postgres:18 -c fsync=off)
    trap 'docker rm -f "$pg" >/dev/null 2>&1 || true' EXIT
    for _ in $(seq 1 60); do
        docker exec "$pg" pg_isready -h 127.0.0.1 -U postgres >/dev/null 2>&1 && break
        sleep 1
    done
    pg_port=$(docker port "$pg" 5432/tcp | head -1 | sed 's/.*://')
    export MEDIA_DISCOVERY_DATABASE_URL="postgresql://postgres:postgres@127.0.0.1:${pg_port}/postgres"
fi

cd "$src"
[ -d .venv ] || uv sync --frozen

MEDIA_DISCOVERY_DATABASE_URL="$MEDIA_DISCOVERY_DATABASE_URL" uv run --frozen alembic upgrade head

echo "serving the old contract on http://127.0.0.1:${port} (ctrl-c to stop)"
exec env \
    ENVIRONMENT=development \
    ENABLE_AUTH=false \
    MEDIA_DISCOVERY_ENABLE_AUTH=false \
    ALLOW_TRUSTED_NETWORK_DEFAULT_TENANT=true \
    DEFAULT_TENANT_ID=0195f48b-d000-7000-8000-000000000001 \
    MEDIA_DISCOVERY_DATABASE_URL="$MEDIA_DISCOVERY_DATABASE_URL" \
    MEDIA_DISCOVERY_ENVIRONMENT=development \
    MEDIA_DISCOVERY_ENABLE_YOUTUBE=false \
    MEDIA_DISCOVERY_ENABLE_VIMEO=false \
    MEDIA_DISCOVERY_ENABLE_PODCAST=false \
    MEDIA_DISCOVERY_ENABLE_WIKIMEDIA=false \
    MEDIA_DISCOVERY_ENABLE_WEB_SCRAPER=false \
    PYTHONPATH=src \
    .venv/bin/python -m uvicorn server:app --host 127.0.0.1 --port "$port"
