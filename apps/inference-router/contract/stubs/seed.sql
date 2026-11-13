-- Deterministic ledger state for the golden capture.
--
-- The five /api/internal/* reads aggregate over inference_call, so their goldens are only
-- reproducible if the table starts from a known state. The capture truncates with
-- RESTART IDENTITY and applies this file, then runs a fixed number of calls; two captures then
-- produce identical bytes.
--
-- Timestamps are FIXED ABSOLUTE dates, never now(): a relative seed would move the timeline's
-- day bucket every day and make yesterday's golden unreproducible. They sit outside the 24h
-- window on purpose, so rows_24h counts only the calls the capture itself makes.
--
-- Synthetic identifiers only.

INSERT INTO inference_call (
    created_at, correlation_id, caller_worker, requested_model, served_provider, served_model,
    temperature, max_tokens, request_sha256, response_sha256, latency_ms, status,
    tenant_id, licence_id, licence_source, finish_reason, traffic_source, request_id,
    streamed, response_json_valid
) VALUES
    ('2026-01-15T10:00:00Z', '00000000-0000-4000-8000-000000000e01', 'seed-worker',
     'stub-model', 'stub-openai', 'stub-model', 0.7, 16, repeat('a', 64), repeat('b', 64),
     10, 'ok', '00000000-0000-4000-8000-0000000000a1', NULL, 'none', 'stop', 'live',
     '00000000-0000-4000-8000-00000000d001', false, false),
    ('2026-01-15T11:00:00Z', '00000000-0000-4000-8000-000000000e02', 'seed-worker',
     'stub-model', 'stub-openai', 'stub-model', 0.7, 16, repeat('a', 64), repeat('b', 64),
     20, 'ok', '00000000-0000-4000-8000-0000000000a1', NULL, 'none', 'stop', 'live',
     '00000000-0000-4000-8000-00000000d002', false, false),
    ('2026-01-15T12:00:00Z', '00000000-0000-4000-8000-000000000e03', 'other-worker',
     'stub-model', 'stub-openai', 'stub-model', 0.7, 16, repeat('a', 64), repeat('b', 64),
     30, 'error', '00000000-0000-4000-8000-0000000000a1', NULL, 'none', NULL, 'live',
     '00000000-0000-4000-8000-00000000d003', false, false);
