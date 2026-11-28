# #852 production runbook — resume here

State at handoff (2026-11-28 ~11:00Z): the promotion and the tr3 lifecycles are proven end to
end on a scratch restore; **nothing has been written to production**; the tr3 review has NOT
been recorded. The remaining work is the standards-v4 lifecycle rework for the three tranche
members (see §3), then the live replay, acts, publish, verify and review (§4).

## 1. The transports (recreate in seconds)

```bash
# the restore CLI (a container on conv852-net against conv852-restore):
cat > /tmp/conv852-bin.sh <<'EOF'
#!/bin/bash
exec docker exec -i conv852-bin /usr/local/bin/conveyor "$@"
EOF
# production: the deployed pod's own binary. The host, login and key are the private estate's:
# supply them in the environment (CONVEYOR_HOST, CONVEYOR_SSH_KEY); never commit the address.
cat > /tmp/conv852-pod-bin.sh <<'EOF'
#!/bin/bash
exec ssh -i "${CONVEYOR_SSH_KEY:?set CONVEYOR_SSH_KEY}" "${CONVEYOR_HOST:?set CONVEYOR_HOST}" \
  "sudo KUBECONFIG=/etc/rancher/k3s/k3s.yaml kubectl -n conveyor exec -i deploy/conveyor -- /usr/local/bin/conveyor $(printf '%q ' "$@")"
EOF
```

The scripts here default `WT` to this worktree; override when running elsewhere. Inputs are
`../inputs/` (relative to this runbook).

## 2. The restore (the rehearsal base)

- `/tmp/conv852-conveyor-restore.sql` = the 02:41Z `pg_dumpall` conveyor section, schema 81.
- Container `conv852-restore` (postgres:18) on 127.0.0.1:55459; roles `conveyor_migrate`,
  `conveyor_runtime`, `process`.
- Re-create: `DROP DATABASE conveyor; CREATE DATABASE conveyor OWNER conveyor_migrate;
  psql -f /tmp/conv852-conveyor-restore.sql`, then `docker exec conv852-bin conveyor migrate`
  (schema 110).

## 3. What remains before the exit can be green: the v4 lifecycle rework

Measured on the restore after every act below: the tr3 exit still lists ~35 packages across the
members (S04 ~11, T13 ~12, T3 ~12). They are the full standards-v4 retrospective set and must be
authored with `scripts/rework-project.py` (one project per invocation, one content JSON each —
the same vehicle that repaired S00/T9/T19F/T14; it publishes the typed
`architecture-artifact-instance` pages and repairs artefact bindings itself). The delivery
documents already written here (`../inputs/delivery/*.md`) are the four `std-delivery-*`
packages per member and should be reused as content, not re-authored.

Content order per project: charter → SoAW → the seven A–D artefacts → delivery contract →
Phase-G review → closure record/review. `scripts/rework-content-check.py` validates a content
file before it runs.

After the rework: the **live publish** (`docs/design/scripts/publish-data.sh`, with
`CONVEYOR_BIN` = the deployed binary, `SNAPSHOT_DATABASE_URL` = a read-only live URL, and
`HOMEPAGE` pointed at a scratch dir so the shared homepage checkout is not touched) regenerates
`docs/design/data`, the rendered pages **and** the standards findings; only then do the
`std-*` packages close (the section rule reads the rendered `output-<id>.html`, never the
snapshot body).

## 4. The proven sequence (scripts in this directory)

1. **P2 promotion replay** — `promote-record.py --apply --local-bin <shim> --journal
   ../journal-curated.jsonl --output /tmp/promo1.json --tenant nonagentic`, twice (pass 2 must
   create nothing). Restore rehearsal: pass 1 **268 created / 204 skipped / 0 failed**, pass 2
   **0 created**.
2. **Acts** — `conv852-acts.sh` (contract, Board decision, approve, Phase-G review, schedule and
   cost baselines + decides, registers, T3's seven artefacts, both closes). Restore result:
   **T13 closed, T3 closed through the full gate**. Resumable via `CONTRACT=`, `BOARD=`,
   `REVIEW=`, `BASE_*`, `SKIP_REGISTERS=`, `SKIP_ARTEFACTS=`.
3. **Delivery documents** — `conv852-delivery.sh` (12 v4 documents + S04 `closure_outputs`).
   Not yet exercised (needs the v4 binary; see §5).
4. **P3 evidence** — `conv852-verify.sh` with `MODE=prod SHA=<the deployed pod's
   CONVEYOR_GIT_SHA>`; it snapshots, writes the three receipts, runs `check-tranche-exit` with
   `--evidence-dir` (the fix that makes it read the fresh receipts) and the audit.
5. **P4 review** — `conv852-review.sh` (evidence → `tranches.update` review_at → approvals →
   `tranches.review continue` → read-backs → compose/publish the `tranche-review` document).
   Rehearsed logic; never run live. The evidence arrays must be non-empty UUID lists
   (`tranche_review_has_evidence`); the document scopes to project P0
   `01a1dcb8-d24b-76de-b901-56fd56e001bc`, never the programme id.

## 5. Blockers and traps at handoff

- **The v4 binary must be deployed.** The v4 image IS pushed (`ghcr.io/nonagenticai/conveyor`
  `latest` and `sha-232c033` both report `CONVEYOR_GIT_SHA=232c0339d`; re-pull before trusting a
  local tag). At handoff the **deployed pod still reports `6918ceee6`** (pre-v4) and refuses the
  v4 categories because the seed is compiled in. Check
  `kubectl exec deploy/conveyor -c conveyor -- printenv CONVEYOR_GIT_SHA` before any live act;
  if it is < `232c0339d`, wait for the Flux ImagePolicy roll (or reconcile it) — do not work
  around it. Note main had already moved to `da5e9b6b4` (refresh) at handoff; whichever main sha
  deploys must still be ≥ `232c0339d`.
- The v4 image must be **pulled and used** to recreate `conv852-bin` before the delivery
  rehearsal (the container recipe is in the session log; `latest` must report a v4 sha).
- `documents.compose`/`publish` need `--if-match` in several places; the scripts here handle it.
- The receipt `../tranche-3.md` carries `<action-id>` placeholders; fill them from the live
  outputs (`/tmp/conv852-*.json` per step) before the PR.
- One PR for the lane (`Closes #852`), opened only when the exit is green and the receipts are
  fresh; CI budget 2 runs.
