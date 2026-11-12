# IMPROVE-01 — built-tree hardening, receipt

**Date:** 2026-11-09T06:21Z– · **Session:** `improve-01` · **Tree:** `$HOME/Movies/conveyor` (no `.git`)
**Lease:** `improve-01`, opened 06:21Z, released with the gate below. Split tree by message with
`s11-gates-cancel-seam` (S11): no `procedure/**`, `store/src/{run,procedure*}.rs`, `worker.rs`,
`migrations/0015-0017, 0021` or S11 index row was touched.

## Files

| path | change |
|---|---|
| `crates/actions/tests/copilot_coverage.rs` | **+** `no_proposal_triggering_action_carries_a_secret` and its recursive `x-secret` walker |
| `crates/api/tests/copilot_rounds.rs` | **new binary** — the `MAX_ROUNDS` refusal |
| `crates/api/tests/copilot_crash_replay.rs` | **new binary** — the crash-window replay |
| `crates/actions/src/spec.rs` | `AGENT_TOOLS` 9 → 12 (`boards.get`, `cards.create`, `runs.create`) + rationale |
| `crates/api/tests/mcp_agent.rs` | the run chain over `/mcp`; the inverted `boards.get` assertion; receipt carries a real `run_id` |
| `crates/actions/src/integrations/external.rs` | `retrieval.query` path `/query` → `/api/query` |
| `crates/api/tests/externals_declared.rs` | **+** `every_external_action_declares_its_services_path_prefix` |
| `docs/plans/slices/S{06b,07,08,09,10}-*.md` | gate lines corrected to targets that exist (§B) |
| `docs/plans/00-INDEX.md` | the `improve-01` lease row |

## What was run

| # | command | result |
|---|---|---|
| 1 | `cargo test -p conveyor-actions --test copilot_coverage` | **3 passed** (`no_proposal_triggering_action_carries_a_secret` green) |
| 2 | reversal: `secrets.create` `confirm = "none"` → `"card"` | **failed by name** — `["secrets.create (value)"]`; reverted |
| 3 | `cargo test -p conveyor-api --test copilot_rounds` | **1 passed** |
| 4 | `cargo test -p conveyor-api --test copilot_crash_replay` | **1 passed** |
| 5 | `cargo test -p conveyor-api --test mcp_agent` | **2 passed** |
| 6 | `cargo test -p conveyor-api --test externals_declared` | **3 passed** |
| 7 | reversal: `path: "/api/query"` → `"/query"` | **failed by name** — `retrieval.query declares /query, but strata_rag serves under "/api"`; restored |
| 8 | `cargo test -p conveyor-api --test copilot_{approve,proposal,replay,reap,session,surface} --test agui_parity` | **8 passed** (S10 regression) |
| 9 | `cargo test -p conveyor-actions --test registry` / `--test parity_http_mcp` / `--lib` | **6 / 4 / 34 passed** |
| 10 | `cargo test -p conveyor-store --test s07_schema` | **3 passed** — ledger `[1,2,3,4,5,7..18,21]` |
| 11 | `cargo clippy -p conveyor-api -p conveyor-actions -p conveyor-store -p conveyor-domain --all-targets -- -D warnings` | **green** |
| 12 | `cargo run -q -p conveyor-server -- --print-openapi --check-lock` | **exit 0** — 209 ops unchanged |
| 13 | corrected **S09** gate (8 binaries) | **30 passed**, 0 failed |
| 14 | corrected **S07** gate (6 binaries) | **36 passed**, 0 failed |
| 15 | corrected **S08** gate (5 binaries) | **28 passed**, 0 failed |
| 16 | `scripts/prove-s3.sh` (RustFS on `127.0.0.1:9000`) | **`S3_WIRE_PROVEN`** — real endpoint, path-style, SigV4 |
| 17 | `cargo test -p conveyor-api --test search_eval -- --ignored` | **1 passed** — `full_text_beats_the_ilike_floor` |

## Receipt — the S06b run (verbatim, `--nocapture`)

```
S06B-RECEIPT {"agent_tools":12,"approval_id":"01a1befc-d75a-7ee9-8867-6e76146863dd","approval_meta":{"allowed_decisions":["approve","reject"],"description":"Which of the two designs should this follow?","proposed_answer":"the first"},"base_url":"http://127.0.0.1:55851","decided_by_kind":"agent","decision":"approve","project_id":"01a1befc-d723-7d28-9b98-e3d14230019e","run_id":"01a1befc-d749-7e50-bafe-acc9a861d423","via":{"channel":"mcp"}}
```

`run_id` is non-null: the agent ran `projects.get` → `boards.get` → `cards.create` → `runs.create` over `/mcp`
under its licence principal. The run is **card-scoped**, not S06c's project-execution task.

## Clause table

| clause | verdict | evidence |
|---|---|---|
| S10 §5.3 — a secret-bearing action cannot become proposal-triggering without redaction | **PROVEN (fail-closed)** | #1 + #2. The guard is over the *next* action; `secrets.create`/`secrets.rotate` are `confirm = "none"` today, so nothing is proposal-triggering and no redactor is written (an unused redactor is untested code) |
| §17 "hard cap, never silent truncation" | **PROVEN** | #3: 8 recorded tool calls, 8 `completions.create` invocations, 8 stub hits, the 9th never requested, terminal `refused` |
| S10-D3 — `proposal:<id>` makes a retry a replay | **PROVEN** | #4: after the crash window the second `execute()` returns `Executed(same invocation)` and the invocation count stays 1 |
| S09 §5.5 — every external path is the service's real path | **PROVEN** | #6 + #7; the table covers all 7 services and panics by name on an unknown one |
| S14 P2 — the evidence contains a `run_id` | **PROVEN (card-scoped)** | #5 + the receipt above |
| S07/S08/S09 gates are executable | **PROVEN** | #13–#15. Each block previously named 3–4 targets that do not exist |

## Corrections this lane made to its own plan — recorded, not hidden

1. **"No new binary" was wrong.** `crates/api/tests/copilot/mod.rs` states that each copilot test binary runs **one**
   test because `CONVEYOR_EXTERNAL_INFERENCE_URL` is process-global; a second test in `copilot_turn.rs` inherited
   the empty URL the first test sets and got a 503. Both new proofs are their own binaries
   (`copilot_rounds`, `copilot_crash_replay`).
2. **A refused turn is HTTP 201, not 422.** `turns.rs:209` returns `CREATED` for every recorded turn; the problem
   status is returned only for `failed`. S10 specifies `refused` as a **turn status**, not a status code, so the
   test asserts the record (`meta.status`, `meta.error.type` = `/problems/copilot-refused`, `error.status` 422).
   ⚠️ The asymmetry — `failed` returns its status, `refused` does not — is recorded here as an **observation**, not
   changed: altering a client-visible status without a spec is a bigger decision than this lane.
3. **`cargo test -p conveyor-store --test migrate` does not exist.** The plan's own gate line had the defect this
   lane exists to remove; the migration ledger is `--test s07_schema`.

## Docs corrected (the "updated spec and plans" the next sessions build from)

| where | was | now |
|---|---|---|
| `01-CONTRACT.md` §17 | refusal code `action-not-permitted` | `action-not-found` (owner ruling; the code never had the other name) |
| `SPEC.md:315` | "S8 and S13 … can run in parallel once S2–S4 exist" | S8 only; S13 depends on S11 |
| `SPEC.md:315` | "S14 is blocked by two things" | three — S12 was omitted |
| `S06b:115,132` | `cargo test -p conveyor-actions approvals` (0 tests) | `--test approval_dual --test mcp_agent`; the vacuous line removed |
| `S07:173` | 5 non-existent targets | real targets; block run (36 tests) |
| `S08:315` | 3 non-existent targets | real targets; block run (28 tests) |
| `S09:192` | 4 non-existent targets | real targets; block run (30 tests) |
| `S09:251` | "the S3 wire is UNVERIFIED" | **PROVEN** by `scripts/prove-s3.sh` |
| `S10:126` | `cargo test -p conveyor-actions external_inference` (0 tests) | `--lib inference` (2 tests) |
| `00-INDEX` S06b / S06c / S10 / S15 | `run_id` outstanding; S06c "plan written"; S10 redaction open; S15 no ledger | all four record their ruling and this receipt |

## Adversarial review — disposition (wave 1: `crates/actions`, wave 2: `crates/store`+`auth`)

⛔ **This lane is NOT done.** The review returned findings after the gate above was green; they are recorded here
with what was verified, what was fixed, and what is still owed. A finding is listed as *unverified* until someone
read the code — an agent's report is a hypothesis, not a defect.

| # | severity | finding | status |
|---|---|---|---|
| 1 | critical | `approvals.decide` is agent-reachable and nothing checks requester ≠ approver, so an agent can decide (or self-approve) the gate that exists to check it | **ACCEPTED by owner ruling 2026-11-09 — no change.** "The agent platform will have full execution ability through the MCP task construct and will decide on its own rules when to ask a human; the conveyor might not know whether an approval is human or agent, and that is fine." S06b's proof stands |
| 2 | high | `projects.close` is agent-reachable, `effect = destructive`, `confirm = none` | **ACCEPTED by owner ruling 2026-11-09 — no change.** Same reasoning: the agent holds full execution ability; the human gate is the agent platform's own decision. ⚠️ Recorded, not silently dropped |
| 3 | high | `documents.update` merges caller `meta` over the row's, so `content_type`/`size_bytes`/`sha256` can be rewritten — an accepted PDF re-labelled `text/html` is stored XSS at the app origin | **FIXED in code** (`documents/actions.rs` now refuses those three keys in the merge). ⚠️ **Regression test OUTSTANDING** — needs the upload harness in `documents_roundtrip` |
| 4 | high | `project_requests.promote` needs only `requests:write` (member) yet creates a project/board, bypassing admin-only `projects:write` | **UNVERIFIED** — next session reads `governance/requests.rs` before acting |
| 5 | medium | `documents/secrets.delete` + `repositories.delete` take `If-Match`, then discard it (`let _ = expected;`) — the delete is unversioned, so a stale ETag destroys a concurrent change instead of 412 | **VERIFIED, NOT FIXED** — needs a store `delete … WHERE version` |
| 6 | medium | `webhooks.test` is `effect = "read"` but POSTs outbound, and `validate_url` accepts loopback/link-local/RFC1918 — blind SSRF from inside the mesh | **VERIFIED, NOT FIXED** — needs the effect changed *and* a resolved-address policy |
| 7–12 | medium/low | unbounded `required_decisions`; process-global circuit breaker not keyed by tenant; upstream error text echoed in `detail`; `completions.create` declared `read` though it is a paid POST; `secrets.rotate` `write` vs `destructive`; `Mutex::lock().expect` panic-on-poison | **UNVERIFIED** — carried into the next session of this lane |

**Wave 2 — `crates/store` + `crates/auth`.** Its strong result first: **cross-tenant leaks: clean** (every
`&PgPool` function opens `begin_scoped`; every table except `tenant`/`app_user` is ENABLE+FORCE RLS; no query runs
on `&self.pool`), and **panics: clean** outside `#[cfg(test)]`. Five findings, none fixed:

| # | severity | finding | status |
|---|---|---|---|
| 13 | medium | `idempotency::begin` inserts `ON CONFLICT DO NOTHING`, but `lookup` treats an **expired** row as `Miss` → retrying a key after its 24 h expiry but before the sweep returns a permanent 409 and can never execute | **UNVERIFIED** — the fix is an `ON CONFLICT … DO UPDATE … WHERE expires_at <= now()` |
| 14 | medium | the decision UPDATE is guarded only by `status='pending'`; **`expires_at` is never checked**, so a gate past its deadline can still be decided before `approval.expire` sweeps it | **UNVERIFIED** — add `AND (expires_at IS NULL OR expires_at > now())` to the guarded update. This one is governance-significant: it decides whether an expired gate is dead |
| 15 | low | `DeliveryTarget`/`NewWebhook` derive `Debug` while holding the plaintext signing secret; `WebhookRecord` deliberately omits it, so the invariant is known | **UNVERIFIED** — latent log leak, no call site logs it today |
| 16 | low | `JwksCache` keys by `kid` only and ignores `url`, contradicting its own "one instance per issuer" doc; two realms sharing a `kid` would cross-reject | **UNVERIFIED** — latent while `from_config` wires one issuer |
| 17 | cosmetic | `grants-runtime.sql` revokes UPDATE/DELETE from the append-only tables except `approval_decision` and `revision` (UPDATE only — `revision` needs DELETE for the pruner) | **UNVERIFIED** — no reachable failure today; the grant is the boundary, not the doc |

## Open — the lane's remaining work, in order

1. **#3's regression test** (the one code fix in this batch that has no proof). Create a document, call
   `documents.update` with `meta: {"content_type": "text/html"}`, assert the downloaded `content-type` is
   unchanged and the update returns either 200-without-change or 422. Prove by reversal.
2. **#4–#17: verify, then fix or write the reason.** Fifteen findings, of which two are already fixed/accepted.
   Verify at the source before acting — an agent's report is a hypothesis (three of wave 1's were verified as
   stated, one suggested fix (`proposal_required`) was measured not to work).
3. **The two rulings are recorded here but NOT in the `00-INDEX` decision register.** They belong there too:
   "an agent principal may decide its own approval gate" and "the agent holds full execution ability; the human
   gate is the agent platform's decision" (owner, 2026-11-09). Until that row exists, this file is the only copy.
4. **The lease was released at 06:38Z with the review still open.** A session continuing this lane re-takes it by
   message; it must not edit `procedure/**`, `store/src/{run,procedure*}.rs`, `worker.rs` or `migrations/0021`.

## What this lane actually proved, stated plainly

The built tree's *tests and gates* now say what they mean: S10's three unproven rules have fail-closed proofs,
the S09 path defect is fixed with a guard over every service, and five slices' gates can be run for the first
time. The tree's *code* is **not** hardened: 17 review findings exist, one is fixed, two are accepted by ruling,
and fourteen are unverified. Do not read the green gate as a hardened tree.

---

# Session 2 — 2026-11-09, `improve-01` / `close-review-findings`

**Tree:** `$HOME/Movies/conveyor` (no `.git`) · **lease:** `00-INDEX.md` §Tree lease, opened 08:46, released
with the gate at the end of this section · **peer:** `s17-benchmark-core` writes new `crates/domain/src/benchmark/**`
files under a disjoint split; no file below is theirs, and the workspace gate was timed around their steps.

## Disposition — all 17 findings, measured

| # | disposition | proof |
|---|---|---|
| 1 | **NO-CHANGE** — owner ruling: an agent may decide its own gate | decision register, `00-INDEX.md` |
| 2 | **NO-CHANGE** — owner ruling: the agent holds full execution ability; `projects.close` stays reachable | decision register |
| 3 | **FIXED + TEST** | `crates/api/tests/documents_roundtrip.rs::a_caller_cannot_rewrite_the_stored_objects_facts`; reversal (skip list disabled) failed naming `text/html` |
| 4 | **NO-CHANGE** — owner ruling: roles are granted per tenant by the tenant-admin; `requests:write` is defined as "create, decide and promote" (`actions/src/spec.rs:49`) | decision register; production provisioning of the first member is S14's |
| 5 | **FIXED + TEST** | `delete_secret_tx` / `delete_repository_tx` now take the version (SELECT … FOR UPDATE, then delete); stale ETag → 412 and the row survives. Reversal (guards disabled) returned 204 for both |
| 6 | **EFFECT FIXED; ADDRESS POLICY DEFERRED** (owner ruling) | `webhooks.test` `effect = write`; unit test `a_test_delivery_is_declared_a_write_not_a_read`; reversal (read) failed. The SSRF exposure is named in the action's doc comment and below |
| 7 | **FIXED + TEST** | `approvals.request` clamps `required_decisions` to `1..=255`; test `a_gate_cannot_require_more_decisions_than_the_bound` (256 → 422, 255 → 200); reversal (bound removed) created a 256-decision gate |
| 8 | **FIXED + TEST** | breaker keyed by `(tenant, action)`; test `one_tenants_open_breaker_does_not_refuse_another_tenant`; reversal (fixed key) failed |
| 9 | **FIXED + TEST** | upstream 4xx body goes to `tracing` only; test in `external_actions.rs` asserts the body is absent from `detail`; reversal (re-echoed) failed with `{"error":"refused"}` |
| 10 | **FIXED** | `completions.create` `effect = write`; unit pin updated. Lock regenerated (see corrections) |
| 11 | **NO-CHANGE** — owner ruling: `secrets.rotate` stays `write`; redaction would break execute because the proposal stores AND replays its input (`api/src/copilot/proposal.rs:188-190`). Proposal-input custody is the real design item | decision register |
| 12 | **FIXED (no test possible)** | `PoisonError::into_inner` at the breaker (3) and matrix cache (6) sites; poisoning has no honest test — recorded, not faked |
| 13 | **FIXED + TEST** | `idempotency::begin` reclaims an expired row (`DO UPDATE … WHERE expires_at <= now()`); test extended in `store/tests/audit_idempotency.rs`; reversal (`DO NOTHING`) failed |
| 14 | **NO-CHANGE + TTL REMOVED** (owner ruling: approvals never expire) | copilot proposals now carry `expires_at = None`; `PROPOSAL_TTL_HOURS` deleted; `copilot_proposal.rs` asserts `expires_at IS NULL`. The opt-in sweep (S11 procedure gates, caller-supplied expiry) is untouched |
| 15 | **FIXED + TEST** | manual redacted `Debug` for `NewWebhook`/`DeliveryTarget`; unit test; reversal (secret printed) failed |
| 16 | **FIXED + TEST** | `JwksCache` keyed by `(url, kid)` and refresh retains other realms; two tests; reversals failed (cross-realm key, and a wiped map after refresh) |
| 17 | **FIXED (no runtime test exists)** | `scripts/grants-runtime.sql` revokes UPDATE+DELETE on `approval_decision` and UPDATE on `trace_link` |

## Corrections this session made to its own plan — recorded, not hidden

1. **#17's plan named `revision` for a revoke; that was wrong.** `store/src/model.rs:1248` executes `UPDATE
   revision SET status = 'superseded'` when a model publishes, and `prune.rs:160` / `model.rs:385` DELETE it.
   Both grants are load-bearing, so `revision` is deliberately NOT revoked and the file says why.
2. **#7's refusal is 422, not 400.** The handler's `invalid()` maps to `action-invalid-input` (422); the test's
   first assertion said 400 and was corrected against the measured response.
3. **#16's first test could not fail by reversal.** A zero TTL forced a refetch on every call, which hides the
   cross-realm bug (the dangerous window is *known kid, still fresh*). Rewritten as two tests — a long-TTL
   cross-realm test and a short-TTL refresh-retention test — and both reversals were re-run.
4. **`effect` is part of the served OpenAPI.** The two effect corrections drifted exactly
   `POST /api/actions/{completions.create,webhooks.test}/invocations`; the lock was regenerated with
   `--write-lock`, and `--check-lock` exits 0 at **210 operations**.

## What was run (all measured this session)

| # | command | result |
|---|---|---|
| 1 | `cargo test -p conveyor-api --test documents_roundtrip` | 2 passed |
| 2 | reversal: skip list disabled | failed — `text/html` returned |
| 3 | `cargo test -p conveyor-store --test audit_idempotency` | 5 passed |
| 4 | reversal: `DO NOTHING` restored | failed at the reclaim assertion |
| 5 | `cargo test -p conveyor-api --test secrets_never_revealed --test repositories_crud` | 2 + 1 passed |
| 6 | reversal: version comparison disabled | both returned 204 instead of 412 |
| 7 | `cargo test -p conveyor-actions --lib` | 37 passed (incl. breaker ×3, webhooks effect, inference effect) |
| 8 | reversals: effect→read, breaker→fixed key, body re-echoed | all three failed by name |
| 9 | `cargo test -p conveyor-api --test approval_dual` | 4 passed |
| 10 | reversal: bound removed | 256 accepted (200) |
| 11 | `cargo test -p conveyor-store --lib` | 36 passed (incl. the redacted-`Debug` test) |
| 12 | reversal: `Debug` printed the secret | failed |
| 13 | `cargo test -p conveyor-auth` | 29 + 7 + 0 passed (incl. both realm tests) |
| 14 | reversals: kid-only key, whole-map replace | both tests failed |
| 15 | `cargo test -p conveyor-actions --test copilot_coverage --test registry --test parity_http_mcp --test idempotency --test external_actions` | 3 / 1 / 4 / 6 / 1 passed |
| 16 | `cargo test -p conveyor-api --test {documents_roundtrip,mcp_agent,machine_login,approval_dual,copilot_rounds,copilot_crash_replay,externals_declared,repositories_crud,kata_role,secrets_never_revealed,copilot_proposal}` | all passed (4/1/1/1/2/3/1/6/2/1/2) |
| 17 | `cargo test -p conveyor-store --test s07_schema --test webhook_seal --test audit_idempotency` | 5 + 3 + 3 passed |
| 18 | `cargo run -q -p conveyor-server -- --print-openapi --write-lock` then `--check-lock` | exit 0, **210 ops** |

## Still open — do not read the green gate as a closed lane

1. **Tenancy requirement (owner, 2026-11-09):** one Keycloak identity must hold different roles in different
   tenants. ⛔ **Blocked by the code and the contract:** `auth/src/authn.rs:96-101` refuses any principal with
   more than one membership (401), and CONTRACT §9 (`01-CONTRACT.md:197`) mandates exactly one. The decision
   register carries an OPEN row; the tenant-selection mechanism is undecided and nothing was built.
2. **Webhook SSRF is deferred, not fixed.** `validate_url` still checks only the scheme, so an admin
   (`boards:write`) can point `webhooks.test` at `169.254.169.254` or a private host. Owner ruling: permissive
   for now, tighten later.
3. **`secrets.rotate` is still declared `write`** although it destroys the old credential — blocked on
   proposal-input custody (#11).
4. **#12 and #17 have no failing-test proof** (no honest poisoning test exists; the grants file has no runtime
   test). They are fixes with a stated measurement, not proofs.
5. **The workspace gate result is recorded separately below when it runs** — it measures a tree that also
   contains `s17-benchmark-core`'s new module, which this lane does not own.

## Workspace gate — measured 09:15–09:22, tree shared with `s17-benchmark-core`

| step | result |
|---|---|
| `cargo test --workspace --no-fail-fast` (first run) | **RED on one target that is not this lane's**: `crates/actions/tests/benchmark_parity.rs::every_banked_log_reproduces_its_banked_verdict` — the peer's fixture reader hit a macOS `._*.log` entry; they fixed it and reported 555/555. **My own** `jwks::tests::a_shared_kid_never_crosses_realms` also failed in that run: it had a load-dependent `min_refresh`, passed alone and failed under a parallel suite. Rewritten (`min_refresh = 0` for the cross-realm test; a 2 s margin for the retention test) and re-run green |
| `cargo test --workspace --no-fail-fast` (final run) | **exit 0** — every target `ok`, 0 failed |
| `cargo clippy -p conveyor-api -p conveyor-actions -p conveyor-store -p conveyor-domain -p conveyor-auth --all-targets -- -D warnings` | **exit 0** |
| `cargo fmt -p api -p actions -p store -p auth -- --check` | **exit 0** |
| `cargo run -q -p conveyor-server -- --print-openapi --check-lock` | **exit 0**, **210 operations** |
| migrations ledger (`cargo test -p conveyor-store --test s07_schema`) | **ok** — includes the peer's `0025_s17_benchmark` |

⚠️ The workspace number is not comparable to the peer's earlier 569/0: the tree gained S17's benchmark module and
this lane's eight new tests. **The gate is green, but the tree is not "hardened"** — see "Still open" above.

## Session 3 — the tenancy change and the two deferrals

**Date:** 2026-11-09 11:58– · **Session:** `improve-01` session 3 (`tenancy-change-deferred-findings`) · **Tree:** same, no `.git`
**Lease:** opened 11:58, released with the gate below. S17 held the tree until it reported GATE_GREEN and released
row 199; this session touched no S17 file and neither migration ledger.

### Owner decisions taken in this session
1. **Scope** = the tenancy change + the two findings deferred by the earlier rulings (#6 webhook address policy,
   #11 `secrets.rotate` custody). The four prose-cap overruns stay untouched.
2. **No silent tenant choice:** a person with more than one membership and no `X-Active-Tenant` is `400
   active_tenant_required`.
3. **S19 CCR 5 stays open** — deferred to the S19 build session, which must return for a decision before the
   `approval.eval_id` ALTER ships. Relayed to that session by message.

### A — tenancy (one identity, many memberships, one active tenant)

| file | change |
|---|---|
| `crates/auth/src/authn.rs` | `X-Active-Tenant` parsed once (a single non-empty value), matched **only** against the caller's own rows (tenant id, then slug); a licence bearer's tenant stays authoritative and a contradicting header is refused; the session lists every active membership |
| `crates/auth/src/error.rs`, `crates/api/src/error.rs` | `active_tenant_required` / `active_tenant_not_permitted` / `active_tenant_invalid` (400/403/400) |
| `crates/auth/src/principal.rs` | `Session::active()` finds the selected row instead of `[0]` |
| `crates/api/tests/active_tenant.rs` | **new** — owner-in-A / member-in-B / refused-in-C, by id and by slug, empty header, single-membership control, and an admin-scope action allowed in A and refused in B |
| `docs/plans/01-CONTRACT.md` §9/§10, `docs/SPEC.md` §5.4/§4.4 | the rule, and **D27** in the decision register |

No migration: `membership` was already unique per `(tenant_id, user_id)`, and `resolve_identity` already read every row.

### B — webhook address policy (#6, previously deferred)
- `validate_url` refuses a literal loopback / link-local (metadata) / multicast / unspecified / broadcast address
  at registration; `deliver_once` additionally resolves **at connect time** through `GuardedResolver` and refuses
  any resolved address in those classes (DNS rebinding), with redirects disabled.
- Private targets (RFC1918, `100.64/10`, ULA) stay allowed: every legitimate webhook target on this estate is
  private. Loopback is allowed only when `CONVEYOR_WEBHOOK_ALLOW_LOOPBACK=1`, which the test harness sets for its
  loopback recorders; a deployed process never sets it.
- Tests: the address-class table, an integration `webhooks.create` refusal naming `url`/`link-local`, and a
  delivery to a `localhost` name proved refused **before the socket opens** (a listener on that port records nothing).

### C — `secrets.rotate` custody (#11, previously deferred)
- `secrets.rotate` is now `effect = "destructive"`, so an agent's call becomes a proposal.
- `proposal::create_tx` seals the exact input with the tenant's KEK (`store::secret::sealer`, the `secrets.*` key)
  into `meta.sealed_input`; `meta.input`/`meta.diff` carry a redacted projection where every `x-secret` property is
  `<redacted>`. `proposal::execute` replays **only** the sealed input; a secret-bearing proposal without a seal is
  `Refused`, never run from the projection. No migration: the sealed blob is another `meta` key and no API returns it.
- `crates/actions/tests/copilot_coverage.rs`'s blanket ban became a positive proof over every proposal-triggering
  action carrying a secret (non-vacuous by assertion).
- `crates/api/tests/proposal_custody.rs` (**new**): a real turn proposes `secrets.rotate` with a canary, the SQL row
  contains the canary nowhere, then approval + `execute` rotates the secret (version 1 → 2) through the sealed input.

### Reversal proofs (each was made to fail, then restored)
| fix | reversal | observed |
|---|---|---|
| A | header ignored, first membership used | both tenancy tests failed — `/api/me` served tenant A for a `dev-b` request, and the "member in B" call created a duplicate in A (409) instead of 403 |
| B | link-local refusal removed | the unit table failed on `http://169.254.169.254/… must be refused`, and `webhooks.create` returned **201** where the test requires 422 |
| C | redaction removed (`input` stored verbatim) | the custody test failed, naming the canary in both `meta.input` and the diff |

### Gate (the exact final tree)
| # | command | result |
|---|---|---|
| 1 | `rbuild --gate` (`fmt --all -- --check` && `clippy` && `test --locked`) | **exit 0** — **119 test binaries, 647 passed / 0 failed** |
| 2 | `rbuild -- clippy --workspace --all-targets -- -D warnings` | **exit 0** |
| 3 | `rbuild -- run -q -p conveyor-server -- --print-openapi --check-lock` | **exit 0**, **210 operations**; `secrets.rotate` hash `b4bd39ac…` → `ed90894591f4bab437c093ddf8fbb05aea37f1837af4d985d9e22e59e1483e44` (effect `write` → `destructive`) |
| 4 | the new and affected binaries inside run 1 | `active_tenant` **2/2**, `proposal_custody` **1/1**, `copilot_coverage` **3/3**, `webhook_shape` **2/2**, `conveyor-actions` lib **39/39** |

⚠️ The gate ran 12:24:35–12:31 local on the tree containing this lane's final code. No `.rs`, `.sql` or `.toml`
file under `crates/`, `apps/` or `migrations/` changed between the gate's start and the clippy/lock checks
(`find -newermt`). The S19 lane resumed after the window closed; its later files are **not** part of these numbers.
The workspace target was the single-lock rbuild tree; by the time the clippy/lock checks ran, `rbuild` had gained
independent build slots, so those two ran in slot 1 while slot 0 was in use.

### What this session did NOT do
- **S19 CCR 5** — open by the owner's answer; the S19 session was told not to ship the ALTER without a decision.
- **`00-INDEX:856`** still calls `0025` "not yet applied"; that row is S17's and was handed over, not edited.
- **The `me` ACTION** (`crates/actions/src/handlers.rs`) still returns the active membership only; `/api/me` (the
  session path) lists every membership. An agent acts in one tenant per call, so this is a recorded deviation.
- **Prose caps** (S07/S09/S11/S08) — outside the chosen scope.

### Re-verification at 17:2x (a later session) — two findings were nearly re-fixed

A session that had not re-read this file's disposition table re-measured #4 and #14 as **code-present**
and implemented both. They were reverted before anything landed, when this table was read:

- **#4** is an owner ruling (`00-INDEX` decision register, approved): `requests:write` is defined as
  "create, decide and promote", so member-scoped `project_requests.promote` is the decision, not a bug.
- **#14** is a no-change ruling: approvals never expire, and the copilot's TTL was removed
  (`api/src/copilot/turns.rs:158` is `expires_at: None`). The sweep stays the only expiry actor, which is
  what `apps/server/tests/approval_expire.rs` asserts.

Both patches worked (F4: member 403 / admin 200, reversal created a project; S14: expired decide 409,
reversal approved it). ⛔ **A finding that is still in the code is not an open finding — this table is the
disposition record.** The only durable cost of the detour is the lesson; no code, migration or count changed.
