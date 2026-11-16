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

-- A second FIXED uuid for the image path, same reason: `cases.txt` cannot name a
-- uuid7 minted at seed time. The layout_type is the reference's own image family
-- (`feature_spotlight`), which draws `image_url` through Chromium; `deck_create_image`
-- supplies a data-URL PNG so the image slot is exercised by every replay. The body is
-- synthetic (a capture-owned template), not a platform row.
INSERT INTO slide_templates (
    template_id, tenant_id, name, display_name, category, layout_type,
    slot_definitions, styling_rules, default_transition,
    default_timing_hint_seconds, is_system, is_active
) VALUES (
    '00000000-0000-0000-0000-0000000000d2',
    '00000000-0000-0000-0000-0000000000c1',
    'capture_image', 'Capture Image', 'capture', 'feature_spotlight',
    '{"title": {"type": "text", "max_length": 80, "required": true},
      "description": {"type": "text", "max_length": 200, "required": false},
      "image_url": {"type": "image", "required": true},
      "bullets": {"type": "list", "required": false}}'::jsonb,
    '{"title_font_size": 36, "layout": "hero"}'::jsonb,
    '{"type": "fade", "duration_seconds": 0.5}'::jsonb,
    8.0, false, true
) ON CONFLICT (template_id) DO NOTHING;
