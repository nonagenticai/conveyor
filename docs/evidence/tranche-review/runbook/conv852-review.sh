#!/bin/bash
# conveyor#852 — record the tranche-3 `continue` review on the live record.
# Restore:    BIN=/tmp/conv852-bin.sh     SNAP=<root>/data OUT=/tmp/conv852-review.json /tmp/conv852-review.sh
# Production: BIN=/tmp/conv852-pod-bin.sh SNAP=<root>/data OUT=/tmp/conv852-review.json /tmp/conv852-review.sh
# Stops on the first refusal. Prints every id. Writes OUT as JSON.
set -euo pipefail

: "${BIN:?set BIN (the record CLI shim)}"
: "${SNAP:?set SNAP (the snapshot root, i.e. <root>/data)}"
: "${OUT:?set OUT (the JSON result path)}"
EVIDENCE_PY=${EVIDENCE_PY:-/tmp/conv852-evidence.py}
TENANT=${TENANT:-nonagentic}

TR3=01a1e9e0-eb54-74d8-a2b5-44fb2b87eb52
PROGRAMME=01a1d039-0d24-7d16-ae52-4841e96ccfac
DOC_PROJECT=01a1dcb8-d24b-76de-b901-56fd56e001bc   # project P0; documents scope to a project, not a programme
DELEG=01a2021d-cca2-757c-bfc2-21bd4e4b6d85
REQUESTER=01a1efd4-c3e5-7b74-b32b-1b15683a8c4e
APPROVER=01a1e8a7-8b23-76a9-9114-aafb95f7a545
DOC_ACTOR=01a1e607-17d9-77d3-98d3-a43a87c4633d

REVIEW_AT=$(date -u +%Y-%m-%dT%H:%M:%SZ)
REVIEW_DATE=${REVIEW_AT%%T*}

WORK=$(mktemp -d "${TMPDIR:-/tmp}/conv852-review.XXXXXX")
trap 'rm -rf "$WORK"' EXIT

say() { printf '%s\n' "$*"; }
die() { printf 'REFUSED: %s\n' "$*" >&2; exit 1; }

invoke() { # invoke <action> <actor> [extra flags...]  (payload on stdin)
  local name=$1 actor=$2; shift 2
  "$BIN" admin action invoke --tenant "$TENANT" --name "$name" \
    --actor-id "$actor" --input /dev/stdin "$@"
}

say "== 1. derive the tranche-3 evidence from $SNAP =="
[ -x "$EVIDENCE_PY" ] || die "evidence script not executable: $EVIDENCE_PY"
python3 "$EVIDENCE_PY" "$SNAP" S04,T13,T3 > "$WORK/evidence.json" \
  || die "evidence derivation failed"
python3 - "$WORK/evidence.json" <<'PY' || die "evidence is missing a key the review records"
import json, sys
e = json.load(open(sys.argv[1]))
need = ("snapshot_sha", "snapshot_manifest_sha256", "baselines",
        "benefit_status", "risk_register", "detail")
missing = [k for k in need if k not in e]
if missing:
    sys.exit("missing evidence keys: " + ", ".join(missing))
for key in ("baselines", "benefit_status", "risk_register"):
    ids = e[key]
    if not isinstance(ids, list) or not ids or not all(isinstance(x, str) for x in ids):
        sys.exit("%s is not a non-empty array of id strings" % key)
print("snapshot_sha=" + str(e["snapshot_sha"]))
print("evidence: baselines=%d benefit_status=%d risk_register=%d"
      % tuple(len(e[k]) for k in ("baselines", "benefit_status", "risk_register")))
PY

say "== 2. read the tr3 row version via tranches.list =="
printf '{"programme_id":"%s"}\n' "$PROGRAMME" | invoke tranches.list "$APPROVER" > "$WORK/tranches-before.json"
TR3_VERSION=$(python3 - "$WORK/tranches-before.json" "$TR3" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
items = d.get("items", d if isinstance(d, list) else [])
row = next((r for r in items if str(r.get("id")) == sys.argv[2]), None)
if row is None:
    sys.exit("no tr3 row in tranches.list")
v = row.get("version", row.get("etag"))
if v is None:
    sys.exit("the tr3 row carries no version")
print(v.strip('"') if isinstance(v, str) else v)
PY
) || die "could not read the tr3 row version"
say "tr3=$TR3 version=$TR3_VERSION"

say "== 3. tranches.update: review_at=$REVIEW_AT (if-match $TR3_VERSION) =="
python3 - "$TR3" "$REVIEW_AT" > "$WORK/update.json" <<'PY'
import json, sys
json.dump({"id": sys.argv[1], "review_at": sys.argv[2]}, sys.stdout)
PY
invoke tranches.update "$APPROVER" --if-match "$TR3_VERSION" < "$WORK/update.json" \
  > "$WORK/update-out.json"
python3 - "$WORK/update-out.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
print("tranche_updated=%s review_at=%s" % (d.get("id", ""), d.get("review_at", "")))
PY

say "== 4. approvals.request (requester) with the derived evidence =="
python3 - "$WORK/evidence.json" "$TR3" "$DELEG" > "$WORK/approval-request.json" <<'PY'
import json, sys
evidence = json.load(open(sys.argv[1]))
tr3, deleg = sys.argv[2], sys.argv[3]
payload = {
    "gate": "tranches.review",
    "kind": "action",
    "action": "tranches.review",
    "input": {"id": tr3, "decision": "continue", "delegation_ref": deleg,
              "evidence": evidence},
    "description": "Record the tranche-3 review: continue, with the snapshot's baselines, benefit status, risk register and the v1 verdict as evidence",
    "proposed_answer": "continue",
}
# The CLI unwraps a bare top-level `input`; wrap the whole payload.
json.dump({"input": payload}, sys.stdout)
PY
invoke approvals.request "$REQUESTER" < "$WORK/approval-request.json" > "$WORK/approval.json"
APPROVAL=$(python3 - "$WORK/approval.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
a = d.get("id") or d.get("approval_id")
if not a:
    sys.exit("approvals.request returned no approval id")
print(a)
PY
) || die "approvals.request returned no approval id"
say "approval=$APPROVAL"

say "== 5. approvals.decide (approver) =="
python3 - "$APPROVAL" "$DELEG" > "$WORK/approval-decide.json" <<'PY'
import json, sys
json.dump({
    "approval_id": sys.argv[1],
    "decision": "approve",
    "note": "Rule approval under the standing delegation: the tr3 review continues on the recorded baselines, benefit status, risk register and the v1 verdict; T13 closure is named as blocked on v1-w4a-5 and the closure gate",
    "delegation_ref": sys.argv[2],
}, sys.stdout)
PY
invoke approvals.decide "$APPROVER" < "$WORK/approval-decide.json" \
  > "$WORK/approval-decide-out.json"
python3 - "$WORK/approval-decide-out.json" <<'PY' || die "the approval was not decided approve"
import json, sys
d = json.load(open(sys.argv[1]))
status = str(d.get("status", "approved"))
if status not in ("approved", "decided"):
    sys.exit("decision status: " + status)
print("approval_status=" + status)
PY

say "== 6. tranches.review continue (approver, approval $APPROVAL) =="
python3 - "$WORK/evidence.json" "$TR3" "$DELEG" > "$WORK/tranche-review.json" <<'PY'
import json, sys
evidence = json.load(open(sys.argv[1]))
json.dump({"id": sys.argv[2], "decision": "continue",
           "delegation_ref": sys.argv[3], "evidence": evidence}, sys.stdout)
PY
invoke tranches.review "$APPROVER" --approval-id "$APPROVAL" \
  < "$WORK/tranche-review.json" > "$WORK/review-out.json"
REVIEW=$(python3 - "$WORK/review-out.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
r = d.get("id") or d.get("review_id")
if not r:
    sys.exit("tranches.review returned no review id")
print(r)
PY
) || die "tranches.review returned no review id"
say "review=$REVIEW"

say "== 7. read back tranches.list =="
printf '{"programme_id":"%s"}\n' "$PROGRAMME" | invoke tranches.list "$APPROVER" > "$WORK/tranches-after.json"
python3 - "$WORK/tranches-after.json" "$TR3" "$WORK/readback.json" <<'PY' \
  || die "read-back failed: tr3 not closed with review_at, or tr4 not authorised"
import json, sys
d = json.load(open(sys.argv[1]))
items = d.get("items", d if isinstance(d, list) else [])
def state(row):
    return row.get("state", row.get("status"))
tr3 = next((r for r in items if str(r.get("id")) == sys.argv[2]), None)
tr4s = [r for r in items if str(r.get("number")) == "4"]
if tr3 is None:
    sys.exit("tr3 is absent from tranches.list")
if not tr4s:
    sys.exit("tranche 4 is absent from tranches.list")
tr4 = tr4s[0]
print("tr3 state=%s review_at=%s" % (state(tr3), tr3.get("review_at")))
print("tr4 id=%s state=%s authorisation_review_id=%s"
      % (tr4.get("id"), state(tr4), tr4.get("authorisation_review_id")))
json.dump({"tr3": tr3, "tr4": tr4}, open(sys.argv[3], "w"))
problems = []
if state(tr3) != "closed":
    problems.append("tr3 state=%r, expected 'closed'" % (state(tr3),))
if not tr3.get("review_at"):
    problems.append("tr3 review_at is empty")
if state(tr4) not in ("authorised", "authorized"):
    problems.append("tr4 state=%r, expected 'authorised'" % (state(tr4),))
if not tr4.get("authorisation_review_id"):
    problems.append("tr4 authorisation_review_id is empty")
if problems:
    sys.exit("; ".join(problems))
PY
TR4=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["tr4"]["id"])' \
  "$WORK/readback.json")
say "tr4=$TR4"

say "== 8. compose the tranche-review document (project P0 $DOC_PROJECT) =="
python3 - "$WORK/evidence.json" "$WORK/readback.json" "$REVIEW" "$APPROVAL" \
  "$REVIEW_DATE" "$DOC_PROJECT" > "$WORK/compose.json" <<'PY'
import json, sys
ev = json.load(open(sys.argv[1]))
rb = json.load(open(sys.argv[2]))
review, approval, date, doc_project = sys.argv[3:7]
tr3, tr4 = rb["tr3"], rb["tr4"]
d = ev.get("detail", {})
b = d.get("benefit", {})
r = d.get("risk", {})
base = d.get("project_baselines", {})
def approved(key):
    return sum(1 for x in base.get(key, []) if x.get("approved"))
body = "\n".join([
    "## Evidence",
    "- Snapshot `%s` (manifest `%s`, migration %s), taken at the deployed binary: the tranche "
    "exit runs on it and the three receipts are regenerated from it."
    % (ev.get("snapshot_sha"), ev.get("snapshot_manifest_sha256"),
       ev.get("snapshot_migration")),
    "- Reviewed programme baselines (approved): %d — management, governance and boundary."
    % len(ev.get("baselines", [])),
    "- Project baselines cited: S04 %d, T13 %d, T3 %d (each a project baseline of the member)."
    % (len(base.get("S04", [])), len(base.get("T13", [])), len(base.get("T3", []))),
    "- Tranche 3 (`%s`) state `%s`; tranche 4 (`%s`) state `%s`."
    % (d.get("tranche_3", {}).get("id"), d.get("tranche_3", {}).get("state"),
       d.get("tranche_4", {}).get("id"), d.get("tranche_4", {}).get("state")),
    "## Benefit status",
    "- Profiles: %s; active: %s; draft: %s; measurements: %s (of %s realisation rows)."
    % (b.get("profiles", 0), ", ".join(b.get("active", [])) or "—",
       ", ".join(b.get("draft", [])) or "—", b.get("measurements", 0),
       b.get("realisation_rows", 0)),
    "- Cited as evidence: %d benefit profiles of the programme." % len(ev.get("benefit_status", [])),
    "## Risk and issue register",
    "- Register rows: %s; risks: %s; open: %s." % (r.get("rows", 0), r.get("risks", 0),
                                                  r.get("open", 0)),
    "- Cited as evidence: %d risk items of the programme." % len(ev.get("risk_register", [])),
    "## Decision",
    "- `continue`: tranche 3 (`%s`) is closed and tranche 4 (`%s`) is authorised "
    "by review `%s` under approval `%s`, decided by a distinct approver under the standing "
    "delegation `01a2021d-cca2-757c-bfc2-21bd4e4b6d85`."
    % (tr3.get("id"), tr4.get("id"), review, approval),
])
json.dump({
    "title": "Tranche review — tranche 3 (%s)" % date,
    "project_id": doc_project,
    "body": body,
    "category": "tranche-review",
    "unit": "tranche-3",
    "publishable": True,
}, sys.stdout)
PY
invoke documents.compose "$DOC_ACTOR" < "$WORK/compose.json" > "$WORK/compose-out.json"
DOC=$(python3 - "$WORK/compose-out.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
if not d.get("id"):
    sys.exit("documents.compose returned no document id")
print(d["id"])
PY
) || die "documents.compose returned no document id"
say "document=$DOC"

say "== 9. read back the composed version and publish the document =="
printf '{"project_id":"%s","category":"tranche-review"}\n' "$DOC_PROJECT" \
  | invoke documents.list "$DOC_ACTOR" > "$WORK/documents.json"
DOC_VERSION=$(python3 - "$WORK/documents.json" "$DOC" "$WORK/compose-out.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
items = d.get("items", d if isinstance(d, list) else [])
row = next((x for x in items if str(x.get("id")) == sys.argv[2]), None)
v = row.get("version") if row else None
if v is None:
    v = json.load(open(sys.argv[3])).get("version")
if v is None:
    sys.exit("no version for the composed document")
print(v.strip('"') if isinstance(v, str) else v)
PY
) || die "could not read the composed document's version"
say "document=$DOC version=$DOC_VERSION"
python3 - "$DOC" > "$WORK/publish.json" <<'PY'
import json, sys
json.dump({"id": sys.argv[1], "publish": True}, sys.stdout)
PY
invoke documents.publish "$DOC_ACTOR" --if-match "$DOC_VERSION" \
  < "$WORK/publish.json" > "$WORK/publish-out.json"
PUBLISHED_AT=$(python3 - "$WORK/publish-out.json" <<'PY'
import json, sys
print(json.load(open(sys.argv[1])).get("published_at", ""))
PY
)
say "published=$DOC published_at=$PUBLISHED_AT"

python3 - "$OUT" "$WORK/evidence.json" "$TR3_VERSION" "$APPROVAL" "$REVIEW" \
  "$DOC" "$DOC_VERSION" "$PUBLISHED_AT" "$WORK/readback.json" <<'PY'
import json, sys
(out, evidence, version, approval, review, doc, doc_version, published,
 readback) = sys.argv[1:10]
e = json.load(open(evidence))
rb = json.load(open(readback))
def state(row):
    return row.get("state", row.get("status"))
result = {
    "snapshot_sha": e.get("snapshot_sha"),
    "snapshot_manifest_sha256": e.get("snapshot_manifest_sha256"),
    "tr3": rb["tr3"]["id"],
    "tr3_version_before_update": version,
    "approval": approval,
    "review": review,
    "tr3_state": state(rb["tr3"]),
    "tr3_review_at": rb["tr3"].get("review_at"),
    "tr4": rb["tr4"]["id"],
    "tr4_state": state(rb["tr4"]),
    "tr4_authorisation_review_id": rb["tr4"].get("authorisation_review_id"),
    "document": doc,
    "document_version": doc_version,
    "published_at": published,
}
with open(out, "w") as f:
    json.dump(result, f, indent=1)
print("wrote " + out)
PY
say "DONE"
