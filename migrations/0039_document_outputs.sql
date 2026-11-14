-- 0039_document_outputs.sql — text outputs on the document row.
--
-- Why one table and not a second aggregate: an intermediate output (a design note, an evidence
-- record, a decision) is a document whose content is small Markdown held in the row, not bytes in
-- the object store. Keeping it on `document` preserves the id, tenant scope, version/ETag, status
-- machine, list/get/update actions and audit trail; `body` is the only new content. `kind` stays
-- 'document', so the existing kind vocabulary is unchanged.
--
-- The contract this schema carries:
--   * a row has bytes (`object_key`) or a body, never both; both may be null for a `report`,
--     whose numbers are live queries (S09 §5.5);
--   * `body` is capped at 256 KiB — an output is a note, not a file;
--   * `publishable` defaults false: existence is not publication, a person publishes it;
--   * `card_id` is a single-column FK. A FK does not enforce tenancy or that the card's board
--     belongs to `project_id`, so the action layer checks both in the caller's tenant (CONTRACT §2).

ALTER TABLE document
    ADD COLUMN card_id     uuid,
    ADD COLUMN body        text,
    ADD COLUMN publishable boolean NOT NULL DEFAULT false,
    ADD CONSTRAINT document_card_fk
        FOREIGN KEY (card_id) REFERENCES card (id),
    ADD CONSTRAINT document_content_shape
        CHECK (body IS NULL OR object_key IS NULL),
    ADD CONSTRAINT document_body_size
        CHECK (body IS NULL OR octet_length(body) <= 262144);

-- The console lists one project's outputs and one card's outputs, newest first.
CREATE INDEX document_card_idx
    ON document (tenant_id, card_id, updated_at DESC, id)
    WHERE card_id IS NOT NULL;

-- The public snapshot reads only published, active rows. The partial index keeps that read off
-- the full document history.
CREATE INDEX document_publishable_idx
    ON document (tenant_id, updated_at DESC, id)
    WHERE publishable AND status = 'active';
