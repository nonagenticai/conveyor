#![forbid(unsafe_code)]
//! The prompt catalogue's reads (SPEC D27).
//!
//! Two things live here: a recipe read by exact `(key, version)` pin, and the **facts** a compile
//! draws from — the run's process or workflow, its step, the capability/agent row, the project,
//! the card and the capability's bound knowledge packs. Everything a surface without a run needs
//! (the copilot's conversation) has its own fact reader here too.
//!
//! ⚠️ Facts are read, never stored: the compiled text is derived on every compile, so changing a
//! card changes the next prompt rather than leaving a stored copy that disagrees with the board.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use conveyor_domain::organisation::ReleaseAnatomy;
use conveyor_domain::prompt::{
    CapabilityFacts, CardFacts, KnowledgeFacts, MemorySelector, ProcessFacts, ProjectFacts,
    PromptDefinition, PromptFacts, PromptSection, PromptSource, SelectorError, StepFacts,
};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::memory::MemoryStoreError;

/// One stored recipe: the definition plus the status the caller checks before using it.
#[derive(Clone, Debug)]
pub struct PromptRecord {
    /// The row id (sections point at it).
    pub id: Uuid,
    /// Key, version, title and the ordered sections.
    pub definition: PromptDefinition,
    /// `active` or `retired`.
    pub status: String,
}

/// A catalogue read failure.
#[derive(Debug, thiserror::Error)]
pub enum PromptStoreError {
    /// The database.
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    /// A stored `source_kind` the domain does not know — a schema violation, not a default.
    #[error("prompt section {position} names unknown source kind {kind:?}")]
    UnknownSource {
        /// The stored spelling.
        kind: String,
        /// Where it sat.
        position: i32,
    },
    /// The selector a caller supplied is not a valid selection config.
    #[error(transparent)]
    Selector(#[from] SelectorError),
    /// The card's memory could not be read.
    #[error(transparent)]
    Memory(#[from] MemoryStoreError),
    /// No such version (or key) in this tenant.
    #[error("the prompt does not exist in this tenant")]
    NotFound,
    /// The version is already active; activating it again is a refusal, not a no-op.
    #[error("the prompt version is already active")]
    AlreadyActive,
    /// The version is already retired.
    #[error("the prompt version is already retired")]
    AlreadyRetired,
    /// A concurrent write took the version or the active slot.
    #[error("a concurrent prompt write won the race")]
    Conflict,
}

/// Read one recipe by its exact pin. A retired version is returned (`status` says so) so the
/// caller can refuse it with the ref in hand; an absent one is `None`.
pub async fn get_prompt_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    key: &str,
    version: i32,
) -> Result<Option<PromptRecord>, PromptStoreError> {
    let row: Option<(Uuid, String, i32, String, String, Option<i32>)> = sqlx::query_as(
        "SELECT id, key, version, status, title, budget_chars FROM prompt
          WHERE tenant_id = $1 AND key = $2 AND version = $3",
    )
    .bind(tenant.as_uuid())
    .bind(key)
    .bind(version)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((id, key, version, status, title, budget_chars)) = row else {
        return Ok(None);
    };

    let sections: Vec<(i32, String, bool, String, Value, Option<i32>)> = sqlx::query_as(
        "SELECT position, source_kind, required, template, selector, budget_chars
           FROM prompt_section
          WHERE tenant_id = $1 AND prompt_id = $2
          ORDER BY position",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;

    let mut parsed = Vec::with_capacity(sections.len());
    for (position, kind, required, template, selector, budget_chars) in sections {
        let source = PromptSource::parse(&kind).ok_or_else(|| PromptStoreError::UnknownSource {
            kind: kind.clone(),
            position,
        })?;
        parsed.push(PromptSection {
            position,
            source,
            required,
            template,
            selector,
            budget_chars: budget_chars.and_then(|value| u32::try_from(value).ok()),
        });
    }

    Ok(Some(PromptRecord {
        id,
        definition: PromptDefinition {
            key,
            version,
            title,
            sections: parsed,
            budget_chars: budget_chars.and_then(|value| u32::try_from(value).ok()),
        },
        status,
    }))
}

/// The `run` columns a compile needs, in one row.
#[derive(sqlx::FromRow)]
struct RunFactRow {
    project_id: Uuid,
    card_id: Option<Uuid>,
    process_id: Option<Uuid>,
    workflow_id: Option<Uuid>,
    capability_id: Option<Uuid>,
    capability_agent_id: Option<Uuid>,
}

/// The facts a run-scoped compile draws from. An absent run yields empty facts (a required
/// section then refuses by name); a run whose step row is not visible yet renders the process
/// without a "current step".
pub async fn facts_for_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    run_step_id: Option<Uuid>,
    memory_selector: &MemorySelector,
    release: Option<&ReleaseAnatomy>,
) -> Result<PromptFacts, PromptStoreError> {
    let mut facts = PromptFacts::default();
    let run: Option<RunFactRow> = sqlx::query_as(
        "SELECT project_id, card_id, process_id, workflow_id, capability_id, capability_agent_id
           FROM run WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(run) = run else {
        return Ok(facts);
    };
    let RunFactRow {
        project_id,
        card_id,
        process_id,
        workflow_id,
        capability_id,
        capability_agent_id,
    } = run;

    facts.step = read_step(conn, tenant, run_step_id).await?;
    facts.process = match process_id {
        Some(id) => read_process(conn, tenant, id).await?,
        None => match workflow_id {
            Some(id) => read_workflow(conn, tenant, id).await?,
            None => None,
        },
    };
    // A plain run has no published procedure, so the release's own request renders in the one
    // `process` section: the job's charter, procedure reference and authority (SPEC_V2 §4.5's
    // Request source), with the run-level step as the plan. No new section kind, no stored PDDL.
    if facts.process.is_none()
        && let Some(anatomy) = release
    {
        facts.process = Some(request_facts(anatomy));
    }
    facts.project = read_project(conn, tenant, project_id).await?;
    facts.card = match card_id {
        Some(id) => read_card(conn, tenant, id).await?,
        None => None,
    };
    if let Some(capability_id) = capability_id {
        // A run whose process binds one agent records it on the run; a process that binds a
        // DIFFERENT agent per step records none, so the step's own binding is the executor whose
        // role, actions and params compile into this prompt. Without this fallback such a
        // process's prompt had no capability source at all and the step failed `prompt_unresolved`.
        let agent_id = match capability_agent_id {
            Some(id) => Some(id),
            None => read_step_agent(conn, tenant, run_step_id).await?,
        };
        facts.capability = read_capability(conn, tenant, capability_id, agent_id).await?;
        // The release is the execution authority (ADR 0010): the compiled capability section
        // names the same model and the same caps the loop will actually use, never the row's
        // values when the release pins lower ones.
        if let Some(anatomy) = release
            && let Some(capability) = facts.capability.as_mut()
        {
            capability.model_ref = anatomy
                .effective_model_ref(&capability.model_ref)
                .to_owned();
            capability.max_iterations =
                i32::from(anatomy.effective_max_iterations(capability.max_iterations));
            capability.timeout_secs =
                i32::try_from(anatomy.effective_timeout_ms(capability.timeout_secs) / 1000)
                    .unwrap_or(capability.timeout_secs);
        }
        let packs: Vec<(String, i32)> = sqlx::query_as(
            "SELECT pack_key, pack_version FROM capability_knowledge
              WHERE tenant_id = $1 AND capability_id = $2
              ORDER BY pack_key, pack_version",
        )
        .bind(tenant.as_uuid())
        .bind(capability_id)
        .fetch_all(&mut *conn)
        .await?;
        facts.knowledge = KnowledgeFacts {
            packs: packs
                .into_iter()
                .map(|(key, version)| format!("{key}@{version}"))
                .collect(),
            hits: Vec::new(),
        };
    }
    if let Some(card_id) = card_id {
        facts.memory =
            crate::memory::select_for_prompt_tx(&mut *conn, tenant, card_id, memory_selector)
                .await?;
    }
    Ok(facts)
}

/// The Request source for a run with no published procedure: the release's own definition.
fn request_facts(anatomy: &ReleaseAnatomy) -> ProcessFacts {
    let mut description = String::new();
    if let Some(charter) = anatomy.charter.as_deref() {
        description.push_str(charter);
    }
    if let Some(procedure) = anatomy.procedure_ref.as_deref() {
        if !description.is_empty() {
            description.push('\n');
        }
        description.push_str(&format!("request: {procedure}"));
    }
    if let Some(authority) = anatomy.authority.as_deref() {
        if !description.is_empty() {
            description.push('\n');
        }
        description.push_str(&format!("authority: {authority}"));
    }
    ProcessFacts {
        key: if anatomy.job_key.is_empty() {
            "job".to_owned()
        } else {
            anatomy.job_key.clone()
        },
        title: if anatomy.title.is_empty() {
            anatomy.job_key.clone()
        } else {
            anatomy.title.clone()
        },
        description,
        version: 0,
        steps: vec![(1, "agent.run".to_owned(), anatomy.gate.clone())],
    }
}

/// The facts a copilot conversation can supply: its project, and nothing run-scoped.
pub async fn facts_for_conversation_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    conversation_id: Uuid,
) -> Result<PromptFacts, PromptStoreError> {
    let project: Option<Option<Uuid>> =
        sqlx::query_scalar("SELECT project_id FROM conversation WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(conversation_id)
            .fetch_optional(&mut *conn)
            .await?;
    let mut facts = PromptFacts::default();
    if let Some(Some(project_id)) = project {
        facts.project = read_project(conn, tenant, project_id).await?;
    }
    Ok(facts)
}

/// The agent a process step binds, when the run itself records none (a process that names a
/// different executor per step).
async fn read_step_agent(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_step_id: Option<Uuid>,
) -> Result<Option<Uuid>, PromptStoreError> {
    let Some(id) = run_step_id else {
        return Ok(None);
    };
    let agent: Option<Option<Uuid>> = sqlx::query_scalar(
        "SELECT s.process_agent_id FROM run_step r
           JOIN process_step s ON s.id = r.process_step_id AND s.tenant_id = r.tenant_id
          WHERE r.tenant_id = $1 AND r.id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(agent.flatten())
}

async fn read_step(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_step_id: Option<Uuid>,
) -> Result<Option<StepFacts>, PromptStoreError> {
    let Some(id) = run_step_id else {
        return Ok(None);
    };
    let row: Option<(i32, String, Option<String>)> = sqlx::query_as(
        "SELECT position, action, node_id FROM run_step
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((position, action, node_id)) = row else {
        return Ok(None);
    };
    // `on_result` lives on the process step, joined only when this row is process-driven.
    let on_result: Option<String> = sqlx::query_scalar(
        "SELECT s.on_result FROM run_step r
           JOIN process_step s ON s.id = r.process_step_id AND s.tenant_id = r.tenant_id
          WHERE r.tenant_id = $1 AND r.id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(Some(StepFacts {
        position,
        action,
        node_id,
        on_result,
    }))
}

async fn read_process(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<ProcessFacts>, PromptStoreError> {
    let row: Option<(String, String, String, i64)> = sqlx::query_as(
        "SELECT key, title, description, version FROM process
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((key, title, description, version)) = row else {
        return Ok(None);
    };
    // `action` is NULL for a gate or a call step (only `computed` invokes an action), so it is
    // read as nullable and rendered empty. Decoding it as `String` failed the whole prompt
    // compile with a 500 for any process that carries a gate — which is every `ask_fr` run.
    let steps: Vec<(i32, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT position, action, gate_kind FROM process_step
          WHERE tenant_id = $1 AND process_id = $2
          ORDER BY position",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    let steps = steps
        .into_iter()
        .map(|(position, action, gate_kind)| (position, action.unwrap_or_default(), gate_kind))
        .collect();
    Ok(Some(ProcessFacts {
        key,
        title,
        description,
        version,
        steps,
    }))
}

/// A workflow run's procedural component: the workflow's identity plus its nodes, in definition
/// order, as `node: action` steps. There is no description column on `workflow`, so the
/// description is empty rather than invented.
async fn read_workflow(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<ProcessFacts>, PromptStoreError> {
    let row: Option<(String, String, i64, Value)> = sqlx::query_as(
        "SELECT key, title, version, definition FROM workflow
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((key, title, version, definition)) = row else {
        return Ok(None);
    };
    let mut steps = Vec::new();
    if let Some(nodes) = definition.get("nodes").and_then(Value::as_array) {
        for (index, node) in nodes.iter().enumerate() {
            let node_id = node.get("id").and_then(Value::as_str).unwrap_or("?");
            let action = node
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or("(no action)");
            steps.push((
                i32::try_from(index).unwrap_or(i32::MAX) + 1,
                format!("{node_id}: {action}"),
                None,
            ));
        }
    }
    Ok(Some(ProcessFacts {
        key,
        title,
        description: String::new(),
        version,
        steps,
    }))
}

async fn read_project(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<ProjectFacts>, PromptStoreError> {
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT key, name, description FROM project WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(|(key, title, description)| ProjectFacts {
        key,
        title,
        description,
    }))
}

async fn read_card(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<CardFacts>, PromptStoreError> {
    let row: Option<(String, String, Option<Uuid>)> = sqlx::query_as(
        "SELECT name, description, list_id FROM card WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((name, description, list_id)) = row else {
        return Ok(None);
    };
    let list_name: Option<String> =
        sqlx::query_scalar("SELECT name FROM list WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(list_id)
            .fetch_optional(&mut *conn)
            .await?;
    Ok(Some(CardFacts {
        id: id.to_string(),
        name,
        description,
        list_name,
    }))
}

/// The `capability_agent` columns a compile needs.
#[derive(sqlx::FromRow)]
struct AgentFactRow {
    key: String,
    model_ref: String,
    allowed_actions: Vec<String>,
    max_iterations: i32,
    timeout_secs: i32,
    params: Value,
}

async fn read_capability(
    conn: &mut PgConnection,
    tenant: TenantId,
    capability_id: Uuid,
    agent_id: Option<Uuid>,
) -> Result<Option<CapabilityFacts>, PromptStoreError> {
    let capability: Option<(String, String)> =
        sqlx::query_as("SELECT code, label FROM capability WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(capability_id)
            .fetch_optional(&mut *conn)
            .await?;
    let Some((key, title)) = capability else {
        return Ok(None);
    };
    let Some(agent_id) = agent_id else {
        return Ok(None);
    };
    let agent: Option<AgentFactRow> = sqlx::query_as(
        "SELECT key, model_ref, allowed_actions, max_iterations, timeout_secs, params
           FROM capability_agent WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(agent_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(agent.map(|agent| CapabilityFacts {
        id: capability_id,
        key,
        title,
        agent_key: agent.key,
        model_ref: agent.model_ref,
        // The binding's declared perspective (slice S21 §3.2). `params` is the existing editable
        // config on `capability_agents.create|update`; a first-class column would change those
        // action schemas and the served spec, which S21 step 1 deliberately does not touch.
        intended_role: agent
            .params
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        allowed_actions: agent.allowed_actions,
        max_iterations: agent.max_iterations,
        timeout_secs: agent.timeout_secs,
    }))
}

/// One section an author supplies to `prompt_recipes.create`.
#[derive(Clone, Debug)]
pub struct NewPromptSection {
    /// 1-based position; unique within the recipe.
    pub position: i32,
    /// What it renders from.
    pub source: PromptSource,
    /// A required section refuses a compile with no source.
    pub required: bool,
    /// The literal text, for a `system` section.
    pub template: String,
    /// The selection config for a rendered kind.
    pub selector: Value,
    /// Its own character ceiling.
    pub budget_chars: Option<u32>,
}

/// A new recipe version, before it is stored.
#[derive(Clone, Debug)]
pub struct NewPrompt {
    /// The recipe key.
    pub key: String,
    /// Human title.
    pub title: String,
    /// In order.
    pub sections: Vec<NewPromptSection>,
}

/// One catalogue row without its sections, for `prompt_recipes.list`.
#[derive(Clone, Debug)]
pub struct PromptSummary {
    /// The recipe key.
    pub key: String,
    /// Dense from 1.
    pub version: i32,
    /// Human title.
    pub title: String,
    /// `draft`, `active` or `retired`.
    pub status: String,
    /// When the version was created.
    pub created_at: DateTime<Utc>,
    /// When it last changed.
    pub updated_at: DateTime<Utc>,
}

/// True when the database refused a duplicate (the version or the active slot).
fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(database) if database.code().as_deref() == Some("23505"))
}

/// Create the next version of a key as a `draft`.
///
/// The version number is computed in the INSERT itself, so a missing key starts at 1; a
/// concurrent create of the same key collides on `(tenant, key, version)` and is a `Conflict`,
/// never a silent overwrite.
pub async fn create_prompt_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    prompt: &NewPrompt,
) -> Result<PromptRecord, PromptStoreError> {
    let inserted: Result<(Uuid, i32), sqlx::Error> = sqlx::query_as(
        "INSERT INTO prompt (tenant_id, key, version, status, title)
         SELECT $1, $2, COALESCE(MAX(version), 0) + 1, 'draft', $3
           FROM prompt
          WHERE tenant_id = $1 AND key = $2
         RETURNING id, version",
    )
    .bind(tenant.as_uuid())
    .bind(&prompt.key)
    .bind(&prompt.title)
    .fetch_one(&mut *conn)
    .await;
    let (id, version) = inserted.map_err(|error| {
        if is_unique_violation(&error) {
            PromptStoreError::Conflict
        } else {
            PromptStoreError::Db(error)
        }
    })?;

    for section in &prompt.sections {
        sqlx::query(
            "INSERT INTO prompt_section
                 (tenant_id, prompt_id, position, source_kind, required, template, selector,
                  budget_chars)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(section.position)
        .bind(section.source.as_str())
        .bind(section.required)
        .bind(&section.template)
        .bind(&section.selector)
        .bind(section.budget_chars.map(i64::from))
        .execute(&mut *conn)
        .await?;
    }

    get_prompt_tx(&mut *conn, tenant, &prompt.key, version)
        .await?
        .ok_or(PromptStoreError::NotFound)
}

/// One version by key: exact when `version` is named, otherwise the key's active version.
pub async fn get_prompt_by_key_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    key: &str,
    version: Option<i32>,
) -> Result<Option<PromptRecord>, PromptStoreError> {
    if let Some(version) = version {
        return get_prompt_tx(&mut *conn, tenant, key, version).await;
    }
    let active: Option<i32> = sqlx::query_scalar(
        "SELECT version FROM prompt
          WHERE tenant_id = $1 AND key = $2 AND status = 'active'",
    )
    .bind(tenant.as_uuid())
    .bind(key)
    .fetch_optional(&mut *conn)
    .await?;
    match active {
        Some(version) => get_prompt_tx(&mut *conn, tenant, key, version).await,
        None => Ok(None),
    }
}

/// The columns `list_prompts_tx` reads; a row type rather than a six-tuple, which clippy refuses.
#[derive(sqlx::FromRow)]
struct PromptSummaryRow {
    key: String,
    version: i32,
    title: String,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

/// One page of catalogue rows, newest version first within a key.
pub async fn list_prompts_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    key: Option<&str>,
    status: Option<&str>,
    limit: i64,
) -> Result<Vec<PromptSummary>, PromptStoreError> {
    let rows: Vec<PromptSummaryRow> = sqlx::query_as(
        "SELECT key, version, title, status, created_at, updated_at
           FROM prompt
          WHERE tenant_id = $1
            AND ($2::text IS NULL OR key = $2)
            AND ($3::text IS NULL OR status = $3)
          ORDER BY key, version DESC
          LIMIT $4",
    )
    .bind(tenant.as_uuid())
    .bind(key)
    .bind(status)
    .bind(limit)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| PromptSummary {
            key: row.key,
            version: row.version,
            title: row.title,
            status: row.status,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
        .collect())
}

/// Activate one version: the key's current active row is retired in the same transaction.
///
/// The target row is locked first, and `uq_prompt_active` is the final authority: two
/// activations of the same version race to `AlreadyActive`, and two different versions cannot
/// leave the key with two active rows.
pub async fn activate_prompt_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    key: &str,
    version: i32,
) -> Result<PromptRecord, PromptStoreError> {
    let status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM prompt
          WHERE tenant_id = $1 AND key = $2 AND version = $3
          FOR UPDATE",
    )
    .bind(tenant.as_uuid())
    .bind(key)
    .bind(version)
    .fetch_optional(&mut *conn)
    .await?;
    match status.as_deref() {
        None => return Err(PromptStoreError::NotFound),
        Some("active") => return Err(PromptStoreError::AlreadyActive),
        Some("draft" | "retired") => {}
        Some(_) => return Err(PromptStoreError::Conflict),
    }
    sqlx::query(
        "UPDATE prompt SET status = 'retired', updated_at = now()
          WHERE tenant_id = $1 AND key = $2 AND status = 'active' AND version <> $3",
    )
    .bind(tenant.as_uuid())
    .bind(key)
    .bind(version)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE prompt SET status = 'active', updated_at = now()
          WHERE tenant_id = $1 AND key = $2 AND version = $3",
    )
    .bind(tenant.as_uuid())
    .bind(key)
    .bind(version)
    .execute(&mut *conn)
    .await
    .map_err(|error| {
        if is_unique_violation(&error) {
            PromptStoreError::Conflict
        } else {
            PromptStoreError::Db(error)
        }
    })?;
    get_prompt_tx(&mut *conn, tenant, key, version)
        .await?
        .ok_or(PromptStoreError::NotFound)
}

/// Retire one version. A retired row stays readable by exact pin.
pub async fn retire_prompt_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    key: &str,
    version: i32,
) -> Result<PromptRecord, PromptStoreError> {
    let status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM prompt
          WHERE tenant_id = $1 AND key = $2 AND version = $3
          FOR UPDATE",
    )
    .bind(tenant.as_uuid())
    .bind(key)
    .bind(version)
    .fetch_optional(&mut *conn)
    .await?;
    match status.as_deref() {
        None => return Err(PromptStoreError::NotFound),
        Some("retired") => return Err(PromptStoreError::AlreadyRetired),
        Some(_) => {}
    }
    sqlx::query(
        "UPDATE prompt SET status = 'retired', updated_at = now()
          WHERE tenant_id = $1 AND key = $2 AND version = $3",
    )
    .bind(tenant.as_uuid())
    .bind(key)
    .bind(version)
    .execute(&mut *conn)
    .await?;
    get_prompt_tx(&mut *conn, tenant, key, version)
        .await?
        .ok_or(PromptStoreError::NotFound)
}
