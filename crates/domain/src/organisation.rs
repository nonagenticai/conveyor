//! The organisation spine's vocabulary (SPEC_V2 §4.1–§4.4; T0, issue #34 P2).
//!
//! Two things live here and nowhere else:
//!
//! - [`release_hash`] — the immutable release's identity. It is the crate's ONE canonical-form
//!   hash ([`crate::compare::content_hash`], the RFC 8785 subset), because a second hashing
//!   function would be a second definition of "the same release". The same job and pins
//!   compiled twice therefore hash identically and their snapshots are byte-identical (P2.1).
//! - [`BindingStatus`] — the binding's state machine (§4.4). A retired binding is refused for
//!   new runs (`409 job_binding_stale`) and existing runs keep their pin.

use std::str::FromStr;

use serde_json::Value;

/// SHA-256 of the canonical release snapshot, lowercase hex.
///
/// The canonical form is [`crate::compare::canonical_json`]: explicit UTF-16 key sort, no
/// whitespace. The store writes the returned string to `job_release.release_hash` and copies it
/// onto every binding and every run that pins the release.
pub fn release_hash(snapshot: &Value) -> String {
    crate::compare::content_hash(snapshot)
}

/// A binding's state (SPEC_V2 §4.4: `draft | active | retired`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BindingStatus {
    /// Authored but not yet usable by a run.
    Draft,
    /// The live pin: a run may name it.
    Active,
    /// Withdrawn: new runs naming it are refused; history keeps its pin.
    Retired,
}

impl BindingStatus {
    /// Every status, in lifecycle order.
    pub const ALL: [BindingStatus; 3] = [
        BindingStatus::Draft,
        BindingStatus::Active,
        BindingStatus::Retired,
    ];

    /// The database spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            BindingStatus::Draft => "draft",
            BindingStatus::Active => "active",
            BindingStatus::Retired => "retired",
        }
    }

    /// Whether a NEW run may name a binding in this state.
    ///
    /// A draft binding has no live pin yet; a retired one is exactly what
    /// `409 job_binding_stale` exists to refuse.
    pub fn is_usable_for_new_runs(self) -> bool {
        matches!(self, BindingStatus::Active)
    }
}

impl FromStr for BindingStatus {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "draft" => Ok(BindingStatus::Draft),
            "active" => Ok(BindingStatus::Active),
            "retired" => Ok(BindingStatus::Retired),
            other => Err(format!("{other:?} is not a binding status")),
        }
    }
}

// ── the release anatomy (S21 step 4, ADR 0010) ──────────────────────────────────────────────

use std::collections::BTreeSet;

use crate::executor::ExecutorKind;

/// The effect classes a grant may name, ranked exactly as the action spec ranks them
/// (`read < write < spend < destructive`, SPEC_V2 §11.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SideEffect {
    /// Reads only.
    Read,
    /// Creates or updates.
    Write,
    /// Spends money without deleting.
    Spend,
    /// Deletes or irreversibly transitions.
    Destructive,
}

impl SideEffect {
    /// The stored spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            SideEffect::Read => "read",
            SideEffect::Write => "write",
            SideEffect::Spend => "spend",
            SideEffect::Destructive => "destructive",
        }
    }

    /// Parse the stored spelling; an unknown ceiling is refused, never treated as unlimited.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "read" => Some(SideEffect::Read),
            "write" => Some(SideEffect::Write),
            "spend" => Some(SideEffect::Spend),
            "destructive" => Some(SideEffect::Destructive),
            _ => None,
        }
    }

    /// The rank comparisons use: `read < write < spend < destructive`.
    pub fn rank(self) -> u8 {
        match self {
            SideEffect::Read => 0,
            SideEffect::Write => 1,
            SideEffect::Spend => 2,
            SideEffect::Destructive => 3,
        }
    }
}

/// One operation the job may use — the release's grant (`op`, `variant`, `max_side_effect`,
/// `enabled`), inside the release hash. It names an operation, never a tool's content: the
/// schemas live on the tool version and the version/credential live on the binding (§11.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseGrant {
    /// Canonical `<toolkit>.<operation>`.
    pub op: String,
    /// `default` or absent — the tool platform has no variant column, and silently ignoring a
    /// variant would be a lie.
    pub variant: Option<String>,
    /// The highest effect class this grant permits.
    pub max_side_effect: SideEffect,
    /// Whether the operation is offered; a disabled grant is neither required nor offered.
    pub enabled: bool,
}

/// The loop caps a release may narrow (SPEC_V2 §4.2 `limits`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReleaseLimits {
    /// Turns per generation (`1..=20`).
    pub max_iterations: Option<u32>,
    /// Wall clock per generation (`1..=300`).
    pub timeout_secs: Option<u32>,
    /// Tool calls per generation (`1..=50`).
    pub max_tool_calls: Option<u32>,
    /// The run's token ceiling (recorded, not enforced in this unit).
    pub max_tokens: Option<u64>,
    /// The run's cost ceiling in USD (recorded, not enforced in this unit).
    pub budget_usd: Option<f64>,
}

/// The execution fields a job definition carries, parsed from `release_snapshot.anatomy`.
///
/// The parser is deliberately strict about the fields it owns — a malformed grant, an unknown
/// effect, a duplicate op or an unknown `limits` key is refused — and deliberately tolerant of
/// the anatomy's display fields (`knowledge`, `memory`, `binding`, `gates`, `capabilities`),
/// which are not execution configuration.
#[derive(Clone, Debug, PartialEq)]
pub struct ReleaseAnatomy {
    /// `job_key`, when the anatomy carries one.
    pub job_key: String,
    /// `title`, when the anatomy carries one.
    pub title: String,
    /// The executor the job declares; when present a binding must agree.
    pub executor_kind: Option<ExecutorKind>,
    /// The inference model the release pins.
    pub model_ref: Option<String>,
    /// `<recipe>@<version>`, the compiled prompt the release pins.
    pub prompt_recipe: Option<String>,
    /// The published procedure the release names, when the job is procedure-backed.
    pub procedure_ref: Option<String>,
    /// The job's purpose — the Request source's description for an `internal_agent` run.
    pub charter: Option<String>,
    /// What the job may do without a gate — also rendered into the Request source.
    pub authority: Option<String>,
    /// The named gate a per-job `gate.precheck` policy may be keyed by.
    pub gate: Option<String>,
    /// The enabled/disabled operation allowlist.
    pub grants: Vec<ReleaseGrant>,
    /// Whether the anatomy declared `grants` at all. Absent is an empty grant, never "no bound".
    pub grants_declared: bool,
    /// The loop caps.
    pub limits: ReleaseLimits,
}

/// One field-level refusal from [`parse_release_anatomy`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnatomyFieldError {
    /// The field path (`grants[2].max_side_effect`).
    pub field: String,
    /// A stable code (`invalid_value`, `missing`, `duplicate`, `out_of_range`).
    pub code: String,
    /// What was wrong.
    pub detail: String,
}

/// Why a release snapshot's anatomy is unusable.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AnatomyError {
    /// The snapshot carries no `anatomy` object at all.
    #[error("the release snapshot carries no anatomy object")]
    Missing,
    /// One or more fields are invalid; every one is named.
    #[error("the release anatomy is invalid")]
    Fields(Vec<AnatomyFieldError>),
}

fn field_error(
    field: impl Into<String>,
    code: &str,
    detail: impl Into<String>,
) -> AnatomyFieldError {
    AnatomyFieldError {
        field: field.into(),
        code: code.to_owned(),
        detail: detail.into(),
    }
}

/// A non-empty trimmed string, or `None`.
fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// `key@version`, version a positive integer.
fn valid_recipe(reference: &str) -> bool {
    let Some((key, version)) = reference.split_once('@') else {
        return false;
    };
    !key.trim().is_empty()
        && !version.trim().is_empty()
        && version.parse::<i64>().is_ok_and(|version| version >= 1)
}

fn parse_limits(value: &Value, errors: &mut Vec<AnatomyFieldError>) -> ReleaseLimits {
    let mut limits = ReleaseLimits::default();
    let Some(object) = value.as_object() else {
        errors.push(field_error(
            "limits",
            "invalid_value",
            "limits must be an object",
        ));
        return limits;
    };
    for (key, value) in object {
        match key.as_str() {
            "max_iterations" => match value.as_u64().filter(|value| (1..=20).contains(value)) {
                Some(number) => limits.max_iterations = Some(number as u32),
                None => errors.push(field_error(
                    "limits.max_iterations",
                    "out_of_range",
                    "max_iterations must be an integer in 1..=20",
                )),
            },
            "timeout_secs" => match value.as_u64().filter(|value| (1..=300).contains(value)) {
                Some(number) => limits.timeout_secs = Some(number as u32),
                None => errors.push(field_error(
                    "limits.timeout_secs",
                    "out_of_range",
                    "timeout_secs must be an integer in 1..=300",
                )),
            },
            "max_tool_calls" => match value.as_u64().filter(|value| (1..=50).contains(value)) {
                Some(number) => limits.max_tool_calls = Some(number as u32),
                None => errors.push(field_error(
                    "limits.max_tool_calls",
                    "out_of_range",
                    "max_tool_calls must be an integer in 1..=50",
                )),
            },
            "max_tokens" => match value.as_u64().filter(|value| *value >= 1) {
                Some(number) => limits.max_tokens = Some(number),
                None => errors.push(field_error(
                    "limits.max_tokens",
                    "out_of_range",
                    "max_tokens must be a positive integer",
                )),
            },
            "budget_usd" => match value.as_f64().filter(|value| *value >= 0.0) {
                Some(number) => limits.budget_usd = Some(number),
                None => errors.push(field_error(
                    "limits.budget_usd",
                    "out_of_range",
                    "budget_usd must be a non-negative number",
                )),
            },
            other => errors.push(field_error(
                format!("limits.{other}"),
                "unknown_field",
                "unknown limit; the loop caps are max_iterations, timeout_secs, max_tool_calls, \
                 max_tokens and budget_usd",
            )),
        }
    }
    limits
}

fn parse_grants(value: &Value, errors: &mut Vec<AnatomyFieldError>) -> Vec<ReleaseGrant> {
    let mut grants = Vec::new();
    let Some(entries) = value.as_array() else {
        errors.push(field_error(
            "grants",
            "invalid_value",
            "grants must be an array of {op, variant?, max_side_effect, enabled?} objects",
        ));
        return grants;
    };
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for (index, entry) in entries.iter().enumerate() {
        let Some(object) = entry.as_object() else {
            errors.push(field_error(
                format!("grants[{index}]"),
                "invalid_value",
                "a grant must be an object; a bare operation string is not one",
            ));
            continue;
        };
        let op = object
            .get("op")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|op| !op.is_empty());
        let Some(op) = op else {
            errors.push(field_error(
                format!("grants[{index}].op"),
                "missing",
                "a grant names its operation as <toolkit>.<operation>",
            ));
            continue;
        };
        if !op.contains('.') {
            errors.push(field_error(
                format!("grants[{index}].op"),
                "invalid_value",
                format!("{op:?} is not <toolkit>.<operation>"),
            ));
            continue;
        }
        if !seen.insert(op.to_owned()) {
            errors.push(field_error(
                format!("grants[{index}].op"),
                "duplicate",
                format!("{op} is granted twice; one row per operation"),
            ));
            continue;
        }
        let variant = match object.get("variant") {
            None | Some(Value::Null) => None,
            Some(Value::String(text)) if text == "default" => Some("default".to_owned()),
            Some(other) => {
                errors.push(field_error(
                    format!("grants[{index}].variant"),
                    "invalid_value",
                    format!(
                        "{other} is not a variant; the tool platform has no variant column, so \
                         only `default` (or absent) is accepted"
                    ),
                ));
                None
            }
        };
        let max_side_effect = match object
            .get("max_side_effect")
            .and_then(Value::as_str)
            .and_then(SideEffect::parse)
        {
            Some(effect) => effect,
            None => {
                errors.push(field_error(
                    format!("grants[{index}].max_side_effect"),
                    "invalid_value",
                    "max_side_effect must be one of read, write, spend, destructive",
                ));
                continue;
            }
        };
        let enabled = match object.get("enabled") {
            None | Some(Value::Null) => true,
            Some(Value::Bool(enabled)) => *enabled,
            Some(_) => {
                errors.push(field_error(
                    format!("grants[{index}].enabled"),
                    "invalid_value",
                    "enabled must be a boolean",
                ));
                continue;
            }
        };
        grants.push(ReleaseGrant {
            op: op.to_owned(),
            variant,
            max_side_effect,
            enabled,
        });
    }
    grants
}

/// Parse the execution anatomy out of a release snapshot.
///
/// Every invalid field is reported, so a publish refusal names all of them at once. The parser
/// never passes silently over a field it owns: an unknown effect, a bare-string grant, a
/// duplicate op or an unknown `limits` key is a refusal.
pub fn parse_release_anatomy(snapshot: &Value) -> Result<ReleaseAnatomy, AnatomyError> {
    let anatomy = snapshot
        .get("anatomy")
        .and_then(Value::as_object)
        .ok_or(AnatomyError::Missing)?;
    let mut errors = Vec::new();

    let executor_kind = match anatomy.get("executor_kind") {
        None | Some(Value::Null) => None,
        Some(Value::String(slug)) => match ExecutorKind::parse(slug) {
            Ok(kind) => Some(kind),
            Err(_) => {
                errors.push(field_error(
                    "executor_kind",
                    "invalid_value",
                    format!("{slug:?} is not a valid executor kind (^[a-z][a-z0-9_]{{1,31}}$)"),
                ));
                None
            }
        },
        Some(_) => {
            errors.push(field_error(
                "executor_kind",
                "invalid_value",
                "executor_kind must be a string slug",
            ));
            None
        }
    };

    let prompt_recipe = match text(anatomy.get("prompt_recipe")) {
        Some(reference) if valid_recipe(&reference) => Some(reference),
        Some(reference) => {
            errors.push(field_error(
                "prompt_recipe",
                "invalid_value",
                format!("{reference:?} is not <recipe>@<version> with a positive version"),
            ));
            None
        }
        None => None,
    };

    let limits = match anatomy.get("limits") {
        None | Some(Value::Null) => ReleaseLimits::default(),
        Some(value) => parse_limits(value, &mut errors),
    };

    let (grants, grants_declared) = match anatomy.get("grants") {
        None | Some(Value::Null) => (Vec::new(), false),
        Some(value) => (parse_grants(value, &mut errors), true),
    };

    if !errors.is_empty() {
        return Err(AnatomyError::Fields(errors));
    }

    Ok(ReleaseAnatomy {
        job_key: text(anatomy.get("job_key")).unwrap_or_default(),
        title: text(anatomy.get("title")).unwrap_or_default(),
        executor_kind,
        model_ref: text(anatomy.get("model_ref")),
        prompt_recipe,
        procedure_ref: text(anatomy.get("procedure_ref")),
        charter: text(anatomy.get("charter")),
        authority: text(anatomy.get("authority")),
        gate: text(anatomy.get("gate")),
        grants,
        grants_declared,
        limits,
    })
}

impl ReleaseAnatomy {
    /// The model the run uses: the release's when present, else the live row's.
    pub fn effective_model_ref<'a>(&'a self, row: &'a str) -> &'a str {
        self.model_ref.as_deref().unwrap_or(row)
    }

    /// The prompt the run compiles: the release's when present, else the live row's.
    pub fn effective_prompt_ref<'a>(&'a self, row: Option<&'a str>) -> Option<&'a str> {
        self.prompt_recipe.as_deref().or(row)
    }

    /// The turn cap: the release and the row are both ceilings; the lower wins.
    pub fn effective_max_iterations(&self, row: i32) -> u8 {
        let row = u32::try_from(row).unwrap_or(1).clamp(1, 20);
        let release = self.limits.max_iterations.unwrap_or(20).clamp(1, 20);
        u8::try_from(release.min(row)).unwrap_or(20)
    }

    /// The wall-clock cap in milliseconds: the lower of the release, the row and the platform max.
    pub fn effective_timeout_ms(&self, row_secs: i32) -> i64 {
        let row = u32::try_from(row_secs).unwrap_or(1).clamp(1, 300);
        let release = self.limits.timeout_secs.unwrap_or(300).clamp(1, 300);
        i64::from(release.min(row)) * 1000
    }

    /// The tool-call cap the release names, when it names one.
    pub fn effective_max_tool_calls(&self) -> Option<u8> {
        self.limits
            .max_tool_calls
            .and_then(|calls| u8::try_from(calls).ok())
    }

    /// The grant for one canonical operation, when it is enabled.
    pub fn enabled_grant(&self, op: &str) -> Option<&ReleaseGrant> {
        self.grants
            .iter()
            .find(|grant| grant.enabled && grant.op == op)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_release_hash_is_the_crate_canonical_hash() {
        let snapshot = json!({"job": "backend_engineer", "pins": {"model": "m", "process": "p"}});
        assert_eq!(
            release_hash(&snapshot),
            crate::compare::content_hash(&snapshot)
        );
        assert_eq!(release_hash(&snapshot), release_hash(&snapshot));
        assert_eq!(release_hash(&snapshot).len(), 64);
    }

    #[test]
    fn reordered_snapshots_hash_identically() {
        let left = json!({"b": 1, "a": {"y": true, "x": [1, 2]}});
        let right = json!({"a": {"x": [1, 2], "y": true}, "b": 1});
        assert_eq!(release_hash(&left), release_hash(&right));
    }

    #[test]
    fn a_different_pin_is_a_different_release() {
        let left = json!({"model": "m-1"});
        let right = json!({"model": "m-2"});
        assert_ne!(release_hash(&left), release_hash(&right));
    }

    #[test]
    fn only_an_active_binding_takes_new_runs() {
        assert!(BindingStatus::Active.is_usable_for_new_runs());
        assert!(!BindingStatus::Draft.is_usable_for_new_runs());
        assert!(!BindingStatus::Retired.is_usable_for_new_runs());
        for status in BindingStatus::ALL {
            assert_eq!(
                BindingStatus::from_str(status.as_str()).expect("round-trips"),
                status
            );
        }
        assert!(BindingStatus::from_str("live").is_err());
    }

    fn anatomy(value: Value) -> Value {
        json!({"job_key": "backend_engineer", "anatomy": value})
    }

    #[test]
    fn an_anatomy_parses_its_execution_fields() {
        let snapshot = anatomy(json!({
            "job_key": "backend_engineer",
            "title": "Backend engineer",
            "executor_kind": "internal_agent",
            "model_ref": "router/medium",
            "prompt_recipe": "implementation_role@4",
            "charter": "Implement bounded backend changes.",
            "authority": "implement, test, propose PR",
            "gate": "merge",
            "grants": [
                {"op": "coding.jobs.create", "max_side_effect": "write"},
                {"op": "coding.jobs.get", "max_side_effect": "read", "enabled": false},
                {"op": "ops.k8s_restart", "variant": "default", "max_side_effect": "destructive"}
            ],
            "limits": {"max_iterations": 12, "timeout_secs": 300, "max_tool_calls": 20,
                        "max_tokens": 40000, "budget_usd": 2.5},
            "knowledge": "base → software",
            "binding": "toolkit coding@4 · ceiling write"
        }));
        let parsed = parse_release_anatomy(&snapshot).expect("parses");
        assert_eq!(parsed.job_key, "backend_engineer");
        assert_eq!(parsed.title, "Backend engineer");
        assert_eq!(
            parsed.executor_kind.as_ref().map(ExecutorKind::as_str),
            Some("internal_agent")
        );
        assert_eq!(parsed.model_ref.as_deref(), Some("router/medium"));
        assert_eq!(
            parsed.prompt_recipe.as_deref(),
            Some("implementation_role@4")
        );
        assert_eq!(parsed.gate.as_deref(), Some("merge"));
        assert!(parsed.grants_declared);
        assert_eq!(parsed.grants.len(), 3);
        assert_eq!(parsed.grants[0].max_side_effect, SideEffect::Write);
        assert!(parsed.grants[0].enabled);
        assert!(!parsed.grants[1].enabled);
        assert_eq!(parsed.grants[2].variant.as_deref(), Some("default"));
        assert_eq!(parsed.limits.max_iterations, Some(12));
        assert_eq!(parsed.limits.budget_usd, Some(2.5));
    }

    #[test]
    fn an_absent_anatomy_and_absent_fields_are_allowed() {
        assert_eq!(
            parse_release_anatomy(&json!({"job_key": "x"})),
            Err(AnatomyError::Missing)
        );
        let parsed = parse_release_anatomy(&anatomy(json!({"capabilities": ["delivery"]})))
            .expect("an anatomy with no execution field parses");
        assert_eq!(parsed.executor_kind, None);
        assert_eq!(parsed.grants, Vec::new());
        assert!(!parsed.grants_declared);
        assert_eq!(parsed.limits, ReleaseLimits::default());
    }

    #[test]
    fn a_bare_string_grant_is_refused() {
        let error = parse_release_anatomy(&anatomy(json!({"grants": ["coding.jobs.create"]})))
            .expect_err("a bare string is not a grant");
        let AnatomyError::Fields(fields) = error else {
            panic!("expected field errors");
        };
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field, "grants[0]");
        assert_eq!(fields[0].code, "invalid_value");
    }

    #[test]
    fn every_bad_grant_field_is_reported_at_once() {
        let error = parse_release_anatomy(&anatomy(json!({
            "grants": [
                {"op": "coding.jobs.create", "max_side_effect": "admin"},
                {"op": "coding.jobs.create", "max_side_effect": "read"},
                {"op": "codingjobs", "max_side_effect": "read"},
                {"op": "coding.jobs.get", "variant": "eu", "max_side_effect": "read"}
            ]
        })))
        .expect_err("four refusals");
        let AnatomyError::Fields(fields) = error else {
            panic!("expected field errors");
        };
        let codes: Vec<&str> = fields.iter().map(|field| field.code.as_str()).collect();
        assert!(codes.contains(&"invalid_value"));
        assert!(codes.contains(&"duplicate"));
        assert_eq!(fields.len(), 4, "{fields:?}");
    }

    #[test]
    fn an_unknown_or_out_of_range_limit_is_refused() {
        let error = parse_release_anatomy(&anatomy(json!({
            "limits": {"turns": 12, "max_iterations": 21}
        })))
        .expect_err("unknown key and out-of-range cap");
        let AnatomyError::Fields(fields) = error else {
            panic!("expected field errors");
        };
        assert!(fields.iter().any(|field| field.field == "limits.turns"));
        assert!(
            fields
                .iter()
                .any(|field| field.field == "limits.max_iterations")
        );
    }

    #[test]
    fn a_recipe_must_name_a_version() {
        assert_eq!(
            parse_release_anatomy(&anatomy(json!({"prompt_recipe": "implementation_role"}))),
            Err(AnatomyError::Fields(vec![field_error(
                "prompt_recipe",
                "invalid_value",
                "\"implementation_role\" is not <recipe>@<version> with a positive version",
            )]))
        );
        assert!(
            parse_release_anatomy(&anatomy(json!({"prompt_recipe": "agent@0"}))).is_err(),
            "version 0 is not a recipe"
        );
    }

    #[test]
    fn the_effective_pins_take_the_release_first_and_the_lower_cap() {
        let parsed = parse_release_anatomy(&anatomy(json!({
            "model_ref": "router/high",
            "prompt_recipe": "agent@2",
            "limits": {"max_iterations": 12, "timeout_secs": 120, "max_tool_calls": 5}
        })))
        .expect("parses");
        assert_eq!(parsed.effective_model_ref("row-model"), "router/high");
        assert_eq!(
            parsed.effective_prompt_ref(Some("agent@1")),
            Some("agent@2")
        );
        assert_eq!(
            parsed.effective_max_iterations(4),
            4,
            "the row is a ceiling"
        );
        assert_eq!(
            parsed.effective_max_iterations(16),
            12,
            "the release is a ceiling"
        );
        assert_eq!(parsed.effective_timeout_ms(60), 60_000);
        assert_eq!(parsed.effective_timeout_ms(300), 120_000);
        assert_eq!(parsed.effective_max_tool_calls(), Some(5));

        let empty = ReleaseAnatomy {
            job_key: String::new(),
            title: String::new(),
            executor_kind: None,
            model_ref: None,
            prompt_recipe: None,
            procedure_ref: None,
            charter: None,
            authority: None,
            gate: None,
            grants: Vec::new(),
            grants_declared: false,
            limits: ReleaseLimits::default(),
        };
        assert_eq!(empty.effective_model_ref("row-model"), "row-model");
        assert_eq!(empty.effective_prompt_ref(Some("agent@1")), Some("agent@1"));
        assert_eq!(empty.effective_max_iterations(3), 3);
        assert_eq!(empty.effective_timeout_ms(300), 300_000);
        assert_eq!(empty.effective_max_tool_calls(), None);
    }

    #[test]
    fn only_an_enabled_grant_is_found() {
        let parsed = parse_release_anatomy(&anatomy(json!({
            "grants": [
                {"op": "coding.jobs.get", "max_side_effect": "read"},
                {"op": "coding.jobs.create", "max_side_effect": "write", "enabled": false}
            ]
        })))
        .expect("parses");
        assert_eq!(
            parsed
                .enabled_grant("coding.jobs.get")
                .map(|grant| grant.op.as_str()),
            Some("coding.jobs.get")
        );
        assert!(parsed.enabled_grant("coding.jobs.create").is_none());
        assert!(parsed.enabled_grant("coding.jobs.delete").is_none());
    }
}
