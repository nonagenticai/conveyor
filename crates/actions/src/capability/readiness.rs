#![forbid(unsafe_code)]
//! The capability readiness projection (S15 step 12; D22 / CONTRACT §17).
//!
//! One computation, two surfaces: `capabilities.get` returns it as `{ready, blocking[]}`, and
//! `process_runs.start` refuses on it with `409 capability_not_ready` **before** the run row,
//! the `run_step` rows or the first job — so a blocked capability holds no live-run slot and
//! leaves nothing to reap.
//!
//! The blockers are the domain's [`ReadinessBlocker`] vocabulary; [`readiness_fields`] is the ONE
//! mapping to the wire `FieldError`, used by both surfaces, so the GET's `blocking[]` and the
//! 409's `errors[]` cannot drift apart. `blocking[]` is CONTRACT §17's name for the set; the
//! 409 carries it in the API-wide `errors[]` envelope (build record, S15 §13, CCR 1).
//!
//! **What makes a capability ready** — every check is a fact, never a stored flag:
//!
//! * it exists and is `active`;
//! * exactly one active `capability_agent` exists (zero or several is a blocker, never a silent
//!   pick);
//! * that agent's `prompt_ref`, when set, is a prompt the tenant's catalogue has at that exact pin
//!   **and** at `active` status — a retired pin is not runnable (the prompt catalogue is
//!   `s13-agent-inference-nodes`' module; this is a read-only consumer);
//! * every `allowed_actions` entry is in the live registry (so `agent.run` may be named before
//!   S13 registers it — a readiness blocker, not a write error);
//! * every **required** knowledge pack resolves to at least one leaf. `pack_leaves` returns
//!   `Ok(vec![])` for a published-but-empty pack (strata's own comment says the caller decides),
//!   and CONTRACT §17 says "no resolvable leaves" is not ready.
//!
//! ⚠️ Do **not** reuse `knowledge/query.rs`'s pack-leaves cache here. A cached leaf set can say
//! "ready" after the pack was pruned, which is exactly what this guard exists to catch. Readiness
//! pays the same live call the start path already paid before step 12.

use conveyor_domain::TenantId;
use conveyor_domain::capability::{PromptRef, Readiness, ReadinessBlocker};
use conveyor_store as store;
use conveyor_store::capability::CapabilityStoreError;
use conveyor_store::secret::{SecretError, open_named_tx};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::integrations::strata::{RAG_TOKEN_SECRET, Strata, StrataError};
use crate::invoke::ActionError;
use crate::registry::registry;
use crate::spec::FieldError;

/// Compute every blocking reason for one capability. Fail-soft: a readiness answer, never a
/// refusal — the caller decides whether to report it (`capabilities.get`) or refuse with it
/// (`process_runs.start`). An infrastructure failure (a database read, the tenant secret read)
/// is still an internal error: it is not a fact about the capability.
///
/// `bound_agents` are the executors the process's steps name (`process_step.process_agent_id`,
/// D29). When it is non-empty, EACH of those agents is checked directly and the capability's
/// "exactly one active executor" rule does not apply — the rule is about unbound capability runs,
/// not a process whose steps already say who runs them. Empty keeps the capability rule.
pub(crate) async fn capability_readiness_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    capability_id: Uuid,
    bound_agents: &[Uuid],
) -> Result<Readiness, ActionError> {
    let mut blocking: Vec<ReadinessBlocker> = Vec::new();

    match store::capability::get_capability_tx(&mut *conn, tenant, capability_id)
        .await
        .map_err(capability_error)?
    {
        None => {
            // Every other check would be noise against a row that does not exist.
            return Ok(Readiness {
                blocking: vec![ReadinessBlocker::CapabilityMissing],
            });
        }
        Some(record) if record.status != "active" => {
            blocking.push(ReadinessBlocker::CapabilityNotActive {
                status: record.status.clone(),
            })
        }
        Some(_) => {}
    }

    // One executor, two ways to name it (D29). A process step binds `process_agent_id`, so every
    // bound agent is checked directly; a capability run with no bound process agent keeps the v1
    // rule — exactly one active executor, ambiguity reported, never guessed. A process that binds
    // a DIFFERENT agent per step passes several ids and is still coherent: each is checked.
    if bound_agents.is_empty() {
        let agents =
            store::capability::list_agents_tx(&mut *conn, tenant, Some(capability_id), 100)
                .await
                .map_err(capability_error)?;
        let active: Vec<_> = agents
            .into_iter()
            .filter(|agent| agent.status == "active")
            .collect();
        match active.as_slice() {
            [] => blocking.push(ReadinessBlocker::AgentMissing),
            [agent] => check_executor(&mut *conn, tenant, agent, &mut blocking).await?,
            _ => blocking.push(ReadinessBlocker::AgentAmbiguous),
        }
    } else {
        for agent_id in bound_agents {
            let record = store::capability::get_agent_tx(&mut *conn, tenant, *agent_id)
                .await
                .map_err(capability_error)?;
            match record {
                None => blocking.push(ReadinessBlocker::AgentMissing),
                Some(agent) if agent.status != "active" => {
                    blocking.push(ReadinessBlocker::AgentNotActive {
                        status: agent.status.clone(),
                    })
                }
                Some(agent) => check_executor(&mut *conn, tenant, &agent, &mut blocking).await?,
            }
        }
    }

    let bindings = store::capability::packs_for_capability_tx(&mut *conn, tenant, capability_id)
        .await
        .map_err(capability_error)?;
    let required: Vec<_> = bindings
        .into_iter()
        .filter(|binding| binding.required)
        .collect();
    if !required.is_empty() {
        match open_named_tx(&mut *conn, tenant, RAG_TOKEN_SECRET).await {
            Ok(bytes) => {
                let token = String::from_utf8(bytes).unwrap_or_default();
                match Strata::from_env() {
                    Some(client) => {
                        for binding in &required {
                            let pack = format!("{}@{}", binding.pack_key, binding.pack_version);
                            let result = client
                                .pack_leaves(
                                    &token,
                                    tenant,
                                    &binding.pack_key,
                                    binding.pack_version,
                                )
                                .await;
                            if let Err(error) = &result {
                                tracing::warn!(
                                    %error,
                                    pack,
                                    "capability readiness: the required pack did not resolve"
                                );
                            }
                            match pack_state(&result) {
                                None => {}
                                Some(PackFault::NotPublished) => {
                                    blocking.push(ReadinessBlocker::PackNotPublished {
                                        pack_key: binding.pack_key.clone(),
                                        pack_version: binding.pack_version,
                                    })
                                }
                                Some(PackFault::Unavailable) => {
                                    blocking.push(ReadinessBlocker::PackUnavailable {
                                        pack_key: binding.pack_key.clone(),
                                        pack_version: binding.pack_version,
                                    })
                                }
                            }
                        }
                    }
                    None => blocking.push(ReadinessBlocker::KnowledgeNotConfigured),
                }
            }
            Err(SecretError::NotFound) => blocking.push(ReadinessBlocker::KnowledgeNotConfigured),
            Err(error) => {
                tracing::error!(%error, "capability readiness: read the strata token");
                return Err(ActionError::internal());
            }
        }
    }

    Ok(Readiness { blocking })
}

/// The per-executor half of readiness: the prompt pin and the action list. Shared by the bound
/// path and the unique-active-agent path so the two can never disagree about a given agent.
async fn check_executor(
    conn: &mut PgConnection,
    tenant: TenantId,
    agent: &store::capability::AgentRecord,
    blocking: &mut Vec<ReadinessBlocker>,
) -> Result<(), ActionError> {
    if let Some(reference) = agent.prompt_ref.as_deref()
        && !prompt_is_active(&mut *conn, tenant, reference).await?
    {
        blocking.push(ReadinessBlocker::PromptMissing {
            prompt_ref: reference.to_owned(),
        });
    }
    for action in &agent.allowed_actions {
        if registry().get(action).is_none() {
            blocking.push(ReadinessBlocker::ActionNotRegistered {
                action: action.clone(),
            });
        }
    }
    Ok(())
}

/// The wire projection of a readiness answer: the SAME array `capabilities.get` returns as
/// `blocking[]` and `process_runs.start` returns as `errors[]`.
pub(crate) fn readiness_fields(readiness: &Readiness) -> Vec<FieldError> {
    readiness.blocking.iter().map(blocker_field).collect()
}

fn blocker_field(blocker: &ReadinessBlocker) -> FieldError {
    let (field, detail) = match blocker {
        ReadinessBlocker::CapabilityMissing => (
            "capability_id",
            "the resolved capability no longer exists".to_owned(),
        ),
        ReadinessBlocker::CapabilityNotActive { status } => (
            "capability_id",
            format!("the capability is {status}, not active"),
        ),
        ReadinessBlocker::AgentMissing => (
            "capability_agent_id",
            "the capability has no active executor".to_owned(),
        ),
        ReadinessBlocker::AgentNotActive { status } => (
            "capability_agent_id",
            format!("the bound capability agent is {status}, not active"),
        ),
        ReadinessBlocker::AgentAmbiguous => (
            "capability_agent_id",
            "more than one active executor is configured; exactly one is required".to_owned(),
        ),
        ReadinessBlocker::ActionNotRegistered { action } => (
            "allowed_actions",
            format!("{action} is not a registered action"),
        ),
        ReadinessBlocker::PackNotPublished {
            pack_key,
            pack_version,
        } => (
            "capability_knowledge",
            format!("{pack_key}@{pack_version} has no published leaves"),
        ),
        ReadinessBlocker::PackUnavailable {
            pack_key,
            pack_version,
        } => (
            "capability_knowledge",
            format!("{pack_key}@{pack_version} could not be reached"),
        ),
        ReadinessBlocker::KnowledgeNotConfigured => (
            "capability_knowledge",
            "this tenant has no usable strata configuration".to_owned(),
        ),
        ReadinessBlocker::PromptMissing { prompt_ref } => (
            "prompt_ref",
            format!("{prompt_ref} is not an active prompt in this tenant's catalogue"),
        ),
    };
    FieldError {
        field: field.to_owned(),
        code: blocker.code().to_owned(),
        detail,
    }
}

/// Whether the tenant's catalogue has this exact pin at `active` status. A retired row is
/// deliberately not runnable (the catalogue returns it so the refusal can name the reason).
/// A non-parsing ref is a blocker, not an error: `capability_agents.create` already refused the
/// grammar, so this is the defensive arm, never the normal path.
async fn prompt_is_active(
    conn: &mut PgConnection,
    tenant: TenantId,
    reference: &str,
) -> Result<bool, ActionError> {
    let parsed = match PromptRef::parse(reference) {
        Ok(parsed) => parsed,
        Err(_) => return Ok(false),
    };
    let Ok(version) = i32::try_from(parsed.version) else {
        return Ok(false);
    };
    let record = store::prompt::get_prompt_tx(&mut *conn, tenant, &parsed.name, version)
        .await
        .map_err(|error| {
            tracing::error!(%error, prompt_ref = reference, "capability readiness: prompt read");
            ActionError::internal()
        })?;
    Ok(record.is_some_and(|record| record.status == "active"))
}

/// How a required pack's leaf read is classified.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PackFault {
    /// The pinned version is absent (`404`), refused, or published with no leaves.
    NotPublished,
    /// The service could not answer (transport, auth, 5xx, or a body this client cannot read).
    Unavailable,
}

/// `None` when the pack resolves to at least one leaf; otherwise the reason it does not.
///
/// A published-but-empty pack is `Ok(vec![])` — strata's own contract says the caller decides
/// whether that is a degradation — so emptiness is a fact here, not an upstream error: CONTRACT
/// §17's "no resolvable leaves" is not ready.
fn pack_state(result: &Result<Vec<Uuid>, StrataError>) -> Option<PackFault> {
    match result {
        Ok(leaves) if leaves.is_empty() => Some(PackFault::NotPublished),
        Ok(_) => None,
        Err(StrataError::NotFound | StrataError::Refused { .. }) => Some(PackFault::NotPublished),
        Err(_) => Some(PackFault::Unavailable),
    }
}

fn capability_error(error: CapabilityStoreError) -> ActionError {
    match error {
        CapabilityStoreError::NotFound => ActionError::resource_not_found("such capability"),
        CapabilityStoreError::Database(error) => {
            tracing::error!(%error, "capability readiness: store read failed");
            ActionError::internal()
        }
        other => ActionError::new(500, "internal", format!("capability readiness: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_required_pack_is_ready_only_with_at_least_one_leaf() {
        assert_eq!(pack_state(&Ok(vec![Uuid::nil()])), None);
        assert_eq!(pack_state(&Ok(vec![])), Some(PackFault::NotPublished));
        assert_eq!(
            pack_state(&Err(StrataError::NotFound)),
            Some(PackFault::NotPublished)
        );
        assert_eq!(
            pack_state(&Err(StrataError::Refused {
                status: 422,
                body: String::new(),
            })),
            Some(PackFault::NotPublished)
        );
        for unavailable in [
            StrataError::Unavailable,
            StrataError::Unauthorized,
            StrataError::Server {
                status: 503,
                body: String::new(),
            },
            StrataError::Protocol,
            StrataError::Transport("timed out".to_owned()),
        ] {
            assert_eq!(
                pack_state(&Err(unavailable)),
                Some(PackFault::Unavailable),
                "a service failure is not a publication fact"
            );
        }
    }
}
