-- inference_call: the policy decision recorded on every call (alembic 0016).
SET LOCAL lock_timeout = '5s';

ALTER TABLE inference_call ADD COLUMN policy_version integer;
ALTER TABLE inference_call ADD COLUMN llm_setup smallint;
ALTER TABLE inference_call ADD COLUMN hop_index smallint;
ALTER TABLE inference_call ADD COLUMN hop_reason text;
ALTER TABLE inference_call ADD COLUMN key_owner text;

ALTER TABLE inference_call ADD CONSTRAINT ck_inference_call_llm_setup
    CHECK (llm_setup IN (1, 2, 3)) NOT VALID;
ALTER TABLE inference_call ADD CONSTRAINT ck_inference_call_key_owner
    CHECK (key_owner IN ('platform', 'owner', 'tenant')) NOT VALID;
ALTER TABLE inference_call ADD CONSTRAINT ck_inference_call_hop_reason
    CHECK (hop_reason IN ('first', 'failover_error', 'failover_validation')) NOT VALID;
ALTER TABLE inference_call ADD CONSTRAINT ck_inference_call_hop_index
    CHECK (hop_index >= 0) NOT VALID;
