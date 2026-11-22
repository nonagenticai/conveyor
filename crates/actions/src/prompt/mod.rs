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

pub mod recipes;

use conveyor_domain::TenantId;
use conveyor_domain::capability::PromptRef;
use conveyor_domain::organisation::ReleaseAnatomy;
use conveyor_domain::prompt::{
    CompiledPrompt, KnowledgeHit, KnowledgeSelector, MemorySelector, PromptCompileError,
    PromptFacts, PromptSource,
};
use conveyor_store as store;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::invoke::ActionError;
use crate::knowledge::retrieval;

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
    /// Knowledge retrieval refused (a required pack is missing or unreachable).
    #[error(transparent)]
    Knowledge(#[from] ActionError),
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
            Self::Knowledge(error) => error,
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
    compile_for_run_with_pins(pool, tenant, run_id, run_step_id, reference, None).await
}

/// [`compile_for_run`] with the run's pinned release in force (S21 step 4).
///
/// The release supplies the model and caps the compiled capability section names, and the
/// Request source for a plain run that has no published procedure. It is still the ONE compiler:
/// there is no second render path, and the compiled text is still never stored.
pub async fn compile_for_run_with_pins(
    pool: &sqlx::PgPool,
    tenant: TenantId,
    run_id: Uuid,
    run_step_id: Option<Uuid>,
    reference: &str,
    release: Option<&ReleaseAnatomy>,
) -> Result<CompiledPrompt, PromptError> {
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(store::prompt::PromptStoreError::Db)?;
    let (key, version) = parse_ref(reference)?;
    let record = resolve(&mut tx, tenant, &key, version, reference).await?;
    let memory_selector = memory_selector_of(&record.definition)?;
    let knowledge_selector = knowledge_selector_of(&record.definition)?;
    let mut facts = store::prompt::facts_for_run_tx(
        &mut tx,
        tenant,
        run_id,
        run_step_id,
        &memory_selector,
        release,
    )
    .await?;
    // Resolve the knowledge targets in the transaction; retrieve after it commits, so no
    // network call is ever made while a database transaction is open (S21 §4, ADR 0009).
    let targets = match (&knowledge_selector, facts.capability.as_ref()) {
        (Some(_), Some(capability)) => Some(
            retrieval::resolve_targets_tx(&mut tx, tenant, capability.id, "agent.run")
                .await
                .map_err(PromptError::Knowledge)?,
        ),
        _ => None,
    };
    tx.commit()
        .await
        .map_err(store::prompt::PromptStoreError::Db)?;

    if let (Some(selector), Some(targets)) = (knowledge_selector, targets)
        && let Some(query) = knowledge_query(&selector, &facts)
    {
        let retrieved = retrieval::retrieve("agent.run", &targets, &query, selector.top_k)
            .await
            .map_err(PromptError::Knowledge)?;
        if !retrieved.packs.is_empty() {
            facts.knowledge.packs = retrieved.packs;
        }
        facts.knowledge.hits = knowledge_hits(&retrieved.hits);
    }
    compile(record, facts, reference)
}

/// Compile a run's prompt inside the caller's transaction, without knowledge retrieval.
///
/// This is the store-only seam for a caller that already holds a transaction; the executor path
/// is [`compile_for_run`], which resolves the targets, commits, and then retrieves.
pub async fn compile_for_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    run_step_id: Option<Uuid>,
    reference: &str,
) -> Result<CompiledPrompt, PromptError> {
    compile_for_run_tx_with_pins(conn, tenant, run_id, run_step_id, reference, None).await
}

/// [`compile_for_run_tx`] with the run's pinned release in force (S21 step 4).
pub async fn compile_for_run_tx_with_pins(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    run_step_id: Option<Uuid>,
    reference: &str,
    release: Option<&ReleaseAnatomy>,
) -> Result<CompiledPrompt, PromptError> {
    let (key, version) = parse_ref(reference)?;
    let record = resolve(conn, tenant, &key, version, reference).await?;
    let memory_selector = memory_selector_of(&record.definition)?;
    let facts = store::prompt::facts_for_run_tx(
        &mut *conn,
        tenant,
        run_id,
        run_step_id,
        &memory_selector,
        release,
    )
    .await?;
    compile(record, facts, reference)
}

/// The memory selection the recipe declares, or the default when it has no memory section.
fn memory_selector_of(
    definition: &conveyor_domain::prompt::PromptDefinition,
) -> Result<MemorySelector, PromptError> {
    for section in &definition.sections {
        if section.source == PromptSource::Memory {
            return MemorySelector::parse(&section.selector).map_err(|error| {
                PromptError::Store(store::prompt::PromptStoreError::Selector(error))
            });
        }
    }
    Ok(MemorySelector::default())
}

/// The knowledge selection the recipe declares, if it has a knowledge section.
fn knowledge_selector_of(
    definition: &conveyor_domain::prompt::PromptDefinition,
) -> Result<Option<KnowledgeSelector>, PromptError> {
    for section in &definition.sections {
        if section.source == PromptSource::Knowledge {
            return KnowledgeSelector::parse(&section.selector)
                .map(Some)
                .map_err(|error| {
                    PromptError::Store(store::prompt::PromptStoreError::Selector(error))
                });
        }
    }
    Ok(None)
}

/// The query a knowledge section asks: the selector's literal, else the card name and description.
fn knowledge_query(selector: &KnowledgeSelector, facts: &PromptFacts) -> Option<String> {
    let text = selector.query.clone().or_else(|| {
        let card = facts.card.as_ref()?;
        let mut text = card.name.clone();
        if !card.description.trim().is_empty() {
            text.push('\n');
            text.push_str(card.description.trim());
        }
        (!text.trim().is_empty()).then_some(text)
    })?;
    let capped: String = text.chars().take(2_000).collect();
    (!capped.trim().is_empty()).then_some(capped)
}

/// Normalise the upstream hit documents into the text the compiler renders and hashes.
fn knowledge_hits(hits: &[Value]) -> Vec<KnowledgeHit> {
    hits.iter()
        .enumerate()
        .filter_map(|(index, hit)| {
            let id = hit
                .get("id")
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| value.to_string())
                })
                .or_else(|| {
                    hit.get("document_id")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| format!("hit-{index}"));
            let text = hit
                .get("text")
                .and_then(Value::as_str)
                .or_else(|| hit.get("content").and_then(Value::as_str))
                .or_else(|| hit.get("body").and_then(Value::as_str))
                .map(str::to_owned)
                .unwrap_or_else(|| hit.to_string());
            let text = text.trim();
            (!text.is_empty()).then(|| KnowledgeHit {
                id,
                text: text.to_owned(),
            })
        })
        .collect()
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
