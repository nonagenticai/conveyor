-- 0081_unit_proof_policy.sql — the engine-derived unit certificate's policy (issue #837).
--
-- A delivery unit's Definition of done is resolved by `development_projects.close` over the
-- engine's own rows. The certificate's facts are derived by the engine (`proofs.attest`) and
-- checked by `checkRecord` in `docs/spec/lean/DeliveryModel.lean`; the specification digest those
-- facts name has no row in the development tables, so the charter pins it here.
--
-- One policy per development project, written at charter and immutable in its specification:
-- `spec_sha256` is never re-pinned (an implementer re-pinning to an easier spec would defeat the
-- gate). `required` selects the gate — advisory when false, refusing when true — and may be
-- tightened through `development_projects.change_judging_surface` by a principal other than the
-- implementer; the implementer can never loosen it (§9.3: a project does not change the surface
-- that judges it). A required policy must name a digest, refused here and by the action.
--
-- Forward only: never edited after this slice closes (CONTRACT §2).

-- The unit's subject is the `development_project` id. `unit_ledger` remains the GitHub-observed
-- chunk subject (#836); `unit` is the engine's own recorded unit.
ALTER TABLE proof DROP CONSTRAINT proof_subject_type_check;
ALTER TABLE proof
    ADD CONSTRAINT proof_subject_type_check
        CHECK (subject_type IN ('card', 'run', 'wbs_node', 'release',
                                'programme', 'package', 'unit_ledger', 'unit'));

CREATE TABLE proof_policy (
    id                      uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id               uuid NOT NULL DEFAULT app_current_tenant(),
    development_project_id  uuid NOT NULL,
    -- The specification the unit's certificate answers, and its pin.
    spec_name               text NOT NULL,
    -- Empty is allowed only while the policy is advisory; a required gate names a digest.
    spec_sha256             text NOT NULL DEFAULT '',
    required                boolean NOT NULL DEFAULT false,
    version                 bigint NOT NULL DEFAULT 1,
    created_at              timestamptz NOT NULL DEFAULT now(),
    updated_at              timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT proof_policy_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id)
        ON DELETE CASCADE,
    CONSTRAINT proof_policy_project_fk FOREIGN KEY (development_project_id)
        REFERENCES development_project (id) ON DELETE CASCADE,
    CONSTRAINT proof_policy_one_per_project UNIQUE (tenant_id, development_project_id),
    CONSTRAINT proof_policy_spec_name_not_blank CHECK (btrim(spec_name) <> ''),
    CONSTRAINT proof_policy_spec_digest_shape
        CHECK (spec_sha256 = '' OR spec_sha256 ~ '^[0-9a-f]{64}$'),
    CONSTRAINT proof_policy_required_needs_spec
        CHECK (NOT required OR spec_sha256 ~ '^[0-9a-f]{64}$')
);

ALTER TABLE proof_policy ENABLE ROW LEVEL SECURITY;
ALTER TABLE proof_policy FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON proof_policy FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
