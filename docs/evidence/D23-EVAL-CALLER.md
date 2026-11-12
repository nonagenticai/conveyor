# D23's amendment — the `eval` caller kind: an eval's model calls are attributable

Lane `s18-kata-improvement` (second commit), migration **`0033_d23_eval_caller.sql`**.

## Why

S19 §9 specified it and S19-C's own report noted it was unbuilt: an eval's model calls went
through the ordinary engine but carried **no join key**, so they either never reached
`inference_call` or (as the judge did) were invisible to it entirely. The consequence was concrete:
`eval_runs.report`'s cost had to be priced from the usage the runner happened to copy onto
`eval_case_result`/`judge_usage`, and `inference_usage.report` could not see eval traffic at all.
The measurement instrument was absent from the ledger everything else is measured by.

## What changed

- **`migrations/0033_d23_eval_caller.sql`** — `caller_kind` gains `eval`; `eval_run_id uuid` is a
  new column; the `inference_call_join_key` CHECK accepts `eval_run_id`, so CONTRACT §18's "whose
  call was this" stays answerable; a new `inference_call_eval_join` CHECK makes the key **required**
  for `eval` and **forbidden** for every other kind; a partial index serves the per-eval read.
- **`conveyor_domain::inference::CallerKind::Eval`** — `ALL` 5 → 6; `as_str`/`parse` follow, so
  `inference_calls.list`'s `caller_kind` filter accepts it with no new validation code.
- **The engine** — `UsageContext` and `CaptureRow` carry `eval_run_id`; the ledger insert binds it.
- **The eval runner** — both invoke sites now pass a `UsageContext` naming `Eval` +
  `eval_run_id`: the **subject** call (`eval.entry_action`, `prompt_ref = eval:<action>`) and the
  **judge** call (`completions.create`, `prompt_ref = eval_judge`). The engine still records only
  when the action declares `captures_usage`, so a subject that is itself a `procedure` or
  `agent.run` is unaffected — its own inner calls keep their own kinds.
- **The reads** — `InferenceCallView.eval_run_id`, `InferenceCallsListIn.eval_run_id`, and the
  store's keyset filter.

## Proof

`crates/store/tests/s16_schema.rs::the_eval_caller_kind_requires_and_owns_its_join_key` asserts the
three limbs against the schema itself, not against a Rust convention: `eval` **with** its key is
accepted, `eval` **without** it is `23514`, and any other kind carrying one is `23514`. Each limb
runs in its own transaction — a refused `INSERT` aborts the transaction it ran in, so a second
negative probe on the same one answers `25P02` instead of exercising the CHECK (the first draft of
this test made exactly that mistake, and it failed).

Green: `conveyor-store --test s16_schema` **2/2**, `--test s16_capture` **4/4**, `--test
s16_prune` **2/2**, `--test inference_` **3/3**, `--test prompt_audit` **3/3**; `conveyor-store
--lib` **37/37** (the embedded migration set now ends at `0033`).

## What this does not do

The amendment makes eval calls *attributable*; it does not move `eval_runs.report`'s cost onto the
ledger. That report still prices the recorded `usage`/`judge_usage` on the result row, which is the
same number by construction once the judge's call is captured. Switching the report to a ledger
join is a separate change and is not needed for the ledger to be complete.
