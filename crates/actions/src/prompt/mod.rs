#![forbid(unsafe_code)]
//! The prompt compiler's action-layer seam (SPEC D27).
//!
//! Three surfaces resolve their system message here, and all three go through the same catalogue:
//!
//! * **`agent.run`** — the resolved path of [`crate::workflow::agent::resolve`], on a run's step;
//! * **the copilot** — `copilot_system@1`, on a conversation's project;
//! * **the prompt audit** — `prompt_audit@1`, which has no run and only a system section.
//!
//! A missing or retired ref is a **readiness refusal**, not an empty prompt: a run that names a
//! prompt the tenant does not have must fail before it spends a model call, and the error names
//! the ref and the missing section. That is the same fail-closed rule the executor already used
//! when the catalogue did not exist (`prompt_unresolved`).

use conveyor_domain::TenantId;
use conveyor_domain::capability::PromptRef;
use conveyor_domain::prompt::{CompiledPrompt, PromptCompileError, PromptFacts, PromptSource};
use conveyor_store as store;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::invoke::ActionError;

/// Why a prompt could not be compiled.
#[derive(Debug, thiserror::Error)]
pub enum PromptError {
    /// `prompt_ref` does not parse as `<name>@<version>`.
    #[error("prompt ref {0:?} is not <name>@<version>")]
    InvalidRef(String),
    /// No catalogue row with that exact pin.
    #[error("prompt {0:?} is not in this tenant's catalogue")]
    Unresolved(String),
    /// The row exists but is retired: a new run may not pin it.
    #[error("prompt {reference:?} is {status}, so a new run may not pin it")]
    Retired {
        /// The pinned ref.
        reference: String,
        /// The stored status.
        status: String,
    },
    /// A section the recipe requires has no source on this surface.
    #[error("prompt {reference:?}: {kind:?} is required and this surface has no such source")]
    MissingSource {
        /// The pinned ref.
        reference: String,
        /// The missing kind.
        kind: PromptSource,
    },
    /// The recipe produced no text.
    #[error("prompt {0:?} rendered no section")]
    Empty(String),
    /// A section (or the whole prompt) exceeded its recipe budget. Refused, never truncated.
    #[error("prompt {reference:?}: {measured} chars over a {limit}-char budget")]
    BudgetExceeded {
        /// The pinned ref.
        reference: String,
        /// `Some(kind)` for one section, `None` for the whole prompt.
        section: Option<PromptSource>,
        /// The rendered size, in characters.
        measured: u32,
        /// The recipe's ceiling.
        limit: u32,
    },
    /// The catalogue read failed.
    #[error(transparent)]
    Store(#[from] store::prompt::PromptStoreError),
}

impl PromptError {
    /// Map onto the action surface: a readiness refusal is `409 capability_not_ready` naming the
    /// field; a database failure is an internal error, never a client fault.
    pub fn into_action_error(self, action: &str) -> ActionError {
        match self {
            Self::Store(error) => {
                tracing::error!(%error, action, "the prompt catalogue read failed");
                ActionError::internal()
            }
            Self::BudgetExceeded {
                reference,
                section,
                measured,
                limit,
            } => {
                let scope = match section {
                    Some(kind) => format!("the {} section", kind.as_str()),
                    None => "the compiled prompt".to_owned(),
                };
                let detail = format!(
                    "{reference}: {scope} rendered {measured} chars, over its {limit}-char budget"
                );
                ActionError::new(
                    409,
                    "capability_not_ready",
                    format!("{action}: the run's prompt is not ready"),
                )
                .with_errors(vec![crate::spec::FieldError {
                    field: "prompt_ref".to_owned(),
                    code: "prompt_over_budget".to_owned(),
                    detail,
                }])
            }
            failure => {
                let detail = failure.to_string();
                ActionError::new(
                    409,
                    "capability_not_ready",
                    format!("{action}: the run's prompt is not ready"),
                )
                .with_errors(vec![crate::spec::FieldError {
                    field: "prompt_ref".to_owned(),
                    code: "prompt_unresolved".to_owned(),
                    detail,
                }])
            }
        }
    }
}

/// Compile a run's prompt on its own connection (the resolved `agent.run` path, which has already
/// committed its capability read).
pub async fn compile_for_run(
    pool: &sqlx::PgPool,
    tenant: TenantId,
    run_id: Uuid,
    run_step_id: Option<Uuid>,
    reference: &str,
) -> Result<CompiledPrompt, PromptError> {
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(store::prompt::PromptStoreError::Db)?;
    let compiled = compile_for_run_tx(&mut tx, tenant, run_id, run_step_id, reference).await?;
    tx.commit()
        .await
        .map_err(store::prompt::PromptStoreError::Db)?;
    Ok(compiled)
}

/// Compile a run's prompt inside the caller's transaction (the executor already holds one while
/// it reads the capability agent).
pub async fn compile_for_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    run_step_id: Option<Uuid>,
    reference: &str,
) -> Result<CompiledPrompt, PromptError> {
    let (key, version) = parse_ref(reference)?;
    let record = resolve(conn, tenant, &key, version, reference).await?;
    let facts = store::prompt::facts_for_run_tx(&mut *conn, tenant, run_id, run_step_id).await?;
    compile(record, facts, reference)
}

/// Compile the copilot's system message for one conversation (its project facts; no run).
pub async fn compile_for_copilot(
    pool: &sqlx::PgPool,
    tenant: TenantId,
    conversation_id: Uuid,
    reference: &str,
) -> Result<CompiledPrompt, PromptError> {
    let (key, version) = parse_ref(reference)?;
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(store::prompt::PromptStoreError::Db)?;
    let record = resolve(&mut tx, tenant, &key, version, reference).await?;
    let facts = store::prompt::facts_for_conversation_tx(&mut tx, tenant, conversation_id).await?;
    tx.commit()
        .await
        .map_err(store::prompt::PromptStoreError::Db)?;
    compile(record, facts, reference)
}

/// Compile a prompt with no run facts (the prompt audit's own instruction).
pub async fn compile_for_audit(
    pool: &sqlx::PgPool,
    tenant: TenantId,
    reference: &str,
) -> Result<CompiledPrompt, PromptError> {
    let (key, version) = parse_ref(reference)?;
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(store::prompt::PromptStoreError::Db)?;
    let record = resolve(&mut tx, tenant, &key, version, reference).await?;
    tx.commit()
        .await
        .map_err(store::prompt::PromptStoreError::Db)?;
    compile(record, PromptFacts::default(), reference)
}

/// Parse `<name>@<version>` with the catalogue's grammar (the same one `capability_agents.create`
/// enforces, so a stored ref always parses).
fn parse_ref(reference: &str) -> Result<(String, i32), PromptError> {
    let parsed =
        PromptRef::parse(reference).map_err(|_| PromptError::InvalidRef(reference.to_owned()))?;
    let version =
        i32::try_from(parsed.version).map_err(|_| PromptError::InvalidRef(reference.to_owned()))?;
    Ok((parsed.name, version))
}

/// Read the exact pin and refuse an absent or retired one before rendering anything.
async fn resolve(
    conn: &mut PgConnection,
    tenant: TenantId,
    key: &str,
    version: i32,
    reference: &str,
) -> Result<store::prompt::PromptRecord, PromptError> {
    let record = store::prompt::get_prompt_tx(&mut *conn, tenant, key, version)
        .await?
        .ok_or_else(|| PromptError::Unresolved(reference.to_owned()))?;
    if record.status != "active" {
        return Err(PromptError::Retired {
            reference: reference.to_owned(),
            status: record.status.clone(),
        });
    }
    Ok(record)
}

/// Render and map the domain's two refusals onto the catalogue's vocabulary.
fn compile(
    record: store::prompt::PromptRecord,
    facts: PromptFacts,
    reference: &str,
) -> Result<CompiledPrompt, PromptError> {
    conveyor_domain::prompt::compile(&record.definition, &facts).map_err(|error| match error {
        PromptCompileError::MissingRequired { kind } => PromptError::MissingSource {
            reference: reference.to_owned(),
            kind,
        },
        PromptCompileError::Empty => PromptError::Empty(reference.to_owned()),
        PromptCompileError::SectionOverBudget {
            kind,
            measured,
            limit,
        } => PromptError::BudgetExceeded {
            reference: reference.to_owned(),
            section: Some(kind),
            measured,
            limit,
        },
        PromptCompileError::PromptOverBudget { measured, limit } => PromptError::BudgetExceeded {
            reference: reference.to_owned(),
            section: None,
            measured,
            limit,
        },
    })
}
