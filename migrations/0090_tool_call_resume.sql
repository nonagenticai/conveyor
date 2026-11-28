-- SPEC_V2 §11.3 (T13, v1-w4a-5): a call that exceeds the tool's own `timeout_secs` is
-- `pending`, not a silent backend failure. The resume token is the status tool's handle on that
-- call; the ledger stays append-only — the token is written with the pending row, never updated.
ALTER TABLE tool_call ADD COLUMN resume_token text;

CREATE UNIQUE INDEX tool_call_resume_token_key
    ON tool_call (tenant_id, resume_token)
    WHERE resume_token IS NOT NULL;
