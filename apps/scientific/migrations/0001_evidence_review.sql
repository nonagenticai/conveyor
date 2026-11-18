-- The evidence-review schema at its migration head: 47 tables.
--
-- Consolidated from 28 historical revisions of the captured service (pin recorded in
-- docs/evidence/evidence-review-capture-2026-11-18.md). The history is not reproduced;
-- the head is. Where a later revision altered an earlier column the final type is used
-- (notably every tenant column is `uuid`, not the original `text`).
--
-- Three invariants live in the database here, not in application code:
--   * every table is behind `ENABLE` + `FORCE ROW LEVEL SECURITY` keyed on
--     `app.current_tenant`, so a non-owner runtime role cannot turn a policy off;
--   * history is append-only: `BEFORE UPDATE OR DELETE` triggers raise
--     `restrict_violation` (SQLSTATE 23001);
--   * "current" is the tail of a supersession chain made linear by partial unique
--     indexes -- no table carries an `is_current` flag.

-- ---------------------------------------------------------------------------
-- 0. Helpers used by CHECK constraints (must exist before the tables)
-- ---------------------------------------------------------------------------

-- Recursive 512-character bound: no string anywhere in a JSON payload may carry
-- more than a minimal fact, so an abstract or full text cannot be stored in an
-- outbox/job row even by a caller that wants to.
CREATE OR REPLACE FUNCTION er_payload_is_minimal(p jsonb) RETURNS boolean AS $$
DECLARE
  child jsonb;
BEGIN
  IF p IS NULL THEN
    RETURN true;
  END IF;
  CASE jsonb_typeof(p)
    WHEN 'string' THEN
      RETURN length(p #>> '{}') <= 512;
    WHEN 'array' THEN
      FOR child IN SELECT jsonb_array_elements(p) LOOP
        IF NOT er_payload_is_minimal(child) THEN
          RETURN false;
        END IF;
      END LOOP;
      RETURN true;
    WHEN 'object' THEN
      FOR child IN SELECT value FROM jsonb_each(p) LOOP
        IF NOT er_payload_is_minimal(child) THEN
          RETURN false;
        END IF;
      END LOOP;
      RETURN true;
    ELSE
      RETURN true;
  END CASE;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- A CHECK cannot contain a subquery, so "this array names nothing twice" needs a
-- function.
CREATE OR REPLACE FUNCTION er_text_array_is_a_set(a text[]) RETURNS boolean AS $$
  SELECT a IS NULL
      OR cardinality(a) = (SELECT count(DISTINCT x) FROM unnest(a) AS x);
$$ LANGUAGE sql IMMUTABLE;

-- ---------------------------------------------------------------------------
-- 1. Idempotency (revision 0001)
-- ---------------------------------------------------------------------------

CREATE TABLE operation_idempotency (
    resource_scope text NOT NULL,
    resource_tenant_id uuid NOT NULL,
    principal_subject_id text NOT NULL,
    operation_id text NOT NULL,
    idempotency_key text NOT NULL,
    request_hash text NOT NULL,
    response_status integer NOT NULL,
    response_headers json NOT NULL DEFAULT '{}'::json,
    response_body json,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT pk_operation_idempotency PRIMARY KEY (
        resource_scope, resource_tenant_id, principal_subject_id, operation_id, idempotency_key
    ),
    CONSTRAINT ck_operation_idempotency_principal_not_blank CHECK (btrim(principal_subject_id) <> '')
);

-- ---------------------------------------------------------------------------
-- 2. Reviews, protocol versions, stages, state history (revision 0002)
-- ---------------------------------------------------------------------------

CREATE TABLE reviews (
    review_id uuid PRIMARY KEY,
    tenant_id uuid NOT NULL,
    project_id text NOT NULL,
    external_key text NOT NULL,
    title text NOT NULL,
    created_by_subject_id text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT uq_reviews_project_external_key UNIQUE (tenant_id, project_id, external_key),
    CONSTRAINT ck_reviews_creator_not_blank CHECK (btrim(created_by_subject_id) <> '')
);

CREATE TABLE review_protocol_versions (
    protocol_version_id uuid PRIMARY KEY,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    version_number integer NOT NULL,
    supersedes_version_id uuid,
    schema_ref text NOT NULL,
    content_hash text NOT NULL,
    inclusion_criteria jsonb NOT NULL,
    exclusion_criteria jsonb NOT NULL,
    stage_definitions jsonb NOT NULL,
    exclusion_reason_vocabulary jsonb NOT NULL,
    reviewers_required integer NOT NULL DEFAULT 2,
    locked_at timestamptz,
    locked_by_subject_id text,
    created_by_subject_id text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_protocol_version_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT fk_protocol_version_supersedes FOREIGN KEY (supersedes_version_id)
        REFERENCES review_protocol_versions (protocol_version_id) ON DELETE RESTRICT,
    CONSTRAINT uq_protocol_version_number UNIQUE (review_id, version_number),
    CONSTRAINT ck_protocol_reviewers_required_positive CHECK (reviewers_required >= 1),
    CONSTRAINT ck_protocol_reason_vocabulary_is_array CHECK (jsonb_typeof(exclusion_reason_vocabulary) = 'array'),
    CONSTRAINT ck_protocol_lock_is_attributable CHECK (
        (locked_at IS NULL AND locked_by_subject_id IS NULL)
        OR (locked_at IS NOT NULL AND btrim(locked_by_subject_id) <> '')
    )
);

CREATE TABLE review_stages (
    stage_id uuid PRIMARY KEY,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    protocol_version_id uuid NOT NULL,
    stage_key text NOT NULL,
    stage_order integer NOT NULL,
    name text NOT NULL,
    reviewers_required integer NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_review_stage_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT fk_review_stage_protocol FOREIGN KEY (protocol_version_id)
        REFERENCES review_protocol_versions (protocol_version_id) ON DELETE RESTRICT,
    CONSTRAINT uq_review_stage_key UNIQUE (review_id, stage_key),
    CONSTRAINT ck_stage_reviewers_required_positive CHECK (reviewers_required >= 1)
);

CREATE TABLE review_state_history (
    state_event_id uuid PRIMARY KEY,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    revision_number integer NOT NULL,
    from_state text,
    to_state text NOT NULL,
    rationale text NOT NULL,
    actor_subject_id text NOT NULL,
    protocol_version_id uuid,
    approval_attestation_ref text,
    occurred_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_state_history_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT fk_state_history_protocol FOREIGN KEY (protocol_version_id)
        REFERENCES review_protocol_versions (protocol_version_id) ON DELETE RESTRICT,
    CONSTRAINT uq_review_state_revision UNIQUE (review_id, revision_number),
    CONSTRAINT ck_state_revision_positive CHECK (revision_number >= 1),
    CONSTRAINT ck_state_rationale_not_blank CHECK (btrim(rationale) <> ''),
    CONSTRAINT ck_state_actor_not_blank CHECK (btrim(actor_subject_id) <> ''),
    CONSTRAINT ck_state_to_state_known CHECK (
        to_state IN ('draft', 'configured', 'active', 'paused', 'closed', 'archived')
    ),
    CONSTRAINT ck_state_active_requires_protocol CHECK (
        (to_state <> 'active') OR (protocol_version_id IS NOT NULL)
    ),
    CONSTRAINT ck_state_reopen_requires_attestation CHECK (
        (NOT (from_state = 'closed' AND to_state = 'active'))
        OR (btrim(coalesce(approval_attestation_ref, '')) <> '')
    )
);

-- ---------------------------------------------------------------------------
-- 3. Imports, records, sealed corpus (revision 0003)
-- ---------------------------------------------------------------------------

CREATE TABLE import_batches (
    import_batch_id uuid PRIMARY KEY,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    source_kind text NOT NULL,
    source_ref jsonb,
    raw_payload_digest text NOT NULL,
    submitted_by_subject_id text NOT NULL,
    accepted_count integer NOT NULL DEFAULT 0,
    rejected_count integer NOT NULL DEFAULT 0,
    quarantined_count integer NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_import_batch_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT ck_import_batch_source_kind CHECK (
        source_kind IN ('discovery_session', 'ris', 'bibtex', 'csv', 'canonical_json')
    ),
    CONSTRAINT ck_import_batch_submitter_not_blank CHECK (btrim(submitted_by_subject_id) <> ''),
    CONSTRAINT uq_import_batch_payload_digest UNIQUE (review_id, raw_payload_digest)
);

CREATE TABLE import_rows (
    import_row_id uuid PRIMARY KEY,
    import_batch_id uuid NOT NULL,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    row_index integer NOT NULL,
    raw_representation jsonb NOT NULL,
    status text NOT NULL,
    quarantine_reason text,
    review_record_id uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_import_row_batch FOREIGN KEY (import_batch_id)
        REFERENCES import_batches (import_batch_id) ON DELETE RESTRICT,
    CONSTRAINT fk_import_row_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT uq_import_row_index UNIQUE (import_batch_id, row_index),
    CONSTRAINT ck_import_row_status CHECK (status IN ('accepted', 'rejected', 'quarantined')),
    CONSTRAINT ck_import_row_outcome_consistent CHECK (
        (status = 'accepted' AND review_record_id IS NOT NULL AND quarantine_reason IS NULL)
        OR (status <> 'accepted' AND review_record_id IS NULL
            AND btrim(coalesce(quarantine_reason, '')) <> '')
    )
);

CREATE TABLE review_records (
    review_record_id uuid PRIMARY KEY,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_review_record_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT
);

CREATE TABLE review_record_versions (
    review_record_version_id uuid PRIMARY KEY,
    review_record_id uuid NOT NULL,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    version_number integer NOT NULL,
    supersedes_version_id uuid,
    schema_ref text NOT NULL,
    content_hash text NOT NULL,
    record_type text NOT NULL,
    title text,
    abstract text,
    publication_year integer,
    authors jsonb,
    container_title text,
    doi text,
    pmid text,
    pmcid text,
    arxiv_id text,
    url text,
    normalized_title text,
    normalized_doi text,
    normalized_pmid text,
    field_provenance jsonb NOT NULL DEFAULT '{}'::jsonb,
    source_import_row_id uuid,
    created_by_subject_id text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_record_version_record FOREIGN KEY (review_record_id)
        REFERENCES review_records (review_record_id) ON DELETE RESTRICT,
    CONSTRAINT fk_record_version_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT fk_record_version_supersedes FOREIGN KEY (supersedes_version_id)
        REFERENCES review_record_versions (review_record_version_id) ON DELETE RESTRICT,
    CONSTRAINT fk_record_version_source_row FOREIGN KEY (source_import_row_id)
        REFERENCES import_rows (import_row_id) ON DELETE RESTRICT,
    CONSTRAINT uq_record_version_number UNIQUE (review_record_id, version_number),
    CONSTRAINT ck_record_version_record_type CHECK (
        record_type IN ('journal_article', 'preprint', 'report', 'dataset', 'other')
    ),
    CONSTRAINT ck_record_version_number_positive CHECK (version_number >= 1),
    CONSTRAINT uq_record_version_supersession_linear UNIQUE (supersedes_version_id)
);

CREATE TABLE corpus_versions (
    corpus_version_id uuid PRIMARY KEY,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    version_number integer NOT NULL,
    supersedes_version_id uuid,
    member_count integer NOT NULL,
    member_ordering_rule text NOT NULL,
    member_manifest_hash text NOT NULL,
    created_by_subject_id text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_corpus_version_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT fk_corpus_version_supersedes FOREIGN KEY (supersedes_version_id)
        REFERENCES corpus_versions (corpus_version_id) ON DELETE RESTRICT,
    CONSTRAINT uq_corpus_version_number UNIQUE (review_id, version_number),
    CONSTRAINT uq_corpus_version_supersession_linear UNIQUE (supersedes_version_id),
    CONSTRAINT ck_corpus_member_count_non_negative CHECK (member_count >= 0),
    CONSTRAINT ck_corpus_ordering_rule_known CHECK (member_ordering_rule = 'record_version_id_asc'),
    CONSTRAINT ck_corpus_manifest_hash_shape CHECK (member_manifest_hash ~ '^sha256:[0-9a-f]{64}$')
);

CREATE TABLE corpus_version_members (
    corpus_version_id uuid NOT NULL,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    review_record_id uuid NOT NULL,
    review_record_version_id uuid NOT NULL,
    member_ordinal integer NOT NULL,
    CONSTRAINT pk_corpus_version_members PRIMARY KEY (corpus_version_id, review_record_id),
    CONSTRAINT fk_corpus_member_version FOREIGN KEY (corpus_version_id)
        REFERENCES corpus_versions (corpus_version_id) ON DELETE RESTRICT,
    CONSTRAINT fk_corpus_member_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT fk_corpus_member_record FOREIGN KEY (review_record_id)
        REFERENCES review_records (review_record_id) ON DELETE RESTRICT,
    CONSTRAINT fk_corpus_member_record_version FOREIGN KEY (review_record_version_id)
        REFERENCES review_record_versions (review_record_version_id) ON DELETE RESTRICT,
    CONSTRAINT uq_corpus_member_ordinal UNIQUE (corpus_version_id, member_ordinal),
    CONSTRAINT ck_corpus_member_ordinal_positive CHECK (member_ordinal >= 1)
);

-- ---------------------------------------------------------------------------
-- 4. Deduplication (revision 0004)
-- ---------------------------------------------------------------------------

CREATE TABLE deduplication_runs (
    dedup_run_id uuid PRIMARY KEY,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    corpus_version_id uuid NOT NULL,
    engine_id text NOT NULL,
    engine_version text NOT NULL,
    config jsonb NOT NULL,
    input_digest text NOT NULL,
    cluster_count integer NOT NULL DEFAULT 0,
    pair_count integer NOT NULL DEFAULT 0,
    created_by_subject_id text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_dedup_run_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT fk_dedup_run_corpus FOREIGN KEY (corpus_version_id)
        REFERENCES corpus_versions (corpus_version_id) ON DELETE RESTRICT,
    CONSTRAINT uq_dedup_run_input_digest UNIQUE (review_id, input_digest)
);

CREATE TABLE duplicate_pair_scores (
    pair_score_id uuid PRIMARY KEY,
    dedup_run_id uuid NOT NULL,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    left_record_id uuid NOT NULL,
    right_record_id uuid NOT NULL,
    matched_on text NOT NULL,
    score numeric(6, 5) NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_pair_score_run FOREIGN KEY (dedup_run_id)
        REFERENCES deduplication_runs (dedup_run_id) ON DELETE RESTRICT,
    CONSTRAINT fk_pair_score_left FOREIGN KEY (left_record_id)
        REFERENCES review_records (review_record_id) ON DELETE RESTRICT,
    CONSTRAINT fk_pair_score_right FOREIGN KEY (right_record_id)
        REFERENCES review_records (review_record_id) ON DELETE RESTRICT,
    CONSTRAINT ck_pair_ordered CHECK (left_record_id < right_record_id),
    CONSTRAINT uq_pair_per_run UNIQUE (dedup_run_id, left_record_id, right_record_id)
);

CREATE TABLE duplicate_clusters (
    cluster_id uuid PRIMARY KEY,
    dedup_run_id uuid NOT NULL,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    cluster_key text NOT NULL,
    member_count integer NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_cluster_run FOREIGN KEY (dedup_run_id)
        REFERENCES deduplication_runs (dedup_run_id) ON DELETE RESTRICT,
    CONSTRAINT fk_cluster_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT uq_cluster_key_per_run UNIQUE (dedup_run_id, cluster_key),
    CONSTRAINT ck_cluster_has_two_members CHECK (member_count >= 2)
);

CREATE TABLE duplicate_cluster_members (
    cluster_id uuid NOT NULL,
    review_record_id uuid NOT NULL,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    review_record_version_id uuid NOT NULL,
    CONSTRAINT pk_duplicate_cluster_members PRIMARY KEY (cluster_id, review_record_id),
    CONSTRAINT fk_cluster_member_cluster FOREIGN KEY (cluster_id)
        REFERENCES duplicate_clusters (cluster_id) ON DELETE RESTRICT,
    CONSTRAINT fk_cluster_member_record FOREIGN KEY (review_record_id)
        REFERENCES review_records (review_record_id) ON DELETE RESTRICT,
    CONSTRAINT fk_cluster_member_record_version FOREIGN KEY (review_record_version_id)
        REFERENCES review_record_versions (review_record_version_id) ON DELETE RESTRICT
);

CREATE TABLE duplicate_resolutions (
    resolution_id uuid PRIMARY KEY,
    cluster_id uuid NOT NULL,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    resolution_type text NOT NULL,
    supersedes_resolution_id uuid,
    survivor_record_id uuid,
    rationale text NOT NULL,
    decided_by_subject_id text NOT NULL,
    decided_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_resolution_cluster FOREIGN KEY (cluster_id)
        REFERENCES duplicate_clusters (cluster_id) ON DELETE RESTRICT,
    CONSTRAINT fk_resolution_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT fk_resolution_supersedes FOREIGN KEY (supersedes_resolution_id)
        REFERENCES duplicate_resolutions (resolution_id) ON DELETE RESTRICT,
    CONSTRAINT fk_resolution_survivor FOREIGN KEY (survivor_record_id)
        REFERENCES review_records (review_record_id) ON DELETE RESTRICT,
    CONSTRAINT ck_resolution_type CHECK (resolution_type IN ('merge', 'split', 'no_match')),
    CONSTRAINT ck_resolution_rationale_not_blank CHECK (btrim(rationale) <> ''),
    CONSTRAINT ck_resolution_actor_not_blank CHECK (btrim(decided_by_subject_id) <> ''),
    CONSTRAINT ck_resolution_survivor_matches_type CHECK (
        (resolution_type = 'merge' AND survivor_record_id IS NOT NULL)
        OR (resolution_type <> 'merge' AND survivor_record_id IS NULL)
    )
);

CREATE TABLE duplicate_survivorship_fields (
    survivorship_field_id uuid PRIMARY KEY,
    resolution_id uuid NOT NULL,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    field_name text NOT NULL,
    value jsonb,
    source_record_version_id uuid NOT NULL,
    CONSTRAINT fk_survivorship_resolution FOREIGN KEY (resolution_id)
        REFERENCES duplicate_resolutions (resolution_id) ON DELETE RESTRICT,
    CONSTRAINT fk_survivorship_source_version FOREIGN KEY (source_record_version_id)
        REFERENCES review_record_versions (review_record_version_id) ON DELETE RESTRICT,
    CONSTRAINT uq_survivorship_field_once UNIQUE (resolution_id, field_name),
    CONSTRAINT ck_survivorship_field_name CHECK (field_name IN (
        'arxiv_id', 'abstract', 'authors', 'container_title', 'doi', 'pmcid', 'pmid',
        'publication_year', 'record_type', 'title', 'url'
    ))
);

-- ---------------------------------------------------------------------------
-- 5. Screening, flow, exports (revision 0005, altered by 0006/0008/0009/0016)
-- ---------------------------------------------------------------------------

CREATE TABLE screening_assignments (
    assignment_id uuid PRIMARY KEY,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    stage_id uuid NOT NULL,
    round_number integer NOT NULL DEFAULT 1,
    review_record_id uuid NOT NULL,
    reviewer_subject_id text NOT NULL,
    assigned_by_subject_id text NOT NULL,
    assigned_at timestamptz NOT NULL DEFAULT now(),
    corpus_version_id uuid NOT NULL,
    CONSTRAINT fk_assignment_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT fk_assignment_stage FOREIGN KEY (stage_id)
        REFERENCES review_stages (stage_id) ON DELETE RESTRICT,
    CONSTRAINT fk_assignment_record FOREIGN KEY (review_record_id)
        REFERENCES review_records (review_record_id) ON DELETE RESTRICT,
    CONSTRAINT fk_assignment_corpus_version FOREIGN KEY (corpus_version_id)
        REFERENCES corpus_versions (corpus_version_id) ON DELETE RESTRICT,
    CONSTRAINT uq_assignment_once UNIQUE (
        review_id, stage_id, round_number, review_record_id, reviewer_subject_id
    ),
    CONSTRAINT ck_assignment_reviewer_not_blank CHECK (btrim(reviewer_subject_id) <> ''),
    CONSTRAINT ck_assignment_round_positive CHECK (round_number >= 1)
);

CREATE TABLE screening_decisions (
    decision_id uuid PRIMARY KEY,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    stage_id uuid NOT NULL,
    round_number integer NOT NULL DEFAULT 1,
    review_record_id uuid NOT NULL,
    review_record_version_id uuid NOT NULL,
    assignment_id uuid NOT NULL,
    reviewer_subject_id text NOT NULL,
    disposition text NOT NULL,
    exclusion_reason_code text,
    rationale text,
    supersedes_decision_id uuid,
    decided_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_decision_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT fk_decision_stage FOREIGN KEY (stage_id)
        REFERENCES review_stages (stage_id) ON DELETE RESTRICT,
    CONSTRAINT fk_decision_record FOREIGN KEY (review_record_id)
        REFERENCES review_records (review_record_id) ON DELETE RESTRICT,
    CONSTRAINT fk_decision_record_version FOREIGN KEY (review_record_version_id)
        REFERENCES review_record_versions (review_record_version_id) ON DELETE RESTRICT,
    CONSTRAINT fk_decision_assignment FOREIGN KEY (assignment_id)
        REFERENCES screening_assignments (assignment_id) ON DELETE RESTRICT,
    CONSTRAINT fk_decision_supersedes FOREIGN KEY (supersedes_decision_id)
        REFERENCES screening_decisions (decision_id) ON DELETE RESTRICT,
    CONSTRAINT ck_decision_disposition CHECK (disposition IN ('include', 'exclude', 'uncertain')),
    CONSTRAINT ck_decision_exclusion_has_reason CHECK (
        (disposition <> 'exclude') OR (btrim(coalesce(exclusion_reason_code, '')) <> '')
    ),
    CONSTRAINT ck_decision_reviewer_not_blank CHECK (btrim(reviewer_subject_id) <> '')
);

CREATE TABLE screening_conflicts (
    conflict_id uuid PRIMARY KEY,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    stage_id uuid NOT NULL,
    round_number integer NOT NULL,
    review_record_id uuid NOT NULL,
    opened_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_conflict_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT fk_conflict_stage FOREIGN KEY (stage_id)
        REFERENCES review_stages (stage_id) ON DELETE RESTRICT,
    CONSTRAINT fk_conflict_record FOREIGN KEY (review_record_id)
        REFERENCES review_records (review_record_id) ON DELETE RESTRICT,
    CONSTRAINT uq_conflict_once UNIQUE (review_id, stage_id, round_number, review_record_id)
);

CREATE TABLE screening_adjudications (
    adjudication_id uuid PRIMARY KEY,
    conflict_id uuid NOT NULL,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    disposition text NOT NULL,
    exclusion_reason_code text,
    rationale text NOT NULL,
    adjudicator_subject_id text NOT NULL,
    supersedes_adjudication_id uuid,
    decided_at timestamptz NOT NULL DEFAULT now(),
    adjudicator_display_name text,
    CONSTRAINT fk_adjudication_conflict FOREIGN KEY (conflict_id)
        REFERENCES screening_conflicts (conflict_id) ON DELETE RESTRICT,
    CONSTRAINT fk_adjudication_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT fk_adjudication_supersedes FOREIGN KEY (supersedes_adjudication_id)
        REFERENCES screening_adjudications (adjudication_id) ON DELETE RESTRICT,
    CONSTRAINT ck_adjudication_disposition CHECK (disposition IN ('include', 'exclude', 'uncertain')),
    CONSTRAINT ck_adjudication_exclusion_has_reason CHECK (
        (disposition <> 'exclude') OR (btrim(coalesce(exclusion_reason_code, '')) <> '')
    ),
    CONSTRAINT ck_adjudication_rationale_not_blank CHECK (btrim(rationale) <> ''),
    CONSTRAINT ck_adjudication_adjudicator_not_blank CHECK (btrim(adjudicator_subject_id) <> '')
);

CREATE TABLE screening_unblinding_events (
    unblinding_event_id uuid PRIMARY KEY,
    conflict_id uuid NOT NULL,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    actor_subject_id text NOT NULL,
    reason text NOT NULL,
    occurred_at timestamptz NOT NULL DEFAULT now(),
    disclosed_to_subject_ids text[] NOT NULL,
    CONSTRAINT fk_unblinding_conflict FOREIGN KEY (conflict_id)
        REFERENCES screening_conflicts (conflict_id) ON DELETE RESTRICT,
    CONSTRAINT fk_unblinding_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT ck_unblinding_reason_not_blank CHECK (btrim(reason) <> ''),
    CONSTRAINT ck_unblinding_actor_not_blank CHECK (btrim(actor_subject_id) <> ''),
    CONSTRAINT ck_unblinding_audience_not_empty CHECK (cardinality(disclosed_to_subject_ids) > 0)
);

CREATE TABLE flow_revisions (
    flow_revision_id uuid PRIMARY KEY,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    revision_number integer NOT NULL,
    opened_at timestamptz NOT NULL DEFAULT now(),
    closed_at timestamptz,
    CONSTRAINT fk_flow_revision_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT uq_flow_revision_number UNIQUE (review_id, revision_number)
);

CREATE TABLE flow_projections (
    projection_id uuid PRIMARY KEY,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    flow_revision_id uuid NOT NULL,
    counts jsonb NOT NULL,
    canonical_counts_hash text NOT NULL,
    computed_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_flow_projection_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT fk_flow_projection_revision FOREIGN KEY (flow_revision_id)
        REFERENCES flow_revisions (flow_revision_id) ON DELETE RESTRICT
);

CREATE TABLE inclusion_exports (
    export_id uuid PRIMARY KEY,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    flow_revision_id uuid NOT NULL,
    corpus_version_id uuid NOT NULL,
    schema_ref text NOT NULL,
    manifest_hash text NOT NULL,
    payload_digest text NOT NULL,
    payload jsonb NOT NULL,
    omissions jsonb NOT NULL,
    created_by_subject_id text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    sealed_with text NOT NULL,
    CONSTRAINT fk_export_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT fk_export_flow_revision FOREIGN KEY (flow_revision_id)
        REFERENCES flow_revisions (flow_revision_id) ON DELETE RESTRICT,
    CONSTRAINT fk_export_corpus_version FOREIGN KEY (corpus_version_id)
        REFERENCES corpus_versions (corpus_version_id) ON DELETE RESTRICT,
    CONSTRAINT uq_export_manifest_per_review UNIQUE (review_id, manifest_hash),
    CONSTRAINT ck_export_omissions_is_array CHECK (jsonb_typeof(omissions) = 'array'),
    CONSTRAINT ck_export_manifest_hash_shape CHECK (manifest_hash ~ '^sha256:[0-9a-f]{64}$'),
    CONSTRAINT ck_export_payload_digest_shape CHECK (payload_digest ~ '^sha256:[0-9a-f]{64}$'),
    CONSTRAINT ck_export_hashes_are_distinct CHECK (manifest_hash <> payload_digest),
    CONSTRAINT ck_export_sealed_with_vocabulary CHECK (
        sealed_with IN ('content_hash', 'canonical_sha256', 'unknown')
    )
);

-- ---------------------------------------------------------------------------
-- 6. Research objects (revision 0017)
-- ---------------------------------------------------------------------------

CREATE TABLE research_objects (
    research_object_id uuid PRIMARY KEY,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    object_key text NOT NULL,
    created_by_subject_id text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_research_object_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT uq_research_object_key UNIQUE (review_id, object_key),
    CONSTRAINT uq_research_object_tenant UNIQUE (research_object_id, tenant_id),
    CONSTRAINT ck_research_object_key_not_blank CHECK (btrim(object_key) <> ''),
    CONSTRAINT ck_research_object_author CHECK (btrim(created_by_subject_id) <> '')
);

CREATE TABLE research_object_versions (
    research_object_version_id uuid PRIMARY KEY,
    research_object_id uuid NOT NULL,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    version_number integer NOT NULL,
    supersedes_version_id uuid,
    protocol_version_id uuid NOT NULL,
    schema_ref text NOT NULL,
    content_hash text NOT NULL,
    content jsonb NOT NULL,
    revision_kind text NOT NULL,
    rationale text NOT NULL,
    created_by_subject_id text NOT NULL,
    created_by_display_name text,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_research_object_version_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT fk_research_object_version_same_tenant FOREIGN KEY (research_object_id, tenant_id)
        REFERENCES research_objects (research_object_id, tenant_id) ON DELETE RESTRICT,
    CONSTRAINT fk_research_object_version_supersedes FOREIGN KEY (supersedes_version_id)
        REFERENCES research_object_versions (research_object_version_id) ON DELETE RESTRICT,
    CONSTRAINT fk_research_object_version_protocol FOREIGN KEY (protocol_version_id)
        REFERENCES review_protocol_versions (protocol_version_id) ON DELETE RESTRICT,
    CONSTRAINT uq_research_object_version_number UNIQUE (research_object_id, version_number),
    CONSTRAINT ck_research_object_version_positive CHECK (version_number >= 1),
    CONSTRAINT ck_research_object_version_hash_shape CHECK (content_hash ~ '^sha256:[0-9a-f]{64}$'),
    CONSTRAINT ck_research_object_revision_kind CHECK (
        revision_kind IN ('initial', 'correction', 'material_change')
    ),
    CONSTRAINT ck_research_object_version_rationale CHECK (btrim(rationale) <> ''),
    CONSTRAINT ck_research_object_version_chain CHECK (
        (version_number = 1 AND supersedes_version_id IS NULL AND revision_kind = 'initial')
        OR (version_number > 1 AND supersedes_version_id IS NOT NULL AND revision_kind <> 'initial')
    )
);

CREATE TABLE research_object_records (
    research_object_record_id uuid PRIMARY KEY,
    research_object_id uuid NOT NULL,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    review_record_id uuid NOT NULL,
    review_record_version_id uuid NOT NULL,
    corpus_version_id uuid NOT NULL,
    link_role text NOT NULL,
    rationale text NOT NULL,
    linked_by_subject_id text NOT NULL,
    linked_by_display_name text,
    linked_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_research_object_record_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT fk_research_object_record_same_tenant FOREIGN KEY (research_object_id, tenant_id)
        REFERENCES research_objects (research_object_id, tenant_id) ON DELETE RESTRICT,
    CONSTRAINT fk_research_object_record_record FOREIGN KEY (review_record_id)
        REFERENCES review_records (review_record_id) ON DELETE RESTRICT,
    CONSTRAINT fk_research_object_record_version FOREIGN KEY (review_record_version_id)
        REFERENCES review_record_versions (review_record_version_id) ON DELETE RESTRICT,
    CONSTRAINT fk_research_object_record_corpus FOREIGN KEY (corpus_version_id)
        REFERENCES corpus_versions (corpus_version_id) ON DELETE RESTRICT,
    CONSTRAINT ck_research_object_record_role CHECK (link_role IN ('primary', 'companion')),
    CONSTRAINT ck_research_object_record_rationale CHECK (btrim(rationale) <> ''),
    CONSTRAINT uq_research_object_record_unique_report UNIQUE (review_id, review_record_id)
);

-- ---------------------------------------------------------------------------
-- 7. Accepted evidence (revision 0018, altered by 0027)
-- ---------------------------------------------------------------------------

CREATE TABLE extraction_datasets (
    extraction_dataset_id uuid PRIMARY KEY,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    created_by_subject_id text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    research_object_id uuid NOT NULL,
    dataset_key text NOT NULL,
    CONSTRAINT fk_extraction_dataset_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT fk_extraction_dataset_object_same_tenant FOREIGN KEY (research_object_id, tenant_id)
        REFERENCES research_objects (research_object_id, tenant_id) ON DELETE RESTRICT,
    CONSTRAINT uq_extraction_dataset_tenant UNIQUE (extraction_dataset_id, tenant_id),
    CONSTRAINT uq_extraction_dataset_key UNIQUE (research_object_id, dataset_key),
    CONSTRAINT ck_extraction_dataset_author CHECK (btrim(created_by_subject_id) <> '')
);

CREATE TABLE extraction_dataset_versions (
    extraction_dataset_version_id uuid PRIMARY KEY,
    extraction_dataset_id uuid NOT NULL,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    version_number integer NOT NULL,
    supersedes_version_id uuid,
    protocol_version_id uuid NOT NULL,
    schema_ref text NOT NULL,
    content_hash text NOT NULL,
    extraction_method text NOT NULL,
    rationale text NOT NULL,
    created_by_subject_id text NOT NULL,
    created_by_display_name text,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_extraction_dataset_version_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT fk_extraction_dataset_version_same_tenant FOREIGN KEY (extraction_dataset_id, tenant_id)
        REFERENCES extraction_datasets (extraction_dataset_id, tenant_id) ON DELETE RESTRICT,
    CONSTRAINT fk_extraction_dataset_version_supersedes FOREIGN KEY (supersedes_version_id)
        REFERENCES extraction_dataset_versions (extraction_dataset_version_id) ON DELETE RESTRICT,
    CONSTRAINT fk_extraction_dataset_version_protocol FOREIGN KEY (protocol_version_id)
        REFERENCES review_protocol_versions (protocol_version_id) ON DELETE RESTRICT,
    CONSTRAINT uq_extraction_dataset_version_number UNIQUE (extraction_dataset_id, version_number),
    CONSTRAINT ck_extraction_dataset_version_positive CHECK (version_number >= 1),
    CONSTRAINT ck_extraction_dataset_hash_shape CHECK (content_hash ~ '^sha256:[0-9a-f]{64}$'),
    CONSTRAINT ck_extraction_method_known CHECK (extraction_method IN ('manual')),
    CONSTRAINT ck_extraction_dataset_rationale CHECK (btrim(rationale) <> ''),
    CONSTRAINT ck_extraction_dataset_version_chain CHECK (
        (version_number = 1 AND supersedes_version_id IS NULL)
        OR (version_number > 1 AND supersedes_version_id IS NOT NULL)
    )
);

CREATE TABLE observations (
    observation_id uuid PRIMARY KEY,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    created_by_subject_id text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    extraction_dataset_id uuid NOT NULL,
    variable_key text NOT NULL,
    CONSTRAINT fk_observation_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT fk_observation_dataset_same_tenant FOREIGN KEY (extraction_dataset_id, tenant_id)
        REFERENCES extraction_datasets (extraction_dataset_id, tenant_id) ON DELETE RESTRICT,
    CONSTRAINT uq_observation_tenant UNIQUE (observation_id, tenant_id),
    CONSTRAINT uq_observation_variable UNIQUE (extraction_dataset_id, variable_key),
    CONSTRAINT ck_observation_author CHECK (btrim(created_by_subject_id) <> '')
);

CREATE TABLE observation_versions (
    observation_version_id uuid PRIMARY KEY,
    observation_id uuid NOT NULL,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    version_number integer NOT NULL,
    supersedes_version_id uuid,
    protocol_version_id uuid NOT NULL,
    extraction_dataset_version_id uuid NOT NULL,
    schema_ref text NOT NULL,
    content_hash text NOT NULL,
    value jsonb NOT NULL,
    unit text,
    basis_kind text NOT NULL,
    document_version_ref jsonb,
    evidence_location jsonb,
    expert_subject_id text,
    expert_role text,
    expert_rationale text,
    expert_decision_ref uuid,
    created_by_subject_id text NOT NULL,
    created_by_display_name text,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_observation_version_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT fk_observation_version_same_tenant FOREIGN KEY (observation_id, tenant_id)
        REFERENCES observations (observation_id, tenant_id) ON DELETE RESTRICT,
    CONSTRAINT fk_observation_version_supersedes FOREIGN KEY (supersedes_version_id)
        REFERENCES observation_versions (observation_version_id) ON DELETE RESTRICT,
    CONSTRAINT fk_observation_version_dataset FOREIGN KEY (extraction_dataset_version_id)
        REFERENCES extraction_dataset_versions (extraction_dataset_version_id) ON DELETE RESTRICT,
    CONSTRAINT fk_observation_version_protocol FOREIGN KEY (protocol_version_id)
        REFERENCES review_protocol_versions (protocol_version_id) ON DELETE RESTRICT,
    CONSTRAINT uq_observation_version_number UNIQUE (observation_id, version_number),
    CONSTRAINT ck_observation_version_positive CHECK (version_number >= 1),
    CONSTRAINT ck_observation_hash_shape CHECK (content_hash ~ '^sha256:[0-9a-f]{64}$'),
    CONSTRAINT ck_observation_basis_known CHECK (
        basis_kind IN ('evidence_located', 'expert_assertion')
    ),
    CONSTRAINT ck_observation_version_chain CHECK (
        (version_number = 1 AND supersedes_version_id IS NULL)
        OR (version_number > 1 AND supersedes_version_id IS NOT NULL)
    ),
    CONSTRAINT ck_observation_basis_exclusive CHECK (
        (basis_kind = 'evidence_located'
            AND document_version_ref IS NOT NULL AND evidence_location IS NOT NULL
            AND expert_subject_id IS NULL AND expert_role IS NULL
            AND expert_rationale IS NULL AND expert_decision_ref IS NULL)
        OR (basis_kind = 'expert_assertion'
            AND document_version_ref IS NULL AND evidence_location IS NULL
            AND btrim(expert_subject_id) <> '' AND btrim(expert_role) <> ''
            AND btrim(expert_rationale) <> '' AND expert_decision_ref IS NOT NULL)
    ),
    CONSTRAINT ck_observation_located_arm_unverifiable CHECK (basis_kind = 'expert_assertion')
);

CREATE TABLE research_assertions (
    research_assertion_id uuid PRIMARY KEY,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    created_by_subject_id text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    assertion_key text NOT NULL,
    CONSTRAINT fk_research_assertion_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT uq_research_assertion_tenant UNIQUE (research_assertion_id, tenant_id),
    CONSTRAINT uq_research_assertion_key UNIQUE (review_id, assertion_key),
    CONSTRAINT ck_research_assertion_author CHECK (btrim(created_by_subject_id) <> ''),
    CONSTRAINT ck_research_assertions_assertion_key_shape CHECK (
        assertion_key ~ '^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$'
        AND length(assertion_key) <= 128
    )
);

CREATE TABLE research_assertion_versions (
    research_assertion_version_id uuid PRIMARY KEY,
    research_assertion_id uuid NOT NULL,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    version_number integer NOT NULL,
    supersedes_version_id uuid,
    protocol_version_id uuid NOT NULL,
    schema_ref text NOT NULL,
    content_hash text NOT NULL,
    statement text NOT NULL,
    content jsonb NOT NULL,
    created_by_subject_id text NOT NULL,
    created_by_display_name text,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_research_assertion_version_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT fk_research_assertion_version_same_tenant FOREIGN KEY (research_assertion_id, tenant_id)
        REFERENCES research_assertions (research_assertion_id, tenant_id) ON DELETE RESTRICT,
    CONSTRAINT fk_research_assertion_version_supersedes FOREIGN KEY (supersedes_version_id)
        REFERENCES research_assertion_versions (research_assertion_version_id) ON DELETE RESTRICT,
    CONSTRAINT fk_research_assertion_version_protocol FOREIGN KEY (protocol_version_id)
        REFERENCES review_protocol_versions (protocol_version_id) ON DELETE RESTRICT,
    CONSTRAINT uq_research_assertion_version_number UNIQUE (research_assertion_id, version_number),
    CONSTRAINT ck_research_assertion_version_positive CHECK (version_number >= 1),
    CONSTRAINT ck_research_assertion_hash_shape CHECK (content_hash ~ '^sha256:[0-9a-f]{64}$'),
    CONSTRAINT ck_research_assertion_statement CHECK (btrim(statement) <> ''),
    CONSTRAINT ck_research_assertion_version_chain CHECK (
        (version_number = 1 AND supersedes_version_id IS NULL)
        OR (version_number > 1 AND supersedes_version_id IS NOT NULL)
    )
);

CREATE TABLE research_assertion_support (
    support_id uuid PRIMARY KEY,
    research_assertion_version_id uuid NOT NULL,
    observation_version_id uuid NOT NULL,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    stance text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_assertion_support_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT fk_assertion_support_assertion FOREIGN KEY (research_assertion_version_id)
        REFERENCES research_assertion_versions (research_assertion_version_id) ON DELETE RESTRICT,
    CONSTRAINT fk_assertion_support_observation FOREIGN KEY (observation_version_id)
        REFERENCES observation_versions (observation_version_id) ON DELETE RESTRICT,
    CONSTRAINT ck_assertion_support_stance CHECK (stance IN ('supports', 'contradicts')),
    CONSTRAINT uq_assertion_support_once UNIQUE (research_assertion_version_id, observation_version_id)
);

CREATE TABLE evidence_acceptances (
    acceptance_id uuid PRIMARY KEY,
    review_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    subject_kind text NOT NULL,
    research_object_version_id uuid,
    extraction_dataset_version_id uuid,
    observation_version_id uuid,
    subject_version_id uuid GENERATED ALWAYS AS (
        COALESCE(research_object_version_id, extraction_dataset_version_id, observation_version_id)
    ) STORED,
    subject_content_hash text NOT NULL,
    protocol_version_id uuid NOT NULL,
    decision text NOT NULL,
    rationale text NOT NULL,
    supersedes_acceptance_id uuid,
    decided_by_subject_id text NOT NULL,
    decided_by_display_name text,
    decided_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_acceptance_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT fk_acceptance_research_object_version FOREIGN KEY (research_object_version_id)
        REFERENCES research_object_versions (research_object_version_id) ON DELETE RESTRICT,
    CONSTRAINT fk_acceptance_extraction_dataset_version FOREIGN KEY (extraction_dataset_version_id)
        REFERENCES extraction_dataset_versions (extraction_dataset_version_id) ON DELETE RESTRICT,
    CONSTRAINT fk_acceptance_observation_version FOREIGN KEY (observation_version_id)
        REFERENCES observation_versions (observation_version_id) ON DELETE RESTRICT,
    CONSTRAINT fk_acceptance_supersedes FOREIGN KEY (supersedes_acceptance_id)
        REFERENCES evidence_acceptances (acceptance_id) ON DELETE RESTRICT,
    CONSTRAINT fk_acceptance_protocol FOREIGN KEY (protocol_version_id)
        REFERENCES review_protocol_versions (protocol_version_id) ON DELETE RESTRICT,
    CONSTRAINT ck_acceptance_subject_kind_known CHECK (subject_kind IN (
        'research_object_version', 'extraction_dataset_version', 'observation_version'
    )),
    CONSTRAINT ck_acceptance_decision_known CHECK (decision IN ('accept', 'withdraw')),
    CONSTRAINT ck_acceptance_rationale CHECK (btrim(rationale) <> ''),
    CONSTRAINT ck_acceptance_author CHECK (btrim(decided_by_subject_id) <> ''),
    CONSTRAINT ck_acceptance_subject_hash_shape CHECK (subject_content_hash ~ '^sha256:[0-9a-f]{64}$'),
    CONSTRAINT ck_acceptance_exactly_one_subject CHECK (
        num_nonnulls(research_object_version_id, extraction_dataset_version_id, observation_version_id) = 1
    ),
    CONSTRAINT ck_acceptance_kind_matches_subject CHECK (
        (subject_kind = 'research_object_version' AND research_object_version_id IS NOT NULL)
        OR (subject_kind = 'extraction_dataset_version' AND extraction_dataset_version_id IS NOT NULL)
        OR (subject_kind = 'observation_version' AND observation_version_id IS NOT NULL)
    ),
    CONSTRAINT ck_acceptance_withdrawal_cites_what_it_withdraws CHECK (
        decision <> 'withdraw' OR supersedes_acceptance_id IS NOT NULL
    )
);

-- ---------------------------------------------------------------------------
-- 8. Domain outbox (revision 0019)
-- ---------------------------------------------------------------------------

CREATE TABLE domain_outbox (
    event_id uuid PRIMARY KEY,
    event_type text NOT NULL,
    schema_ref text NOT NULL,
    producer text NOT NULL,
    resource_scope text NOT NULL,
    tenant_id uuid NOT NULL,
    aggregate_type text NOT NULL,
    aggregate_id uuid NOT NULL,
    aggregate_version integer NOT NULL,
    occurred_at timestamptz NOT NULL DEFAULT now(),
    payload jsonb NOT NULL DEFAULT '{}'::jsonb,
    correlation_id uuid,
    causation_id uuid,
    content_hash text,
    CONSTRAINT ck_domain_outbox_registered_event CHECK (event_type IN (
        'review.created.v1', 'review.activated.v1', 'review.paused.v1', 'review.resumed.v1',
        'review.reopened.v1', 'review.archived.v1', 'review.import.completed.v1',
        'review.duplicate_cluster.proposed.v1', 'review.duplicate_cluster.resolved.v1',
        'review.decision.recorded.v1', 'review.conflict.opened.v1', 'review.conflict.resolved.v1',
        'review.research_object.approved.v1', 'review.proposal.decided.v1',
        'review.observation.accepted.v1', 'review.observation.superseded.v1',
        'review.assertion.recorded.v1', 'review.appraisal.recorded.v1',
        'review.evidence_snapshot.created.v1', 'review.stage.completed.v1', 'review.closed.v1'
    )),
    CONSTRAINT ck_domain_outbox_event_name_shape CHECK (
        event_type ~ '^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+\.v[1-9][0-9]*$'
    ),
    CONSTRAINT ck_domain_outbox_event_name_not_regulatory CHECK (
        event_type !~ 'regulatory|legal_|normative|consolidation'
    ),
    CONSTRAINT ck_domain_outbox_envelope_pinned CHECK (
        schema_ref = 'urn:ahtapol:research-event-envelope:1'
    ),
    CONSTRAINT ck_domain_outbox_producer CHECK (producer = 'evidence-review'),
    CONSTRAINT ck_domain_outbox_scope CHECK (resource_scope IN ('global', 'tenant')),
    CONSTRAINT ck_domain_outbox_aggregate_version CHECK (aggregate_version >= 1),
    CONSTRAINT ck_domain_outbox_aggregate_type CHECK (aggregate_type ~ '^[a-z][a-z0-9_]*$'),
    CONSTRAINT ck_domain_outbox_payload_is_minimal CHECK (
        jsonb_typeof(payload) = 'object' AND er_payload_is_minimal(payload)
    ),
    CONSTRAINT ck_domain_outbox_content_hash_shape CHECK (
        content_hash IS NULL OR content_hash ~ '^sha256:[0-9a-f]{64}$'
    )
);

-- ---------------------------------------------------------------------------
-- 9. Durable jobs (revision 0020)
-- ---------------------------------------------------------------------------

CREATE TABLE durable_jobs (
    job_id uuid PRIMARY KEY,
    tenant_id uuid NOT NULL,
    job_type text NOT NULL,
    review_id uuid NOT NULL,
    state text NOT NULL DEFAULT 'queued',
    command_hash text NOT NULL,
    input_snapshot_hash text,
    idempotency_key text,
    profile_version text,
    capability_version text,
    pack_refs jsonb NOT NULL DEFAULT '[]'::jsonb,
    correlation_id uuid,
    causation_id uuid,
    workflow_ref text,
    originating_subject_id text,
    originating_actor_role text,
    attempt integer NOT NULL DEFAULT 0,
    max_attempts integer NOT NULL DEFAULT 3,
    worker_id text,
    lease_expires_at timestamptz,
    heartbeat_at timestamptz,
    cancel_requested_at timestamptz,
    cancel_requested_by text,
    waiting_reason text,
    waiting_dependency_ref text,
    reconcile_after timestamptz,
    callback_state text,
    progress jsonb NOT NULL DEFAULT '{}'::jsonb,
    result_summary jsonb,
    result_artifact_ref text,
    run_manifest_ref text,
    error_code text,
    error_detail text,
    queued_at timestamptz NOT NULL DEFAULT now(),
    started_at timestamptz,
    terminal_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_durable_job_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id) ON DELETE RESTRICT,
    CONSTRAINT ck_durable_jobs_state CHECK (
        state IN ('queued', 'running', 'waiting', 'succeeded', 'failed', 'cancelled')
    ),
    CONSTRAINT ck_durable_jobs_registered_type CHECK (
        job_type IN ('import', 'deduplication', 'prioritization', 'evidence_snapshot', 'export')
    ),
    CONSTRAINT ck_durable_jobs_type_shape CHECK (job_type ~ '^[a-z][a-z0-9_]*$'),
    CONSTRAINT ck_durable_jobs_type_not_regulatory CHECK (
        job_type !~ 'regulatory|legal_|normative|consolidation'
    ),
    CONSTRAINT ck_durable_jobs_terminal_at CHECK (
        (state IN ('succeeded', 'failed', 'cancelled')) = (terminal_at IS NOT NULL)
    ),
    CONSTRAINT ck_durable_jobs_running_holds_lease CHECK (
        (state = 'running') = (worker_id IS NOT NULL AND lease_expires_at IS NOT NULL)
    ),
    CONSTRAINT ck_durable_jobs_waiting_declares_why CHECK (
        (state = 'waiting') = (waiting_reason IS NOT NULL AND reconcile_after IS NOT NULL)
    ),
    CONSTRAINT ck_durable_jobs_failure_names_itself CHECK (
        (state = 'failed') = (error_code IS NOT NULL)
    ),
    CONSTRAINT ck_durable_jobs_attempt_within_policy CHECK (
        max_attempts >= 1 AND attempt >= 0 AND attempt <= max_attempts
    ),
    CONSTRAINT ck_durable_jobs_worker_id_shape CHECK (
        worker_id IS NULL OR worker_id ~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,126}$'
    ),
    CONSTRAINT ck_durable_jobs_command_hash_shape CHECK (command_hash ~ '^sha256:[0-9a-f]{64}$'),
    CONSTRAINT ck_durable_jobs_input_hash_shape CHECK (
        input_snapshot_hash IS NULL OR input_snapshot_hash ~ '^sha256:[0-9a-f]{64}$'
    ),
    CONSTRAINT ck_durable_jobs_progress_is_minimal CHECK (
        jsonb_typeof(progress) = 'object' AND er_payload_is_minimal(progress)
    ),
    CONSTRAINT ck_durable_jobs_result_is_minimal CHECK (
        result_summary IS NULL
        OR (jsonb_typeof(result_summary) = 'object' AND er_payload_is_minimal(result_summary))
    ),
    CONSTRAINT ck_durable_jobs_pack_refs_is_minimal CHECK (
        jsonb_typeof(pack_refs) = 'array' AND er_payload_is_minimal(pack_refs)
    ),
    CONSTRAINT ck_durable_jobs_error_detail_is_bounded CHECK (
        error_detail IS NULL OR length(error_detail) <= 512
    ),
    CONSTRAINT ck_durable_jobs_cancel_names_its_author CHECK (
        (cancel_requested_at IS NULL) = (cancel_requested_by IS NULL)
    )
);

CREATE TABLE job_attempts (
    attempt_id uuid PRIMARY KEY,
    job_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    attempt_number integer NOT NULL,
    worker_id text NOT NULL,
    capability_version text,
    started_at timestamptz NOT NULL DEFAULT now(),
    ended_at timestamptz,
    outcome text,
    error_code text,
    error_detail text,
    resource_summary jsonb,
    CONSTRAINT fk_job_attempt_job FOREIGN KEY (job_id)
        REFERENCES durable_jobs (job_id) ON DELETE RESTRICT,
    CONSTRAINT uq_job_attempts_ordinal UNIQUE (job_id, attempt_number),
    CONSTRAINT ck_job_attempts_ordinal_positive CHECK (attempt_number >= 1),
    CONSTRAINT ck_job_attempts_outcome CHECK (
        outcome IS NULL OR outcome IN ('succeeded', 'failed', 'cancelled', 'lease_expired')
    ),
    CONSTRAINT ck_job_attempts_ended_has_outcome CHECK ((ended_at IS NULL) = (outcome IS NULL)),
    CONSTRAINT ck_job_attempts_failure_names_itself CHECK (
        (outcome = 'failed') = (error_code IS NOT NULL)
    ),
    CONSTRAINT ck_job_attempts_worker_id_shape CHECK (
        worker_id ~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,126}$'
    ),
    CONSTRAINT ck_job_attempts_error_detail_is_bounded CHECK (
        error_detail IS NULL OR length(error_detail) <= 512
    ),
    CONSTRAINT ck_job_attempts_resource_summary_is_minimal CHECK (
        resource_summary IS NULL
        OR (jsonb_typeof(resource_summary) = 'object' AND er_payload_is_minimal(resource_summary))
    )
);

-- ---------------------------------------------------------------------------
-- 10. Evidence snapshots (revision 0021)
-- ---------------------------------------------------------------------------

CREATE TABLE evidence_snapshots (
    snapshot_id uuid PRIMARY KEY,
    tenant_id uuid NOT NULL,
    review_id uuid NOT NULL,
    version_number integer NOT NULL,
    supersedes_snapshot_id uuid,
    schema_ref text NOT NULL,
    intended_use text NOT NULL,
    protocol_version_id uuid NOT NULL,
    corpus_version_id uuid,
    member_count integer NOT NULL,
    member_manifest_hash text NOT NULL,
    member_ordering_rule text NOT NULL,
    content_hash text NOT NULL,
    gates_evaluated text[] NOT NULL DEFAULT '{}'::text[],
    job_id uuid NOT NULL,
    created_by_subject_id text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_evidence_snapshot_review FOREIGN KEY (review_id)
        REFERENCES reviews (review_id),
    CONSTRAINT fk_evidence_snapshot_supersedes FOREIGN KEY (supersedes_snapshot_id)
        REFERENCES evidence_snapshots (snapshot_id),
    CONSTRAINT fk_evidence_snapshot_protocol FOREIGN KEY (protocol_version_id)
        REFERENCES review_protocol_versions (protocol_version_id),
    CONSTRAINT fk_evidence_snapshot_corpus FOREIGN KEY (corpus_version_id)
        REFERENCES corpus_versions (corpus_version_id),
    CONSTRAINT fk_evidence_snapshot_job FOREIGN KEY (job_id)
        REFERENCES durable_jobs (job_id),
    CONSTRAINT uq_snapshot_version_per_review UNIQUE (review_id, version_number),
    CONSTRAINT ck_evidence_snapshots_manifest_pinned CHECK (
        schema_ref = 'urn:ahtapol:evidence-snapshot-manifest:1'
    ),
    CONSTRAINT ck_evidence_snapshots_version_positive CHECK (version_number >= 1),
    CONSTRAINT ck_evidence_snapshots_first_version_supersedes_nothing CHECK (
        (version_number = 1) = (supersedes_snapshot_id IS NULL)
    ),
    CONSTRAINT ck_evidence_snapshots_intended_use_shape CHECK (intended_use ~ '^[a-z][a-z0-9_]*$'),
    CONSTRAINT ck_evidence_snapshots_intended_use_not_regulatory CHECK (
        intended_use !~ 'regulatory|legal_|normative|consolidation'
    ),
    CONSTRAINT ck_evidence_snapshots_member_count CHECK (member_count >= 0),
    CONSTRAINT ck_evidence_snapshots_member_hash_shape CHECK (
        member_manifest_hash ~ '^sha256:[0-9a-f]{64}$'
    ),
    CONSTRAINT ck_evidence_snapshots_content_hash_shape CHECK (content_hash ~ '^sha256:[0-9a-f]{64}$'),
    CONSTRAINT ck_evidence_snapshots_ordering_rule_shape CHECK (
        member_ordering_rule ~ '^[a-z][a-z0-9_]*$'
    ),
    CONSTRAINT ck_evidence_snapshots_gate_ledger_is_honest CHECK (gates_evaluated <@ ARRAY[
        'unresolved_conflicts', 'uncommitted_protocol', 'unresolved_critical_proposals',
        'denied_rights', 'stale_mandatory_evidence'
    ]::text[]),
    CONSTRAINT ck_evidence_snapshots_gate_ledger_has_no_repeats CHECK (
        er_text_array_is_a_set(gates_evaluated)
    )
);

CREATE TABLE evidence_snapshot_members (
    member_id uuid PRIMARY KEY,
    tenant_id uuid NOT NULL,
    snapshot_id uuid NOT NULL,
    owner_context text NOT NULL,
    entity_type text NOT NULL,
    entity_id uuid NOT NULL,
    version_id uuid NOT NULL,
    version_number integer NOT NULL,
    member_schema_ref text NOT NULL,
    content_hash text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_snapshot_member_snapshot FOREIGN KEY (snapshot_id)
        REFERENCES evidence_snapshots (snapshot_id),
    CONSTRAINT ck_snapshot_member_owner_shape CHECK (owner_context ~ '^[a-z][a-z0-9-]*$'),
    CONSTRAINT ck_snapshot_member_entity_type CHECK (entity_type IN (
        'research_object_version', 'extraction_dataset_version', 'observation_version',
        'review_record_version'
    )),
    CONSTRAINT ck_snapshot_member_version_positive CHECK (version_number >= 1),
    CONSTRAINT ck_snapshot_member_content_hash_shape CHECK (content_hash ~ '^sha256:[0-9a-f]{64}$'),
    CONSTRAINT uq_snapshot_member_identity UNIQUE (snapshot_id, owner_context, entity_type, entity_id)
);

CREATE TABLE snapshot_artifacts (
    artifact_id uuid PRIMARY KEY,
    tenant_id uuid NOT NULL,
    snapshot_id uuid NOT NULL,
    artifact_kind text NOT NULL,
    media_type text NOT NULL,
    payload_digest text NOT NULL,
    byte_size bigint NOT NULL,
    serializer_version text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_snapshot_artifact_snapshot FOREIGN KEY (snapshot_id)
        REFERENCES evidence_snapshots (snapshot_id),
    CONSTRAINT ck_snapshot_artifact_kind_shape CHECK (artifact_kind ~ '^[a-z][a-z0-9_]*$'),
    CONSTRAINT ck_snapshot_artifact_media_type_shape CHECK (media_type ~ '^[a-z]+/[a-zA-Z0-9.+_-]+$'),
    CONSTRAINT ck_snapshot_artifact_digest_shape CHECK (payload_digest ~ '^sha256:[0-9a-f]{64}$'),
    CONSTRAINT ck_snapshot_artifact_byte_size CHECK (byte_size >= 0),
    CONSTRAINT uq_snapshot_artifact_kind UNIQUE (snapshot_id, artifact_kind)
);

-- ---------------------------------------------------------------------------
-- 11. Research event path (revision 0022, altered by 0024)
-- ---------------------------------------------------------------------------

CREATE TABLE research_domain_events (
    event_id uuid PRIMARY KEY,
    log_position bigint GENERATED ALWAYS AS IDENTITY NOT NULL UNIQUE,
    event_type text NOT NULL,
    schema_ref text NOT NULL,
    producer text NOT NULL,
    resource_scope text NOT NULL,
    tenant_id uuid NOT NULL,
    aggregate_type text NOT NULL,
    aggregate_id uuid NOT NULL,
    aggregate_version integer NOT NULL,
    occurred_at timestamptz NOT NULL,
    payload jsonb NOT NULL,
    correlation_id uuid,
    causation_id uuid,
    content_hash text,
    relayed_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_rde_event_name_shape CHECK (
        event_type ~ '^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+\.v[1-9][0-9]*$'
    ),
    CONSTRAINT ck_rde_event_name_not_regulatory CHECK (
        event_type !~ 'regulatory|legal_|normative|consolidation'
    ),
    CONSTRAINT ck_rde_scope CHECK (resource_scope IN ('global', 'tenant')),
    CONSTRAINT ck_rde_aggregate_version CHECK (aggregate_version >= 1),
    CONSTRAINT ck_rde_aggregate_type CHECK (aggregate_type ~ '^[a-z][a-z0-9_]*$'),
    CONSTRAINT ck_rde_payload_is_minimal CHECK (
        jsonb_typeof(payload) = 'object' AND er_payload_is_minimal(payload)
    ),
    CONSTRAINT ck_rde_content_hash_shape CHECK (
        content_hash IS NULL OR content_hash ~ '^sha256:[0-9a-f]{64}$'
    ),
    CONSTRAINT ck_rde_registered_event CHECK (event_type IN (
        'review.created.v1', 'review.activated.v1', 'review.paused.v1', 'review.resumed.v1',
        'review.reopened.v1', 'review.archived.v1', 'review.import.completed.v1',
        'review.duplicate_cluster.proposed.v1', 'review.duplicate_cluster.resolved.v1',
        'review.decision.recorded.v1', 'review.conflict.opened.v1', 'review.conflict.resolved.v1',
        'review.research_object.approved.v1', 'review.proposal.decided.v1',
        'review.observation.accepted.v1', 'review.observation.superseded.v1',
        'review.assertion.recorded.v1', 'review.appraisal.recorded.v1',
        'review.evidence_snapshot.created.v1', 'review.stage.completed.v1', 'review.closed.v1'
    ))
);

CREATE TABLE event_deliveries (
    delivery_id uuid PRIMARY KEY,
    event_id uuid NOT NULL,
    tenant_id uuid NOT NULL,
    consumer text NOT NULL,
    state text NOT NULL DEFAULT 'pending',
    claimed_by text,
    lease_expires_at timestamptz,
    attempt integer NOT NULL DEFAULT 0,
    max_attempts integer NOT NULL DEFAULT 5,
    defer_reason text,
    retry_after timestamptz,
    poison_reason text,
    poison_detail text,
    acknowledged_at timestamptz,
    replay_of_delivery_id uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_event_delivery_event FOREIGN KEY (event_id)
        REFERENCES research_domain_events (event_id),
    CONSTRAINT ck_event_deliveries_state CHECK (
        state IN ('pending', 'claimed', 'deferred', 'acknowledged', 'poisoned')
    ),
    CONSTRAINT ck_event_deliveries_consumer_shape CHECK (consumer ~ '^[a-z][a-z0-9_]*$'),
    CONSTRAINT ck_event_deliveries_attempt_within_policy CHECK (
        max_attempts >= 1 AND attempt >= 0 AND attempt <= max_attempts
    ),
    CONSTRAINT ck_event_deliveries_claimed_holds_lease CHECK (
        (state = 'claimed') = (claimed_by IS NOT NULL AND lease_expires_at IS NOT NULL)
    ),
    CONSTRAINT ck_event_deliveries_deferral_declares_why CHECK (
        (state = 'deferred') = (defer_reason IS NOT NULL AND retry_after IS NOT NULL)
    ),
    CONSTRAINT ck_event_deliveries_poison_names_itself CHECK (
        (state = 'poisoned') = (poison_reason IS NOT NULL)
    ),
    CONSTRAINT ck_event_deliveries_poison_reason_registered CHECK (
        poison_reason IS NULL OR poison_reason IN (
            'incompatible_schema', 'impossible_transition',
            'handler_failed_repeatedly', 'unknown_consumer'
        )
    ),
    CONSTRAINT ck_event_deliveries_poison_detail_bounded CHECK (
        poison_detail IS NULL OR length(poison_detail) <= 512
    ),
    CONSTRAINT ck_event_deliveries_ack_is_stamped CHECK (
        (state = 'acknowledged') = (acknowledged_at IS NOT NULL)
    ),
    CONSTRAINT ck_event_deliveries_terminal_holds_no_lease CHECK (
        state NOT IN ('acknowledged', 'poisoned') OR lease_expires_at IS NULL
    )
);

CREATE TABLE domain_inbox (
    receipt_id uuid PRIMARY KEY,
    tenant_id uuid NOT NULL,
    consumer text NOT NULL,
    event_id uuid NOT NULL,
    aggregate_type text NOT NULL,
    aggregate_id uuid NOT NULL,
    aggregate_version integer NOT NULL,
    outcome text NOT NULL,
    detail text,
    recorded_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT uq_domain_inbox_consumer_event UNIQUE (consumer, event_id),
    CONSTRAINT ck_domain_inbox_consumer_shape CHECK (consumer ~ '^[a-z][a-z0-9_]*$'),
    CONSTRAINT ck_domain_inbox_outcome CHECK (
        outcome IN ('applied', 'skipped_duplicate', 'skipped_not_applicable')
    ),
    CONSTRAINT ck_domain_inbox_aggregate_version CHECK (aggregate_version >= 1),
    CONSTRAINT ck_domain_inbox_detail_bounded CHECK (detail IS NULL OR length(detail) <= 512)
);

CREATE TABLE projection_checkpoints (
    checkpoint_id uuid PRIMARY KEY,
    tenant_id uuid NOT NULL,
    consumer text NOT NULL,
    index_version integer NOT NULL DEFAULT 1,
    event_position bigint NOT NULL DEFAULT 0,
    lag_events integer NOT NULL DEFAULT 0,
    repair_state text NOT NULL DEFAULT 'healthy',
    repair_detail text,
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT uq_projection_checkpoint UNIQUE (consumer, tenant_id, index_version),
    CONSTRAINT ck_projection_checkpoints_consumer_shape CHECK (consumer ~ '^[a-z][a-z0-9_]*$'),
    CONSTRAINT ck_projection_checkpoints_index_version CHECK (index_version >= 1),
    CONSTRAINT ck_projection_checkpoints_position CHECK (event_position >= 0),
    CONSTRAINT ck_projection_checkpoints_lag CHECK (lag_events >= 0),
    CONSTRAINT ck_projection_checkpoints_repair_state CHECK (
        repair_state IN ('healthy', 'repairing', 'stalled')
    ),
    CONSTRAINT ck_projection_checkpoints_detail_bounded CHECK (
        repair_detail IS NULL OR length(repair_detail) <= 512
    )
);

-- ---------------------------------------------------------------------------
-- 12. Artifact records (revision 0025, corrected by 0026)
-- ---------------------------------------------------------------------------

CREATE TABLE artifact_records (
    artifact_record_id uuid PRIMARY KEY,
    tenant_id uuid NOT NULL,
    owning_aggregate_type text NOT NULL,
    owning_aggregate_id uuid NOT NULL,
    artifact_id text NOT NULL,
    artifact_version text NOT NULL,
    artifact_content_hash text NOT NULL,
    classification text NOT NULL,
    semantic_role text NOT NULL,
    rights_source_id text NOT NULL,
    rights_document_id text NOT NULL,
    rights_document_version_id text NOT NULL,
    rights_decision_id text NOT NULL,
    rights_evaluated_purpose text NOT NULL,
    rights_decision_version text NOT NULL,
    rights_decision_hash text NOT NULL,
    rights_evaluated_at timestamptz NOT NULL,
    rights_expiry_at timestamptz,
    rights_territory_constraint text,
    rights_checks_evaluated text[] NOT NULL DEFAULT '{}'::text[],
    residency_policy_ref text NOT NULL,
    created_by_subject_id text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ck_artifact_records_aggregate_type_shape CHECK (
        owning_aggregate_type ~ '^[a-z][a-z0-9_]*$'
    ),
    CONSTRAINT ck_artifact_records_artifact_id_is_a_uuid CHECK (
        artifact_id ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
    ),
    CONSTRAINT ck_artifact_records_artifact_version_is_a_positive_integer CHECK (
        artifact_version ~ '^[1-9][0-9]*$'
    ),
    CONSTRAINT ck_artifact_records_artifact_hash_shape CHECK (
        artifact_content_hash ~ '^sha256:[0-9a-f]{64}$'
    ),
    CONSTRAINT ck_artifact_records_classification_shape CHECK (classification ~ '^[a-z][a-z0-9_]*$'),
    CONSTRAINT ck_artifact_records_classification_not_regulatory CHECK (
        classification !~ 'regulatory|legal_|normative|consolidation'
    ),
    CONSTRAINT ck_artifact_records_semantic_role_shape CHECK (semantic_role ~ '^[a-z][a-z0-9_]*$'),
    CONSTRAINT ck_artifact_records_rights_source_id_not_blank CHECK (length(trim(rights_source_id)) > 0),
    CONSTRAINT ck_artifact_records_rights_document_id_not_blank CHECK (length(trim(rights_document_id)) > 0),
    CONSTRAINT ck_artifact_records_rights_document_version_id_not_blank CHECK (
        length(trim(rights_document_version_id)) > 0
    ),
    CONSTRAINT ck_artifact_records_rights_decision_id_not_blank CHECK (length(trim(rights_decision_id)) > 0),
    CONSTRAINT ck_artifact_records_rights_purpose_shape CHECK (
        rights_evaluated_purpose ~ '^[a-z][a-z0-9_]*$'
    ),
    CONSTRAINT ck_artifact_records_rights_purpose_not_regulatory CHECK (
        rights_evaluated_purpose !~ 'regulatory|legal_|normative|consolidation'
    ),
    CONSTRAINT ck_artifact_records_rights_decision_version_not_blank CHECK (
        length(trim(rights_decision_version)) > 0
    ),
    CONSTRAINT ck_artifact_records_rights_decision_hash_shape CHECK (
        rights_decision_hash ~ '^sha256:[0-9a-f]{64}$'
    ),
    CONSTRAINT ck_artifact_records_rights_expiry_after_evaluation CHECK (
        rights_expiry_at IS NULL OR rights_expiry_at > rights_evaluated_at
    ),
    CONSTRAINT ck_artifact_records_rights_territory_not_blank CHECK (
        rights_territory_constraint IS NULL OR length(trim(rights_territory_constraint)) > 0
    ),
    CONSTRAINT ck_artifact_records_rights_checks_are_registered CHECK (rights_checks_evaluated <@ ARRAY[
        'display', 'model_provider_use', 'translation', 'quotation', 'export',
        'redistribution', 'publication'
    ]::text[]),
    CONSTRAINT ck_artifact_records_rights_checks_have_no_repeats CHECK (
        er_text_array_is_a_set(rights_checks_evaluated)
    ),
    CONSTRAINT ck_artifact_records_residency_policy_ref_not_blank CHECK (
        length(trim(residency_policy_ref)) > 0
    )
);

-- ---------------------------------------------------------------------------
-- 13. Indexes
-- ---------------------------------------------------------------------------

CREATE INDEX ix_review_state_history_review ON review_state_history (review_id, revision_number);
CREATE INDEX ix_review_protocol_versions_review ON review_protocol_versions (review_id);
CREATE INDEX ix_review_stages_review ON review_stages (review_id, stage_order);

CREATE INDEX ix_record_versions_doi ON review_record_versions (review_id, normalized_doi);
CREATE INDEX ix_record_versions_pmid ON review_record_versions (review_id, normalized_pmid);
CREATE INDEX ix_record_versions_title_year
    ON review_record_versions (review_id, normalized_title, publication_year);
CREATE INDEX ix_record_versions_record ON review_record_versions (review_record_id, version_number);
CREATE INDEX ix_import_rows_batch ON import_rows (import_batch_id, row_index);
CREATE INDEX ix_import_rows_review_status ON import_rows (review_id, status);
CREATE INDEX ix_corpus_members_record ON corpus_version_members (review_id, review_record_id);

CREATE INDEX ix_dedup_runs_review ON deduplication_runs (review_id, created_at);
CREATE INDEX ix_cluster_members_record ON duplicate_cluster_members (review_id, review_record_id);
CREATE INDEX ix_resolutions_cluster ON duplicate_resolutions (cluster_id, decided_at);

CREATE INDEX ix_assignments_queue ON screening_assignments (
    review_id, stage_id, round_number, reviewer_subject_id, assigned_at, assignment_id
);
CREATE INDEX ix_assignment_corpus_version ON screening_assignments (corpus_version_id);
CREATE INDEX ix_decisions_record ON screening_decisions (review_id, stage_id, review_record_id);
CREATE INDEX ix_decisions_reviewer_progress ON screening_decisions (
    review_id, stage_id, round_number, reviewer_subject_id
) WHERE supersedes_decision_id IS NULL;
CREATE INDEX ix_conflicts_review ON screening_conflicts (review_id, stage_id, round_number);
CREATE INDEX ix_unblinding_conflict ON screening_unblinding_events (conflict_id);
CREATE INDEX ix_flow_projections_review ON flow_projections (review_id, computed_at);

CREATE INDEX ix_research_object_versions_object
    ON research_object_versions (research_object_id, version_number);
CREATE INDEX ix_research_objects_review ON research_objects (review_id);
CREATE INDEX ix_research_object_records_object ON research_object_records (research_object_id);

CREATE INDEX ix_observations_dataset ON observations (extraction_dataset_id);
CREATE INDEX ix_observation_versions_observation
    ON observation_versions (observation_id, version_number);
CREATE INDEX ix_acceptances_subject ON evidence_acceptances (review_id, subject_version_id);
CREATE INDEX ix_assertion_support_observation ON research_assertion_support (observation_version_id);

CREATE INDEX ix_domain_outbox_occurred_at ON domain_outbox (occurred_at);
CREATE INDEX ix_domain_outbox_aggregate
    ON domain_outbox (resource_scope, tenant_id, aggregate_type, aggregate_id, aggregate_version);

CREATE UNIQUE INDEX uq_durable_jobs_idempotency
    ON durable_jobs (tenant_id, review_id, job_type, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
CREATE INDEX ix_durable_jobs_claimable ON durable_jobs (state, job_type, queued_at)
    WHERE state IN ('queued', 'waiting');
CREATE INDEX ix_durable_jobs_lease_expiry ON durable_jobs (lease_expires_at)
    WHERE state = 'running';
CREATE INDEX ix_durable_jobs_review ON durable_jobs (tenant_id, review_id, queued_at);
CREATE INDEX ix_job_attempts_job ON job_attempts (job_id, attempt_number);

CREATE UNIQUE INDEX uq_snapshot_one_chain_per_review
    ON evidence_snapshots (review_id) WHERE supersedes_snapshot_id IS NULL;
CREATE UNIQUE INDEX uq_snapshot_supersession_linear
    ON evidence_snapshots (supersedes_snapshot_id) WHERE supersedes_snapshot_id IS NOT NULL;
CREATE UNIQUE INDEX uq_snapshot_one_per_job ON evidence_snapshots (job_id);
CREATE INDEX ix_evidence_snapshots_review ON evidence_snapshots (tenant_id, review_id, created_at);
CREATE INDEX ix_evidence_snapshots_content_hash ON evidence_snapshots (content_hash);
CREATE INDEX ix_snapshot_members_order
    ON evidence_snapshot_members (snapshot_id, owner_context, entity_type, entity_id, version_id);

CREATE INDEX ix_research_domain_events_order ON research_domain_events (
    resource_scope, tenant_id, aggregate_type, aggregate_id, aggregate_version
);
CREATE INDEX ix_research_domain_events_position ON research_domain_events (log_position);
CREATE UNIQUE INDEX uq_event_deliveries_consumer_event
    ON event_deliveries (event_id, consumer) WHERE replay_of_delivery_id IS NULL;
CREATE INDEX ix_event_deliveries_claimable ON event_deliveries (consumer, state, created_at)
    WHERE state IN ('pending', 'deferred');
CREATE INDEX ix_event_deliveries_lease ON event_deliveries (lease_expires_at)
    WHERE state = 'claimed';
CREATE INDEX ix_domain_inbox_progress
    ON domain_inbox (consumer, tenant_id, aggregate_type, aggregate_id, aggregate_version);

CREATE INDEX ix_artifact_records_owning_aggregate
    ON artifact_records (tenant_id, owning_aggregate_type, owning_aggregate_id);
CREATE INDEX ix_artifact_records_artifact
    ON artifact_records (tenant_id, artifact_id, artifact_version);

-- Supersession-chain partial uniques. One root/successor per chain node; the tail
-- of each chain is the current fact.
CREATE UNIQUE INDEX uq_resolution_supersession_linear
    ON duplicate_resolutions (supersedes_resolution_id)
    WHERE supersedes_resolution_id IS NOT NULL;
CREATE UNIQUE INDEX uq_resolution_one_head_per_cluster
    ON duplicate_resolutions (cluster_id)
    WHERE supersedes_resolution_id IS NULL;

CREATE UNIQUE INDEX uq_decision_one_current_per_reviewer
    ON screening_decisions (review_id, stage_id, round_number, review_record_id, reviewer_subject_id)
    WHERE supersedes_decision_id IS NULL;
CREATE UNIQUE INDEX uq_decision_supersession_linear
    ON screening_decisions (supersedes_decision_id)
    WHERE supersedes_decision_id IS NOT NULL;

CREATE UNIQUE INDEX uq_adjudication_one_current_per_conflict
    ON screening_adjudications (conflict_id)
    WHERE supersedes_adjudication_id IS NULL;
CREATE UNIQUE INDEX uq_adjudication_supersession_linear
    ON screening_adjudications (supersedes_adjudication_id)
    WHERE supersedes_adjudication_id IS NOT NULL;

CREATE UNIQUE INDEX uq_research_object_version_supersession_linear
    ON research_object_versions (supersedes_version_id)
    WHERE supersedes_version_id IS NOT NULL;
CREATE UNIQUE INDEX uq_research_object_record_one_primary
    ON research_object_records (research_object_id)
    WHERE link_role = 'primary';

CREATE UNIQUE INDEX uq_extraction_dataset_supersession_linear
    ON extraction_dataset_versions (supersedes_version_id)
    WHERE supersedes_version_id IS NOT NULL;
CREATE UNIQUE INDEX uq_observation_supersession_linear
    ON observation_versions (supersedes_version_id)
    WHERE supersedes_version_id IS NOT NULL;
CREATE UNIQUE INDEX uq_research_assertion_supersession_linear
    ON research_assertion_versions (supersedes_version_id)
    WHERE supersedes_version_id IS NOT NULL;

CREATE UNIQUE INDEX uq_acceptance_one_chain_per_subject
    ON evidence_acceptances (review_id, subject_version_id)
    WHERE supersedes_acceptance_id IS NULL;
CREATE UNIQUE INDEX uq_acceptance_supersession_linear
    ON evidence_acceptances (supersedes_acceptance_id)
    WHERE supersedes_acceptance_id IS NOT NULL;

-- ---------------------------------------------------------------------------
-- 14. Shared guard functions
-- ---------------------------------------------------------------------------

-- The unconditional append-only guard. Trigger names keep the captured service's
-- `trg_<table>_append_only`; the reason text arrives as the trigger argument.
CREATE OR REPLACE FUNCTION er_append_only_guard() RETURNS trigger AS $$
BEGIN
  RAISE EXCEPTION '% is append-only: %', TG_TABLE_NAME, TG_ARGV[0]
    USING ERRCODE = 'restrict_violation';
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION er_immutable_guard() RETURNS trigger AS $$
BEGIN
  RAISE EXCEPTION '% is immutable: %', TG_TABLE_NAME, TG_ARGV[0]
    USING ERRCODE = 'restrict_violation';
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION er_no_delete_guard() RETURNS trigger AS $$
BEGIN
  RAISE EXCEPTION '% is not deletable: %', TG_TABLE_NAME, TG_ARGV[0]
    USING ERRCODE = 'restrict_violation';
END;
$$ LANGUAGE plpgsql;

-- Every lock, decision, job and analysis pins the exact protocol version, and the
-- pin must be LOCKED. SECURITY DEFINER so the check does not depend on what the
-- writer can see through RLS.
CREATE OR REPLACE FUNCTION er_pins_locked_protocol() RETURNS trigger AS $$
DECLARE
  pinned RECORD;
BEGIN
  SELECT p.review_id, p.tenant_id, p.locked_at
    INTO pinned
    FROM review_protocol_versions p
   WHERE p.protocol_version_id = NEW.protocol_version_id;

  IF NOT FOUND THEN
    RAISE EXCEPTION 'protocol_version_id % does not exist', NEW.protocol_version_id
      USING ERRCODE = 'foreign_key_violation';
  END IF;

  IF pinned.review_id <> NEW.review_id OR pinned.tenant_id <> NEW.tenant_id THEN
    RAISE EXCEPTION
      'protocol_version_id % belongs to a different review or tenant',
      NEW.protocol_version_id
      USING ERRCODE = 'restrict_violation';
  END IF;

  IF pinned.locked_at IS NULL THEN
    RAISE EXCEPTION
      'protocol_version_id % is not locked; evidence cannot be judged against a protocol that can still change',
      NEW.protocol_version_id
      USING ERRCODE = 'restrict_violation';
  END IF;

  RETURN NEW;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Lifecycle transitions as data, checked on INSERT. Includes 0028's
-- draft -> archived edge (an abandoned draft can be archived).
CREATE OR REPLACE FUNCTION review_state_history_legal_transition() RETURNS trigger AS $$
DECLARE
  prev_state text;
  prev_revision int;
BEGIN
  SELECT h.to_state, h.revision_number INTO prev_state, prev_revision
    FROM review_state_history h
   WHERE h.review_id = NEW.review_id
   ORDER BY h.revision_number DESC
   LIMIT 1;

  IF prev_state IS NULL THEN
    IF NEW.from_state IS NOT NULL OR NEW.to_state <> 'draft' OR NEW.revision_number <> 1 THEN
      RAISE EXCEPTION 'the first state row must be revision 1, NULL -> draft (got % -> %, revision %)',
        NEW.from_state, NEW.to_state, NEW.revision_number
        USING ERRCODE = 'restrict_violation';
    END IF;
    RETURN NEW;
  END IF;

  IF NEW.from_state IS DISTINCT FROM prev_state THEN
    RAISE EXCEPTION 'stale transition: review is in state %, caller declared %',
      prev_state, NEW.from_state
      USING ERRCODE = 'restrict_violation';
  END IF;

  IF NEW.revision_number <> prev_revision + 1 THEN
    RAISE EXCEPTION 'revision must be contiguous: expected %, got %',
      prev_revision + 1, NEW.revision_number
      USING ERRCODE = 'restrict_violation';
  END IF;

  IF NOT ((NEW.from_state, NEW.to_state) IN (
      ('draft', 'configured'),
      ('configured', 'active'),
      ('active', 'paused'),
      ('paused', 'active'),
      ('active', 'closed'),
      ('paused', 'closed'),
      ('closed', 'active'),
      ('closed', 'archived'),
      ('draft', 'archived')
  )) THEN
    RAISE EXCEPTION 'illegal lifecycle transition % -> %', NEW.from_state, NEW.to_state
      USING ERRCODE = 'restrict_violation';
  END IF;

  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION review_state_history_activation_gate() RETURNS trigger AS $$
DECLARE
  pv_review uuid;
  pv_locked timestamptz;
BEGIN
  IF NEW.protocol_version_id IS NULL THEN
    RETURN NEW;
  END IF;
  SELECT p.review_id, p.locked_at INTO pv_review, pv_locked
    FROM review_protocol_versions p
   WHERE p.protocol_version_id = NEW.protocol_version_id;

  IF pv_review IS DISTINCT FROM NEW.review_id THEN
    RAISE EXCEPTION 'protocol version % belongs to a different review', NEW.protocol_version_id
      USING ERRCODE = 'restrict_violation';
  END IF;
  IF pv_locked IS NULL THEN
    RAISE EXCEPTION 'protocol version % is not locked; activation requires a locked protocol',
      NEW.protocol_version_id
      USING ERRCODE = 'restrict_violation';
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Protocol versions are immutable once locked; an unlocked draft is still being
-- written.
CREATE OR REPLACE FUNCTION review_protocol_versions_locked_immutable() RETURNS trigger AS $$
BEGIN
  IF OLD.locked_at IS NOT NULL THEN
    RAISE EXCEPTION 'protocol version % is locked and cannot be modified or deleted',
      OLD.protocol_version_id
      USING ERRCODE = 'restrict_violation';
  END IF;
  IF TG_OP = 'DELETE' THEN
    RETURN OLD;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Stages are append-only until the stage set has been used; after that even a
-- DELETE is refused, and an UPDATE always is.
CREATE OR REPLACE FUNCTION review_stages_append_only() RETURNS trigger AS $$
DECLARE
  bound_tenant uuid;
BEGIN
  IF TG_OP <> 'DELETE' THEN
    RAISE EXCEPTION 'review_stages is append-only: a stage that ordered a recorded judgement cannot be re-ordered afterwards'
      USING ERRCODE = 'restrict_violation';
  END IF;

  bound_tenant := current_setting('app.current_tenant', true)::uuid;

  IF bound_tenant IS NULL OR bound_tenant IS DISTINCT FROM OLD.tenant_id THEN
    RAISE EXCEPTION
      'review_stages is append-only: app.current_tenant is not bound to this row''s tenant, '
      'so whether this stage has been used cannot be determined'
      USING ERRCODE = 'restrict_violation';
  END IF;

  IF EXISTS (SELECT 1 FROM screening_assignments a WHERE a.review_id = OLD.review_id)
     OR EXISTS (SELECT 1 FROM screening_decisions d WHERE d.review_id = OLD.review_id)
     OR EXISTS (SELECT 1 FROM screening_conflicts c WHERE c.review_id = OLD.review_id)
     OR EXISTS (SELECT 1 FROM inclusion_exports e WHERE e.review_id = OLD.review_id)
     OR EXISTS (
          SELECT 1 FROM review_state_history h
           WHERE h.review_id = OLD.review_id AND h.to_state = 'active'
        )
  THEN
    RAISE EXCEPTION
      'review_stages is append-only once its stage set has been used: a stage that ordered a recorded judgement cannot be re-ordered afterwards. '
      'This review has been activated, or holds an assignment, decision, conflict or sealed export.'
      USING ERRCODE = 'restrict_violation';
  END IF;

  RETURN OLD;
END;
$$ LANGUAGE plpgsql;

-- The corpus seal: count, ordinals and manifest hash are re-derived at COMMIT and
-- the transaction is refused on disagreement.
CREATE OR REPLACE FUNCTION corpus_versions_verify_seal() RETURNS trigger AS $$
DECLARE
  cv record;
  actual_count int;
  actual_hash text;
  ordering_ok boolean;
BEGIN
  SELECT * INTO cv FROM corpus_versions WHERE corpus_version_id = NEW.corpus_version_id;
  IF NOT FOUND THEN
    RETURN NULL;
  END IF;

  SELECT count(*) INTO actual_count
    FROM corpus_version_members m
   WHERE m.corpus_version_id = cv.corpus_version_id;

  IF actual_count <> cv.member_count THEN
    RAISE EXCEPTION 'corpus % sealed member_count % but holds % members',
      cv.corpus_version_id, cv.member_count, actual_count
      USING ERRCODE = 'restrict_violation';
  END IF;

  SELECT bool_and(ok) INTO ordering_ok FROM (
    SELECT m.member_ordinal = row_number() OVER (
             ORDER BY m.review_record_version_id::text ASC
           ) AS ok
      FROM corpus_version_members m
     WHERE m.corpus_version_id = cv.corpus_version_id
  ) t;

  IF actual_count > 0 AND NOT coalesce(ordering_ok, false) THEN
    RAISE EXCEPTION 'corpus % member ordinals do not follow ordering rule %',
      cv.corpus_version_id, cv.member_ordering_rule
      USING ERRCODE = 'restrict_violation';
  END IF;

  SELECT 'sha256:' || encode(
           sha256(convert_to(coalesce(string_agg(m.review_record_version_id::text, E'\n'
                                      ORDER BY m.review_record_version_id::text ASC), ''), 'UTF8')),
           'hex')
    INTO actual_hash
    FROM corpus_version_members m
   WHERE m.corpus_version_id = cv.corpus_version_id;

  IF actual_hash IS DISTINCT FROM cv.member_manifest_hash THEN
    RAISE EXCEPTION 'corpus % sealed manifest % but its members hash to %',
      cv.corpus_version_id, cv.member_manifest_hash, actual_hash
      USING ERRCODE = 'restrict_violation';
  END IF;

  RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION corpus_versions_verify_seal_on_version() RETURNS trigger AS $$
DECLARE
  actual_count int;
  actual_hash text;
BEGIN
  SELECT count(*) INTO actual_count
    FROM corpus_version_members m WHERE m.corpus_version_id = NEW.corpus_version_id;
  IF actual_count <> NEW.member_count THEN
    RAISE EXCEPTION 'corpus % sealed member_count % but holds % members',
      NEW.corpus_version_id, NEW.member_count, actual_count
      USING ERRCODE = 'restrict_violation';
  END IF;
  SELECT 'sha256:' || encode(
           sha256(convert_to(coalesce(string_agg(m.review_record_version_id::text, E'\n'
                                      ORDER BY m.review_record_version_id::text ASC), ''), 'UTF8')),
           'hex')
    INTO actual_hash
    FROM corpus_version_members m WHERE m.corpus_version_id = NEW.corpus_version_id;
  IF actual_hash IS DISTINCT FROM NEW.member_manifest_hash THEN
    RAISE EXCEPTION 'corpus % sealed manifest % but its members hash to %',
      NEW.corpus_version_id, NEW.member_manifest_hash, actual_hash
      USING ERRCODE = 'restrict_violation';
  END IF;
  RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION corpus_version_members_same_review() RETURNS trigger AS $$
DECLARE
  cv_review uuid;
  rec_review uuid;
  ver_record uuid;
BEGIN
  SELECT review_id INTO cv_review FROM corpus_versions WHERE corpus_version_id = NEW.corpus_version_id;
  SELECT review_id INTO rec_review FROM review_records WHERE review_record_id = NEW.review_record_id;
  SELECT review_record_id INTO ver_record
    FROM review_record_versions WHERE review_record_version_id = NEW.review_record_version_id;

  IF cv_review IS DISTINCT FROM NEW.review_id OR rec_review IS DISTINCT FROM NEW.review_id THEN
    RAISE EXCEPTION 'corpus member crosses reviews: corpus %, record %, member declared %',
      cv_review, rec_review, NEW.review_id
      USING ERRCODE = 'restrict_violation';
  END IF;
  IF ver_record IS DISTINCT FROM NEW.review_record_id THEN
    RAISE EXCEPTION 'member version % does not belong to record %',
      NEW.review_record_version_id, NEW.review_record_id
      USING ERRCODE = 'restrict_violation';
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION duplicate_resolutions_same_cluster() RETURNS trigger AS $$
DECLARE
  prev record;
BEGIN
  IF NEW.supersedes_resolution_id IS NULL THEN
    RETURN NEW;
  END IF;
  SELECT cluster_id, review_id, tenant_id INTO prev
    FROM duplicate_resolutions WHERE resolution_id = NEW.supersedes_resolution_id;
  IF prev.cluster_id IS DISTINCT FROM NEW.cluster_id
     OR prev.review_id IS DISTINCT FROM NEW.review_id
     OR prev.tenant_id IS DISTINCT FROM NEW.tenant_id THEN
    RAISE EXCEPTION 'resolution % cannot supersede % across cluster/review/tenant',
      NEW.resolution_id, NEW.supersedes_resolution_id
      USING ERRCODE = 'restrict_violation';
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Separation of duty: an adjudicator who recorded a decision in the conflict
-- cannot settle it. SECURITY DEFINER so it reads decisions the lead cannot see.
CREATE OR REPLACE FUNCTION screening_adjudications_separation_of_duty() RETURNS trigger AS $$
DECLARE
  took_part boolean;
BEGIN
  SELECT EXISTS (
    SELECT 1
      FROM screening_conflicts c
      JOIN screening_decisions d
        ON d.review_id = c.review_id
       AND d.stage_id = c.stage_id
       AND d.round_number = c.round_number
       AND d.review_record_id = c.review_record_id
     WHERE c.conflict_id = NEW.conflict_id
       AND d.reviewer_subject_id = NEW.adjudicator_subject_id
  ) INTO took_part;

  IF took_part THEN
    RAISE EXCEPTION
      'adjudicator % recorded a decision in this conflict and cannot adjudicate it',
      NEW.adjudicator_subject_id
      USING ERRCODE = 'restrict_violation';
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

CREATE OR REPLACE FUNCTION research_object_versions_supersession_scope() RETURNS trigger AS $$
DECLARE
  prior RECORD;
BEGIN
  IF NEW.supersedes_version_id IS NULL THEN
    RETURN NEW;
  END IF;

  SELECT v.research_object_id, v.review_id, v.tenant_id, v.version_number
    INTO prior
    FROM research_object_versions v
   WHERE v.research_object_version_id = NEW.supersedes_version_id;

  IF NOT FOUND THEN
    RAISE EXCEPTION 'superseded version % does not exist', NEW.supersedes_version_id
      USING ERRCODE = 'foreign_key_violation';
  END IF;

  IF prior.research_object_id <> NEW.research_object_id
     OR prior.review_id <> NEW.review_id
     OR prior.tenant_id <> NEW.tenant_id THEN
    RAISE EXCEPTION
      'version % cannot supersede %: different research object, review or tenant',
      NEW.research_object_version_id, NEW.supersedes_version_id
      USING ERRCODE = 'restrict_violation';
  END IF;

  IF prior.version_number >= NEW.version_number THEN
    RAISE EXCEPTION
      'version %s cannot supersede the later or equal version %s',
      NEW.version_number, prior.version_number
      USING ERRCODE = 'restrict_violation';
  END IF;

  RETURN NEW;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

CREATE OR REPLACE FUNCTION research_object_records_scope() RETURNS trigger AS $$
DECLARE
  rec RECORD;
BEGIN
  SELECT r.review_id, r.tenant_id INTO rec
    FROM review_records r
   WHERE r.review_record_id = NEW.review_record_id;

  IF NOT FOUND OR rec.review_id <> NEW.review_id OR rec.tenant_id <> NEW.tenant_id THEN
    RAISE EXCEPTION
      'review_record % does not belong to review % in this tenant',
      NEW.review_record_id, NEW.review_id
      USING ERRCODE = 'restrict_violation';
  END IF;

  IF NOT EXISTS (
    SELECT 1 FROM corpus_version_members m
     WHERE m.corpus_version_id = NEW.corpus_version_id
       AND m.review_record_id = NEW.review_record_id
       AND m.review_record_version_id = NEW.review_record_version_id
       AND m.tenant_id = NEW.tenant_id
  ) THEN
    RAISE EXCEPTION
      'review_record version % is not a member of corpus version %',
      NEW.review_record_version_id, NEW.corpus_version_id
      USING ERRCODE = 'restrict_violation';
  END IF;

  RETURN NEW;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- `subject_content_hash` is a witness: it must equal the subject version's hash,
-- and a chain must stay about one version.
CREATE OR REPLACE FUNCTION evidence_acceptances_witnesses_its_subject() RETURNS trigger AS $$
DECLARE
  actual text;
  prior RECORD;
  mine uuid;
BEGIN
  SELECT CASE NEW.subject_kind
           WHEN 'research_object_version' THEN
             (SELECT v.content_hash FROM research_object_versions v
               WHERE v.research_object_version_id = NEW.research_object_version_id)
           WHEN 'extraction_dataset_version' THEN
             (SELECT v.content_hash FROM extraction_dataset_versions v
               WHERE v.extraction_dataset_version_id = NEW.extraction_dataset_version_id)
           WHEN 'observation_version' THEN
             (SELECT v.content_hash FROM observation_versions v
               WHERE v.observation_version_id = NEW.observation_version_id)
         END
    INTO actual;

  IF actual IS NULL THEN
    RAISE EXCEPTION 'acceptance names a % that does not exist', NEW.subject_kind
      USING ERRCODE = 'foreign_key_violation';
  END IF;

  IF actual <> NEW.subject_content_hash THEN
    RAISE EXCEPTION
      'acceptance witnesses % but the % actually hashes to %',
      NEW.subject_content_hash, NEW.subject_kind, actual
      USING ERRCODE = 'restrict_violation';
  END IF;

  IF NEW.supersedes_acceptance_id IS NOT NULL THEN
    SELECT a.subject_version_id, a.review_id, a.tenant_id INTO prior
      FROM evidence_acceptances a
     WHERE a.acceptance_id = NEW.supersedes_acceptance_id;

    -- subject_version_id is GENERATED and still NULL in a BEFORE trigger.
    mine := COALESCE(NEW.research_object_version_id,
                     NEW.extraction_dataset_version_id,
                     NEW.observation_version_id);

    IF NOT FOUND THEN
      RAISE EXCEPTION 'superseded acceptance % does not exist', NEW.supersedes_acceptance_id
        USING ERRCODE = 'foreign_key_violation';
    END IF;

    IF prior.subject_version_id <> mine
       OR prior.review_id <> NEW.review_id
       OR prior.tenant_id <> NEW.tenant_id THEN
      RAISE EXCEPTION
        'acceptance % cannot supersede %: a different subject version, review or tenant',
        NEW.acceptance_id, NEW.supersedes_acceptance_id
        USING ERRCODE = 'restrict_violation';
    END IF;
  END IF;

  RETURN NEW;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

CREATE OR REPLACE FUNCTION evidence_snapshots_supersede_within_one_review()
RETURNS trigger AS $$
DECLARE
  prior RECORD;
BEGIN
  IF NEW.supersedes_snapshot_id IS NULL THEN
    RETURN NEW;
  END IF;
  SELECT review_id, tenant_id INTO prior
    FROM evidence_snapshots WHERE snapshot_id = NEW.supersedes_snapshot_id;
  IF NOT FOUND THEN
    RAISE EXCEPTION 'snapshot % supersedes % which does not exist',
      NEW.snapshot_id, NEW.supersedes_snapshot_id
      USING ERRCODE = 'foreign_key_violation';
  END IF;
  IF prior.review_id <> NEW.review_id OR prior.tenant_id <> NEW.tenant_id THEN
    RAISE EXCEPTION
      'snapshot % (review %) cannot supersede % (review %): a chain belongs to one review',
      NEW.snapshot_id, NEW.review_id, NEW.supersedes_snapshot_id, prior.review_id
      USING ERRCODE = 'restrict_violation';
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- A batch may only have its three tallies completed, and only by the transaction
-- that inserted it.
CREATE OR REPLACE FUNCTION import_batches_append_only() RETURNS trigger AS $fn$
BEGIN
  IF TG_OP = 'DELETE' THEN
    RAISE EXCEPTION
      'import_batches is append-only: a batch is the provenance of every record it carried'
      USING ERRCODE = 'restrict_violation';
  END IF;

  IF NEW.import_batch_id IS DISTINCT FROM OLD.import_batch_id
     OR NEW.review_id IS DISTINCT FROM OLD.review_id
     OR NEW.tenant_id IS DISTINCT FROM OLD.tenant_id
     OR NEW.source_kind IS DISTINCT FROM OLD.source_kind
     OR NEW.source_ref IS DISTINCT FROM OLD.source_ref
     OR NEW.raw_payload_digest IS DISTINCT FROM OLD.raw_payload_digest
     OR NEW.submitted_by_subject_id IS DISTINCT FROM OLD.submitted_by_subject_id
     OR NEW.created_at IS DISTINCT FROM OLD.created_at
  THEN
    RAISE EXCEPTION
      'import_batches is append-only: only accepted_count, rejected_count and quarantined_count '
      'may be written, and only by the transaction that inserted the batch'
      USING ERRCODE = 'restrict_violation';
  END IF;

  IF OLD.created_at IS DISTINCT FROM transaction_timestamp() THEN
    RAISE EXCEPTION
      'import_batches is append-only once its transaction has committed: the tallies of batch % '
      'were completed by the transaction that created it and cannot be rewritten',
      OLD.import_batch_id
      USING ERRCODE = 'restrict_violation';
  END IF;

  RETURN NEW;
END;
$fn$ LANGUAGE plpgsql;

-- The durable-job state machine: only the drawn transitions, terminal rows are
-- immutable, cancellation intent cannot be withdrawn, attempts never go backwards.
CREATE OR REPLACE FUNCTION durable_jobs_transition() RETURNS trigger AS $$
BEGIN
  IF OLD.state IN ('succeeded', 'failed', 'cancelled') THEN
    RAISE EXCEPTION
      'durable_jobs % is terminal in state % and cannot be modified; '
      'a cancellation that arrives after a terminal commit returns that outcome',
      OLD.job_id, OLD.state
      USING ERRCODE = 'restrict_violation';
  END IF;

  IF NEW.state IS DISTINCT FROM OLD.state AND NOT (
       (OLD.state = 'queued' AND NEW.state = 'running')
    OR (OLD.state = 'queued' AND NEW.state = 'cancelled')
    OR (OLD.state = 'running' AND NEW.state = 'succeeded')
    OR (OLD.state = 'running' AND NEW.state = 'failed')
    OR (OLD.state = 'running' AND NEW.state = 'cancelled')
    OR (OLD.state = 'running' AND NEW.state = 'waiting')
    OR (OLD.state = 'running' AND NEW.state = 'queued')
    OR (OLD.state = 'waiting' AND NEW.state = 'queued')
    OR (OLD.state = 'waiting' AND NEW.state = 'cancelled')
  ) THEN
    RAISE EXCEPTION 'durable_jobs % cannot move from % to %',
      OLD.job_id, OLD.state, NEW.state
      USING ERRCODE = 'restrict_violation';
  END IF;

  IF NEW.cancel_requested_at IS NULL AND OLD.cancel_requested_at IS NOT NULL THEN
    RAISE EXCEPTION 'durable_jobs %: cancellation intent cannot be withdrawn', OLD.job_id
      USING ERRCODE = 'restrict_violation';
  END IF;

  IF NEW.attempt < OLD.attempt THEN
    RAISE EXCEPTION 'durable_jobs %: attempt counter cannot go backwards', OLD.job_id
      USING ERRCODE = 'restrict_violation';
  END IF;

  NEW.updated_at := now();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION durable_jobs_no_delete() RETURNS trigger AS $$
BEGIN
  RAISE EXCEPTION 'durable_jobs is not deletable: a job that can be erased '
                  'cannot answer whether a scientific run happened'
    USING ERRCODE = 'restrict_violation';
END;
$$ LANGUAGE plpgsql;

-- An attempt may be updated once, to close it; after that it is sealed.
CREATE OR REPLACE FUNCTION job_attempts_seal() RETURNS trigger AS $$
BEGIN
  IF TG_OP = 'DELETE' THEN
    RAISE EXCEPTION 'job_attempts is append-only: execution history is not erasable'
      USING ERRCODE = 'restrict_violation';
  END IF;
  IF OLD.ended_at IS NOT NULL THEN
    RAISE EXCEPTION 'job_attempts % ended at % and is sealed', OLD.attempt_id, OLD.ended_at
      USING ERRCODE = 'restrict_violation';
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- ---------------------------------------------------------------------------
-- 15. Triggers
-- ---------------------------------------------------------------------------

CREATE TRIGGER trg_review_protocol_versions_locked_immutable
    BEFORE UPDATE OR DELETE ON review_protocol_versions
    FOR EACH ROW EXECUTE FUNCTION review_protocol_versions_locked_immutable();

CREATE TRIGGER trg_review_state_history_append_only
    BEFORE UPDATE OR DELETE ON review_state_history
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard('a transition that happened cannot un-happen');

CREATE TRIGGER trg_review_state_history_legal_transition
    BEFORE INSERT ON review_state_history
    FOR EACH ROW EXECUTE FUNCTION review_state_history_legal_transition();

CREATE TRIGGER trg_review_state_history_activation_gate
    BEFORE INSERT ON review_state_history
    FOR EACH ROW EXECUTE FUNCTION review_state_history_activation_gate();

CREATE TRIGGER trg_review_records_immutable
    BEFORE UPDATE OR DELETE ON review_records
    FOR EACH ROW EXECUTE FUNCTION er_immutable_guard('corrections create a new review_record_version');

CREATE TRIGGER trg_review_record_versions_immutable
    BEFORE UPDATE OR DELETE ON review_record_versions
    FOR EACH ROW EXECUTE FUNCTION er_immutable_guard('a correction is a new version, never an edit');

CREATE TRIGGER trg_corpus_versions_immutable
    BEFORE UPDATE OR DELETE ON corpus_versions
    FOR EACH ROW EXECUTE FUNCTION er_immutable_guard(
        'any add, remove or replace of a member creates a NEW corpus version'
    );

CREATE TRIGGER trg_corpus_version_members_immutable
    BEFORE UPDATE OR DELETE ON corpus_version_members
    FOR EACH ROW EXECUTE FUNCTION er_immutable_guard('membership is sealed at the version it belongs to');

CREATE CONSTRAINT TRIGGER trg_corpus_members_verify_seal
    AFTER INSERT ON corpus_version_members
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION corpus_versions_verify_seal();

CREATE CONSTRAINT TRIGGER trg_corpus_versions_verify_seal
    AFTER INSERT ON corpus_versions
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION corpus_versions_verify_seal_on_version();

CREATE TRIGGER trg_corpus_version_members_same_review
    BEFORE INSERT ON corpus_version_members
    FOR EACH ROW EXECUTE FUNCTION corpus_version_members_same_review();

CREATE TRIGGER trg_duplicate_resolutions_same_cluster
    BEFORE INSERT ON duplicate_resolutions
    FOR EACH ROW EXECUTE FUNCTION duplicate_resolutions_same_cluster();

CREATE TRIGGER trg_duplicate_resolutions_append_only
    BEFORE UPDATE OR DELETE ON duplicate_resolutions
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard(
        'reversal is a NEW row superseding the old one, never an edit'
    );

CREATE TRIGGER trg_duplicate_survivorship_fields_append_only
    BEFORE UPDATE OR DELETE ON duplicate_survivorship_fields
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard('field survivorship belongs to its resolution');

CREATE TRIGGER trg_import_batches_append_only
    BEFORE UPDATE OR DELETE ON import_batches
    FOR EACH ROW EXECUTE FUNCTION import_batches_append_only();

CREATE TRIGGER trg_import_rows_append_only
    BEFORE UPDATE OR DELETE ON import_rows
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard(
        'identified_total is derived from these rows, and a sealed export cites it'
    );

CREATE TRIGGER trg_duplicate_clusters_append_only
    BEFORE UPDATE OR DELETE ON duplicate_clusters
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard(
        'a run is frozen, and a cluster that ordered a resolution cannot be re-drawn afterwards'
    );

CREATE TRIGGER trg_duplicate_cluster_members_append_only
    BEFORE UPDATE OR DELETE ON duplicate_cluster_members
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard(
        'membership decides which records a merge removes, and duplicates_removed is derived from it'
    );

CREATE TRIGGER trg_review_stages_append_only
    BEFORE UPDATE OR DELETE ON review_stages
    FOR EACH ROW EXECUTE FUNCTION review_stages_append_only();

CREATE TRIGGER trg_screening_assignments_append_only
    BEFORE UPDATE OR DELETE ON screening_assignments
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard(
        'assignment inputs are immutable (ER-004.1)'
    );

CREATE TRIGGER trg_screening_decisions_append_only
    BEFORE UPDATE OR DELETE ON screening_decisions
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard(
        'a correction supersedes; a decision is never overwritten'
    );

CREATE TRIGGER trg_screening_conflicts_append_only
    BEFORE UPDATE OR DELETE ON screening_conflicts
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard(
        'resolution is an adjudication row, not a status update'
    );

CREATE TRIGGER trg_screening_adjudications_separation_of_duty
    BEFORE INSERT ON screening_adjudications
    FOR EACH ROW EXECUTE FUNCTION screening_adjudications_separation_of_duty();

CREATE TRIGGER trg_screening_adjudications_append_only
    BEFORE UPDATE OR DELETE ON screening_adjudications
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard(
        'a revised adjudication supersedes the previous one'
    );

CREATE TRIGGER trg_screening_unblinding_events_append_only
    BEFORE UPDATE OR DELETE ON screening_unblinding_events
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard(
        'the audit trail of who looked is the whole point'
    );

CREATE TRIGGER trg_inclusion_exports_append_only
    BEFORE UPDATE OR DELETE ON inclusion_exports
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard(
        'a sealed export is evidence; a new export is a new row'
    );

CREATE TRIGGER trg_research_objects_append_only
    BEFORE UPDATE OR DELETE ON research_objects
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard(
        'identity never changes; revise the object with a new version and merge with a supersession'
    );

CREATE TRIGGER trg_research_object_versions_append_only
    BEFORE UPDATE OR DELETE ON research_object_versions
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard(
        'a version is immutable; record a new one that supersedes it'
    );

CREATE TRIGGER trg_research_object_versions_pins_locked_protocol
    BEFORE INSERT ON research_object_versions
    FOR EACH ROW EXECUTE FUNCTION er_pins_locked_protocol();

CREATE TRIGGER trg_research_object_versions_supersession_scope
    BEFORE INSERT ON research_object_versions
    FOR EACH ROW EXECUTE FUNCTION research_object_versions_supersession_scope();

CREATE TRIGGER trg_research_object_records_append_only
    BEFORE UPDATE OR DELETE ON research_object_records
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard(
        'a link is a recorded judgement; record a correcting link rather than editing this one'
    );

CREATE TRIGGER trg_research_object_records_scope
    BEFORE INSERT ON research_object_records
    FOR EACH ROW EXECUTE FUNCTION research_object_records_scope();

CREATE TRIGGER trg_extraction_datasets_append_only
    BEFORE UPDATE OR DELETE ON extraction_datasets
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard('identity never changes; revise with a new version');

CREATE TRIGGER trg_extraction_dataset_versions_append_only
    BEFORE UPDATE OR DELETE ON extraction_dataset_versions
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard('a version is immutable; supersede it instead');

CREATE TRIGGER trg_extraction_dataset_versions_pins_locked_protocol
    BEFORE INSERT ON extraction_dataset_versions
    FOR EACH ROW EXECUTE FUNCTION er_pins_locked_protocol();

CREATE TRIGGER trg_observations_append_only
    BEFORE UPDATE OR DELETE ON observations
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard('identity never changes; revise with a new version');

CREATE TRIGGER trg_observation_versions_append_only
    BEFORE UPDATE OR DELETE ON observation_versions
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard('a version is immutable; supersede it instead');

CREATE TRIGGER trg_observation_versions_pins_locked_protocol
    BEFORE INSERT ON observation_versions
    FOR EACH ROW EXECUTE FUNCTION er_pins_locked_protocol();

CREATE TRIGGER trg_research_assertions_append_only
    BEFORE UPDATE OR DELETE ON research_assertions
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard('identity never changes; revise with a new version');

CREATE TRIGGER trg_research_assertion_versions_append_only
    BEFORE UPDATE OR DELETE ON research_assertion_versions
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard('a version is immutable; supersede it instead');

CREATE TRIGGER trg_research_assertion_versions_pins_locked_protocol
    BEFORE INSERT ON research_assertion_versions
    FOR EACH ROW EXECUTE FUNCTION er_pins_locked_protocol();

CREATE TRIGGER trg_research_assertion_support_append_only
    BEFORE UPDATE OR DELETE ON research_assertion_support
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard(
        'support is part of the assertion version it belongs to; revise the assertion instead'
    );

CREATE TRIGGER trg_evidence_acceptances_append_only
    BEFORE UPDATE OR DELETE ON evidence_acceptances
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard(
        'a decision is a record of what someone decided; withdraw it with a new row'
    );

CREATE TRIGGER trg_evidence_acceptances_pins_locked_protocol
    BEFORE INSERT ON evidence_acceptances
    FOR EACH ROW EXECUTE FUNCTION er_pins_locked_protocol();

CREATE TRIGGER trg_evidence_acceptances_witnesses_its_subject
    BEFORE INSERT ON evidence_acceptances
    FOR EACH ROW EXECUTE FUNCTION evidence_acceptances_witnesses_its_subject();

CREATE TRIGGER trg_domain_outbox_append_only
    BEFORE UPDATE OR DELETE ON domain_outbox
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard(
        'an event that can be edited after it was committed beside the state it describes '
        'reopens the exact gap the outbox closes'
    );

CREATE TRIGGER trg_durable_jobs_transition
    BEFORE UPDATE ON durable_jobs
    FOR EACH ROW EXECUTE FUNCTION durable_jobs_transition();

CREATE TRIGGER trg_durable_jobs_no_delete
    BEFORE DELETE ON durable_jobs
    FOR EACH ROW EXECUTE FUNCTION durable_jobs_no_delete();

CREATE TRIGGER trg_job_attempts_seal
    BEFORE UPDATE OR DELETE ON job_attempts
    FOR EACH ROW EXECUTE FUNCTION job_attempts_seal();

CREATE TRIGGER trg_evidence_snapshots_supersede_within_one_review
    BEFORE INSERT ON evidence_snapshots
    FOR EACH ROW EXECUTE FUNCTION evidence_snapshots_supersede_within_one_review();

CREATE TRIGGER trg_evidence_snapshots_append_only
    BEFORE UPDATE OR DELETE ON evidence_snapshots
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard(
        'a snapshot that can be edited after a run bound its hash is a snapshot that '
        'cannot answer what that run was allowed to read'
    );

CREATE TRIGGER trg_evidence_snapshot_members_append_only
    BEFORE UPDATE OR DELETE ON evidence_snapshot_members
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard(
        'removing a member after the manifest digest was published would make the '
        'digest unverifiable rather than wrong, which is harder to notice'
    );

CREATE TRIGGER trg_snapshot_artifacts_append_only
    BEFORE UPDATE OR DELETE ON snapshot_artifacts
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard(
        'an artifact row is a claim about bytes that already exist'
    );

CREATE TRIGGER trg_research_domain_events_append_only
    BEFORE UPDATE OR DELETE ON research_domain_events
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard(
        'a durable log whose rows can be edited after the fact is not a log, and replay '
        'would reproduce something that never happened'
    );

CREATE TRIGGER trg_event_deliveries_no_delete
    BEFORE DELETE ON event_deliveries
    FOR EACH ROW EXECUTE FUNCTION er_no_delete_guard(
        'a deletable delivery makes the question of whether this consumer ever saw the event unanswerable'
    );

CREATE TRIGGER trg_domain_inbox_no_delete
    BEFORE DELETE ON domain_inbox
    FOR EACH ROW EXECUTE FUNCTION er_no_delete_guard(
        'a deleted receipt reopens the exactly-once window it exists to close'
    );

CREATE TRIGGER trg_projection_checkpoints_no_delete
    BEFORE DELETE ON projection_checkpoints
    FOR EACH ROW EXECUTE FUNCTION er_no_delete_guard(
        'a deleted checkpoint silently restarts a consumer from the beginning of the log'
    );

CREATE TRIGGER trg_artifact_records_append_only
    BEFORE UPDATE OR DELETE ON artifact_records
    FOR EACH ROW EXECUTE FUNCTION er_append_only_guard(
        'a rights snapshot that can be edited after the fact is not a snapshot, and a '
        'rights-change must open new impact work rather than rewrite what was checked'
    );

-- ---------------------------------------------------------------------------
-- 16. Row-level security: ENABLE + FORCE on every table, tenant predicate
-- ---------------------------------------------------------------------------

ALTER TABLE operation_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE operation_idempotency FORCE ROW LEVEL SECURITY;
CREATE POLICY operation_idempotency_tenant ON operation_idempotency
    USING (resource_tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (resource_tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE reviews ENABLE ROW LEVEL SECURITY;
ALTER TABLE reviews FORCE ROW LEVEL SECURITY;
CREATE POLICY reviews_tenant ON reviews
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE review_protocol_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE review_protocol_versions FORCE ROW LEVEL SECURITY;
CREATE POLICY review_protocol_versions_tenant ON review_protocol_versions
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE review_stages ENABLE ROW LEVEL SECURITY;
ALTER TABLE review_stages FORCE ROW LEVEL SECURITY;
CREATE POLICY review_stages_tenant ON review_stages
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE review_state_history ENABLE ROW LEVEL SECURITY;
ALTER TABLE review_state_history FORCE ROW LEVEL SECURITY;
CREATE POLICY review_state_history_tenant ON review_state_history
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE import_batches ENABLE ROW LEVEL SECURITY;
ALTER TABLE import_batches FORCE ROW LEVEL SECURITY;
CREATE POLICY import_batches_tenant ON import_batches
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE import_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE import_rows FORCE ROW LEVEL SECURITY;
CREATE POLICY import_rows_tenant ON import_rows
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE review_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE review_records FORCE ROW LEVEL SECURITY;
CREATE POLICY review_records_tenant ON review_records
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE review_record_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE review_record_versions FORCE ROW LEVEL SECURITY;
CREATE POLICY review_record_versions_tenant ON review_record_versions
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE corpus_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE corpus_versions FORCE ROW LEVEL SECURITY;
CREATE POLICY corpus_versions_tenant ON corpus_versions
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE corpus_version_members ENABLE ROW LEVEL SECURITY;
ALTER TABLE corpus_version_members FORCE ROW LEVEL SECURITY;
CREATE POLICY corpus_version_members_tenant ON corpus_version_members
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE deduplication_runs ENABLE ROW LEVEL SECURITY;
ALTER TABLE deduplication_runs FORCE ROW LEVEL SECURITY;
CREATE POLICY deduplication_runs_tenant ON deduplication_runs
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE duplicate_pair_scores ENABLE ROW LEVEL SECURITY;
ALTER TABLE duplicate_pair_scores FORCE ROW LEVEL SECURITY;
CREATE POLICY duplicate_pair_scores_tenant ON duplicate_pair_scores
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE duplicate_clusters ENABLE ROW LEVEL SECURITY;
ALTER TABLE duplicate_clusters FORCE ROW LEVEL SECURITY;
CREATE POLICY duplicate_clusters_tenant ON duplicate_clusters
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE duplicate_cluster_members ENABLE ROW LEVEL SECURITY;
ALTER TABLE duplicate_cluster_members FORCE ROW LEVEL SECURITY;
CREATE POLICY duplicate_cluster_members_tenant ON duplicate_cluster_members
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE duplicate_resolutions ENABLE ROW LEVEL SECURITY;
ALTER TABLE duplicate_resolutions FORCE ROW LEVEL SECURITY;
CREATE POLICY duplicate_resolutions_tenant ON duplicate_resolutions
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE duplicate_survivorship_fields ENABLE ROW LEVEL SECURITY;
ALTER TABLE duplicate_survivorship_fields FORCE ROW LEVEL SECURITY;
CREATE POLICY duplicate_survivorship_fields_tenant ON duplicate_survivorship_fields
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE screening_assignments ENABLE ROW LEVEL SECURITY;
ALTER TABLE screening_assignments FORCE ROW LEVEL SECURITY;
CREATE POLICY screening_assignments_tenant ON screening_assignments
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

-- The blinding policy is the whole blinding boundary, and tenant isolation is
-- folded into it: separate permissive policies are OR-ed, so a second tenant
-- policy would let a cross-tenant read of one's own subject id succeed.
ALTER TABLE screening_decisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE screening_decisions FORCE ROW LEVEL SECURITY;
CREATE POLICY screening_decisions_blinded ON screening_decisions
    USING (
        tenant_id = current_setting('app.current_tenant', true)::uuid
        AND (
            reviewer_subject_id = current_setting('app.actor_subject_id', true)
            OR current_setting('app.actor_role', true) = 'lead'
            OR EXISTS (
                SELECT 1
                  FROM screening_unblinding_events u
                  JOIN screening_conflicts c ON c.conflict_id = u.conflict_id
                 WHERE c.review_id = screening_decisions.review_id
                   AND c.stage_id = screening_decisions.stage_id
                   AND c.round_number = screening_decisions.round_number
                   AND c.review_record_id = screening_decisions.review_record_id
                   AND current_setting('app.actor_subject_id', true) = ANY (u.disclosed_to_subject_ids)
            )
        )
    )
    WITH CHECK (
        tenant_id = current_setting('app.current_tenant', true)::uuid
        AND reviewer_subject_id = current_setting('app.actor_subject_id', true)
    );

ALTER TABLE screening_conflicts ENABLE ROW LEVEL SECURITY;
ALTER TABLE screening_conflicts FORCE ROW LEVEL SECURITY;
CREATE POLICY screening_conflicts_tenant ON screening_conflicts
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE screening_adjudications ENABLE ROW LEVEL SECURITY;
ALTER TABLE screening_adjudications FORCE ROW LEVEL SECURITY;
CREATE POLICY screening_adjudications_tenant ON screening_adjudications
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE screening_unblinding_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE screening_unblinding_events FORCE ROW LEVEL SECURITY;
CREATE POLICY screening_unblinding_events_tenant ON screening_unblinding_events
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE flow_revisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE flow_revisions FORCE ROW LEVEL SECURITY;
CREATE POLICY flow_revisions_tenant ON flow_revisions
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE flow_projections ENABLE ROW LEVEL SECURITY;
ALTER TABLE flow_projections FORCE ROW LEVEL SECURITY;
CREATE POLICY flow_projections_tenant ON flow_projections
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE inclusion_exports ENABLE ROW LEVEL SECURITY;
ALTER TABLE inclusion_exports FORCE ROW LEVEL SECURITY;
CREATE POLICY inclusion_exports_tenant ON inclusion_exports
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE research_objects ENABLE ROW LEVEL SECURITY;
ALTER TABLE research_objects FORCE ROW LEVEL SECURITY;
CREATE POLICY research_objects_tenant ON research_objects
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE research_object_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE research_object_versions FORCE ROW LEVEL SECURITY;
CREATE POLICY research_object_versions_tenant ON research_object_versions
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE research_object_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE research_object_records FORCE ROW LEVEL SECURITY;
CREATE POLICY research_object_records_tenant ON research_object_records
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE extraction_datasets ENABLE ROW LEVEL SECURITY;
ALTER TABLE extraction_datasets FORCE ROW LEVEL SECURITY;
CREATE POLICY extraction_datasets_tenant ON extraction_datasets
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE extraction_dataset_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE extraction_dataset_versions FORCE ROW LEVEL SECURITY;
CREATE POLICY extraction_dataset_versions_tenant ON extraction_dataset_versions
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE observations ENABLE ROW LEVEL SECURITY;
ALTER TABLE observations FORCE ROW LEVEL SECURITY;
CREATE POLICY observations_tenant ON observations
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE observation_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE observation_versions FORCE ROW LEVEL SECURITY;
CREATE POLICY observation_versions_tenant ON observation_versions
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE research_assertions ENABLE ROW LEVEL SECURITY;
ALTER TABLE research_assertions FORCE ROW LEVEL SECURITY;
CREATE POLICY research_assertions_tenant ON research_assertions
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE research_assertion_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE research_assertion_versions FORCE ROW LEVEL SECURITY;
CREATE POLICY research_assertion_versions_tenant ON research_assertion_versions
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE research_assertion_support ENABLE ROW LEVEL SECURITY;
ALTER TABLE research_assertion_support FORCE ROW LEVEL SECURITY;
CREATE POLICY research_assertion_support_tenant ON research_assertion_support
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE evidence_acceptances ENABLE ROW LEVEL SECURITY;
ALTER TABLE evidence_acceptances FORCE ROW LEVEL SECURITY;
CREATE POLICY evidence_acceptances_tenant ON evidence_acceptances
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE domain_outbox ENABLE ROW LEVEL SECURITY;
ALTER TABLE domain_outbox FORCE ROW LEVEL SECURITY;
CREATE POLICY domain_outbox_tenant ON domain_outbox
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE durable_jobs ENABLE ROW LEVEL SECURITY;
ALTER TABLE durable_jobs FORCE ROW LEVEL SECURITY;
CREATE POLICY durable_jobs_tenant ON durable_jobs
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE job_attempts ENABLE ROW LEVEL SECURITY;
ALTER TABLE job_attempts FORCE ROW LEVEL SECURITY;
CREATE POLICY job_attempts_tenant ON job_attempts
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE evidence_snapshots ENABLE ROW LEVEL SECURITY;
ALTER TABLE evidence_snapshots FORCE ROW LEVEL SECURITY;
CREATE POLICY evidence_snapshots_tenant ON evidence_snapshots
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE evidence_snapshot_members ENABLE ROW LEVEL SECURITY;
ALTER TABLE evidence_snapshot_members FORCE ROW LEVEL SECURITY;
CREATE POLICY evidence_snapshot_members_tenant ON evidence_snapshot_members
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE snapshot_artifacts ENABLE ROW LEVEL SECURITY;
ALTER TABLE snapshot_artifacts FORCE ROW LEVEL SECURITY;
CREATE POLICY snapshot_artifacts_tenant ON snapshot_artifacts
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE research_domain_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE research_domain_events FORCE ROW LEVEL SECURITY;
CREATE POLICY research_domain_events_tenant ON research_domain_events
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE event_deliveries ENABLE ROW LEVEL SECURITY;
ALTER TABLE event_deliveries FORCE ROW LEVEL SECURITY;
CREATE POLICY event_deliveries_tenant ON event_deliveries
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE domain_inbox ENABLE ROW LEVEL SECURITY;
ALTER TABLE domain_inbox FORCE ROW LEVEL SECURITY;
CREATE POLICY domain_inbox_tenant ON domain_inbox
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE projection_checkpoints FORCE ROW LEVEL SECURITY;
CREATE POLICY projection_checkpoints_tenant ON projection_checkpoints
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

ALTER TABLE artifact_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE artifact_records FORCE ROW LEVEL SECURITY;
CREATE POLICY artifact_records_tenant ON artifact_records
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant', true)::uuid);

-- ---------------------------------------------------------------------------
-- 17. Additive cross-tenant worker policies
--
-- The captured service granted these only when its restricted worker/relay role
-- existed; a fresh database without the role gets the tenant policy only, with a
-- NOTICE naming what was skipped.
-- ---------------------------------------------------------------------------

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'evidence_review_worker') THEN
    EXECUTE 'CREATE POLICY durable_jobs_worker ON durable_jobs TO evidence_review_worker USING (true) WITH CHECK (true)';
    EXECUTE 'GRANT SELECT, INSERT, UPDATE ON durable_jobs TO evidence_review_worker';
  ELSE
    RAISE NOTICE 'role evidence_review_worker is absent; cross-tenant policy on durable_jobs skipped';
  END IF;
END $$;

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'evidence_review_worker') THEN
    EXECUTE 'CREATE POLICY job_attempts_worker ON job_attempts TO evidence_review_worker USING (true) WITH CHECK (true)';
    EXECUTE 'GRANT SELECT, INSERT, UPDATE ON job_attempts TO evidence_review_worker';
  ELSE
    RAISE NOTICE 'role evidence_review_worker is absent; cross-tenant policy on job_attempts skipped';
  END IF;
END $$;

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'evidence_review_worker') THEN
    EXECUTE 'CREATE POLICY domain_outbox_worker ON domain_outbox TO evidence_review_worker USING (true) WITH CHECK (true)';
    EXECUTE 'GRANT SELECT, INSERT, UPDATE ON domain_outbox TO evidence_review_worker';
  ELSE
    RAISE NOTICE 'role evidence_review_worker is absent; cross-tenant policy on domain_outbox skipped';
  END IF;
END $$;

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'evidence_review_worker') THEN
    EXECUTE 'CREATE POLICY research_domain_events_worker ON research_domain_events TO evidence_review_worker USING (true) WITH CHECK (true)';
    EXECUTE 'GRANT SELECT, INSERT, UPDATE ON research_domain_events TO evidence_review_worker';
  ELSE
    RAISE NOTICE 'role evidence_review_worker is absent; cross-tenant policy on research_domain_events skipped';
  END IF;
END $$;

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'evidence_review_worker') THEN
    EXECUTE 'CREATE POLICY event_deliveries_worker ON event_deliveries TO evidence_review_worker USING (true) WITH CHECK (true)';
    EXECUTE 'GRANT SELECT, INSERT, UPDATE ON event_deliveries TO evidence_review_worker';
  ELSE
    RAISE NOTICE 'role evidence_review_worker is absent; cross-tenant policy on event_deliveries skipped';
  END IF;
END $$;

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'evidence_review_worker') THEN
    EXECUTE 'CREATE POLICY domain_inbox_worker ON domain_inbox TO evidence_review_worker USING (true) WITH CHECK (true)';
    EXECUTE 'GRANT SELECT, INSERT, UPDATE ON domain_inbox TO evidence_review_worker';
  ELSE
    RAISE NOTICE 'role evidence_review_worker is absent; cross-tenant policy on domain_inbox skipped';
  END IF;
END $$;

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'evidence_review_worker') THEN
    EXECUTE 'CREATE POLICY projection_checkpoints_worker ON projection_checkpoints TO evidence_review_worker USING (true) WITH CHECK (true)';
    EXECUTE 'GRANT SELECT, INSERT, UPDATE ON projection_checkpoints TO evidence_review_worker';
  ELSE
    RAISE NOTICE 'role evidence_review_worker is absent; cross-tenant policy on projection_checkpoints skipped';
  END IF;
END $$;

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'evidence_review_worker') THEN
    EXECUTE 'CREATE POLICY artifact_records_worker ON artifact_records TO evidence_review_worker USING (true) WITH CHECK (true)';
    EXECUTE 'GRANT SELECT, INSERT, UPDATE ON artifact_records TO evidence_review_worker';
  ELSE
    RAISE NOTICE 'role evidence_review_worker is absent; cross-tenant policy on artifact_records skipped';
  END IF;
END $$;
