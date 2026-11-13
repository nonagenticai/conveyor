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
    /// `artefacts.json` data: governed assets and document metadata.
    pub artefacts: Value,
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
    let projects = rows(
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

    let departments = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.name), '[]'::jsonb) FROM (
             SELECT id, name, parent_id, version, created_at, updated_at
               FROM department WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;

    let boards = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.project_id, t.position, t.id), '[]'::jsonb) FROM (
             SELECT id, project_id, name, position, version, created_at
               FROM board WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let lists = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.board_id, t.position, t.id), '[]'::jsonb) FROM (
             SELECT id, board_id, name, position, kind, color, wip_limit, version
               FROM list WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let cards = rows(
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
    let runs = rows(
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
    let run_steps = rows(
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
    let approvals = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.created_at, t.id), '[]'::jsonb) FROM (
             SELECT id, run_id, card_id, project_id, gate, kind, status, required_decisions,
                    requested_by, expires_at, version, created_at, updated_at
               FROM approval WHERE tenant_id = $1
         ) t",
        tenant_id,
    )
    .await?;
    let approval_decisions = rows(
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
    let comments = rows(
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
    let documents = rows(
        &mut *conn,
        "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY t.created_at, t.id), '[]'::jsonb) FROM (
             SELECT id, project_id, kind, title, status, content_hash, version, created_at,
                    updated_at
               FROM document WHERE tenant_id = $1
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

    Ok(SnapshotData {
        programme: json!({"programme": programme, "projects": projects}),
        organisation: json!({
            "industries": [],
            "departments": departments,
            "jobs": [],
            "releases": [],
            "bindings": []
        }),
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
        artefacts: json!({"governed_assets": assets, "documents": documents}),
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
