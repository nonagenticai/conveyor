---
slice: S14
title: Cutover — deploy, seed, rehearse, decommission fr / RM / Planka
status: planned
depends_on: [S00, S06b, S12, S13]
spec_sections: ["§3 D1/D7/D8/D13/D16", "§4.5", "§8 S14", "§9", "§11 R7/R9/R11"]
adds_tables: []
adds_actions: []
adds_events: []
adds_modules: []
parity_ids: ALL-USED (parity/parity.yaml)
---

⚠️ **Amended 2026-11-09 (process model).** The start path is `projects.create` (the fr webhook is retired, not re-pointed); the Redis bus is part of the deployed stack (`conveyor-redis`, no PVC); the trace-diff source is the process trace (`run_step`), and the `workflow` table is dropped by `0034` after this slice's dump. The decommission order is unchanged.

# S14 — Cutover

Programme project: S14 · issues #18, #44

## 1. Objective

At the end, `conveyor` is the only live conveyor service: created (the programme's first git step), deployed, seeded,
rehearsed against the frozen old system, switched at the **agent's conveyor registration**, proven by a real `agent`
task, and fr / requirements_management / Planka decommissioned — each dumped first, with a timed rollback rehearsed.

## 2. Starting state — preconditions are executable, not asserted

| # | Precondition | Check — paste output into `R0-preflight.txt` |
|---|---|---|
| P1 | S13 closed | `ssh -i ~/.ssh/deploy-key admin@192.0.2.10 "sudo -u postgres psql -d n8n -Atc 'select count(*) from workflow_entity where \"activeVersionId\" is not null'"` → `0` — **S13's close deactivates every flow it replaces, so 0 is produced there, not here**; `parity/n8n-dispositions.tsv` (S13's deliverable) has 348 rows |
| P2 | S06b closed | `docs/evidence/S06b-agent-task.md` exists with a `run_id`, a project id and one decided approval, plus the imported tool keys (`projects.create`, not `tool.*`) |
| P3 | `parity.yaml` signed | `yq -r '[.[]|select(.status=="used")]|length' parity/parity.yaml` > 0; schema check green; SUMMARY has a dated sign line |
| P4 | Old system frozen (D8) | `docs/cutover/freeze-log.md` exists, its freeze date is set, and every fr/RM/`kanban` commit merged since has a disposition (`absorbed` / `not-applicable` / `dropped`) |
| P5 | 8930 free | local: `lsof -nP -iTCP:8930 -sTCP:LISTEN` → empty (measured free 2026-11-07). In-cluster: `kubectl get svc -A -o json \| jq -r '.items[].spec.ports[]?.port' \| grep -x 8930` → empty |
| P6 | Disk | `df -h /srv/backups` on master ≥ 3× the three dumps; doubt ⇒ STOP, never prune |

⚠️ P2 is the **local** S06b evidence. The **live** `agent` task against the deployed conveyor is S14's gate (§7), not
its precondition — do not start S14 believing the agent path is already proven.

⚠️ **P2 proves S06b's tool contract, not the one live at T0.** The agent platform is developed in parallel on another
machine. Before step 14, import the deployed conveyor's `/mcp` tool list into a scratch tenant and diff the resulting
op keys and schemas against S06b's evidence. A difference is a cutover decision to record, never a silent pass — the
agent platform is stranded by a renamed tool or a changed schema, not by a 500.

## 3. Scope

**IN:** archived-repo decision; repo + CI; namespace/service/DB/roles/PgBouncer/secret; ImagePolicy; ingress;
Keycloak realm/clients; licence; `migrate --seed`; rehearsals R1–R4; cutover T0; the agent's conveyor registration;
decommission of `fr`/`requirements`/`kanban`; dump + restore test; rollback rehearsal and runbook.

**OUT:** data migration (D7 — nothing is copied; the conveyor starts empty); `mcp_server` (stays as the gateway for
the agent platform's other backends; only its `fr` toolkit retires); `agent` (it changes only its conveyor
registration and credential); `licenses`/`billing_service`/`coding-*` (external, D12/D14); the `n8n` deployment's
removal (flows retired at S13; the idle process stays, see §9.9); `kata-n8n`.

## 4. Source material

| path | read this | carry | do not carry |
|---|---|---|---|
| `SPEC.md` §4.5, §8 S14, §9, §11 R7/R9/R11 | the channel contract, the cutover order, the three risks | the sequence and gates | — |
| `01-CONTRACT.md` §2, §9, §10, §13, §14 | roles, licence, compose/port rules, "no `git init` before repo creation" | the values in §5 | — |
| `02-source-map/40-live-state.md` | Loki `24h`; n8n 348/332; Planka test-only; fr 86 domain tables (90 relations) | the evidence classes | "no traffic" as proof |
| `02-source-map/30-rm-planka-mcp-n8n.md` §2, §3 | the old `mcp_server` channel (historical, superseded); Planka namespace `kanban` | the `fr` toolkit's retirement | the HMAC channel (D19 retires it) |
| `$HOME/Movies/gitops-production` @ `c269b5c96ade` (read-only) | `scripts/new-rust-target.sh`, `.github/workflows/rust-service.yml`; `services/{fr,mcp,pgbouncer,cloudflared-apps}`, `istio/app-hosts/`, `flux-system/image-automation.yaml`, `policies/cross-namespace/` | shapes and current values | — |
| `nonagenticai/conveyor` (archived) | measured 2026-11-07: `archived=true`, `pushed_at 2026-10-29` | the name conflict | its content (dead) |

## 5. Deliverables — the values, fixed here

| Item | Value |
|---|---|
| repo | `nonagenticai/conveyor` **after** the archived repo is renamed (§9.1) |
| namespace / service | `conveyor` / `conveyor.conveyor.svc.cluster.local:8930` |
| host (D16) | `conveyor.nonagentic.ai` — a **new** host; `fr.nonagentic.ai` is not re-pointed |
| DB | `conveyor`; `conveyor_migrate` owns the schema; `conveyor_runtime` is `NOSUPERUSER NOBYPASSRLS` |
| URLs | `DATABASE_URL` → `pgbouncer:6432`; `DATABASE_MIGRATE_URL` → `postgres.example.com:5432` |
| PgBouncer | `conveyor = host=postgres.example.com port=5432 dbname=conveyor pool_size=12` (fr's shape) |
| Keycloak (D13) | realm `conveyor`; `conveyor-web` public PKCE; `conveyor-backend` confidential service account |
| licence | `app_key=conveyor`; `GET /licenses/active?tenant_id=&app_key=conveyor`; synced into `app_licence` |
| ingress | `conveyor-public-vs` in `istio/app-hosts/virtualservices.yaml`, gateway `n8n-gateway`, → `:8930`; host added to `cloudflared-apps/configmap.yaml`; `dns-ensure` creates the CNAME (≤10 min) |
| **channel value 1** | `MCP_FR_BASE_URL`: `http://fr-backend.fr.svc.cluster.local:8888` → **`http://conveyor.conveyor.svc.cluster.local:8930`** |
| **channel value 2** | `MCP_FR_CHANNEL_SECRET` (mcp) = `FR_MCP_CHANNEL_SECRET` (conveyor) — one value, two names; rotated at T0 |
| andon | `POST /api/actions/andon.engage/invocations` and `…/andon.release/invocations` (S06's two actions — there is no separate `/api/andon` route) + an `andon_state` row the engine reads |
| port | **8930** (never 8000) |

Also delivered: `docs/cutover/freeze-log.md`, `docs/cutover/rollback.md`, `docs/cutover/evidence/R0..R7-*`
(`.md`, except the parity record `R5-parity-final.txt`),
`docs/cutover/fixtures/` (recorded payloads).

## 6. Build order

1. **Name decision (day 1).** `GH_SHIM=off gh api repos/nonagenticai/conveyor` → `archived=true`; rename:
   `GH_SHIM=off gh repo rename conveyor-archive -R nonagenticai/conveyor --yes`. George authorises (deleting forfeits
   history). ⚠️ If an archived rename is refused (**UNVERIFIED**): unarchive → rename → re-archive. Verify 404.
2. **Create the repo** (the programme's first `gh repo create`; contract §13 — no local `git init` before it). Run
   `new-rust-target.sh nonagenticai/conveyor --name-ok` with `REGISTRY_USERNAME`/`REGISTRY_PASSWORD` exported and
   `GH_SHIM=off`, after a `--dry-run`. `--name-ok` because the name lacks `-rs`; no `--source`. Then clone, copy the
   local docs tree in, commit, push.
3. **CI wiring.** `.github/workflows/ci.yml` calls `rust-service.yml@main` with `image_name: conveyor`,
   `deploy_namespace: conveyor`, `working_directory: "."`, `cargo_package: ""` (empty ⇒ `--workspace`),
   `rust_toolchain: "1.99.0"` (default 1.98.0 — pass it). Green CI + image in `registry.example.com:5000`.
4. **Database + roles**, direct on the primary (`postgres.example.com`, never through PgBouncer):
   `sudo -u postgres psql -v ON_ERROR_STOP=1 -c "create role conveyor_migrate login password '…'; create role conveyor_runtime login nosuperuser nobypassrls password '…'; create database conveyor owner conveyor_migrate;"`
   New DB, new user — never another service's role (D1).
5. **Deploy shape.** `services/conveyor/{namespace,deployment,service,networkpolicy,secrets.sops.yaml,kustomization}.yaml`;
   `migrate` as an explicit pre-rollout step; PgBouncer stanza; ImageRepository + ImagePolicy in
   `flux-system/image-automation.yaml`; AuthorizationPolicies `mcp → conveyor:8930` and `conveyor → mcp:8033`
   (**measured 2026-11-07**: `services/mcp/deployment.yaml` containerPort `8033`; the external driver is `8036`).
   A missing cross-ns object is a silent refusal. Check: `kubectl -n conveyor get networkpolicy,deploy,svc`.
6. **Ingress.** `conveyor-public-vs` copying `fr-public-vs` (no oauth2-proxy; the app does its own OIDC) plus the
   cloudflared host. Do **not** add the host to `AUTH_REQUIRED_HOSTS` in `envoy-filters/ext-authz-enforce.yaml`.
   Gate: `curl -sf https://conveyor.nonagentic.ai/health`.
7. **Identity + licence.** Realm `conveyor` + both clients via the Keycloak admin REST at
   **`https://auth.nonagentic.ai/admin/realms`** (fr already calls `/admin/realms/{realm}/users/{id}`, measured
   2026-11-07); the `conveyor` app row is `POST /apps` (`operation_id=create_app`, 201) and the tenant licence is
   `POST /licenses/user/{subject_id}` (`create_user_license`) in `licenses` — **measured from `$HOME/Movies/licenses/main.py`**;
   then one `app_licence` sync from `GET /licenses/active?tenant_id=&app_key=conveyor`. Also create **one Keycloak
   test user** in the `conveyor` realm (a real principal with a membership in the cutover tenant) — S12's e2e suite
   logs in with it (`E2E_USER`/`E2E_PASS`), because dev-auth is refused in production.
   Check: `curl -s -H "Authorization: Bearer $KC_TOKEN" https://auth.nonagentic.ai/admin/realms/conveyor | jq -e '.realm=="conveyor"'`.
8. **Seed** — `migrate --seed`, idempotent, no hand-written SQL: the S07 catalogue (re-count: card types 293,
   project-type↔card-type 282, capabilities 68, phases 32, WIP limits 26, project types 26 — `count(*)` first,
   `40-live-state.md` shows estimates), the card-type × phase × list matrix derived from the 280 retired flows, one
   tenant, the first `super_admin` from its Keycloak `sub`, and the licence sync. No operational data (D7).
9. **Rehearsal R1 — real conveyor, n8n still executing** (SPEC §9). Drive one card intake → active →
   `waiting_approval` → done on the new app while `n8n` is live; approval answered over the MCP channel. Evidence
   `R1-live-n8n.md`: card id, run id, approval id, one 204, and proof no request reached fr/RM/Planka.
   Check: `curl -sf localhost:8930/api/actions/cards.create -X POST -H "authorization: Bearer $TOK" -H 'content-type: application/json' -d '{"input":{"board_id":"…","name":"R1 card","card_type_code":"…"}}' | jq -e '.id'`, then the same card's `run` and `approval` ids appear in `GET /api/actions/runs.list`.
10. **Rehearsal R2 — replay.** Replay `docs/cutover/fixtures/` (bodies recorded in S00) through
    `POST /api/actions/{name}/invocations`; every fixture must be accepted or rejected for the *declared* schema reason.
    Evidence `R2-replay.md` (one line per fixture).
    Check: `for f in docs/cutover/fixtures/*.json; do printf '%s %s\n' "$(jq -r .action "$f")" "$(curl -s -o /dev/null -w '%{http_code}' -X POST -H "authorization: Bearer $TOK" -H 'content-type: application/json' "localhost:8930/api/actions/$(jq -r .action "$f")/invocations" -d @"$f")"; done > docs/cutover/evidence/R2-codes.txt` and then `! grep -qE ' (0[0-9]{2}|5[0-9]{2})$' docs/cutover/evidence/R2-codes.txt` — one `<action> <code>` per line, and an unreachable target or a server fault cannot be read as a declared refusal.
11. **Rehearsal R3 — in-app engine.** Execute the S13 re-authored procedures/workflows and diff the execution trace
    against the retired flows. Evidence `R3-inapp-engine.md`. Check: `cargo test -p conveyor-workflow --test trace_diff`.
12. **Rehearsal R4 — rollback drill, timed.** Switch the agent's conveyor registration, run an agent task, switch
    back, run again; record wall-clock minutes (`R4-rollback.md`; target ≤ 15 min). A drill needing a code change has
    failed.
    Check: `S=$(date +%s); <switch + task + switch back>; echo $(( ($(date +%s)-S)/60 )) min` and the number is in the file.
13. **T0 — freeze, dump, final parity.** Engage andon; scale `fr`, `requirements`, `kanban` deployments to 0; then
    `D=/srv/backups/pg/cutover-<date>; sudo -u postgres mkdir -p "$D"; for db in fr requirements_api kanban; do
    sudo -u postgres pg_dump -Fc -d "$db" -f "$D/$db.dump" && sudo -u postgres pg_restore --list "$D/$db.dump"
    > /dev/null && sha256sum "$D/$db.dump"; done`, then `rsync "$D/"` to `mf1:/srv/backups/pg/cutover-<date>/`.
    Then the final parity pass (§7) as `R5-parity-final.txt`. The nightly `pg_dumpall` is **not** a substitute.
14. **T0 — switch the agent's registration.** One change in `agent`: the conveyor external MCP system's `base_url`
    moves to the deployed conveyor and its credential becomes the tenant's conveyor licence. `mcp_server` needs no
    change at T0; its `fr` toolkit is deleted after the live task passes. Merge, then
    `kubectl -n conveyor rollout status deploy/conveyor`, then the live `agent` task (gate). Re-import the deployed
    `/mcp` tool list into a scratch tenant and diff the op keys and schemas against S06b's evidence (a difference is
    a recorded cutover decision). Evidence `R6-agent-task.md`.
15. **T+14 — decommission.** Only if no rollback: probe list (§7) green, dumps restore-tested into a scratch database,
    then `kubectl delete ns fr requirements kanban`; `dropdb fr requirements_api kanban` + `drop role`; remove the
    pgbouncer stanzas, VirtualService, cloudflared host, ImagePolicies; archive the fr / requirements_management repos.
    **Kept:** the three dumps + sha256 on master and mf1, `docs/cutover/`, the freeze log, the `planka-private` image
    digest (90 days). Evidence `R7-decommission.md`.
16. **Rollback runbook** (`docs/cutover/rollback.md`, written before T0, exercised in R4): restore = scale the three
    namespaces back to 1, revert the T0 commit (the registration), `rollout status`, run a live agent task. Writes made
    in the conveyor after T0 are **not** migrated back (R9, no data migration either direction) — say so in the runbook.
    Check: `test -s docs/cutover/rollback.md && grep -q 'not migrated back' docs/cutover/rollback.md`.

## 7. Gate

```bash
set -euo pipefail
source ~/.cargo/env          # CONTRACT §16 — cargo is not on PATH
cd $HOME/Movies/conveyor
# Export before running; nothing here hard-codes a credential or a host.
#   export KUBECONFIG=<the k3s kubeconfig>       BASE=https://conveyor.nonagentic.ai
#   export TOK=<super_admin bearer>              CARD=<a seeded active card id>
#   export C=$HOME/Movies/gitops-production
#   export E2E_USER=<the Keycloak test user>     E2E_PASS=<its password>   (S12's suite logs in with real OIDC
#                                                                          here: dev-auth is refused in production)

# 1. Parity (R5). S00 §7 makes `acceptance` a typed REFERENCE — `test:<crate>::<name>`,
#    `e2e:<spec>::<name>`, `probe:<METHOD> <path>` — not a shell command, so run each KIND
#    rather than piping the strings into a shell (that would exit 127 on the first item).
test -z "$(yq -r '.[] | select(.status == "used") | select((.acceptance // "") | test("^(test|e2e|probe):") | not) | .id' parity/parity.yaml)"
cargo test --workspace                                     # every test:<crate>::<name>
npx playwright install chromium                                          # a clean host has no browser (S12 §6 step 0)
BASE_URL="$BASE" npx playwright test -c apps/web/playwright.config.ts   # every e2e:<spec>::<name>, against the deployed app (S12's config skips its webServer when BASE_URL is set)
yq -r '.[] | select(.status == "used") | .acceptance | select(test("^probe:")) | sub("^probe:"; "")' parity/parity.yaml \
  | while read -r m p; do printf '%s %s %s\n' "$(curl -s -o /dev/null -w '%{http_code}' -H "authorization: Bearer $TOK" -X "$m" "$BASE$p")" "$m" "$p"; done \
  > docs/cutover/evidence/R5-probes.txt
test "$(yq '[.[] | select(.status == "used") | select(.acceptance // "" | test("^probe:"))] | length' parity/parity.yaml)" \
     -eq "$(wc -l < docs/cutover/evidence/R5-probes.txt | tr -d ' ')"     # one row per probe acceptance; empty only when there are none
! grep -qE '^(0[0-9][0-9]|5[0-9][0-9]) ' docs/cutover/evidence/R5-probes.txt   # no connect failure (000) and no 5xx
# R5-parity-final.txt still records pass/fail per parity id — the probes above prove reachability,
# the per-item expectation lives in that file.

# 2. The new app is alive.
test "$(curl -s -o /dev/null -w '%{http_code}' "$BASE/health")" = 200

# 3. Andon on the new app (S06 §5.2): `andon.engage {reason}`, `andon.release {}` — andon is
#    per-tenant singleton state, so there is no id to pass. The 409 is on `runs.create`.
test "$(curl -s -o /tmp/andon-engage.json -w '%{http_code}' -X POST "$BASE/api/actions/andon.engage/invocations" -H "content-type: application/json" \
      -H "authorization: Bearer $TOK" -d '{"input":{"reason":"cutover gate"}}')" = 200 && jq -e . /tmp/andon-engage.json >/dev/null
test "$(curl -s -o /dev/null -w '%{http_code}' -X POST "$BASE/api/actions/runs.create/invocations" -H "content-type: application/json" \
      -H "authorization: Bearer $TOK" -d "{\"input\":{\"card_id\":\"$CARD\"}}")" = 409
test "$(curl -s -o /tmp/andon-release.json -w '%{http_code}' -X POST "$BASE/api/actions/andon.release/invocations" -H "content-type: application/json" \
      -H "authorization: Bearer $TOK" -d '{"input":{}}')" = 200 && jq -e . /tmp/andon-release.json >/dev/null

# 4. The legacy n8n deployment still exists and this gauge still tracks it (idle, not decommissioned).
test -n "$(kubectl -n n8n get deploy n8n-worker -o name)"

# 5. No path still reaches fr / RM / Planka — object absence, not belief.
test -z "$(kubectl get ns fr requirements kanban --ignore-not-found -o name)"   # prints nothing
! kubectl get svc,deploy -A | grep -qE 'fr-backend|requirements|kanban'               # no match
! grep -qE '^(fr|kanban|requirements)' "$C/clusters/strato/services/pgbouncer/pgbouncer.ini"
! grep -rq 'fr.nonagentic.ai' "$C/clusters/strato/services/cloudflared-apps/configmap.yaml" \
      "$C/clusters/strato/istio/app-hosts/"
test "$(curl -s -o /dev/null -w '%{http_code}' https://fr.nonagentic.ai/health)" != 200   # from OUTSIDE the cluster

# 6. Manual evidence that must be on disk before this gate is green.
for f in R1-live-n8n.md R2-replay.md R3-inapp-engine.md R4-rollback.md R5-parity-final.txt R6-agent-task.md; do test -s "docs/cutover/evidence/$f" || { echo "missing evidence: $f" >&2; exit 1; }; done
test -s docs/cutover/evidence/R6-agent-task.md   # one real `agent` task end to end: projects.create →
                                                 # pending-approval → approved through `mcp_server` →
                                                 # approval.closed → run `succeeded`; project id on
                                                 # `external_ref`; one activity row
test -s docs/cutover/evidence/R4-rollback.md     # rollback executed once, timed, ≤ 15 min
```

Loki is a **24 h sample**: it can never prove completeness. Only object absence does.

## 8. Traps

1. **The archived repo owns the name.** `gh repo create` returns 422 while `nonagenticai/conveyor` exists
   (measured `archived=true`, `pushed_at 2026-10-29`). Rename, do not delete.
2. **`fr.nonagentic.ai` is still live and serving.** D16 keeps the host with the old app until decommission; there is
   no DNS switch. Re-pointing it early is a split-brain, not a cutover.
3. **Stranding the agent platform.** One URL + one secret under two names; the MAC is over **raw bytes**, so a
   re-serialising proxy 401s everything (±300 s skew likewise). Use the in-cluster URL, not the Cloudflare host.
4. **Deleting a DB before its dump.** D7 discards the data; it does not license dropping a database that was never
   dumped and restore-tested.
5. **`pg_dumpall` runs on master** (02:40 UTC, retain 3) — it is not a cutover dump, and a `dropdb` on master
   propagates to mf1's streaming standby the moment it commits.
6. **The freeze is a rule, not a mood.** A "quick fix" to fr/RM/Planka during S14 goes in `freeze-log.md` and is
   absorbed or dropped.
7. **The ext_authz allowlist is an enumeration** — do not add the conveyor host to `AUTH_REQUIRED_HOSTS`.

## 9. Decisions closed here

1. The archived repo is **renamed to `conveyor-archive`**, never deleted; George authorises, the session executes with
   `GH_SHIM=off`.
2. Warm window = **14 days**; the **point of no return is `dropdb`**, not the switch — scaling and re-pointing are
   reversible, dropping is not.
3. Cutover is **host-parallel**: `conveyor.nonagentic.ai` is new; `fr.nonagentic.ai` is retired only at decommission.
4. The channel base URL is the **in-cluster** service address; the secret is **rotated at T0** and both names are
   changed in one commit.
5. A pre-deletion dump is mandatory evidence (three `-Fc` dumps, restore-tested) even though D7 discards the data.
6. Rehearsal evidence is numbered `R1`–`R4`, cutover `R5`–`R7`, under `docs/cutover/evidence/`.
7. The andon gate is the conveyor's `andon_state`; it does **not** rely on the fr UI, which is gone.
8. Freeze date = the SPEC approval date (2026-11-07); `freeze-log.md` starts there.
9. The legacy `n8n` deployment stays idle, undecommissioned (flows died at S13; DB dumped at T+14).

## 10. Open questions

1. **Who gives the T0 go?** Default: George, in writing, after R1–R4 are green.
2. **14 or 30 days warm?** Default 14.
3. **Old repos archived or deleted?** Default archive; deletion is a separate, later decision.
4. **First `super_admin`** — default `georgedekker`'s Keycloak subject.
5. **Licence minting path** — resolved 2026-11-07 from `$HOME/Movies/licenses/main.py`: register the app with
   `POST /apps` (`create_app`, 201), mint the tenant licence with `POST /licenses/user/{subject_id}`
   (`create_user_license`), and sync from `GET /licenses/active` (`get_active_license`). Step 7 executes exactly that.
