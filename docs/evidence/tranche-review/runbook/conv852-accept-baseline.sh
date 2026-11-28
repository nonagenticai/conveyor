#!/bin/bash
# Accept the conformance baseline at the current register, naming the decision that accepted it.
# Mirrors publish-data.sh steps 1-3b so the gap set it accepts is the gap set the strict publish
# gate will read. The v4 standard's categories put the live debt in the ledger explicitly:
# every entry keeps its disposition and owner, the new entries name the v4 evidence campaign.
set -euo pipefail
ROOT=${ROOT:?set ROOT (the checkout whose docs/evidence/conformance-baseline.json is written)}
CONVEYOR_BIN=${CONVEYOR_BIN:?set CONVEYOR_BIN}
SNAPSHOT_DATABASE_URL=${SNAPSHOT_DATABASE_URL:?set SNAPSHOT_DATABASE_URL}
ACCEPTED_BY=${ACCEPTED_BY:?set ACCEPTED_BY}
APPROVER=${APPROVER:?set APPROVER}
ACCEPTANCE_REF=${ACCEPTANCE_REF:?set ACCEPTANCE_REF}
DELEGATION_REF=${DELEGATION_REF:?set DELEGATION_REF}
TENANT=${TENANT:-nonagentic}
cd "$ROOT"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/data" "$tmp/docs" "$tmp/conformance"
cp docs/evidence/conformance-baseline.json "$tmp/previous-baseline.json"

scripts/fetch-issues.sh "$tmp/data/issues.json"
"$CONVEYOR_BIN" snapshot --tenant "$TENANT" --out "$tmp/data" --outputs-dir "$tmp/docs" \
  --git-sha "$(git rev-parse HEAD)" --issues "$tmp/data/issues.json" --database-url "$SNAPSHOT_DATABASE_URL"
if [ -f docs/design/data/coverage.json ]; then cp docs/design/data/coverage.json "$tmp/data/coverage.json"; fi
if [ -f docs/design/data/actions.json ]; then cp docs/design/data/actions.json "$tmp/data/actions.json"; fi

audit_args=(--snapshot "$tmp/data" --out "$tmp/conformance" --snapshot-label docs/design/data)
audit_args+=(--path-label "$tmp/conformance/standards-findings.json=docs/evidence/standards-findings.json")
python3 scripts/audit-programme-conformance.py "${audit_args[@]}" --report
python3 scripts/generate-standards.py \
  --snapshot "$tmp/data" --outputs-dir "$tmp/docs" \
  --gaps "$tmp/conformance/conformance-gap-register.json" \
  --findings-out "$tmp/conformance/standards-findings.json" >/dev/null

python3 scripts/audit-programme-conformance.py "${audit_args[@]}" \
  --findings docs/evidence/v1-conformance-findings.json \
  --findings "$tmp/conformance/standards-findings.json" \
  --deep-review docs/evidence/record-w3/deep-review-findings.json \
  --deep-review docs/evidence/record-w4/deep-review-findings.json \
  --deep-review docs/evidence/record-w5/deep-review-findings.json \
  --report --accept-baseline --rework-out "$tmp/conformance/rework.json" \
  --accepted-by "$ACCEPTED_BY" --approver "$APPROVER" \
  --acceptance-ref "$ACCEPTANCE_REF" --delegation-ref "$DELEGATION_REF" \
  --baseline docs/evidence/conformance-baseline.json

# The reason is the entry's provenance. An entry the previous acceptance carried keeps its
# history; a new one names the v4 evidence campaign as the reduction owner.
python3 - "$tmp/previous-baseline.json" docs/evidence/conformance-baseline.json <<'PY'
import json, sys
previous = { (e["rule"], e["subject"]): e
             for e in json.load(open(sys.argv[1])).get("entries", []) }
path = sys.argv[2]
document = json.load(open(path))
new = 0
for entry in document["entries"]:
    old = previous.get((entry["rule"], entry["subject"]))
    if old is not None and old.get("reason"):
        entry["reason"] = old["reason"]
    else:
        entry["reason"] = (
            "standard v4 evidence set (#930); accepted 2026-11-28 under the standing delegation; "
            "the ratchet (#928) forbids growth"
        )
        new += 1
json.dump(document, open(path, "w"), indent=2)
open(path, "a").write("\n")
print(f"accepted {len(document['entries'])} entries ({new} new, "
      f"{len(document['entries']) - new} carried), acceptance {document['acceptance_ref']}")
PY
