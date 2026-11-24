# S21 step 10 — the L1 laya gate: receipt

Issue #656 · branch `issue/656-s21-l1-laya` · ADR `0018-l1-laya-provenance` · CONTRACT §28 ·
S21 §22 · resolves rework `v1-w1c-3`, `v1-w1c-4`, `v1-w1c-5`

**State: built.** Measured at `7d56abb2` on 2026-11-24, before the PR. The merge is the lane's
last CI run; nothing in this receipt is claimed for `main` yet.

## What changes at this level

Before: `ladder.evaluate` took `answer`, `confidence`, `latency_ms` and `probabilities` from its
request body and wrote `rung = 'laya'`, so any `governance:write` caller could record a
calibrated-looking L1 decision it had invented; options were inline request text; an outage could
not become `gate_unavailable`; and nothing could stop the case being asked.

After: the ladder builds the case from the effective policy and an authored, versioned option set;
the engine performs the request as the external action `ladder.precheck`; `ladder.evaluate` records
a decision only from the engine's recorded exchange (the claim's request hash, a 2xx status, the
invocation's measured duration); the answer must be an authored key; an unavailable answer follows
the policy's fail action; and a policy whose `fire_when` does not hold is not asked at all.

## Measured suites (exact head `7d56abb2`)

| Suite | Result |
|---|---|
| `crates/actions/tests/ladder_laya.rs` (new) | **15 passed** |
| `crates/actions/tests/decision_ladder.rs` (rewritten to the verified path) | **7 passed** |
| `crates/actions/tests/draft_apply.rs` (rewritten) | **7 passed** |
| `crates/actions/tests/internal_agent_runtime.rs` | **18 passed, 2 ignored** |
| `crates/actions/tests/executor_runtime.rs` | **8 passed** |
| `crates/actions/tests/registry.rs` | **12 passed** — 543 actions (524 internal, 19 external) |
| `crates/actions/tests/parity_http_mcp.rs` | **7 passed** — both option-set actions and `ladder.evaluate`'s refusal sampled over HTTP and MCP |
| `crates/store/tests/prompt_catalogue.rs` | **7 passed** |
| `crates/actions` lib unit tests | **175 passed** |
| `clippy -D warnings` (domain, store, actions, api, server; all targets) | **exit 0** |
| `cargo fmt --all -- --check` | **exit 0** |
| `served-spec.lock.json` | **574 operations**, `--check-lock` exit 0 |
| design-pack gate (coverage ×2, seed, docs pack, data/standards/outputs/console/v2/studio/learning/workbench/architecture) | **all pass** |
| rework audit `--rework-check v1-w1c-3/-4/-5` | **each prints "is cleared"** |
| `board.py --dry-run` | clean (reported only, nothing changed) |

## Scenarios

| Scenario | Test | Result |
|---|---|---|
| P1.1 the precheck records the case under its claim | `ladder_laya::the_precheck_call_records_the_case` | pass — one stub call, the policy's `model_ref` and a typed `score` question |
| P1.2 the decision derives from the recording | `::evaluate_derives_the_verdict_from_the_recording` | pass — answer/confidence from the response, measured `latency_ms`, `meta.laya_call_key` / `laya_invocation_id` / `case_sha256` |
| P1.3 a caller-supplied verdict is refused | `::a_caller_supplied_confidence_is_refused` | pass — `422 caller_supplied_verdict`, zero decisions |
| P1.4 another case's claim is refused | `::a_call_for_another_case_is_refused` | pass — `422 laya_call_unverified`, zero decisions |
| P1.5 the gate makes one laya request | `internal_agent_runtime::the_gate_records_a_laya_decision` | pass — stub hit count 1, one decision |
| P2.1 the question comes from the authored set | `ladder_laya::the_question_comes_from_the_authored_set` | pass — `choice`, criteria `[accept, refuse]`; inline `options` `422 invalid_input` (`inline_options`) |
| P2.2 an out-of-set answer is refused | `::an_answer_outside_the_set_is_refused` | pass — `422 laya_answer_not_in_options`, zero decisions |
| P2.3 a declared choice with no set is refused | `::a_case_without_an_authored_set_is_refused` | pass — `409 option_set_missing` |
| P2.4 authoring twice bumps the version | `::the_set_version_increments` | pass — versions 1 then 2, `.get` reads 2 |
| P2.5 the decision records the authored rows | `::a_choice_decision_records_the_authored_set` | pass — `option_set_version=1`, two `decision_option` rows with the authored labels |
| P3.1 an outage follows the policy | `::an_outage_follows_the_fail_policy` | pass — closed → refused, open+handover → escalated, `meta.gate_unavailable`, one call per case |
| P3.1b no recorded call writes no decision | `::an_unrecorded_outage_writes_no_decision` | pass — `422 laya_call_unverified`, zero decisions |
| P3.2 a non-firing case is not asked | `::a_case_that_does_not_fire_is_not_asked` | pass — `fires:false`, `409 case_not_fired`, zero stub calls; clearing `fire_when` makes it fire again |
| P4.2 the register clears the findings | the rework audit above | pass — all three cleared |

## Reversal proofs (fired, red, reverted, green)

| # | Edit that reverses the property | Test | Observed |
|---|---|---|---|
| 1 | accept a caller-supplied verdict (the refusal check removed) | `a_caller_supplied_confidence_is_refused` | **red** — the contract's named refusal disappears; the request falls through to `laya_call_unverified` |
| 2 | build a constant, schema-valid case body (the facts ignored) | `a_call_for_another_case_is_refused` | **red** — `201`, another case's exchange accepted |
| 3 | accept an answer outside the authored set (`unwrap_or("accepted")`) | `an_answer_outside_the_set_is_refused` | **red** — `201` |
| 4 | ignore `fail_policy` on an unreadable answer (`Outcome::Accepted`) | `an_outage_follows_the_fail_policy` | **red** — `accepted` instead of `refused` |
| 5 | make the executor call `ladder.precheck` a second time | `internal_agent_runtime::the_gate_records_a_laya_decision` | **red** — "one laya call": 2 |

All five were reverted to the committed head before the next ran; the table's green state is the
suite results above.

## Notes and fixture amendments (recorded, not hidden)

- **`decision.input_hash` and `meta.case_sha256` are the case's sha256.** The engine's idempotency
  witness is a different hash (`idempotency::request_hash`, an FNV-1a claim hash); the P1b cut had
  used it as `input_hash`, which the column's sha256 check refuses. The case now carries both:
  `Case::hash` (the claim) and `Case::sha256` (the decision).
- **The executor now uses the shared builder.** It previously constructed its own `judge::request`
  and derived the claim from that copy; `decide()` now calls `case::build`, so the authored menu and
  `fire_when` reach an in-process gate too.
- **The parity samples were rewritten, not relaxed.** `ladder.evaluate`'s sample now presents a
  claim no engine call backs and both transports must refuse it (`422`); the verified happy path is
  `ladder_laya.rs`'s, since a parity sample cannot call a real laya. `decision_option_sets.set` is
  sampled live (the set it authors is read by `.get` in the same round).
- **Two ladder test helpers changed shape.** `decision_ladder`'s `evaluate` now drives the verified
  path with a stub whose verdict it controls, and uses a distinct draft text per verdict: two
  evaluations of an identical case share the claim and the engine replays the first exchange — that
  is the verification working, not a fixture shortcut.
- **A deterministic parity sample cannot use `prepare`** (that branch of the harness runs none);
  the read is chained to the set the authoring sample just wrote.

## Residuals (kept, not fixed here)

- Automatic apply of an L1-accepted draft (ADR 0011:77) — the decision now exists honestly; what
  happens to an accepted draft is its own unit.
- The live model/laya proof remains **blocked** on the estate's caller licence (S21-RUN.md); every
  proof here is stub-backed and the tests say so.
- The SOP judge (`laya.judge`, `process/step.rs`) keeps its own question and threshold.
- Mid-loop budget enforcement is the #526 lane's; release pinning (`v1-p42-1`), prompt-budget
  authoring (`v1-p77-2`) and the NULL-hash guard (`v1-p77-1`) are untouched.
- A tenant-authored `fire_when` / question-type UI; both are authored through the actions.
