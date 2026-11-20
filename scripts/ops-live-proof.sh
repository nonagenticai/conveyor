#!/usr/bin/env bash
# The P1.4 live proof: an injected pod failure in the tenant ops namespace, mitigated by an
# approved destructive write, resolved with a linked postmortem (conveyor#78 P1.4, conveyor#8 P7).
#
# The deployed service has no documented headless bearer token, so the operator CLI is the
# supported write path (skill `conveyor-api`). Every step is one action through the real engine;
# nothing here writes SQL.
#
# The mitigation is DESTRUCTIVE and its approval is a HUMAN decision: this script requests the
# gate and stops. A person decides it in the console (or with `approvals.decide` under their own
# identity — never the service actor that requested it), then `continue` resumes the lifecycle.
#
# Usage:
#   scripts/ops-live-proof.sh inject    # crash-looping Deployment into the tenant ops namespace
#   scripts/ops-live-proof.sh open      # ops.incident on the programme board
#   scripts/ops-live-proof.sh triage     # ops.triage with the published crash-loop runbook
#   scripts/ops-live-proof.sh request    # approvals.request for the restart (prints the id)
#   scripts/ops-live-proof.sh wait       # poll until the approval is decided
#   scripts/ops-live-proof.sh mitigate   # ops.mitigate under the approved gate
#   scripts/ops-live-proof.sh resolve     # ops.postmortem + ops.resolve
#   scripts/ops-live-proof.sh assert      # read the card meta back
#   scripts/ops-live-proof.sh cleanup     # delete the fixture Deployment
#
# State (ids) is kept in a state file so the steps can run in separate invocations around the
# human decision.
set -uo pipefail

CTL=${CTL:-$HOME/.claude/skills/conveyor-api/scripts/conveyor-ctl}
TENANT=${TENANT:-nonagentic}
NAMESPACE=${NAMESPACE:-nonagentic-ops}
DEPLOYMENT=${DEPLOYMENT:-ops-proof}
STATE=${STATE:-/tmp/ops-live-proof.state}
# The host and key are the operator's, from the environment — never committed (the public repo
# refuses internal hostnames and tailnet addresses; the same values live in the conveyor-api skill).
: "${CONVEYOR_SSH_KEY:?export CONVEYOR_SSH_KEY (the operator key; see the conveyor-api skill)}"
: "${CONVEYOR_HOST:?export CONVEYOR_HOST (user@host; see the conveyor-api skill)}"
KUBECTL_SSH=(ssh -i "$CONVEYOR_SSH_KEY" -o ConnectTimeout=10 "$CONVEYOR_HOST")
KUBECTL=(sudo KUBECONFIG=/etc/rancher/k3s/k3s.yaml kubectl)

say() { printf '%s\n' "$*"; }
die() { say "FAILED: $*" >&2; exit 1; }
load_state() { [ -f "$STATE" ] && . "$STATE" || true; }
save() { printf '%s=%q\n' "$1" "$2" >> "$STATE"; }
kube() { "${KUBECTL_SSH[@]}" "${KUBECTL[*]} $*"; }

# A destructive invoke, with the approved gate named on the CALL (the wrapper has no flag for
# it, so this is the raw documented form).
invoke_gated() {
  local action="$1" approval="$2" input="$3"
  ssh -i "$CONVEYOR_SSH_KEY" -o ConnectTimeout=10 "$CONVEYOR_HOST" \
    sudo KUBECONFIG=/etc/rancher/k3s/k3s.yaml kubectl -n "${CONVEYOR_NAMESPACE:-conveyor}" \
    exec -i "${CONVEYOR_DEPLOY:-deploy/conveyor}" -- \
    conveyor admin action invoke --tenant "$TENANT" --name "$action" \
    --input /dev/stdin --approval-id "$approval" <<<"$input"
}

# The board/list: the T9 architecture project's board carries the programme's operational cards.
board() {
  if [ -z "${BOARD_ID:-}" ]; then
    die "BOARD_ID is unset (export BOARD_ID=<uuid> LIST_ID=<uuid> RUNBOOK_ID=<uuid>)"
  fi
}

cmd_inject() {
  load_state
  say "== inject: a crash-looping Deployment $DEPLOYMENT in $NAMESPACE"
  kube apply -f - <<EOF
apiVersion: apps/v1
kind: Deployment
metadata:
  name: $DEPLOYMENT
  namespace: $NAMESPACE
  labels: { app: ops-proof, conveyor.nonagentic.ai/family: ops-proof }
spec:
  replicas: 1
  selector: { matchLabels: { app: ops-proof } }
  template:
    metadata: { labels: { app: ops-proof } }
    spec:
      securityContext:
        runAsNonRoot: true
        runAsUser: 65532
        seccompProfile: { type: RuntimeDefault }
      containers:
        - name: crash
          image: busybox:1.36
          command: ["sh", "-c", "echo injected failure; exit 1"]
          securityContext:
            allowPrivilegeEscalation: false
            capabilities: { drop: ["ALL"] }
          resources: { requests: { cpu: 10m, memory: 16Mi }, limits: { cpu: 50m, memory: 32Mi } }
EOF
  say "== wait for the pod to fail (the runbook's trigger)"
  for _ in $(seq 1 30); do
    waiting=$("${KUBECTL_SSH[@]}" "${KUBECTL[*]} -n $NAMESPACE get pods -l app=ops-proof -o jsonpath='{.items[*].status.containerStatuses[*].state.waiting.reason}'" 2>/dev/null || true)
    case "$waiting" in *CrashLoopBackOff*|*Error*) say "== pod is failing: $waiting"; return 0;; esac
    sleep 4
  done
  say "WARNING: the pod has not reached CrashLoopBackOff yet; continuing with the incident"
}

cmd_open() {
  load_state; board
  say "== ops.incident"
  local doc; doc=$(mktemp)
  python3 - "$doc" "$BOARD_ID" "$LIST_ID" <<'PY'
import json,sys
json.dump({"input": {"board_id": sys.argv[2], "list_id": sys.argv[3],
                     "service": "ops-proof", "severity": "sev2",
                     "title": "Incident: injected pod failure in the tenant ops namespace",
                     "evidence": {"detected_by": "scripts/ops-live-proof.sh",
                                  "namespace": "nonagentic-ops", "deployment": "ops-proof"}}}, open(sys.argv[1],"w"))
PY
  local out; out=$("$CTL" ops.incident "$(cat "$doc")") || die "ops.incident refused"
  INCIDENT_ID=$(printf '%s' "$out" | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')
  save INCIDENT_ID "$INCIDENT_ID"; say "== incident $INCIDENT_ID"
}

cmd_triage() {
  load_state; [ -n "${INCIDENT_ID:-}" ] || die "run open first"
  [ -n "${RUNBOOK_ID:-}" ] || die "RUNBOOK_ID is unset (a PUBLISHED procedure id)"
  say "== ops.triage"
  local doc; doc=$(mktemp)
  python3 - "$doc" "$INCIDENT_ID" "$RUNBOOK_ID" <<'PY'
import json,sys
json.dump({"input": {"incident_id": sys.argv[2], "severity": "sev2", "runbook_procedure_id": sys.argv[3],
                     "narrative": "The injected workload exits 1 on start; the runbook restarts it and reads the rollout back."}}, open(sys.argv[1],"w"))
PY
  "$CTL" ops.triage "$(cat "$doc")" >/dev/null || die "ops.triage refused"
  say "== triaged (no cluster write)"
}

mitigation_input() {
  python3 - "$INCIDENT_ID" "$NAMESPACE" "$DEPLOYMENT" <<'PY'
import json,sys
print(json.dumps({"incident_id": sys.argv[1], "action": "ops.restart",
                  "input": {"namespace": sys.argv[2], "deployment": sys.argv[3]}}))
PY
}

cmd_request() {
  load_state; [ -n "${INCIDENT_ID:-}" ] || die "run open first"
  say "== approvals.request for ops.mitigate"
  local doc; doc=$(mktemp)
  python3 - "$doc" "$INCIDENT_ID" "$NAMESPACE" "$DEPLOYMENT" <<'PY'
import json,sys
mitigation = {"incident_id": sys.argv[2], "action": "ops.restart",
              "input": {"namespace": sys.argv[3], "deployment": sys.argv[4]}}
# The CLI unwraps a top-level "input" member, so the document is nested once more.
json.dump({"input": {"gate": "ops.mitigate", "kind": "ops", "card_id": sys.argv[2],
                     "description": "Restart the injected workload after the crash-loop runbook selected it",
                     "action": "ops.mitigate", "input": mitigation}}, open(sys.argv[1],"w"))
PY
  local out; out=$("$CTL" approvals.request "$(cat "$doc")") || die "approvals.request refused"
  APPROVAL_ID=$(printf '%s' "$out" | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')
  save APPROVAL_ID "$APPROVAL_ID"
  say "== approval $APPROVAL_ID requested — a HUMAN must decide it; then run: $0 wait && $0 mitigate"
}

cmd_wait() {
  load_state; [ -n "${APPROVAL_ID:-}" ] || die "run request first"
  say "== waiting for a human decision on $APPROVAL_ID (never decided by the requesting actor)"
  for _ in $(seq 1 120); do
    status=$(printf '{"id":"%s"}' "$APPROVAL_ID" | "$CTL" approvals.get 2>/dev/null | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("status") or d.get("approval",{}).get("status",""))' 2>/dev/null || true)
    case "$status" in
      approved) say "== approved"; return 0;;
      rejected|cancelled) die "the approval is $status";;
    esac
    sleep 5
  done
  die "no decision within 10 minutes"
}

cmd_mitigate() {
  load_state; [ -n "${INCIDENT_ID:-}" ] || die "run open first"; [ -n "${APPROVAL_ID:-}" ] || die "run request first"
  say "== ops.mitigate under $APPROVAL_ID"
  local doc; doc=$(mktemp); mitigation_input > "$doc"
  invoke_gated ops.mitigate "$APPROVAL_ID" "$(cat "$doc")" >/dev/null || die "ops.mitigate refused"
  say "== mitigated: the restart was applied and read back"
}

cmd_resolve() {
  load_state; [ -n "${INCIDENT_ID:-}" ] || die "run open first"
  say "== ops.postmortem"
  local doc; doc=$(mktemp)
  python3 - "$doc" "$INCIDENT_ID" <<'PY'
import json,sys
json.dump({"input": {"incident_id": sys.argv[2],
                     "evidence": {"read_back": "deployment restart observed", "method": "scripts/ops-live-proof.sh"},
                     "postmortem": {"title": "Postmortem: injected crash loop in nonagentic-ops",
                                    "body": "An injected workload in the tenant ops namespace exited 1 on start. The crash-loop runbook triaged it with no cluster write, a human approved the restart, the write read back, and the incident resolved. Not systemic: the workload is a proof fixture.",
                                    "systemic": False}}}, open(sys.argv[1],"w"))
PY
  "$CTL" ops.postmortem "$(cat "$doc")" >/dev/null || die "ops.postmortem refused"
  say "== postmortem written"
  say "== ops.resolve"
  printf '{"incident_id":"%s"}' "$INCIDENT_ID" > "$doc"
  local wrapped; wrapped=$(mktemp); python3 - "$doc" "$wrapped" <<'PY'
import json,sys
json.dump({"input": json.load(open(sys.argv[1]))}, open(sys.argv[2],"w"))
PY
  "$CTL" ops.resolve "$(cat "$wrapped")" >/dev/null || die "ops.resolve refused"
  say "== resolved"
}

cmd_assert() {
  load_state; [ -n "${INCIDENT_ID:-}" ] || die "run open first"
  printf '{"id":"%s"}' "$INCIDENT_ID" | "$CTL" cards.get > /tmp/ops-live-card.json || die "cards.get failed"
  python3 - <<'PY'
import json,sys
card = json.load(open("/tmp/ops-live-card.json"))
card = card.get("card", card)
meta = card.get("meta", {})
required = ["service", "severity", "detected_at", "mitigated_at", "resolved_at", "runbook_procedure_id", "postmortem_asset_id"]
missing = [k for k in required if not meta.get(k)]
if missing:
    print(f"FAILED: the incident meta is missing {missing}: {meta}", file=sys.stderr); sys.exit(1)
print(f"ASSERT OK: {card.get('name')} meta={ {k: meta[k] for k in required} }")
PY
}

cmd_cleanup() {
  load_state
  kube delete deployment "$DEPLOYMENT" -n "$NAMESPACE" --ignore-not-found
  rm -f "$STATE"
  say "== fixture removed"
}

case "${1:-}" in
  inject) cmd_inject;;
  open) cmd_open;;
  triage) cmd_triage;;
  request) cmd_request;;
  wait) cmd_wait;;
  mitigate) cmd_mitigate;;
  resolve) cmd_resolve;;
  assert) cmd_assert;;
  cleanup) cmd_cleanup;;
  *) die "usage: $0 {inject|open|triage|request|wait|mitigate|resolve|assert|cleanup}";;
esac
