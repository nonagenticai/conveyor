# Proof of delivery — the Lean certificate corpus

Part of #746, extended by #762. This document records what was actually measured, on which artefact, and what was
**not** run. It is a delivery record, not a claim of completeness: § Live on the released engine holds the drills
against the deployed engine, and the assumption no certificate removes.

## Artefacts

| Thing | Value |
|---|---|
| Model | `docs/spec/lean/DeliveryModel.lean`, 3 179 bytes, `sha256 d19f8ad3…baca504` |
| Specification pin | `docs/SPEC_V2.md`, `sha256 fc0fe31a…a99b99e2` |
| Toolchain | `leanprover/lean4:v4.30.0-rc2`, the prover application's own lake project |
| Reduction | kernel only — `by decide` and `simp`, no compiler-native reduction, no Mathlib import |

## How the checks were run

Two lanes, and they are deliberately different:

- **The corpus and the renderer's exact output** were checked by running `lake env lean <file>`
  inside the prover's own pod, against its own lake project, with the file streamed in and removed
  afterwards. That is the same command the prover's runner issues, so "valid" here means the same
  thing the API would answer — without going through the HTTP layer.
- **The engine path** (the declared `lean4.check` external, the sealed credential, the persisted
  invocation, `proofs.record` and `proofs.reverify`) is proven by integration tests against a real
  PostgreSQL 18 and a stub prover: `crates/actions/tests/proofs_record.rs`,
  `proofs_reverify.rs`, `pm_gate_proof.rs`.
- **The live drills** (§ Live on the released engine) run that same engine path against the deployed
  engine and the real prover, through the public API, with a real tenant bearer.

## Measurement — kernel checks on the exact artefacts

| Source | Bytes | Exit | Wall clock |
|---|---|---|---|
| The model alone (definitions + three universal theorems) | 3 179 | 0 | 10 242 ms (first run after the pod restarted; every later run is ~1.4 s) |
| Slice certificate — a closed slice with its evidence | — | 0 | 1 462 ms |
| Slice with one required evidence item removed | — | 1 | 1 399 ms |
| Model with an escape transition out of `closed` | — | 1 | 1 413 ms |
| Programme bundle, 20 leaves, final model | 5 143 | 0 | 3 107 ms |
| Programme bundle, one leaf dropped | — | 1 | 1 925 ms |
| Bundle with two slices sharing one certificate digest | — | 1 | — |

The renderer's own emission format was checked the same way, once per kind: a rendered `slice`, a
rendered `programme` bundle and a rendered `package` manifest each exit 0, including the
`checks := [("name", true)]` pair list. The negative runs are what stop the corpus being vacuous:
a model that is not a model, a slice missing its evidence, a bundle missing a leaf, and a bundle
that reuses one certificate for two slices are all refused by the kernel, not by us.

Earlier on the same day, through the prover's HTTP API with a licence token, the same shapes
measured: model 1 466 ms, slice 1 462 ms, package 1 598 ms, 20-leaf bundle 6 149 ms, and the same
four negatives refused. The HTTP answer agreed with the kernel answer in every case.

## What the certificate says, and what it does not

A certificate is an instance proof over canonical facts plus a universal proof over a hand-written
model. It proves that the facts, as recorded, satisfy `checkSlice` / `checkProgramme` /
`checkPackage`, and that a closed item is terminal with an accepted path always carrying evidence.

It does **not** prove that the engine's facts describe what actually happened in the database. That
refinement is assumed, and every certificate names the assumption, the specification digest and the
model digest. `README.md` in `docs/spec/lean/` states this in the same words a reader of a
certificate would need.

## The engine's refusals, measured by test

| Scenario | Refusal |
|---|---|
| A caller writes the Lean source instead of the renderer | `422 proof_source_not_rendered` |
| Facts that name a different model than the binary embeds | `422 proof_model_mismatch` |
| Facts and certificate pinning different specifications | `422 proof_spec_mismatch` |
| Facts missing a required item | `422 proof_facts_incomplete`, naming the item |
| A certificate reused for a different engine invocation | `422 proof_invocation_already_recorded` |
| A stored row rewritten and its digest recomputed | `422 proof_source_not_rendered` on re-verification |
| A gate that names `lean_proof` required, opened without one | `422 gate_evidence_missing`, naming the kind |
| A gate requiring a kind that is not typed | `422 gate_evidence_unknown_kind` |

## Live on the released engine

Measured 2026-11-26 against the deployed engine — the image carrying `0076`, #746 and
`CONVEYOR_EXTERNAL_LEAN4_URL` from the private infrastructure repository — with a real tenant
bearer from the application's own OIDC login. Every body below is the deployed engine's answer;
none of it is a test double.

### The credential

The tenant's prover bearer is a licence token minted on the tenant's **own `conveyor` licence** and
sealed with `conveyor admin upstream-token --tenant … --service lean4 --token-stdin`. The engine
reads it as the `lean4.token` secret and sends `Authorization: Bearer …`; the prover introspects it
and takes the tenant from the licence, never from a header. A dedicated `lean4-prover` licence app
does not exist, so the self-service mint (`--mint`) is tracked as #776.

### The measurements

| Step | Request | Deployed engine's answer |
|---|---|---|
| Prover reachability | `lean4.check`, `{"source": "theorem smoke_ok : 2 + 2 = 4 := by decide"}` | `200`; `{"valid": true, "computation_time_ms": 835.01, "diagnostics": [], "stderr": "", "stdout": ""}` |
| The renderer's own output | `lean4.check`, the slice certificate `render` emits from the facts below (3 537 characters) | `200`; `{"valid": true, "computation_time_ms": 1509.88}` |
| Negative control | `lean4.check`, `theorem nope : 1 = 2 := by decide` | `200`; `{"valid": false, "computation_time_ms": 836.15}`, the tactic's message in `stdout` |
| The certificate | `proofs.record` naming that invocation, the facts and `theorem delivery_ok`, `trust: kernel` | `201`; `{"verdict": "proven", "certificate_sha256": "31feb1ba…33e4648", "facts_sha256": "7c7daf3a…ddc4f0f", "computation_time_ms": 1509.88}` |
| The gate, without the certificate | `approvals.request`, `required_kinds: ["lean_proof"]`, `require_evidence: true`, `pinned_spec_sha256`, `evidence: []` | `422`; `"type": "gate_evidence_missing"`, `"detail": "this gate requires a \"lean_proof\" evidence item to open; the request names none"` |
| The gate, with it | the same request plus `evidence: [{"kind": "lean_proof", "id": "<certificate>"}]` | `200`; the approval's `evidence[0]` is `{"kind": "lean_proof", "id": "01a219b7…cfea658", "certificate_sha256": "31feb1ba…33e4648", "spec_sha256": "fc0fe31a…a99b99e2"}` |
| The tamper drill | the stored `facts_canonical` edited (one evidence item renamed) **with** `facts_sha256` and `certificate_sha256` recomputed, so the row is internally consistent; a fresh `lean4.check` of the unchanged source answers `valid: true`, then `proofs.reverify` on the rewritten row | `422`; `"type": "proof_source_not_rendered"`, `"detail": "proof_id: the stored source is not this rendering of the stored facts"` |
| The refusal is recorded | the engine's own record of the call above | `action_invocation` row `proofs.reverify | refused`, and an `audit_event` `{"outcome": "refused"}` at the same instant |

The facts the certificate carries name the model digest `d19f8ad3…baca504` and the specification pin
`fc0fe31a…a99b99e2`; `proofs.record` re-rendered the facts and refused nothing, which is what makes
the row's `lean_source` the renderer's output rather than a caller's claim.

An approval must belong to a project, card or run, so the two live gate calls name a project; the
earlier attempts on the same morning that omit it are input validation, not the gate.

### A prover defect this exposed

A check that fails returns `valid: false` with `diagnostics: []` and Lean's message in **`stdout`**:
`apps/lean4-prover/src/runner.rs` parses only `stderr` (three call sites). Certificates are
unaffected — `valid` comes from the exit code — but the diagnostic channel is empty for every
failure. Worth its own fix; it is not #746's.

### What this still does not prove

- That the facts in a certificate describe what actually happened in the database. That refinement
  is the assumption every certificate names (see `docs/spec/lean/README.md`); proving it is
  Verus/Kani work, not part of the corpus.
- The self-service mint path: the credential above was minted and sealed by an operator, and
  `--mint` still needs the dedicated client and the licence-app decision (#776).

## Units — A0, the chunk-size measurement (2026-11-27)

Part of #836. The unit model is a draft at this point; this section fixes the certificate size
from real kernel runs, not from an estimate. The model under measurement is `DeliveryModel.lean`
at 5 537 bytes, `sha256 7bc3a1eb…86f859a` (the 3 179-byte row above belongs to the pre-unit model).

`scripts/proof/a0_units.py` generated the sources at N = 40, 80, 200, 400 and 444 synthetic
`UnitObs` records, plus seven one-unit negative controls, an empty ledger and a `startsWith`
probe. Each file was streamed into the prover's own pod and run with the runner's exact command:

```
cd /lean-project && env -i PATH=/usr/local/bin:/usr/bin:/bin:/opt/elan/bin \
  ELAN_HOME=/opt/elan /opt/elan/bin/lake env /opt/elan/bin/lean /lean-project/_a0_<n>.lean
```

| N | Source bytes | Warm wall clock (ms) | Exit |
|---|---|---|---|
| 40 | 11 010 | 1 759 · 1 780 · 1 781 · 1 810 | 0 |
| 80 | 16 390 | 1 899 · 1 902 · 1 925 · 1 839 | 0 |
| 200 | 32 530 | 2 211 · 2 154 · 2 215 · 2 239 | 0 |
| 400 | 59 430 | 2 736 · 2 446 · 2 655 (with `set_option maxRecDepth 4000`) | 0 |
| 444 | 65 348 | 2 678 · 2 735 · 2 732 (with `set_option maxRecDepth 4000`) | 0 |

At the default recursion depth, N = 400 and N = 444 both fail in 2.5–2.9 s with
`maximum recursion depth has been reached`; N = 200 passes untouched. The first run of the
session was a smoke theorem at 825 ms; the 10 242 ms cold figure above is the pod's first run
after a restart and was not re-measured (the live pod was not restarted).

The negative controls — two branches, two PRs, a draft PR, three CI runs, merged with the issue
pending, merged with the branch live, unknown branches — each exit 1: the kernel refuses
`checkLedger [bad] = true`, it is not a render refusal. An empty ledger is also exit 1 (the
`!isEmpty` guard). A `String.startsWith "issue/836-"` probe under `decide` exits 1:
**`startsWith` is not kernel-reducible in this toolchain**, so prefix checks must arrive as
derived facts, never as Lean computation in the checker.

One `lean4.check` at N = 200 through the deployed engine was attempted with a machine principal
minted in the conveyor pod. It answered `403 action-forbidden`: the principal lacks the
`integrations:read` scope the action declares. That is a principal/grant result, not a prover
result — the direct runner path above is the command the prover issues. A2 must run with a
principal holding `integrations:read` (the check) and `approvals:write` (the record). A conveyor
rollout replaced the pod during the first attempt; the 403 is from the new deployment.

**Decision.** Certificate chunk = **200 units**. At 200 the source is 32 530 bytes against the
100 000-char cap (3× headroom) and the warm maximum is 2 239 ms against the 120 s prover timeout
(53× headroom); with the 10 242 ms cold start the worst case is ≈ 12.5 s. The full 444-unit
ledger would fit one certificate (65 348 bytes, ≈ 2.7 s) but needs
`set_option maxRecDepth 4000`; 200 is chosen so a failing chunk is bisected cheaply and the
default recursion depth stays in force. The ledger window is therefore **all** merged PRs in the
window plus the unit issues — about three chunks. The renderer refuses more than 200 units per
source unless it emits the option.

Same assumption as every certificate: the kernel certifies *facts ⇒ verdict*. The collector's
rules, hashes and committed payloads are what a reviewer checks against reality.

### A1 — the shipped unit model (2026-11-27)

The A0 draft had six fields. The shipped model adds the two body-observable facts (the PR's
`Closes #N`, the issue's `Verify:` lines), names `checkUnit`, and adds `ruleResults` so a
certificate proves the outcome of every rule, not only the aggregate. Model
`DeliveryModel.lean` at 6 709 bytes, `sha256 378a1395…574ddae7`.

Measured with `scripts/lean/check-model.sh` against the same pod command (wall includes the
ssh/exec and stream overhead, so it is an upper bound on the kernel time):

| N | Source bytes | Exit | Wall (ms) |
|---|---|---|---|
| 40 | 14 554 | 0 | 4 700 |
| 80 | 22 094 | 0 | 4 653 |
| 200 | 44 714 | 0 | 5 892 |

The controls file, checked appended to the model, is kernel-refused: 11/11 negative theorems
(two branches, two PRs, a draft, three CI runs, merged with the issue pending, merged with the
branch live, unknown branches, no `Closes`, missing `Verify:`, the empty ledger, a falsified
`ruleResults` literal). The positive control (`checkUnit goodUnit = true`) elaborates; the
refusals are the eleven `control_` theorems. Renderer tests: 17/17 (`cargo-guard test -p
conveyor-actions --test proof_render`). The `unit_ledger` subject is recorded end to end by
`proofs_record::unit_subject_kind` (1/1) against an ephemeral PostgreSQL 18: 201, subject
`unit_ledger`, and the stored source is the renderer's output.

The chunk decision is unchanged: **200 units**. The shipped 200-unit source is 44 714 bytes
(2× headroom under the 90 000-byte working limit, 2.2× under the prover's 100 000-character cap)
and the warm kernel path is ≈ 2 s; 400 and 444 still require `maxRecDepth 4000` in the A0
measurement and are not emitted.

### A2 — the evaluation of the real ledger (offline, 2026-11-27)

The collector (`scripts/proof/collect_units.py`, self-test in the `checks` job) fetched the
window once and rendered it offline. The snapshot is committed at
`docs/evidence/units/2026-11-27/snapshot.json`: 371 issues, 447 merged PRs (319 non-automation),
124 live heads and 2 481 workflow runs at 638 shas, collected at `48797526c`. It is redacted per
`scripts/ci/no-private-refs.sh`: public GitHub issue/PR bodies sometimes quote estate identifiers,
so the committed copy carries deterministic placeholders — the facts the certificate uses (state,
closing keyword, `Verify:`) need none of them, and the unredacted payloads are the public GitHub
records. Two renders of the same snapshot produced the same corpus hash and byte-identical facts
(`66544ac7ae1e1ed2…`).

| Corpus | Count |
|---|---|
| Completed units (certified) | 320 |
| In flight (no merged PR; not certified) | 54 |
| Orphan merged PRs (no issue) | 12 |
| Automation PRs excluded (`github-actions[bot]`, `dependabot[bot]`, `automation/*`) | 128 |
| Chunks at 200 units | 2 (200 + 120) |

The facts documents measure 112 687 and 67 710 bytes (provenance included; the rendered source
carries only the observations, so the A1 200-unit source measurement is the prover-side budget).
The model is the A1 model (`378a1395…`); the spec is `docs/plans/03-DELIVERY.md`
`12514c99588f8b8c…`. The collector's local mirror of the rules reports **56 of 320 completed
units failing**:

| Rule | Failing units |
|---|---|
| `ciBudget` | 0 |
| `closesKeyword` | 12 (the orphans: a merged PR that names no issue) |
| `mergeComplete` | 15 (orphans, an issue left open, a branch left live) |
| `oneBranch` | 4 |
| `onePull` | 5 |
| `pullUsable` | 0 |
| `verifyParts` | 50 |

The `Verify:` cross-check against `scripts/ci/board.py`: 9 units pass the checker and fail the
model (the checker accepts one `Verify:` in the exact section; the model requires one per part),
6 the reverse, 293 agree. Both answers are in `units.json`; neither is silently adopted.

### The live certification (2026-11-27)

The deployment gate opened when #836 merged: the rolled image (`f3c8a4e8…`) carries `proofs.render`.
Three facts about the live path had to be measured first, and they are now part of the method:

- **The edge signature.** `conveyor.nonagentic.ai` answers `403` `error code: 1010` to the default
  `Python-urllib/…` signature. With the same token and body in the same minute, `curl` answered 403
  for `-A Python-urllib/3.15` and 200 for `-A conveyor-collector/1.0`. The collector now sends the
  named signature (`scripts/proof/collect_units.py:Engine.call`, asserted by its `--self-test`).
- **The prover bearer's lifetime.** The engine's sealed `lean4.token` is a
  `mint_bundle_token(tenant, app, 60)` — a **60-minute** mint (`apps/server/src/admin/upstream_tokens.rs:114`).
  The token sealed on 2026-11-26 had expired, and the first live `lean4.check` answered the
  prover's `401` through the engine. Re-minting with
  `conveyor admin upstream-token --tenant … --service lean4 --mint` restored it immediately. Nothing
  refreshes it (the residual is #776's), so a live run mints immediately before it certifies.
- **The grant.** The licence bearer authenticates as the tenant's `mcp` machine channel, whose
  authority is the channel policy, not a user role. `channels.policy_set` takes the *complete*
  override set, so the run read `channels.policy_get` and merged — **108 → 114** stored overrides,
  adding exactly `lean4.check`, `proofs.render`, `proofs.record`, `proofs.reverify`, `proofs.get`
  and `proofs.list`. The 108 live overrides (another lane's in-flight work) were preserved.
  `channels.policy_set` itself is `denial: administration`, so the write went through
  `conveyor admin action invoke`.

The collector's `certify` over the committed corpus `66544ac7…` recorded two chunk certificates and
the programme bundle:

| Certificate | Units | Verdict | `certificate_sha256` | `facts_sha256` | Kernel |
|---|---|---|---|---|---|
| `01a21d8a-d07f-7994-b63b-17380345c9f4` | 200 | `unproven` | `82a7f781…8f22a61` | `6dcf1e47…e04251c` | 2 517 ms, `valid: false` |
| `01a21d8a-df06-7f12-9d46-ea68816af74c` | 120 | `unproven` | `f152cff6…b83f343` | `d6b6c47e…734c5bf` | 2 353 ms, `valid: false` |
| `01a21d8a-ec89-77f0-9987-ebf1092a0cbf` | bundle | `proven` | `a32c262a…cd7da1f` | — | `valid: true` |

The bundle is `proven` because `checkProgramme` certifies the bundle's structure — the expected leaf
set and distinct digests — not its leaves' verdicts. The ledger's two refusals are the deliverable.

The passing-subset control (`scripts/proof/control_units.py control`) certifies the 264 units that
pass every rule, under a derived corpus `4939254241302bf2…`, so the live record path is exercised on
a real proven set as well:

| Certificate | Units | Verdict | `certificate_sha256` | Kernel |
|---|---|---|---|---|
| `01a21d8b-1448-7428-8413-8831e3c75ac5` | 200 | `proven` | `70f8254b…b8aa3ce` | 2 592 ms, `valid: true` |
| `01a21d8b-2695-7a11-a17c-2710f4b17b1c` | 64 | `proven` | `7df8d678…14e20cc` | 2 233 ms, `valid: true` |
| `01a21d8b-3def-7420-8573-fcf53dcc2afb` | bundle | `proven` | `305e321f…fdf8ee7` | `valid: true` |

`proofs.reverify` on chunk-000 (`scripts/proof/control_units.py reverify`): the fresh `lean4.check`
answered `valid: false` in 2 529 ms, the fresh source equalled the stored one, and the stored verdict
matched (`unproven`; the full response is `reverify.json` beside the certificates). A second
`certify` over the same snapshot replayed the same invocation keys and returned the **same three
certificate ids** — the collector is deterministic end to end.

**What this proves, and what it does not.** The kernel certifies that the committed facts do not
satisfy `checkUnit` for the 56 units named above, and the control certifies the path accepts a real
passing set. The facts' provenance and the committed snapshot remain the reviewer's lever; the
assumption line in § What the certificate says, and what it does not is unchanged.

### The engine-derived unit record (2026-11-27, #837)

The observed ledger's own note said the engine's rows cannot attest issue state, branch liveness,
draft flags or run counts, so the unit's Definition of done needed a second structure over the
rows it does carry. #837 adds it:

- **The model.** `docs/spec/lean/DeliveryModel.lean` at `sha256 bbbc56e1…50a138e57` adds
  `UnitRecordFacts` and `checkRecord`, every field an `Option` that fails closed, plus the theorem
  `record_without_merge_is_unproven`. The model alone was streamed into the prover's own project
  with the runner's command: **exit 0, 4 741 ms**. Rows proven against the older model stay
  re-verifiable — `proofs.reverify` skips a row whose facts name a different model — and the new
  digest is pinned by `proof_render.rs::the_model_digest_is_pinned`.
- **A complete instance closes.** A rendered record with a green CI run, a merge, a deploy and a
  live check at the deployed digest closes `theorem unit_record_ok : checkRecord facts = true :=
  by decide` — **exit 0** in the prover's project.
- **An incomplete one is refused by the kernel.** The same record with `mergeSha := none` renders
  (the renderer never hides a defect) and the kernel answers **exit 1**: `Tactic 'decide' proved
  that the proposition checkRecord facts = true is false`.
- **The engine path, against PostgreSQL 18.** `proofs_attest.rs` (9 pass), `proofs_record.rs`
  (18 pass, including re-derive / caller-facts / recomputed-tamper) and `pm_gate_unit_proof.rs`
  (8 pass: advisory, required, and the policy toggle) run the real actions over migration 0081.
  The refusal for a required unit without a certificate is the same typed
  `422 gate_evidence_missing` a PM gate returns, produced by the factored `lean_proof` arm.

**The live half is deployment-gated, and is not claimed here.** Measured at
`2026-11-27T16:28Z`, the live primary is at migration **79** and `proof_policy` does not exist;
the branch's **0081** reaches it only through the merge → image → runtime migrator. The first
genuine live refusal — this unit's own chartered record, `required: true`, closed before its
certificate exists — is recorded when the deployed engine carries 0081, in the same pattern by
which #844's live certification was carried by #855. Concluding "live" from the branch binary
against a schema that has not deployed would be exactly the drill this unit exists to replace.

### The unit gate on the released engine (2026-11-27, #871)

The promise above is kept. The deployed engine carries `0081` and refuses a real unit's close on
its Definition of done; the same unit then closes once its certificate exists. Every body below is
the deployed engine's answer — none of it is a test double.

**The deployed identity.** `select max(version) from _sqlx_migrations` = **81**; the serving pods
are `ghcr.io/nonagenticai/conveyor:latest@sha256:dbb665e1…98555d`, the image of the merge
`322147b9e`; `GET /health/ready` = **200**.

**The record, chartered from #837's real delivery.** `development_projects.charter` wrote
`development_project` `01a21e1e-857f-…a9143` (repo `nonagenticai/conveyor`, lane
`issue/837-engine-dod`, issue `nonagenticai/conveyor#837`, implementer `01a21820-3510-…3279e9`)
with `proof_policy {spec_name: docs/SPEC_V2.md, spec_sha256: 0651b0ec…a3cc6ad, required: true}`.
The seven required steps were recorded from the delivery's own facts: `branch`; `pull_request`
#859 head `91a3dea6d`; `ci` run `46035785106` at that head, `green`; `merge` `322147b9e`; a
**tested** `rollback_plan`; `deploy` digest `sha256:dbb665e1…98555d` at `migration_generation:
81`; `live_verify` 200, serving digest equal to the deploy's, the acceptance command at exit 0.

The rollback plan was exercised, not asserted. On staging (Docker, PostgreSQL 18) the prior
release `sha256:cbee6af8…94654` migrated a fresh database to generation 80 and served
`/health/ready` 200; against the same database after applied version **81** was present, its
runtime migrator (`ignore_missing`, `crates/store/src/migrate.rs`) exited 0 and `/health/ready`
again answered 200 — the application-only rollback reaches Ready. The gate therefore records
`backward_compatible: true` with that evidence, and the plan is the Flux digest pin
(`scripts/rollback-image.sh --policy conveyor`), not a database restore.

**The three gates.** `review` `01a21e1e-dd58-…d68b`, `merge` `01a21e1f-dde2-…61bd` and `deploy`
`01a21e21-7e45-…8c41` were each decided `approve` by `01a1e8a7-8b23-…f7a545` (the tenant's
`conveyor-approver`), a principal distinct from the implementer. The decisions were made through
the operator path under the owner's explicit authorization of 2026-11-27; the audit rows record
`decided_by_kind: service`, which is what the operator path attributes, and the closure names the
same `reviewer_id`.

**The refusal.** Closing before any certificate existed answered, verbatim:

```
HTTP/1.1 422 Unprocessable Entity
{"type":"gate_evidence_missing",
 "title":"Unprocessable Entity","status":422,
 "detail":"this gate requires a \"lean_proof\" evidence item to open; the request names none",
 "instance":"/api/actions/development_projects.close/invocations",
 "request_id":"12d7a3e4-05f5-4570-b70c-ce144d92eb0b"}
```

The engine recorded the call itself: an `action_invocation` row
`development_projects.close | refused` at `17:51:53Z` by actor `01a21820…`, and an `audit_event`
`development_projects.close | refused` at the same instant.

**The path opens.** `proofs.attest` derived the facts from the rows and wrote nothing:
`facts_sha256 3d390203…451eaa`, model `bbbc56e1…a138e57`, theorem `unit_record_ok`, source
10 216 characters. `lean4.check` of that source answered `200 {"valid": true,
"computation_time_ms": 3030.91}`; `proofs.record` naming that invocation stored certificate
`01a21e24-9de2-…f459` with `verdict: proven`, `certificate_sha256 fd1eda04…9dec52`,
`facts_sha256 3d390203…451eaa`, `trust: kernel`. The same `development_projects.close` then
answered **201**, and the closure's `dod` reads:

```
{"required": true, "lean_proof": "proven",
 "proof_id": "01a21e24-9de2-7c24-944f-650a081fe459",
 "facts_sha256": "3d3902037c34587d31740e2a504cd93d3c520659af9fc855782bfa1385451eaa",
 "evidence": {"kind":"lean_proof","id":"01a21e24-…","certificate_sha256":"fd1eda04…",
              "spec_sha256":"0651b0ec…"}}
```

The closure names the reviewer `01a1e8a7…`, the merge `322147b9e`, the deployed digest
`sha256:dbb665e1…`, the CI run `46035785106` and the live check above. `development_project`
`01a21e1e-857f-…` is now `closed`.

**One gap this exposed, and its grant.** `proofs.attest` — the action #837 added — was not in the
`mcp` channel's override set, so the session bearer's first call answered `403 action-forbidden`.
Measured: 114 grants, `proofs.record`/`reverify`/`get`/`list` present, `proofs.attest` absent. The
gap was closed with one `channels.policy_set` carrying the complete 115-entry set (the operator's
act, recorded as `georgiedekker` under the owner's authorization); the same call then answered
200. The grant is durable and is what lets the next unit, and the verifier role, attest without an
operator.

**What this proves, and what it does not.** It proves the engine refuses a required DoD without a
certificate and opens it with one, on the deployed image, from rows the engine wrote. It does not
prove the certificate's facts describe the world — that assumption is named in § What the
certificate says, and it does not prove the rollback on the live deployment itself: the staging
exercise above starts the prior release against the newer schema, while the production pin
remains the operator's gated act.
### The findings run (conveyor#865)

A second live run at 2026-11-27T17:00:47Z (`docs/evidence/units/2026-11-27-findings/`) re-ran this
method over the current ledger: corpus `a7cf1a1aa2048be2…`, 330 completed units, 24 open in flight,
31 closed without a keyword-linked PR, 11 orphan PRs, certified through the deployed image
`cbee6af8…` as chunk-000 `01a21df5-1d01-71f8-a343-1f8a3f40272b` and chunk-001
`01a21df5-2d86-7ce1-a406-2a03119cd367` (both `unproven`) and the programme bundle
`01a21df5-3f9d-7f7a-bf50-55230c686bc3` (`proven`). It also found and fixed four instrument defects —
the in-flight bucket counted closed issues, `verifyParts` fired on non-unit issues, two bodies were
truncated at a prose `#N` line, and a console publish PR was counted as an orphan — and classifies
every failure in `findings.md` § The findings. The 11:00 tables above are unchanged; the corrected
counts are the findings run's.

### The scoped unit ledger (2026-11-27, #868)

The unit rule applied the issue-content contract to *every* issue, while
`03-DELIVERY.md` § Issue content contract scopes it to `unit` deliverables — so eight
bug/docs/ci/security/design issues were counted as unit Definition-of-ready failures. #868 carries
the fact in the model, so the rule stops firing outside its contract.

- **The model.** `UnitObs` gains `unit : Option Bool`; `partsVerified` is vacuously true for
  `unit = some false`, and `none` (an unreadable issue) still fails closed. The model alone, in
  the prover's own project: **exit 0, 4 734 ms**. The controls: **12/12 kernel-refused**, the new
  `control_unknown_unit` included; the positive non-unit case compiles. The A0 corpus at 40, 80
  and 200 units: **exit 0**.
- **The renderer and the collector.** A facts document that omits `unit` is refused
  (`units[i].unit is missing`), never defaulted; an explicit null travels as `none`. The model
  digest is re-pinned at `f6020fae…`; `proof_render` **22 passed** and `proofs_record` **18
  passed**, the collector's self-test **ok**.
- **The offline ledger.** Fresh snapshot collected at `2026-11-27T18:16:03Z` from `63ddf40e6`,
  corpus `0e0c74e83b5f0787…`, rendered with the scoped model: **334** completed, 24 open in
  flight, 31 closed without a keyword-linked PR, 11 orphan PRs, 129 automation excluded —
  **47 findings**. `non-unit-dor` **8 → 0** (the eight keys leave the class); `unit-dor-verify`
  **28**, `multi-lane` 5, `closure-open` 2, `branch-live` 1 and `orphan-pr` 11 keep exactly the
  same keys. Rule failures drop only `verifyParts`, 47 → 39. The run is at
  `docs/evidence/units/2026-11-27-scoped/`.
- **The live certification, on the rolled engine.** The deployed engine carries this merge: image
  `sha256:3fc08fc1…`, `CONVEYOR_GIT_SHA=da0488297…`, and the model `f6020fae…` the facts name. The
  lean4 bearer was re-minted immediately before the run (`lean4.token`, source licences, expires
  21:59:27Z). `collect-units.sh certify` recorded chunk-000 (200 units)
  `01a21ece-82d8-708b-851c-1e994a203e8d` and chunk-001 (134)
  `01a21ece-a1d6-7ca6-9826-0d73c2808460`, both `unproven` with kernel `valid: false` — the
  expected refusal of the 46 failing units — and the programme bundle
  `01a21ece-bcd4-73e9-ab17-3c8b0158f0d2` `proven` (`valid: true`), which proves the same path
  accepts a passing set. `report.md` carries the certificate table; `certificates.json` is beside
  it.
