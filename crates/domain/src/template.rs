#![forbid(unsafe_code)]
//! `{{ path }}` substitution — the ONE resolver a process step uses (S11 §5.4).
//!
//! Deliberately not a language. There are no filters, no conditionals, no truthiness and no functions: a
//! process's step order IS its control flow (S11 trap: "PDL creeping back"). What is legal:
//!
//! * `{{ run.inputs.ticket }}` — a reference; optional spaces inside the braces.
//! * A string that is **only** a reference is replaced by the value **with its JSON type preserved**.
//! * A reference embedded in text interpolates **scalars only**; an object or array is a refusal, never a
//!   `[object Object]`.
//! * `\{{` emits a literal `{{`. No other escape exists.
//! * A path that exists and holds `null` resolves to `null`. Absent is an error; null is a value.
//!
//! Roots: `run`, `steps`, `card`, `process`. `steps.<n>` addresses an earlier step's output and refuses
//! `n >= position` — a step cannot read a step that has not run, which is also what makes the trace linear.

use std::fmt;

use serde_json::{Map, Value};

/// The values a template may address.
#[derive(Debug, Clone)]
pub struct TemplateCtx {
    /// `run.id`, `run.card_id`, `run.project_id`, `run.correlation_id`, `run.inputs.<path>`.
    pub run: Value,
    /// One entry per **earlier** step, 0-based: entry `n-1` is `steps.<n>.output`.
    pub steps: Vec<StepOutcome>,
    /// `card.id`, `card.name`, … `card.meta.<path>`.
    pub card: Value,
    /// `board.lists.<kind>` — the card's board lanes by kind (`intake`, `active`, …), each a lane
    /// uuid. The one way a computed step can name a destination lane without hardcoding an id
    /// (`cards.move` takes a lane uuid). Empty for a workflow, which has no board.
    pub board: Value,
    /// `process.key`, `process.version`.
    pub process: Value,
    /// The position of the step being resolved (1-based). `steps.<n>` requires `n < position`.
    pub position: usize,
}

/// One earlier step's recorded outcome.
#[derive(Debug, Clone)]
pub struct StepOutcome {
    /// The value the action returned (the `run_step.output` row).
    pub output: Value,
}

/// Why a template could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateError {
    /// A `{{` with no closing `}}`.
    Unterminated(String),
    /// The root is not one of `run`, `steps`, `card`, `process`.
    UnknownRoot { pointer: String, root: String },
    /// The path does not exist in the context.
    UnknownPath { pointer: String },
    /// A `steps.<n>` reference to the current or a later step.
    ForwardReference {
        pointer: String,
        referenced: usize,
        position: usize,
    },
    /// An embedded (mid-text) reference resolved to an object or an array.
    NotScalar { pointer: String },
    /// An index that is not a number where a step index is required, or out of range.
    BadIndex { pointer: String },
}

impl TemplateError {
    /// The JSON pointer of the offending template node, for the problem+json `errors[]` member.
    pub fn pointer(&self) -> &str {
        match self {
            Self::Unterminated(pointer)
            | Self::UnknownPath { pointer }
            | Self::NotScalar { pointer }
            | Self::BadIndex { pointer } => pointer,
            Self::UnknownRoot { pointer, .. } => pointer,
            Self::ForwardReference { pointer, .. } => pointer,
        }
    }
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unterminated(p) => write!(f, "{p}: an opening `{{{{` has no closing `}}}}`"),
            Self::UnknownRoot { pointer, root } => write!(
                f,
                "{pointer}: `{root}` is not a template root (run, steps, card, process)"
            ),
            Self::UnknownPath { pointer } => write!(f, "{pointer}: the path does not exist"),
            Self::ForwardReference {
                pointer,
                referenced,
                position,
            } => write!(
                f,
                "{pointer}: step {referenced} has not run yet (this is step {position})"
            ),
            Self::NotScalar { pointer } => {
                write!(f, "{pointer}: an embedded reference must be a scalar")
            }
            Self::BadIndex { pointer } => write!(f, "{pointer}: not a legal step index"),
        }
    }
}

impl std::error::Error for TemplateError {}

/// Resolve every template value in `template`.
pub fn resolve(template: &Value, ctx: &TemplateCtx) -> Result<Value, TemplateError> {
    resolve_at(template, ctx, "$")
}

fn resolve_at(template: &Value, ctx: &TemplateCtx, pointer: &str) -> Result<Value, TemplateError> {
    match template {
        Value::String(text) => resolve_text(text, ctx, pointer),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (index, item) in items.iter().enumerate() {
                out.push(resolve_at(item, ctx, &format!("{pointer}/{index}"))?);
            }
            Ok(Value::Array(out))
        }
        Value::Object(map) => {
            let mut out = Map::with_capacity(map.len());
            for (key, value) in map {
                out.insert(
                    key.clone(),
                    resolve_at(value, ctx, &format!("{pointer}/{}", escape(key)))?,
                );
            }
            Ok(Value::Object(out))
        }
        other => Ok(other.clone()),
    }
}

/// One piece of a template string.
enum Piece<'a> {
    /// Literal text, already unescaped.
    Text(String),
    /// A reference's path, trimmed.
    Reference(&'a str),
}

/// Split a template string into literal text and references.
///
/// `\{{` becomes a literal `{{` in the surrounding text and never starts a reference.
fn pieces<'a>(text: &'a str, pointer: &str) -> Result<Vec<Piece<'a>>, TemplateError> {
    let mut out = Vec::new();
    let mut literal = String::new();
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let rest = &text[index..];
        if rest.starts_with("\\{{") {
            literal.push_str("{{");
            index += 3; // consume the backslash and both braces
            continue;
        }
        if let Some(after) = rest.strip_prefix("{{") {
            let Some(end) = after.find("}}") else {
                return Err(TemplateError::Unterminated(pointer.to_owned()));
            };
            let path = after[..end].trim();
            if path.is_empty() {
                return Err(TemplateError::UnknownPath {
                    pointer: pointer.to_owned(),
                });
            }
            if !literal.is_empty() {
                out.push(Piece::Text(std::mem::take(&mut literal)));
            }
            out.push(Piece::Reference(path));
            index = text.len() - after.len() + end + 2;
            continue;
        }
        let ch = rest.chars().next().expect("a non-empty rest has a char");
        literal.push(ch);
        index += ch.len_utf8();
    }
    if !literal.is_empty() {
        out.push(Piece::Text(literal));
    }
    Ok(out)
}

fn resolve_text(text: &str, ctx: &TemplateCtx, pointer: &str) -> Result<Value, TemplateError> {
    let parts = pieces(text, pointer)?;
    if parts.is_empty() {
        return Ok(Value::String(String::new()));
    }
    // A whole-string reference keeps the value's JSON type.
    if parts.len() == 1
        && let Piece::Reference(path) = &parts[0]
    {
        return lookup(path, ctx, pointer);
    }
    let mut out = String::with_capacity(text.len());
    for part in parts {
        match part {
            Piece::Text(literal) => out.push_str(&literal),
            Piece::Reference(path) => {
                let value = lookup(path, ctx, pointer)?;
                match value {
                    Value::String(s) => out.push_str(&s),
                    Value::Number(n) => out.push_str(&n.to_string()),
                    Value::Bool(b) => out.push_str(if b { "true" } else { "false" }),
                    Value::Null => {}
                    Value::Array(_) | Value::Object(_) => {
                        return Err(TemplateError::NotScalar {
                            pointer: pointer.to_owned(),
                        });
                    }
                }
            }
        }
    }
    Ok(Value::String(out))
}

/// Walk one dotted path against the four roots.
fn lookup(path: &str, ctx: &TemplateCtx, pointer: &str) -> Result<Value, TemplateError> {
    let mut segments = path.split('.');
    let root = segments.next().unwrap_or_default();
    let mut current = match root {
        "run" => ctx.run.clone(),
        "steps" => return lookup_step(segments, ctx, pointer),
        "card" => ctx.card.clone(),
        "board" => ctx.board.clone(),
        "process" => ctx.process.clone(),
        other => {
            return Err(TemplateError::UnknownRoot {
                pointer: pointer.to_owned(),
                root: other.to_owned(),
            });
        }
    };
    for segment in segments {
        current = descend(current, segment, pointer)?;
    }
    Ok(current)
}

/// `steps.<n>.output.<path>` — `n` is 1-based and must name an earlier step.
fn lookup_step<'a>(
    mut segments: impl Iterator<Item = &'a str>,
    ctx: &TemplateCtx,
    pointer: &str,
) -> Result<Value, TemplateError> {
    let Some(raw) = segments.next() else {
        return Err(TemplateError::UnknownPath {
            pointer: pointer.to_owned(),
        });
    };
    let Ok(index) = raw.parse::<usize>() else {
        return Err(TemplateError::BadIndex {
            pointer: pointer.to_owned(),
        });
    };
    if index == 0 || index >= ctx.position {
        return Err(TemplateError::ForwardReference {
            pointer: pointer.to_owned(),
            referenced: index,
            position: ctx.position,
        });
    }
    let Some(outcome) = ctx.steps.get(index - 1) else {
        return Err(TemplateError::UnknownPath {
            pointer: pointer.to_owned(),
        });
    };
    let mut current = outcome.output.clone();
    // `.output` is how the plan spells the reference; accept it as a no-op member.
    if let Some(first) = segments.next() {
        if first == "output" {
            // descend into the output's own members from here
        } else {
            current = descend(current, first, pointer)?;
        }
    }
    for segment in segments {
        current = descend(current, segment, pointer)?;
    }
    Ok(current)
}

fn descend(value: Value, segment: &str, pointer: &str) -> Result<Value, TemplateError> {
    match value {
        Value::Object(map) => map
            .get(segment)
            .cloned()
            .ok_or_else(|| TemplateError::UnknownPath {
                pointer: pointer.to_owned(),
            }),
        Value::Array(items) => {
            let Ok(index) = segment.parse::<usize>() else {
                return Err(TemplateError::BadIndex {
                    pointer: pointer.to_owned(),
                });
            };
            items
                .get(index)
                .cloned()
                .ok_or_else(|| TemplateError::UnknownPath {
                    pointer: pointer.to_owned(),
                })
        }
        _ => Err(TemplateError::UnknownPath {
            pointer: pointer.to_owned(),
        }),
    }
}

/// RFC 6901 pointer escaping for an object key.
fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// Every `{{ path }}` in a template, with the JSON pointer of the node it sits in.
///
/// Publish-time uses this without a runtime context: a process is validated when it is published, long before
/// there is a run to resolve against.
pub fn references(template: &Value) -> Result<Vec<(String, String)>, TemplateError> {
    let mut out = Vec::new();
    collect(template, "$", &mut out)?;
    Ok(out)
}

fn collect(
    template: &Value,
    pointer: &str,
    out: &mut Vec<(String, String)>,
) -> Result<(), TemplateError> {
    match template {
        Value::String(text) => {
            for piece in pieces(text, pointer)? {
                if let Piece::Reference(path) = piece {
                    out.push((pointer.to_owned(), path.to_owned()));
                }
            }
            Ok(())
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect(item, &format!("{pointer}/{index}"), out)?;
            }
            Ok(())
        }
        Value::Object(map) => {
            for (key, value) in map {
                collect(value, &format!("{pointer}/{}", escape(key)), out)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Check a template's references **statically**, at publish time: every root is legal and every `steps.<n>`
/// names an earlier step. It cannot check that a path exists — that is a run-time fact.
pub fn validate_template(template: &Value, position: usize) -> Result<(), TemplateError> {
    for (pointer, path) in references(template)? {
        let mut segments = path.split('.');
        let root = segments.next().unwrap_or_default();
        match root {
            "run" | "card" | "board" | "process" => {}
            "steps" => {
                let Some(raw) = segments.next() else {
                    return Err(TemplateError::UnknownPath { pointer });
                };
                let Ok(index) = raw.parse::<usize>() else {
                    return Err(TemplateError::BadIndex { pointer });
                };
                if index == 0 || index >= position {
                    return Err(TemplateError::ForwardReference {
                        pointer,
                        referenced: index,
                        position,
                    });
                }
            }
            other => {
                return Err(TemplateError::UnknownRoot {
                    pointer,
                    root: other.to_owned(),
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx() -> TemplateCtx {
        TemplateCtx {
            run: json!({
                "id": "run-1",
                "card_id": "card-1",
                "project_id": "project-1",
                "correlation_id": null,
                "inputs": {"ticket": "T-7", "count": 3, "flag": true, "gone": null,
                           "obj": {"a": 1}, "list": [1, 2]},
            }),
            steps: vec![StepOutcome {
                output: json!({"id": "card-1", "version": 4}),
            }],
            card: json!({"id": "card-1", "name": "Card", "version": 5, "meta": {"x": "y"}}),
            board: json!({"lists": {"intake": "lane-intake", "active": "lane-active"}}),
            process: json!({"key": "dev-phase", "version": 2}),
            position: 2,
        }
    }

    #[test]
    fn a_whole_string_reference_keeps_its_json_type() {
        let out = resolve(&json!({"n": "{{ run.inputs.count }}"}), &ctx()).expect("resolves");
        assert_eq!(out, json!({"n": 3}));
        assert!(out["n"].is_number(), "a number must not become a string");

        let out = resolve(&json!({"b": "{{run.inputs.flag}}"}), &ctx()).expect("resolves");
        assert_eq!(out["b"], json!(true));
    }

    #[test]
    fn an_embedded_reference_interpolates_scalars() {
        let out = resolve(
            &json!({"title": "run {{ run.id }} for {{card.name}}"}),
            &ctx(),
        )
        .expect("resolves");
        assert_eq!(out["title"], "run run-1 for Card");
    }

    #[test]
    fn a_backslash_escapes_the_braces() {
        let out = resolve(&json!({"t": "literal \\{{ run.id }} stays"}), &ctx()).expect("resolves");
        assert_eq!(out["t"], "literal {{ run.id }} stays");
    }

    #[test]
    fn a_null_is_a_value_and_an_absent_path_is_an_error() {
        let out = resolve(&json!("{{run.inputs.gone}}"), &ctx()).expect("resolves");
        assert_eq!(out, Value::Null);

        let error = resolve(&json!("{{run.inputs.nope}}"), &ctx()).expect_err("absent");
        assert!(
            matches!(error, TemplateError::UnknownPath { .. }),
            "{error}"
        );
    }

    #[test]
    fn a_step_may_read_an_earlier_step_only() {
        let out = resolve(&json!("{{steps.1.output.version}}"), &ctx()).expect("resolves");
        assert_eq!(out, json!(4));

        let error = resolve(&json!("{{steps.2.output.x}}"), &ctx()).expect_err("forward");
        assert!(
            matches!(
                error,
                TemplateError::ForwardReference {
                    referenced: 2,
                    position: 2,
                    ..
                }
            ),
            "{error}"
        );

        let error = resolve(&json!("{{steps.0.output.x}}"), &ctx()).expect_err("zero");
        assert!(
            matches!(error, TemplateError::ForwardReference { .. }),
            "{error}"
        );
    }

    #[test]
    fn an_unknown_root_names_itself() {
        let error = resolve(&json!("{{tenant.id}}"), &ctx()).expect_err("root");
        match error {
            TemplateError::UnknownRoot { root, .. } => assert_eq!(root, "tenant"),
            other => panic!("{other}"),
        }
    }

    #[test]
    fn the_board_lanes_resolve_by_kind() {
        // D21: a computed step names a destination lane as `{{board.lists.<kind>}}`; the board
        // context is a first-class root, not an unknown one.
        assert_eq!(
            resolve(&json!("{{board.lists.active}}"), &ctx()).expect("the active lane"),
            json!("lane-active")
        );
    }

    #[test]
    fn an_embedded_object_is_refused_rather_than_stringified() {
        let error = resolve(&json!("value {{run.inputs.obj}}"), &ctx()).expect_err("object");
        assert!(matches!(error, TemplateError::NotScalar { .. }), "{error}");
    }

    #[test]
    fn an_unterminated_reference_is_named() {
        let error = resolve(&json!({"t": "{{run.id"}), &ctx()).expect_err("unterminated");
        assert!(matches!(error, TemplateError::Unterminated(_)), "{error}");
        assert_eq!(
            error.pointer(),
            "$/t",
            "the pointer names the template node"
        );
    }

    #[test]
    fn objects_and_arrays_are_walked() {
        let out = resolve(
            &json!({"ids": ["{{card.id}}", "{{run.inputs.list.0}}"], "when": "{{process.version}}"}),
            &ctx(),
        )
        .expect("resolves");
        assert_eq!(out, json!({"ids": ["card-1", 1], "when": 2}));
    }
}
