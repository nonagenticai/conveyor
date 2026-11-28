#!/bin/bash
# #852 direct acts: the T3 delivery contract + Phase-G review, the T13/T3 schedule
# baselines, and the T13/T3 closes. Run on the restore (BIN=/tmp/conv852-bin.sh) and on
# production (BIN=/tmp/conv852-pod-bin.sh). Stops on the first refusal.
set -e
BIN=${BIN:-/tmp/conv852-bin.sh}
WT=${WT:-/Users/georgedekker/Movies/nonagenticai/conveyor/.worktrees/issue-852}
IN="$WT/docs/evidence/tranche-review/inputs"
OUT=${OUT:-/tmp/conv852-acts.json}
E2E=01a1e607-17d9-77d3-98d3-a43a87c4633d
OWNER=01a1efd4-c3e5-7b74-b32b-1b15683a8c4e
APPROVER=01a1e8a7-8b23-76a9-9114-aafb95f7a545
DELEG=01a2021d-cca2-757c-bfc2-21bd4e4b6d85
T13=01a1d039-0d37-730f-8f8a-2f50e5a0933d
T3=01a1d039-0d33-7915-8833-75f4dc31cc07

invoke() { local name=$1; shift; "$BIN" admin action invoke --tenant nonagentic --name "$name" --input /dev/stdin "$@"; }
jget() { python3 -c "import json,sys; d=json.load(sys.stdin); print(d$1)"; }

echo "== 1. T3 contract request =="
if [ -n "${CONTRACT:-}" ]; then
  echo "contract (resumed)=$CONTRACT"
else
  CONTRACT=$(invoke architecture.contracts_request --actor-id $E2E < "$IN/t3-contract.json" | jget "['id']")
  echo "contract=$CONTRACT"
fi

echo "== 2. architecture board decision =="
if [ -n "${BOARD:-}" ]; then
  echo "board_approval (resumed)=$BOARD"
else
  sed "s/<CONTRACT_ID>/$CONTRACT/g" "$IN/t3-board-decision.json" \
    | invoke architecture.board_decide --actor-id $APPROVER | tee /tmp/conv852-board-out.json | jget "['approval_id']"
  BOARD=$(jget "['approval_id']" < /tmp/conv852-board-out.json)
  echo "board_approval=$BOARD"
fi

echo "== 3. contracts_approve =="
CJSON=$(invoke architecture.contracts_get --actor-id $E2E <<<"{\"id\":\"$CONTRACT\"}")
CSTATUS=$(python3 -c "import json,sys; print(json.load(sys.stdin)['status'])" <<<"$CJSON")
CV=$(python3 -c "import json,sys; print(json.load(sys.stdin)['version'])" <<<"$CJSON")
echo "contract status=$CSTATUS version=$CV"
if [ "$CSTATUS" = "live" ]; then
  echo "(resumed, contract already live)"
else
  sed -e "s/<CONTRACT_ID>/$CONTRACT/g" -e "s/<BOARD_APPROVAL_ID>/$BOARD/g" "$IN/t3-contract-approve.json" \
    | invoke architecture.contracts_approve --actor-id $APPROVER --if-match "$CV" > /tmp/conv852-approve-out.json
  jget "['status']" < /tmp/conv852-approve-out.json || true
fi

echo "== 4. Phase-G review =="
CLOSURE=$(invoke documents.list --actor-id $E2E <<<"{\"project_id\":\"$T3\",\"category\":\"closure-review\"}" \
  | python3 -c "import json,sys; d=json.load(sys.stdin); items=[x for x in d.get('items',[]) if x.get('published_at')]; items.sort(key=lambda x:str(x.get('published_at'))); print(items[-1]['id'] if items else '')")
echo "closure_review=$CLOSURE"
[ -n "$CLOSURE" ] || { echo "no published T3 closure review"; exit 1; }
if [ -n "${REVIEW:-}" ]; then
  echo "review (resumed)=$REVIEW"
else
  sed -e "s/<CONTRACT_ID>/$CONTRACT/g" -e "s/<CLOSURE_REVIEW_ID>/$CLOSURE/g" "$IN/t3-review.json" \
    | invoke architecture.reviews_record --actor-id $APPROVER > /tmp/conv852-review-out.json
  REVIEW=$(jget "['id']" < /tmp/conv852-review-out.json)
  echo "review=$REVIEW"
fi

echo "== 5. schedule and cost baselines =="
for P in $T13 $T3; do
  short=$( [ "$P" = "$T13" ] && echo t13 || echo t3 )
  for KIND in schedule cost; do
    var="BASE_${short}_${KIND}"
    if [ -n "${!var:-}" ]; then
      echo "baseline $P $KIND (resumed)=${!var}"
      continue
    fi
    AP=$(invoke projects.baseline --actor-id $E2E < "$IN/$short-$KIND-baseline.json" | jget "['approval_id']")
    echo "baseline $P $KIND approval=$AP"
    printf '{"approval_id":"%s","decision":"approve","note":"Rule approval under the standing delegation: the %s baseline is frozen over the project'"'"'s existing WBS and approved artefacts.","delegation_ref":"%s"}' "$AP" "$KIND" "$DELEG" \
      | invoke approvals.decide --actor-id $APPROVER > /dev/null
    export "BASE_${short}_${KIND}=$AP"
  done
done

echo "== 5b. registers: risk, assumption, dependency =="
if [ -n "${SKIP_REGISTERS:-}" ]; then echo "(skipped, resumed)"; fi
closure_doc() {
  local P=$1
  invoke documents.list --actor-id $E2E <<<"{\"project_id\":\"$P\",\"category\":\"closure-review\"}" \
    | python3 -c "import json,sys; d=json.load(sys.stdin); items=[x for x in d.get('items',[]) if x.get('published_at')]; items.sort(key=lambda x:str(x.get('published_at'))); print(items[-1]['id'] if items else '')"
}
for P in $T13 $T3; do
  [ -z "${SKIP_REGISTERS:-}" ] || continue
  short=$( [ "$P" = "$T13" ] && echo t13 || echo t3 )
  DOC=$(closure_doc $P)
  echo "registers $P closure_doc=$DOC"
  [ -n "$DOC" ] || { echo "no published closure review for $P"; exit 1; }
  python3 - "$IN/$short-registers.json" "$DOC" > "/tmp/conv852-registers-$short.jsonl" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
for row in d['rows']:
    row = dict(row)
    row['project_id'] = d['project_id']
    row['links'] = {'cards': [], 'requirements': [], 'decisions': [sys.argv[2]]}
    print(json.dumps(row))
PY
  while IFS= read -r line; do
    printf '%s' "$line" | invoke registers.create --actor-id $E2E | jget "['id']"
  done < "/tmp/conv852-registers-$short.jsonl"
done

echo "== 5c. T3 retrospective ADM artefacts (Phase A-D) =="
if [ -n "${SKIP_ARTEFACTS:-}" ]; then echo "(skipped, resumed)"; fi
if [ -z "${SKIP_ARTEFACTS:-}" ]; then
  python3 - "$IN/t3-artefacts.json" "$DELEG" > /tmp/conv852-t3-artefacts.jsonl <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
for a in d['artefacts']:
    payload = dict(a)
    payload['project_id'] = d['project_id']
    payload['meta'] = {'artifact_key': a['artifact_key'], 'retrospective': True,
                       'delegation_ref': sys.argv[2]}
    del payload['artifact_key']
    print(json.dumps(payload))
PY
  while IFS= read -r line; do
    ART=$(printf '%s' "$line" | invoke architecture.artifacts_create --actor-id $E2E | jget "['id']")
    AV=$(invoke architecture.artifacts_get --actor-id $E2E <<<"{\"id\":\"$ART\"}" | jget "['version']")
    invoke architecture.artifacts_approve --actor-id $APPROVER --if-match "$AV" <<<"{\"id\":\"$ART\"}" > /dev/null
    echo "artefact $ART approved"
  done < /tmp/conv852-t3-artefacts.jsonl
fi

echo "== 6. closes =="
close_project() {
  local P=$1 reason=$2
  local GATE ST
  ST=$(invoke projects.get --actor-id $E2E <<<"{\"id\":\"$P\"}" \
    | python3 -c "import json,sys; print(json.load(sys.stdin)['project']['status'])")
  if [ "$ST" = "closed" ]; then echo "close $P (resumed, already closed)"; return 0; fi
  GATE=$(invoke approvals.list --actor-id $E2E <<<"{\"project_id\":\"$P\"}" \
    | python3 -c "import json,sys; d=json.load(sys.stdin); items=[x for x in d.get('items',[]) if x.get('gate')=='projects.close' and x.get('status')=='approved']; print(items[-1]['id'] if items else '')")
  echo "close $P gate=$GATE"
  [ -n "$GATE" ] || { echo "no approved projects.close gate for $P"; exit 1; }
  printf '{"id":"%s","reason":"%s"}' "$P" "$reason" \
    | invoke projects.close --actor-id $APPROVER --approval-id "$GATE" \
    | python3 -c "import json,sys; print(json.load(sys.stdin)['project']['status'])"
}
close_project $T13 "Rework complete through tr3; the closure review is published and the schedule baseline approved; the open items are named, not waived."
close_project $T3 "Rework complete through tr3; the closure review is published, the delivery contract and its Phase-G review are recorded, and the schedule baseline is approved; the open items are named, not waived."

echo "== read-backs =="
python3 - "$CONTRACT" "$BOARD" "$REVIEW" "$CLOSURE" "$OUT" <<'PY'
import json, os, sys
c, b, r, cr, out = sys.argv[1:6]
json.dump({
    'contract': c,
    'board_approval': b,
    'review': r,
    'closure_review_t3': cr,
    'baseline_approval_t13_schedule': os.environ.get('BASE_t13_schedule', ''),
    'baseline_approval_t13_cost': os.environ.get('BASE_t13_cost', ''),
    'baseline_approval_t3_schedule': os.environ.get('BASE_t3_schedule', ''),
    'baseline_approval_t3_cost': os.environ.get('BASE_t3_cost', ''),
}, open(out, 'w'), indent=1)
print('wrote', out)
PY
echo "DONE"
