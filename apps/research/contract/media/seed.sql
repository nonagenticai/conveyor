-- The pre-state every golden in this directory was captured against.
--
-- Run once against the reference's scratch database, after `alembic upgrade head` and before
-- `capture-goldens.sh`. Without it the capture inherits whatever the previous run left behind:
-- the first capture of this contract shipped a `collections_empty` golden that contained a row,
-- created nine seconds earlier by `collection_create_validation` in a prior run.
--
-- Every value here is synthetic. The tenant is the reference's own development default, which the
-- capture rewrites to `00000000-0000-7000-8000-000000000001` before committing (see README
-- §"Sanitised on capture") because the deployed default names a real customer.
--
-- Deliberately **no collections**: `collections_*` cases create what they delete, so a collection
-- here would make the listing case prove nothing. The collection round-trip is captured on the
-- collection `collection_create` mints.

BEGIN;

INSERT INTO media_items (
    id, tenant_id, title, description, url, media_type, source, external_ids,
    duration_seconds, thumbnail_url, author_name, author_channel_url, published_at, language,
    tags, view_count, like_count, quality_score, quality_components, metadata, embedding,
    created_at, updated_at
) VALUES
-- The subject of every `items_*` happy-path case.
('10000000-0000-7000-8000-000000000001',
 '00000000-0000-7000-8000-000000000001',
 'Synthetic video one', 'A synthetic description', 'https://example.com/media/one',
 'video', 'web_scraper', '{"example_id":"one"}'::jsonb,
 120, 'https://example.com/thumb/one.png', 'Synthetic Author', 'https://example.com/author/synthetic',
 '2026-01-02T03:04:05Z', 'en', '["synthetic","golden"]'::jsonb, 1000, 40,
 NULL, NULL, '{"seed":"one"}'::jsonb, NULL,
 '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z'),

-- Same media_type and author as the subject: what the metadata top-up in /related returns.
('10000000-0000-7000-8000-000000000002',
 '00000000-0000-7000-8000-000000000001',
 'Synthetic video two', 'A second synthetic description', 'https://example.com/media/two',
 'video', 'web_scraper', '{"example_id":"two"}'::jsonb,
 600, 'https://example.com/thumb/two.png', 'Synthetic Author', 'https://example.com/author/synthetic',
 '2026-02-02T03:04:05Z', 'en', '["synthetic"]'::jsonb, 500, 5,
 NULL, NULL, '{"seed":"two"}'::jsonb, NULL,
 '2026-02-02T03:04:05Z', '2026-02-02T03:04:05Z'),

-- A different media_type and author: /related must NOT return it for the subject.
('10000000-0000-7000-8000-000000000003',
 '00000000-0000-7000-8000-000000000001',
 'Synthetic episode three', NULL, 'https://example.com/media/three',
 'podcast', 'podcast', '{"example_id":"three"}'::jsonb,
 NULL, NULL, 'Another Author', NULL,
 NULL, NULL, '[]'::jsonb, NULL, NULL,
 NULL, NULL, '{"seed":"three"}'::jsonb, NULL,
 '2026-03-02T03:04:05Z', '2026-03-02T03:04:05Z');

INSERT INTO search_sessions (id, tenant_id, name, query, status, item_count, created_at) VALUES
('20000000-0000-7000-8000-000000000001',
 '00000000-0000-7000-8000-000000000001',
 'Seeded session', 'golden', 'completed', 2, '2026-04-02T03:04:05Z');

INSERT INTO session_items (session_id, media_item_id, discovery_method, source_adapter, quality_score) VALUES
('20000000-0000-7000-8000-000000000001', '10000000-0000-7000-8000-000000000001', 'search', 'web_scraper', NULL),
('20000000-0000-7000-8000-000000000001', '10000000-0000-7000-8000-000000000002', 'search', 'web_scraper', NULL);

COMMIT;
