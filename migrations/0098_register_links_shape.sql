-- 0098_register_links_shape.sql — §5.5: the register's links document is the declared shape
-- (#526, v1-p55-1).
--
-- Forward only: never edited after this slice closes (CONTRACT §2).
--
-- `project_register_item.links` shipped in 0044 as `jsonb NOT NULL DEFAULT '[]'`, and the only
-- internal writer appended `{kind, id}` entries to that array. SPEC_V2 §5.5 declares the
-- document as `{cards[], requirements[], decisions[]}`. Legacy rows are normalised first; the
-- CHECK is the second door under the actions' `422 links_invalid`, and the default is the empty
-- document so an omitted `links` can never store an array again.
--
-- A legacy bare array carries no typed bucket. Its entries are preserved under `requirements`
-- (an entry keeps its own `kind`, so a typed reader can still tell a benefit or objective
-- citation from a requirement), and the other two arrays start empty. A legacy object that is
-- missing one of the three arrays, or carries a non-array member, is rebuilt the same way; its
-- valid arrays are kept.
--
-- ⚠️ `project_register_item` is FORCE RLS, so the migration owner (`conveyor_migrate`) is
-- subject to `tenant_isolation` too; with `app.current_tenant` unset a bare UPDATE matches ZERO
-- rows silently and leaves the legacy arrays for the CHECK to refuse. The force is lifted for
-- the statement and restored immediately; the ALTER holds an ACCESS EXCLUSIVE lock inside
-- sqlx's migration transaction, so no application session observes the window (the pattern of
-- 0028 and 0042).

ALTER TABLE project_register_item NO FORCE ROW LEVEL SECURITY;

UPDATE project_register_item
   SET links = jsonb_build_object(
           'cards',
           CASE
               WHEN jsonb_typeof(links->'cards') = 'array' THEN links->'cards'
               ELSE '[]'::jsonb
           END,
           'requirements',
           CASE
               WHEN jsonb_typeof(links) = 'array' THEN links
               WHEN jsonb_typeof(links->'requirements') = 'array' THEN links->'requirements'
               ELSE '[]'::jsonb
           END,
           'decisions',
           CASE
               WHEN jsonb_typeof(links->'decisions') = 'array' THEN links->'decisions'
               ELSE '[]'::jsonb
           END
       )
 WHERE jsonb_typeof(links) IS DISTINCT FROM 'object'
    OR jsonb_typeof(links->'cards') IS DISTINCT FROM 'array'
    OR jsonb_typeof(links->'requirements') IS DISTINCT FROM 'array'
    OR jsonb_typeof(links->'decisions') IS DISTINCT FROM 'array';

ALTER TABLE project_register_item FORCE ROW LEVEL SECURITY;

ALTER TABLE project_register_item
    ALTER COLUMN links SET DEFAULT
        '{"cards": [], "requirements": [], "decisions": []}'::jsonb;

-- An absent member passes (the comparison is NULL), so this enforces "an array when present"
-- while the actions require all three; the objectness test is what refuses a bare array.
ALTER TABLE project_register_item
    ADD CONSTRAINT project_register_links_shape
    CHECK (
        jsonb_typeof(links) = 'object'
        AND jsonb_typeof(links->'cards') = 'array'
        AND jsonb_typeof(links->'requirements') = 'array'
        AND jsonb_typeof(links->'decisions') = 'array'
    );
