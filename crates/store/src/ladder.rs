#![forbid(unsafe_code)]
//! T2 — the decision ladder's queries (SPEC_V2 §7, §10.3, §12 T2; issue #73).
//!
//! Every function is `_tx`: the caller has already opened a tenant-scoped transaction, and the
//! write shares it with the action's outbox event (CONTRACT §7). No function here speaks the
//! action layer's vocabulary; a refusal the domain owns is raised as
//! [`LadderStoreError::Illegal`] and mapped once, by the action.
//!
//! The one read model is [`effective_policy_tx`]: a platform `decision_policy` is the case; the
//! tenant's `decision_policy_refinement` overrides its threshold, fail action and (optionally)
//! its laya config; the decision pins the three versions it read, so the row is reproducible
//! after any of them moves (§7.2).

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use conveyor_domain::ladder::{
    FailAction, FailPolicy, LadderError, Outcome, Pins, Rung, is_input_hash, missing_pins,
};
use serde_json::Value;
use sqlx::{AssertSqlSafe, PgConnection};
use uuid::Uuid;

use crate::parent::{Parent, ParentError, ensure_optional_parent_tx};

/// The decision ladder's failures.
#[derive(Debug, thiserror::Error)]
pub enum LadderStoreError {
    /// No such row in this tenant.
    #[error("the {what} does not exist in this tenant")]
    NotFound {
        /// The row's noun.
        what: &'static str,
    },
    /// The row changed since it was read.
    #[error("the {what} changed since it was read")]
    Stale {
        /// The row's noun.
        what: &'static str,
    },
    /// A policy already exists for this case.
    #[error("a policy already exists for this case")]
    Conflict {
        /// The row's noun.
        what: &'static str,
    },
    /// A referenced parent does not exist in this tenant.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The domain refused the write.
    #[error(transparent)]
    Illegal(#[from] LadderError),
    /// A human decision was recorded by a non-person actor.
    #[error("a human decision must be recorded by a person")]
    HumanRequiresPerson,
    /// The approval a human decision cites is not approved with an approve decision.
    #[error("the cited approval is not approved with an approve decision")]
    ApprovalNotAccepted,
    /// The approval a human decision cites belongs to another card.
    #[error("the cited approval belongs to another card")]
    ApprovalWrongCard,
    /// A stored vocabulary value no longer parses.
    #[error("the database holds {what} = {value:?}, which the domain does not know")]
    Vocabulary {
        /// The column's meaning.
        what: &'static str,
        /// The stored value.
        value: String,
    },
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

// ── policies ──────────────────────────────────────────────────────────────────────────────

/// One `decision_policy` row: the case, its threshold and what a failed answer does.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct PolicyRecord {
    /// The row id.
    pub id: Uuid,
    /// The case kind (e.g. `gate.precheck`, `request.classify`).
    pub kind: String,
    /// The gate the policy is specific to, if any.
    pub gate: Option<String>,
    /// The acceptance threshold in 0–1; `None` until the case has one.
    pub threshold: Option<f64>,
    /// `closed | open`.
    pub fail_policy: String,
    /// `handover | block | annotate`.
    pub action: String,
    /// Free-form metadata.
    pub meta: Value,
    /// The If-Match version.
    pub version: i64,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last write.
    pub updated_at: DateTime<Utc>,
}

const POLICY_COLUMNS: &str = "id, kind, gate, threshold::float8 AS threshold, fail_policy, action, \
                              meta, version, created_at, updated_at";

/// A policy to create.
#[derive(Clone, Debug)]
pub struct NewPolicy {
    /// The case kind.
    pub kind: String,
    /// The gate, when the policy is gate-specific.
    pub gate: Option<String>,
    /// The acceptance threshold in 0–1.
    pub threshold: Option<f64>,
    /// `closed | open`.
    pub fail_policy: String,
    /// `handover | block | annotate`.
    pub action: String,
    /// Free-form metadata.
    pub meta: Value,
}

impl NewPolicy {
    /// Validate the vocabularies before the insert reaches a CHECK.
    fn validate(&self) -> Result<(), LadderError> {
        if self.kind.trim().is_empty() {
            return Err(LadderError::FieldMissing {
                field: "kind",
                rung: "policy",
            });
        }
        if FailPolicy::parse(&self.fail_policy).is_none() {
            return Err(LadderError::UnknownFailPolicy {
                value: self.fail_policy.clone(),
            });
        }
        if FailAction::parse(&self.action).is_none() {
            return Err(LadderError::UnknownFailAction {
                value: self.action.clone(),
            });
        }
        if let Some(threshold) = self.threshold {
            conveyor_domain::ladder::in_unit_interval("threshold", threshold)?;
        }
        Ok(())
    }
}

/// Insert one policy. A second policy for the same `(kind, gate)` is a conflict, never a
/// silent shadow: the refinement is the tenant's change path, not a second policy.
pub async fn insert_policy_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: &NewPolicy,
) -> Result<PolicyRecord, LadderStoreError> {
    new.validate().map_err(LadderStoreError::Illegal)?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM decision_policy
            WHERE tenant_id = $1 AND kind = $2 AND gate IS NOT DISTINCT FROM $3
         )",
    )
    .bind(tenant.as_uuid())
    .bind(&new.kind)
    .bind(&new.gate)
    .fetch_one(&mut *conn)
    .await?;
    if exists {
        return Err(LadderStoreError::Conflict {
            what: "decision_policy",
        });
    }
    let sql = format!(
        "INSERT INTO decision_policy (tenant_id, kind, gate, threshold, fail_policy, action, meta)
         VALUES ($1, $2, $3, $4::float8::numeric, $5, $6, $7)
         RETURNING {POLICY_COLUMNS}"
    );
    let row = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(&new.kind)
        .bind(&new.gate)
        .bind(new.threshold)
        .bind(&new.fail_policy)
        .bind(&new.action)
        .bind(&new.meta)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row)
}

/// Read one policy.
pub async fn get_policy_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<PolicyRecord>, LadderStoreError> {
    let sql =
        format!("SELECT {POLICY_COLUMNS} FROM decision_policy WHERE tenant_id = $1 AND id = $2");
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// The fields a policy change may set; an absent field keeps the stored value.
#[derive(Clone, Debug, Default)]
pub struct PolicyPatch {
    /// The new threshold in 0–1.
    pub threshold: Option<f64>,
    /// `closed | open`.
    pub fail_policy: Option<String>,
    /// `handover | block | annotate`.
    pub action: Option<String>,
    /// The replacement metadata.
    pub meta: Option<Value>,
}

/// Change a policy's threshold, fail behaviour or action. A policy change is a new version;
/// the caller's `expected` version makes a concurrent change a 412, never a lost update.
pub async fn update_policy_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    patch: &PolicyPatch,
    expected: Option<i64>,
) -> Result<PolicyRecord, LadderStoreError> {
    if let Some(threshold) = patch.threshold {
        conveyor_domain::ladder::in_unit_interval("threshold", threshold)?;
    }
    if let Some(value) = patch.fail_policy.as_deref()
        && FailPolicy::parse(value).is_none()
    {
        return Err(LadderStoreError::Illegal(LadderError::UnknownFailPolicy {
            value: value.to_owned(),
        }));
    }
    if let Some(value) = patch.action.as_deref()
        && FailAction::parse(value).is_none()
    {
        return Err(LadderStoreError::Illegal(LadderError::UnknownFailAction {
            value: value.to_owned(),
        }));
    }
    let current =
        get_policy_tx(&mut *conn, tenant, id)
            .await?
            .ok_or(LadderStoreError::NotFound {
                what: "decision_policy",
            })?;
    if expected.is_some_and(|version| version != current.version) {
        return Err(LadderStoreError::Stale {
            what: "decision_policy",
        });
    }
    let sql = format!(
        "UPDATE decision_policy
         SET threshold = $3::float8::numeric, fail_policy = $4, action = $5, meta = $6,
             version = version + 1, updated_at = now()
         WHERE tenant_id = $1 AND id = $2
         RETURNING {POLICY_COLUMNS}"
    );
    let row = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(patch.threshold.or(current.threshold))
        .bind(patch.fail_policy.as_deref().unwrap_or(&current.fail_policy))
        .bind(patch.action.as_deref().unwrap_or(&current.action))
        .bind(patch.meta.as_ref().unwrap_or(&current.meta))
        .fetch_one(&mut *conn)
        .await?;
    Ok(row)
}

/// List policies, optionally for one kind.
pub async fn list_policies_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kind: Option<&str>,
) -> Result<Vec<PolicyRecord>, LadderStoreError> {
    let sql = format!(
        "SELECT {POLICY_COLUMNS} FROM decision_policy
         WHERE tenant_id = $1 AND ($2::text IS NULL OR kind = $2)
         ORDER BY kind, gate NULLS FIRST, created_at DESC"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(kind)
        .fetch_all(&mut *conn)
        .await?)
}

// ── refinements ──────────────────────────────────────────────────────────────────────────

/// The tenant's override of one policy.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct RefinementRecord {
    /// The row id.
    pub id: Uuid,
    /// The policy it refines.
    pub policy_id: Uuid,
    /// The threshold override in 0–1.
    pub threshold_override: Option<f64>,
    /// A whole-rule override, when the tenant replaces the rule and not only its threshold.
    pub rule_override: Value,
    /// `handover | block | annotate`, when the tenant changes the fail action.
    pub fail_action: Option<String>,
    /// The laya config the refinement pins.
    pub laya_config_id: Option<Uuid>,
    /// The refinement version every decision pins.
    pub version: i64,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last write.
    pub updated_at: DateTime<Utc>,
}

const REFINEMENT_COLUMNS: &str = "id, policy_id, threshold_override::float8 AS threshold_override, \
                                  rule_override, fail_action, laya_config_id, version, created_at, \
                                  updated_at";

/// A refinement to publish.
#[derive(Clone, Debug, Default)]
pub struct NewRefinement {
    /// The threshold override in 0–1.
    pub threshold_override: Option<f64>,
    /// A whole-rule override.
    pub rule_override: Value,
    /// `handover | block | annotate`.
    pub fail_action: Option<String>,
    /// The laya config the refinement pins.
    pub laya_config_id: Option<Uuid>,
}

/// Publish a tenant refinement: create version 1, or bump the existing row.
///
/// The unique index `decision_policy_refinement_one_per_policy` is the contract — a tenant has
/// ONE live refinement per policy, and publishing a new one is a new version of it.
pub async fn publish_refinement_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    policy_id: Uuid,
    new: &NewRefinement,
) -> Result<RefinementRecord, LadderStoreError> {
    if let Some(threshold) = new.threshold_override {
        conveyor_domain::ladder::in_unit_interval("threshold_override", threshold)?;
    }
    if let Some(value) = &new.fail_action
        && FailAction::parse(value).is_none()
    {
        return Err(LadderStoreError::Illegal(LadderError::UnknownFailAction {
            value: value.clone(),
        }));
    }
    let present: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM decision_policy WHERE tenant_id = $1 AND id = $2)",
    )
    .bind(tenant.as_uuid())
    .bind(policy_id)
    .fetch_one(&mut *conn)
    .await?;
    if !present {
        return Err(LadderStoreError::NotFound {
            what: "decision_policy",
        });
    }
    if let Some(config) = new.laya_config_id {
        let config_present: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM laya_inference_config WHERE tenant_id = $1 AND id = $2)",
        )
        .bind(tenant.as_uuid())
        .bind(config)
        .fetch_one(&mut *conn)
        .await?;
        if !config_present {
            return Err(LadderStoreError::NotFound {
                what: "laya_inference_config",
            });
        }
    }
    let sql = format!(
        "INSERT INTO decision_policy_refinement
            (tenant_id, policy_id, threshold_override, rule_override, fail_action, laya_config_id)
         VALUES ($1, $2, $3::float8::numeric, $4, $5, $6)
         ON CONFLICT (tenant_id, policy_id) DO UPDATE
         SET threshold_override = EXCLUDED.threshold_override,
             rule_override = EXCLUDED.rule_override,
             fail_action = EXCLUDED.fail_action,
             laya_config_id = EXCLUDED.laya_config_id,
             version = decision_policy_refinement.version + 1,
             updated_at = now()
         RETURNING {REFINEMENT_COLUMNS}"
    );
    let row = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(policy_id)
        .bind(new.threshold_override)
        .bind(&new.rule_override)
        .bind(&new.fail_action)
        .bind(new.laya_config_id)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row)
}

/// Read the refinement for one policy.
pub async fn get_refinement_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    policy_id: Uuid,
) -> Result<Option<RefinementRecord>, LadderStoreError> {
    let sql = format!(
        "SELECT {REFINEMENT_COLUMNS} FROM decision_policy_refinement
         WHERE tenant_id = $1 AND policy_id = $2"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(policy_id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// List refinements, optionally for one policy.
pub async fn list_refinements_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    policy_id: Option<Uuid>,
) -> Result<Vec<RefinementRecord>, LadderStoreError> {
    let sql = format!(
        "SELECT {REFINEMENT_COLUMNS} FROM decision_policy_refinement
         WHERE tenant_id = $1 AND ($2::uuid IS NULL OR policy_id = $2)
         ORDER BY created_at DESC"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(policy_id)
        .fetch_all(&mut *conn)
        .await?)
}

// ── laya inference config ────────────────────────────────────────────────────────────────

/// One tenant-scoped laya inference config.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct LayaConfigRecord {
    /// The row id.
    pub id: Uuid,
    /// The model reference (e.g. `jev-latest`).
    pub model_ref: String,
    /// The request parameters.
    pub params: Value,
    /// The request timeout.
    pub timeout_ms: i32,
    /// The version every decision pins.
    pub version: i64,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last write.
    pub updated_at: DateTime<Utc>,
}

const LAYA_COLUMNS: &str = "id, model_ref, params, timeout_ms, version, created_at, updated_at";

/// A laya config to create.
#[derive(Clone, Debug)]
pub struct NewLayaConfig {
    /// The model reference.
    pub model_ref: String,
    /// The request parameters.
    pub params: Value,
    /// The request timeout in milliseconds.
    pub timeout_ms: i32,
}

/// Insert one laya inference config.
pub async fn insert_laya_config_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: &NewLayaConfig,
) -> Result<LayaConfigRecord, LadderStoreError> {
    if new.model_ref.trim().is_empty() {
        return Err(LadderStoreError::Illegal(LadderError::FieldMissing {
            field: "model_ref",
            rung: "laya",
        }));
    }
    let sql = format!(
        "INSERT INTO laya_inference_config (tenant_id, model_ref, params, timeout_ms)
         VALUES ($1, $2, $3, $4)
         RETURNING {LAYA_COLUMNS}"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(&new.model_ref)
        .bind(&new.params)
        .bind(new.timeout_ms)
        .fetch_one(&mut *conn)
        .await?)
}

/// Read one laya config.
pub async fn get_laya_config_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<LayaConfigRecord>, LadderStoreError> {
    let sql = format!(
        "SELECT {LAYA_COLUMNS} FROM laya_inference_config WHERE tenant_id = $1 AND id = $2"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// The tenant's latest laya config, the default when a refinement names none.
pub async fn latest_laya_config_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Option<LayaConfigRecord>, LadderStoreError> {
    let sql = format!(
        "SELECT {LAYA_COLUMNS} FROM laya_inference_config
         WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT 1"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .fetch_optional(&mut *conn)
        .await?)
}

/// List a tenant's laya configs.
pub async fn list_laya_configs_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Vec<LayaConfigRecord>, LadderStoreError> {
    let sql = format!(
        "SELECT {LAYA_COLUMNS} FROM laya_inference_config
         WHERE tenant_id = $1 ORDER BY created_at DESC"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .fetch_all(&mut *conn)
        .await?)
}

// ── the effective policy ─────────────────────────────────────────────────────────────────

/// The policy a case resolves to, with the tenant's refinement applied.
#[derive(Clone, Debug)]
pub struct EffectivePolicy {
    /// The platform policy.
    pub policy: PolicyRecord,
    /// The tenant's refinement, when one exists.
    pub refinement: Option<RefinementRecord>,
    /// The laya config in force (the refinement's, else the tenant's latest).
    pub laya_config: Option<LayaConfigRecord>,
}

impl EffectivePolicy {
    /// The threshold in force: the refinement's override, else the policy's.
    pub fn threshold(&self) -> Option<f64> {
        self.refinement
            .as_ref()
            .and_then(|refinement| refinement.threshold_override)
            .or(self.policy.threshold)
    }

    /// The fail action in force.
    pub fn action(&self) -> Result<FailAction, LadderStoreError> {
        let value = self
            .refinement
            .as_ref()
            .and_then(|refinement| refinement.fail_action.as_deref())
            .unwrap_or(&self.policy.action);
        FailAction::parse(value).ok_or_else(|| LadderStoreError::Vocabulary {
            what: "decision policy action",
            value: value.to_owned(),
        })
    }

    /// The fail policy in force.
    pub fn fail_policy(&self) -> Result<FailPolicy, LadderStoreError> {
        FailPolicy::parse(&self.policy.fail_policy).ok_or_else(|| LadderStoreError::Vocabulary {
            what: "decision policy fail_policy",
            value: self.policy.fail_policy.clone(),
        })
    }

    /// The version pins a decision recorded from this policy carries.
    pub fn pins(&self) -> Pins {
        Pins {
            policy_version: Some(self.policy.version),
            refinement_version: self.refinement.as_ref().map(|row| row.version),
            laya_config_version: self.laya_config.as_ref().map(|row| row.version),
        }
    }

    /// Resolve the policy's defaults into the domain's refusal vocabulary.
    pub fn validate_vocabulary(&self) -> Result<(), LadderStoreError> {
        self.action()?;
        self.fail_policy()?;
        Ok(())
    }
}

/// Resolve the policy for `kind` (preferring a gate-specific row) and the tenant's refinement.
pub async fn effective_policy_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kind: &str,
    gate: Option<&str>,
) -> Result<Option<EffectivePolicy>, LadderStoreError> {
    let sql = format!(
        "SELECT {POLICY_COLUMNS} FROM decision_policy
         WHERE tenant_id = $1 AND kind = $2 AND (gate = $3::text OR gate IS NULL)
         ORDER BY (gate = $3::text) DESC NULLS LAST, created_at DESC
         LIMIT 1"
    );
    let Some(policy) = sqlx::query_as::<_, PolicyRecord>(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(kind)
        .bind(gate)
        .fetch_optional(&mut *conn)
        .await?
    else {
        return Ok(None);
    };
    let refinement = get_refinement_tx(&mut *conn, tenant, policy.id).await?;
    let laya_config = match refinement
        .as_ref()
        .and_then(|refinement| refinement.laya_config_id)
    {
        Some(id) => get_laya_config_tx(&mut *conn, tenant, id).await?,
        None => latest_laya_config_tx(&mut *conn, tenant).await?,
    };
    let effective = EffectivePolicy {
        policy,
        refinement,
        laya_config,
    };
    effective.validate_vocabulary()?;
    Ok(Some(effective))
}

// ── decisions ────────────────────────────────────────────────────────────────────────────

/// One authored option recorded with a decision.
#[derive(Clone, Debug)]
pub struct NewOption {
    /// The option key the answer names.
    pub key: String,
    /// The human label.
    pub label: String,
    /// The option's payload.
    pub payload: Value,
}

/// A decision to record.
#[derive(Clone, Debug)]
pub struct NewDecision {
    /// The case kind.
    pub kind: String,
    /// Which rung made it.
    pub rung: Rung,
    /// The project the decision belongs to.
    pub project_id: Option<Uuid>,
    /// The card the decision belongs to.
    pub card_id: Option<Uuid>,
    /// The question, in the case's terms.
    pub question: String,
    /// The scalar answer (a `noul` value, a chosen option key, or the human verdict).
    pub answer: String,
    /// The authored option set's version.
    pub option_set_version: Option<i64>,
    /// The probabilities the question type provides.
    pub probabilities: Value,
    /// The confidence in 0–1.
    pub confidence: Option<f64>,
    /// The sha256 of the inputs the answer consumed.
    pub input_hash: Option<String>,
    /// The policy the decision enforced.
    pub policy_id: Option<Uuid>,
    /// The policy version it read.
    pub policy_version: Option<i64>,
    /// The refinement applied, if any.
    pub policy_refinement_id: Option<Uuid>,
    /// The refinement version it read.
    pub refinement_version: Option<i64>,
    /// The laya config used, if any.
    pub laya_config_id: Option<Uuid>,
    /// The laya config version it read.
    pub laya_config_version: Option<i64>,
    /// `system | user | agent | service`.
    pub actor_kind: String,
    /// The acting principal.
    pub actor_id: Option<Uuid>,
    /// The approval a human decision records.
    pub approval_id: Option<Uuid>,
    /// The evaluation latency.
    pub latency_ms: Option<i32>,
    /// What the ladder concluded.
    pub outcome: Outcome,
    /// When the decision was made; `None` while an escalation is pending.
    pub decided_at: Option<DateTime<Utc>>,
    /// Free-form metadata.
    pub meta: Value,
    /// The authored options, recorded as `decision_option` rows.
    pub options: Vec<NewOption>,
}

/// One decision row as the actions read it.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct DecisionRecord {
    /// The row id.
    pub id: Uuid,
    /// The case kind.
    pub kind: String,
    /// `rule | laya | human`.
    pub rung: String,
    /// The project, if any.
    pub project_id: Option<Uuid>,
    /// The card, if any.
    pub card_id: Option<Uuid>,
    /// The question.
    pub question: String,
    /// The answer.
    pub answer: Option<String>,
    /// The authored option set's version.
    pub option_set_version: Option<i64>,
    /// The probabilities.
    pub probabilities: Value,
    /// The confidence.
    pub confidence: Option<f64>,
    /// The policy id.
    pub policy_id: Option<Uuid>,
    /// The policy version pinned.
    pub policy_version: Option<i64>,
    /// The refinement id.
    pub policy_refinement_id: Option<Uuid>,
    /// The refinement version pinned.
    pub refinement_version: Option<i64>,
    /// The laya config id.
    pub laya_config_id: Option<Uuid>,
    /// The laya config version pinned.
    pub laya_config_version: Option<i64>,
    /// `system | user | agent | service`.
    pub actor_kind: String,
    /// The acting principal.
    pub actor_id: Option<Uuid>,
    /// The approval a human decision records.
    pub approval_id: Option<Uuid>,
    /// The input hash.
    pub input_hash: Option<String>,
    /// The evaluation latency.
    pub latency_ms: Option<i32>,
    /// `accepted | refused | escalated`.
    pub outcome: Option<String>,
    /// When the decision was made.
    pub decided_at: Option<DateTime<Utc>>,
    /// Free-form metadata.
    pub meta: Value,
    /// The If-Match version.
    pub version: i64,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}

const DECISION_COLUMNS: &str = "id, kind, rung, project_id, card_id, question, answer, \
    option_set_version, probabilities, confidence::float8 AS confidence, policy_id, \
    policy_version, policy_refinement_id, refinement_version, laya_config_id, \
    laya_config_version, actor_kind, actor_id, approval_id, input_hash, latency_ms, outcome, \
    decided_at, meta, version, created_at";

/// Record one decision and its authored options in the caller's transaction.
pub async fn record_decision_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: &NewDecision,
) -> Result<DecisionRecord, LadderStoreError> {
    if new.answer.trim().is_empty() {
        return Err(LadderStoreError::Illegal(LadderError::AnswerMissing));
    }
    if let Some(confidence) = new.confidence {
        conveyor_domain::ladder::in_unit_interval("confidence", confidence)?;
    }
    let pins = Pins {
        policy_version: new.policy_version,
        refinement_version: new.refinement_version,
        laya_config_version: new.laya_config_version,
    };
    let missing = missing_pins(new.rung, &pins, new.approval_id);
    if !missing.is_empty() {
        return Err(LadderStoreError::Illegal(LadderError::PinsIncomplete {
            rung: new.rung.as_str(),
            missing,
        }));
    }
    if new.rung.requires_reproducibility() {
        let Some(hash) = new.input_hash.as_deref() else {
            return Err(LadderStoreError::Illegal(LadderError::FieldMissing {
                field: "input_hash",
                rung: new.rung.as_str(),
            }));
        };
        if !is_input_hash(hash) {
            return Err(LadderStoreError::Illegal(LadderError::InputHashShape));
        }
    } else if let Some(hash) = new.input_hash.as_deref()
        && !is_input_hash(hash)
    {
        return Err(LadderStoreError::Illegal(LadderError::InputHashShape));
    }
    ensure_optional_parent_tx(&mut *conn, tenant, Parent::Project, new.project_id)
        .await
        .map_err(LadderStoreError::Parent)?;
    ensure_optional_parent_tx(&mut *conn, tenant, Parent::Card, new.card_id)
        .await
        .map_err(LadderStoreError::Parent)?;
    ensure_optional_parent_tx(&mut *conn, tenant, Parent::Approval, new.approval_id)
        .await
        .map_err(LadderStoreError::Parent)?;

    // The L3 human rung is a person's act, and the approval it cites must actually have resolved
    // in its favour. This lives in the one writer every decision path traverses, so
    // `ladder.record`, the `approvals.decide` path and a later caller cannot disagree
    // (SPEC_V2 §7.1; rework `v1-w1c-2:T2`).
    if new.rung == Rung::Human {
        if new.actor_kind != "user" {
            return Err(LadderStoreError::HumanRequiresPerson);
        }
        if let Some(approval_id) = new.approval_id {
            let Some((approval_card, approved)) =
                crate::approval::approved_gate_tx(&mut *conn, tenant, approval_id).await?
            else {
                return Err(LadderStoreError::NotFound { what: "approval" });
            };
            if !approved {
                return Err(LadderStoreError::ApprovalNotAccepted);
            }
            if let (Some(decision_card), Some(approval_card)) = (new.card_id, approval_card)
                && decision_card != approval_card
            {
                return Err(LadderStoreError::ApprovalWrongCard);
            }
        }
    }

    let sql = format!(
        "INSERT INTO decision
            (tenant_id, kind, rung, project_id, card_id, question, answer, option_set_version,
             probabilities, confidence, policy_id, policy_version, policy_refinement_id,
             refinement_version, laya_config_id, laya_config_version, actor_kind, actor_id,
             approval_id, input_hash, latency_ms, outcome, decided_at, meta)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10::float8::numeric,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24)
         RETURNING {DECISION_COLUMNS}"
    );
    let row: DecisionRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(&new.kind)
        .bind(new.rung.as_str())
        .bind(new.project_id)
        .bind(new.card_id)
        .bind(&new.question)
        .bind(&new.answer)
        .bind(new.option_set_version)
        .bind(&new.probabilities)
        .bind(new.confidence)
        .bind(new.policy_id)
        .bind(new.policy_version)
        .bind(new.policy_refinement_id)
        .bind(new.refinement_version)
        .bind(new.laya_config_id)
        .bind(new.laya_config_version)
        .bind(&new.actor_kind)
        .bind(new.actor_id)
        .bind(new.approval_id)
        .bind(&new.input_hash)
        .bind(new.latency_ms)
        .bind(new.outcome.as_str())
        .bind(new.decided_at)
        .bind(&new.meta)
        .fetch_one(&mut *conn)
        .await?;
    for option in &new.options {
        sqlx::query(
            "INSERT INTO decision_option (tenant_id, decision_id, key, label, payload)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(tenant.as_uuid())
        .bind(row.id)
        .bind(&option.key)
        .bind(&option.label)
        .bind(&option.payload)
        .execute(&mut *conn)
        .await?;
    }
    Ok(row)
}

/// Read one decision.
pub async fn get_decision_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<DecisionRecord>, LadderStoreError> {
    let sql = format!("SELECT {DECISION_COLUMNS} FROM decision WHERE tenant_id = $1 AND id = $2");
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// List decisions, newest first, filtered by the case and the row they belong to.
pub async fn list_decisions_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kind: Option<&str>,
    project_id: Option<Uuid>,
    card_id: Option<Uuid>,
    limit: i64,
) -> Result<Vec<DecisionRecord>, LadderStoreError> {
    let sql = format!(
        "SELECT {DECISION_COLUMNS} FROM decision
         WHERE tenant_id = $1
           AND ($2::text IS NULL OR kind = $2)
           AND ($3::uuid IS NULL OR project_id = $3)
           AND ($4::uuid IS NULL OR card_id = $4)
         ORDER BY created_at DESC, id DESC
         LIMIT $5"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(kind)
        .bind(project_id)
        .bind(card_id)
        .bind(limit.clamp(1, 200))
        .fetch_all(&mut *conn)
        .await?)
}
