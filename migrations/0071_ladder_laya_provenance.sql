-- 0071_ladder_laya_provenance.sql — an L1 decision names the verified laya call behind it
-- (SPEC_V2 §7.2; finding v1-w1c-3).
--
-- Before this migration `ladder.evaluate` took the answer, confidence, probabilities and
-- latency from the request body and stamped `rung = 'laya'` on them. The three columns below
-- make the row say which `laya.classify` invocation produced it and what request/response it
-- was verified against; the CHECK refuses a laya row that names none of them.
--
-- The request hash is the idempotency store's own canonical hash (`fnv1a64:<16 hex>:<len>`),
-- so the same value the replay was looked up by is the one the row pins. The response hash is
-- a sha256 of the laya answer document.
ALTER TABLE decision
    ADD COLUMN laya_invocation_id uuid,
    ADD COLUMN laya_request_hash  text,
    ADD COLUMN laya_response_hash text,
    ADD CONSTRAINT decision_laya_invocation_fk
        FOREIGN KEY (laya_invocation_id) REFERENCES action_invocation (id),
    ADD CONSTRAINT decision_laya_provenance_complete
        CHECK (rung <> 'laya'
               OR (laya_invocation_id IS NOT NULL
                   AND laya_request_hash IS NOT NULL
                   AND laya_response_hash IS NOT NULL)),
    ADD CONSTRAINT decision_laya_request_hash_shape
        CHECK (laya_request_hash IS NULL
               OR laya_request_hash ~ '^fnv1a64:[0-9a-f]{16}:[0-9]+$'),
    ADD CONSTRAINT decision_laya_response_hash_shape
        CHECK (laya_response_hash IS NULL
               OR laya_response_hash ~ '^[0-9a-f]{64}$');

-- The citation is a lookup in one direction (decision → its invocation) and a retention
-- guard in the other (`action_invocation.prune` must not delete a cited invocation).
CREATE INDEX decision_laya_invocation_idx
    ON decision (laya_invocation_id)
    WHERE laya_invocation_id IS NOT NULL;
