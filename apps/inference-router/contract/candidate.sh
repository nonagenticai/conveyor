#!/usr/bin/env bash
# Boot the RUST candidate against the same dependencies the oracle uses, and replay the goldens.
#
#   candidate.sh --binary PATH [--only PATTERN]... [--url-only] [--keep] [--prefix NAME] [--down]
#
# The oracle (oracle.sh) runs INSIDE a private docker network. The candidate is a HOST binary, so
# each dependency is published on a loopback port and the provider catalog is rewritten to point
# at the stub's published port. Nothing depends on a container a previous run left behind:
# `--down` removes everything, and a run without --keep tears down on exit.
#
# The normal use is the gate:
#   rbuild -- build -p conveyor-inference-router            # builds on m1
#   bash candidate.sh --binary <path>/target/debug/conveyor-inference-router \
#        --only 'models*' --only 'providers*' --only 'whoami'
# Exit status is verify.sh's: 0 means every replayed case matched.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
prefix="ir-candidate"
keep=0
down=0
url_only=0
binary=""
only=()

usage() {
    sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//' >&2
    exit 2
}

while [ $# -gt 0 ]; do
    case "$1" in
        --binary) binary="${2:-}"; shift 2 ;;
        --only) only+=("${2:-}"); shift 2 ;;
        --url-only) url_only=1; keep=1; shift ;;
        --keep) keep=1; shift ;;
        --down) down=1; shift ;;
        --prefix) prefix="${2:-}"; shift 2 ;;
        *) usage ;;
    esac
done

net="$prefix-net"
pg="$prefix-pg"
redis="$prefix-redis"
stub="$prefix-stub"
pidfile="$here/.$prefix.pid"
logfile="/tmp/$prefix-candidate.log"

remove_all() {
    if [ -f "$pidfile" ]; then
        kill "$(cat "$pidfile")" >/dev/null 2>&1 || true
        rm -f "$pidfile"
    fi
    for container in "$stub" "$redis" "$pg"; do
        docker rm -f -v "$container" >/dev/null 2>&1 || true
    done
    docker network rm "$net" >/dev/null 2>&1 || true
}

if [ "$down" = 1 ]; then
    remove_all
    echo "candidate: removed (prefix $prefix)"
    exit 0
fi

command -v docker >/dev/null || { echo "missing docker (run with a login shell: /usr/local/bin on PATH)" >&2; exit 2; }
command -v curl >/dev/null || { echo "missing curl" >&2; exit 2; }
[ -n "$binary" ] || { echo "--binary is required" >&2; exit 2; }
[ -x "$binary" ] || { echo "not an executable: $binary" >&2; exit 2; }

if [ "$keep" != 1 ]; then
    trap 'remove_all' EXIT
fi

free_port() {
    local port
    for port in $(seq 18000 18200); do
        if ! (exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null; then
            printf '%s' "$port"
            return 0
        fi
        exec 3>&- 2>/dev/null || true
    done
    return 1
}

# --down is the only command that may run without --binary above; remove_all on a previous run
# gives this one the same names back.
remove_all

docker network create "$net" >/dev/null

echo "==> postgres:18 (published on loopback)"
docker run -d --name "$pg" --network "$net" -p 127.0.0.1::5432 \
    -e POSTGRES_USER=postgres -e POSTGRES_PASSWORD=oracle -e POSTGRES_DB=inference_router \
    postgres:18 >/dev/null
pg_port="$(docker port "$pg" 5432/tcp | head -1 | sed 's/.*://')"

echo "==> redis:8.10.1-alpine (the cost bus)"
docker run -d --name "$redis" --network "$net" -p 127.0.0.1::6379 \
    library/redis:8.10.1-alpine >/dev/null
redis_port="$(docker port "$redis" 6379/tcp | head -1 | sed 's/.*://')"

echo "==> stub (providers + licences + billing reserve)"
docker run -d --name "$stub" --network "$net" -p 127.0.0.1::9100 \
    -v "$here/stubs:/stub:ro" \
    python:3.13-slim python /stub/oracle_stub.py 9100 >/dev/null
stub_port="$(docker port "$stub" 9100/tcp | head -1 | sed 's/.*://')"

echo "==> waiting for postgres"
deadline=$((SECONDS + 120))
until docker exec "$pg" pg_isready -U postgres >/dev/null 2>&1; do
    [ $SECONDS -lt $deadline ] || { docker logs "$pg" 2>&1 | tail -20 >&2; echo "postgres never became ready" >&2; exit 1; }
    sleep 1
done
until curl -s -o /dev/null "http://127.0.0.1:${stub_port}/" 2>/dev/null; do
    [ $SECONDS -lt $deadline ] || { echo "stub never became reachable" >&2; exit 1; }
    sleep 1
done

providers="$(mktemp)"
sed "s|http://ir44-stub:9100/v1|http://127.0.0.1:${stub_port}/v1|" \
    "$here/stubs/oracle-providers.yaml" >"$providers"

port="$(free_port)" || { echo "no free loopback port" >&2; exit 1; }
url="http://127.0.0.1:${port}"
db_url="postgres://postgres:oracle@127.0.0.1:${pg_port}/inference_router"

export BIND_ADDR="127.0.0.1:${port}"
export INFERENCE_ROUTER_DATABASE_URL="$db_url"
export INFERENCE_ROUTER_PROVIDERS_CONFIG_PATH="$providers"
export INFERENCE_ROUTER_LICENSE_INTROSPECT_URL="http://127.0.0.1:${stub_port}/license/introspect"
export INFERENCE_ROUTER_INTROSPECT_MODE=enforce
export INFERENCE_ROUTER_BUDGET_GATE_MODE=enforce
export INFERENCE_ROUTER_BUDGET_GATE_BILLING_BASE_URL="http://127.0.0.1:${stub_port}"
export INFERENCE_ROUTER_POLICY_MODE=on
export INFERENCE_ROUTER_INTERNAL_SECRET=stub-internal-secret
export INFERENCE_ROUTER_METERING_REDIS_URL="redis://127.0.0.1:${redis_port}/0"
export INFERENCE_ROUTER_METERING_COSTS_ENABLED=true
export INFERENCE_ROUTER_METERING_COSTS_STREAM=billing:costs
export COMMERCIAL_PLATFORM_TENANT_IDS=00000000-0000-4000-8000-0000000000ff
export AUTH_DELEGATING_SUBJECTS=8382f0b3-c082-4f97-8d47-120ce5a18e05

echo "==> migrate"
"$binary" migrate

echo "==> candidate $binary"
nohup "$binary" serve >"$logfile" 2>&1 &
echo $! >"$pidfile"

deadline=$((SECONDS + 60))
until curl -fsS "$url/health" >/dev/null 2>&1; do
    if ! kill -0 "$(cat "$pidfile")" 2>/dev/null; then
        echo "candidate exited before becoming healthy:" >&2
        tail -40 "$logfile" >&2
        exit 1
    fi
    [ $SECONDS -lt $deadline ] || { echo "candidate never became healthy" >&2; tail -40 "$logfile" >&2; exit 1; }
    sleep 1
done

echo "candidate: $url"
echo "psql:      docker exec -i $pg psql -U postgres -d inference_router"
echo "log:       $logfile"

if [ "$url_only" = 1 ]; then
    echo "kept:      kill \$(cat $pidfile); docker rm -f $stub $redis $pg; docker network rm $net"
    exit 0
fi

only_args=()
for pattern in "${only[@]+"${only[@]}"}"; do
    only_args+=(--only "$pattern")
done

IR_PSQL="docker exec -i $pg psql -U postgres -d inference_router -v ON_ERROR_STOP=1" \
    bash "$here/verify.sh" --candidate "$url" "${only_args[@]+"${only_args[@]}"}"
