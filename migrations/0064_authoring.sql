-- 0064_authoring.sql — T17 (#87, ported from #526): the tenant-asset authoring family
-- (SPEC_V2 §5.7, §10.4;
-- finding v1-w3b-3).
--
-- An intake is the tenant's brief plus its documents; every asset the authoring pipeline
-- produces is one `authoring_asset` draft citing that intake. The lifecycle is
-- draft → reviewed | rejected → published:
--   * `draft` is written by `asset_draft_tx`;
--   * `reviewed` / `rejected` are set only by `asset_review_tx`, which requires an
--     `authoring.review` approval carrying a HUMAN decision for this exact row
--     (SPEC_V2 §5.7: the chat surface does not decide);
--   * `published` is set only by `asset_publish_tx`, from `reviewed`, and
--     `uq_authoring_asset_published` is the one-published-row-per-(tenant, kind, key)
--     guarantee (a check-then-update could race).
--
-- The payload CHECKs are §5.7's hard rule at the second lock: a chat message or a document can
-- never grant a tool, widen an effect ceiling, approve its own artefact or change an approval
-- policy. `crates/store/src/authoring.rs` refuses the same shape first so a caller gets a typed
-- error; the CHECK is what makes a hand-written INSERT obey it too.
--
-- The dependency order of §5.7 (industry → department → job/capability → procedure → knowledge
-- pack → policies/templates → tools) is not a DB constraint: a dependency is named in
-- `depends_on` as an array of asset ids, and the store refuses a draft whose named dependency is
-- not a published asset in the same tenant. `depends_on` is JSONB rather than a join table so a
-- draft can cite ids before either row is touched by a lock; the store re-checks on publish.
--
-- Every table is tenant-scoped with `app_current_tenant()`, carries `version` for a later
-- If-Match, and is FORCE RLS — the same shape as 0044's plan spine and 0054's architecture.
--
-- Forward only: never edited after this slice closes (CONTRACT §2).

-- ── authoring_intake — the tenant's brief and the documents behind it ────────────────────
CREATE TABLE authoring_intake (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    brief       text NOT NULL,
    sources     jsonb NOT NULL DEFAULT '[]'::jsonb,
    status      text NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'closed')),
    version     bigint NOT NULL DEFAULT 1,
    created_by  uuid NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT authoring_intake_tenant_fk FOREIGN KEY (tenant_id)
        REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT authoring_intake_created_by_fk FOREIGN KEY (created_by) REFERENCES app_user (id),
    -- The store trims before it writes; this is the lock against a hand-written blank.
    CONSTRAINT authoring_intake_brief_not_blank CHECK (btrim(brief) <> ''),
    CONSTRAINT authoring_intake_sources_is_array CHECK (jsonb_typeof(sources) = 'array')
);
CREATE INDEX authoring_intake_updated_idx ON authoring_intake (tenant_id, updated_at DESC, id);

ALTER TABLE authoring_intake ENABLE ROW LEVEL SECURITY;
ALTER TABLE authoring_intake FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON authoring_intake FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── authoring_asset — one draft of one tenant asset, produced from an intake ─────────────
-- `key` is the tenant's own identifier for the asset (`engineering`, `sop-onboard`, …); the
-- partial unique index makes "one published version per (kind, key)" the database's promise,
-- while drafts of a key may coexist.
CREATE TABLE authoring_asset (
    id                 uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id          uuid NOT NULL DEFAULT app_current_tenant(),
    intake_id          uuid,
    kind               text NOT NULL
                       CHECK (kind IN ('industry', 'department', 'job', 'capability', 'procedure',
                                       'knowledge_pack', 'project_template', 'policy',
                                       'tool_request')),
    key                text NOT NULL,
    payload            jsonb NOT NULL,
    depends_on         jsonb NOT NULL DEFAULT '[]'::jsonb,
    status             text NOT NULL DEFAULT 'draft'
                       CHECK (status IN ('draft', 'reviewed', 'rejected', 'published')),
    review_approval_id uuid,
    published_at       timestamptz,
    version            bigint NOT NULL DEFAULT 1,
    created_by         uuid NOT NULL,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT authoring_asset_tenant_fk FOREIGN KEY (tenant_id)
        REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT authoring_asset_intake_fk FOREIGN KEY (intake_id)
        REFERENCES authoring_intake (id),
    CONSTRAINT authoring_asset_review_approval_fk FOREIGN KEY (review_approval_id)
        REFERENCES approval (id),
    CONSTRAINT authoring_asset_created_by_fk FOREIGN KEY (created_by) REFERENCES app_user (id),
    CONSTRAINT authoring_asset_key_not_blank CHECK (btrim(key) <> ''),
    CONSTRAINT authoring_asset_payload_is_object CHECK (jsonb_typeof(payload) = 'object'),
    CONSTRAINT authoring_asset_payload_no_authority CHECK (
        NOT (payload ? 'approval_policy'
             OR payload ? 'effect_ceiling'
             OR payload ? 'grants'
             OR payload ? 'self_approval')
    ),
    CONSTRAINT authoring_asset_depends_on_is_array CHECK (jsonb_typeof(depends_on) = 'array'),
    CONSTRAINT authoring_asset_published_has_time
        CHECK (status <> 'published' OR published_at IS NOT NULL)
);
CREATE INDEX authoring_asset_kind_status_idx ON authoring_asset (tenant_id, kind, status);
CREATE INDEX authoring_asset_updated_idx ON authoring_asset (tenant_id, updated_at DESC, id);
-- One PUBLISHED row per (tenant, kind, key). The store maps a violation of this index to
-- `AlreadyPublished`; concurrent publishers lose here, never at a read.
CREATE UNIQUE INDEX uq_authoring_asset_published
    ON authoring_asset (tenant_id, kind, key) WHERE status = 'published';

ALTER TABLE authoring_asset ENABLE ROW LEVEL SECURITY;
ALTER TABLE authoring_asset FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON authoring_asset FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
