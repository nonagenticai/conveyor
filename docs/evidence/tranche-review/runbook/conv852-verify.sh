#!/bin/bash
# #852 evidence: snapshot the target record at the deployed sha, regenerate the three receipts,
# run the tranche-3 exit and the audit. Restore mode: SRC=restore (docker bin container).
# Production mode: SRC=prod (the strato transport, binary on the host beside the primary).
set -e
MODE=${MODE:-restore}
WT=${WT:-/Users/georgedekker/Movies/nonagenticai/conveyor/.worktrees/issue-852}
SHA=${SHA:-9e2ebbeac3d5a1612d298b67f7d7b4f07058e36e}
ROOT=${ROOT:-/tmp/conv852-p3-$MODE}
EVID=${EVID:-/tmp/conv852-evidence-$MODE}
cd "$WT"
rm -rf "$ROOT" "$EVID"; mkdir -p "$ROOT" "$EVID"

echo "== snapshot =="
if [ "$MODE" = "restore" ]; then
  docker cp "$WT/docs/design/data/issues.json" conv852-bin:/issues.json
  docker exec conv852-bin /usr/local/bin/conveyor snapshot --tenant nonagentic --git-sha "$SHA" \
    --out "$ROOT/data" --outputs-dir "$ROOT/docs" --issues /issues.json
  docker cp conv852-bin:"$ROOT/data" "$ROOT/data"
  docker cp conv852-bin:"$ROOT/docs" "$ROOT/docs"
elif [ "$MODE" = "prod" ]; then
  POD=$(ssh -i ~/.ssh/strato-vps -o ConnectTimeout=10 admin@100.81.177.99 "sudo KUBECONFIG=/etc/rancher/k3s/k3s.yaml kubectl -n conveyor get pods -l app=conveyor -o jsonpath='{.items[0].metadata.name}'" 2>/dev/null)
  ssh -i ~/.ssh/strato-vps -o ConnectTimeout=10 admin@100.81.177.99 "sudo bash -c 'set -a; . /tmp/conv852-live.env; set +a; /tmp/conveyor-probe snapshot --tenant nonagentic --git-sha $SHA --out $ROOT/data --outputs-dir $ROOT/docs --issues /tmp/conv852-issues.json'"
  mkdir -p "$ROOT"; scp -i ~/.ssh/strato-vps -qr admin@100.81.177.99:"$ROOT/data" "$ROOT/data"; scp -i ~/.ssh/strato-vps -qr admin@100.81.177.99:"$ROOT/docs" "$ROOT/docs"
fi
test -f "$ROOT/data/manifest.json" && test -f "$ROOT/docs/outputs.js"

echo "== verify-project S04/T13/T3 =="
for P in S04 T13 T3; do
  PROGRAMME_RECORD_LOCAL_BIN="${SHIM:-/tmp/conv852-bin.sh}" \
    python3 scripts/verify-project.py --project "$P" --snapshot "$ROOT/data" --evidence-dir "$EVID" 2>&1 | tail -4 || true
done

echo "== check-tranche-exit 3 (receipts from $EVID) =="
python3 scripts/check-tranche-exit.py --tranche 3 --snapshot "$ROOT/data" --evidence-dir "$EVID" > /tmp/conv852-exit.txt 2>&1; rc=$?; cat /tmp/conv852-exit.txt; echo "check-tranche-exit rc=$rc"

echo "== audit =="
python3 scripts/audit-programme-conformance.py --snapshot "$ROOT/data" --outputs "$ROOT/docs/outputs.js" \
  --findings docs/evidence/v1-conformance-findings.json --findings docs/evidence/standards-findings.json \
  --rework-out "$ROOT/audit/rework.json" --out "$ROOT/audit" --report 2>&1 | tail -3
python3 - "$ROOT/audit/rework.json" <<'PY'
import json,sys
d=json.load(open(sys.argv[1])); pk=d['packages']
for proj in ('S04','T13','T3'):
    rows=[p for p in pk if str(p.get('key','')).endswith(':'+proj) or p.get('owner_project')==proj]
    print(proj, 'open packages:', len(rows), [r.get('key') for r in rows][:8])
print('total', len(pk), 'open', sum(1 for p in pk if p.get('status')=='open'))
PY
echo "DONE ($MODE)"
