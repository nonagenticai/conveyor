-- 0102_phase_e_h_artifacts.sql — the ADM phases E–H in the artefact standard (#526, SPEC_V2 §8.1).
--
-- Forward only: never edited after this slice closes (CONTRACT §2).
--
-- Why the rows exist: the adopted profile already names E–H and their contract kinds
-- (`f = delivery_contract`, `g = statement_of_intent`, `h = request_for_architecture`), but
-- `artifact_type` stopped at D. A phase-E/F/H requirement could therefore never resolve to a
-- typed, approvable artefact: the generator emitted an absent finding for a type the standard
-- did not contain, and no row could ever satisfy it.
--
-- Tailoring (owner decision 2026-11-24): every delivery project carries Preliminary, A–D and a
-- Phase-G review; the programme (architecture-track) projects also carry E–H. The standard is
-- programme-wide, so the rows are present for every tenant; the audit and the generator apply
-- them per track, never to a delivery project.
--
-- Kinds are the closed vocabulary in `crates/domain/src/architecture.rs` (`catalog`, `matrix`,
-- `diagram`, `principles_catalog`); a plan or a review is a catalog row, a requirements set a
-- matrix, a diagram a diagram.
--
-- No seed rows: the rows are standard types for every tenant, inserted idempotently.
--
-- `artifact_type` is FORCE RLS, so the INSERT ... SELECT runs with `app.current_tenant` unset and
-- the policy refuses it. Backfill under NO FORCE and restore FORCE (the 0096/0098 pattern).

ALTER TABLE artifact_type NO FORCE ROW LEVEL SECURITY;

INSERT INTO artifact_type
    (tenant_id, key, name, artifact_kind, phase_code, plateau, mandatory, position)
SELECT t.id, v.key, v.name, v.artifact_kind, v.phase_code, v.plateau, true, v.position
  FROM tenant t
 CROSS JOIN (VALUES
    ('gap_analysis',                  'Gap Analysis Matrix',                    'matrix',  'e', 'hld', 10),
    ('candidate_solutions',           'Candidate Solutions Catalog',            'catalog', 'e', 'hld', 11),
    ('project_context_diagram',       'Project Context Diagram',                'diagram', 'e', 'hld', 12),
    ('benefits_diagram',              'Benefits Diagram',                       'diagram', 'e', 'hld', 13),
    ('implementation_migration_plan', 'Implementation and Migration Plan',      'catalog', 'f', 'hld', 14),
    ('compliance_review',             'Phase-G Compliance Review',              'catalog', 'g', 'hld', 15),
    ('architecture_change_request',   'Architecture Change Request',            'catalog', 'h', 'hld', 16),
    ('architecture_requirement',      'Architecture Requirement',               'catalog', 'h', 'hld', 17)
 ) AS v(key, name, artifact_kind, phase_code, plateau, position)
ON CONFLICT (tenant_id, key, phase_code, plateau) DO NOTHING;

ALTER TABLE artifact_type FORCE ROW LEVEL SECURITY;
