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
    /// `kind` is not `slice`, `programme`, `package`, `unit_ledger` or `unit_record`.
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
        // The engine's own unit record is a single facts structure, like a slice: every field
        // is an observation and `none` fails closed, so an incomplete record renders and the
        // kernel refuses it rather than the renderer hiding the defect.
        "unit_record" => {
            let issue_ref = optional_text(object, "issue_ref")?;
            let repo = optional_text(object, "repo")?;
            let lane = optional_text(object, "lane")?;
            let pull_number = optional_nat(object, "pull_number")?;
            let pull_url = optional_text(object, "pull_url")?;
            let pull_head = optional_text(object, "pull_head")?;
            let ci_run_id = optional_text(object, "ci_run_id")?;
            let ci_sha = optional_text(object, "ci_sha")?;
            let ci_verdict = optional_text(object, "ci_verdict")?;
            let merge_sha = optional_text(object, "merge_sha")?;
            let deploy_digest = optional_text(object, "deploy_digest")?;
            let serving_digest = optional_text(object, "serving_digest")?;
            let live_health = optional_nat(object, "live_health")?;
            let live_acceptance_exit = optional_int(object, "live_acceptance_exit")?;
            let gate_review = optional_bool(object, "gate_review")?;
            let gate_merge = optional_bool(object, "gate_merge")?;
            let gate_deploy = optional_bool(object, "gate_deploy")?;
            push_field(&mut body, "specSha256", &quoted(spec), true);
            push_field(&mut body, "modelSha256", &quoted(&embedded), true);
            push_field(
                &mut body,
                "issueRef",
                &option_lean(issue_ref, |value| quoted(&value)),
                true,
            );
            push_field(
                &mut body,
                "repo",
                &option_lean(repo, |value| quoted(&value)),
                true,
            );
            push_field(
                &mut body,
                "lane",
                &option_lean(lane, |value| quoted(&value)),
                true,
            );
            push_field(
                &mut body,
                "pullNumber",
                &option_lean(pull_number, |value| value.to_string()),
                true,
            );
            push_field(
                &mut body,
                "pullUrl",
                &option_lean(pull_url, |value| quoted(&value)),
                true,
            );
            push_field(
                &mut body,
                "pullHead",
                &option_lean(pull_head, |value| quoted(&value)),
                true,
            );
            push_field(
                &mut body,
                "ciRunId",
                &option_lean(ci_run_id, |value| quoted(&value)),
                true,
            );
            push_field(
                &mut body,
                "ciSha",
                &option_lean(ci_sha, |value| quoted(&value)),
                true,
            );
            push_field(
                &mut body,
                "ciVerdict",
                &option_lean(ci_verdict, |value| quoted(&value)),
                true,
            );
            push_field(
                &mut body,
                "mergeSha",
                &option_lean(merge_sha, |value| quoted(&value)),
                true,
            );
            push_field(
                &mut body,
                "deployDigest",
                &option_lean(deploy_digest, |value| quoted(&value)),
                true,
            );
            push_field(
                &mut body,
                "servingDigest",
                &option_lean(serving_digest, |value| quoted(&value)),
                true,
            );
            push_field(
                &mut body,
                "liveHealth",
                &option_lean(live_health, |value| value.to_string()),
                true,
            );
            push_field(
                &mut body,
                "liveAcceptanceExit",
                &option_lean(live_acceptance_exit, |value| value.to_string()),
                true,
            );
            push_field(
                &mut body,
                "gateReview",
                &option_lean(gate_review, |value| value.to_string()),
                true,
            );
            push_field(
                &mut body,
                "gateMerge",
                &option_lean(gate_merge, |value| value.to_string()),
                true,
            );
            push_field(
                &mut body,
                "gateDeploy",
                &option_lean(gate_deploy, |value| value.to_string()),
                false,
            );
            ("UnitRecordFacts", "checkRecord")
        }
        // A ledger chunk is a list of observations, not a single facts record: it has its own
        // emitter and its own theorem pair. Failing units are deliberately *not* refused here —
        // the kernel is what refuses them, and the resulting unproven certificate is the point.
        "unit_ledger" => return render_unit_ledger(object, theorem),
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

/// An observation that may be absent: `null` is `none`, a string is `some`, anything else is a
/// type error. The renderer never defaults an absent observation to a passing value.
fn optional_text(object: &Map<String, Value>, field: &str) -> Result<Option<String>, RenderError> {
    match object.get(field) {
        None => Err(RenderError::Missing(field.to_owned())),
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => {
            check_text(field, value)?;
            Ok(Some(value.clone()))
        }
        Some(_) => Err(RenderError::NotA(format!(
            "{field} is not a string or null"
        ))),
    }
}

fn optional_nat(object: &Map<String, Value>, field: &str) -> Result<Option<u64>, RenderError> {
    match object.get(field) {
        None => Err(RenderError::Missing(field.to_owned())),
        Some(Value::Null) => Ok(None),
        Some(value) => value.as_u64().map(Some).ok_or_else(|| {
            RenderError::NotA(format!("{field} is not a non-negative integer or null"))
        }),
    }
}

fn optional_int(object: &Map<String, Value>, field: &str) -> Result<Option<i64>, RenderError> {
    match object.get(field) {
        None => Err(RenderError::Missing(field.to_owned())),
        Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_i64()
            .map(Some)
            .ok_or_else(|| RenderError::NotA(format!("{field} is not an integer or null"))),
    }
}

fn optional_bool(object: &Map<String, Value>, field: &str) -> Result<Option<bool>, RenderError> {
    match object.get(field) {
        None => Err(RenderError::Missing(field.to_owned())),
        Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_bool()
            .map(Some)
            .ok_or_else(|| RenderError::NotA(format!("{field} is not a boolean or null"))),
    }
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

// ── the unit ledger (`docs/plans/03-DELIVERY.md`, A0 measurement) ────────────────────────

/// The chunk the prover was measured at (`docs/evidence/proof-of-delivery.md` § Units). A failing
/// chunk is bisected at this size; a larger ledger is the caller's to chunk, never a source that
/// silently needs `maxRecDepth`.
const UNIT_LEDGER_MAX: usize = 200;

/// One observed issue state, as the facts document spells it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum IssueObs {
    Pending,
    Closed,
}

impl IssueObs {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "pending" => Some(Self::Pending),
            "closed" => Some(Self::Closed),
            _ => None,
        }
    }

    fn lean(self) -> &'static str {
        match self {
            Self::Pending => ".pending",
            Self::Closed => ".closed",
        }
    }
}

/// One observed PR state, as the facts document spells it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PullObs {
    Draft,
    Ready,
    Merged,
}

impl PullObs {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "draft" => Some(Self::Draft),
            "ready" => Some(Self::Ready),
            "merged" => Some(Self::Merged),
            _ => None,
        }
    }

    fn lean(self) -> &'static str {
        match self {
            Self::Draft => ".draft",
            Self::Ready => ".ready",
            Self::Merged => ".merged",
        }
    }
}

/// One unit's observed facts. Every field is an `Option`: the document must carry it (null when
/// unknown), and unknown fails closed in the model.
struct UnitEntry {
    key: String,
    issue: Option<IssueObs>,
    branches: Option<u64>,
    branch_deleted: Option<bool>,
    pulls: Option<u64>,
    pull: Option<PullObs>,
    ci_runs: Option<u64>,
    closes_keyword: Option<bool>,
    verify_parts: Option<bool>,
}

impl UnitEntry {
    fn one_branch(&self) -> bool {
        self.branches == Some(1)
    }

    fn one_pull(&self) -> bool {
        self.pulls == Some(1)
    }

    fn pull_usable(&self) -> bool {
        matches!(self.pull, Some(PullObs::Ready | PullObs::Merged))
    }

    fn ci_budget(&self) -> bool {
        self.ci_runs.is_some_and(|runs| runs <= 2)
    }

    fn merge_complete(&self) -> bool {
        match (self.pull, self.issue, self.branch_deleted) {
            (Some(PullObs::Merged), Some(IssueObs::Closed), Some(true)) => true,
            (Some(PullObs::Merged), _, _) => false,
            _ => true,
        }
    }

    fn closes_stated(&self) -> bool {
        self.closes_keyword == Some(true)
    }

    fn parts_verified(&self) -> bool {
        self.verify_parts == Some(true)
    }
}

/// Render a `unit_ledger` facts document: the model, the observations, the kernel-checked
/// per-rule outcomes, and the aggregate theorem.
fn render_unit_ledger(object: &Map<String, Value>, theorem: &str) -> Result<String, RenderError> {
    let items = object
        .get("units")
        .ok_or_else(|| RenderError::Missing("units".to_owned()))?
        .as_array()
        .ok_or_else(|| RenderError::NotA("units is not a list".to_owned()))?;
    if items.len() > UNIT_LEDGER_MAX {
        return Err(RenderError::Incomplete(format!(
            "the unit ledger holds {} units; a certificate chunk is at most {UNIT_LEDGER_MAX}",
            items.len()
        )));
    }
    let mut entries = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        entries.push(unit_entry(item, index)?);
    }
    entries.sort_by(|left, right| left.key.cmp(&right.key));
    if let Some(pair) = entries.windows(2).find(|pair| pair[0].key == pair[1].key) {
        return Err(RenderError::Incomplete(format!(
            "unit {:?} appears more than once",
            pair[0].key
        )));
    }

    let literals: Vec<String> = entries.iter().map(unit_literal).collect();
    let mut source = String::new();
    source.push_str(MODEL_SOURCE);
    source.push_str("\ndef facts : List UnitObs := [\n");
    source.push_str(&literals.join(",\n"));
    source.push_str("\n]\n\n");
    source.push_str(&format!(
        "theorem {theorem}_rules : ruleResults facts = {} := by decide\n\n",
        rules_literal(&entries)
    ));
    source.push_str(&format!(
        "theorem {theorem} : checkLedger facts = true := by decide\n"
    ));
    Ok(source)
}

fn unit_entry(item: &Value, index: usize) -> Result<UnitEntry, RenderError> {
    let object = item
        .as_object()
        .ok_or_else(|| RenderError::NotA(format!("units[{index}] is not an object")))?;
    let key = text(object, "key")?.to_owned();
    if key.is_empty() {
        return Err(RenderError::NotA(format!("units[{index}].key is empty")));
    }
    Ok(UnitEntry {
        key,
        issue: unit_issue(object, index)?,
        branches: unit_nat(object, index, "branches")?,
        branch_deleted: unit_bool(object, index, "branchDeleted")?,
        pulls: unit_nat(object, index, "pulls")?,
        pull: unit_pull(object, index)?,
        ci_runs: unit_nat(object, index, "ciRuns")?,
        closes_keyword: unit_bool(object, index, "closesKeyword")?,
        verify_parts: unit_bool(object, index, "verifyParts")?,
    })
}

fn unit_field<'a>(
    object: &'a Map<String, Value>,
    index: usize,
    field: &str,
) -> Result<&'a Value, RenderError> {
    object
        .get(field)
        .ok_or_else(|| RenderError::NotA(format!("units[{index}].{field} is missing")))
}

fn unit_bool(
    object: &Map<String, Value>,
    index: usize,
    field: &str,
) -> Result<Option<bool>, RenderError> {
    let value = unit_field(object, index, field)?;
    if value.is_null() {
        return Ok(None);
    }
    value.as_bool().map(Some).ok_or_else(|| {
        RenderError::NotA(format!("units[{index}].{field} is not a boolean or null"))
    })
}

fn unit_nat(
    object: &Map<String, Value>,
    index: usize,
    field: &str,
) -> Result<Option<u64>, RenderError> {
    let value = unit_field(object, index, field)?;
    if value.is_null() {
        return Ok(None);
    }
    value.as_u64().map(Some).ok_or_else(|| {
        RenderError::NotA(format!(
            "units[{index}].{field} is not a non-negative integer or null"
        ))
    })
}

fn unit_issue(object: &Map<String, Value>, index: usize) -> Result<Option<IssueObs>, RenderError> {
    let value = unit_field(object, index, "issue")?;
    if value.is_null() {
        return Ok(None);
    }
    let raw = value.as_str().ok_or_else(|| {
        RenderError::NotA(format!("units[{index}].issue is not a string or null"))
    })?;
    IssueObs::parse(raw).map(Some).ok_or_else(|| {
        RenderError::NotA(format!(
            "units[{index}].issue {raw:?} is not pending or closed"
        ))
    })
}

fn unit_pull(object: &Map<String, Value>, index: usize) -> Result<Option<PullObs>, RenderError> {
    let value = unit_field(object, index, "pull")?;
    if value.is_null() {
        return Ok(None);
    }
    let raw = value
        .as_str()
        .ok_or_else(|| RenderError::NotA(format!("units[{index}].pull is not a string or null")))?;
    PullObs::parse(raw).map(Some).ok_or_else(|| {
        RenderError::NotA(format!(
            "units[{index}].pull {raw:?} is not draft, ready or merged"
        ))
    })
}

/// One `UnitObs` literal, one line — the shape the A0 emitter measured.
fn unit_literal(entry: &UnitEntry) -> String {
    format!(
        "  {{ issue := {}, branches := {}, branchDeleted := {}, pulls := {}, pull := {}, \
         ciRuns := {}, closesKeyword := {}, verifyParts := {} }}",
        option_lean(entry.issue, |value| value.lean().to_owned()),
        option_lean(entry.branches, |value| value.to_string()),
        option_lean(entry.branch_deleted, |value| value.to_string()),
        option_lean(entry.pulls, |value| value.to_string()),
        option_lean(entry.pull, |value| value.lean().to_owned()),
        option_lean(entry.ci_runs, |value| value.to_string()),
        option_lean(entry.closes_keyword, |value| value.to_string()),
        option_lean(entry.verify_parts, |value| value.to_string()),
    )
}

fn option_lean<T>(value: Option<T>, render: impl Fn(T) -> String) -> String {
    match value {
        Some(value) => format!("some {}", render(value)),
        None => "none".to_owned(),
    }
}

/// The per-rule outcomes, in the model's `ruleResults` order. The kernel proves this literal, so a
/// renderer that computes a rule differently from the model is refused, not shipped.
fn rules_literal(entries: &[UnitEntry]) -> String {
    let pair = |name: &str, value: bool| format!("(\"{name}\", {value})");
    let inner = [
        pair("oneBranch", entries.iter().all(UnitEntry::one_branch)),
        pair("onePull", entries.iter().all(UnitEntry::one_pull)),
        pair("pullUsable", entries.iter().all(UnitEntry::pull_usable)),
        pair("ciBudget", entries.iter().all(UnitEntry::ci_budget)),
        pair(
            "mergeComplete",
            entries.iter().all(UnitEntry::merge_complete),
        ),
        pair(
            "closesKeyword",
            entries.iter().all(UnitEntry::closes_stated),
        ),
        pair("verifyParts", entries.iter().all(UnitEntry::parts_verified)),
    ]
    .join(", ");
    format!("[{inner}]")
}
