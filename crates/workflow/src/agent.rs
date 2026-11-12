#![forbid(unsafe_code)]
//! The model-driven executor (S13 step 8): `agent.run`, one bounded tool loop.
//!
//! The split is the same as [`crate::scheduler`]'s, for the same reason: this crate has no I/O and
//! no clock. [`Agent::start`] validates the step input and says what the caller must do — call the
//! model, call a tool — and [`Agent::apply`] takes the reply and says what comes next. The caller
//! (S13's `workflow.node`, S11's `procedure.step`) performs the effects and reports them.
//!
//! Everything the loop needs arrives in the step input (CONTRACT §17 / SPEC D22): `agent.run` never
//! queries a `capability_agent` row. The caps are the contract's 8/20 turns, 20/50 tool calls and
//! 300 s; a caller may lower one, never raise it above the ceiling, and hitting one fails **typed**
//! (`agent.turn_limit`, `agent.tool_limit`, `agent.timeout`) — never a silent stop (S13 §8 trap 5).

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Turns a step input gets when it names none (CONTRACT §17).
pub const DEFAULT_TURNS: u8 = 8;
/// The turn ceiling; a step input or a `capability_agent` row may not exceed it.
pub const MAX_TURNS: u8 = 20;
/// Tool calls a step input gets when it names none.
pub const DEFAULT_TOOL_CALLS: u8 = 20;
/// The tool-call ceiling.
pub const MAX_TOOL_CALLS: u8 = 50;
/// The wall-clock ceiling for one loop, in milliseconds (300 s).
pub const MAX_TIMEOUT_MS: i64 = 300_000;
/// The wall clock a step input gets when it names none.
pub const DEFAULT_TIMEOUT_MS: i64 = MAX_TIMEOUT_MS;
/// Agent→agent depth: an `agent.run` inside an `agent.run` is refused (S13 §9.5).
pub const MAX_DEPTH: u8 = 0;

/// One chat message in the normalised shape the loop records and replays.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// `system`, `user`, `assistant` or `tool`.
    pub role: String,
    /// The text, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// The tool calls an assistant message asked for.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// The call a `tool` message answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// The tool's name, on a `tool` message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Message {
    /// A message with text only.
    pub fn text(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: Some(content.into()),
            tool_calls: Vec::new(),
            tool_call_id: None,
            name: None,
        }
    }

    /// The result of one tool call, fed back to the model.
    pub fn tool_result(call: &ToolCall, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_owned(),
            content: Some(content.into()),
            tool_calls: Vec::new(),
            tool_call_id: Some(call.id.clone()),
            name: Some(call.name.clone()),
        }
    }
}

/// One tool call the model asked for, normalised.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// The model's call id; the `tool` message answers it.
    pub id: String,
    /// The registry action to invoke.
    pub name: String,
    /// The arguments, already parsed (the router sends a JSON string).
    #[serde(default)]
    pub arguments: Value,
}

/// The validated caps one loop runs under.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Caps {
    /// Model calls allowed.
    pub turns: u8,
    /// Tool calls allowed, counted across every turn.
    pub tool_calls: u8,
    /// Wall clock for the whole loop.
    pub timeout_ms: i64,
    /// Agent→agent depth; anything above [`MAX_DEPTH`] is refused.
    pub depth: u8,
}

impl Default for Caps {
    fn default() -> Self {
        Self {
            turns: DEFAULT_TURNS,
            tool_calls: DEFAULT_TOOL_CALLS,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            depth: MAX_DEPTH,
        }
    }
}

/// The `agent.run` step input (CONTRACT §17): resolved by the step executor, never by the loop.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentConfig {
    /// The model ref the router is asked for.
    pub model: String,
    /// The conversation, including the one system message the executor rendered from `prompt_ref`.
    pub messages: Vec<Message>,
    /// The tool schemas the model may name, built from the row's `allowed_actions`.
    #[serde(default)]
    pub tools: Vec<Value>,
    /// Turns allowed; absent takes [`DEFAULT_TURNS`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u8>,
    /// Tool calls allowed; absent takes [`DEFAULT_TOOL_CALLS`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tool_calls: Option<u8>,
    /// Wall clock; absent takes [`DEFAULT_TIMEOUT_MS`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<i64>,
    /// The `capability_agent` row the executor resolved this from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_agent_id: Option<Uuid>,
    /// `<name>@<version>`, carried so the invocation hash and the trace pin what ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_ref: Option<String>,
    /// The run the loop executes inside (the caller's join key).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<Uuid>,
    /// The `run_step` the loop executes for, when the caller knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_step_id: Option<Uuid>,
    /// Agent→agent depth; absent is 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u8>,
}

impl AgentConfig {
    /// The caps this input asks for, or the first one that is out of range.
    pub fn caps(&self) -> Result<Caps, AgentError> {
        let turns = self.max_turns.unwrap_or(DEFAULT_TURNS);
        if turns == 0 || turns > MAX_TURNS {
            return Err(AgentError::CapInvalid {
                field: "max_turns",
                value: i64::from(turns),
                cap: i64::from(MAX_TURNS),
            });
        }
        let tool_calls = self.max_tool_calls.unwrap_or(DEFAULT_TOOL_CALLS);
        if tool_calls == 0 || tool_calls > MAX_TOOL_CALLS {
            return Err(AgentError::CapInvalid {
                field: "max_tool_calls",
                value: i64::from(tool_calls),
                cap: i64::from(MAX_TOOL_CALLS),
            });
        }
        let timeout_ms = self.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
        if timeout_ms <= 0 || timeout_ms > MAX_TIMEOUT_MS {
            return Err(AgentError::CapInvalid {
                field: "timeout_ms",
                value: timeout_ms,
                cap: MAX_TIMEOUT_MS,
            });
        }
        let depth = self.depth.unwrap_or(MAX_DEPTH);
        if depth > MAX_DEPTH {
            return Err(AgentError::Depth { depth });
        }
        if self.model.trim().is_empty() {
            return Err(AgentError::CapInvalid {
                field: "model",
                value: 0,
                cap: 0,
            });
        }
        Ok(Caps {
            turns,
            tool_calls,
            timeout_ms,
            depth,
        })
    }
}

/// Why a loop stopped, one code per failure the contract names.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AgentError {
    /// A cap in the step input is out of range; the input is refused, never clamped.
    #[error("{field} = {value} is outside 1..={cap}")]
    CapInvalid {
        /// The input field.
        field: &'static str,
        /// What it asked for.
        value: i64,
        /// The ceiling.
        cap: i64,
    },
    /// The loop used every turn it was allowed and the model still asked for a tool.
    #[error("the agent used its {turns} of {cap} turns")]
    TurnLimit {
        /// Turns used.
        turns: u8,
        /// Turns allowed.
        cap: u8,
    },
    /// The loop asked for more tool calls than it was allowed.
    #[error("the agent asked for {requested} tool calls with {cap} allowed")]
    ToolLimit {
        /// Calls requested in total.
        requested: u32,
        /// Calls allowed.
        cap: u8,
    },
    /// The loop ran longer than its wall clock.
    #[error("the agent ran {elapsed_ms} ms; its cap is {cap} ms")]
    Timeout {
        /// Measured wall time.
        elapsed_ms: i64,
        /// The cap.
        cap: i64,
    },
    /// An `agent.run` inside an `agent.run`.
    #[error("agent.run cannot call agent.run (depth {depth})")]
    Depth {
        /// The depth the input asked for.
        depth: u8,
    },
}

impl AgentError {
    /// The typed code the error class and the audit row record.
    pub fn code(&self) -> &'static str {
        match self {
            Self::CapInvalid { .. } => "agent.cap_invalid",
            Self::TurnLimit { .. } => "agent.turn_limit",
            Self::ToolLimit { .. } => "agent.tool_limit",
            Self::Timeout { .. } => "agent.timeout",
            Self::Depth { .. } => "agent.depth",
        }
    }
}

/// What one model reply said.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelReply {
    /// The assistant text, when there is one.
    pub content: Option<String>,
    /// The calls it asked for; empty means the answer is final.
    pub tool_calls: Vec<ToolCall>,
    /// The router's `usage` object, carried to the ledger by the engine.
    pub usage: Option<Value>,
}

impl ModelReply {
    /// A final answer with no tool calls.
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: Some(content.into()),
            tool_calls: Vec::new(),
            usage: None,
        }
    }

    /// A reply that asks for tools.
    pub fn calls(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            content: None,
            tool_calls,
            usage: None,
        }
    }
}

/// What one tool invocation reported.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolReply {
    /// The call this answers; a mismatch is ignored rather than recorded against the wrong tool.
    pub id: String,
    /// The text fed back to the model (a JSON body, an error message).
    pub content: String,
    /// Whether the invocation failed or was refused.
    pub is_error: bool,
    /// The `action_invocation` the engine wrote, when it has one.
    pub invocation_id: Option<Uuid>,
}

/// What the caller reports to [`Agent::apply`].
#[derive(Clone, Debug, PartialEq)]
pub enum Reply {
    /// The model answered.
    Model(ModelReply),
    /// A tool ran (or was refused).
    Tool(ToolReply),
    /// The caller's wall clock, checked against the cap.
    Elapsed {
        /// Milliseconds since the loop started.
        elapsed_ms: i64,
    },
}

/// One recorded tool call in the loop's output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolCallRecord {
    /// The model's call id.
    pub id: String,
    /// The action invoked.
    pub name: String,
    /// Whether it failed or was refused.
    pub is_error: bool,
    /// The `action_invocation` the engine wrote.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invocation_id: Option<Uuid>,
}

/// What a finished loop produced.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentOutput {
    /// The model's final text.
    pub output: String,
    /// Turns used.
    pub turns: u8,
    /// Every tool call, in order.
    pub tool_calls: Vec<ToolCallRecord>,
    /// The last reply's `usage`, when the router sent one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Value>,
    /// The `capability_agent` the executor resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_agent_id: Option<Uuid>,
    /// `<name>@<version>` the executor rendered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_ref: Option<String>,
}

/// What the caller must do next.
#[derive(Clone, Debug, PartialEq)]
pub enum Plan {
    /// Call the model with these arguments; then [`Agent::apply`] a [`Reply::Model`].
    CallModel {
        /// The model ref.
        model: String,
        /// The conversation so far.
        messages: Vec<Message>,
        /// The tool schemas.
        tools: Vec<Value>,
    },
    /// Invoke this action; then [`Agent::apply`] a [`Reply::Tool`].
    CallTool(ToolCall),
    /// The loop answered.
    Finished(AgentOutput),
    /// The loop hit a cap or a refusal; the node records `error_class = code`.
    Failed(AgentError),
}

#[derive(Clone, Debug, PartialEq)]
enum Phase {
    Model,
    Tool,
    Done(AgentOutput),
    Failed(AgentError),
}

/// One bounded tool loop. Start it, then drive it with the replies.
#[derive(Clone, Debug)]
pub struct Agent {
    caps: Caps,
    model: String,
    tools: Vec<Value>,
    messages: Vec<Message>,
    prompt_ref: Option<String>,
    capability_agent_id: Option<Uuid>,
    turns: u8,
    tool_calls: u32,
    records: Vec<ToolCallRecord>,
    pending: VecDeque<ToolCall>,
    usage: Option<Value>,
    phase: Phase,
}

impl Agent {
    /// Validate the step input and say what the caller must do first.
    pub fn start(config: AgentConfig) -> Result<(Self, Plan), AgentError> {
        let caps = config.caps()?;
        let agent = Self {
            caps,
            model: config.model,
            tools: config.tools,
            messages: config.messages,
            prompt_ref: config.prompt_ref,
            capability_agent_id: config.capability_agent_id,
            turns: 0,
            tool_calls: 0,
            records: Vec::new(),
            pending: VecDeque::new(),
            usage: None,
            phase: Phase::Model,
        };
        let plan = agent.next();
        Ok((agent, plan))
    }

    /// The caps this loop runs under.
    pub fn caps(&self) -> Caps {
        self.caps
    }

    /// Turns used so far.
    pub fn turns(&self) -> u8 {
        self.turns
    }

    /// Tool calls executed so far.
    pub fn tool_calls(&self) -> u32 {
        self.tool_calls
    }

    /// The tool names the model may name: `function.name`, or a bare `name` for a schema without
    /// the `type: function` wrapper.
    pub fn tool_names(&self) -> Vec<&str> {
        self.tools
            .iter()
            .filter_map(|tool| {
                tool.pointer("/function/name")
                    .and_then(Value::as_str)
                    .or_else(|| tool.get("name").and_then(Value::as_str))
            })
            .collect()
    }

    /// The `prompt_ref` the executor resolved.
    pub fn prompt_ref(&self) -> Option<&str> {
        self.prompt_ref.as_deref()
    }

    /// The `capability_agent` the executor resolved.
    pub fn capability_agent_id(&self) -> Option<Uuid> {
        self.capability_agent_id
    }

    /// Report the reply to the pending plan and say what comes next.
    pub fn apply(&mut self, reply: Reply) -> Plan {
        match &self.phase {
            Phase::Done(_) | Phase::Failed(_) => return self.next(),
            _ => {}
        }
        match reply {
            Reply::Elapsed { elapsed_ms } => {
                if elapsed_ms > self.caps.timeout_ms {
                    self.phase = Phase::Failed(AgentError::Timeout {
                        elapsed_ms,
                        cap: self.caps.timeout_ms,
                    });
                }
            }
            Reply::Model(reply) => {
                if !matches!(self.phase, Phase::Model) {
                    return self.next();
                }
                self.turns = self.turns.saturating_add(1);
                self.messages.push(Message {
                    role: "assistant".to_owned(),
                    content: reply.content.clone(),
                    tool_calls: reply.tool_calls.clone(),
                    tool_call_id: None,
                    name: None,
                });
                if let Some(usage) = reply.usage {
                    self.usage = Some(usage);
                }
                if reply.tool_calls.is_empty() {
                    self.phase = Phase::Done(AgentOutput {
                        output: reply.content.unwrap_or_default(),
                        turns: self.turns,
                        tool_calls: std::mem::take(&mut self.records),
                        usage: self.usage.clone(),
                        capability_agent_id: self.capability_agent_id,
                        prompt_ref: self.prompt_ref.clone(),
                    });
                } else if self.turns >= self.caps.turns {
                    // There is no model call left to answer the tools, so running them would be
                    // effects spent on a loop that cannot finish: fail before executing any.
                    self.phase = Phase::Failed(AgentError::TurnLimit {
                        turns: self.turns,
                        cap: self.caps.turns,
                    });
                } else {
                    let requested = self.tool_calls + reply.tool_calls.len() as u32;
                    if requested > u32::from(self.caps.tool_calls) {
                        self.phase = Phase::Failed(AgentError::ToolLimit {
                            requested,
                            cap: self.caps.tool_calls,
                        });
                    } else {
                        self.pending = reply.tool_calls.into();
                        self.phase = Phase::Tool;
                    }
                }
            }
            Reply::Tool(reply) => {
                if !matches!(self.phase, Phase::Tool) {
                    return self.next();
                }
                if self.pending.front().map(|call| call.id.as_str()) != Some(reply.id.as_str()) {
                    // Not the call we are waiting for; a misrouted reply must not be recorded
                    // against another tool.
                    return self.next();
                }
                let Some(call) = self.pending.pop_front() else {
                    return self.next();
                };
                self.tool_calls = self.tool_calls.saturating_add(1);
                self.records.push(ToolCallRecord {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    is_error: reply.is_error,
                    invocation_id: reply.invocation_id,
                });
                self.messages
                    .push(Message::tool_result(&call, reply.content));
                if self.pending.is_empty() {
                    self.phase = Phase::Model;
                }
            }
        }
        self.next()
    }

    fn next(&self) -> Plan {
        match &self.phase {
            Phase::Model => Plan::CallModel {
                model: self.model.clone(),
                messages: self.messages.clone(),
                tools: self.tools.clone(),
            },
            Phase::Tool => match self.pending.front() {
                Some(call) => Plan::CallTool(call.clone()),
                None => Plan::CallModel {
                    model: self.model.clone(),
                    messages: self.messages.clone(),
                    tools: self.tools.clone(),
                },
            },
            Phase::Done(output) => Plan::Finished(output.clone()),
            Phase::Failed(error) => Plan::Failed(error.clone()),
        }
    }
}
