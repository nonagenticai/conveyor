-- 0042_document_workflow.sql — provenance, the publish workflow, and attachments.
--
-- The work store (S09) already holds text outputs on the `document` row. This migration makes the
-- row carry the facts a programme worked by several people needs:
--
--   * WHO wrote it — `created_by`/`updated_by`, nullable because the 56 outputs created before
--     this migration have no recorded author; NULL is honest and renders "unrecorded".
--   * WHEN it became public and by whom — `published_at`/`published_by`, set by the publish action.
--     Legacy published rows keep NULL rather than a guessed timestamp.
--   * a publish REQUEST — `publish_requested_at`, the author's "ready for an admin" signal. It is
--     not a state machine: `publishable` remains the one public bit.
--   * WHAT standard slot it fills — `category` (the output vocabulary) and `unit` (the programme
--     unit: a project key or an issue number). Both were free-form `meta` keys; the columns are
--     authoritative from here and the action layer mirrors them into `meta` for readers that
--     predate this migration.
--   * an ATTACHMENT — a document row whose `kind='attachment'` and whose `parent_document_id` names
--     the output it belongs to. One aggregate, as 0039 chose: the id, tenant scope, hash, ETag,
--     purge and download path are already right for a file. An attachment is public only when its
--     parent is, so it has no public bit of its own.

ALTER TABLE document
    ADD COLUMN created_by uuid,
    ADD COLUMN updated_by uuid,
    ADD COLUMN published_at timestamptz,
    ADD COLUMN published_by uuid,
    ADD COLUMN publish_requested_at timestamptz,
    ADD COLUMN parent_document_id uuid,
    ADD COLUMN category text,
    ADD COLUMN unit text,
    ADD CONSTRAINT document_created_by_fk FOREIGN KEY (created_by) REFERENCES app_user (id),
    ADD CONSTRAINT document_updated_by_fk FOREIGN KEY (updated_by) REFERENCES app_user (id),
    ADD CONSTRAINT document_published_by_fk FOREIGN KEY (published_by) REFERENCES app_user (id),
    -- A single-column FK, like `card_id`: the action layer checks that the parent is in the
    -- caller's tenant and is not itself an attachment.
    ADD CONSTRAINT document_parent_fk FOREIGN KEY (parent_document_id) REFERENCES document (id);

-- Postgres named the inline CHECK in 0013 `document_kind_check`; the attachment kind joins the
-- vocabulary rather than inventing a second table.
ALTER TABLE document DROP CONSTRAINT document_kind_check;
ALTER TABLE document ADD CONSTRAINT document_kind_check
    CHECK (kind IN ('document', 'report', 'export', 'attachment'));

-- Backfill the two promoted keys. An empty string is not a category.
--
-- ⚠️ `document` is FORCE ROW LEVEL SECURITY and a migration has no tenant context, so a bare
-- UPDATE here would match ZERO rows: `tenant_id = app_current_tenant()` is NULL. The force is
-- lifted for the statement and restored immediately; the ALTER takes an ACCESS EXCLUSIVE lock,
-- so no application read or write can observe the window. sqlx runs a migration in one
-- transaction, so a failure rolls the restore back with the statement.
ALTER TABLE document NO FORCE ROW LEVEL SECURITY;
UPDATE document
   SET category = NULLIF(btrim(meta->>'category'), ''),
       unit = NULLIF(btrim(meta->>'unit'), '')
 WHERE meta ? 'category' OR meta ? 'unit';
ALTER TABLE document FORCE ROW LEVEL SECURITY;

-- The project workspace asks "which of the expected slots does this project have?"; the register
-- filters by unit; the attachment list reads by parent; the admin queue reads requested rows.
CREATE INDEX document_category_idx
    ON document (tenant_id, project_id, category)
    WHERE category IS NOT NULL;
CREATE INDEX document_unit_idx
    ON document (tenant_id, unit)
    WHERE unit IS NOT NULL;
CREATE INDEX document_parent_idx
    ON document (tenant_id, parent_document_id)
    WHERE parent_document_id IS NOT NULL;
CREATE INDEX document_review_queue_idx
    ON document (tenant_id, publish_requested_at, updated_at DESC, id)
    WHERE publish_requested_at IS NOT NULL AND status = 'active';

-- Attachments are never listed as outputs: the output read stays `body IS NOT NULL`, and the
-- console register filters `kind='document'`. This partial index keeps the register's default
-- read off attachment rows.
CREATE INDEX document_output_idx
    ON document (tenant_id, updated_at DESC, id)
    WHERE kind = 'document' AND status = 'active';
