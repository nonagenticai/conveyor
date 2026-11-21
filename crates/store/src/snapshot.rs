//! Read-only, public-safe snapshots of a tenant — the data the design site renders.
//!
//! Every query selects an explicit column list; `SELECT *` is never used, so a column added by a
//! later migration cannot leak into a public file by accident. The secret table is read for
//! **metadata only** (`name`, `scope`, timestamps) — never `ciphertext`, never `key_id`. User
//! rows carry a display name, never an email or an OIDC subject.
//!
//! The caller runs this inside one `BEGIN READ ONLY` transaction scoped to the tenant
//! ([`crate::begin_scoped`] + `SET TRANSACTION READ ONLY`), so the whole snapshot is one
//! consistent read and no statement can write.
//!
//! Postgres renders each row set with `jsonb_agg(to_jsonb(t) ORDER BY …)`: the JSONB text form
//! is normalised by the server, so the same rows produce the same bytes on every run. Any
//! ordering that matters is in the `ORDER BY`.

use std::collections::HashSet;

use conveyor_domain::TenantId;
use serde_json::{Value, json};
use sqlx::PgConnection;

/// The six public files plus the data every file is built from.
#[derive(Debug)]
pub struct SnapshotData {
    /// `programme.json` data: the programme and project rows.
    pub programme: Value,
    /// `organisation.json` data: the organisation spine (empty before T0 lands).
    pub organisation: Value,
    /// `config.json` data: catalogue, capabilities, agents, procedures, prompts, schedules.
    pub config: Value,
    /// `work.json` data: boards, lists, cards, runs, approvals, comments, people.
    pub work: Value,
    /// `msp.json` data: the programme-definition records — benefit profiles, the benefits map,
    /// realization, stakeholder profiles, tranches and their reviews, baselines, cadence, WBS,
    /// registers and typed project dependencies.
    pub msp: Value,
    /// `artefacts.json` data: governed assets and document metadata.
    pub artefacts: Value,
    /// `architecture.json` data: the TOGAF ADM capability's rows — profiled standard, artefacts
    /// with their entries and links, contracts, the board, the compliance reviews and waivers,
    /// and the two assessment baselines.
    pub architecture: Value,
    /// `ops.json` data: the incident cards the T7 lifecycle writes — service, severity, the four
    /// timestamps and the runbook/postmortem links. Never the triage narrative or the evidence:
    /// those are the operator's text, not the programme's public record.
    pub ops: Value,
    /// `measure.json` data: inference aggregates (no prompt or response bodies exist).
    pub measure: Value,
    /// Secret **metadata** — never a value, never a key id.
    pub secrets: Value,
}

/// Read one tenant's snapshot. The caller owns the transaction and must set it read-only.
pub async fn snapshot_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<SnapshotData, sqlx::Error> {
    let tenant_id = tenant.as_uuid();

    let programme = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.name), '[]'::jsonb) FROM (
             SELECT id, name, status, owner_user_id, version, created_at, updated_at
               FROM programme WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let mut projects = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.key), '[]'::jsonb) FROM (
             SELECT p.id, p.key, p.name, p.description, p.status, p.programme_id,
                    p.project_type_code, p.phase_code, p.repo_url, p.meta, p.version,
                    p.created_at, p.updated_at,
                    (SELECT count(*) FROM board b WHERE b.project_id = p.id) AS board_count
               FROM project p WHERE p.tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;

    let mut boards = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.project_id, t.position, t.id), '[]'::jsonb) FROM (
             SELECT id, project_id, name, position, version, created_at
               FROM board WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let mut lists = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.board_id, t.position, t.id), '[]'::jsonb) FROM (
             SELECT id, board_id, name, position, kind, color, wip_limit, version
               FROM list WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let mut cards = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.created_at, t.id), '[]'::jsonb) FROM (
             SELECT c.id, c.board_id, c.list_id, c.card_type_id, c.parent_card_id, c.name,
                    c.position, c.due_at, c.archived_at, c.assignee_id,
                    u.display_name AS assignee_name, c.list_changed_at, c.created_by,
                    c.version, c.created_at, c.updated_at,
                    -- The plan seed's named keys only. `card.meta` is free-form JSONB and may
                    -- hold a person's text; the public payload never selects the whole object.
                    c.meta ->> 'unit_key' AS unit_key,
                    c.meta -> 'issue' AS issue,
                    c.meta ->> 'process_key' AS process_key,
                    c.meta ->> 'plan' AS plan
               FROM card c LEFT JOIN app_user u ON u.id = c.assignee_id
              WHERE c.tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let card_types = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.code), '[]'::jsonb) FROM (
             SELECT id, code, label, definition, is_active, version
               FROM card_type WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let mut runs = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.created_at, t.id), '[]'::jsonb) FROM (
             SELECT id, card_id, project_id, status, attempt, error_class, process_id,
                    workflow_id, workflow_step_id, capability_id, capability_agent_id,
                    parent_run_id, step_id, depth, process_version, started_at, finished_at,
                    version, created_at, updated_at
               FROM run WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let mut run_steps = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.run_id, t.position, t.attempt, t.id), '[]'::jsonb) FROM (
             SELECT id, run_id, process_step_id, position, attempt, action, status, node_id,
                    input_hash, output_hash, duration_ms, procedure_version, started_at,
                    finished_at, created_at, updated_at
               FROM run_step WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let mut approvals = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.created_at, t.id), '[]'::jsonb) FROM (
             SELECT id, run_id, card_id, project_id, gate, kind, status, required_decisions,
                    requested_by, expires_at, version, created_at, updated_at
               FROM approval WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let mut approval_decisions = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.created_at, t.id), '[]'::jsonb) FROM (
             SELECT d.id, d.approval_id, d.approver_id, u.display_name AS approver_name,
                    d.decision, d.decided_by_kind, d.via, d.created_at
               FROM approval_decision d LEFT JOIN app_user u ON u.id = d.approver_id
              WHERE d.tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let mut comments = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.created_at, t.id), '[]'::jsonb) FROM (
             SELECT c.id, c.card_id, c.author_id, u.display_name AS author_name,
                    c.edited_at, c.version, c.created_at, c.updated_at
               FROM comment c LEFT JOIN app_user u ON u.id = c.author_id
              WHERE c.tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let people = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.display_name, t.id), '[]'::jsonb) FROM (
             SELECT u.id, u.display_name, u.kind, u.status
               FROM app_user u
              WHERE EXISTS (SELECT 1 FROM membership m
                             WHERE m.user_id = u.id AND m.tenant_id = $1)
                 OR EXISTS (SELECT 1 FROM card c
                             WHERE c.assignee_id = u.id AND c.tenant_id = $1)
         ) t",
        tenant_id,
    )
    .await?;

    let catalogue = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.kind, t.sort, t.code), '[]'::jsonb) FROM (
             SELECT id, kind, code, label, meta, sort, is_active, version
               FROM catalogue_entry WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let capabilities = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.code), '[]'::jsonb) FROM (
             SELECT id, code, label, description, status, kind, meta, version
               FROM capability WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let agents = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.key), '[]'::jsonb) FROM (
             SELECT id, capability_id, key, status, role, model_ref, prompt_ref, params,
                    allowed_actions, max_iterations, timeout_secs, version
               FROM capability_agent WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let processes = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.key, t.version), '[]'::jsonb) FROM (
             SELECT id, key, title, description, status, version, card_type_id, owner_user_id,
                    capability_id, list_kind, project_type_id, outputs, created_at, updated_at
               FROM process WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let process_steps = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.process_id, t.position), '[]'::jsonb) FROM (
             SELECT id, process_id, position, action, input_template, gate_kind, on_result,
                    max_attempts, kind, actor, capability_id, process_agent_id, procedure_id,
                    child_process_key, created_at, updated_at
               FROM process_step WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let procedures = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.key, t.version), '[]'::jsonb) FROM (
             SELECT id, key, title, description, status, version, definition_of_ready,
                    definition_of_done, acceptance_criteria, inspection, outputs,
                    created_at, updated_at
               FROM procedure WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let prompts = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.key, t.version), '[]'::jsonb) FROM (
             SELECT id, key, version, status, title
               FROM prompt WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let prompt_sections = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.prompt_id, t.position), '[]'::jsonb) FROM (
             SELECT id, prompt_id, position, source_kind, required, template
               FROM prompt_section WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let schedules = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.kind), '[]'::jsonb) FROM (
             SELECT id, kind, cron, timezone, enabled, next_run_at, last_run_at, version
               FROM schedule WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;

    let assets = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.kind, t.key), '[]'::jsonb) FROM (
             SELECT id, kind, key, title, status, version, supersedes_id,
                    superseded_by_id, project_id, owner_user_id, phase_code, effort, reach,
                    impact, confidence, links, created_at, updated_at
               FROM governed_asset WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    // Only published, active outputs. The public snapshot is the one place a document leaves the
    // tenant, so the filter is the publication decision — never "every row that exists". The
    // projection is a white-list: id, owner, title and timestamps. A body, a content hash or free
    // metadata would be a second, unintended disclosure.
    let mut documents = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.created_at, t.id), '[]'::jsonb) FROM (
             SELECT id, project_id, card_id, kind, title, category, unit, publishable,
                    updated_at, created_at
               FROM document
              WHERE tenant_id = $1 AND published_at IS NOT NULL AND status = 'active'
         ) t",
        tenant_id,
    )
    .await?;

    let inference_totals: Value = sqlx::query_scalar(
        "SELECT jsonb_build_object(
             'calls', count(*),
             'prompt_tokens', COALESCE(sum(prompt_tokens), 0),
             'completion_tokens', COALESCE(sum(completion_tokens), 0),
             'total_tokens', COALESCE(sum(total_tokens), 0),
             'cache_hit_calls', COALESCE(sum((cache_status = 'hit')::int), 0),
             'cache_write_calls', COALESCE(sum((cache_status = 'write')::int), 0),
             'cache_miss_calls', COALESCE(sum((cache_status = 'miss')::int), 0),
             'unknown_cache_calls', COALESCE(sum((cache_status = 'unknown')::int), 0),
             'non_ok_calls', COALESCE(sum((outcome <> 'ok')::int), 0)
         ) FROM inference_call WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_one(&mut *conn)
    .await?;
    let inference_by_model = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.model), '[]'::jsonb) FROM (
             SELECT model, count(*) AS calls, sum(prompt_tokens) AS prompt_tokens,
                    sum(completion_tokens) AS completion_tokens, sum(total_tokens) AS total_tokens,
                    COALESCE(sum((cache_status = 'hit')::int), 0) AS cache_hits
               FROM inference_call WHERE tenant_id = $1 GROUP BY model
         ) t",
        tenant_id,
    )
    .await?;
    let inference_by_caller = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.caller_kind), '[]'::jsonb) FROM (
             SELECT caller_kind, count(*) AS calls, sum(total_tokens) AS total_tokens
               FROM inference_call WHERE tenant_id = $1 GROUP BY caller_kind
         ) t",
        tenant_id,
    )
    .await?;

    // Secret NAMES can reveal tenants and integrations, so the public snapshot carries only an
    // aggregate: how many secrets exist, which scopes they occupy and when one last changed.
    let secrets: Value = sqlx::query_scalar(
        "SELECT jsonb_build_object(
             'count', count(*),
             'scopes', COALESCE(jsonb_agg(DISTINCT scope), '[]'::jsonb),
             'last_rotated_at', max(updated_at)
         ) FROM secret WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_one(&mut *conn)
    .await?;

    // The MSP records (#305). Explicit columns: a later migration's column cannot leak in. The
    // actor ids are joined to a display name, matching the work section's convention.
    let benefit_profiles = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.reference, t.id), '[]'::jsonb) FROM (
             SELECT id, programme_id, project_id, register_item_id, reference, description,
                    category, objectives, observable_outcome, kpis, baseline, target,
                    measurement_method, financial_value, cost, capabilities, preconditions,
                    risks, owner_job_id, benefit_owner_job_id, operational_area, status, version,
                    created_at, updated_at
               FROM benefit_profile WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let benefit_links = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.benefit_id, t.created_at, t.id), '[]'::jsonb) FROM (
             SELECT id, programme_id, output_ref, capability_ref, operational_outcome, benefit_id,
                    strategic_objective, kind, version, created_at
               FROM benefit_link WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let benefit_transitions = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.benefit_id, t.created_at, t.id), '[]'::jsonb) FROM (
             SELECT id, benefit_id, baseline_measured_at, readiness, enacted_at, post_measured_at,
                    retired_at, evidence, version, created_at, updated_at
               FROM benefit_transition WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let stakeholder_profiles = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.name, t.id), '[]'::jsonb) FROM (
             SELECT id, programme_id, project_id, name, concerns, sensitivities, importance,
                    influence, interest, exposure, support_level, support_trend,
                    engagement_target, benefits, disbenefits, version, created_at, updated_at
               FROM stakeholder_profile WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let tranches = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.programme_id, t.number), '[]'::jsonb) FROM (
             SELECT id, programme_id, number, state, entry_criteria, exit_criteria, baseline_ref,
                    review_at, authorised_at, authorisation_review_id, version, created_at,
                    updated_at
               FROM programme_tranche WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let tranche_reviews = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.tranche_id, t.reviewed_at, t.id), '[]'::jsonb) FROM (
             SELECT r.id, r.tranche_id, r.decision, r.evidence, r.reviewed_by,
                    u.display_name AS reviewer_name, r.reviewed_at, r.version
               FROM programme_tranche_review r
               LEFT JOIN app_user u ON u.id = r.reviewed_by
              WHERE r.tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let programme_baselines = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.programme_id, t.kind, t.taken_at, t.id), '[]'::jsonb) FROM (
             SELECT b.id, b.programme_id, b.kind, b.snapshot, b.taken_at, b.taken_by,
                    ub.display_name AS taken_by_name, b.approved_by,
                    ua.display_name AS approved_by_name, b.version, b.created_at
               FROM programme_baseline b
               LEFT JOIN app_user ub ON ub.id = b.taken_by
               LEFT JOIN app_user ua ON ua.id = b.approved_by
              WHERE b.tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let cadences = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.programme_id, t.kind, t.period, t.id), '[]'::jsonb) FROM (
             SELECT id, programme_id, kind, period, state, meta, version, created_at, updated_at
               FROM programme_cadence WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    // The cadence record's two satellites: the demo credits a cadence issued and the lessons its
    // retros raised. They travel beside `cadences` in `msp`; a credit names its cadence and a
    // lesson names its programme, so neither needs a programme column of its own.
    let demo_credits = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.cadence_id, t.deliverable_id, t.id), '[]'::jsonb) FROM (
             SELECT id, cadence_id, deliverable_id, evidence, credited_at, version
               FROM programme_demo_credit WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let lessons = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.programme_id, t.cadence_id, t.finding_key, t.id), '[]'::jsonb) FROM (
             SELECT id, programme_id, cadence_id, finding_key, statement, evidence,
                    independent_count, state, source, version, created_at, updated_at
               FROM lesson WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let wbs_nodes = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.project_id, t.parent_id, t.position, t.id), '[]'::jsonb) FROM (
             SELECT id, project_id, parent_id, position::float8 AS position, kind, title, body,
                    status, acceptance, evidence, card_id, business_value, time_criticality,
                    risk_reduction, job_size, version, created_at, updated_at
               FROM wbs_node WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let project_baselines = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.project_id, t.taken_at, t.id), '[]'::jsonb) FROM (
             SELECT b.id, b.project_id, b.kind, b.snapshot, b.taken_at, b.taken_by,
                    ub.display_name AS taken_by_name, b.approved_by,
                    ua.display_name AS approved_by_name, b.version, b.created_at
               FROM project_baseline b
               LEFT JOIN app_user ub ON ub.id = b.taken_by
               LEFT JOIN app_user ua ON ua.id = b.approved_by
              WHERE b.tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let register_items = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.project_id, t.created_at, t.id), '[]'::jsonb) FROM (
             SELECT id, project_id, kind, code, title, body, owner_job_id, status, severity,
                    probability, impact, links, baseline_ref, meta, version, created_at, updated_at
               FROM project_register_item WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let project_dependencies = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.from_project, t.to_project, t.kind, t.id), '[]'::jsonb) FROM (
             SELECT id, programme_id, from_project, to_project, kind, status, meta, version,
                    created_at, updated_at
               FROM project_dependency WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;

    let adm_profiles = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.key), '[]'::jsonb) FROM (
             SELECT id, key, name, phase_set, artifact_requirements, contract_kinds, status,
                    adopted_at, board_decision_id, version
               FROM adm_profile WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let artifact_standard = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.phase_code, t.plateau, t.position, t.key), '[]'::jsonb) FROM (
             SELECT id, key, name, artifact_kind, phase_code, plateau, mandatory, position
               FROM artifact_type WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let mut architecture_artifacts = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.phase_code, t.plateau, t.title, t.id), '[]'::jsonb) FROM (
             SELECT a.id, a.project_id, a.artifact_type_id, a.phase_code, a.plateau,
                    a.artifact_kind, a.title, a.status, a.artifact_version, a.change_initiative,
                    a.purpose, a.stakeholders, a.scope, a.document_id, a.contract_id, a.meta,
                    a.version, a.created_at, a.updated_at,
                    COALESCE((SELECT jsonb_agg(to_jsonb(e) ORDER BY e.position, e.axis_a)
                                FROM architecture_artifact_entry e
                               WHERE e.tenant_id = a.tenant_id AND e.artifact_id = a.id),
                             '[]'::jsonb) AS entries
               FROM architecture_artifact a WHERE a.tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let architecture_links = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.kind, t.from_artifact_id), '[]'::jsonb) FROM (
             SELECT id, from_artifact_id, to_artifact_id, kind, version
               FROM architecture_artifact_link WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let mut architecture_contracts = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.phase_code, t.kind, t.created_at, t.id), '[]'::jsonb) FROM (
             SELECT id, project_id, kind, phase_code, title, body, status, approval_id,
                    approved_by, requested_by, approved_at, supersedes_id, superseded_by_id,
                    version, created_at, updated_at
               FROM architecture_contract WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let mut architecture_reviews = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.reviewed_at DESC, t.project_id, t.id), '[]'::jsonb) FROM (
             SELECT r.id, r.project_id, r.contract_id, r.level, r.findings, r.evidence,
                    r.outcome, r.reviewed_by, u.display_name AS reviewed_by_name,
                    r.reviewed_at, r.version, r.created_at, r.updated_at
               FROM architecture_compliance_review r
               LEFT JOIN app_user u ON u.id = r.reviewed_by
              WHERE r.tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let mut architecture_waivers = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.reassessment_at DESC, t.review_id, t.id), '[]'::jsonb) FROM (
             SELECT w.id, w.review_id, w.owner_user_id, u.display_name AS owner_name,
                    w.reassessment_at, w.status, w.version, w.created_at, w.updated_at
               FROM architecture_waiver w
               LEFT JOIN app_user u ON u.id = w.owner_user_id
              WHERE w.tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let architecture_board = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.key), '[]'::jsonb) FROM (
             SELECT id, kind, key, name, charter, commissioning_authority, decision_limits,
                    members, status, version, created_at, updated_at
               FROM governance_body
              WHERE tenant_id = $1 AND kind = 'architecture_board'
         ) t",
        tenant_id,
    )
    .await?;
    let architecture_maturity = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.assessed_at DESC, t.assessment_id, t.dimension), '[]'::jsonb) FROM (
             SELECT id, assessment_id, stage, dimension, score, assessed_at, evidence
               FROM architecture_maturity WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let architecture_skills = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.assessed_at DESC, t.role, t.category), '[]'::jsonb) FROM (
             SELECT id, assessment_id, role, job_key, category, proficiency, assessed_at, evidence
               FROM architecture_skill_assessment WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    // The incident lifecycle (T7, SPEC_V2 §9.2). An incident is an ordinary `incident` card whose
    // `meta` carries the record — so the public section selects those named keys only, never the
    // whole object: `triage_narrative`, `mitigation` and `evidence` are the operator's text. The
    // runbook is a published procedure and the postmortem a governed asset; the id join gives the
    // reader the titles without a second file. The joins compare `::text` so a hand-edited meta
    // value that is not a uuid is a non-match, not a 500 on the public route.
    let mut incidents = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.created_at, t.id), '[]'::jsonb) FROM (
             SELECT c.id, c.board_id, c.name, c.created_at, c.updated_at,
                    c.meta ->> 'service' AS service,
                    c.meta ->> 'severity' AS severity,
                    c.meta ->> 'detected_at' AS detected_at,
                    c.meta ->> 'triaged_at' AS triaged_at,
                    c.meta ->> 'mitigated_at' AS mitigated_at,
                    c.meta ->> 'resolved_at' AS resolved_at,
                    c.meta ->> 'runbook_procedure_id' AS runbook_procedure_id,
                    p.key AS runbook_key,
                    p.title AS runbook_title,
                    c.meta ->> 'postmortem_asset_id' AS postmortem_asset_id,
                    a.title AS postmortem_title
               FROM card c
               JOIN card_type ct ON ct.id = c.card_type_id AND ct.code = 'incident'
               LEFT JOIN procedure p
                      ON p.tenant_id = c.tenant_id
                     AND p.id::text = c.meta ->> 'runbook_procedure_id'
               LEFT JOIN governed_asset a
                      ON a.tenant_id = c.tenant_id
                     AND a.id::text = c.meta ->> 'postmortem_asset_id'
              WHERE c.tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    // ⛔ The public pack is the PROGRAMME's view (SPEC_V2 §5; the console already filters its
    // project list the same way — `docs/design/v2.js:4193`). A project outside a programme is a
    // session fixture — a proof, a scratch reproduction — never programme work, and its boards,
    // runs, comments and approvals are not the programme's story. Filtered here, at the one
    // snapshot every public surface reads, so a future fixture cannot become a distractor just
    // by being created. The authenticated API (`GET /api/projects`) is unaffected: an operator
    // still sees every row.
    let bound_projects = ids_where(&projects, "programme_id", |field| !field.is_null());
    retain_ids(&mut projects, "id", &bound_projects);
    retain_ids(&mut boards, "project_id", &bound_projects);
    let bound_boards = ids_where(&boards, "id", |_| true);
    retain_ids(&mut lists, "board_id", &bound_boards);
    retain_ids(&mut cards, "board_id", &bound_boards);
    // An incident is a card: the same programme boundary decides whether it is the programme's
    // public record.
    retain_ids(&mut incidents, "board_id", &bound_boards);
    let bound_cards = ids_where(&cards, "id", |_| true);
    retain_ids(&mut runs, "project_id", &bound_projects);
    let bound_runs = ids_where(&runs, "id", |_| true);
    retain_ids(&mut run_steps, "run_id", &bound_runs);
    retain_ids(&mut comments, "card_id", &bound_cards);
    // An approval belongs to the programme when it names a bound project or hangs from a card
    // of one; a gate with neither parent is an orphan and is not public.
    if let Some(rows) = approvals.as_array_mut() {
        rows.retain(|row| {
            row.get("project_id")
                .and_then(Value::as_str)
                .is_some_and(|id| bound_projects.contains(id))
                || row
                    .get("card_id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| bound_cards.contains(id))
        });
    }
    let bound_approvals = ids_where(&approvals, "id", |_| true);
    retain_ids(&mut approval_decisions, "approval_id", &bound_approvals);
    // A published output is public because it belongs to the programme. A document with no
    // project is a tenant-level note the tenant chose to publish, and stays.
    if let Some(rows) = documents.as_array_mut() {
        rows.retain(|row| {
            row.get("project_id")
                .and_then(Value::as_str)
                .is_none_or(|id| bound_projects.contains(id))
        });
    }
    // The architecture rows follow the same programme boundary: an artefact, contract or
    // compliance review tied to a project outside the programme is not the public capability's
    // story; a row with no project is programme-level and stays (as documents do). A waiver hangs
    // from a review, so it follows the review's boundary.
    for row_set in [
        &mut architecture_artifacts,
        &mut architecture_contracts,
        &mut architecture_reviews,
    ] {
        if let Some(rows) = row_set.as_array_mut() {
            rows.retain(|row| {
                row.get("project_id")
                    .and_then(Value::as_str)
                    .is_none_or(|id| bound_projects.contains(id))
            });
        }
    }
    let bound_reviews = ids_where(&architecture_reviews, "id", |_| true);
    retain_ids(&mut architecture_waivers, "review_id", &bound_reviews);

    Ok(SnapshotData {
        programme: json!({"programme": programme, "projects": projects}),
        // T0: the organisation spine (§4) — industries, the placed departments, the job
        // catalogue with its latest release's anatomy flattened onto each row, releases and
        // bindings. Structural fields only (the publication privacy contract).
        organisation: crate::organisation::organisation_json_tx(&mut *conn, tenant).await?,
        config: json!({
            "catalogue": catalogue,
            "capabilities": capabilities,
            "capability_agents": agents,
            "processes": processes,
            "process_steps": process_steps,
            "procedures": procedures,
            "prompts": prompts,
            "prompt_sections": prompt_sections,
            "schedules": schedules
        }),
        work: json!({
            "boards": boards,
            "lists": lists,
            "cards": cards,
            "card_types": card_types,
            "runs": runs,
            "run_steps": run_steps,
            "approvals": approvals,
            "approval_decisions": approval_decisions,
            "comments": comments,
            "people": people
        }),
        msp: json!({
            "benefit_profiles": benefit_profiles,
            "benefit_links": benefit_links,
            "benefit_transitions": benefit_transitions,
            "stakeholder_profiles": stakeholder_profiles,
            "tranches": tranches,
            "tranche_reviews": tranche_reviews,
            "programme_baselines": programme_baselines,
            "cadences": cadences,
            "demo_credits": demo_credits,
            "lessons": lessons,
            "wbs_nodes": wbs_nodes,
            "project_baselines": project_baselines,
            "register_items": register_items,
            "project_dependencies": project_dependencies
        }),
        artefacts: json!({"governed_assets": assets, "documents": documents}),
        architecture: json!({
            "profiles": adm_profiles,
            "standard": artifact_standard,
            "artifacts": architecture_artifacts,
            "links": architecture_links,
            "contracts": architecture_contracts,
            "compliance_reviews": architecture_reviews,
            "waivers": architecture_waivers,
            "board": architecture_board,
            "maturity": architecture_maturity,
            "skills": architecture_skills
        }),
        ops: json!({
            "incidents": incidents
        }),
        measure: json!({
            "inference": {
                "totals": inference_totals,
                "by_model": inference_by_model,
                "by_caller": inference_by_caller
            },
            "note": "tokens are stored; cost is derived by the app from the pinned price version \
                     and is not stored in the ledger"
        }),
        secrets: json!({"secrets": secrets}),
    })
}

/// The `id`s of the rows whose `key` field satisfies `keep`.
///
/// The snapshot's sections are `jsonb_agg` arrays of row objects, so the scope filter is a set
/// operation over the rendered values rather than a second `WHERE` on every query: the ids a
/// section carries are already the ids the dependent sections name.
fn ids_where(value: &Value, key: &str, keep: impl Fn(&Value) -> bool) -> HashSet<String> {
    value
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter(|row| row.get(key).is_some_and(&keep))
                .filter_map(|row| row.get("id").and_then(Value::as_str).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Keep only the rows whose `key` field names one of `ids`.
fn retain_ids(value: &mut Value, key: &str, ids: &HashSet<String>) {
    if let Some(rows) = value.as_array_mut() {
        rows.retain(|row| {
            row.get(key)
                .and_then(Value::as_str)
                .is_some_and(|id| ids.contains(id))
        });
    }
}

/// One `jsonb_agg` query: a single jsonb value, bound to the tenant.
async fn rows(
    conn: &mut PgConnection,
    sql: &'static str,
    tenant_id: uuid::Uuid,
) -> Result<Value, sqlx::Error> {
    sqlx::query_scalar::<_, Value>(sql)
        .bind(tenant_id)
        .fetch_one(&mut *conn)
        .await
}
