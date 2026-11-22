#![forbid(unsafe_code)]
//! T2 — the decision ladder's queries (SPEC_V2 §7, §10.3, §12 T2; issue #73).
//!
//! Every function is `_tx`: the caller has already opened a tenant-scoped transaction, and the
//! write shares it with the action's outbox event (CONTRACT §7). No function here speaks the
//! action layer's vocabulary; a refusal the domain owns is raised as
//! [`LadderStoreError::Illegal`] and mapped once, by the action.
//!
//! The policy read model is [`effective_policy_tx`]: a platform `decision_policy` is the case;
//! the tenant's `decision_policy_refinement` overrides its threshold, fail action and
//! (optionally) its laya config; the refinement pins the policy content it was authored
//! against, and the decision pins the three versions it read, so the row is reproducible after
//! any of them moves (§7.2).
//!
//! The option read model is [`effective_option_set_tx`]: a published tenant
//! `decision_option_set` shadows the library set for the same `(kind, gate)`; otherwise the
//! library applies and the tenant's `decision_option_refinement` rows override individual
//! entries. Each refinement pins the base entry's canonical hash; a changed base is refused.
//! A decision pins the set id, version, effective hash and every applied refinement hash, and
//! its `decision_option` rows are the immutable snapshot of what it offered.

use std::collections::{BTreeSet, HashMap};

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use conveyor_domain::ladder::{
    FailAction, FailPolicy, LadderError, MAX_OPTIONS, MIN_OPTIONS, OptionOrigin, OptionPin,
    Outcome, Pins, Rung, is_input_hash, missing_pins,
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
    /// A platform-authored row cannot be changed through a tenant action.
    #[error("the {what} is authored by the platform, not this tenant")]
    ReadOnly {
        /// The row's noun.
        what: &'static str,
    },
    /// A tenant set's option is edited through the set, not refined.
    #[error("{key:?} belongs to a tenant-authored set; edit the set rather than refine it")]
    NotRefinable {
        /// The option key.
        key: String,
    },
    /// A refinement was authored against content that has since changed.
    #[error("the {what} {key} changed since the refinement pinned it")]
    BaseMismatch {
        /// The base row's noun.
        what: &'static str,
        /// The base row's key.
        key: String,
    },
    /// A referenced parent does not exist in this tenant.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The domain refused the write.
    #[error(transparent)]
    Illegal(#[from] LadderError),
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
    /// The canonical hash of the policy content this refinement was authored against.
    ///
    /// `None` is the legacy shape: a row published before 0070. Every refinement published
    /// since pins the hash, and a policy edit that changes the content refuses the stale row.
    pub base_hash: Option<String>,
    /// The refinement version every decision pins.
    pub version: i64,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last write.
    pub updated_at: DateTime<Utc>,
}

const REFINEMENT_COLUMNS: &str = "id, policy_id, threshold_override::float8 AS threshold_override, \
                                  rule_override, fail_action, laya_config_id, base_hash, version, \
                                  created_at, updated_at";

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
    let policy =
        get_policy_tx(&mut *conn, tenant, policy_id)
            .await?
            .ok_or(LadderStoreError::NotFound {
                what: "decision_policy",
            })?;
    // The refinement pins the policy content it was authored against. A later policy edit
    // changes the hash and the refinement is refused at resolution, never silently applied to
    // a different threshold.
    let base_hash = policy_base_hash(&policy);
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
            (tenant_id, policy_id, threshold_override, rule_override, fail_action, laya_config_id,
             base_hash)
         VALUES ($1, $2, $3::float8::numeric, $4, $5, $6, $7)
         ON CONFLICT (tenant_id, policy_id) DO UPDATE
         SET threshold_override = EXCLUDED.threshold_override,
             rule_override = EXCLUDED.rule_override,
             fail_action = EXCLUDED.fail_action,
             laya_config_id = EXCLUDED.laya_config_id,
             base_hash = EXCLUDED.base_hash,
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
        .bind(&base_hash)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row)
}

/// The canonical hash of a policy's base content (see [`conveyor_domain::ladder::policy_base_hash`]).
fn policy_base_hash(policy: &PolicyRecord) -> String {
    conveyor_domain::ladder::policy_base_hash(
        &policy.kind,
        policy.gate.as_deref(),
        policy.threshold,
        &policy.fail_policy,
        &policy.action,
        &policy.meta,
    )
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

// ── authored option sets ─────────────────────────────────────────────────────────────────

/// One authored option set: a case's base (library) or the tenant's shadow of it.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct OptionSetRecord {
    /// The row id.
    pub id: Uuid,
    /// The case kind.
    pub kind: String,
    /// The gate the set is specific to, if any.
    pub gate: Option<String>,
    /// `library | tenant`.
    pub origin: String,
    /// `draft | published`; only a published set is resolvable.
    pub status: String,
    /// The authored revision every decision pins.
    pub version: i64,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last write.
    pub updated_at: DateTime<Utc>,
}

const OPTION_SET_COLUMNS: &str = "id, kind, gate, origin, status, version, created_at, updated_at";

/// One member of an authored set.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct OptionEntryRecord {
    /// The row id; a refinement names it.
    pub id: Uuid,
    /// The set it belongs to.
    pub set_id: Uuid,
    /// The option key the answer must equal.
    pub key: String,
    /// The human label.
    pub label: String,
    /// The description the laya wire reads.
    pub criteria: String,
    /// The authored order inside the set.
    pub position: i32,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last write.
    pub updated_at: DateTime<Utc>,
}

const OPTION_ENTRY_COLUMNS: &str =
    "id, set_id, key, label, criteria, position, created_at, updated_at";

/// The tenant's refinement of one library entry.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct OptionRefinementRecord {
    /// The row id.
    pub id: Uuid,
    /// The library entry it refines.
    pub entry_id: Uuid,
    /// The effective label, when the tenant changed it.
    pub label: Option<String>,
    /// The effective wire description, when the tenant changed it.
    pub criteria: Option<String>,
    /// The effective authored order, when the tenant changed it.
    pub position: Option<i32>,
    /// Whether the option is withheld from the wire.
    pub hidden: bool,
    /// The canonical hash of the base entry this refinement was authored against.
    pub base_hash: String,
    /// The refinement version.
    pub version: i64,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last write.
    pub updated_at: DateTime<Utc>,
}

const OPTION_REFINEMENT_COLUMNS: &str = "id, entry_id, label, criteria, position, hidden, \
                                         base_hash, version, created_at, updated_at";

/// One entry of a set the caller is authoring.
#[derive(Clone, Debug)]
pub struct NewOptionEntry {
    /// The option key.
    pub key: String,
    /// The human label.
    pub label: String,
    /// The wire description.
    pub criteria: String,
    /// The authored order; 1-based and unique inside the set.
    pub position: i32,
}

/// A set to author.
#[derive(Clone, Debug)]
pub struct NewOptionSet {
    /// The case kind.
    pub kind: String,
    /// The gate, when the set is gate-specific.
    pub gate: Option<String>,
    /// `library | tenant`; a tenant action always writes `tenant`, the platform seed `library`.
    pub origin: String,
    /// The set's members.
    pub entries: Vec<NewOptionEntry>,
}

/// The fields a refinement may set; an absent field keeps the stored refinement, and a value
/// equal to the base reverts that field.
#[derive(Clone, Debug, Default)]
pub struct OptionRefinePatch {
    /// The replacement label.
    pub label: Option<String>,
    /// The replacement wire description.
    pub criteria: Option<String>,
    /// The replacement position.
    pub position: Option<i32>,
    /// Whether the option is withheld from the wire.
    pub hidden: Option<bool>,
}

/// One library entry with the set it belongs to.
#[derive(Clone, Debug)]
pub struct OptionEntryContext {
    /// The base entry.
    pub entry: OptionEntryRecord,
    /// The set that owns it; `origin` decides whether it is refinable.
    pub set: OptionSetRecord,
}

/// One effective option: the base entry with the tenant's refinement applied.
#[derive(Clone, Debug)]
pub struct EffectiveOption {
    /// The base entry id, so a refine action can name it.
    pub entry_id: Uuid,
    /// The option key.
    pub key: String,
    /// The effective label.
    pub label: String,
    /// The effective wire description.
    pub criteria: String,
    /// The effective authored order.
    pub position: i32,
    /// Whether the option is withheld from the wire.
    pub hidden: bool,
    /// The canonical hash of the base entry.
    pub base_hash: String,
    /// The refinement's pinned base hash, when one applied.
    pub refinement_hash: Option<String>,
    /// Whether a refinement applied.
    pub refined: bool,
}

impl EffectiveOption {
    /// The description the laya wire reads: the criteria, else the label, else the key.
    pub fn wire_label(&self) -> &str {
        if !self.criteria.trim().is_empty() {
            &self.criteria
        } else if !self.label.trim().is_empty() {
            &self.label
        } else {
            &self.key
        }
    }
}

/// The set a case resolves to, after library-first/tenant-shadow selection and refinements.
#[derive(Clone, Debug)]
pub struct EffectiveOptionSet {
    /// The set row every decision pins.
    pub id: Uuid,
    /// The case kind.
    pub kind: String,
    /// The gate, when the set is gate-specific.
    pub gate: Option<String>,
    /// `library | tenant`.
    pub origin: String,
    /// The set version every decision pins.
    pub version: i64,
    /// Every member, hidden ones flagged. The wire carries only [`Self::visible`].
    pub options: Vec<EffectiveOption>,
}

impl EffectiveOptionSet {
    /// The options the wire may offer.
    pub fn visible(&self) -> Vec<&EffectiveOption> {
        self.options
            .iter()
            .filter(|option| !option.hidden)
            .collect()
    }

    /// The base hash of every refinement applied, sorted and de-duplicated.
    pub fn refinement_hashes(&self) -> Vec<String> {
        let mut hashes: Vec<String> = self
            .options
            .iter()
            .filter_map(|option| option.refinement_hash.clone())
            .collect();
        hashes.sort();
        hashes.dedup();
        hashes
    }

    /// The canonical hash of the effective visible options, pinned on a decision.
    pub fn content_hash(&self) -> String {
        let pins: Vec<OptionPin<'_>> = self
            .options
            .iter()
            .filter(|option| !option.hidden)
            .map(|option| OptionPin {
                key: &option.key,
                label: &option.label,
                criteria: &option.criteria,
                position: option.position,
                base_hash: option
                    .refinement_hash
                    .as_deref()
                    .unwrap_or(&option.base_hash),
            })
            .collect();
        conveyor_domain::ladder::option_set_hash(&self.kind, self.gate.as_deref(), &pins)
    }
}

/// Refuse a set laya cannot ask: 2–255 members, unique keys and unique 1-based positions.
fn validate_option_entries(entries: &[NewOptionEntry]) -> Result<(), LadderError> {
    if !(MIN_OPTIONS..=MAX_OPTIONS).contains(&entries.len()) {
        return Err(LadderError::OptionSetSize {
            count: entries.len(),
        });
    }
    let mut keys = BTreeSet::new();
    let mut positions = BTreeSet::new();
    for entry in entries {
        let key = entry.key.trim();
        if key.is_empty() {
            return Err(LadderError::FieldMissing {
                field: "key",
                rung: "option set",
            });
        }
        if !keys.insert(key.to_owned()) {
            return Err(LadderError::DuplicateOption {
                key: key.to_owned(),
            });
        }
        if entry.position < 1 {
            return Err(LadderError::InvalidPosition);
        }
        if !positions.insert(entry.position) {
            return Err(LadderError::DuplicatePosition {
                position: entry.position,
            });
        }
    }
    Ok(())
}

/// Insert one entry and return the stored row.
async fn insert_option_entry_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    set_id: Uuid,
    entry: &NewOptionEntry,
) -> Result<OptionEntryRecord, LadderStoreError> {
    let sql = format!(
        "INSERT INTO decision_option_entry (tenant_id, set_id, key, label, criteria, position)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING {OPTION_ENTRY_COLUMNS}"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(set_id)
        .bind(entry.key.trim())
        .bind(&entry.label)
        .bind(&entry.criteria)
        .bind(entry.position)
        .fetch_one(&mut *conn)
        .await?)
}

/// Author one set. A second set for the same `(kind, gate, origin)` is a conflict: the
/// tenant's shadow is one row, and a change is a new version of it.
pub async fn insert_option_set_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    new: &NewOptionSet,
) -> Result<(OptionSetRecord, Vec<OptionEntryRecord>), LadderStoreError> {
    let kind = new.kind.trim();
    if kind.is_empty() {
        return Err(LadderStoreError::Illegal(LadderError::FieldMissing {
            field: "kind",
            rung: "option set",
        }));
    }
    if OptionOrigin::parse(&new.origin).is_none() {
        return Err(LadderStoreError::Illegal(
            LadderError::UnknownOptionOrigin {
                value: new.origin.clone(),
            },
        ));
    }
    validate_option_entries(&new.entries)?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM decision_option_set
            WHERE tenant_id = $1 AND kind = $2 AND gate IS NOT DISTINCT FROM $3::text
              AND origin = $4
         )",
    )
    .bind(tenant.as_uuid())
    .bind(kind)
    .bind(&new.gate)
    .bind(&new.origin)
    .fetch_one(&mut *conn)
    .await?;
    if exists {
        return Err(LadderStoreError::Conflict {
            what: "decision_option_set",
        });
    }
    let sql = format!(
        "INSERT INTO decision_option_set (tenant_id, kind, gate, origin)
         VALUES ($1, $2, $3, $4)
         RETURNING {OPTION_SET_COLUMNS}"
    );
    let set: OptionSetRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(kind)
        .bind(&new.gate)
        .bind(&new.origin)
        .fetch_one(&mut *conn)
        .await?;
    let mut entries = Vec::with_capacity(new.entries.len());
    for entry in &new.entries {
        entries.push(insert_option_entry_tx(&mut *conn, tenant, set.id, entry).await?);
    }
    Ok((set, entries))
}

/// Read one set.
pub async fn get_option_set_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<OptionSetRecord>, LadderStoreError> {
    let sql = format!(
        "SELECT {OPTION_SET_COLUMNS} FROM decision_option_set WHERE tenant_id = $1 AND id = $2"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// Read one set's entries in authored order.
pub async fn option_entries_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    set_id: Uuid,
) -> Result<Vec<OptionEntryRecord>, LadderStoreError> {
    let sql = format!(
        "SELECT {OPTION_ENTRY_COLUMNS} FROM decision_option_entry
         WHERE tenant_id = $1 AND set_id = $2 ORDER BY position, key"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(set_id)
        .fetch_all(&mut *conn)
        .await?)
}

/// Replace a tenant set's entries. A platform library set is authored by the seed, not by a
/// tenant action, so it is read-only here.
pub async fn update_option_set_entries_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    entries: &[NewOptionEntry],
    expected: Option<i64>,
) -> Result<(OptionSetRecord, Vec<OptionEntryRecord>), LadderStoreError> {
    let current =
        get_option_set_tx(&mut *conn, tenant, id)
            .await?
            .ok_or(LadderStoreError::NotFound {
                what: "decision_option_set",
            })?;
    if current.origin != OptionOrigin::Tenant.as_str() {
        return Err(LadderStoreError::ReadOnly {
            what: "platform option set",
        });
    }
    if expected.is_some_and(|version| version != current.version) {
        return Err(LadderStoreError::Stale {
            what: "decision_option_set",
        });
    }
    validate_option_entries(entries)?;
    sqlx::query("DELETE FROM decision_option_entry WHERE tenant_id = $1 AND set_id = $2")
        .bind(tenant.as_uuid())
        .bind(id)
        .execute(&mut *conn)
        .await?;
    let sql = format!(
        "UPDATE decision_option_set
         SET version = version + 1, updated_at = now()
         WHERE tenant_id = $1 AND id = $2
         RETURNING {OPTION_SET_COLUMNS}"
    );
    let set: OptionSetRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;
    let mut stored = Vec::with_capacity(entries.len());
    for entry in entries {
        stored.push(insert_option_entry_tx(&mut *conn, tenant, set.id, entry).await?);
    }
    Ok((set, stored))
}

/// Publish a set. Publishing an already published set is a no-op: the row is returned as it
/// stands (the caller's precondition already refused a concurrent change).
pub async fn publish_option_set_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected: Option<i64>,
) -> Result<(OptionSetRecord, Vec<OptionEntryRecord>), LadderStoreError> {
    let current =
        get_option_set_tx(&mut *conn, tenant, id)
            .await?
            .ok_or(LadderStoreError::NotFound {
                what: "decision_option_set",
            })?;
    if expected.is_some_and(|version| version != current.version) {
        return Err(LadderStoreError::Stale {
            what: "decision_option_set",
        });
    }
    if current.status == "published" {
        let entries = option_entries_tx(&mut *conn, tenant, id).await?;
        return Ok((current, entries));
    }
    let sql = format!(
        "UPDATE decision_option_set
         SET status = 'published', version = version + 1, updated_at = now()
         WHERE tenant_id = $1 AND id = $2
         RETURNING {OPTION_SET_COLUMNS}"
    );
    let set: OptionSetRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;
    let entries = option_entries_tx(&mut *conn, tenant, set.id).await?;
    Ok((set, entries))
}

/// Resolve the effective option set for a case.
///
/// A published tenant set shadows the library for the same `(kind, gate)`; otherwise the
/// published library set applies. A library entry is refined per tenant, with the refinement's
/// `base_hash` checked against the entry it read. `None` means the case is a `score` question:
/// no authored finite choice exists.
pub async fn effective_option_set_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kind: &str,
    gate: Option<&str>,
) -> Result<Option<EffectiveOptionSet>, LadderStoreError> {
    let sql = format!(
        "SELECT {OPTION_SET_COLUMNS} FROM decision_option_set
         WHERE tenant_id = $1 AND kind = $2 AND gate IS NOT DISTINCT FROM $3::text
           AND status = 'published'
         ORDER BY (origin = 'tenant') DESC, version DESC, created_at DESC
         LIMIT 1"
    );
    let Some(set) = sqlx::query_as::<_, OptionSetRecord>(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(kind)
        .bind(gate)
        .fetch_optional(&mut *conn)
        .await?
    else {
        return Ok(None);
    };
    let entries = option_entries_tx(&mut *conn, tenant, set.id).await?;
    let refinements: HashMap<Uuid, OptionRefinementRecord> =
        if set.origin == OptionOrigin::Library.as_str() {
            let ids: Vec<Uuid> = entries.iter().map(|entry| entry.id).collect();
            let sql = format!(
                "SELECT {OPTION_REFINEMENT_COLUMNS} FROM decision_option_refinement
                 WHERE tenant_id = $1 AND entry_id = ANY($2)"
            );
            let rows: Vec<OptionRefinementRecord> = sqlx::query_as(AssertSqlSafe(sql))
                .bind(tenant.as_uuid())
                .bind(ids)
                .fetch_all(&mut *conn)
                .await?;
            rows.into_iter().map(|row| (row.entry_id, row)).collect()
        } else {
            HashMap::new()
        };
    let mut options = Vec::with_capacity(entries.len());
    for entry in entries {
        let base_hash = conveyor_domain::ladder::option_base_hash(
            &set.kind,
            set.gate.as_deref(),
            &entry.key,
            &entry.label,
            &entry.criteria,
            entry.position,
        );
        let mut option = EffectiveOption {
            entry_id: entry.id,
            key: entry.key.clone(),
            label: entry.label.clone(),
            criteria: entry.criteria.clone(),
            position: entry.position,
            hidden: false,
            base_hash: base_hash.clone(),
            refinement_hash: None,
            refined: false,
        };
        if let Some(refinement) = refinements.get(&entry.id) {
            // A refinement is valid only against the base it pinned. The library changed
            // underneath it: refuse, never re-apply it to different content.
            if refinement.base_hash != base_hash {
                return Err(LadderStoreError::BaseMismatch {
                    what: "decision_option",
                    key: entry.key.clone(),
                });
            }
            option.label = refinement.label.clone().unwrap_or(entry.label);
            option.criteria = refinement.criteria.clone().unwrap_or(entry.criteria);
            option.position = refinement.position.unwrap_or(entry.position);
            option.hidden = refinement.hidden;
            option.refinement_hash = Some(refinement.base_hash.clone());
            option.refined = true;
        }
        options.push(option);
    }
    options.sort_by(|left, right| {
        left.position
            .cmp(&right.position)
            .then_with(|| left.key.cmp(&right.key))
    });
    let visible = options.iter().filter(|option| !option.hidden).count();
    if !(MIN_OPTIONS..=MAX_OPTIONS).contains(&visible) {
        return Err(LadderStoreError::Illegal(LadderError::OptionSetSize {
            count: visible,
        }));
    }
    Ok(Some(EffectiveOptionSet {
        id: set.id,
        kind: set.kind,
        gate: set.gate,
        origin: set.origin,
        version: set.version,
        options,
    }))
}

/// Read one entry with the set that owns it.
pub async fn option_entry_context_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    entry_id: Uuid,
) -> Result<Option<OptionEntryContext>, LadderStoreError> {
    let sql = format!(
        "SELECT {OPTION_ENTRY_COLUMNS} FROM decision_option_entry
         WHERE tenant_id = $1 AND id = $2"
    );
    let Some(entry) = sqlx::query_as::<_, OptionEntryRecord>(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(entry_id)
        .fetch_optional(&mut *conn)
        .await?
    else {
        return Ok(None);
    };
    let set = get_option_set_tx(&mut *conn, tenant, entry.set_id)
        .await?
        .ok_or(LadderStoreError::NotFound {
            what: "decision_option_set",
        })?;
    Ok(Some(OptionEntryContext { entry, set }))
}

/// Read the tenant's refinement of one entry.
pub async fn get_option_refinement_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    entry_id: Uuid,
) -> Result<Option<OptionRefinementRecord>, LadderStoreError> {
    let sql = format!(
        "SELECT {OPTION_REFINEMENT_COLUMNS} FROM decision_option_refinement
         WHERE tenant_id = $1 AND entry_id = $2"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(entry_id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// A patch value's next state: absent keeps the stored refinement; blank or equal to the base
/// reverts the field; anything else becomes the refinement.
fn refined_text(patch: Option<&str>, base: &str, current: Option<String>) -> Option<String> {
    match patch {
        None => current,
        Some(value) => {
            let value = value.trim();
            if value.is_empty() || value == base {
                None
            } else {
                Some(value.to_owned())
            }
        }
    }
}

/// Refine one library entry, or revert the refinement when every field is back to the base.
///
/// Hiding is refused when it would leave fewer than two visible options. A refinement of a
/// tenant-authored set is refused: the tenant edits its own set instead.
pub async fn refine_option_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    context: &OptionEntryContext,
    patch: &OptionRefinePatch,
) -> Result<Option<OptionRefinementRecord>, LadderStoreError> {
    if context.set.origin != OptionOrigin::Library.as_str() {
        return Err(LadderStoreError::NotRefinable {
            key: context.entry.key.clone(),
        });
    }
    let base_hash = conveyor_domain::ladder::option_base_hash(
        &context.set.kind,
        context.set.gate.as_deref(),
        &context.entry.key,
        &context.entry.label,
        &context.entry.criteria,
        context.entry.position,
    );
    let current = get_option_refinement_tx(&mut *conn, tenant, context.entry.id).await?;
    let label = refined_text(
        patch.label.as_deref(),
        &context.entry.label,
        current.as_ref().and_then(|row| row.label.clone()),
    );
    let criteria = refined_text(
        patch.criteria.as_deref(),
        &context.entry.criteria,
        current.as_ref().and_then(|row| row.criteria.clone()),
    );
    let position = match patch.position {
        None => current.as_ref().and_then(|row| row.position),
        Some(value) => {
            if value < 1 {
                return Err(LadderStoreError::Illegal(LadderError::InvalidPosition));
            }
            (value != context.entry.position).then_some(value)
        }
    };
    let hidden = patch
        .hidden
        .unwrap_or_else(|| current.as_ref().is_some_and(|row| row.hidden));
    if label.is_none() && criteria.is_none() && position.is_none() && !hidden {
        sqlx::query(
            "DELETE FROM decision_option_refinement WHERE tenant_id = $1 AND entry_id = $2",
        )
        .bind(tenant.as_uuid())
        .bind(context.entry.id)
        .execute(&mut *conn)
        .await?;
        return Ok(None);
    }
    if hidden {
        // The platform's floor: a question keeps at least two visible options.
        let visible_others: i64 = sqlx::query_scalar(
            "SELECT count(*)
               FROM decision_option_entry e
               LEFT JOIN decision_option_refinement r
                 ON r.entry_id = e.id AND r.tenant_id = $1
              WHERE e.tenant_id = $1 AND e.set_id = $2 AND e.id <> $3
                AND NOT COALESCE(r.hidden, false)",
        )
        .bind(tenant.as_uuid())
        .bind(context.set.id)
        .bind(context.entry.id)
        .fetch_one(&mut *conn)
        .await?;
        let floor = i64::try_from(MIN_OPTIONS).unwrap_or(2);
        if visible_others < floor {
            return Err(LadderStoreError::Illegal(LadderError::TooFewOptions {
                visible: usize::try_from(visible_others).unwrap_or(0),
            }));
        }
    }
    let sql = format!(
        "INSERT INTO decision_option_refinement
            (tenant_id, entry_id, label, criteria, position, hidden, base_hash)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (tenant_id, entry_id) DO UPDATE
         SET label = EXCLUDED.label,
             criteria = EXCLUDED.criteria,
             position = EXCLUDED.position,
             hidden = EXCLUDED.hidden,
             base_hash = EXCLUDED.base_hash,
             version = decision_option_refinement.version + 1,
             updated_at = now()
         RETURNING {OPTION_REFINEMENT_COLUMNS}"
    );
    Ok(Some(
        sqlx::query_as(AssertSqlSafe(sql))
            .bind(tenant.as_uuid())
            .bind(context.entry.id)
            .bind(&label)
            .bind(&criteria)
            .bind(position)
            .bind(hidden)
            .bind(&base_hash)
            .fetch_one(&mut *conn)
            .await?,
    ))
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
            laya_invocation_id: None,
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
    // A refinement is valid only against the policy content it was authored against. A
    // policy edit since the publish is a refusal, not a silent re-application (§7.2).
    if let Some(pinned) = refinement
        .as_ref()
        .and_then(|refinement| refinement.base_hash.as_deref())
    {
        let current = policy_base_hash(&policy);
        if current != pinned {
            return Err(LadderStoreError::BaseMismatch {
                what: "decision_policy",
                key: policy.id.to_string(),
            });
        }
    }
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

/// One effective option recorded with a decision.
///
/// The action builds these from the resolved option set, never from caller input; the store
/// uses them both as the decision's immutable snapshot and as the set the answer is checked
/// against.
#[derive(Clone, Debug)]
pub struct NewOption {
    /// The option key the answer names.
    pub key: String,
    /// The human label.
    pub label: String,
    /// The wire description.
    pub criteria: String,
    /// The authored order.
    pub position: i32,
    /// The option's payload.
    pub payload: Value,
    /// The refinement's pinned base hash, when one applied.
    pub refinement_hash: Option<String>,
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
    /// The authored option set the decision was constrained to.
    pub option_set_id: Option<Uuid>,
    /// The authored option set's version.
    pub option_set_version: Option<i64>,
    /// The canonical hash of the effective options the decision offered.
    pub option_set_hash: Option<String>,
    /// The base hash of every refinement the effective set applied, hidden ones included.
    pub option_refinement_hashes: Vec<String>,
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
    /// The verified `laya.classify` invocation behind a laya answer.
    pub laya_invocation_id: Option<Uuid>,
    /// The idempotency request hash the verified answer was produced for.
    pub laya_request_hash: Option<String>,
    /// The sha256 of the laya answer document.
    pub laya_response_hash: Option<String>,
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
    /// The authored option set the decision was constrained to.
    pub option_set_id: Option<Uuid>,
    /// The authored option set's version.
    pub option_set_version: Option<i64>,
    /// The canonical hash of the effective options the decision offered.
    pub option_set_hash: Option<String>,
    /// The base hash of every refinement the effective set applied.
    pub option_refinement_hashes: Vec<String>,
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
    /// The verified `laya.classify` invocation behind a laya answer.
    pub laya_invocation_id: Option<Uuid>,
    /// The idempotency request hash the verified answer was produced for.
    pub laya_request_hash: Option<String>,
    /// The sha256 of the laya answer document.
    pub laya_response_hash: Option<String>,
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
    option_set_id, option_set_version, option_set_hash, option_refinement_hashes, \
    probabilities, confidence::float8 AS confidence, policy_id, policy_version, \
    policy_refinement_id, refinement_version, laya_config_id, laya_config_version, \
    laya_invocation_id, laya_request_hash, laya_response_hash, actor_kind, actor_id, \
    approval_id, input_hash, latency_ms, outcome, decided_at, meta, version, created_at";

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
    // Options are never a caller's list: they are the effective set the action resolved, and
    // the answer must be one of its keys. A decision with no set carries no options at all.
    match (
        new.option_set_id,
        new.option_set_version,
        new.option_set_hash.as_deref(),
    ) {
        (None, None, None) => {
            if !new.options.is_empty() {
                return Err(LadderStoreError::Illegal(LadderError::FieldMissing {
                    field: "option_set_id",
                    rung: new.rung.as_str(),
                }));
            }
        }
        (Some(id), Some(version), Some(hash)) => {
            if !is_input_hash(hash) {
                return Err(LadderStoreError::Illegal(LadderError::InputHashShape));
            }
            let stored: Option<i64> = sqlx::query_scalar(
                "SELECT version FROM decision_option_set WHERE tenant_id = $1 AND id = $2",
            )
            .bind(tenant.as_uuid())
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
            match stored {
                None => {
                    return Err(LadderStoreError::NotFound {
                        what: "decision_option_set",
                    });
                }
                Some(stored) if stored != version => {
                    return Err(LadderStoreError::Stale {
                        what: "decision_option_set",
                    });
                }
                Some(_) => {}
            }
            if new.options.is_empty() {
                return Err(LadderStoreError::Illegal(LadderError::FieldMissing {
                    field: "options",
                    rung: new.rung.as_str(),
                }));
            }
            if !new.options.iter().any(|option| option.key == new.answer) {
                return Err(LadderStoreError::Illegal(LadderError::AnswerNotInSet {
                    answer: new.answer.clone(),
                }));
            }
        }
        _ => {
            return Err(LadderStoreError::Illegal(LadderError::FieldMissing {
                field: "option_set_pin",
                rung: new.rung.as_str(),
            }));
        }
    }
    if new
        .options
        .iter()
        .filter_map(|option| option.refinement_hash.as_deref())
        .chain(new.option_refinement_hashes.iter().map(String::as_str))
        .any(|hash| !is_input_hash(hash))
    {
        return Err(LadderStoreError::Illegal(LadderError::InputHashShape));
    }
    if new
        .options
        .iter()
        .any(|option| option.key.trim().is_empty() || option.position < 1)
    {
        return Err(LadderStoreError::Illegal(LadderError::InvalidPosition));
    }
    let pins = Pins {
        policy_version: new.policy_version,
        refinement_version: new.refinement_version,
        laya_config_version: new.laya_config_version,
        laya_invocation_id: new.laya_invocation_id,
    };
    let missing = missing_pins(new.rung, &pins, new.approval_id);
    if !missing.is_empty() {
        return Err(LadderStoreError::Illegal(LadderError::PinsIncomplete {
            rung: new.rung.as_str(),
            missing,
        }));
    }
    // A laya answer is not the row's own arithmetic: it is the verified response of an
    // `laya.classify` invocation (§7.2, finding v1-w1c-3). The row pins the invocation and the
    // request/response hashes that call was verified against; the database CHECK is the second
    // lock, never the first.
    if matches!(new.rung, Rung::Laya) {
        if new
            .laya_request_hash
            .as_deref()
            .is_none_or(|hash| hash.trim().is_empty())
        {
            return Err(LadderStoreError::Illegal(LadderError::FieldMissing {
                field: "laya_request_hash",
                rung: new.rung.as_str(),
            }));
        }
        if new
            .laya_response_hash
            .as_deref()
            .is_none_or(|hash| hash.trim().is_empty())
        {
            return Err(LadderStoreError::Illegal(LadderError::FieldMissing {
                field: "laya_response_hash",
                rung: new.rung.as_str(),
            }));
        }
        if let Some(hash) = new.laya_response_hash.as_deref()
            && !is_input_hash(hash)
        {
            return Err(LadderStoreError::Illegal(LadderError::InputHashShape));
        }
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

    let refinement_hashes = new.option_refinement_hashes.clone();
    let sql = format!(
        "INSERT INTO decision
            (tenant_id, kind, rung, project_id, card_id, question, answer, option_set_id,
             option_set_version, option_set_hash, option_refinement_hashes, probabilities,
             confidence, policy_id, policy_version, policy_refinement_id, refinement_version,
             laya_config_id, laya_config_version, laya_invocation_id, laya_request_hash,
             laya_response_hash, actor_kind, actor_id, approval_id, input_hash, latency_ms,
             outcome, decided_at, meta)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13::float8::numeric,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28,$29,$30)
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
        .bind(new.option_set_id)
        .bind(new.option_set_version)
        .bind(&new.option_set_hash)
        .bind(refinement_hashes)
        .bind(&new.probabilities)
        .bind(new.confidence)
        .bind(new.policy_id)
        .bind(new.policy_version)
        .bind(new.policy_refinement_id)
        .bind(new.refinement_version)
        .bind(new.laya_config_id)
        .bind(new.laya_config_version)
        .bind(new.laya_invocation_id)
        .bind(&new.laya_request_hash)
        .bind(&new.laya_response_hash)
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
            "INSERT INTO decision_option
                (tenant_id, decision_id, key, label, criteria, position, payload)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(tenant.as_uuid())
        .bind(row.id)
        .bind(&option.key)
        .bind(&option.label)
        .bind(&option.criteria)
        .bind(option.position)
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
