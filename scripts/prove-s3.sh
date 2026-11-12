#!/usr/bin/env bash
# Prove the S3 wire (S09 §5.2's one UNVERIFIED leg) against a throwaway S3-compatible server.
#
# Why this exists: `minio/minio` is no longer pullable on this box (MinIO removed its Docker Hub images and
# quay.io answers 401 to an anonymous pull), so the suite runs `object_store`'s in-memory backend. That proves
# the trait and the key scheme, never endpoint, path-style addressing or SigV4. RustFS is an S3-compatible
# server that does pull, and `mc` creates the bucket because the `ObjectStore` trait has no create-bucket.
#
# Usage: scripts/prove-s3.sh          (leaves nothing behind)
set -euo pipefail
source ~/.cargo/env
cd "$(dirname "$0")/.."

NAME=conveyor-s3-proof
PORT=9000
BUCKET=conveyor-documents
DATA=$(mktemp -d)

cleanup() {
  docker rm -f "$NAME" >/dev/null 2>&1 || true
  rm -rf "$DATA"
}
trap cleanup EXIT

docker run -d --name "$NAME" -p "127.0.0.1:$PORT:9000" \
  -e RUSTFS_ACCESS_KEY=conveyor \
  -e RUSTFS_SECRET_KEY=conveyor-dev-secret \
  -v "$DATA:/data" \
  rustfs/rustfs:latest >/dev/null

ready=0
for _ in $(seq 1 120); do
  if curl -fsS -o /dev/null "http://127.0.0.1:$PORT/health"; then
    ready=1
    break
  fi
  sleep 0.5
done
[ "$ready" = 1 ] || { echo "the S3 server never became healthy" >&2; docker logs "$NAME" >&2; exit 1; }

# `/health` answers before the IAM layer is ready, so the alias is retried rather than assumed.
aliased=0
for _ in $(seq 1 120); do
  if mc alias set conveyor-proof "http://127.0.0.1:$PORT" conveyor conveyor-dev-secret >/dev/null 2>&1; then
    aliased=1
    break
  fi
  sleep 0.5
done
[ "$aliased" = 1 ] || { echo "the S3 server never accepted its credentials" >&2; docker logs "$NAME" >&2; exit 1; }
mc mb --ignore-existing "conveyor-proof/$BUCKET" >/dev/null

CONVEYOR_S3_ENDPOINT="http://127.0.0.1:$PORT" \
CONVEYOR_S3_ACCESS_KEY=conveyor \
CONVEYOR_S3_SECRET_KEY=conveyor-dev-secret \
CONVEYOR_S3_BUCKET="$BUCKET" \
  cargo test -p conveyor-actions --test s3_wire -- --ignored --nocapture

printf '\nS3_WIRE_PROVEN\n'
