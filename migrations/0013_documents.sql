-- 0013_documents.sql — the four S09 tables: document, secret, repository, project_repository.
--
-- Column-for-column from 02-DATA-ARCHITECTURE §2.10. Forward only: never edited after this slice closes
-- (CONTRACT §2). Declared in dependency order — `secret` first, because `repository.credential_secret_id`
-- names it.
--
-- The design decision that shapes the whole slice is here, in the schema:
--
--   * **Document bytes never live in a row.** `document` is metadata; `object_key` points into the object
--     store and `content_hash` is the sha256 the bytes must reproduce on download. A bytea column would
--     TOAST every row, multiply WAL and put binaries into every dump (S09 §8.1).
--   * **`secret.ciphertext` is opaque to SQL.** It holds `nonce ‖ ct ‖ tag` sealed under the tenant's HKDF
--     key; `key_id` names the KEK generation. There is no `secret` reveal path in the action registry, and
--     the database has no way to check the plaintext — which is the point.
--   * **Repositories are references.** `url` and `installation_ref` describe where code lives; the conveyor
--     clones nothing (S09 §5.3). `credential_secret_id` is a pointer into `secret`, resolved in the caller's
--     tenant by the store — a single-column FK does not enforce tenancy (CONTRACT §2).
--
-- All four carry `tenant_id` + FORCE RLS. `document`, `secret` and `repository` are mutable aggregates and
-- carry `version`/`created_at`/`updated_at`; `project_repository` is a link row with no version (its
-- identity is the pair, and link/unlink are the only writes).

-- ── secret ─────────────────────────────────────────────────────────────────────────────────
-- Rotation bumps `version` and REPLACES the ciphertext: the old value does not survive, by design (S09 §5.4).
-- `(tenant_id, name)` is unique so a name is one secret, and `scope` is free text today (`tenant` default)
-- because the only consumers — a repository credential and a webhook signing key — both want tenant scope.
CREATE TABLE secret (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    name       text NOT NULL,
    ciphertext bytea NOT NULL,
    key_id     text NOT NULL,
    scope      text NOT NULL DEFAULT 'tenant',
    version    bigint NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT secret_tenant_name_key UNIQUE (tenant_id, name),
    CONSTRAINT secret_name_not_blank CHECK (btrim(name) <> ''),
    CONSTRAINT secret_ciphertext_not_empty CHECK (octet_length(ciphertext) > 0)
);

-- ── repository ─────────────────────────────────────────────────────────────────────────────
-- `url` is unique per tenant: the same repo linked twice would make `project_repository` ambiguous.
-- `installation_ref` is the GitHub App installation id as text — an opaque provider handle, never parsed.
CREATE TABLE repository (
    id                   uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id            uuid NOT NULL DEFAULT app_current_tenant(),
    provider             text NOT NULL DEFAULT 'github',
    url                  text NOT NULL,
    default_branch       text NOT NULL DEFAULT 'main',
    installation_ref     text,
    credential_secret_id uuid,
    version              bigint NOT NULL DEFAULT 1,
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT repository_tenant_url_key UNIQUE (tenant_id, url),
    CONSTRAINT repository_url_not_blank CHECK (btrim(url) <> ''),
    CONSTRAINT repository_credential_secret_fk
        FOREIGN KEY (credential_secret_id) REFERENCES secret (id)
);

-- ── document ───────────────────────────────────────────────────────────────────────────────
-- `kind='report'` rows are created by `documents.report` and have NO object: their numbers are live queries
-- (S09 §5.5), so `object_key` stays null for them. `kind='export'` is the S08 render artefact, written by the
-- renderer into the same bucket (S09 §10 Q3).
--
-- `status` is a tombstone machine: `delete` sets `deleted` and enqueues `document.purge`, which removes the
-- object; the row stays so the id, hash and audit trail survive the bytes.
CREATE TABLE document (
    id           uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id    uuid NOT NULL DEFAULT app_current_tenant(),
    project_id   uuid,
    kind         text NOT NULL DEFAULT 'document'
                 CHECK (kind IN ('document', 'report', 'export')),
    title        text NOT NULL,
    status       text NOT NULL DEFAULT 'active'
                 CHECK (status IN ('active', 'rejected', 'failed', 'deleted')),
    object_key   text,
    content_hash text,
    external_ref text,
    meta         jsonb NOT NULL DEFAULT '{}'::jsonb,
    version      bigint NOT NULL DEFAULT 1,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT document_project_fk FOREIGN KEY (project_id) REFERENCES project (id),
    CONSTRAINT document_title_not_blank CHECK (btrim(title) <> ''),
    CONSTRAINT document_hash_shape CHECK (content_hash IS NULL OR content_hash ~ '^[0-9a-f]{64}$')
);
CREATE INDEX document_list_idx ON document (tenant_id, project_id, kind, updated_at DESC, id);
CREATE INDEX document_kind_created_idx ON document (tenant_id, kind, created_at DESC, id);

-- ── project_repository ────────────────────────────────────────────────────────────────────
-- The link is the identity: no id column, no version. `role` is free text (`primary` default) because the
-- vocabulary is the consumer's (coding services name their own roles).
CREATE TABLE project_repository (
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    project_id    uuid NOT NULL,
    repository_id uuid NOT NULL,
    role          text NOT NULL DEFAULT 'primary',
    created_at    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (project_id, repository_id),
    CONSTRAINT project_repository_project_fk FOREIGN KEY (project_id) REFERENCES project (id),
    CONSTRAINT project_repository_repository_fk FOREIGN KEY (repository_id) REFERENCES repository (id)
);
CREATE INDEX project_repository_repo_idx ON project_repository (tenant_id, repository_id);

-- ── RLS: enabled AND forced on all four ───────────────────────────────────────────────────
-- FORCE is what makes the owner subject to the policy too; without it the schema owner reads every tenant's
-- rows (S03's rls_domain test asserts exactly this shape).
ALTER TABLE secret             ENABLE ROW LEVEL SECURITY;
ALTER TABLE secret             FORCE  ROW LEVEL SECURITY;
ALTER TABLE repository         ENABLE ROW LEVEL SECURITY;
ALTER TABLE repository         FORCE  ROW LEVEL SECURITY;
ALTER TABLE document           ENABLE ROW LEVEL SECURITY;
ALTER TABLE document           FORCE  ROW LEVEL SECURITY;
ALTER TABLE project_repository ENABLE ROW LEVEL SECURITY;
ALTER TABLE project_repository FORCE  ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON secret FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON repository FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON document FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON project_repository FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
