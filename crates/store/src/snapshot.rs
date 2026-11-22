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

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use conveyor_domain::inference::UsageGroup;
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
    /// `work.json` data: boards, lists, cards, runs, approvals, comments, people, and each
    /// project's observed activity window.
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
    /// `measure.json` data: inference aggregates (no prompt or response bodies exist), including
    /// the per-project and per-card spend the delivery view reads.
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
                    (SELECT count(*) FROM board b WHERE b.project_id = p.id) AS board_count,
                    (SELECT count(*) FROM board b JOIN card c ON c.board_id = b.id
                      WHERE b.project_id = p.id) AS card_count
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
                    -- A rework package's typed keys; absent meta is SQL NULL, rendered as JSON
                    -- null, never as an empty string.
                    c.meta ->> 'rework_key' AS rework_key,
                    c.meta ->> 'rule' AS rule,
                    c.meta ->> 'severity' AS severity,
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
    // Each project's observed activity window — the actuals a Gantt bar is drawn from. Built from
    // events (cards and runs) rather than from `project.created_at`, so a bar starts when work
    // started, not when the row was inserted. A project with no event is absent here; the page
    // then shows its row without a bar rather than inventing one.
    let mut project_activity = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.project_id), '[]'::jsonb) FROM (
             SELECT project_id, min(ts) AS started_at, max(ts) AS last_activity_at
               FROM (
                    SELECT b.project_id, c.created_at AS ts
                      FROM board b JOIN card c ON c.board_id = b.id AND c.tenant_id = b.tenant_id
                     WHERE b.tenant_id = $1
                    UNION ALL
                    SELECT b.project_id, c.list_changed_at
                      FROM board b JOIN card c ON c.board_id = b.id AND c.tenant_id = b.tenant_id
                     WHERE b.tenant_id = $1
                    UNION ALL
                    SELECT b.project_id, c.archived_at
                      FROM board b JOIN card c ON c.board_id = b.id AND c.tenant_id = b.tenant_id
                     WHERE b.tenant_id = $1 AND c.archived_at IS NOT NULL
                    UNION ALL
                    SELECT r.project_id, r.created_at FROM run r WHERE r.tenant_id = $1
                    UNION ALL
                    SELECT r.project_id, r.started_at
                      FROM run r WHERE r.tenant_id = $1 AND r.started_at IS NOT NULL
                    UNION ALL
                    SELECT r.project_id, r.finished_at
                      FROM run r WHERE r.tenant_id = $1 AND r.finished_at IS NOT NULL
               ) events
              WHERE project_id IS NOT NULL
              GROUP BY project_id
         ) t",
        tenant_id,
    )
    .await?;
    let mut approvals = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.created_at, t.id), '[]'::jsonb) FROM (
             SELECT id, run_id, card_id, project_id, gate, kind, status, required_decisions,
                    requested_by, expires_at, meta, version, created_at, updated_at
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

    // The metering totals. Money is never a stored ledger column (SPEC_V2 D52): `inference_call`
    // stores tokens and the cost is derived at read time from `commercial_price`, with the exact
    // formula `metrics.usage` and `inference_usage.report` use — a token-priced model by prompt
    // and completion tokens, a unit-priced model by `usage_units`. A call the price join cannot
    // price contributes NULL (zero to the sum) and is counted in `unpriced_calls`, mirroring
    // `metrics.usage`, so "no price" never renders as "free". `tool_cost_usd` is the
    // `tool_call` ledger's own recorded cost. Money is rounded to 6 decimals and stays a JSON
    // number in the snapshot (the per-project/per-card rows keep the report's decimal string).
    let inference_totals: Value = sqlx::query_scalar(
        "WITH priced AS (
             SELECT ic.*,
                    CASE
                        WHEN p.model IS NULL THEN NULL
                        WHEN p.unit = 'token' THEN
                            (ic.prompt_tokens::numeric * p.input_usd_per_1m
                             + ic.completion_tokens::numeric * p.output_usd_per_1m) / 1000000
                        WHEN ic.usage_units IS NOT NULL THEN ic.usage_units * p.unit_usd
                        ELSE NULL
                    END AS row_cost
               FROM inference_call ic
               LEFT JOIN commercial_price p
                      ON p.model = coalesce(ic.served_model, ic.model)
                     AND ((p.unit = 'token' AND ic.usage_unit IS NULL) OR p.unit = ic.usage_unit)
              WHERE ic.tenant_id = $1
         )
         SELECT jsonb_build_object(
             'calls', count(*),
             'prompt_tokens', COALESCE(sum(prompt_tokens), 0),
             'completion_tokens', COALESCE(sum(completion_tokens), 0),
             'total_tokens', COALESCE(sum(total_tokens), 0),
             'cache_hit_calls', COALESCE(sum((cache_status = 'hit')::int), 0),
             'cache_write_calls', COALESCE(sum((cache_status = 'write')::int), 0),
             'cache_miss_calls', COALESCE(sum((cache_status = 'miss')::int), 0),
             'unknown_cache_calls', COALESCE(sum((cache_status = 'unknown')::int), 0),
             'non_ok_calls', COALESCE(sum((outcome <> 'ok')::int), 0),
             'cost_usd', COALESCE(round(sum(row_cost), 6), 0),
             'unpriced_calls', count(*) - count(row_cost),
             'tool_cost_usd', (
                 SELECT COALESCE(round(sum(cost), 6), 0)
                   FROM tool_call WHERE tenant_id = $1
             )
         ) FROM priced",
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
             SELECT id, project_id, programme_id, kind, code, title, body, owner_job_id, status,
                    severity, probability, impact, response, review_at, links, baseline_ref, meta,
                    version, created_at, updated_at
               FROM project_register_item WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    // §8.3 benefit evaluation: the measurements a profile is judged against. One row per reading
    // (baseline | interim | post), so the pack can compute a variance rather than trusting the two
    // nullable timestamps on `benefit_transition`.
    let benefit_measurements = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.benefit_id, t.measured_at, t.id), '[]'::jsonb) FROM (
             SELECT id, benefit_id, programme_id, tranche_id, kind, target_value, actual_value,
                    unit, method, measured_at, evidence
               FROM benefit_measurement WHERE tenant_id = $1
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
    // Test residue is not the programme's portfolio. The e2e/UI-proof rows named below created
    // projects in the live tenant with no marker; the public pack must neither count them as
    // programme projects nor let their boards, cards, runs, documents or architecture rows in.
    // They are listed under `programme.test_projects` with the reason, so the exclusion is
    // visible rather than silent. The authenticated API (`GET /api/projects`) is unaffected: an
    // operator still sees every row.
    let mut test_projects: Vec<Value> = Vec::new();
    if let Some(rows) = projects.as_array_mut() {
        rows.retain(|row| match test_row_reason(row) {
            Some(reason) => {
                test_projects.push(json!({
                    "key": row.get("key").cloned().unwrap_or(Value::Null),
                    "name": row.get("name").cloned().unwrap_or(Value::Null),
                    "status": row.get("status").cloned().unwrap_or(Value::Null),
                    "reason": reason,
                }));
                false
            }
            None => true,
        });
    }
    // ⛔ The public pack is the TENANT'S PORTFOLIO (SPEC_V2 §5.1; owner brief 2026-11-21). Every
    // programme-bound project is public. A standalone project is public only when it is a real
    // change initiative: status `active`/`paused` AND it carries a board or a card. A standalone
    // draft, or a bare row with neither, is a session fixture — a proof, a scratch reproduction —
    // never portfolio work, and its boards, runs, comments and approvals are not the programme's
    // story. Filtered here, at the one snapshot every public surface reads, so a future fixture
    // cannot become a public row just by being created. The authenticated API
    // (`GET /api/projects`) is unaffected: an operator still sees every row.
    let bound_projects = ids_where_row(&projects, |row| {
        let programme_bound = row.get("programme_id").is_some_and(|id| !id.is_null());
        let status = row
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let in_flight = matches!(status, "active" | "paused");
        let has_work = |key: &str| row.get(key).and_then(Value::as_i64).unwrap_or(0) > 0;
        programme_bound || (in_flight && (has_work("board_count") || has_work("card_count")))
    });
    retain_ids(&mut projects, "id", &bound_projects);
    retain_ids(&mut project_activity, "project_id", &bound_projects);
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
    // of one; a gate with neither parent is an orphan and is not public. The one exception is a
    // gate a public record CITES: a contract's approval is an `architecture_board` decision with
    // no project or card parent, and it is the contract's evidence — dropping it made every
    // contract unprovable from the public record. It is kept because it is cited, never because
    // it is parented, and its decisions follow through `bound_approvals`.
    let cited_approvals: std::collections::HashSet<String> = architecture_contracts
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row.get("approval_id").and_then(Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    if let Some(rows) = approvals.as_array_mut() {
        rows.retain(|row| {
            row.get("project_id")
                .and_then(Value::as_str)
                .is_some_and(|id| bound_projects.contains(id))
                || row
                    .get("card_id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| bound_cards.contains(id))
                || row
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| cited_approvals.contains(id))
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

    // Per-project and per-card spend for the delivery view. The metering aggregate is the one
    // priced read (tokens stored, money derived); it is built AFTER the boundary so a filtered
    // project's calls cannot reach a public file. `cost_usd` stays a decimal string, exactly as
    // the usage report renders it — an unpriced model contributes zero and is counted in
    // `unpriced_calls`, never shown as free.
    let since = DateTime::<Utc>::UNIX_EPOCH;
    let until = Utc::now();
    let usage_entry = |row: conveyor_domain::inference::UsageRow| {
        json!({
            "calls": row.calls,
            "prompt_tokens": row.prompt_tokens,
            "completion_tokens": row.completion_tokens,
            "total_tokens": row.prompt_tokens + row.completion_tokens,
            "cost_usd": row.cost_usd,
            "priced_calls": row.priced_calls,
            "unpriced_calls": row.unpriced_calls,
        })
    };
    let usage_by_project: Vec<Value> =
        crate::inference::usage_rows_tx(&mut *conn, tenant, UsageGroup::Project, since, until)
            .await
            .map_err(inference_error)?
            .into_iter()
            .filter(|row| bound_projects.contains(&row.bucket))
            .map(|row| {
                let project_id = row.bucket.clone();
                let mut value = usage_entry(row);
                value["project_id"] = Value::String(project_id);
                value
            })
            .collect();
    let usage_by_card: Vec<Value> =
        crate::inference::usage_by_card_tx(&mut *conn, tenant, since, until)
            .await
            .map_err(inference_error)?
            .into_iter()
            .filter(|row| bound_cards.contains(&row.bucket))
            .map(|row| {
                let card_id = row.bucket.clone();
                let mut value = usage_entry(row);
                value["card_id"] = Value::String(card_id);
                value
            })
            .collect();

    // The derived read-model sections (issue #528, Part 4). Each is a pure function over rows
    // already loaded above and runs AFTER the portfolio boundary, so a derived row can never
    // name a project the pack does not publish. The row sections stay as they are; these answer
    // the questions an MSP/TOGAF reader asks of them.
    let benefit_realisation_section =
        benefit_realisation(&benefit_profiles, &benefit_transitions, &projects);
    let dependency_section = dependencies(&projects);
    let tranche_reconciliation_section =
        tranche_reconciliation(&tranches, &tranche_reviews, &projects);
    let phase_progress_section = phase_progress(
        &projects,
        &architecture_artifacts,
        &architecture_contracts,
        &architecture_reviews,
    );

    Ok(SnapshotData {
        programme: json!({
            "programme": programme,
            "projects": projects,
            "test_projects": test_projects,
        }),
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
            "project_activity": project_activity,
            "approvals": approvals,
            "approval_decisions": approval_decisions,
            "comments": comments,
            "people": people
        }),
        msp: json!({
            "benefit_profiles": benefit_profiles,
            "benefit_links": benefit_links,
            "benefit_transitions": benefit_transitions,
            "benefit_measurements": benefit_measurements,
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
            "project_dependencies": project_dependencies,
            "benefit_realisation": benefit_realisation_section,
            "dependencies": dependency_section,
            "tranche_reconciliation": tranche_reconciliation_section
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
            "skills": architecture_skills,
            "phase_progress": phase_progress_section
        }),
        ops: json!({
            "incidents": incidents
        }),
        measure: json!({
            "inference": {
                "totals": inference_totals,
                "by_model": inference_by_model,
                "by_caller": inference_by_caller,
                "by_project": usage_by_project,
                "by_card": usage_by_card
            },
            "note": "tokens are stored; cost is derived by the app from the pinned price version \
                     and is not stored in the ledger. `totals`, `by_model` and `by_caller` are \
                     tenant-wide; `by_project` and `by_card` cover only the rows this snapshot \
                     publishes, so the differences are the boundary, not a missing call"
        }),
        secrets: json!({"secrets": secrets}),
    })
}

/// The inference store's only error is a database refusal; the snapshot's error type is the same.
fn inference_error(error: crate::inference::InferenceStoreError) -> sqlx::Error {
    match error {
        crate::inference::InferenceStoreError::Database(inner) => inner,
    }
}

/// The public project keys by row id. A derived section names a project by key: the key is the
/// reader's name for it, and an id would dangle outside the tenant.
/// The two e2e/UI-proof projects that predate the `meta.test_row` marker.
const LEGACY_TEST_PROJECT_KEYS: [&str; 2] = [
    "AAA-UI-PROOF-285-2026-11-19T07-3-2",
    "AAA-UI-PROOF-285-2026-11-19T07-4",
];

/// Why a project row is test residue, or `None` when it is a real project.
fn test_row_reason(row: &Value) -> Option<&'static str> {
    let marked = row
        .get("meta")
        .and_then(Value::as_object)
        .and_then(|meta| meta.get("test_row"))
        .and_then(Value::as_bool)
        == Some(true);
    if marked {
        return Some("meta.test_row is true");
    }
    let key = row.get("key").and_then(Value::as_str).unwrap_or_default();
    if LEGACY_TEST_PROJECT_KEYS.contains(&key) {
        return Some("a known pre-marker e2e project key");
    }
    None
}

fn project_key_by_id(projects: &Value) -> HashMap<String, String> {
    projects
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    Some((
                        row.get("id")?.as_str()?.to_owned(),
                        row.get("key")?.as_str()?.to_owned(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `msp.benefit_realisation`: one entry per benefit profile, sorted by reference.
///
/// The arithmetic is `spine::benefit_realization_tx`'s, applied to the rows already loaded: the
/// profile's `baseline.value` first, then the first transition's `evidence.baseline_value`; the
/// post value is the latest transition's `evidence.post_value`; the delta is post minus baseline.
/// A profile whose baseline is missing, a placeholder or non-numeric is NOT measured — it is
/// `unbaselined`, whatever a transition row claims, so the gap is visible instead of implied.
fn benefit_realisation(profiles: &Value, transitions: &Value, projects: &Value) -> Value {
    let project_keys = project_key_by_id(projects);
    let mut by_benefit: HashMap<&str, Vec<&Value>> = HashMap::new();
    for transition in transitions
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        if let Some(id) = transition.get("benefit_id").and_then(Value::as_str) {
            by_benefit.entry(id).or_default().push(transition);
        }
    }
    let mut entries: Vec<(&str, &str, Value)> = profiles
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|profile| {
            let id = profile
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let reference = profile
                .get("reference")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let rows = by_benefit.get(id).cloned().unwrap_or_default();
            let baseline_json = profile.get("baseline").filter(|value| !value.is_null());
            let raw_baseline = baseline_json
                .and_then(|baseline| baseline.get("value"))
                .and_then(Value::as_f64)
                .or_else(|| baseline_json.and_then(Value::as_f64))
                .or_else(|| {
                    rows.iter().find_map(|transition| {
                        transition
                            .get("evidence")
                            .and_then(|evidence| evidence.get("baseline_value"))
                            .and_then(Value::as_f64)
                    })
                });
            // A baseline is a placeholder when no numeric value exists anywhere (the seeded
            // profiles carry a qualitative "not measured …" string) or when the row says so.
            // The explicit flag WINS over a stray number: a placeholder is not a measurement,
            // so it contributes neither a state nor a delta.
            let placeholder = raw_baseline.is_none()
                || baseline_json
                    .and_then(|baseline| baseline.get("placeholder"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
            let baseline_value = if placeholder { None } else { raw_baseline };
            let baseline_measured_at = baseline_json
                .and_then(|baseline| baseline.get("measured_at"))
                .filter(|value| !value.is_null())
                .cloned()
                .or_else(|| {
                    rows.iter().find_map(|transition| {
                        transition
                            .get("baseline_measured_at")
                            .filter(|value| !value.is_null())
                            .cloned()
                    })
                })
                .unwrap_or(Value::Null);
            let post_value = rows.iter().rev().find_map(|transition| {
                transition
                    .get("evidence")
                    .and_then(|evidence| evidence.get("post_value"))
                    .and_then(Value::as_f64)
            });
            let delta = match (baseline_value, post_value) {
                (Some(baseline), Some(post)) => Some(post - baseline),
                _ => None,
            };
            let enacted = |transition: &&Value| {
                transition
                    .get("enacted_at")
                    .is_some_and(|value| !value.is_null())
            };
            let retired = |transition: &&Value| {
                transition
                    .get("retired_at")
                    .is_some_and(|value| !value.is_null())
            };
            let state = if rows.iter().any(retired) {
                "retired"
            } else if rows.iter().any(|transition| {
                transition
                    .get("post_measured_at")
                    .is_some_and(|value| !value.is_null())
                    && transition
                        .get("evidence")
                        .and_then(|evidence| evidence.get("post_value"))
                        .and_then(Value::as_f64)
                        .is_some()
            }) {
                "realised"
            } else if rows.iter().any(enacted) {
                "transitioned"
            } else if baseline_value.is_some() {
                // A dormant transition row (no `enacted_at`) leaves the benefit baselined.
                "baselined"
            } else {
                "unbaselined"
            };
            let transitions: Vec<Value> = rows
                .iter()
                .map(|transition| {
                    json!({
                        "baseline_measured_at": transition
                            .get("baseline_measured_at")
                            .cloned()
                            .unwrap_or(Value::Null),
                        "enacted_at": transition.get("enacted_at").cloned().unwrap_or(Value::Null),
                        "post_measured_at": transition
                            .get("post_measured_at")
                            .cloned()
                            .unwrap_or(Value::Null),
                        "retired_at": transition.get("retired_at").cloned().unwrap_or(Value::Null),
                        "evidence": transition.get("evidence").cloned().unwrap_or(Value::Null),
                    })
                })
                .collect();
            let project = profile
                .get("project_id")
                .and_then(Value::as_str)
                .and_then(|id| project_keys.get(id))
                .map(|key| Value::String(key.clone()))
                .unwrap_or(Value::Null);
            let entry = json!({
                "reference": reference,
                "project": project,
                "baseline": {
                    "value": baseline_value,
                    "measured_at": baseline_measured_at,
                    "placeholder": placeholder,
                },
                "target": profile.get("target").cloned().unwrap_or(Value::Null),
                "measurement_method": profile
                    .get("measurement_method")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                // The seeded profile records its owner role in `operational_area`.
                "owner_role": profile
                    .get("operational_area")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                "transitions": transitions,
                "latest_post": post_value,
                "delta": delta,
                "state": state,
            });
            (reference, id, entry)
        })
        .collect();
    entries.sort_by(|left, right| left.0.cmp(right.0).then_with(|| left.1.cmp(right.1)));
    Value::Array(entries.into_iter().map(|(_, _, entry)| entry).collect())
}

/// The typed kinds `project_dependency` accepts. A declared edge with any other kind stays
/// untyped: the snapshot never invents a kind.
const DEPENDENCY_KINDS: [&str; 4] = ["blocks", "needs", "supersedes", "informs"];

/// A declared edge: `(from, to, kind, typed)`.
type DependencyEdge = (String, String, Option<String>, bool);

/// `msp.dependencies`: the programme's DECLARED edges — each project's `meta.depends_on`, not the
/// `project_dependency` rows. A string is an untyped edge (`kind: null, typed: false`); an object
/// may carry `{to, kind}` but its kind counts only when it is one `project_dependency` accepts.
/// Unknown targets and cycles are published, never fatal: they are the record's condition.
fn dependencies(projects: &Value) -> Value {
    let rows = projects.as_array().map(Vec::as_slice).unwrap_or_default();
    let known: BTreeSet<&str> = rows
        .iter()
        .filter_map(|project| project.get("key").and_then(Value::as_str))
        .collect();
    let mut edges: BTreeSet<DependencyEdge> = BTreeSet::new();
    for project in rows {
        let Some(from) = project.get("key").and_then(Value::as_str) else {
            continue;
        };
        let Some(items) = project
            .get("meta")
            .and_then(|meta| meta.get("depends_on"))
            .and_then(Value::as_array)
        else {
            continue;
        };
        for item in items {
            let (to, kind) = match item {
                Value::String(text) => (text.trim(), None),
                Value::Object(_) => {
                    let Some(to) = item.get("to").and_then(Value::as_str) else {
                        continue;
                    };
                    let kind = item
                        .get("kind")
                        .and_then(Value::as_str)
                        .filter(|kind| DEPENDENCY_KINDS.contains(kind))
                        .map(str::to_owned);
                    (to.trim(), kind)
                }
                _ => continue,
            };
            if to.is_empty() {
                continue;
            }
            let typed = kind.is_some();
            edges.insert((from.to_owned(), to.to_owned(), kind, typed));
        }
    }
    let mut counts: BTreeMap<&str, u64> = DEPENDENCY_KINDS.iter().map(|kind| (*kind, 0)).collect();
    let mut untyped = 0u64;
    for (_, _, kind, _) in &edges {
        match kind {
            Some(kind) => *counts.entry(kind.as_str()).or_default() += 1,
            None => untyped += 1,
        }
    }
    let unknown: BTreeSet<(String, String)> = edges
        .iter()
        .filter(|(_, to, _, _)| !known.contains(to.as_str()))
        .map(|(from, to, _, _)| (from.clone(), to.clone()))
        .collect();
    let edges_json: Vec<Value> = edges
        .iter()
        .map(
            |(from, to, kind, typed)| json!({"from": from, "to": to, "kind": kind, "typed": typed}),
        )
        .collect();
    let unknown_targets: Vec<Value> = unknown
        .iter()
        .map(|(from, to)| json!({"from": from, "to": to}))
        .collect();
    let cycles = dependency_cycles(&known, &edges);
    json!({
        "edges": edges_json,
        "counts": {
            "blocks": counts["blocks"],
            "needs": counts["needs"],
            "supersedes": counts["supersedes"],
            "informs": counts["informs"],
        },
        "untyped": untyped,
        "unknown_targets": unknown_targets,
        "cycles": cycles,
    })
}

/// The strongly connected components of the declared graph that contain a cycle (an SCC of more
/// than one node, or one node with a self-edge). Kosaraju, iterating nodes and neighbours in
/// sorted order, so the output is identical on every run.
fn dependency_cycles(nodes: &BTreeSet<&str>, edges: &BTreeSet<DependencyEdge>) -> Vec<Vec<String>> {
    let mut adjacency: BTreeMap<&str, Vec<&str>> =
        nodes.iter().map(|node| (*node, Vec::new())).collect();
    let mut reverse: BTreeMap<&str, Vec<&str>> =
        nodes.iter().map(|node| (*node, Vec::new())).collect();
    for (from, to, _, _) in edges {
        if !nodes.contains(to.as_str()) {
            continue;
        }
        if let Some(neighbours) = adjacency.get_mut(from.as_str()) {
            neighbours.push(to.as_str());
        }
        if let Some(neighbours) = reverse.get_mut(to.as_str()) {
            neighbours.push(from.as_str());
        }
    }
    for neighbours in adjacency.values_mut() {
        neighbours.sort_unstable();
        neighbours.dedup();
    }
    for neighbours in reverse.values_mut() {
        neighbours.sort_unstable();
        neighbours.dedup();
    }
    // First pass: finish order on the graph.
    let mut visited: BTreeSet<&str> = BTreeSet::new();
    let mut finish: Vec<&str> = Vec::new();
    for start in nodes {
        if visited.contains(*start) {
            continue;
        }
        visited.insert(*start);
        let mut stack: Vec<(&str, usize)> = vec![(*start, 0)];
        while let Some((node, position)) = stack.pop() {
            let neighbours = &adjacency[node];
            if position < neighbours.len() {
                stack.push((node, position + 1));
                let next = neighbours[position];
                if visited.insert(next) {
                    stack.push((next, 0));
                }
            } else {
                finish.push(node);
            }
        }
    }
    // Second pass: components on the reversed graph, in reverse finish order.
    let mut assigned: BTreeSet<&str> = BTreeSet::new();
    let mut cycles: Vec<Vec<String>> = Vec::new();
    for start in finish.into_iter().rev() {
        if !assigned.insert(start) {
            continue;
        }
        let mut component: Vec<&str> = Vec::new();
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            component.push(node);
            for &neighbour in &reverse[node] {
                if assigned.insert(neighbour) {
                    stack.push(neighbour);
                }
            }
        }
        let self_loop = component.len() == 1 && adjacency[component[0]].contains(&component[0]);
        if component.len() > 1 || self_loop {
            component.sort_unstable();
            cycles.push(component.into_iter().map(str::to_owned).collect());
        }
    }
    cycles.sort();
    cycles
}

/// `msp.tranche_reconciliation`: one entry per tranche, sorted by number. The review is the
/// latest one; `authorised` is true only when its decision is `continue`. A closed project in a
/// tranche with no such decision is a flag, not a correction — the snapshot reports it.
fn tranche_reconciliation(tranches: &Value, reviews: &Value, projects: &Value) -> Value {
    let mut latest_review: HashMap<&str, &Value> = HashMap::new();
    for review in reviews.as_array().map(Vec::as_slice).unwrap_or_default() {
        let Some(tranche_id) = review.get("tranche_id").and_then(Value::as_str) else {
            continue;
        };
        let replace = latest_review
            .get(tranche_id)
            .is_none_or(|current| row_is_later(review, current, "reviewed_at"));
        if replace {
            latest_review.insert(tranche_id, review);
        }
    }
    let projects = projects.as_array().map(Vec::as_slice).unwrap_or_default();
    let mut entries: Vec<(i64, String, Value)> = tranches
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|tranche| {
            let number = tranche
                .get("number")
                .and_then(Value::as_i64)
                .unwrap_or_default();
            let programme = tranche
                .get("programme_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let id = tranche
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let review = latest_review.get(id).copied();
            let authorised = review
                .and_then(|review| review.get("decision"))
                .and_then(Value::as_str)
                == Some("continue");
            let mut total = 0u64;
            let mut active = 0u64;
            let mut closed = 0u64;
            let mut archived = 0u64;
            for project in projects {
                let same_programme =
                    project.get("programme_id").and_then(Value::as_str) == Some(programme);
                let in_tranche = project
                    .get("meta")
                    .and_then(|meta| meta.get("tranche"))
                    .and_then(tranche_number)
                    == Some(number);
                if !(same_programme && in_tranche) {
                    continue;
                }
                total += 1;
                match project.get("status").and_then(Value::as_str) {
                    Some("active") => active += 1,
                    Some("closed") => closed += 1,
                    Some("archived") => archived += 1,
                    _ => {}
                }
            }
            let mut flags: Vec<&str> = Vec::new();
            if review.is_none() {
                flags.push("review_missing");
            }
            if !authorised && closed > 0 {
                flags.push("projects_closed_before_authorisation");
            }
            let review_value = review
                .map(|review| {
                    json!({
                        "decision": review.get("decision").cloned().unwrap_or(Value::Null),
                        "decided_at": review.get("reviewed_at").cloned().unwrap_or(Value::Null),
                    })
                })
                .unwrap_or(Value::Null);
            let entry = json!({
                "number": number,
                "declared_state": tranche.get("state").cloned().unwrap_or(Value::Null),
                "review": review_value,
                "authorised": authorised,
                "projects": {
                    "total": total,
                    "active": active,
                    "closed": closed,
                    "archived": archived,
                },
                "flags": flags,
            });
            (number, programme.to_owned(), entry)
        })
        .collect();
    entries.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    Value::Array(entries.into_iter().map(|(_, _, entry)| entry).collect())
}

/// A `meta.tranche` value is a number in the seed and may be a numeric string in a hand edit.
fn tranche_number(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
}

/// Whether `candidate` is later than `current` by a timestamp field, breaking a tie by row id.
fn row_is_later(candidate: &Value, current: &Value, timestamp: &str) -> bool {
    let candidate_at = candidate
        .get(timestamp)
        .and_then(Value::as_str)
        .unwrap_or_default();
    let current_at = current
        .get(timestamp)
        .and_then(Value::as_str)
        .unwrap_or_default();
    candidate_at > current_at
        || (candidate_at == current_at
            && candidate
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                > current
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default())
}

/// `architecture.phase_progress`: one entry per project that declares `meta.adm_phase` (or has a
/// contract), sorted by project key. Phases `a..h` are always listed; `preliminary` is listed only
/// when declared. A phase is `evidenced` when an artefact, contract or compliance review names it;
/// absence is explicit, and `next_required` is the first phase that still lacks evidence.
fn phase_progress(
    projects: &Value,
    artifacts: &Value,
    contracts: &Value,
    reviews: &Value,
) -> Value {
    let mut artifact_phases: HashSet<(String, String)> = HashSet::new();
    for artifact in artifacts.as_array().map(Vec::as_slice).unwrap_or_default() {
        if let (Some(project), Some(phase)) = (
            artifact.get("project_id").and_then(Value::as_str),
            artifact.get("phase_code").and_then(Value::as_str),
        ) {
            artifact_phases.insert((project.to_owned(), phase.to_owned()));
        }
    }
    let mut contract_by_phase: HashMap<(String, String), &Value> = HashMap::new();
    for contract in contracts.as_array().map(Vec::as_slice).unwrap_or_default() {
        let (Some(project), Some(phase)) = (
            contract.get("project_id").and_then(Value::as_str),
            contract.get("phase_code").and_then(Value::as_str),
        ) else {
            continue;
        };
        let key = (project.to_owned(), phase.to_owned());
        let replace = contract_by_phase
            .get(&key)
            .is_none_or(|current| row_is_later(contract, current, "created_at"));
        if replace {
            contract_by_phase.insert(key, contract);
        }
    }
    let mut review_by_contract: HashMap<&str, &Value> = HashMap::new();
    for review in reviews.as_array().map(Vec::as_slice).unwrap_or_default() {
        let Some(contract_id) = review.get("contract_id").and_then(Value::as_str) else {
            continue;
        };
        let replace = review_by_contract
            .get(contract_id)
            .is_none_or(|current| row_is_later(review, current, "reviewed_at"));
        if replace {
            review_by_contract.insert(contract_id, review);
        }
    }
    let mut entries: Vec<(&str, Value)> = Vec::new();
    for project in projects.as_array().map(Vec::as_slice).unwrap_or_default() {
        let Some(key) = project.get("key").and_then(Value::as_str) else {
            continue;
        };
        let Some(id) = project.get("id").and_then(Value::as_str) else {
            continue;
        };
        let declared_phase = project
            .get("meta")
            .and_then(|meta| meta.get("adm_phase"))
            .and_then(Value::as_str);
        let has_contract = contract_by_phase
            .keys()
            .any(|(project_id, _)| project_id == id);
        if declared_phase.is_none() && !has_contract {
            continue;
        }
        let mut order: Vec<&str> = Vec::new();
        if declared_phase == Some("preliminary") {
            order.push("preliminary");
        }
        order.extend(["a", "b", "c", "d", "e", "f", "g", "h"]);
        let mut phases: Vec<Value> = Vec::new();
        let mut next_required = Value::Null;
        for phase in order {
            let phase_key = (id.to_owned(), phase.to_owned());
            let contract = contract_by_phase.get(&phase_key).copied();
            let compliance = contract
                .and_then(|contract| contract.get("id").and_then(Value::as_str))
                .and_then(|contract_id| review_by_contract.get(contract_id))
                .map(|review| {
                    json!({
                        "outcome": review.get("outcome").cloned().unwrap_or(Value::Null),
                        "reviewed_at": review.get("reviewed_at").cloned().unwrap_or(Value::Null),
                    })
                });
            let evidenced =
                artifact_phases.contains(&phase_key) || contract.is_some() || compliance.is_some();
            if !evidenced && next_required.is_null() {
                next_required = Value::String(phase.to_owned());
            }
            phases.push(json!({
                "phase": phase,
                "state": if evidenced { "evidenced" } else { "not-evidenced" },
                "contract": contract
                    .map(|contract| json!({
                        "id": contract.get("id").cloned().unwrap_or(Value::Null),
                        "kind": contract.get("kind").cloned().unwrap_or(Value::Null),
                        "status": contract.get("status").cloned().unwrap_or(Value::Null),
                    }))
                    .unwrap_or(Value::Null),
                "compliance": compliance.unwrap_or(Value::Null),
            }));
        }
        let declared = declared_phase
            .map(|phase| {
                json!({
                    "adm_phase": phase,
                    "plateau": project
                        .get("meta")
                        .and_then(|meta| meta.get("plateau"))
                        .cloned()
                        .unwrap_or(Value::Null),
                })
            })
            .unwrap_or(Value::Null);
        entries.push((
            key,
            json!({
                "project": key,
                "declared": declared,
                "phases": phases,
                "next_required": next_required,
            }),
        ));
    }
    entries.sort_by(|left, right| left.0.cmp(right.0));
    Value::Array(entries.into_iter().map(|(_, entry)| entry).collect())
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

/// The `id`s of the rows that satisfy a whole-row predicate — for a boundary that reads more than
/// one field (`programme_id`, `status`, `board_count`, `card_count`).
fn ids_where_row(value: &Value, keep: impl Fn(&Value) -> bool) -> HashSet<String> {
    value
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter(|row| keep(row))
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
