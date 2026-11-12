#![forbid(unsafe_code)]
//! The turn runner (S10 §5.4, §6 step 9).
//!
//! One user turn → the model (`completions.create`, through the registry engine) → validate → either
//! invoke the action or raise a proposal → repeat, at most [`MAX_ROUNDS`] times. Only the terminal
//! turn is durable: the assistant row is inserted `streaming`, and the terminal `meta`, the proposal
//! (when there is one) and the `conversation_turn.created` event all commit in **one** transaction
//! (S10-D4). A crash therefore leaves `streaming`, which `copilot.turn.reap` marks `interrupted` —
//! never a turn claiming an effect that did not commit.
//!
//! The model is never handed a tool outside the caller's surface, and a tool it asks for anyway ends
//! the turn `refused` — the refusal names the tool, because the model is the caller's own agent, not
//! an untrusted third party.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use conveyor_actions::agent_surface;
use conveyor_actions::integrations::inference::{ChatMessage, CompletionIn};
use conveyor_actions::spec::ActionSpec;
use conveyor_actions::{ActionError, Authority, InvokeRequest, invoke, registry};
use conveyor_domain::copilot::{
    MAX_ROUNDS, ModelRun, ToolCallRecord, TurnError, TurnMeta, TurnStatus,
};
use conveyor_store::TenantId;
use conveyor_store::copilot::{self, NewTurn};
use conveyor_store::outbox::{self, NewEvent};

use crate::error::ApiError;
use crate::extract::{Caller, RequireLicence};
use crate::state::AppState;

use super::conversations::turn_out;
use super::{begin, proposal};

/// The history window handed to the model (S10 open question 3).
pub const MAX_HISTORY: i64 = 20;

/// The most characters of one tool result the model sees.
pub const TOOL_RESULT_CHARS: usize = 4_000;

/// The catalogue pin the copilot compiles its system message from (SPEC D27).
pub const SYSTEM_PROMPT_REF: &str = "copilot_system@1";

/// The seed text behind `copilot_system@1` (the prompt audit reads the same copy).
pub use conveyor_actions::prompt_audit::COPILOT_SYSTEM_PROMPT as SYSTEM_PROMPT;

/// The turn request.
#[derive(Clone, Debug, Deserialize, utoipa::ToSchema)]
pub struct TurnIn {
    /// The user's message.
    pub body: String,
}

/// `POST /api/copilot/conversations/{id}/turns` — append a user turn and run the copilot's answer.
#[utoipa::path(
    post,
    path = "/api/copilot/conversations/{id}/turns",
    tag = "copilot",
    params(("id" = Uuid, Path, description = "The conversation id")),
    request_body = TurnIn,
    responses(
        (status = 201, description = "The assistant turn, terminal"),
        (status = 400, description = "An empty body", body = crate::error::ProblemDetail),
        (status = 404, description = "No such conversation in this tenant", body = crate::error::ProblemDetail),
        (status = 409, description = "The conversation is archived", body = crate::error::ProblemDetail)
    )
)]
pub async fn create(
    State(state): State<AppState>,
    caller: Caller,
    _licence: RequireLicence,
    Path(conversation_id): Path<Uuid>,
    Json(input): Json<TurnIn>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if input.body.trim().is_empty() {
        return Err(ApiError::bad_request("body: must not be empty"));
    }
    let tenant = caller.tenant;

    // One transaction: the user turn and the still-streaming assistant turn. A conversation is
    // per user; another user's id is answered exactly like an absent one.
    let (turn_id, project_id) = {
        let mut tx = begin(&state, tenant).await?;
        let conversation = copilot::conversation(&mut tx, tenant, conversation_id)
            .await?
            .filter(|conversation| {
                conversation.user_id.is_none()
                    || conversation.user_id == Some(caller.principal.user_id.as_uuid())
            })
            .ok_or_else(|| ApiError::not_found("no such conversation in this tenant"))?;
        copilot::append_turn_tx(
            &mut tx,
            tenant,
            &NewTurn {
                conversation_id,
                role: "user".to_owned(),
                body: input.body.clone(),
                meta: json!({}),
                trace_id: None,
            },
        )
        .await?;
        let assistant = copilot::append_turn_tx(
            &mut tx,
            tenant,
            &NewTurn {
                conversation_id,
                role: "assistant".to_owned(),
                body: String::new(),
                meta: TurnMeta::streaming().to_value(),
                trace_id: None,
            },
        )
        .await?;
        tx.commit().await?;
        (assistant.id, conversation.project_id)
    };

    let end = run(&state, &caller, conversation_id, turn_id, project_id).await;
    // A failed turn is durable AND reported: the row records the failure, and the response is the
    // RFC 9457 problem the same failure would produce anywhere else (S10 §7 f).
    let failure = match &end {
        End::Failed {
            code,
            status,
            detail,
            ..
        } => Some((*code, *status, detail.clone())),
        _ => None,
    };

    // One transaction: the terminal meta, the proposal (if any) and the event.
    let mut tx = begin(&state, tenant).await?;
    let (mut meta, body, draft) = end.into_terminal();
    let mut proposal_id = None;
    if let Some(draft) = draft {
        let approval = proposal::create_tx(
            &mut tx,
            tenant,
            &proposal::ProposalRequest {
                conversation_id,
                turn_id,
                project_id,
                spec: draft.spec,
                input: draft.input,
                summary: draft.summary,
                proposer_id: caller.principal.user_id.as_uuid(),
                proposer_kind: caller.principal.kind.as_actor_kind(),
                proposer_role: caller.role,
                // Approvals never expire (owner ruling 2026-11-09): a tenant that refuses to answer
                // stalls its own project forever, and time must not silently kill a gate. Expiry
                // survives only as an explicit opt-in by a caller or a process step.
                expires_at: None,
            },
        )
        .await
        .map_err(|error| {
            tracing::error!(%error, "creating a copilot proposal");
            ApiError::internal("the proposal could not be recorded")
        })?;
        meta.approval_id = Some(approval.id);
        proposal_id = Some(approval.id);
    }
    let meta_value = meta.to_value();
    let Some(turn) = copilot::finish_turn_tx(&mut tx, tenant, turn_id, &body, &meta_value).await?
    else {
        // Another writer already finished this turn; never write a second outcome.
        tx.rollback().await?;
        return Err(ApiError::conflict("the turn was already finished"));
    };
    outbox::append(
        &mut tx,
        tenant,
        &NewEvent {
            topic: format!("conversation:{conversation_id}"),
            event_type: "conversation_turn.created".to_owned(),
            aggregate_type: "conversation_turn".to_owned(),
            aggregate_id: turn.id,
            payload: json!({
                "conversation_id": conversation_id,
                "turn": turn_out(&turn),
                "proposal_id": proposal_id,
            }),
            correlation_id: None,
            causation_id: None,
        },
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "appending the conversation_turn.created event");
        ApiError::internal("the turn could not be published")
    })?;
    tx.commit().await?;

    if let Some((code, status, detail)) = failure {
        let status = StatusCode::from_u16(status).unwrap_or(StatusCode::SERVICE_UNAVAILABLE);
        return Err(ApiError::new(
            status,
            code,
            format!("{detail} (turn {turn_id} recorded as failed)"),
        ));
    }
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "conversation": conversation_id,
            "turn": turn_out(&turn),
            "proposal_id": proposal_id,
        })),
    ))
}

/// One round's answer from the router.
struct Answer {
    content: String,
    tool_calls: Vec<CalledTool>,
    raw_tool_calls: Value,
    model: ModelRun,
}

/// One tool the model asked for.
struct CalledTool {
    id: Option<String>,
    name: String,
    arguments: String,
}

/// A proposal the turn ended in.
struct Draft {
    spec: &'static ActionSpec,
    input: Value,
    summary: String,
}

/// How a turn ended.
enum End {
    Completed {
        body: String,
        model: ModelRun,
        tool_calls: Vec<ToolCallRecord>,
    },
    Proposed {
        body: String,
        model: ModelRun,
        tool_calls: Vec<ToolCallRecord>,
        draft: Draft,
    },
    Refused {
        detail: String,
        model: Option<ModelRun>,
        tool_calls: Vec<ToolCallRecord>,
    },
    Failed {
        code: &'static str,
        status: u16,
        detail: String,
        model: Option<ModelRun>,
        tool_calls: Vec<ToolCallRecord>,
    },
}

impl End {
    fn into_terminal(self) -> (TurnMeta, String, Option<Draft>) {
        match self {
            Self::Completed {
                body,
                model,
                tool_calls,
            } => (
                TurnMeta {
                    status: TurnStatus::Completed,
                    model: Some(model),
                    tool_calls,
                    error: None,
                    approval_id: None,
                },
                body,
                None,
            ),
            Self::Proposed {
                body,
                model,
                tool_calls,
                draft,
            } => (
                TurnMeta {
                    status: TurnStatus::Proposed,
                    model: Some(model),
                    tool_calls,
                    error: None,
                    approval_id: None,
                },
                body,
                Some(draft),
            ),
            Self::Refused {
                detail,
                model,
                tool_calls,
            } => (
                TurnMeta {
                    status: TurnStatus::Refused,
                    model,
                    tool_calls,
                    error: Some(TurnError {
                        kind: "/problems/copilot-refused".to_owned(),
                        status: 422,
                        detail: detail.clone(),
                    }),
                    approval_id: None,
                },
                detail,
                None,
            ),
            Self::Failed {
                code,
                status,
                detail,
                model,
                tool_calls,
            } => (
                TurnMeta {
                    status: TurnStatus::Failed,
                    model,
                    tool_calls,
                    error: Some(TurnError {
                        kind: format!("/problems/{code}"),
                        status,
                        detail: detail.clone(),
                    }),
                    approval_id: None,
                },
                detail,
                None,
            ),
        }
    }
}

/// Run the model loop.
async fn run(
    state: &AppState,
    caller: &Caller,
    conversation_id: Uuid,
    turn_id: Uuid,
    _project_id: Option<Uuid>,
) -> End {
    let tenant = caller.tenant;
    let surface = agent_surface::surface(
        registry(),
        caller.principal.kind,
        caller.role,
        caller.channel.as_ref(),
    );
    let history = {
        let mut tx = match begin(state, tenant).await {
            Ok(tx) => tx,
            Err(_) => {
                return End::Failed {
                    code: "internal",
                    status: 500,
                    detail: "the conversation could not be read".to_owned(),
                    model: None,
                    tool_calls: Vec::new(),
                };
            }
        };
        match copilot::recent_turns(&mut tx, tenant, conversation_id, MAX_HISTORY).await {
            Ok(rows) => {
                let _ = tx.commit().await;
                rows
            }
            Err(_) => {
                return End::Failed {
                    code: "internal",
                    status: 500,
                    detail: "the conversation could not be read".to_owned(),
                    model: None,
                    tool_calls: Vec::new(),
                };
            }
        }
    };

    // SPEC D27: the copilot's system message is compiled from the tenant's catalogue, not read
    // from a constant. A tenant without the row (or with it retired) is a configuration error
    // that fails the turn before a model call, never an empty system message.
    let system_prompt = match conveyor_actions::prompt::compile_for_copilot(
        &state.pool,
        tenant,
        conversation_id,
        SYSTEM_PROMPT_REF,
    )
    .await
    {
        Ok(compiled) => compiled.text,
        Err(error) => {
            tracing::error!(%error, "copilot: the prompt catalogue could not compile the system message");
            return End::Failed {
                code: "internal",
                status: 500,
                detail: "the copilot's prompt is not configured".to_owned(),
                model: None,
                tool_calls: Vec::new(),
            };
        }
    };

    let mut messages: Vec<ChatMessage> = Vec::with_capacity(history.len() + 1);
    messages.push(ChatMessage {
        role: "system".to_owned(),
        content: system_prompt,
        tool_calls: None,
        tool_call_id: None,
        name: None,
    });
    for turn in &history {
        if turn.role == "assistant" && turn.body.is_empty() {
            continue;
        }
        messages.push(ChatMessage {
            role: turn.role.clone(),
            content: turn.body.clone(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
    }
    let tools: Vec<Value> = surface.iter().map(|spec| tool_declaration(spec)).collect();

    let mut tool_calls: Vec<ToolCallRecord> = Vec::new();
    let mut last_model: Option<ModelRun> = None;

    for round in 0..MAX_ROUNDS {
        let answer = match call_model(state, caller, turn_id, round, &messages, &tools).await {
            Ok(answer) => answer,
            Err(error) => {
                return End::Failed {
                    code: error.code,
                    status: error.status,
                    detail: error.detail.clone(),
                    model: last_model,
                    tool_calls,
                };
            }
        };
        last_model = Some(answer.model.clone());

        if answer.tool_calls.is_empty() {
            return End::Completed {
                body: answer.content,
                model: answer.model,
                tool_calls,
            };
        }

        messages.push(ChatMessage {
            role: "assistant".to_owned(),
            content: answer.content.clone(),
            tool_calls: Some(answer.raw_tool_calls.clone()),
            tool_call_id: None,
            name: None,
        });

        for (index, call) in answer.tool_calls.iter().enumerate() {
            let Some(spec) = surface
                .iter()
                .find(|spec| spec.mcp_name() == call.name)
                .copied()
            else {
                return End::Refused {
                    detail: format!(
                        "the model asked for a tool this caller does not have: {}",
                        call.name
                    ),
                    model: last_model,
                    tool_calls,
                };
            };
            let Ok(input) = serde_json::from_str::<Value>(&call.arguments) else {
                return End::Refused {
                    detail: format!("the model's arguments for {} are not JSON", spec.name),
                    model: last_model,
                    tool_calls,
                };
            };
            // Schema-validate before anything is invoked or proposed. `_version` is lifted to the
            // invocation's precondition (CONTRACT §3), exactly as `invoke` does it, so a proposal
            // pins the version the model observed.
            let mut validated = input.clone();
            if let Some(object) = validated.as_object_mut() {
                object.remove("_version");
            }
            let errors = conveyor_actions::validate_input(&spec.input_schema(), &validated);
            if !errors.is_empty() {
                return End::Refused {
                    detail: format!(
                        "the model's input for {} is invalid: {}",
                        spec.name,
                        serde_json::to_string(&errors).unwrap_or_default()
                    ),
                    model: last_model,
                    tool_calls,
                };
            }
            if proposal::required(spec) {
                let summary = if answer.content.trim().is_empty() {
                    format!("Run {}", spec.name)
                } else {
                    answer.content.trim().to_owned()
                };
                return End::Proposed {
                    body: summary.clone(),
                    model: answer.model,
                    tool_calls,
                    draft: Draft {
                        spec,
                        input,
                        summary,
                    },
                };
            }
            let key = call.id.clone().unwrap_or_else(|| format!("call-{index}"));
            match invoke_action(
                state,
                caller,
                spec,
                &input,
                &format!("turn:{turn_id}:{round}:{key}"),
            )
            .await
            {
                Ok((record, result)) => {
                    tool_calls.push(record);
                    messages.push(ChatMessage {
                        role: "tool".to_owned(),
                        content: truncate(&result.to_string(), TOOL_RESULT_CHARS),
                        tool_calls: None,
                        tool_call_id: call.id.clone(),
                        name: Some(spec.mcp_name()),
                    });
                }
                Err(error) => {
                    // A refused tool is an agent-loop event, not a turn failure: the model gets the
                    // reason and may recover. The call is still recorded with its outcome.
                    tool_calls.push(ToolCallRecord {
                        action: spec.name.to_owned(),
                        invocation_id: None,
                        outcome: format!("error:{}", error.code),
                    });
                    messages.push(ChatMessage {
                        role: "tool".to_owned(),
                        content: truncate(&error.detail, TOOL_RESULT_CHARS),
                        tool_calls: None,
                        tool_call_id: call.id.clone(),
                        name: Some(spec.mcp_name()),
                    });
                }
            }
        }
    }

    End::Refused {
        detail: format!("the turn exceeded its {MAX_ROUNDS} tool rounds"),
        model: last_model,
        tool_calls,
    }
}

/// Ask the model through the registry engine.
async fn call_model(
    state: &AppState,
    caller: &Caller,
    turn_id: Uuid,
    round: u8,
    messages: &[ChatMessage],
    tools: &[Value],
) -> Result<Answer, ActionError> {
    let input = CompletionIn {
        model: None,
        messages: messages.to_vec(),
        tools: tools.to_vec(),
        tool_choice: None,
        temperature: None,
        max_tokens: Some(1024),
        response_format: None,
    };
    let outcome = invoke(
        &state.pool,
        registry(),
        caller.tenant,
        InvokeRequest {
            action: "completions.create".to_owned(),
            input: serde_json::to_value(&input).map_err(ActionError::serialize)?,
            precondition: None,
            idempotency_key: Some(format!("turn:{turn_id}:{round}")),
            actor_id: caller.principal.user_id.as_uuid(),
            actor_kind: caller.principal.kind.as_actor_kind().to_owned(),
            role: caller.role,
            policy: caller.policy.clone(),
            authority: Authority::of(caller),
            request_id: None,
            trace_id: None,
            depth: 0,
            app_key: state.config.app_key.clone(),
            usage: Some(conveyor_actions::UsageContext {
                caller_kind: conveyor_domain::inference::CallerKind::Copilot,
                run_id: None,
                run_step_id: None,
                conversation_turn_id: Some(turn_id),
                eval_run_id: None,
                prompt_ref: Some("copilot".to_owned()),
                prompt_version: Some("1".to_owned()),
                correlation_id: Some(format!("turn:{turn_id}")),
            }),
        },
    )
    .await?;
    parse_answer(&outcome.body)
}

/// Read the router's OpenAI-compatible answer.
fn parse_answer(body: &Value) -> Result<Answer, ActionError> {
    let output = &body["output"];
    let message = &output["choices"][0]["message"];
    if message.is_null() {
        return Err(ActionError::new(
            502,
            "inference-shape",
            "the router returned no choices[0].message",
        ));
    }
    let content = message["content"].as_str().unwrap_or_default().to_owned();
    let raw_tool_calls = message["tool_calls"].clone();
    let mut tool_calls = Vec::new();
    if let Some(list) = raw_tool_calls.as_array() {
        for call in list {
            tool_calls.push(CalledTool {
                id: call["id"].as_str().map(str::to_owned),
                name: call["function"]["name"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                arguments: call["function"]["arguments"]
                    .as_str()
                    .unwrap_or("{}")
                    .to_owned(),
            });
        }
    }
    Ok(Answer {
        content,
        tool_calls,
        raw_tool_calls,
        model: ModelRun {
            action: "completions.create".to_owned(),
            model: output["model"].as_str().unwrap_or("unknown").to_owned(),
            request_id: body["request_id"].as_str().map(str::to_owned),
        },
    })
}

/// Invoke one action as the caller, returning its record and its output.
async fn invoke_action(
    state: &AppState,
    caller: &Caller,
    spec: &ActionSpec,
    input: &Value,
    idempotency_key: &str,
) -> Result<(ToolCallRecord, Value), ActionError> {
    let outcome = invoke(
        &state.pool,
        registry(),
        caller.tenant,
        InvokeRequest {
            action: spec.name.to_owned(),
            input: input.clone(),
            precondition: None,
            idempotency_key: Some(idempotency_key.to_owned()),
            actor_id: caller.principal.user_id.as_uuid(),
            actor_kind: caller.principal.kind.as_actor_kind().to_owned(),
            role: caller.role,
            policy: caller.policy.clone(),
            authority: Authority::of(caller),
            request_id: None,
            trace_id: None,
            depth: 0,
            app_key: state.config.app_key.clone(),
            usage: None,
        },
    )
    .await?;
    let record = ToolCallRecord {
        action: spec.name.to_owned(),
        invocation_id: outcome.invocation_id,
        outcome: "ok".to_owned(),
    };
    Ok((record, outcome.body["output"].clone()))
}

/// The OpenAI tool declaration for one action.
fn tool_declaration(spec: &ActionSpec) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": spec.mcp_name(),
            "description": format!("{} ({}; effect {})", spec.name, spec.kind.as_str(), spec.effect.as_str()),
            "parameters": spec.input_schema(),
        }
    })
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    text.chars().take(max).collect::<String>() + "…"
}

/// `copilot.turn.reap` — mark `streaming` turns older than `older_than_secs` as `interrupted` and
/// emit each turn's `conversation_turn.created` event, all in one transaction (S10 §5.4).
///
/// A reaped turn is a durable outcome, so the event is written with it; a client that reconnects
/// sees the interrupted turn exactly once.
pub async fn reap(
    pool: &sqlx::PgPool,
    tenant: TenantId,
    older_than_secs: i64,
) -> Result<u64, String> {
    let mut tx = conveyor_store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| format!("begin: {error}"))?;
    let rows = copilot::reap_streaming_tx(&mut tx, tenant, older_than_secs)
        .await
        .map_err(|error| format!("reap: {error}"))?;
    for turn in &rows {
        outbox::append(
            &mut tx,
            tenant,
            &NewEvent {
                topic: format!("conversation:{}", turn.conversation_id),
                event_type: "conversation_turn.created".to_owned(),
                aggregate_type: "conversation_turn".to_owned(),
                aggregate_id: turn.id,
                payload: json!({
                    "conversation_id": turn.conversation_id,
                    "turn": turn_out(turn),
                    "proposal_id": null,
                }),
                correlation_id: None,
                causation_id: None,
            },
        )
        .await
        .map_err(|error| format!("event: {error}"))?;
    }
    tx.commit()
        .await
        .map_err(|error| format!("commit: {error}"))?;
    Ok(rows.len() as u64)
}
