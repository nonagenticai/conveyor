#!/bin/bash
# #852 tr3 rework: publish the four v4 delivery documents per member project, then declare
# S04's closure outputs. Resumable: a published document of the category is skipped.
# Run on the restore (BIN=/tmp/conv852-bin.sh) and on production (BIN=/tmp/conv852-pod-bin.sh).
set -euo pipefail
BIN=${BIN:?set BIN}
WT=${WT:-/Users/georgedekker/Movies/nonagenticai/conveyor/.worktrees/issue-852}
IN="$WT/docs/evidence/tranche-review/inputs/delivery"
OUT=${OUT:-/tmp/conv852-delivery.json}
ACTOR=01a1e607-17d9-77d3-98d3-a43a87c4633d

S04=01a1d039-0d28-7eb6-a62a-f420e742595c
T13=01a1d039-0d37-730f-8f8a-2f50e5a0933d
T3=01a1d039-0d33-7915-8833-75f4dc31cc07

invoke() { local name=$1; shift; "$BIN" admin action invoke --tenant nonagentic --name "$name" --actor-id "$ACTOR" --input /dev/stdin "$@"; }
jq_py() { python3 -c "import json,sys; d=json.load(sys.stdin); print(d$1)"; }

title_for() {
  case "$2" in
    user-stories) echo "User Stories — $1" ;;
    definition-of-ready) echo "Definition of Ready — $1" ;;
    definition-of-done) echo "Definition of Done — $1" ;;
    acceptance-criteria) echo "Acceptance Criteria — $1" ;;
  esac
}
name_for() {
  case "$1" in
    S04) echo "S04 Action registry" ;;
    T13) echo "T13 Tool platform" ;;
    T3) echo "T3 PM job" ;;
  esac
}

RESULT=/tmp/conv852-delivery-result.jsonl
: > "$RESULT"
for KEY in S04 T13 T3; do
  case "$KEY" in S04) PID=$S04 ;; T13) PID=$T13 ;; T3) PID=$T3 ;; esac
  for CAT in user-stories definition-of-ready definition-of-done acceptance-criteria; do
    EXIST=$(invoke documents.list <<<"{\"project_id\":\"$PID\",\"category\":\"$CAT\"}" \
      | python3 -c "import json,sys; d=json.load(sys.stdin); items=[x for x in d.get('items',[]) if x.get('published_at')]; print(items[-1]['id'] if items else '')")
    if [ -n "$EXIST" ]; then
      echo "$KEY $CAT published already: $EXIST"
      python3 -c "import json,sys; print(json.dumps({'project':'$KEY','category':'$CAT','document':'$EXIST','state':'present'}))" >> "$RESULT"
      continue
    fi
    BODY_FILE="$IN/$KEY-$CAT.md"
    [ -f "$BODY_FILE" ] || { echo "missing body $BODY_FILE"; exit 1; }
    python3 - "$BODY_FILE" "$PID" "$CAT" "$(title_for "$(name_for "$KEY")" "$CAT")" "$KEY" > /tmp/conv852-compose.json <<'PY'
import json, sys
body = open(sys.argv[1]).read()
print(json.dumps({"title": sys.argv[4], "project_id": sys.argv[2], "body": body,
                  "category": sys.argv[3], "unit": sys.argv[5], "publishable": True}))
PY
    DOC=$(invoke documents.compose < /tmp/conv852-compose.json | jq_py "['id']")
    V=$(invoke documents.list <<<"{\"project_id\":\"$PID\",\"category\":\"$CAT\"}" \
      | python3 -c "import json,sys; d=json.load(sys.stdin); items=[x for x in d.get('items',[]) if x.get('id')=='$DOC']; print(items[0].get('version') if items else '')")
    [ -n "$V" ] || { echo "no version for $DOC"; exit 1; }
    invoke documents.publish --if-match "$V" <<<"{\"id\":\"$DOC\",\"publish\":true}" \
      | python3 -c "import json,sys; d=json.load(sys.stdin); print('published', d.get('id'), d.get('published_at'))"
    python3 -c "import json,sys; print(json.dumps({'project':'$KEY','category':'$CAT','document':'$DOC','state':'created'}))" >> "$RESULT"
  done
done

echo "== S04 closure outputs declaration =="
CV=$(invoke projects.get <<<"{\"id\":\"$S04\"}" | jq_py "['project']['version']")
invoke projects.update --if-match "$CV" <<<"{\"id\":\"$S04\",\"meta\":{\"closure_outputs\":[\"evidence\",\"closure-review\"]}}" \
  | python3 -c "import json,sys; d=json.load(sys.stdin); print('S04 update', d.get('project',{}).get('version'))"

python3 -c "import json; print(json.dumps({'S04':'$S04','T13':'$T13','T3':'$T3'}))" > /tmp/conv852-delivery-ids.json
cp "$RESULT" "$OUT"
echo "wrote $OUT"
