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

use conveyor_domain::TenantId;
use conveyor_domain::prompt::{
    CapabilityFacts, CardFacts, ProcessFacts, ProjectFacts, PromptDefinition, PromptFacts,
    PromptSection, PromptSource, StepFacts,
};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

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

    let sections: Vec<(i32, String, bool, String, Option<i32>)> = sqlx::query_as(
        "SELECT position, source_kind, required, template, budget_chars FROM prompt_section
          WHERE tenant_id = $1 AND prompt_id = $2
          ORDER BY position",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;

    let mut parsed = Vec::with_capacity(sections.len());
    for (position, kind, required, template, budget_chars) in sections {
        let source = PromptSource::parse(&kind).ok_or_else(|| PromptStoreError::UnknownSource {
            kind: kind.clone(),
            position,
        })?;
        parsed.push(PromptSection {
            position,
            source,
            required,
            template,
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
        let packs: Vec<(String, i32)> = sqlx::query_as(
            "SELECT pack_key, pack_version FROM capability_knowledge
              WHERE tenant_id = $1 AND capability_id = $2
              ORDER BY pack_key, pack_version",
        )
        .bind(tenant.as_uuid())
        .bind(capability_id)
        .fetch_all(&mut *conn)
        .await?;
        facts.knowledge = packs
            .into_iter()
            .map(|(key, version)| format!("{key}@{version}"))
            .collect();
    }
    Ok(facts)
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
