-- 0076_lean_proof.sql — the delivery-proof certificate (issue #744).
--
-- SPEC_V2 §5.3/§5.4: a deliverable is accepted on evidence, and a gate resolves its evidence
-- before it opens. A `proof` row is the re-checkable form of that evidence: the canonical facts,
-- the spec they answer, the exact Lean source the prover checked, the verdict, and the identity of
-- the checker. It is written once and never edited — a re-verification is the same row.
--
-- `certificate_sha256` is the digest of the canonical tuple, computed by `proofs.record` in Rust
-- (`conveyor_domain::compare::content_hash`). `facts_sha256` is the digest of `facts_canonical`,
-- so a verifier can re-render the source from the stored facts and refuse a mismatch without
-- hashing anything inside Lean.
--
-- Forward only: never edited after this slice closes (CONTRACT §2).

CREATE TABLE proof (
    id                   uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id            uuid NOT NULL DEFAULT app_current_tenant(),
    -- what the certificate is about
    subject_type         text NOT NULL
                         CHECK (subject_type IN ('card', 'run', 'wbs_node', 'release',
                                                 'programme', 'package')),
    subject_id           uuid NOT NULL,
    -- the specification the statement answers
    spec_name            text NOT NULL,
    spec_sha256          text NOT NULL CHECK (spec_sha256 ~ '^[0-9a-f]{64}$'),
    -- the facts, canonically serialised, and their digest
    facts_canonical      text NOT NULL,
    facts_sha256         text NOT NULL CHECK (facts_sha256 ~ '^[0-9a-f]{64}$'),
    -- the check itself
    lean_source          text NOT NULL,
    theorem              text NOT NULL,
    -- `kernel` is `by decide`; `native` is `native_decide`, which trusts the compiler and is a
    -- different claim. Recorded rather than flattened, because the difference matters to a reader.
    trust                text NOT NULL CHECK (trust IN ('kernel', 'native')),
    verdict              text NOT NULL CHECK (verdict IN ('proven', 'unproven')),
    computation_time_ms  double precision,
    prover_identity      jsonb NOT NULL DEFAULT '{}'::jsonb,
    -- the engine invocation this certificate records, and its idempotency key
    invocation_id        uuid,
    idempotency_key      text NOT NULL,
    certificate_sha256   text NOT NULL CHECK (certificate_sha256 ~ '^[0-9a-f]{64}$'),
    created_at           timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT proof_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT proof_spec_name_not_blank CHECK (btrim(spec_name) <> ''),
    CONSTRAINT proof_facts_not_blank CHECK (btrim(facts_canonical) <> ''),
    CONSTRAINT proof_source_not_blank CHECK (btrim(lean_source) <> ''),
    CONSTRAINT proof_theorem_not_blank CHECK (btrim(theorem) <> ''),
    CONSTRAINT proof_key_not_blank CHECK (btrim(idempotency_key) <> ''),
    CONSTRAINT proof_timeout_not_negative
        CHECK (computation_time_ms IS NULL OR computation_time_ms >= 0),
    -- one certificate per tuple, and one certificate per engine call: a second record of the same
    -- invocation is a replay, not a second proof
    CONSTRAINT proof_certificate_unique UNIQUE (tenant_id, certificate_sha256),
    CONSTRAINT proof_invocation_unique UNIQUE (tenant_id, idempotency_key)
);

CREATE INDEX proof_subject_idx ON proof (tenant_id, subject_type, subject_id, created_at DESC);
CREATE INDEX proof_spec_idx ON proof (tenant_id, spec_sha256);

ALTER TABLE proof ENABLE ROW LEVEL SECURITY;
ALTER TABLE proof FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON proof FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
