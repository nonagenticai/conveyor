#!/usr/bin/env bash
# Prove the estate toolkits against the deployed conveyor (T14, SPEC_V2 §11.2, §11.8, §11.9; #83).
#
# Every action goes through the operator path (`conveyor admin action invoke`), which is the SAME
# engine as `POST /api/actions/<name>/invocations` — no second dispatcher, no raw SQL mutation of
# tenant state. `seed` is the exception and says so: it inserts the project, the run and the
# run-scoped bindings the broker needs, because no action starts a run outside a procedure.
#
# Usage:
#   CONVEYOR_TENANT=<slug> scripts/prove-estate.sh publish
#   CONVEYOR_TENANT=<slug> scripts/prove-estate.sh probe-all [--dry-run]
#   CONVEYOR_TENANT=<slug> RUN_ID=<uuid> scripts/prove-estate.sh call-all
#   CONVEYOR_TENANT=<slug> scripts/prove-estate.sh zero-use-report [window_days]
#   CONVEYOR_TENANT=<slug> RUN_ID=<uuid> scripts/prove-estate.sh outage
#   CONVEYOR_DATABASE_URL=<url> CONVEYOR_TENANT=<slug> scripts/prove-estate.sh seed
#
# Environment:
#   CONVEYOR_NAMESPACE     namespace of the deployment           (default: conveyor)
#   CONVEYOR_DEPLOYMENT    deployment whose pod runs the action  (default: conveyor)
#   CONVEYOR_TENANT        tenant slug — required
#   CONVEYOR_DATABASE_URL  only for `seed` — the tenant's database as its owner role
#   CONVEYOR_BINARY        the binary inside the pod             (default: /usr/local/bin/service)
#   KUBECTL                required — a kubectl command that reaches the deployed cluster
set -euo pipefail
cd "$(dirname "$0")/.."

NS=${CONVEYOR_NAMESPACE:-conveyor}
DEPLOY=${CONVEYOR_DEPLOYMENT:-conveyor}
TENANT=${CONVEYOR_TENANT:?set CONVEYOR_TENANT to the tenant slug}
BINARY=${CONVEYOR_BINARY:-/usr/local/bin/service}
KUBECTL=${KUBECTL:?set KUBECTL to a kubectl that reaches the deployed cluster}

pod() {
  $KUBECTL -n "$NS" get pod -l "app=$DEPLOY" \
    --field-selector=status.phase=Running -o jsonpath='{.items[0].metadata.name}'
}

# Run one action through the operator path and print its output document.
action() {
  local name=$1 input=${2:-'{}'}
  local target
  target=$(pod)
  [ -n "$target" ] || { echo "no running $DEPLOY pod in $NS" >&2; exit 1; }
  printf '{"input": %s}' "$input" | $KUBECTL -n "$NS" exec -i "$target" -- \
    "$BINARY" admin action invoke --tenant "$TENANT" --name "$name" --input /dev/stdin
}

# One field out of an action's JSON answer (a strict dotted path, no jq dependency).
field() {
  python3 -I -c 'import json,sys; d=json.load(sys.stdin); [d:=d[k] for k in sys.argv[1].split(".")]; print(d)' "$1"
}

psql_owner() {
  : "${CONVEYOR_DATABASE_URL:?set CONVEYOR_DATABASE_URL for this subcommand}"
  psql "$CONVEYOR_DATABASE_URL" -v ON_ERROR_STOP=1 -qAt "$@"
}

cmd=${1:-}
shift || true

case "$cmd" in
publish)
  action toolkits.publish '{}'
  ;;

probe-all)
  dry='{}'
  if [ "${1:-}" = "--dry-run" ]; then dry='{"dry_run": true}'; fi
  action tools.probe "$dry"
  ;;

seed)
  # Idempotent enough to repeat: the project is reused, a fresh run is started each time, and the
  # four published toolkits are bound to it with the ceiling their effect class needs (`spend` for
  # the three that render, which also permits read and write).
  project_key="T14-PROOF"
  run_id=$(psql_owner <<SQL
BEGIN;
SELECT set_config('app.current_tenant', (SELECT id::text FROM tenant WHERE slug = '$TENANT'), true);
INSERT INTO project (key, name) VALUES ('$project_key', 'T14 estate toolkit proof')
  ON CONFLICT (tenant_id, key) DO NOTHING;
WITH started AS (
  INSERT INTO run (project_id, status)
    SELECT id, 'running' FROM project WHERE key = '$project_key'
    RETURNING id
)
INSERT INTO tool_binding (run_id, toolkit_id, effect_ceiling, enabled)
  SELECT started.id, k.id, 'spend', true
    FROM started, toolkit k
   WHERE k.key IN ('coding', 'speech', 'transcription', 'image_gen')
     AND k.status = 'published'
     AND NOT EXISTS (
       SELECT 1 FROM tool_binding b
        WHERE b.run_id = started.id AND b.toolkit_id = k.id);
COMMIT;
SELECT id FROM run WHERE project_id = (SELECT id FROM project WHERE key = '$project_key')
 ORDER BY created_at DESC LIMIT 1;
SQL
)
  echo "seeded run $run_id"
  ;;

call-all)
  : "${RUN_ID:?set RUN_ID to the run the seed created}"
  # One declared operation per published toolkit (P1.2). The inputs are the actions' own documents:
  # a call is validated against the schema the publish copied from the registry.
  action tools.call "{\"run_id\": \"$RUN_ID\", \"tool\": \"coding.jobs_get\", \"input\": {\"id\": \"probe-job\"}}"

  # `media.transcribe` takes a stored audio document; create a 0.1 s silent WAV first.
  wav=$(python3 -I -c '
import base64, io, struct, wave
buf = io.BytesIO()
with wave.open(buf, "wb") as w:
    w.setnchannels(1); w.setsampwidth(1); w.setframerate(8000)
    w.writeframes(b"\x80" * 800)
print(base64.b64encode(buf.getvalue()).decode())')
  document=$(action documents.create \
    "{\"title\": \"estate proof audio\", \"content_type\": \"audio/wav\", \"content_base64\": \"$wav\"}" \
    | field id)
  echo "created audio document $document"
  action tools.call "{\"run_id\": \"$RUN_ID\", \"tool\": \"transcription.transcribe\", \"input\": {\"document\": \"$document\"}}"

  action tools.call "{\"run_id\": \"$RUN_ID\", \"tool\": \"speech.synthesize\", \"input\": {\"text\": \"estate proof\"}}"
  action tools.call "{\"run_id\": \"$RUN_ID\", \"tool\": \"image_gen.generate\", \"input\": {\"prompt\": \"estate proof\"}}"
  ;;

zero-use-report)
  action tools.usage "{\"window_days\": ${1:-30}}"
  ;;

outage)
  # A real destination failure: the coding Deployment is scaled to zero, the probe reports it, the
  # broker refuses before any call, and scaling back lifts the refusal. Nothing is mocked.
  : "${RUN_ID:?set RUN_ID to the run the seed created}"
  echo "== scale coding to zero"
  $KUBECTL -n coding scale deploy coding --replicas=0
  $KUBECTL -n coding rollout status deploy/coding --timeout=120s || true
  echo "== probe (real failure, not a mock)"
  action tools.probe '{"toolkit": "coding"}'
  echo "== call (refused before the destination)"
  action tools.call "{\"run_id\": \"$RUN_ID\", \"tool\": \"coding.jobs_get\", \"input\": {\"id\": \"probe-job\"}}" || true
  echo "== restore"
  $KUBECTL -n coding scale deploy coding --replicas=2
  $KUBECTL -n coding rollout status deploy/coding --timeout=180s
  echo "== probe (the refusal lifts)"
  action tools.probe '{"toolkit": "coding"}'
  ;;

*)
  echo "usage: $0 {publish|seed|probe-all|call-all|zero-use-report|outage}" >&2
  exit 2
  ;;
esac
