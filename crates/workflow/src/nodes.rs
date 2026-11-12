#![forbid(unsafe_code)]
//! Pure per-node planning (S13 steps 6–7): what a node runs, what it reports, and its limits.
//!
//! Nothing here reads a database, a clock or a registry: the caller supplies the template context and
//! executes the effect. That is what lets `workflow_nodes` prove template rendering, predicates,
//! retry/timeout classification and idempotency without Postgres.

use conveyor_domain::template::{self, TemplateCtx, TemplateError};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::definition::{MAX_TIMEOUT_MS, Node, Predicate};

/// The frozen UUIDv5 namespace every node attempt's `Idempotency-Key` is derived in.
///
/// The literal is `uuid5(NAMESPACE_URL, "https://conveyor.dev/workflow.node")`, computed once and
/// pasted because `new_v5` is not a `const fn`. Frozen: changing it silently changes every key, so a
/// replay after a deploy would look like a fresh invocation.
pub const WORKFLOW_NODE_NS: Uuid = Uuid::from_u128(0xa7dc_4d54_f0a3_5a2e_913c_4da6_8af1_376d);

/// The `Idempotency-Key` one attempt presents: `uuidv5(ns, "<run_id>:<node_id>:<attempt>")`
/// (S13 §5.3). Stable across a replay of the same attempt, different for a retry.
pub fn idempotency_key(run_id: Uuid, node_id: &str, attempt: u32) -> String {
    Uuid::new_v5(
        &WORKFLOW_NODE_NS,
        format!("{run_id}:{node_id}:{attempt}").as_bytes(),
    )
    .to_string()
}

/// Why a node cannot be planned.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NodeError {
    /// A template did not resolve.
    #[error("node {node:?}: {detail}")]
    Template {
        /// The node id.
        node: String,
        /// The template's message.
        detail: String,
    },
    /// A predicate's operands cannot be compared.
    #[error("node {node:?}: {detail}")]
    Predicate {
        /// The node id.
        node: String,
        /// What could not be compared.
        detail: String,
    },
    /// The node declares a timeout above the run-time cap.
    #[error("node {node:?}: timeout {declared} ms exceeds the {max} ms cap")]
    Timeout {
        /// The node id.
        node: String,
        /// The declared value.
        declared: i64,
        /// [`MAX_TIMEOUT_MS`].
        max: i64,
    },
    /// A loop's `items` did not resolve to an array.
    #[error("node {node:?}: `items` resolves to {found}, not an array")]
    Items {
        /// The node id.
        node: String,
        /// The JSON type it resolved to.
        found: &'static str,
    },
    /// A node kind needs a field the document does not carry (publish should have refused it).
    #[error("node {node:?}: a {kind} node has no `{field}`")]
    Missing {
        /// The node id.
        node: String,
        /// Its kind.
        kind: &'static str,
        /// The missing field.
        field: &'static str,
    },
}

/// Resolve a node's `input_template` (an absent template is `{}`).
pub fn input(node: &Node, ctx: &TemplateCtx) -> Result<Value, NodeError> {
    match &node.input_template {
        Some(value) => template::resolve(value, ctx).map_err(|error| NodeError::Template {
            node: node.id.clone(),
            detail: error.to_string(),
        }),
        None => Ok(json!({})),
    }
}

/// Resolve a `procedure` node's `inputs` template (an absent template is `{}`).
pub fn procedure_inputs(node: &Node, ctx: &TemplateCtx) -> Result<Value, NodeError> {
    match &node.inputs {
        Some(value) => template::resolve(value, ctx).map_err(|error| NodeError::Template {
            node: node.id.clone(),
            detail: error.to_string(),
        }),
        None => Ok(json!({})),
    }
}

/// Resolve and evaluate a `condition` node's predicate.
pub fn evaluate(node: &Node, ctx: &TemplateCtx) -> Result<bool, NodeError> {
    let predicate = node.predicate.as_ref().ok_or_else(|| NodeError::Missing {
        node: node.id.clone(),
        kind: "condition",
        field: "predicate",
    })?;
    evaluate_at(&node.id, predicate, ctx)
}

fn evaluate_at(node: &str, predicate: &Predicate, ctx: &TemplateCtx) -> Result<bool, NodeError> {
    match predicate {
        Predicate::Eq { left, right } => {
            let (left, right) = operands(node, left, right, ctx)?;
            Ok(equal(&left, &right))
        }
        Predicate::Ne { left, right } => {
            let (left, right) = operands(node, left, right, ctx)?;
            Ok(!equal(&left, &right))
        }
        Predicate::Lt { left, right } => order(node, left, right, ctx, |o| o < 0),
        Predicate::Le { left, right } => order(node, left, right, ctx, |o| o <= 0),
        Predicate::Gt { left, right } => order(node, left, right, ctx, |o| o > 0),
        Predicate::Ge { left, right } => order(node, left, right, ctx, |o| o >= 0),
        Predicate::In { left, right } => {
            let (left, right) = operands(node, left, right, ctx)?;
            let Some(items) = right.as_array() else {
                return Err(NodeError::Predicate {
                    node: node.to_owned(),
                    detail: "`in` needs an array on the right".to_owned(),
                });
            };
            Ok(items.iter().any(|item| equal(&left, item)))
        }
        Predicate::Empty { left } => {
            let value = operand(node, left, ctx)?;
            Ok(is_empty(&value))
        }
        Predicate::Exists { left } => {
            let value = operand(node, left, ctx)?;
            Ok(!value.is_null())
        }
        Predicate::And { of } => {
            for child in of {
                if !evaluate_at(node, child, ctx)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Predicate::Or { of } => {
            for child in of {
                if evaluate_at(node, child, ctx)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Predicate::Not { of } => Ok(!evaluate_at(node, of, ctx)?),
    }
}

fn operands(
    node: &str,
    left: &Value,
    right: &Value,
    ctx: &TemplateCtx,
) -> Result<(Value, Value), NodeError> {
    Ok((operand(node, left, ctx)?, operand(node, right, ctx)?))
}

/// Resolve one operand. An unknown path is `null`, so `empty`/`exists` can test absence — a template
/// that is merely unknown is the normal way to ask "is this set?".
fn operand(node: &str, value: &Value, ctx: &TemplateCtx) -> Result<Value, NodeError> {
    match template::resolve(value, ctx) {
        Ok(resolved) => Ok(resolved),
        Err(TemplateError::UnknownPath { .. }) => Ok(Value::Null),
        Err(error) => Err(NodeError::Template {
            node: node.to_owned(),
            detail: error.to_string(),
        }),
    }
}

/// JSON equality, with numbers compared by value (`1` equals `1.0`, which `Value` alone does not).
fn equal(left: &Value, right: &Value) -> bool {
    match (left.as_f64(), right.as_f64()) {
        (Some(left), Some(right)) => left == right,
        _ => left == right,
    }
}

fn order(
    node: &str,
    left: &Value,
    right: &Value,
    ctx: &TemplateCtx,
    accept: impl Fn(i8) -> bool,
) -> Result<bool, NodeError> {
    let (left, right) = operands(node, left, right, ctx)?;
    let ordering = match (&left, &right) {
        (Value::Number(_), Value::Number(_)) => {
            let (left, right) = (left.as_f64().unwrap_or(0.0), right.as_f64().unwrap_or(0.0));
            if left < right {
                -1
            } else if left > right {
                1
            } else {
                0
            }
        }
        (Value::String(left), Value::String(right)) => match left.cmp(right) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        },
        _ => {
            return Err(NodeError::Predicate {
                node: node.to_owned(),
                detail: format!(
                    "`{}` and `{}` are not both numbers or both strings",
                    json_type(&left),
                    json_type(&right)
                ),
            });
        }
    };
    Ok(accept(ordering))
}

fn is_empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.is_empty(),
        Value::Array(items) => items.is_empty(),
        _ => false,
    }
}

fn json_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

/// Resolve a `loop` node's `items` to an iteration count.
pub fn loop_items(node: &Node, ctx: &TemplateCtx) -> Result<u32, NodeError> {
    let template = node.items.as_ref().ok_or_else(|| NodeError::Missing {
        node: node.id.clone(),
        kind: "loop",
        field: "items",
    })?;
    let resolved = template::resolve(template, ctx).map_err(|error| NodeError::Template {
        node: node.id.clone(),
        detail: error.to_string(),
    })?;
    let Some(items) = resolved.as_array() else {
        return Err(NodeError::Items {
            node: node.id.clone(),
            found: json_type(&resolved),
        });
    };
    Ok(items.len().min(u32::MAX as usize) as u32)
}

/// The timeout a run applies, refusing a declaration above the cap.
///
/// Publish refuses the same value; this is the run-time twin, because a stored document is not a
/// promise that the validator that wrote it is the one running now.
pub fn timeout_ms(node: &Node) -> Result<i64, NodeError> {
    let declared = node.timeout_ms();
    if !(1..=MAX_TIMEOUT_MS).contains(&declared) {
        return Err(NodeError::Timeout {
            node: node.id.clone(),
            declared,
            max: MAX_TIMEOUT_MS,
        });
    }
    Ok(declared)
}
