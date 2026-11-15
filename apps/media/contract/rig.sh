#!/usr/bin/env bash
# The capture rig for the media group.
#
#   rig.sh up                                  start (or reuse) the scratch Postgres and Redis
#   rig.sh db NAME                             create database NAME if it does not exist
#   rig.sh run NAME IMAGE HOSTPORT CONTAINERPORT DB [ENV=VAL ...]
#   rig.sh wait URL [TRIES]                    poll URL until it answers, 2s apart
#   rig.sh logs NAME                           tail a reference's log
#   rig.sh stop NAME                           remove one reference container
#   rig.sh down                                remove the scratch Postgres and Redis
#
# The direction is load-bearing: every reference is run **as the image the cluster runs**, pulled by
# digest to this host. Nothing here talks to the cluster, and the only endpoints a reference may
# reach are on localhost (scratch Postgres, scratch Redis, and whatever a card starts itself).
#
# `DATABASE_URL` and `REDIS_URL` are supplied for the card; anything else the service needs at boot
# is passed as an `ENV=VAL` argument.
set -euo pipefail

PG_PORT="${PG_PORT:-55433}"
REDIS_PORT="${REDIS_PORT:-56379}"
PG_CONTAINER="${PG_CONTAINER:-conveyor-media-pg}"
REDIS_CONTAINER="${REDIS_CONTAINER:-conveyor-media-redis}"
DB_USER="${DB_USER:-cap}"
DB_PASS="${DB_PASS:-cap}"

cmd="${1:-}"
shift || true

case "$cmd" in
up)
    docker inspect "$PG_CONTAINER" >/dev/null 2>&1 || \
        docker run -d --name "$PG_CONTAINER" \
            -e POSTGRES_PASSWORD="$DB_PASS" -e POSTGRES_USER="$DB_USER" \
            -p "127.0.0.1:$PG_PORT:5432" postgres:18 >/dev/null
    docker inspect "$REDIS_CONTAINER" >/dev/null 2>&1 || \
        docker run -d --name "$REDIS_CONTAINER" \
            -p "127.0.0.1:$REDIS_PORT:6379" library/redis:8.10.1-alpine >/dev/null
    docker start "$PG_CONTAINER" >/dev/null 2>&1 || true
    docker start "$REDIS_CONTAINER" >/dev/null 2>&1 || true
    for _ in $(seq 1 30); do
        docker exec "$PG_CONTAINER" pg_isready -U "$DB_USER" >/dev/null 2>&1 && break
        sleep 1
    done
    docker exec "$PG_CONTAINER" pg_isready -U "$DB_USER"
    docker exec "$REDIS_CONTAINER" redis-cli ping
    ;;
db)
    name="${1:?usage: rig.sh db NAME}"
    docker exec "$PG_CONTAINER" psql -U "$DB_USER" -d postgres -tAc \
        "select 1 from pg_database where datname='$name'" | grep -q 1 || \
        docker exec "$PG_CONTAINER" psql -U "$DB_USER" -d postgres -c "create database \"$name\"" >/dev/null
    echo "db $name ready"
    ;;
run)
    name="${1:?usage: rig.sh run NAME IMAGE HOSTPORT CONTAINERPORT DB [ENV=VAL ...]}"
    image="${2:?}"; hostport="${3:?}"; cport="${4:?}"; db="${5:?}"; shift 5
    docker rm -f "cap-$name" >/dev/null 2>&1 || true
    args=(-d --name "cap-$name" -p "127.0.0.1:$hostport:$cport"
          -e "DATABASE_URL=postgresql+psycopg://$DB_USER:$DB_PASS@host.docker.internal:$PG_PORT/$db"
          -e "REDIS_URL=redis://host.docker.internal:$REDIS_PORT/0"
          -e "API_PORT=$cport"
          -e "ENVIRONMENT=development"
          # w2v_common's auth has no bypass flag that works: `AUTH_ENABLED=false` is ignored by the
          # services that pass their own override. The trusted-network lane is the one that works,
          # and it is also the honest one — it still REFUSES a request with no X-Tenant-Id, so the
          # tenant contract is pinned rather than disabled. Measured on tts_gateway 2026-11-15.
          -e "AUTH_TRUST_INCLUSTER_IDENTITY=true"
          -e "TRUSTED_CIDRS=172.16.0.0/12,10.0.0.0/8,192.168.0.0/16")
    for kv in "$@"; do args+=(-e "$kv"); done
    docker run "${args[@]}" "$image" >/dev/null
    echo "cap-$name started on 127.0.0.1:$hostport"
    ;;
wait)
    url="${1:?usage: rig.sh wait URL [TRIES]}"; tries="${2:-45}"
    for i in $(seq 1 "$tries"); do
        code="$(curl -s -o /dev/null -m 5 -w '%{http_code}' "$url" 2>/dev/null || true)"
        case "$code" in 2*|3*|400|401|403|405|422) echo "$url -> $code after ${i}0s$([ "$i" -lt 10 ] && echo s || echo s)"; exit 0 ;; esac
        sleep 2
    done
    echo "$url did not answer after $tries tries" >&2
    exit 1
    ;;
logs) docker logs "cap-${1:?usage: rig.sh logs NAME}" 2>&1 | tail -40 ;;
stop) docker rm -f "cap-${1:?usage: rig.sh stop NAME}" >/dev/null 2>&1 || true; echo "cap-$1 removed" ;;
down)
    docker rm -f "$PG_CONTAINER" "$REDIS_CONTAINER" >/dev/null 2>&1 || true
    echo "rig down"
    ;;
*)
    sed -n '2,12p' "$0" >&2
    exit 2
    ;;
esac
