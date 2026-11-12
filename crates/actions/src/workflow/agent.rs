#![forbid(unsafe_code)]
//! `agent.run` — the model-driven executor (S13 step 8, CONTRACT §17 / SPEC D22).
//!
//! The loop itself is pure and lives in [`conveyor_workflow::agent`]. This module is the seam the
//! step executor uses: it resolves the run's `capability_agent` into the step input
//! ([`resolve`]) and drives the loop by invoking `inference.chat` and the agent's tools
//! ([`drive`]) — one `action_invocation` per tool call, all on the run's trace id.
//!
//! ⛔ **The action's own handler refuses.** An internal action runs inside the invocation's
//! transaction and [`HandlerCtx`] carries no pool, so a handler cannot call [`invoke`] for the
//! model or a tool: doing so would need a second connection while the first holds the
//! transaction. The executor (`crate::workflow::node`) is the only layer that already invokes
//! actions outside a transaction, so it special-cases `agent.run` and calls [`drive`]. A direct
//! `POST /api/actions/agent.run/invocations` therefore fails typed instead of pretending to run a
//! loop it cannot drive.
//!
//! The tool allowlist is the resolved `tools` array, not the principal's scopes alone: a call the
//! model names but the executor did not declare is refused **before** execution
//! (`action-not-found`) and fed back to the model, never audited after the fact (CONTRACT §17).

use std::time::Instant;

use conveyor_auth::Role;
use conveyor_domain::TenantId;
use conveyor_domain::inference::CallerKind;
use conveyor_store as store;
use conveyor_workflow::agent::{
    Agent, AgentConfig, AgentError, MAX_TURNS, Message, ModelReply, Plan, Reply, ToolCall,
    ToolReply,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::invoke::{ActionError, Authority, HandlerCtx, InvokeRequest, UsageContext, invoke};
use crate::registry::registry;
use crate::spec::{FieldError, HandlerOutput};

/// The external action the loop calls for the model.
pub const MODEL_ACTION: &str = "inference.chat";

/// One message in the step input, in the shape the renderer and the loop share.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AgentMessage {
    /// `system`, `user`, `assistant` or `tool`.
    pub role: String,
    /// The text, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// The calls an assistant message asked for.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<AgentToolCall>,
    /// The call a `tool` message answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// The tool's name, on a `tool` message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// One tool call in the step input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AgentToolCall {
    /// The model's call id.
    pub id: String,
    /// The registry action.
    pub name: String,
    /// The arguments, already parsed.
    #[serde(default)]
    pub arguments: Value,
}

/// The `agent.run` step input: exactly what CONTRACT §17 says the executor resolves.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AgentRunIn {
    /// The model ref.
    pub model: String,
    /// The conversation, including the system message the executor rendered from `prompt_ref`.
    pub messages: Vec<AgentMessage>,
    /// The tool schemas the model may name.
    #[serde(default)]
    pub tools: Vec<Value>,
    /// Turns allowed; absent takes the contract default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u8>,
    /// Tool calls allowed; absent takes the contract default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tool_calls: Option<u8>,
    /// Wall clock; absent takes the contract default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<i64>,
    /// The `capability_agent` this was resolved from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_agent_id: Option<Uuid>,
    /// `<name>@<version>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_ref: Option<String>,
    /// The run the step executes inside.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<Uuid>,
    /// The `run_step` the step executes for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_step_id: Option<Uuid>,
    /// Agent→agent depth; absent is 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u8>,
}

impl AgentRunIn {
    /// The loop's config.
    pub fn into_config(self) -> AgentConfig {
        AgentConfig {
            model: self.model,
            messages: self
                .messages
                .into_iter()
                .map(|message| Message {
                    role: message.role,
                    content: message.content,
                    tool_calls: message
                        .tool_calls
                        .into_iter()
                        .map(|call| ToolCall {
                            id: call.id,
                            name: call.name,
                            arguments: call.arguments,
                        })
                        .collect(),
                    tool_call_id: message.tool_call_id,
                    name: message.name,
                })
                .collect(),
            tools: self.tools,
            max_turns: self.max_turns,
            max_tool_calls: self.max_tool_calls,
            timeout_ms: self.timeout_ms,
            capability_agent_id: self.capability_agent_id,
            prompt_ref: self.prompt_ref,
            run_id: self.run_id,
            run_step_id: self.run_step_id,
            depth: self.depth,
        }
    }
}

/// One tool call in the output.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AgentRunTool {
    /// The model's call id.
    pub id: String,
    /// The action invoked.
    pub name: String,
    /// Whether it failed or was refused.
    pub is_error: bool,
    /// The `action_invocation` the engine wrote, when it wrote one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invocation_id: Option<Uuid>,
}

/// What a finished loop produced.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AgentRunOut {
    /// The model's final text.
    pub output: String,
    /// Turns used.
    pub turns: u8,
    /// Every tool call, in order.
    pub tool_calls: Vec<AgentRunTool>,
    /// The last reply's `usage`, when the router sent one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Value>,
    /// The `capability_agent` the executor resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_agent_id: Option<Uuid>,
    /// `<name>@<version>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_ref: Option<String>,
}

impl From<conveyor_workflow::agent::AgentOutput> for AgentRunOut {
    fn from(output: conveyor_workflow::agent::AgentOutput) -> Self {
        Self {
            output: output.output,
            turns: output.turns,
            tool_calls: output
                .tool_calls
                .into_iter()
                .map(|call| AgentRunTool {
                    id: call.id,
                    name: call.name,
                    is_error: call.is_error,
                    invocation_id: call.invocation_id,
                })
                .collect(),
            usage: output.usage,
            capability_agent_id: output.capability_agent_id,
            prompt_ref: output.prompt_ref,
        }
    }
}

/// Where one loop runs: the run, its step, and the executor's authority.
pub(crate) struct RunTrace<'a> {
    /// The run.
    pub run_id: Uuid,
    /// The `run_step` the node attempt records.
    pub run_step_id: Option<Uuid>,
    /// The nesting depth the caller is at; each model/tool call is one deeper.
    pub depth: u32,
    /// The node attempt's idempotency key: model and tool calls are keyed under it, so a node
    /// retry replays stored responses instead of buying a second round of effects.
    pub idempotency_prefix: Option<&'a str>,
    /// `APP_KEY` — entitlement reads the invocation may perform.
    pub app_key: &'a str,
}

/// The declared action. Its handler refuses (see the module docs); the loop runs in the executor.
#[crate::action(
    name = "agent.run",
    version = 1,
    kind = "internal",
    scopes = ["workflows:write"],
    idempotent = false,
    timeout_ms = 300000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn agent_run(
    _tx: &mut PgConnection,
    _ctx: HandlerCtx,
    _input: AgentRunIn,
) -> Result<HandlerOutput<AgentRunOut>, ActionError> {
    Err(ActionError::new(
        409,
        "conflict",
        "agent.run executes inside a run's step, driven by its executor; start a workflow run \
         that names it rather than invoking the action directly (CONTRACT §17)",
    ))
}

/// Resolve the step input for an `agent.run` node (CONTRACT §17: the step executor, not the loop).
///
/// **A bound run is resolved from its row first, and the row is authoritative.** When the run
/// carries a `capability_agent_id`, the row supplies `model_ref`, `allowed_actions` → tool
/// schemas, the caps and the prompt recipe. The step input may supply the messages (the compiled
/// prompt is inserted here) and may **lower** a cap, never raise it; it may not substitute the
/// model, the tool surface, the prompt recipe or the executor. A conflicting field is refused by
/// name — D22's one persisted source is not advisory.
///
/// **An unbound run uses the explicit path**: the step input carries `model` and `messages`, and
/// the config records truthfully that no `capability_agent` was resolved (S13 §5.3 "Sequencing").
pub(crate) async fn resolve(
    pool: &PgPool,
    tenant: TenantId,
    run_id: Uuid,
    run_step_id: Option<Uuid>,
    capability_agent_id: Option<Uuid>,
    node_input: &Value,
) -> Result<AgentConfig, ActionError> {
    if let Some(agent_id) = capability_agent_id {
        return resolve_bound(pool, tenant, run_id, run_step_id, agent_id, node_input).await;
    }

    let has_model = node_input.get("model").and_then(Value::as_str).is_some();
    let has_messages = node_input.get("messages").is_some();
    if !(has_model && has_messages) {
        return Err(ActionError::new(
            422,
            "agent.config_missing",
            format!(
                "run {run_id} declares no capability_agent and the step input carries no explicit \
                 agent config (model + messages are required on the explicit path)"
            ),
        ));
    }
    let input: AgentRunIn = serde_json::from_value(node_input.clone()).map_err(|error| {
        ActionError::new(
            422,
            "agent.config_invalid",
            format!("the step input is not an agent config: {error}"),
        )
    })?;
    if input.capability_agent_id.is_some() {
        return Err(config_conflict(
            "the step input claims a capability_agent_id, but the run declares no executor \
             (CONTRACT §17: the run is the one source)",
        ));
    }
    let mut config = input.into_config();
    config.capability_agent_id = None;
    config.run_id = Some(run_id);
    config.run_step_id = run_step_id;
    Ok(config)
}

/// The bound half of [`resolve`]: the row is the configuration; the step input may only narrow it.
async fn resolve_bound(
    pool: &PgPool,
    tenant: TenantId,
    run_id: Uuid,
    run_step_id: Option<Uuid>,
    agent_id: Uuid,
    node_input: &Value,
) -> Result<AgentConfig, ActionError> {
    if let Some(claimed) = node_input
        .get("capability_agent_id")
        .and_then(Value::as_str)
    {
        let claimed = Uuid::parse_str(claimed).map_err(|error| {
            config_invalid(format!("capability_agent_id is not a uuid: {error}"))
        })?;
        if claimed != agent_id {
            return Err(config_conflict(format!(
                "the step input names capability_agent {claimed}; the run is bound to {agent_id}"
            )));
        }
    }

    let mut tx = store::begin_scoped(pool, tenant).await.map_err(|error| {
        tracing::error!(%error, "agent.run: begin the capability read");
        ActionError::internal()
    })?;
    let record = store::capability::get_agent_tx(&mut tx, tenant, agent_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "agent.run: read the capability agent");
            ActionError::internal()
        })?
        .ok_or_else(|| {
            ActionError::new(
                422,
                "agent.executor_missing",
                format!("run {run_id} names capability_agent {agent_id}, which does not exist"),
            )
        })?;
    tx.commit().await.map_err(|error| {
        tracing::error!(%error, "agent.run: commit the capability read");
        ActionError::internal()
    })?;

    if record.status != "active" {
        return Err(ActionError::new(
            409,
            "capability_not_ready",
            format!(
                "capability_agent {agent_id} is {:?}, not active",
                record.status
            ),
        ));
    }

    // How the row's configuration is defended: a step may narrow it, never substitute it.
    if let Some(model) = node_input.get("model").and_then(Value::as_str)
        && model != record.model_ref.as_str()
    {
        return Err(config_conflict(format!(
            "the step input names model {model:?}; the bound capability_agent {agent_id} uses {:?}",
            record.model_ref
        )));
    }
    if let Some(tools) = node_input.get("tools") {
        match tools.as_array() {
            Some(list) if list.is_empty() => {}
            _ => {
                return Err(config_conflict(format!(
                    "the step input supplies tools; the bound capability_agent {agent_id}'s \
                     allowed_actions are the tool surface"
                )));
            }
        }
    }
    if let Some(reference) = node_input.get("prompt_ref").and_then(Value::as_str)
        && Some(reference) != record.prompt_ref.as_deref()
    {
        return Err(config_conflict(format!(
            "the step input names prompt {reference:?}; the bound capability_agent {agent_id} \
             resolves {:?}",
            record.prompt_ref
        )));
    }
    let bound_turns = u8::try_from(record.max_iterations).unwrap_or(MAX_TURNS);
    let max_turns = match node_input.get("max_turns") {
        None | Some(Value::Null) => bound_turns,
        Some(value) => {
            let asked = value
                .as_u64()
                .ok_or_else(|| config_invalid("max_turns is not a non-negative integer"))?;
            if asked > u64::from(bound_turns) {
                return Err(cap_exceeds_bound(
                    "max_turns",
                    asked,
                    u64::from(bound_turns),
                ));
            }
            u8::try_from(asked).map_err(|_| config_invalid("max_turns does not fit a u8"))?
        }
    };
    let bound_timeout_ms = i64::from(record.timeout_secs) * 1000;
    let timeout_ms = match node_input.get("timeout_ms") {
        None | Some(Value::Null) => bound_timeout_ms,
        Some(value) => {
            let asked = value
                .as_i64()
                .ok_or_else(|| config_invalid("timeout_ms is not an integer"))?;
            if asked > bound_timeout_ms {
                return Err(cap_exceeds_bound("timeout_ms", asked, bound_timeout_ms));
            }
            asked
        }
    };
    let max_tool_calls = match node_input.get("max_tool_calls") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            value
                .as_u64()
                .and_then(|asked| u8::try_from(asked).ok())
                .ok_or_else(|| config_invalid("max_tool_calls is not a u8"))?,
        ),
    };

    let mut tools = Vec::with_capacity(record.allowed_actions.len());
    for name in &record.allowed_actions {
        let Some(spec) = registry().get(name) else {
            return Err(capability_not_ready(
                "allowed_actions",
                "unknown_action",
                format!("{name} is not a registered action"),
            ));
        };
        let parameters = serde_json::to_value((spec.input)())
            .unwrap_or_else(|_| json!({"type": "object", "additionalProperties": true}));
        tools.push(json!({
            "type": "function",
            "function": {"name": spec.name, "parameters": parameters}
        }));
    }

    let mut messages: Vec<AgentMessage> = node_input
        .get("messages")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| {
            ActionError::new(
                422,
                "agent.config_invalid",
                format!("the node input's messages are malformed: {error}"),
            )
        })?
        .unwrap_or_default();

    // The resolved path has exactly one source for the system message: the catalogue (SPEC D27).
    // A caller-supplied one here would make the compiled prompt and the input two truths, so it
    // is refused rather than silently preferred either way.
    if let Some(reference) = record.prompt_ref.as_deref() {
        if messages.iter().any(|message| message.role == "system") {
            return Err(ActionError::new(
                422,
                "agent.prompt_conflict",
                format!(
                    "capability_agent {agent_id} resolves prompt {reference:?}; the step input must \
                     not also carry a system message (the catalogue is the one source)"
                ),
            ));
        }
        let compiled = crate::prompt::compile_for_run(pool, tenant, run_id, run_step_id, reference)
            .await
            .map_err(|error| error.into_action_error("agent.run"))?;
        messages.insert(
            0,
            AgentMessage {
                role: "system".to_owned(),
                content: Some(compiled.text),
                tool_calls: Vec::new(),
                tool_call_id: None,
                name: None,
            },
        );
    }

    let input = AgentRunIn {
        model: record.model_ref.clone(),
        messages,
        tools,
        max_turns: Some(max_turns),
        max_tool_calls,
        timeout_ms: Some(timeout_ms),
        capability_agent_id: Some(agent_id),
        prompt_ref: record.prompt_ref.clone(),
        run_id: Some(run_id),
        run_step_id,
        depth: Some(0),
    };
    Ok(input.into_config())
}

/// The step input is malformed for the field it names.
fn config_invalid(detail: impl Into<String>) -> ActionError {
    ActionError::new(422, "agent.config_invalid", detail)
}

/// The step input tries to substitute a field the run's `capability_agent` owns.
fn config_conflict(detail: impl Into<String>) -> ActionError {
    ActionError::new(422, "agent.config_conflict", detail)
}

/// The step input raises a cap above the bound agent's (CONTRACT §17: lower, never raise).
fn cap_exceeds_bound(
    field: &str,
    asked: impl std::fmt::Display,
    bound: impl std::fmt::Display,
) -> ActionError {
    ActionError::new(
        422,
        "agent.cap_exceeds_bound",
        format!(
            "the step input's {field} = {asked} exceeds the bound capability agent's {bound} \
             (CONTRACT §17: a step may lower a cap, never raise it)"
        ),
    )
}

fn capability_not_ready(field: &str, code: &str, detail: impl Into<String>) -> ActionError {
    ActionError::new(
        409,
        "capability_not_ready",
        "agent.run: the run's capability is not ready",
    )
    .with_errors(vec![FieldError {
        field: field.to_owned(),
        code: code.to_owned(),
        detail: detail.into(),
    }])
}

/// Drive one loop to its end. Every effect goes through [`invoke`], so each model call and tool
/// call is an `action_invocation` on the run's trace id with the engine's idempotency and audit.
pub(crate) async fn drive(
    pool: &PgPool,
    tenant: TenantId,
    run: &RunTrace<'_>,
    config: AgentConfig,
) -> Result<AgentRunOut, ActionError> {
    let (mut agent, mut plan) = Agent::start(config).map_err(typed)?;
    let started = Instant::now();
    let allowed: Vec<String> = agent
        .tool_names()
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    let prompt_ref = agent.prompt_ref().map(str::to_owned);

    loop {
        if started.elapsed().as_millis() as i64 > agent.caps().timeout_ms {
            return Err(typed(AgentError::Timeout {
                elapsed_ms: started.elapsed().as_millis() as i64,
                cap: agent.caps().timeout_ms,
            }));
        }
        match plan {
            Plan::CallModel {
                model,
                messages,
                tools,
            } => {
                let turn = u32::from(agent.turns()) + 1;
                let input = json!({
                    "model": model,
                    "messages": messages.iter().map(wire_message).collect::<Vec<_>>(),
                    "tools": tools,
                });
                let outcome = invoke(
                    pool,
                    registry(),
                    tenant,
                    InvokeRequest {
                        action: MODEL_ACTION.to_owned(),
                        input,
                        precondition: None,
                        idempotency_key: run
                            .idempotency_prefix
                            .map(|prefix| format!("{prefix}:turn{turn}")),
                        actor_id: Uuid::nil(),
                        actor_kind: "service".to_owned(),
                        role: Role::KataExecutor,
                        policy: Default::default(),
                        authority: Authority::Internal,
                        request_id: Some(format!("agent.run:{}:turn{turn}", run.run_id)),
                        trace_id: Some(format!("run:{}", run.run_id)),
                        depth: run.depth.saturating_add(1),
                        app_key: run.app_key.to_owned(),
                        usage: Some(UsageContext {
                            caller_kind: CallerKind::AgentRun,
                            run_id: Some(run.run_id),
                            run_step_id: run.run_step_id,
                            conversation_turn_id: None,
                            eval_run_id: None,
                            prompt_ref: prompt_ref.clone(),
                            prompt_version: prompt_ref
                                .as_deref()
                                .and_then(|reference| reference.split_once('@'))
                                .map(|(_, version)| version.to_owned()),
                            correlation_id: Some(format!("run:{}", run.run_id)),
                        }),
                    },
                )
                .await?;
                // A 5xx from the router must retry the tick, not fail the node: the caller maps
                // this status to `Deferred`.
                let reply = model_reply(&outcome.body)?;
                plan = agent.apply(Reply::Model(reply));
            }
            Plan::CallTool(call) => {
                let reply = if allowed.iter().any(|name| name == &call.name) {
                    let index = agent.tool_calls();
                    let outcome = invoke(
                        pool,
                        registry(),
                        tenant,
                        InvokeRequest {
                            action: call.name.clone(),
                            input: call.arguments.clone(),
                            precondition: None,
                            idempotency_key: run
                                .idempotency_prefix
                                .map(|prefix| format!("{prefix}:tool{index}")),
                            actor_id: Uuid::nil(),
                            actor_kind: "service".to_owned(),
                            role: Role::KataExecutor,
                            policy: Default::default(),
                            authority: Authority::Internal,
                            request_id: Some(format!("agent.run:{}:tool{index}", run.run_id)),
                            trace_id: Some(format!("run:{}", run.run_id)),
                            depth: run.depth.saturating_add(1),
                            app_key: run.app_key.to_owned(),
                            usage: None,
                        },
                    )
                    .await;
                    match outcome {
                        Ok(outcome) => ToolReply {
                            id: call.id.clone(),
                            content: outcome.body.to_string(),
                            is_error: false,
                            invocation_id: outcome.invocation_id,
                        },
                        // A tool failure is the model's information, not the run's death: the
                        // contract keeps the loop alive and records the refusal.
                        Err(error) => ToolReply {
                            id: call.id.clone(),
                            content: json!({
                                "type": format!("/problems/{}", error.code),
                                "detail": error.detail,
                            })
                            .to_string(),
                            is_error: true,
                            invocation_id: None,
                        },
                    }
                } else {
                    ToolReply {
                        id: call.id.clone(),
                        content: json!({
                            "type": "/problems/action-not-found",
                            "detail": format!(
                                "{} is not one of this executor's allowed actions",
                                call.name
                            ),
                        })
                        .to_string(),
                        is_error: true,
                        invocation_id: None,
                    }
                };
                plan = agent.apply(Reply::Tool(reply));
            }
            Plan::Finished(output) => return Ok(AgentRunOut::from(output)),
            Plan::Failed(error) => return Err(typed(error)),
        }
    }
}

/// Resolve and drive one `agent.run` step — the one seam every caller of the loop shares.
///
/// S11's `procedure.step` and S13's `workflow.node` are both step executors (CONTRACT §17), so
/// neither may reach the action's handler: it refuses by construction (it holds the invocation
/// transaction and no pool). Resolving the run's `capability_agent` into the step input and
/// driving the loop is executor work, and it is identical for both — one implementation here, so
/// a procedure step naming `agent.run` behaves exactly as a workflow node does. S18's benchmark
/// producer is a third **caller**, not a fourth executor: it resolves the agent into an explicit
/// step input and drives this same loop, so the arm measures the product's loop rather than a
/// second implementation of it.
///
/// `idempotency_prefix` is the caller's attempt key (`run_step` position/attempt): every model
/// and tool call is keyed under it, so a retry replays stored responses instead of buying a
/// second round of effects.
#[allow(clippy::too_many_arguments)]
pub async fn run_step(
    pool: &PgPool,
    tenant: TenantId,
    run_id: Uuid,
    run_step_id: Option<Uuid>,
    capability_agent_id: Option<Uuid>,
    app_key: &str,
    idempotency_prefix: Option<&str>,
    depth: u32,
    input: &Value,
) -> Result<AgentRunOut, ActionError> {
    let config = resolve(
        pool,
        tenant,
        run_id,
        run_step_id,
        capability_agent_id,
        input,
    )
    .await?;
    let trace = RunTrace {
        run_id,
        run_step_id,
        depth,
        idempotency_prefix,
        app_key,
    };
    drive(pool, tenant, &trace, config).await
}

/// The OpenAI message shape the router accepts.
fn wire_message(message: &Message) -> Value {
    let mut wire =
        json!({"role": message.role, "content": message.content.clone().unwrap_or_default()});
    if !message.tool_calls.is_empty() {
        wire["tool_calls"] = Value::Array(
            message
                .tool_calls
                .iter()
                .map(|call| {
                    json!({
                        "id": call.id,
                        "type": "function",
                        "function": {
                            "name": call.name,
                            "arguments": call.arguments.to_string(),
                        }
                    })
                })
                .collect(),
        );
    }
    if let Some(id) = &message.tool_call_id {
        wire["tool_call_id"] = json!(id);
    }
    if let Some(name) = &message.name {
        wire["name"] = json!(name);
    }
    wire
}

/// Parse the router's answer into the loop's reply.
fn model_reply(body: &Value) -> Result<ModelReply, ActionError> {
    let doc = body.get("output").unwrap_or(body);
    let message = doc
        .pointer("/choices/0/message")
        .ok_or_else(|| bad_response("the router's answer has no choices[0].message"))?;
    let content = message
        .get("content")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let mut tool_calls = Vec::new();
    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
        for (index, call) in calls.iter().enumerate() {
            let name = call
                .pointer("/function/name")
                .and_then(Value::as_str)
                .ok_or_else(|| bad_response("a tool call has no function.name"))?;
            let arguments = match call.pointer("/function/arguments") {
                Some(Value::String(raw)) => serde_json::from_str(raw).unwrap_or_else(|_| json!({})),
                Some(value) => value.clone(),
                None => json!({}),
            };
            let id = call
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("call-{index}"));
            tool_calls.push(ToolCall {
                id,
                name: name.to_owned(),
                arguments,
            });
        }
    }
    Ok(ModelReply {
        content,
        tool_calls,
        usage: doc.get("usage").cloned(),
    })
}

fn bad_response(detail: &str) -> ActionError {
    ActionError::new(
        502,
        "inference-bad-response",
        format!("{MODEL_ACTION}: {detail}"),
    )
}

/// Turn a loop refusal into the action error the node records as its `error_class`.
fn typed(error: AgentError) -> ActionError {
    ActionError::new(422, error.code(), error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_router_answer_becomes_a_reply_with_parsed_arguments() {
        let body = json!({
            "output": {
                "choices": [{"message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call-1",
                        "type": "function",
                        "function": {"name": "cards.move", "arguments": "{\"card_id\":\"c1\"}"}
                    }]
                }}],
                "usage": {"total_tokens": 7}
            }
        });
        let reply = model_reply(&body).expect("a well-formed answer");
        assert_eq!(reply.tool_calls.len(), 1);
        assert_eq!(reply.tool_calls[0].name, "cards.move");
        assert_eq!(reply.tool_calls[0].arguments, json!({"card_id": "c1"}));
        assert_eq!(reply.usage, Some(json!({"total_tokens": 7})));
        assert!(reply.content.is_none());
    }

    #[test]
    fn an_answer_without_a_message_is_a_typed_refusal_not_an_empty_turn() {
        let error = model_reply(&json!({"output": {"choices": []}})).expect_err("refused");
        assert_eq!(error.code, "inference-bad-response");
    }

    #[test]
    fn the_wire_message_serialises_tool_calls_and_results() {
        let call = ToolCall {
            id: "call-1".to_owned(),
            name: "cards.move".to_owned(),
            arguments: json!({"card_id": "c1"}),
        };
        let assistant = Message {
            role: "assistant".to_owned(),
            content: None,
            tool_calls: vec![call.clone()],
            tool_call_id: None,
            name: None,
        };
        let wire = wire_message(&assistant);
        assert_eq!(wire["tool_calls"][0]["function"]["name"], "cards.move");
        assert_eq!(
            wire["tool_calls"][0]["function"]["arguments"],
            "{\"card_id\":\"c1\"}"
        );
        let result = Message::tool_result(&call, "{\"ok\":true}");
        let wire = wire_message(&result);
        assert_eq!(wire["role"], "tool");
        assert_eq!(wire["tool_call_id"], "call-1");
    }

    #[test]
    fn the_explicit_step_input_is_the_loop_config() {
        let input: AgentRunIn = serde_json::from_value(json!({
            "model": "m",
            "messages": [{"role": "system", "content": "s"}],
            "tools": [],
            "max_turns": 2
        }))
        .expect("parses");
        let config = input.into_config();
        assert_eq!(config.model, "m");
        assert_eq!(config.max_turns, Some(2));
        assert_eq!(config.caps().expect("valid").turns, 2);
    }
}
