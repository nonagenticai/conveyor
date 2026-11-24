-- 0072_tool_meta.sql — the tool's verification declaration (SPEC_V2 §11.3, §11.6; issue #557).
--
-- A write or destructive tool must declare the sibling read tool that verifies the write. The
-- declaration is per-tool contract data stored beside the tool version; a published row is
-- immutable, so a changed declaration is a new toolkit version (`content_hash` covers it).
--
-- Forward only: never edited after this slice closes (CONTRACT §2).
ALTER TABLE tool ADD COLUMN meta jsonb NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE tool ADD CONSTRAINT tool_meta_is_object CHECK (jsonb_typeof(meta) = 'object');
