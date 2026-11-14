#!/usr/bin/env bash
# Boot the OLD (Python) service from a checkout so `capture.sh` has something to read.
#
#   SCIENTIFIC_PROCESSOR_SRC=/path/to/scientific-processor ./run-local.sh [PORT]
#
# Why a local instance at all: the old service's production settings refuse to start without a
# graph-adapter URL and a discovery-engine URL, and both backends are gone (the graph store's data
# was destroyed 2026-11-01). `ENVIRONMENT=development` is the switch that makes those two
# requirements non-fatal, so the service boots far enough to serve its own document.
#
# The pinned capture commit is recorded in manifest.json. This script never writes into the old
# checkout beyond its own `.venv` (uv sync).
set -euo pipefail

port="${1:-8899}"
src="${SCIENTIFIC_PROCESSOR_SRC:-}"
if [ -z "$src" ] || [ ! -f "$src/pyproject.toml" ]; then
    echo "set SCIENTIFIC_PROCESSOR_SRC to a checkout of the old service" >&2
    exit 2
fi

cd "$src"
if [ ! -d .venv ]; then
    uv sync --frozen
fi

echo "serving the old contract on http://127.0.0.1:${port} (ctrl-c to stop)"
exec env \
    ENVIRONMENT=development \
    PYTHONPATH=src \
    SCIENTIFIC_PROCESSOR_BIND_HOST=127.0.0.1 \
    SCIENTIFIC_PROCESSOR_BIND_PORT="$port" \
    uv run python -m uvicorn app:create_app --factory --host 127.0.0.1 --port "$port"
