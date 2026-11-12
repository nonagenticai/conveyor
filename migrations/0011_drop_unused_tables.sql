-- 0011 — the S00 day-2 DROP verdicts, executed.
--
-- The usage inventory was specified to run FIRST (SPEC §8 S0) and was completed on 2026-11-08, after these
-- tables had already been built by S06/S07/S08. Its verdict for all seven is DROP, on measured evidence
-- (parity/evidence/02-day2-method.txt §9 "PENDING-S0 — the 13 tables…" (lines 188-222 at 2026-11-08)):
--
--   audience, audience_member  no `streams` node operation in any of the 348 flows; fr.streams 0 rows;
--                              asset_stream_membership 0; fr has no /api/streams route at all
--   outline, outline_node      RM toc_trees/toc_nodes/toc_settings 0; fr project_outlines 0; no `toc`
--                              operation in any flow; fr /toc/* = 15 unused + 1 rare
--   deliverable                project_deliverables 0, project_milestones 0, no deliverable operation
--   project_capability         pm_project_capabilities 0, no projectCapability operation
--   register_item              pm_resources / pm_risks / pm_benefits all 0, no register route at all
--
-- `project_stakeholder` (21 live rows) and `custom_field*`/`webhook_subscription`/`conversation*` KEEP.
--
-- Child first, and deliberately without CASCADE: nothing outside this set references these tables (the only
-- inbound FKs are outline_node→outline and audience_member→audience), so a CASCADE could only ever hide an
-- unexpected dependency. Policies, indexes, constraints and table-level grants fall with each table.
--
-- This is a one-way door in the new database. The old RM/fr/Planka databases stay read-only until S14, so the
-- verdict is reversible by re-porting a feature — a new migration, not an edit to an applied one.

DROP TABLE outline_node;
DROP TABLE outline;
DROP TABLE audience_member;
DROP TABLE audience;
DROP TABLE deliverable;
DROP TABLE project_capability;
DROP TABLE register_item;
