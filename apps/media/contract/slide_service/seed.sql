-- Synthetic pre-state for the slide_service contract capture.
--
-- One tenant-owned template with a FIXED uuid, because the reference mints every
-- row id with uuid7() and `cases.txt` is static: without this row the deck-create
-- case could only name a template that does not exist, and the render path would
-- never be exercised by a replay. The body is the reference's own `title` seed
-- template (src/seed/seed_templates.py), owned by the synthetic capture tenant
-- instead of the platform, so it is visible to exactly the X-Tenant-Id in
-- `cases.txt` and to nothing else.
--
-- Load after `alembic upgrade head` and before the first case runs.
INSERT INTO slide_templates (
    template_id, tenant_id, name, display_name, category, layout_type,
    slot_definitions, styling_rules, default_transition,
    default_timing_hint_seconds, is_system, is_active
) VALUES (
    '00000000-0000-0000-0000-0000000000d1',
    '00000000-0000-0000-0000-0000000000c1',
    'capture_title', 'Capture Title', 'capture', 'title',
    '{"title": {"type": "text", "max_length": 80, "required": true},
      "subtitle": {"type": "text", "max_length": 120, "required": false}}'::jsonb,
    '{"title_font_size": 40, "subtitle_font_size": 24, "layout": "centered"}'::jsonb,
    '{"type": "fade", "duration_seconds": 0.5}'::jsonb,
    8.0, false, true
) ON CONFLICT (template_id) DO NOTHING;
