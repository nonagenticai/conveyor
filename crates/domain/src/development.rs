//! Development projects — the self-development path (SPEC_V2 §5.2, §5.4, §6.4, §9.1, §9.3,
//! §12 T6; issue #80).
//!
//! Pure rules only. The store writes what these agree to and the action layer is the transport;
//! the database refuses the vocabulary.
//!
//! Three properties live here in their checkable form:
//!
//! - **no step may be skipped** ([`Step::requires`]): every step names its predecessors, and
//!   closure requires the full ordered set;
//! - **evidence is typed** ([`validate_evidence`]): a branch names its base, a PR carries a
//!   number/url/head, a CI verdict is `green` or `red`, a live check carries the serving digest
//!   and a 200 from `/health`;
//! - **auto-merge is conditions, never a constant** ([`AutoMergeConditions`]): every value is
//!   read from the tenant's policy row and evaluated against the project's recorded evidence.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Timestamp;

/// The gate set a development project is chartered with (§5.2: design conditional, merge,
/// deploy). The project's own actions cannot change it (§9.3).
pub const DEFAULT_GATE_SET: [&str; 3] = ["review", "merge", "deploy"];

/// The surfaces a development project may not change for itself (§9.3). A governance project
/// with a different approver set changes them.
///
/// `proof_policy` is the one surface whose `required` flag may be tightened through the action:
/// the specification pin is never rewritten, and the implementer can never loosen it (#837).
pub const JUDGING_SURFACES: [&str; 4] =
    ["gate_set", "approval_policy", "instrument", "proof_policy"];

/// The approver kind an approval-policy row names (§6.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApproverKind {
    /// A person decides the gate. The default.
    Human,
    /// The tenant's earned auto-merge policy decides, when its conditions hold.
    Auto,
    /// The triage agent decides (the lane's explicit non-governance declaration, §5.4).
    TriageAgent,
    /// A deterministic rule decides.
    Rule,
    /// Another agent decides.
    Agent,
    /// A per-tenant channel service account decides (S06d's connector identity).
    Service,
}

impl ApproverKind {
    /// The stored and wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Auto => "auto",
            Self::TriageAgent => "triage_agent",
            Self::Rule => "rule",
            Self::Agent => "agent",
            Self::Service => "service",
        }
    }
}

impl fmt::Display for ApproverKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a string is not an approver kind.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown approver kind {0:?}")]
pub struct ApproverKindError(pub String);

impl FromStr for ApproverKind {
    type Err = ApproverKindError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "human" => Ok(Self::Human),
            "auto" => Ok(Self::Auto),
            "triage_agent" => Ok(Self::TriageAgent),
            "rule" => Ok(Self::Rule),
            "agent" => Ok(Self::Agent),
            "service" => Ok(Self::Service),
            other => Err(ApproverKindError(other.to_owned())),
        }
    }
}

impl From<crate::approval::DeciderKind> for ApproverKind {
    fn from(kind: crate::approval::DeciderKind) -> Self {
        match kind {
            crate::approval::DeciderKind::Human => Self::Human,
            crate::approval::DeciderKind::TriageAgent => Self::TriageAgent,
            crate::approval::DeciderKind::Rule => Self::Rule,
            crate::approval::DeciderKind::Agent => Self::Agent,
            crate::approval::DeciderKind::Service => Self::Service,
        }
    }
}

/// One step of the delivery path, in order (§9.1 3–8).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Step {
    /// The lane branch.
    Branch,
    /// The pull request.
    PullRequest,
    /// The CI run's verdict.
    Ci,
    /// The merge commit.
    Merge,
    /// The rollback plan, recorded before the deploy.
    RollbackPlan,
    /// The deployed image digest.
    Deploy,
    /// The live check at the deployed digest.
    LiveVerify,
    /// An exercised rollback: the prior digest serves again.
    RollbackExercise,
}

impl Step {
    /// Every step, in delivery order.
    pub const ALL: [Step; 8] = [
        Step::Branch,
        Step::PullRequest,
        Step::Ci,
        Step::Merge,
        Step::RollbackPlan,
        Step::Deploy,
        Step::LiveVerify,
        Step::RollbackExercise,
    ];

    /// The steps closure requires. An exercised rollback is evidence the closure carries when
    /// it exists; it is not a precondition of an ordinary landing.
    pub const REQUIRED_FOR_CLOSURE: [Step; 7] = [
        Step::Branch,
        Step::PullRequest,
        Step::Ci,
        Step::Merge,
        Step::RollbackPlan,
        Step::Deploy,
        Step::LiveVerify,
    ];

    /// The stored and wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Branch => "branch",
            Self::PullRequest => "pull_request",
            Self::Ci => "ci",
            Self::Merge => "merge",
            Self::RollbackPlan => "rollback_plan",
            Self::Deploy => "deploy",
            Self::LiveVerify => "live_verify",
            Self::RollbackExercise => "rollback_exercise",
        }
    }

    /// The steps that must already be recorded before this one. §9.1's order, with the rollback
    /// plan before the deploy and the live check after it.
    pub fn requires(self) -> &'static [Step] {
        match self {
            Self::Branch => &[],
            Self::PullRequest => &[Self::Branch],
            Self::Ci => &[Self::PullRequest],
            Self::Merge => &[Self::Ci],
            Self::RollbackPlan => &[Self::Merge],
            Self::Deploy => &[Self::Merge, Self::RollbackPlan],
            Self::LiveVerify => &[Self::Deploy],
            Self::RollbackExercise => &[Self::Deploy],
        }
    }
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a string is not a step.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown development step {0:?}")]
pub struct StepError(pub String);

impl FromStr for Step {
    type Err = StepError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Step::ALL
            .into_iter()
            .find(|step| step.as_str() == raw)
            .ok_or_else(|| StepError(raw.to_owned()))
    }
}

/// Why a step's evidence is not the shape the step writes.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct EvidenceError(pub String);

fn non_empty<'a>(evidence: &'a Value, key: &str) -> Result<&'a str, EvidenceError> {
    match evidence.get(key).and_then(Value::as_str) {
        Some(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(EvidenceError(format!("`{key}` must be a non-empty string"))),
    }
}

fn hex(value: &str, key: &str, min: usize, max: usize) -> Result<(), EvidenceError> {
    if (min..=max).contains(&value.len()) && value.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(EvidenceError(format!(
            "`{key}` must be {min}..={max} hexadecimal characters"
        )))
    }
}

fn integer(evidence: &Value, key: &str) -> Result<i64, EvidenceError> {
    evidence
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| EvidenceError(format!("`{key}` must be an integer")))
}

fn string_array(evidence: &Value, key: &str) -> Result<Vec<String>, EvidenceError> {
    let Some(items) = evidence.get(key).and_then(Value::as_array) else {
        return Err(EvidenceError(format!(
            "`{key}` must be an array of non-empty strings"
        )));
    };
    if items.is_empty() {
        return Err(EvidenceError(format!("`{key}` must not be empty")));
    }
    items
        .iter()
        .map(|item| {
            item.as_str()
                .filter(|value| !value.trim().is_empty())
                .map(str::to_owned)
                .ok_or_else(|| {
                    EvidenceError(format!("`{key}` must be an array of non-empty strings"))
                })
        })
        .collect()
}

/// Validate one step's evidence document. The action calls this before any write; the store
/// never sees a shape the step's own reader cannot use.
pub fn validate_evidence(step: Step, evidence: &Value) -> Result<(), EvidenceError> {
    if !evidence.is_object() {
        return Err(EvidenceError("evidence must be a JSON object".to_owned()));
    }
    match step {
        Step::Branch => {
            non_empty(evidence, "name")?;
            non_empty(evidence, "base")?;
        }
        Step::PullRequest => {
            let number = integer(evidence, "number")?;
            if number <= 0 {
                return Err(EvidenceError(
                    "`number` must be a positive PR number".to_owned(),
                ));
            }
            let url = non_empty(evidence, "url")?;
            if !(url.starts_with("https://") || url.starts_with("http://")) {
                return Err(EvidenceError("`url` must be an http(s) URL".to_owned()));
            }
            let head = non_empty(evidence, "head_sha")?;
            hex(head, "head_sha", 7, 64)?;
            if let Some(paths) = evidence.get("paths") {
                let paths = paths.as_array().ok_or_else(|| {
                    EvidenceError("`paths` must be an array of non-empty strings".to_owned())
                })?;
                for path in paths {
                    if path.as_str().is_none_or(|value| value.trim().is_empty()) {
                        return Err(EvidenceError(
                            "`paths` must be an array of non-empty strings".to_owned(),
                        ));
                    }
                }
            }
        }
        Step::Ci => {
            non_empty(evidence, "run_id")?;
            let sha = non_empty(evidence, "sha")?;
            hex(sha, "sha", 7, 64)?;
            non_empty(evidence, "verdict")?;
            CiVerdict::from_str(non_empty(evidence, "verdict")?)?;
        }
        Step::Merge => {
            let sha = non_empty(evidence, "merge_sha")?;
            hex(sha, "merge_sha", 40, 40)?;
        }
        Step::RollbackPlan => {
            non_empty(evidence, "prior_digest")?;
            non_empty(evidence, "method")?;
            if evidence.get("tested").and_then(Value::as_bool).is_none() {
                return Err(EvidenceError("`tested` must be a boolean".to_owned()));
            }
            string_array(evidence, "evidence")?;
            // Every migration must be expand/contract (backward-compatible with the
            // immediately prior release), because the runtime migrator lets the prior release
            // start against the newer schema. A deploy whose migrations are *not* additive
            // forbids an application-only rollback: the plan must name the restore path
            // instead (issue #669; `migrations/README.md`).
            let backward_compatible = evidence
                .get("backward_compatible")
                .and_then(Value::as_bool)
                .ok_or_else(|| {
                    EvidenceError("`backward_compatible` must be a boolean".to_owned())
                })?;
            if !backward_compatible {
                non_empty(evidence, "restore_path").map_err(|_| {
                    EvidenceError(
                        "`restore_path` is required when `backward_compatible` is false: a \
                         non-additive migration requires a restore-based rollback through the \
                         gated ops path, never an application-only rollback"
                            .to_owned(),
                    )
                })?;
            }
        }
        Step::Deploy => {
            non_empty(evidence, "digest")?;
            let generation = integer(evidence, "migration_generation")?;
            if generation < 1 {
                return Err(EvidenceError(
                    "`migration_generation` must be the positive max applied migration version \
                     at deploy time"
                        .to_owned(),
                ));
            }
        }
        Step::LiveVerify => {
            let status = integer(evidence, "health_status")?;
            if status != 200 {
                return Err(EvidenceError(format!(
                    "`health_status` must be 200, got {status}"
                )));
            }
            non_empty(evidence, "serving_digest")?;
            non_empty(evidence, "acceptance_command")?;
            if integer(evidence, "acceptance_exit")? != 0 {
                return Err(EvidenceError(
                    "`acceptance_exit` must be 0: the acceptance command failed".to_owned(),
                ));
            }
        }
        Step::RollbackExercise => {
            let deployed = non_empty(evidence, "deployed_digest")?;
            let restored = non_empty(evidence, "restored_digest")?;
            if deployed == restored {
                return Err(EvidenceError(
                    "the rollback must restore a different digest than the deployed change it \
                     rolled back from"
                        .to_owned(),
                ));
            }
            let status = integer(evidence, "health_status")?;
            if status != 200 {
                return Err(EvidenceError(format!(
                    "`health_status` must be 200, got {status}"
                )));
            }
            let generation = integer(evidence, "migration_generation")?;
            if generation < 1 {
                return Err(EvidenceError(
                    "`migration_generation` must be the positive max applied migration version \
                     at deploy time"
                        .to_owned(),
                ));
            }
            string_array(evidence, "evidence")?;
        }
    }
    Ok(())
}

/// A CI run's verdict. Red is a refusal, never a retry (§9.1 7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CiVerdict {
    Green,
    Red,
}

impl CiVerdict {
    /// The stored and wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Green => "green",
            Self::Red => "red",
        }
    }
}

impl fmt::Display for CiVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CiVerdict {
    type Err = EvidenceError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "green" => Ok(Self::Green),
            "red" => Ok(Self::Red),
            other => Err(EvidenceError(format!(
                "`verdict` must be `green` or `red`, got {other:?}"
            ))),
        }
    }
}

/// The typed conditions an auto-merge policy carries (§6.4). Every field is read from the
/// tenant's `approval_policy` row; none of it is a constant in code.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AutoMergeConditions {
    /// The track record: at least this many consecutive green CI runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consecutive_green_runs: Option<u32>,
    /// The revert count: the policy is revoked above this many reverts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_reverts: Option<u32>,
    /// The path scope: every changed path must start with one of these prefixes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_scope: Vec<String>,
    /// The independent-reviewer requirement: the review gate must be decided by a principal
    /// other than the implementer.
    #[serde(default)]
    pub independent_reviewer: bool,
}

impl AutoMergeConditions {
    /// Parse and type-check a policy row's `conditions` document.
    pub fn parse(value: &Value) -> Result<Self, EvidenceError> {
        if !value.is_object() {
            return Err(EvidenceError(
                "auto-merge conditions must be a JSON object".to_owned(),
            ));
        }
        let conditions: Self = serde_json::from_value(value.clone()).map_err(|error| {
            EvidenceError(format!("auto-merge conditions are not typed: {error}"))
        })?;
        if let Some(scope) = conditions.path_scope.iter().find(|p| p.trim().is_empty()) {
            return Err(EvidenceError(format!(
                "path_scope contains a blank prefix {scope:?}"
            )));
        }
        if value.as_object().is_some_and(serde_json::Map::is_empty) {
            return Err(EvidenceError(
                "an auto-merge policy must carry at least one condition".to_owned(),
            ));
        }
        Ok(conditions)
    }
}

/// The recorded evidence an auto-merge evaluation reads.
#[derive(Clone, Debug)]
pub struct AutoMergeInputs<'a> {
    /// Consecutive green CI runs on this project.
    pub green_runs: u32,
    /// Reverts attributed to this project.
    pub reverts: u32,
    /// The paths the PR changed, when the PR step recorded them.
    pub changed_paths: Option<&'a [String]>,
    /// Whether the review gate was decided by a principal other than the implementer.
    pub independent_reviewer: bool,
}

/// The outcome of evaluating a policy row against the project's evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutoMergeEvaluation {
    /// Every condition holds.
    pub satisfied: bool,
    /// Why a condition did not hold, in row order.
    pub reasons: Vec<String>,
}

impl AutoMergeConditions {
    /// Evaluate the conditions against the evidence. A condition that cannot be checked (for
    /// example a path scope with no recorded paths) fails closed.
    pub fn evaluate(&self, inputs: &AutoMergeInputs<'_>) -> AutoMergeEvaluation {
        let mut reasons = Vec::new();
        if let Some(required) = self.consecutive_green_runs
            && inputs.green_runs < required
        {
            reasons.push(format!(
                "the project has {} consecutive green CI run(s); the policy requires {required}",
                inputs.green_runs
            ));
        }
        if let Some(max) = self.max_reverts
            && inputs.reverts > max
        {
            reasons.push(format!(
                "the project has {} revert(s); the policy allows at most {max}",
                inputs.reverts
            ));
        }
        if !self.path_scope.is_empty() {
            match inputs.changed_paths {
                Some(paths) if !paths.is_empty() => {
                    if let Some(path) = paths
                        .iter()
                        .find(|path| !self.path_scope.iter().any(|scope| path.starts_with(scope)))
                    {
                        reasons.push(format!(
                            "the change touches {path:?}, which is outside the policy's path scope"
                        ));
                    }
                }
                _ => reasons.push(
                    "the PR records no changed paths, so the policy's path scope cannot be checked"
                        .to_owned(),
                ),
            }
        }
        if self.independent_reviewer && !inputs.independent_reviewer {
            reasons.push(
                "the review gate was not decided by a principal other than the implementer"
                    .to_owned(),
            );
        }
        AutoMergeEvaluation {
            satisfied: reasons.is_empty(),
            reasons,
        }
    }
}

/// Whether an auto-merge policy is still in force at `at`. An expired policy fails closed: the
/// gate falls back to a human.
pub fn auto_merge_in_force(expires_at: Option<Timestamp>, at: Timestamp) -> bool {
    expires_at.is_some_and(|expires_at| expires_at > at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_step_names_its_predecessors_and_closure_needs_the_ordered_set() {
        assert!(Step::Branch.requires().is_empty());
        assert_eq!(Step::PullRequest.requires(), &[Step::Branch]);
        assert_eq!(Step::Ci.requires(), &[Step::PullRequest]);
        assert_eq!(Step::Merge.requires(), &[Step::Ci]);
        assert_eq!(Step::RollbackPlan.requires(), &[Step::Merge]);
        assert_eq!(Step::Deploy.requires(), &[Step::Merge, Step::RollbackPlan]);
        assert_eq!(Step::LiveVerify.requires(), &[Step::Deploy]);
        assert_eq!(Step::RollbackExercise.requires(), &[Step::Deploy]);
        assert_eq!(Step::REQUIRED_FOR_CLOSURE.len(), 7);
        assert!(!Step::REQUIRED_FOR_CLOSURE.contains(&Step::RollbackExercise));
    }

    #[test]
    fn the_step_evidence_shapes_are_typed() {
        assert!(
            validate_evidence(
                Step::Branch,
                &json!({"name": "issue/80-t6", "base": "main"})
            )
            .is_ok()
        );
        assert!(
            validate_evidence(Step::Branch, &json!({"name": "issue/80-t6"})).is_err(),
            "a branch without its base is not evidence"
        );
        assert!(
            validate_evidence(
                Step::Ci,
                &json!({"run_id": "1", "sha": "abcdef1", "verdict": "green"})
            )
            .is_ok()
        );
        assert!(
            validate_evidence(
                Step::Ci,
                &json!({"run_id": "1", "sha": "abcdef1", "verdict": "yellow"})
            )
            .is_err(),
            "a verdict outside green/red is not a CI verdict"
        );
        assert!(
            validate_evidence(
                Step::Merge,
                &json!({"merge_sha": "0123456789abcdef0123456789abcdef01234567"})
            )
            .is_ok()
        );
        assert!(
            validate_evidence(Step::Merge, &json!({"merge_sha": "abc"})).is_err(),
            "a merge SHA is 40 hexadecimal characters"
        );
        assert!(
            validate_evidence(
                Step::LiveVerify,
                &json!({
                    "health_status": 200,
                    "serving_digest": "sha256:abc",
                    "acceptance_command": "curl -fsS $BASE/health",
                    "acceptance_exit": 0,
                })
            )
            .is_ok()
        );
        assert!(
            validate_evidence(
                Step::LiveVerify,
                &json!({
                    "health_status": 503,
                    "serving_digest": "sha256:abc",
                    "acceptance_command": "curl -fsS $BASE/health",
                    "acceptance_exit": 0,
                })
            )
            .is_err(),
            "a live check that did not get 200 is a failed check"
        );
        assert!(
            validate_evidence(
                Step::RollbackPlan,
                &json!({
                    "prior_digest": "sha256:old",
                    "method": "flux rollback",
                    "tested": true,
                    "backward_compatible": true,
                    "evidence": ["staging rollback 2026-11-24"],
                })
            )
            .is_ok()
        );
        assert!(
            validate_evidence(
                Step::RollbackPlan,
                &json!({
                    "prior_digest": "sha256:old",
                    "method": "flux rollback",
                    "tested": true,
                    "evidence": ["staging rollback 2026-11-24"],
                })
            )
            .is_err(),
            "a rollback plan must declare whether the deploy's migrations are expand/contract"
        );
        assert!(
            validate_evidence(
                Step::RollbackPlan,
                &json!({
                    "prior_digest": "sha256:old",
                    "method": "restore the database",
                    "tested": true,
                    "backward_compatible": false,
                    "evidence": ["staging restore exercised 2026-11-24"],
                })
            )
            .is_err(),
            "a non-additive deploy must name the restore path, not an application rollback"
        );
        assert!(
            validate_evidence(
                Step::RollbackPlan,
                &json!({
                    "prior_digest": "sha256:old",
                    "method": "restore the database",
                    "tested": true,
                    "backward_compatible": false,
                    "restore_path": "ops: restore the nightly dump to generation 69",
                    "evidence": ["staging restore exercised 2026-11-24"],
                })
            )
            .is_ok()
        );
        assert!(
            validate_evidence(
                Step::Deploy,
                &json!({"digest": "sha256:new", "migration_generation": 70})
            )
            .is_ok()
        );
        assert!(
            validate_evidence(Step::Deploy, &json!({"digest": "sha256:new"})).is_err(),
            "a deploy records the max applied migration version (the schema generation)"
        );
        assert!(
            validate_evidence(
                Step::RollbackExercise,
                &json!({
                    "deployed_digest": "sha256:new",
                    "restored_digest": "sha256:new",
                    "migration_generation": 70,
                    "health_status": 200,
                    "evidence": ["health 200"],
                })
            )
            .is_err(),
            "a rollback that restored the same digest did not roll back"
        );
        assert!(
            validate_evidence(
                Step::RollbackExercise,
                &json!({
                    "deployed_digest": "sha256:new",
                    "restored_digest": "sha256:old",
                    "migration_generation": 70,
                    "health_status": 200,
                    "evidence": ["health 200 at the prior digest"],
                })
            )
            .is_ok()
        );
        assert!(
            validate_evidence(
                Step::RollbackExercise,
                &json!({
                    "deployed_digest": "sha256:new",
                    "restored_digest": "sha256:old",
                    "health_status": 200,
                    "evidence": ["health 200 at the prior digest"],
                })
            )
            .is_err(),
            "a rollback names the schema generation it exercised against"
        );
    }

    #[test]
    fn auto_merge_conditions_are_typed_and_fail_closed() {
        let conditions = AutoMergeConditions::parse(&json!({
            "consecutive_green_runs": 2,
            "max_reverts": 0,
            "path_scope": ["crates/"],
            "independent_reviewer": true,
        }))
        .expect("typed conditions");

        let satisfied = conditions.evaluate(&AutoMergeInputs {
            green_runs: 2,
            reverts: 0,
            changed_paths: Some(&["crates/actions/src/development.rs".to_owned()]),
            independent_reviewer: true,
        });
        assert!(satisfied.satisfied, "{satisfied:?}");

        let failed = conditions.evaluate(&AutoMergeInputs {
            green_runs: 1,
            reverts: 1,
            changed_paths: Some(&["docs/readme.md".to_owned()]),
            independent_reviewer: false,
        });
        assert!(!failed.satisfied);
        assert_eq!(failed.reasons.len(), 4, "{failed:?}");

        assert!(
            AutoMergeConditions::parse(&json!({})).is_err(),
            "an empty condition set is not an earned policy"
        );
        assert!(
            AutoMergeConditions::parse(&json!({"revert_limit": 1})).is_err(),
            "an unknown key is refused, never ignored"
        );
    }

    #[test]
    fn an_expired_policy_fails_closed() {
        let at = chrono::DateTime::from_timestamp(1_000, 0).expect("instant");
        assert!(auto_merge_in_force(
            Some(chrono::DateTime::from_timestamp(1_001, 0).expect("instant")),
            at
        ));
        assert!(!auto_merge_in_force(None, at));
        assert!(!auto_merge_in_force(
            Some(chrono::DateTime::from_timestamp(999, 0).expect("instant")),
            at
        ));
    }
}
