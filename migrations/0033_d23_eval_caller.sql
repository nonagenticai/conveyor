-- S16/D23 — the eval caller kind and its join key (the S19 §9 amendment, applied 2026-11-09).
--
-- S19 §9 specified that an eval's model calls are attributable to the eval run: `caller_kind =
-- 'eval'` plus `inference_call.eval_run_id`. Without it an eval's spend is only known from the
-- usage the runner happened to copy onto `eval_case_result`, and `inference_usage.report` cannot
-- see eval traffic at all — the measurement instrument would be invisible in the ledger it is
-- supposed to be measured by.
--
-- The join key follows CONTRACT §18: a row must always answer "whose call was this", so `eval`
-- requires its own id exactly as the other kinds require theirs.

ALTER TABLE inference_call
    DROP CONSTRAINT inference_call_caller_kind_check;
ALTER TABLE inference_call
    ADD CONSTRAINT inference_call_caller_kind_check
    CHECK (caller_kind IN ('copilot','procedure_step','workflow_step','agent_run','eval','other'));

ALTER TABLE inference_call ADD COLUMN eval_run_id uuid;

ALTER TABLE inference_call DROP CONSTRAINT inference_call_join_key;
ALTER TABLE inference_call
    ADD CONSTRAINT inference_call_join_key
    CHECK (
        caller_kind = 'other'
        OR run_id IS NOT NULL
        OR run_step_id IS NOT NULL
        OR conversation_turn_id IS NOT NULL
        OR eval_run_id IS NOT NULL);

-- An eval call is identified by its eval run; a non-eval call never carries one.
ALTER TABLE inference_call
    ADD CONSTRAINT inference_call_eval_join
    CHECK (caller_kind = 'eval' OR eval_run_id IS NULL);

CREATE INDEX inference_call_eval_run_idx
    ON inference_call (tenant_id, eval_run_id) WHERE eval_run_id IS NOT NULL;
