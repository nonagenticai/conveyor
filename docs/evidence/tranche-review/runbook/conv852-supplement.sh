#!/bin/bash
# #852 tr3 v4 supplement: publish the authored analysis/design documents the output standard's
# class requires before a member can close (architecture-analysis, inception-analysis,
# architecture-design). Resumable: a published document of the category is skipped.
# Run on the restore (BIN=/tmp/conv852-bin.sh) and on production (BIN=/tmp/conv852-pod-bin.sh).
set -euo pipefail
BIN=${BIN:?set BIN}
WT=${WT:-/Users/georgedekker/Movies/nonagenticai/conveyor/.worktrees/issue-852}
IN="$WT/docs/evidence/tranche-review/inputs/v4"
OUT=${OUT:-/tmp/conv852-supplement.json}
ACTOR=01a1e607-17d9-77d3-98d3-a43a87c4633d

S04=01a1d039-0d28-7eb6-a62a-f420e742595c
T13=01a1d039-0d37-730f-8f8a-2f50e5a0933d
T3=01a1d039-0d33-7915-8833-75f4dc31cc07

invoke() { local name=$1; shift; "$BIN" admin action invoke --tenant nonagentic --name "$name" --actor-id "$ACTOR" --input /dev/stdin "$@"; }
jq_py() { python3 -c "import json,sys; d=json.load(sys.stdin); print(d$1)"; }

# project-key category file-stem title
rows=(
  "S04 inception-analysis inception-S04 'Inception analysis — S04 action registry (retrospective, 2026-11-28)'"
  "S04 architecture-analysis analysis-S04 'Architecture analysis — S04 action registry (retrospective, 2026-11-28)'"
  "S04 architecture-design design-S04 'Architecture design — S04 action registry (retrospective, 2026-11-28)'"
  "T13 architecture-analysis analysis-T13 'Architecture analysis — T13 tool platform (retrospective, 2026-11-28)'"
  "T3 inception-analysis inception-T3 'Inception analysis — T3 PM job (retrospective, 2026-11-28)'"
  "T3 architecture-analysis analysis-T3 'Architecture analysis — T3 PM job (retrospective, 2026-11-28)'"
  "T3 architecture-design design-T3 'Architecture design — T3 PM job (retrospective, 2026-11-28)'"
)

RESULT=/tmp/conv852-supplement-result.jsonl
: > "$RESULT"
for spec in "${rows[@]}"; do
  read -r KEY CAT STEM TITLE <<<"$spec"
  case "$KEY" in S04) PID=$S04 ;; T13) PID=$T13 ;; T3) PID=$T3 ;; esac
  EXIST=$(invoke documents.list <<<"{\"project_id\":\"$PID\",\"category\":\"$CAT\"}" \
    | python3 -c "import json,sys; d=json.load(sys.stdin); items=[x for x in d.get('items',[]) if x.get('published_at')]; items.sort(key=lambda x: str(x.get('published_at'))); print(items[-1]['id'] if items else '')")
  if [ -n "$EXIST" ]; then
    echo "$KEY $CAT published already: $EXIST"
    python3 -c "import json; print(json.dumps({'project':'$KEY','category':'$CAT','document':'$EXIST','state':'present'}))" >> "$RESULT"
    continue
  fi
  BODY_FILE="$IN/$STEM.md"
  [ -f "$BODY_FILE" ] || { echo "missing body $BODY_FILE"; exit 1; }
  python3 - "$BODY_FILE" "$PID" "$CAT" "$TITLE" "$KEY" > /tmp/conv852-supplement-compose.json <<'PY'
import json, sys
body = open(sys.argv[1], encoding="utf-8").read()
print(json.dumps({"title": sys.argv[4], "project_id": sys.argv[2], "body": body,
                  "category": sys.argv[3], "unit": sys.argv[5], "publishable": True}))
PY
  DOC=$(invoke documents.compose < /tmp/conv852-supplement-compose.json | jq_py "['id']")
  V=$(invoke documents.list <<<"{\"project_id\":\"$PID\",\"category\":\"$CAT\"}" \
    | python3 -c "import json,sys; d=json.load(sys.stdin); items=[x for x in d.get('items',[]) if x.get('id')=='$DOC']; print(items[0].get('version') if items else '')")
  [ -n "$V" ] || { echo "no version for $DOC"; exit 1; }
  invoke documents.publish --if-match "$V" <<<"{\"id\":\"$DOC\",\"publish\":true}" \
    | python3 -c "import json,sys; d=json.load(sys.stdin); print('published', d.get('id'), d.get('published_at'))"
  python3 -c "import json; print(json.dumps({'project':'$KEY','category':'$CAT','document':'$DOC','state':'created'}))" >> "$RESULT"
done
cp "$RESULT" "$OUT"
echo "wrote $OUT"
