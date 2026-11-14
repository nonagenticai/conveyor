-- inference_call: licence attribution, sampling penalties and finish_reason (alembic 0010).
-- Additive and nullable only. The lock_timeout is transaction-scoped, so it is safe through a
-- pooler and never queues live inserts behind the ALTER.
SET LOCAL lock_timeout = '5s';

ALTER TABLE inference_call ADD COLUMN licence_id text;
ALTER TABLE inference_call ADD COLUMN licence_source text;
ALTER TABLE inference_call ADD COLUMN repetition_penalty double precision;
ALTER TABLE inference_call ADD COLUMN frequency_penalty double precision;
ALTER TABLE inference_call ADD COLUMN finish_reason text;
