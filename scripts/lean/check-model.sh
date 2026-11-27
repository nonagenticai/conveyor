#!/usr/bin/env bash
# Check the Lean model and its unit corpus in the prover's own project (conveyor#836).
#
# The prover's runner issues exactly this command, so "valid" here means what the API answers
# without going through HTTP:
#
#   cd /lean-project && env -i PATH=... ELAN_HOME=... lake env lean <file>
#
# Modes:
#   model                          the model alone (its universal theorems, no facts)
#   controls                       the model + docs/spec/lean/UnitModel.controls.lean; every
#                                  `theorem control_` must be kernel-refused
#   corpus --sizes 40,80,200       sources emitted by scripts/proof/a0_units.py, each exit 0
#
# The pod is found in the lean4-prover namespace on the k3s control plane. Override the access
# with LEAN4_SSH_HOST / LEAN4_SSH_KEY / LEAN4_NAMESPACE when needed. Files are streamed in and
# removed afterwards; nothing is restarted.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
NAMESPACE="${LEAN4_NAMESPACE:-lean4-prover}"
# The public tree carries no estate address: name the control plane and its key in the
# environment of the session that runs this check.
SSH_HOST="${LEAN4_SSH_HOST:?set LEAN4_SSH_HOST to the k3s control-plane login}"
SSH_KEY="${LEAN4_SSH_KEY:?set LEAN4_SSH_KEY to the private key for it}"
KUBE="sudo KUBECONFIG=/etc/rancher/k3s/k3s.yaml kubectl -n $NAMESPACE"

remote() {
  ssh -i "$SSH_KEY" -o ConnectTimeout=10 "$SSH_HOST" "$@"
}

pod() {
  remote "$KUBE get pods --field-selector=status.phase=Running \
    -o jsonpath='{.items[0].metadata.name}'"
}

POD="$(pod)"
if [ -z "$POD" ]; then
  echo "no running pod in namespace $NAMESPACE" >&2
  exit 2
fi

now_ms() {
  python3 -c 'import time; print(int(time.time() * 1000))'
}

# run_source <name> <file>: stream the file into the pod, run the runner's command, remove it.
# Returns the prover command's exit code; its output goes to stdout/stderr.
run_source() {
  local name="$1" file="$2"
  remote "$KUBE exec -i $POD -- sh -c 'cat > /lean-project/_check_${name}.lean'" <"$file"
  local rc=0
  remote "$KUBE exec $POD -- sh -c 'cd /lean-project && env -i \
PATH=/usr/local/bin:/usr/bin:/bin:/opt/elan/bin ELAN_HOME=/opt/elan \
/opt/elan/bin/lake env /opt/elan/bin/lean /lean-project/_check_${name}.lean'" || rc=$?
  remote "$KUBE exec $POD -- sh -c 'rm -f /lean-project/_check_${name}.lean'" \
    >/dev/null 2>&1 || true
  return "$rc"
}

mode="${1:-}"
case "$mode" in
  model)
    start="$(now_ms)"
    if run_source model "$ROOT/docs/spec/lean/DeliveryModel.lean"; then
      rc=0
    else
      rc=$?
    fi
    end="$(now_ms)"
    printf 'model exit=%s wall_ms=%s\n' "$rc" "$((end - start))"
    exit "$rc"
    ;;

  controls)
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    {
      cat "$ROOT/docs/spec/lean/DeliveryModel.lean"
      printf '\n'
      cat "$ROOT/docs/spec/lean/UnitModel.controls.lean"
    } >"$tmp/controls.lean"
    expected="$(grep -c '^theorem control_' "$ROOT/docs/spec/lean/UnitModel.controls.lean")"

    if run_source controls "$tmp/controls.lean" >"$tmp/controls.out" 2>&1; then
      echo "UNEXPECTED: the controls passed; the model cannot fail" >&2
      cat "$tmp/controls.out" >&2
      exit 1
    fi
    errors="$(grep -c 'error:' "$tmp/controls.out" || true)"
    if [ "$errors" -ne "$expected" ]; then
      echo "controls: $errors refusal(s), expected $expected" >&2
      cat "$tmp/controls.out" >&2
      exit 1
    fi
    echo "controls: $errors/$expected kernel-refused"
    exit 0
    ;;

  corpus)
    shift
    sizes="40,80,200"
    while [ $# -gt 0 ]; do
      case "$1" in
        --sizes)
          sizes="$2"
          shift 2
          ;;
        *)
          echo "unknown argument: $1" >&2
          exit 2
          ;;
      esac
    done
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    python3 -I "$ROOT/scripts/proof/a0_units.py" --out "$tmp" >/dev/null
    printf '%-6s %-8s %-6s %-8s\n' N bytes exit wall_ms
    for n in ${sizes//,/ }; do
      file="$tmp/ledger_${n}.lean"
      if [ ! -f "$file" ]; then
        echo "the emitter produced no ledger_${n}.lean" >&2
        exit 2
      fi
      bytes="$(wc -c <"$file" | tr -d ' ')"
      start="$(now_ms)"
      if run_source "corpus_${n}" "$file" >"$tmp/out_${n}" 2>&1; then
        rc=0
      else
        rc=$?
      fi
      end="$(now_ms)"
      printf '%-6s %-8s %-6s %-8s\n' "$n" "$bytes" "$rc" "$((end - start))"
      if [ "$rc" -ne 0 ]; then
        cat "$tmp/out_${n}" >&2
        exit 1
      fi
    done
    exit 0
    ;;

  *)
    echo "usage: $0 model|controls|corpus [--sizes 40,80,200]" >&2
    exit 2
    ;;
esac
