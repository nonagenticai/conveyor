-- 0070_tool_authoring.sql — T15: tool authoring and BYO tools (SPEC_V2 §11.3–§11.6, §11.8,
-- D42, D43, D50; issue #86).
--
-- Forward only: never edited after this slice closes (CONTRACT §2).
--
-- The tool platform (0055) owns the four callable tables. This migration adds the authoring
-- side of the estate:
--
--   * `governed_asset(kind='tool_draft')` — an imported OpenAPI document or a BYO MCP
--     discovery, the draft the authoring pipeline validates, reviews and publishes (§11.6).
--     The lifecycle and the revision table are 0002's; only the vocabulary widens.
--   * `tool_source` — the tenant's registered destination: an OpenAPI base URL or a
--     streamable-HTTP MCP endpoint, with the egress allowlist and the credential alias that
--     were reviewed at registration. A call never carries a URL (§11.3 step 5).
--   * `tool.source_id` / `tool.destination` — link a published operation to its registered
--     source and to the one operation it dispatches (method+path, or an MCP tool name).
--   * `tool_binding.tool_keys` — binding-scoped discovery: a BYO binding that names a subset
--     exposes only that subset to the job (§11.5 item 2). Empty means every published tool
--     of the pinned version, which is what every estate toolkit binding means.
--   * `tool_call.binding_id` — the grant a call was authorised by, so the ledger answers
--     "which binding made this callable" without a join through the run.
--   * `tool_promotion` — the platform-catalogue promotion request. Only a development project
--     with an approved platform review may open one; a tenant admin is refused (§11.5).
--   * `approval_policy.required_decisions` — the publication gate's quorum. The approver kind
--     and expiry already live on this row (0068); the count is the one term T15 adds, so a
--     tenant changes the publication gate with no code change (D50).
--   * `approval` — one partial unique index: one pending authoring gate per draft.

-- ── governed_asset — a tool draft is a kind, not a new table ────────────────────────────
ALTER TABLE governed_asset DROP CONSTRAINT governed_asset_kind_check;
ALTER TABLE governed_asset ADD CONSTRAINT governed_asset_kind_check CHECK (
    kind IN ('requirement', 'principle', 'adr', 'charter', 'closure', 'postmortem',
             'sop_source', 'mandate', 'programme_brief', 'blueprint', 'business_case',
             'refined_vision', 'benefits_strategy', 'benefits_realization_plan',
             'projects_dossier', 'minutes', 'tool_draft')
);

-- ── approval_policy — the publication gate's quorum ──────────────────────────────────────
ALTER TABLE approval_policy
    ADD COLUMN required_decisions int NOT NULL DEFAULT 1;
ALTER TABLE approval_policy
    ADD CONSTRAINT approval_policy_required_decisions_range
        CHECK (required_decisions BETWEEN 1 AND 255);

-- One pending authoring gate per draft. `uq_appr_pending` is (tenant, run_id, gate) and treats
-- a NULL run_id as distinct, and a publication gate belongs to no run — this is its guarantee.
CREATE UNIQUE INDEX uq_appr_tool_authoring_pending
    ON approval (tenant_id, gate, (meta ->> 'draft_id'))
    WHERE status = 'pending' AND kind = 'authoring';

-- ── tool_source — the tenant's registered destination ────────────────────────────────────
-- The URL is reviewed at registration: scheme + host, no wildcard anywhere. `egress` is the
-- host allowlist the broker re-checks at call time; an entry of `*` is refused by the action
-- and by the CHECK below.
CREATE TABLE tool_source (
    id               uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id        uuid NOT NULL DEFAULT app_current_tenant(),
    -- The destination alias the toolkit's `service` names (`weather`), never a URL at call time.
    key              text NOT NULL,
    title            text NOT NULL,
    kind             text NOT NULL CHECK (kind IN ('openapi', 'mcp')),
    -- `https://api.example.com` or `http://127.0.0.1:8080` — a base, not an operation.
    base_url         text NOT NULL,
    auth_adapter     text NOT NULL DEFAULT 'bearer'
                     CHECK (auth_adapter IN ('bearer', 'none', 'sealed-profile')),
    -- The sealed secret's name (`weather.token`); never the value.
    credential_alias text,
    -- The allowlisted hosts (`["api.example.com"]`); a wildcard is never an allowlist.
    egress           jsonb NOT NULL DEFAULT '[]'::jsonb,
    -- `published` at registration: the registering caller (tools:write) is the reviewer.
    status           text NOT NULL DEFAULT 'published'
                     CHECK (status IN ('draft', 'published', 'retired')),
    meta             jsonb NOT NULL DEFAULT '{}'::jsonb,
    version          bigint NOT NULL DEFAULT 1,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT tool_source_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id)
        ON DELETE CASCADE,
    CONSTRAINT tool_source_tenant_key_key UNIQUE (tenant_id, key),
    CONSTRAINT tool_source_key_shape CHECK (key ~ '^[a-z][a-z0-9_]*$'),
    CONSTRAINT tool_source_base_url_shape
        CHECK (base_url ~ '^https?://[^/*[:space:]]+(/[^[:space:]*]*)*$'),
    CONSTRAINT tool_source_credential_not_blank
        CHECK (credential_alias IS NULL OR btrim(credential_alias) <> ''),
    CONSTRAINT tool_source_egress_is_array CHECK (jsonb_typeof(egress) = 'array'),
    -- Any wildcard is a wildcard: `"*"` and `"*.example.com"` are both refused as text.
    CONSTRAINT tool_source_egress_has_no_wildcard
        CHECK (egress::text NOT LIKE '%*%' AND base_url NOT LIKE '%*%'),
    CONSTRAINT tool_source_meta_is_object CHECK (jsonb_typeof(meta) = 'object')
);

CREATE INDEX tool_source_tenant_kind_idx ON tool_source (tenant_id, kind, status);

ALTER TABLE tool_source ENABLE ROW LEVEL SECURITY;
ALTER TABLE tool_source FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON tool_source FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── tool — the registered source and the one operation it dispatches ────────────────────
ALTER TABLE tool
    ADD COLUMN source_id   uuid,
    ADD COLUMN destination jsonb NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE tool
    ADD CONSTRAINT tool_source_fk FOREIGN KEY (source_id) REFERENCES tool_source (id),
    ADD CONSTRAINT tool_destination_is_object CHECK (jsonb_typeof(destination) = 'object');

-- ── tool_binding — the bound subset (empty = every published tool of the version) ───────
ALTER TABLE tool_binding
    ADD COLUMN tool_keys text[] NOT NULL DEFAULT '{}';
ALTER TABLE tool_binding
    ADD CONSTRAINT tool_binding_tool_keys_no_nulls
        CHECK (array_position(tool_keys, NULL) IS NULL);

-- ── tool_call — the binding that authorised the call ────────────────────────────────────
ALTER TABLE tool_call ADD COLUMN binding_id uuid;
ALTER TABLE tool_call
    ADD CONSTRAINT tool_call_binding_fk FOREIGN KEY (binding_id) REFERENCES tool_binding (id)
        ON DELETE SET NULL;
CREATE INDEX tool_call_binding_idx ON tool_call (tenant_id, binding_id, created_at DESC, id);

-- ── tool_promotion — the platform-catalogue promotion request ───────────────────────────
-- A tenant admin cannot promote a tenant toolkit into the platform catalogue. The sanctioned
-- path is a development project whose review gate (T6's `development_projects.review`) is
-- approved; this row is that request, and it never itself publishes anything (§11.5).
CREATE TABLE tool_promotion (
    id                     uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id              uuid NOT NULL DEFAULT app_current_tenant(),
    toolkit_id             uuid NOT NULL,
    development_project_id uuid NOT NULL,
    review_approval_id     uuid NOT NULL,
    status                 text NOT NULL DEFAULT 'requested'
                           CHECK (status IN ('requested', 'withdrawn', 'accepted', 'refused')),
    requested_by           uuid,
    meta                   jsonb NOT NULL DEFAULT '{}'::jsonb,
    version                bigint NOT NULL DEFAULT 1,
    created_at             timestamptz NOT NULL DEFAULT now(),
    updated_at             timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT tool_promotion_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id)
        ON DELETE CASCADE,
    CONSTRAINT tool_promotion_toolkit_fk FOREIGN KEY (toolkit_id) REFERENCES toolkit (id),
    CONSTRAINT tool_promotion_project_fk FOREIGN KEY (development_project_id)
        REFERENCES development_project (id) ON DELETE CASCADE,
    CONSTRAINT tool_promotion_review_fk FOREIGN KEY (review_approval_id) REFERENCES approval (id),
    CONSTRAINT tool_promotion_meta_is_object CHECK (jsonb_typeof(meta) = 'object')
);

CREATE UNIQUE INDEX tool_promotion_open_key
    ON tool_promotion (tenant_id, toolkit_id)
    WHERE status = 'requested';
CREATE INDEX tool_promotion_tenant_idx
    ON tool_promotion (tenant_id, status, created_at DESC, id);

ALTER TABLE tool_promotion ENABLE ROW LEVEL SECURITY;
ALTER TABLE tool_promotion FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON tool_promotion FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
