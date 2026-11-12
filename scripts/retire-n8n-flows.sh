#!/usr/bin/env bash
# S13 step 15b — retire the legacy n8n flows.
#
# The n8n public API caps a page at 250 of the 348 flows, so this pages by `cursor`, asserts the
# count, and only then (with --apply) flips every flow to `active: false`. The count is an
# assertion, not a report: a flow that appeared after the migration is not ours to retire, so a
# different count refuses the run.
#
# Dry-run is the default. `--apply` is S14 P1: it must not run until the conveyor image carrying
# the re-authored flows (and its hook secret) is serving, because the old flows are the estate's
# live automation until then. The n8n container keeps running; only the triggers are turned off.
#
# Usage (from a pod in namespace `n8n`, or a host with the API reachable):
#   N8N_API_KEY=... ./scripts/retire-n8n-flows.sh            # list, change nothing
#   N8N_API_KEY=... ./scripts/retire-n8n-flows.sh --apply    # flip every flow inactive
#
# After --apply the script asserts the API's active list is empty. The DB half of the assertion
# (`SELECT count(*) FROM workflow_entity WHERE active` = 0) is printed rather than run here: the
# database credential is not this script's to hold.
set -euo pipefail

N8N_URL="${N8N_URL:-http://n8n.n8n.svc.cluster.local:5678}"
EXPECTED="${EXPECTED_FLOWS:-348}"

if [[ -z "${N8N_API_KEY:-}" ]]; then
  echo "retire-n8n-flows: N8N_API_KEY is not set" >&2
  exit 2
fi

api() {
  curl -fsS -H "X-N8N-API-KEY: ${N8N_API_KEY}" "$@"
}

ids=()
cursor=""
while :; do
  url="${N8N_URL}/api/v1/workflows?limit=250"
  if [[ -n "${cursor}" ]]; then
    url="${url}&cursor=${cursor}"
  fi
  page="$(api "${url}")"
  while IFS= read -r id; do
    [[ -n "${id}" ]] && ids+=("${id}")
  done < <(jq -r '.data[].id' <<<"${page}")
  cursor="$(jq -r '.nextCursor // empty' <<<"${page}")"
  [[ -z "${cursor}" ]] && break
done

count="${#ids[@]}"
echo "retire-n8n-flows: ${count} flows (expected ${EXPECTED})"
if [[ "${count}" -ne "${EXPECTED}" ]]; then
  echo "REFUSED: the flow count changed; review the new/removed flows before retiring" >&2
  exit 1
fi

if [[ "${1:-}" != "--apply" ]]; then
  printf '%s\n' "${ids[@]}" | head -5
  echo "... ${count} ids; dry run, nothing changed (pass --apply to retire)"
  exit 0
fi

for id in "${ids[@]}"; do
  curl -fsS -X PATCH \
    -H "X-N8N-API-KEY: ${N8N_API_KEY}" \
    -H 'content-type: application/json' \
    -d '{"active":false}' \
    "${N8N_URL}/api/v1/workflows/${id}" >/dev/null
done

remaining="$(api "${N8N_URL}/api/v1/workflows?active=true&limit=1" | jq '.data | length')"
if [[ "${remaining}" -ne 0 ]]; then
  echo "REFUSED: ${remaining} flows are still active after the sweep" >&2
  exit 1
fi
echo "retired ${count} flows; the API's active list is empty."
echo "DB assertion (run on the n8n database): SELECT count(*) FROM workflow_entity WHERE active; -- expect 0"
