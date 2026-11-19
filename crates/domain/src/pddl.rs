#![forbid(unsafe_code)]
//! The STRIPS subset a procedure step's action space is written in (T16, SPEC_V2 §5.6, D53, D5).
//!
//! **Two spaces, never mixed.** This module is the *action* space only: a step's PDDL action text
//! (stored by `process_step.action_text`) and its precondition/effect literals. The procedural
//! knowledge (the ontology) is not here and is not stored by conveyor — owner decision 2026-11-19,
//! knowledge lives in `strata_rag`/`strata_ingest` (#413).
//!
//! **The subset.** Ground literals only: `predicate`, `predicate(arg, …)`, optionally prefixed
//! `not `. No variables, no quantifiers, no numeric fluents, no planner. A fact state is the sorted
//! set of the non-negated atoms that currently hold; a precondition is checked against it and an
//! effect adds and deletes atoms. Everything is deterministic and reproducible from the row.
//!
//! The domain text a converter produced is stored for a person to read; the executor reads only the
//! literal arrays this module validates, so a malformed domain cannot reach a run.

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A literal is bounded: a predicate call longer than this is a corrupted document, not a fact.
pub const MAX_LITERAL_CHARS: usize = 512;

/// What went wrong with a PDDL value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PddlError {
    /// The input field the refusal names.
    pub field: &'static str,
    /// What is wrong, in the caller's vocabulary.
    pub detail: String,
}

impl PddlError {
    fn new(field: &'static str, detail: impl Into<String>) -> Self {
        Self {
            field,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for PddlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.detail)
    }
}

/// One ground literal: an atom, optionally negated.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Literal {
    /// `true` for `not <atom>`.
    pub negated: bool,
    /// The canonical atom (`predicate` or `predicate(arg, arg)`).
    pub atom: String,
}

/// An effect's add and delete lists, as stored in `process_step.pddl_effects`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Effects {
    /// Atoms that start holding.
    #[serde(default)]
    pub add: Vec<String>,
    /// Atoms that stop holding.
    #[serde(default)]
    pub delete: Vec<String>,
}

impl Effects {
    /// The wire shape the column stores.
    pub fn to_json(&self) -> Value {
        json!({ "add": self.add, "delete": self.delete })
    }

    /// Whether any effect is declared.
    pub fn is_empty(&self) -> bool {
        self.add.is_empty() && self.delete.is_empty()
    }
}

/// Parse one literal string into its canonical form.
///
/// Accepted: `predicate`, `predicate(a, b)`, `not predicate`, `not predicate(a, b)`. Predicate
/// names are `[a-z][a-z0-9_]*`; arguments are non-empty tokens of letters, digits, `_`, `-`, `.`
/// or `:` (no spaces inside a token; spaces around the commas are allowed and normalised).
pub fn parse_literal(field: &'static str, raw: &str) -> Result<Literal, PddlError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(PddlError::new(field, "an empty literal is not a fact"));
    }
    if trimmed.len() > MAX_LITERAL_CHARS {
        return Err(PddlError::new(
            field,
            format!("a literal longer than {MAX_LITERAL_CHARS} characters is not a fact"),
        ));
    }
    let (negated, body) = match trimmed.strip_prefix("not ") {
        Some(rest) => (true, rest.trim()),
        None => (false, trimmed),
    };
    if body.is_empty() {
        return Err(PddlError::new(field, "`not` with no atom"));
    }
    let atom = canonical_atom(field, body)?;
    Ok(Literal { negated, atom })
}

/// Canonicalise one atom (`predicate` or `predicate(args)`).
fn canonical_atom(field: &'static str, body: &str) -> Result<String, PddlError> {
    let body = body.trim();
    let (predicate, args) = match body.split_once('(') {
        Some((predicate, rest)) => {
            let rest = rest.strip_suffix(')').ok_or_else(|| {
                PddlError::new(field, format!("`{body}` is missing its closing `)`"))
            })?;
            if rest.contains('(') {
                return Err(PddlError::new(
                    field,
                    format!("`{body}` nests a second `(`"),
                ));
            }
            (predicate.trim(), Some(rest))
        }
        None => (body, None),
    };
    if !is_predicate(predicate) {
        return Err(PddlError::new(
            field,
            format!("`{predicate}` is not a predicate name ([a-z][a-z0-9_]*)"),
        ));
    }
    // `not` is the negation keyword, never a predicate: `not ` (trimmed to `not`) is a negation
    // with no atom, and `not(a)` is a negation misspelled as a call.
    if predicate == "not" {
        return Err(PddlError::new(
            field,
            "`not` negates an atom and is never itself one",
        ));
    }
    match args {
        None => Ok(predicate.to_owned()),
        Some(args) => {
            let mut parsed = Vec::new();
            for part in args.split(',') {
                let arg = part.trim();
                if !is_arg(arg) {
                    return Err(PddlError::new(
                        field,
                        format!("`{arg}` is not an argument token"),
                    ));
                }
                parsed.push(arg);
            }
            if parsed.is_empty() {
                return Err(PddlError::new(field, "an empty argument list"));
            }
            Ok(format!("{predicate}({})", parsed.join(",")))
        }
    }
}

fn is_predicate(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn is_arg(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':')
        })
}

/// Parse a precondition array (`["a", "not b"]`).
pub fn parse_preconditions(field: &'static str, value: &Value) -> Result<Vec<Literal>, PddlError> {
    let Some(items) = value.as_array() else {
        return Err(PddlError::new(field, "must be an array of literals"));
    };
    items
        .iter()
        .map(|item| {
            let Some(raw) = item.as_str() else {
                return Err(PddlError::new(field, "every precondition must be a string"));
            };
            parse_literal(field, raw)
        })
        .collect()
}

/// Parse an effect object (`{"add": [...], "delete": [...]}`) into canonical atoms.
pub fn parse_effects(field: &'static str, value: &Value) -> Result<Effects, PddlError> {
    let Some(object) = value.as_object() else {
        return Err(PddlError::new(
            field,
            "must be an object with `add` and/or `delete`",
        ));
    };
    let mut effects = Effects::default();
    for (key, target) in [
        ("add", &mut effects.add),
        ("delete", &mut effects.delete),
    ] {
        let Some(list) = object.get(key) else {
            continue;
        };
        let Some(items) = list.as_array() else {
            return Err(PddlError::new(field, format!("`{key}` must be an array")));
        };
        for item in items {
            let Some(raw) = item.as_str() else {
                return Err(PddlError::new(field, "every effect must be a string"));
            };
            // An effect is an atom, never a negated literal: `delete` is the negation.
            let literal = parse_literal(field, raw)?;
            if literal.negated {
                return Err(PddlError::new(
                    field,
                    format!("`{raw}` is negated; put it in `delete`, never `not` it in `add`"),
                ));
            }
            target.push(literal.atom);
        }
    }
    for key in object.keys() {
        if key != "add" && key != "delete" {
            return Err(PddlError::new(field, format!("unknown effect key `{key}`")));
        }
    }
    effects.add.sort();
    effects.add.dedup();
    effects.delete.sort();
    effects.delete.dedup();
    Ok(effects)
}

/// Parse a fact state (the JSON `["atom", …]` shape `process.pddl_init` and `run_step` store).
pub fn parse_state(field: &'static str, value: &Value) -> Result<Vec<String>, PddlError> {
    let Some(items) = value.as_array() else {
        return Err(PddlError::new(field, "must be an array of atoms"));
    };
    let mut state = Vec::with_capacity(items.len());
    for item in items {
        let Some(raw) = item.as_str() else {
            return Err(PddlError::new(field, "every fact must be a string"));
        };
        let literal = parse_literal(field, raw)?;
        if literal.negated {
            return Err(PddlError::new(
                field,
                format!("`{raw}` is negated; a state holds atoms, not negations"),
            ));
        }
        state.push(literal.atom);
    }
    state.sort();
    state.dedup();
    Ok(state)
}

/// Does one literal hold in `state`?
pub fn holds(state: &[String], literal: &Literal) -> bool {
    let present = state.iter().any(|atom| atom == &literal.atom);
    present != literal.negated
}

/// Check every precondition; `Err` names the first literal that does not hold.
pub fn check_preconditions(state: &[String], preconditions: &[Literal]) -> Result<(), String> {
    for literal in preconditions {
        if !holds(state, literal) {
            return Err(format!(
                "the precondition `{}` does not hold",
                literal_text(literal)
            ));
        }
    }
    Ok(())
}

/// Apply an effect to a state: adds first, then deletes; the result is sorted and deduplicated.
pub fn apply_effects(state: &[String], effects: &Effects) -> Vec<String> {
    let mut next: Vec<String> = state.to_vec();
    for atom in &effects.add {
        if !next.iter().any(|existing| existing == atom) {
            next.push(atom.clone());
        }
    }
    next.retain(|atom| !effects.delete.iter().any(|gone| gone == atom));
    next.sort();
    next.dedup();
    next
}

/// The printable form of a literal (`not parked(truck-1)`).
pub fn literal_text(literal: &Literal) -> String {
    if literal.negated {
        format!("not {}", literal.atom)
    } else {
        literal.atom.clone()
    }
}

/// The wire form of a parsed state.
pub fn state_json(state: &[String]) -> Value {
    json!(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_are_canonicalised_and_negation_is_understood() {
        let literal = parse_literal("pddl_preconditions", "  inspect( dock-1 , truck-2 ) ").unwrap();
        assert_eq!(literal.atom, "inspect(dock-1,truck-2)");
        assert!(!literal.negated);
        let negated = parse_literal("pddl_preconditions", "not blocked").unwrap();
        assert!(negated.negated);
        assert_eq!(negated.atom, "blocked");
    }

    #[test]
    fn malformed_literals_are_refused_by_name() {
        for raw in ["", "not ", "Inspect()", "inspect(", "in spect", "inspect(a,,b)"] {
            assert!(
                parse_literal("pddl_preconditions", raw).is_err(),
                "{raw:?} must be refused"
            );
        }
    }

    #[test]
    fn a_precondition_is_checked_against_the_state() {
        let state = parse_state("state", &json!(["dock_free(dock-1)"])).unwrap();
        let ok = parse_preconditions("p", &json!(["dock_free(dock-1)", "not dock_busy(dock-1)"]))
            .unwrap();
        assert!(check_preconditions(&state, &ok).is_ok());
        let refused = parse_preconditions("p", &json!(["dock_busy(dock-1)"])).unwrap();
        assert!(check_preconditions(&state, &refused).is_err());
    }

    #[test]
    fn effects_add_then_delete() {
        let state = parse_state("state", &json!(["a", "b"])).unwrap();
        let effects = parse_effects(
            "e",
            &json!({"add": ["c", "a"], "delete": ["b"]}),
        )
        .unwrap();
        assert_eq!(apply_effects(&state, &effects), vec!["a", "c"]);
    }

    #[test]
    fn a_negated_effect_is_refused() {
        let error = parse_effects("e", &json!({"add": ["not a"]})).unwrap_err();
        assert!(error.detail.contains("delete"));
    }

    #[test]
    fn an_unknown_effect_key_is_refused() {
        let error = parse_effects("e", &json!({"remove": ["a"]})).unwrap_err();
        assert!(error.detail.contains("remove"));
    }
}
