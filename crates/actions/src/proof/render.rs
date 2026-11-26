#![forbid(unsafe_code)]
//! The facts renderer — one pure function from canonical facts to the Lean source a certificate is
//! built from.
//!
//! Why it exists: a certificate is only worth what its source says. If a caller could write the
//! source, a certificate would prove whatever the caller liked; if the engine wrote it any way it
//! pleased, "re-render and compare" would be advisory. So the source is a **pure function of the
//! facts**, and `proofs.record`/`proofs.reverify` refuse a source that is not this function's
//! output for those facts (`proof_source_not_rendered`, in `mod.rs`).
//!
//! The model text is `include_str!`ed from `docs/spec/lean/DeliveryModel.lean`, so the bytes a
//! certificate carries and the bytes the corpus test checks are the same bytes. The model's digest
//! travels in every facts document; a facts document naming a different model is refused rather
//! than rendered, because a certificate that mixed two models would mean nothing.
//!
//! Determinism: no clock, no map iteration, every list sorted before emission, and the facts are
//! read by field name. Two renders of the same document are byte-identical — the test
//! `the_same_facts_render_byte_identical` is what keeps that true.

use std::fmt::Write as _;

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// The hand-written model, embedded verbatim at the head of every rendered source.
pub const MODEL_SOURCE: &str = include_str!("../../../../docs/spec/lean/DeliveryModel.lean");

/// The digest a facts document must name in `model_sha256`.
pub fn model_sha256() -> String {
    sha256_hex(MODEL_SOURCE.as_bytes())
}

/// Lower-case hex SHA-256, the spelling every digest column in this corpus uses.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut text = String::with_capacity(64);
    for byte in digest {
        let _ = write!(text, "{byte:02x}");
    }
    text
}

/// Why a facts document could not be rendered.
///
/// Each variant becomes a typed `422` in `mod.rs`; the slug matters because a gate or an operator
/// acts on the difference between "you named the wrong model" and "your facts are incomplete".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderError {
    /// `kind` is not `slice`, `programme` or `package`.
    UnknownKind(String),
    /// The document is not a JSON object.
    NotAnObject,
    /// A required field is absent.
    Missing(String),
    /// A field has the wrong JSON type; the string names the field and what was expected.
    NotA(String),
    /// A string is not printable ASCII, or names a native reduction the kernel does not back.
    Forbidden(String),
    /// `model_sha256` names a model the binary does not embed.
    ModelMismatch { named: String, embedded: String },
    /// The facts are well formed but do not satisfy the model; the string names the item.
    Incomplete(String),
    /// The theorem name is not a Lean identifier.
    TheoremName(String),
}

/// Render `facts` into the Lean source whose theorem `theorem` closes.
pub fn render(facts: &Value, theorem: &str) -> Result<String, RenderError> {
    if !is_theorem_name(theorem) {
        return Err(RenderError::TheoremName(theorem.to_owned()));
    }
    let object = facts.as_object().ok_or(RenderError::NotAnObject)?;
    let embedded = model_sha256();
    let named = text(object, "model_sha256")?;
    if named != embedded {
        return Err(RenderError::ModelMismatch {
            named: named.to_owned(),
            embedded,
        });
    }
    let spec = digest(object, "spec_sha256")?;
    let kind = text(object, "kind")?;

    let mut body = String::new();
    let (structure, checker) = match kind {
        "slice" => {
            let state = text(object, "state")?;
            let state = lean_state(state)?;
            let required = sorted(object, "required")?;
            let present = sorted(object, "present")?;
            if state != ".closed" {
                return Err(RenderError::Incomplete(
                    "\"state\": the slice is not closed".to_owned(),
                ));
            }
            if let Some(item) = required.iter().find(|item| !present.contains(item)) {
                return Err(RenderError::Incomplete(format!(
                    "required evidence {item:?}"
                )));
            }
            push_field(&mut body, "specSha256", &quoted(spec), true);
            push_field(&mut body, "modelSha256", &quoted(&embedded), true);
            push_field(&mut body, "state", state, true);
            push_field(&mut body, "required", &lean_list(&required), true);
            push_field(&mut body, "present", &lean_list(&present), false);
            ("SliceFacts", "checkSlice")
        }
        "programme" => {
            let expected = sorted(object, "expected")?;
            let leaves = sorted_leaves(object, "leaves")?;
            let names: Vec<String> = leaves.iter().map(|(name, _)| name.clone()).collect();
            let digests: Vec<String> = {
                let mut digests: Vec<String> =
                    leaves.iter().map(|(_, digest)| digest.clone()).collect();
                digests.sort();
                digests
            };
            if let Some(duplicate) = first_duplicate(&names) {
                return Err(RenderError::Incomplete(format!(
                    "leaf {duplicate:?} is present more than once"
                )));
            }
            if let Some(item) = expected.iter().find(|item| !names.contains(item)) {
                return Err(RenderError::Incomplete(format!("leaf {item:?} is missing")));
            }
            if let Some(item) = names.iter().find(|item| !expected.contains(item)) {
                return Err(RenderError::Incomplete(format!(
                    "leaf {item:?} is not expected"
                )));
            }
            if let Some(duplicate) = first_duplicate(&digests) {
                return Err(RenderError::Incomplete(format!(
                    "certificate {duplicate:?} backs more than one leaf"
                )));
            }
            push_field(&mut body, "specSha256", &quoted(spec), true);
            push_field(&mut body, "modelSha256", &quoted(&embedded), true);
            push_field(&mut body, "expected", &lean_list(&expected), true);
            push_field(&mut body, "leaves", &lean_leaves(&leaves), false);
            ("ProgrammeFacts", "checkProgramme")
        }
        "package" => {
            let migrations = sorted(object, "migrations")?;
            let ledger = sorted(object, "ledger")?;
            let routes = sorted(object, "routes")?;
            let served = sorted(object, "served")?;
            let checks = sorted_checks(object, "checks")?;
            if let Some(item) = migrations.iter().find(|item| !ledger.contains(item)) {
                return Err(RenderError::Incomplete(format!(
                    "migration {item:?} is not in the ledger"
                )));
            }
            if let Some(item) = ledger.iter().find(|item| !migrations.contains(item)) {
                return Err(RenderError::Incomplete(format!(
                    "ledger entry {item:?} has no migration"
                )));
            }
            if let Some(route) = routes.iter().find(|route| !served.contains(route)) {
                return Err(RenderError::Incomplete(format!(
                    "route {route:?} is not in the served document"
                )));
            }
            if let Some((name, _)) = checks.iter().find(|(_, passed)| !passed) {
                return Err(RenderError::Incomplete(format!(
                    "check {name:?} did not pass"
                )));
            }
            push_field(&mut body, "specSha256", &quoted(spec), true);
            push_field(&mut body, "modelSha256", &quoted(&embedded), true);
            push_field(&mut body, "migrations", &lean_list(&migrations), true);
            push_field(&mut body, "ledger", &lean_list(&ledger), true);
            push_field(&mut body, "routes", &lean_list(&routes), true);
            push_field(&mut body, "served", &lean_list(&served), true);
            push_field(&mut body, "checks", &lean_checks(&checks), false);
            ("PackageFacts", "checkPackage")
        }
        other => return Err(RenderError::UnknownKind(other.to_owned())),
    };

    Ok(format!(
        "{MODEL_SOURCE}\ndef facts : {structure} := {{\n{body}}}\n\n\
         theorem {theorem} : {checker} facts = true := by decide\n"
    ))
}

/// Whether `name` is a Lean identifier the emitted `theorem {name}` can carry.
pub fn is_theorem_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '\'')
}

fn push_field(out: &mut String, name: &str, value: &str, comma: bool) {
    let _ = write!(out, "  {name} := {value}");
    if comma {
        out.push(',');
    }
    out.push('\n');
}

/// A JSON string field, validated as safe to embed in a Lean string literal.
fn text<'a>(object: &'a Map<String, Value>, field: &str) -> Result<&'a str, RenderError> {
    let value = object
        .get(field)
        .ok_or_else(|| RenderError::Missing(field.to_owned()))?
        .as_str()
        .ok_or_else(|| RenderError::NotA(format!("{field} is not a string")))?;
    check_text(field, value)?;
    Ok(value)
}

/// A SHA-256 field: 64 lower-case hex characters, as the certificate column's CHECK wants it.
fn digest<'a>(object: &'a Map<String, Value>, field: &str) -> Result<&'a str, RenderError> {
    let value = text(object, field)?;
    let ok = value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if !ok {
        return Err(RenderError::NotA(format!(
            "{field} is not a lower-case SHA-256"
        )));
    }
    Ok(value)
}

/// Whether a string is safe to embed: printable ASCII, no quote, no backslash, no native name.
///
/// This is what makes injection impossible. Byte 0x5C (backslash) is refused because Lean reads
/// it as an escape, and a quote would close the literal; with both gone, the only bytes that can
/// appear inside the quotes are the printable ASCII that Lean reads literally.
fn check_text(field: &str, value: &str) -> Result<(), RenderError> {
    if value.contains("native_decide") || value.contains("+native") {
        return Err(RenderError::Forbidden(format!(
            "{field}: names a native reduction, which the kernel does not check"
        )));
    }
    let unsafe_byte = value
        .bytes()
        .any(|byte| !(0x20..=0x7E).contains(&byte) || byte == b'"' || byte == b'\\');
    if unsafe_byte {
        return Err(RenderError::Forbidden(format!(
            "{field}: must be printable ASCII without a quote or a backslash"
        )));
    }
    Ok(())
}

/// A list of strings, sorted so the emitted source is canonical.
fn sorted(object: &Map<String, Value>, field: &str) -> Result<Vec<String>, RenderError> {
    let items = object
        .get(field)
        .ok_or_else(|| RenderError::Missing(field.to_owned()))?
        .as_array()
        .ok_or_else(|| RenderError::NotA(format!("{field} is not a list")))?;
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let value = item
            .as_str()
            .ok_or_else(|| RenderError::NotA(format!("{field}[{index}] is not a string")))?;
        check_text(field, value)?;
        out.push(value.to_owned());
    }
    out.sort();
    Ok(out)
}

/// A list of `[name, passed]` pairs, sorted by name. Duplicates are kept: the model refuses them,
/// so a renderer that deduplicated would hide a defect the prover is meant to catch.
fn sorted_checks(
    object: &Map<String, Value>,
    field: &str,
) -> Result<Vec<(String, bool)>, RenderError> {
    let items = object
        .get(field)
        .ok_or_else(|| RenderError::Missing(field.to_owned()))?
        .as_array()
        .ok_or_else(|| RenderError::NotA(format!("{field} is not a list")))?;
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let pair = item
            .as_array()
            .filter(|pair| pair.len() == 2)
            .ok_or_else(|| {
                RenderError::NotA(format!("{field}[{index}] is not a [name, passed] pair"))
            })?;
        let name = pair[0]
            .as_str()
            .ok_or_else(|| RenderError::NotA(format!("{field}[{index}][0] is not a string")))?;
        check_text(field, name)?;
        let passed = pair[1]
            .as_bool()
            .ok_or_else(|| RenderError::NotA(format!("{field}[{index}][1] is not a boolean")))?;
        out.push((name.to_owned(), passed));
    }
    out.sort();
    Ok(out)
}

/// A list of `[slice, certificate digest]` leaves, sorted by slice, digests validated.
fn sorted_leaves(
    object: &Map<String, Value>,
    field: &str,
) -> Result<Vec<(String, String)>, RenderError> {
    let items = object
        .get(field)
        .ok_or_else(|| RenderError::Missing(field.to_owned()))?
        .as_array()
        .ok_or_else(|| RenderError::NotA(format!("{field} is not a list")))?;
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let pair = item
            .as_array()
            .filter(|pair| pair.len() == 2)
            .ok_or_else(|| {
                RenderError::NotA(format!("{field}[{index}] is not a [slice, digest] pair"))
            })?;
        let name = pair[0]
            .as_str()
            .ok_or_else(|| RenderError::NotA(format!("{field}[{index}][0] is not a string")))?;
        check_text(field, name)?;
        let digest = pair[1]
            .as_str()
            .ok_or_else(|| RenderError::NotA(format!("{field}[{index}][1] is not a string")))?;
        if !is_sha256(digest) {
            return Err(RenderError::NotA(format!(
                "{field}[{index}][1] is not a lower-case SHA-256"
            )));
        }
        out.push((name.to_owned(), digest.to_owned()));
    }
    out.sort();
    Ok(out)
}

fn first_duplicate(items: &[String]) -> Option<&str> {
    items
        .windows(2)
        .find(|pair| pair[0] == pair[1])
        .map(|pair| pair[0].as_str())
}

/// The Lean constructor for a slice state.
fn lean_state(state: &str) -> Result<&'static str, RenderError> {
    Ok(match state {
        "planned" => ".planned",
        "ready" => ".ready",
        "inProgress" => ".inProgress",
        "blocked" => ".blocked",
        "closed" => ".closed",
        other => {
            return Err(RenderError::NotA(format!(
                "state {other:?} is not one of planned, ready, inProgress, blocked, closed"
            )));
        }
    })
}

fn quoted(value: &str) -> String {
    format!("\"{value}\"")
}

fn lean_list(items: &[String]) -> String {
    let inner: Vec<String> = items.iter().map(|item| quoted(item)).collect();
    format!("[{}]", inner.join(", "))
}

fn lean_checks(items: &[(String, bool)]) -> String {
    let inner: Vec<String> = items
        .iter()
        .map(|(name, passed)| format!("({}, {})", quoted(name), passed))
        .collect();
    format!("[{}]", inner.join(", "))
}

/// A SHA-256 as the certificate columns spell it: 64 lower-case hex characters.
pub fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn lean_leaves(items: &[(String, String)]) -> String {
    let inner: Vec<String> = items
        .iter()
        .map(|(name, digest)| format!("({}, {})", quoted(name), quoted(digest)))
        .collect();
    format!("[{}]", inner.join(", "))
}
