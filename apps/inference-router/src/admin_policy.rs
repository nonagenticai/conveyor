//! The write side of the tenant inference policy: the strict schema-v1 document, its
//! validation, and the immutable-version store.
//!
//! Ported from the reference's `services/inference_policy.py` (gate 1a). The runtime read model
//! in [`crate::policy`] stays lenient on purpose — it must serve a stored row, not reject it —
//! while THIS model refuses unknown keys, because a PUT document carries keys (provider keys,
//! budget, markup) that are never part of the policy.
//!
//! The store's contract:
//!
//! * a PUT of the ACTIVE document (same `sha256` of the canonical JSON) creates no version and
//!   reports `unchanged`;
//! * a new document writes a version, activates it, appends the audit row and bumps
//!   `policy_epoch` in ONE transaction;
//! * activate moves the pointer, audits and bumps in one transaction;
//! * the epoch bump is what clears every replica's cache (the poller in `policy.rs`), so a write
//!   is served within the poll interval — there is no shared in-process state to pop.

use std::collections::BTreeMap;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize, de};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::policy::PLATFORM_MODEL;

/// `GET /api/admin/catalog`'s `byo_providers`, in the reference's order.
pub const BYO_PROVIDERS: [&str; 5] = [
    "openai",
    "anthropic",
    "xai",
    "deepseek",
    "openai_compatible",
];

const TIERS: [&str; 3] = ["small", "medium", "high"];
const SETUPS: [&str; 3] = ["local", "owner_commercial", "byo"];
const KINDS: [&str; 3] = ["platform", "owner", "byo"];

/// One validation failure, in the reference's `{path, code}` shape.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Violation {
    pub path: String,
    pub code: String,
}

impl Violation {
    fn new(path: impl Into<String>, code: &'static str) -> Self {
        Self {
            path: path.into(),
            code: code.to_owned(),
        }
    }
}

/// A policy hop's target.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub kind: String,
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<Uuid>,
}

/// One hop.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hop {
    pub target: Target,
    pub model: String,
    #[serde(default = "yes")]
    pub failover_on_error: bool,
    #[serde(default)]
    pub failover_on_validation: bool,
}

fn yes() -> bool {
    true
}

/// The runaway limits. Stored and validated in gate 1a; enforced later.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rpm: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrency: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller_rpm: Option<i64>,
    /// Pydantic serialises `Decimal` as a string in JSON mode; keep that spelling so the
    /// canonical document's sha256 matches the reference for the same input.
    #[serde(
        default,
        deserialize_with = "decimal",
        skip_serializing_if = "Option::is_none"
    )]
    pub eur_per_minute: Option<String>,
    #[serde(
        default,
        deserialize_with = "decimal",
        skip_serializing_if = "Option::is_none"
    )]
    pub eur_per_hour: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_threshold: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_window_s: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_hops: Option<i64>,
    #[serde(
        default,
        deserialize_with = "decimal",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_cost_eur_per_request: Option<String>,
}

fn decimal<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(raw)) => Ok(Some(raw)),
        Some(Value::Number(number)) => Ok(Some(number.to_string())),
        Some(_) => Err(de::Error::custom("not a decimal")),
    }
}

/// The cascade block (unsupported in gate 1a, but stored and validated).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cascade {
    #[serde(default)]
    pub enabled: bool,
}

/// The strict schema-v1 document, exactly the reference's `PolicyDoc`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyDocument {
    #[serde(rename = "schema")]
    pub schema_version: i32,
    pub setup: String,
    #[serde(default = "local_allowed")]
    pub byo_mode: String,
    pub tiers: BTreeMap<String, Vec<Hop>>,
    #[serde(default = "nearest_larger")]
    pub tier_inheritance: String,
    #[serde(default)]
    pub limits: Limits,
    #[serde(default)]
    pub cascade: Cascade,
}

fn local_allowed() -> String {
    "local_allowed".to_owned()
}

fn nearest_larger() -> String {
    "nearest_larger_then_smaller".to_owned()
}

impl PolicyDocument {
    /// The canonical JSON the store persists and hashes: sorted keys, compact separators and no
    /// nulls — the reference's `canonical_json(doc.to_json())`.
    pub fn canonical(&self) -> String {
        let value = serde_json::to_value(self).expect("a serialisable policy document");
        canonical_json(&value)
    }

    /// `sha256(canonical())`, the replay-stable document identity.
    pub fn doc_sha256(&self) -> String {
        sha256_hex(self.canonical().as_bytes())
    }
}

pub fn canonical_json(value: &Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            out.push('{');
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).expect("a string key"));
                out.push(':');
                write_canonical(&map[key], out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&serde_json::to_string(other).expect("a scalar")),
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Validate a PUT body. Returns the parsed document when structurally valid (semantic violations
/// may still be present) and EVERY violation, in the reference's order.
pub fn validate(
    doc: &Value,
    platform_providers: &[String],
) -> (Option<PolicyDocument>, Vec<Violation>) {
    let mut violations = Vec::new();
    let structurally_valid = structural(doc, &mut violations);
    let Some(object) = doc.as_object() else {
        return (None, violations);
    };
    if !structurally_valid {
        return (None, violations);
    }
    let parsed: PolicyDocument = match serde_json::from_value(Value::Object(object.clone())) {
        Ok(parsed) => parsed,
        Err(_) => {
            violations.push(Violation::new("", "invalid"));
            return (None, violations);
        }
    };
    semantic(&parsed, platform_providers, &mut violations);
    (Some(parsed), violations)
}

fn structural(doc: &Value, out: &mut Vec<Violation>) -> bool {
    let Some(object) = doc.as_object() else {
        out.push(Violation::new("", "invalid"));
        return false;
    };
    let mut ok = true;
    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "schema" | "setup" | "byo_mode" | "tiers" | "tier_inheritance" | "limits" | "cascade"
        ) {
            out.push(Violation::new(key, "invalid"));
            ok = false;
        }
    }
    match object.get("schema") {
        Some(Value::Number(number)) if number.as_i64() == Some(1) => {}
        Some(_) => {
            out.push(Violation::new("schema", "invalid"));
            ok = false;
        }
        None => {
            out.push(Violation::new("schema", "invalid"));
            ok = false;
        }
    }
    match object.get("setup").and_then(Value::as_str) {
        Some(setup) if SETUPS.contains(&setup) => {}
        Some(_) => {
            out.push(Violation::new("setup", "invalid"));
            ok = false;
        }
        None => {
            out.push(Violation::new("setup", "invalid"));
            ok = false;
        }
    }
    if let Some(byo_mode) = object.get("byo_mode")
        && !matches!(
            byo_mode.as_str(),
            Some("local_allowed" | "byo_required" | "platform_only")
        )
    {
        out.push(Violation::new("byo_mode", "invalid"));
        ok = false;
    }
    if let Some(inheritance) = object.get("tier_inheritance")
        && inheritance.as_str() != Some("nearest_larger_then_smaller")
    {
        out.push(Violation::new("tier_inheritance", "invalid"));
        ok = false;
    }
    match object.get("tiers").and_then(Value::as_object) {
        None => {
            out.push(Violation::new("tiers", "invalid"));
            ok = false;
        }
        Some(tiers) => {
            for (tier, hops) in tiers {
                let path = format!("tiers.{tier}");
                if !TIERS.contains(&tier.as_str()) {
                    out.push(Violation::new(path, "invalid"));
                    ok = false;
                    continue;
                }
                let Some(hops) = hops.as_array() else {
                    out.push(Violation::new(path, "invalid"));
                    ok = false;
                    continue;
                };
                for (index, hop) in hops.iter().enumerate() {
                    if !hop_is_valid(hop, &format!("{path}.{index}"), out) {
                        ok = false;
                    }
                }
            }
        }
    }
    if let Some(limits) = object.get("limits")
        && !limits_is_valid(limits, out)
    {
        ok = false;
    }
    if let Some(cascade) = object.get("cascade") {
        let Some(cascade) = cascade.as_object() else {
            out.push(Violation::new("cascade", "invalid"));
            return false;
        };
        for key in cascade.keys() {
            if key != "enabled" {
                out.push(Violation::new(format!("cascade.{key}"), "invalid"));
                ok = false;
            }
        }
        if let Some(enabled) = cascade.get("enabled")
            && !enabled.is_boolean()
        {
            out.push(Violation::new("cascade.enabled", "invalid"));
            ok = false;
        }
    }
    ok
}

fn hop_is_valid(hop: &Value, base: &str, out: &mut Vec<Violation>) -> bool {
    let Some(object) = hop.as_object() else {
        out.push(Violation::new(base, "invalid"));
        return false;
    };
    let mut ok = true;
    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "target" | "model" | "failover_on_error" | "failover_on_validation"
        ) {
            out.push(Violation::new(format!("{base}.{key}"), "invalid"));
            ok = false;
        }
    }
    match object.get("model").and_then(Value::as_str) {
        Some(model) if !model.is_empty() && model.chars().count() <= 200 => {}
        Some(_) => {
            out.push(Violation::new(format!("{base}.model"), "invalid"));
            ok = false;
        }
        None => {
            out.push(Violation::new(format!("{base}.model"), "invalid"));
            ok = false;
        }
    }
    for flag in ["failover_on_error", "failover_on_validation"] {
        if let Some(value) = object.get(flag)
            && !value.is_boolean()
        {
            out.push(Violation::new(format!("{base}.{flag}"), "invalid"));
            ok = false;
        }
    }
    match object.get("target").and_then(Value::as_object) {
        None => {
            out.push(Violation::new(format!("{base}.target"), "invalid"));
            ok = false;
        }
        Some(target) => {
            for key in target.keys() {
                if !matches!(key.as_str(), "kind" | "provider" | "credential_id") {
                    out.push(Violation::new(format!("{base}.target.{key}"), "invalid"));
                    ok = false;
                }
            }
            match target.get("kind").and_then(Value::as_str) {
                Some(kind) if KINDS.contains(&kind) => {}
                Some(_) => {
                    out.push(Violation::new(format!("{base}.target.kind"), "invalid"));
                    ok = false;
                }
                None => {
                    out.push(Violation::new(format!("{base}.target.kind"), "invalid"));
                    ok = false;
                }
            }
            match target.get("provider").and_then(Value::as_str) {
                Some(provider) if !provider.is_empty() && provider.chars().count() <= 64 => {}
                Some(_) => {
                    out.push(Violation::new(format!("{base}.target.provider"), "invalid"));
                    ok = false;
                }
                None => {
                    out.push(Violation::new(format!("{base}.target.provider"), "invalid"));
                    ok = false;
                }
            }
            if let Some(credential) = target.get("credential_id")
                && !credential.is_null()
                && !credential
                    .as_str()
                    .is_some_and(|raw| Uuid::parse_str(raw).is_ok())
            {
                out.push(Violation::new(
                    format!("{base}.target.credential_id"),
                    "invalid",
                ));
                ok = false;
            }
        }
    }
    ok
}

fn limits_is_valid(limits: &Value, out: &mut Vec<Violation>) -> bool {
    let Some(object) = limits.as_object() else {
        out.push(Violation::new("limits", "invalid"));
        return false;
    };
    let mut ok = true;
    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "rpm"
                | "max_concurrency"
                | "caller_rpm"
                | "eur_per_minute"
                | "eur_per_hour"
                | "loop_threshold"
                | "loop_window_s"
                | "max_hops"
                | "max_cost_eur_per_request"
        ) {
            out.push(Violation::new(format!("limits.{key}"), "invalid"));
            ok = false;
        }
    }
    for (key, minimum, maximum) in [
        ("rpm", 1, None),
        ("max_concurrency", 1, None),
        ("caller_rpm", 1, None),
        ("loop_threshold", 1, None),
        ("loop_window_s", 1, None),
        ("max_hops", 1, Some(3)),
    ] {
        let Some(value) = object.get(key) else {
            continue;
        };
        if value.is_null() {
            continue;
        }
        match value.as_i64() {
            Some(number) if number >= minimum && maximum.is_none_or(|max| number <= max) => {}
            _ => {
                out.push(Violation::new(format!("limits.{key}"), "invalid"));
                ok = false;
            }
        }
    }
    for key in ["eur_per_minute", "eur_per_hour", "max_cost_eur_per_request"] {
        let Some(value) = object.get(key) else {
            continue;
        };
        if value.is_null() {
            continue;
        }
        let parsed = value.as_f64().or_else(|| {
            value
                .as_str()
                .and_then(|raw| raw.trim().parse::<f64>().ok())
        });
        match parsed {
            Some(number) if number >= 0.0 => {}
            _ => {
                out.push(Violation::new(format!("limits.{key}"), "invalid"));
                ok = false;
            }
        }
    }
    ok
}

fn semantic(doc: &PolicyDocument, platform_providers: &[String], out: &mut Vec<Violation>) {
    let tier_count = doc.tiers.len();
    if (doc.setup == "local" && tier_count != 1) || !(1..=3).contains(&tier_count) {
        out.push(Violation::new("tiers", "tier_count"));
    }
    let allowed: &[&str] = match doc.setup.as_str() {
        "local" => &["platform"],
        "owner_commercial" => &["owner", "platform"],
        _ => &["byo"],
    };
    for (tier, hops) in &doc.tiers {
        if !(1..=3).contains(&hops.len()) {
            out.push(Violation::new(format!("tiers.{tier}"), "hop_count"));
        }
        for (index, hop) in hops.iter().enumerate() {
            let base = format!("tiers.{tier}.{index}");
            let kind = hop.target.kind.as_str();
            if kind == "owner" && doc.setup != "owner_commercial" {
                out.push(Violation::new(
                    format!("{base}.target.kind"),
                    "owner_hop_requires_owner_commercial",
                ));
            } else if !allowed.contains(&kind) {
                out.push(Violation::new(
                    format!("{base}.target.kind"),
                    "hop_kind_not_allowed",
                ));
            }
            match kind {
                "platform" => {
                    if !platform_providers
                        .iter()
                        .any(|name| name == &hop.target.provider)
                    {
                        out.push(Violation::new(
                            format!("{base}.target.provider"),
                            "provider_not_eligible",
                        ));
                    }
                    if hop.model != PLATFORM_MODEL {
                        out.push(Violation::new(format!("{base}.model"), "model_not_allowed"));
                    }
                    if hop.target.credential_id.is_some() {
                        out.push(Violation::new(
                            format!("{base}.target.credential_id"),
                            "invalid",
                        ));
                    }
                }
                "owner" => {
                    // Gate 1a has no configured owner provider: every owner hop is refused.
                    out.push(Violation::new(
                        format!("{base}.target.provider"),
                        "owner_provider_unconfigured",
                    ));
                    if hop.target.credential_id.is_some() {
                        out.push(Violation::new(
                            format!("{base}.target.credential_id"),
                            "invalid",
                        ));
                    }
                }
                "byo" => {
                    if hop.target.credential_id.is_none() {
                        out.push(Violation::new(
                            format!("{base}.target.credential_id"),
                            "credential_required",
                        ));
                    }
                    if !BYO_PROVIDERS.contains(&hop.target.provider.as_str()) {
                        out.push(Violation::new(
                            format!("{base}.target.provider"),
                            "provider_not_allowed",
                        ));
                    }
                }
                _ => {}
            }
        }
    }
    if doc.cascade.enabled {
        out.push(Violation::new("cascade.enabled", "cascade_unsupported"));
    }
    if doc.setup == "owner_commercial" {
        out.push(Violation::new("setup", "setup2_disabled"));
    }
    if doc.setup == "byo" {
        out.push(Violation::new("setup", "byo_disabled"));
    }
}

/// Who wrote.
#[derive(Clone, Debug)]
pub struct Actor {
    pub kind: String,
    pub ref_hash: String,
}

/// The PUT result, exactly the reference's `PutResult.to_json()`.
#[derive(Clone, Debug)]
pub struct PutResult {
    pub version: i32,
    pub active_version: i32,
    pub doc_sha256: String,
    pub unchanged: bool,
}

impl PutResult {
    pub fn to_json(&self) -> Value {
        json!({
            "version": self.version,
            "active_version": self.active_version,
            "doc_sha256": self.doc_sha256,
            "unchanged": self.unchanged,
        })
    }
}

const ACTIVE_SQL: &str = "SELECT v.version, v.doc, v.doc_sha256 FROM tenant_inference_policy p \
     JOIN tenant_inference_policy_version v \
       ON v.tenant_id = p.tenant_id AND v.version = p.active_version \
     WHERE p.tenant_id = $1";

/// Store `doc` as a new version and activate it — or report `unchanged`.
pub async fn put(
    pool: &PgPool,
    tenant: Uuid,
    doc: &PolicyDocument,
    actor: &Actor,
) -> Result<PutResult, sqlx::Error> {
    let stored = doc.canonical();
    let sha = doc.doc_sha256();
    let mut tx = pool.begin().await?;
    lock(&mut tx, tenant).await?;
    let current: Option<(i32, Value, String)> = sqlx::query_as(ACTIVE_SQL)
        .bind(tenant)
        .fetch_optional(&mut *tx)
        .await?;
    let result = if let Some((version, _, current_sha)) = current
        && current_sha.trim() == sha
    {
        audit(
            &mut tx,
            tenant,
            "put_unchanged",
            Some(version),
            actor,
            Some(&sha),
        )
        .await?;
        PutResult {
            version,
            active_version: version,
            doc_sha256: sha,
            unchanged: true,
        }
    } else {
        let next: i32 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM tenant_inference_policy_version \
             WHERE tenant_id = $1",
        )
        .bind(tenant)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO tenant_inference_policy_version \
             (tenant_id, version, doc, doc_sha256, actor_kind, actor_ref_hash) \
             VALUES ($1, $2, CAST($3 AS jsonb), $4, $5, $6)",
        )
        .bind(tenant)
        .bind(next)
        .bind(&stored)
        .bind(&sha)
        .bind(&actor.kind)
        .bind(&actor.ref_hash)
        .execute(&mut *tx)
        .await?;
        upsert_active(&mut tx, tenant, next).await?;
        bump_epoch(&mut tx).await?;
        audit(&mut tx, tenant, "put", Some(next), actor, Some(&sha)).await?;
        PutResult {
            version: next,
            active_version: next,
            doc_sha256: sha,
            unchanged: false,
        }
    };
    tx.commit().await?;
    Ok(result)
}

/// Point the tenant at `version`. `None` when the version does not exist.
pub async fn activate(
    pool: &PgPool,
    tenant: Uuid,
    version: i32,
    actor: &Actor,
) -> Result<Option<i32>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    lock(&mut tx, tenant).await?;
    let exists: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM tenant_inference_policy_version WHERE tenant_id = $1 AND version = $2",
    )
    .bind(tenant)
    .bind(version)
    .fetch_optional(&mut *tx)
    .await?;
    if exists.is_none() {
        tx.rollback().await?;
        return Ok(None);
    }
    upsert_active(&mut tx, tenant, version).await?;
    bump_epoch(&mut tx).await?;
    audit(&mut tx, tenant, "activate", Some(version), actor, None).await?;
    tx.commit().await?;
    Ok(Some(version))
}

/// The GET document: the active version, its document and the version list.
pub async fn describe(pool: &PgPool, tenant: Uuid) -> Result<Value, sqlx::Error> {
    let active: Option<(i32, Value)> = sqlx::query_as(
        "SELECT v.version, v.doc FROM tenant_inference_policy p \
         JOIN tenant_inference_policy_version v \
           ON v.tenant_id = p.tenant_id AND v.version = p.active_version \
         WHERE p.tenant_id = $1",
    )
    .bind(tenant)
    .fetch_optional(pool)
    .await?;
    let versions: Vec<(i32, DateTime<Utc>, String)> = sqlx::query_as(
        "SELECT version, created_at, actor_kind FROM tenant_inference_policy_version \
         WHERE tenant_id = $1 ORDER BY version",
    )
    .bind(tenant)
    .fetch_all(pool)
    .await?;
    Ok(json!({
        "active_version": active.as_ref().map(|(version, _)| *version),
        "doc": active.as_ref().map(|(_, doc)| doc.clone()),
        "versions": versions
            .into_iter()
            .map(|(version, created_at, actor_kind)| json!({
                "version": version,
                "created_at": isoformat(created_at),
                "actor_kind": actor_kind,
            }))
            .collect::<Vec<_>>(),
    }))
}

/// One row of the admin read-back, named once instead of as a ten-tuple.
#[derive(Debug, sqlx::FromRow)]
struct CallRow {
    request_id: Option<Uuid>,
    served_provider: Option<String>,
    served_model: Option<String>,
    served_model_root: Option<String>,
    policy_version: Option<i32>,
    llm_setup: Option<i16>,
    key_owner: Option<String>,
    hop_index: Option<i16>,
    hop_reason: Option<String>,
    status: String,
}

/// One tenant's calls for a correlation id: the provisioning smoke's read-back.
pub async fn tenant_calls(
    pool: &PgPool,
    tenant: Uuid,
    correlation_id: &str,
) -> Result<Vec<Value>, sqlx::Error> {
    let rows: Vec<CallRow> = sqlx::query_as(
        "SELECT request_id, served_provider, served_model, served_model_root, policy_version, \
                llm_setup, key_owner, hop_index, hop_reason, status \
           FROM inference_call WHERE tenant_id = $1 AND correlation_id = $2 \
          ORDER BY created_at DESC LIMIT 50",
    )
    .bind(tenant)
    .bind(correlation_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            json!({
                "inference_id": row.request_id.map(|id| id.to_string()),
                "served_provider": row.served_provider,
                "served_model": row.served_model,
                "served_model_root": row.served_model_root,
                "policy_version": row.policy_version,
                "llm_setup": row.llm_setup,
                "key_owner": row.key_owner,
                "hop_index": row.hop_index,
                "hop_reason": row.hop_reason,
                "status": row.status,
            })
        })
        .collect())
}

async fn lock(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("tenant_inference_policy:{tenant}"))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn upsert_active(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: Uuid,
    version: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO tenant_inference_policy (tenant_id, active_version, updated_at) \
         VALUES ($1, $2, now()) \
         ON CONFLICT (tenant_id) DO UPDATE \
         SET active_version = EXCLUDED.active_version, updated_at = now()",
    )
    .bind(tenant)
    .bind(version)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn bump_epoch(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE policy_epoch SET epoch = epoch + 1 WHERE id = 1")
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: Uuid,
    action: &str,
    version: Option<i32>,
    actor: &Actor,
    sha: Option<&str>,
) -> Result<(), sqlx::Error> {
    let detail = match sha {
        Some(sha) => json!({"doc_sha256": sha}),
        None => json!({}),
    };
    sqlx::query(
        "INSERT INTO tenant_policy_audit \
         (tenant_id, action, version, actor_kind, actor_ref_hash, detail) \
         VALUES ($1, $2, $3, $4, $5, CAST($6 AS jsonb))",
    )
    .bind(tenant)
    .bind(action)
    .bind(version)
    .bind(&actor.kind)
    .bind(&actor.ref_hash)
    .bind(detail.to_string())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Python's `datetime.isoformat()`: microseconds only when non-zero, `+00:00` not `Z`.
fn isoformat(at: DateTime<Utc>) -> String {
    let format = if at.timestamp_subsec_nanos() == 0 {
        SecondsFormat::Secs
    } else {
        SecondsFormat::Micros
    };
    at.to_rfc3339_opts(format, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_doc() -> Value {
        json!({
            "schema": 1,
            "setup": "local",
            "tiers": {"medium": [{
                "target": {"kind": "platform", "provider": "pgx-local"},
                "model": "local-llm"
            }]}
        })
    }

    #[test]
    fn a_valid_local_document_has_no_violations() {
        let (doc, violations) = validate(&local_doc(), &["pgx-local".to_owned()]);
        assert!(violations.is_empty(), "{violations:?}");
        let doc = doc.expect("parsed");
        assert_eq!(doc.setup, "local");
        assert_eq!(doc.byo_mode, "local_allowed");
        assert!(!doc.cascade.enabled);
    }

    #[test]
    fn unknown_keys_are_refused_with_their_path() {
        let mut doc = local_doc();
        doc["markup"] = json!(1.5);
        let (parsed, violations) = validate(&doc, &["pgx-local".to_owned()]);
        assert!(parsed.is_none());
        assert!(
            violations
                .iter()
                .any(|v| v.path == "markup" && v.code == "invalid"),
            "{violations:?}"
        );

        let mut hop = local_doc();
        hop["tiers"]["medium"][0]["secret"] = json!("sk-test");
        let (_, violations) = validate(&hop, &["pgx-local".to_owned()]);
        assert!(
            violations
                .iter()
                .any(|v| v.path == "tiers.medium.0.secret" && v.code == "invalid"),
            "{violations:?}"
        );
    }

    #[test]
    fn the_semantic_rules_match_the_reference() {
        // One tier for `local`; two tiers is a tier_count violation.
        let mut two = local_doc();
        two["tiers"]["high"] = json!([{
            "target": {"kind": "platform", "provider": "pgx-local"},
            "model": "local-llm"
        }]);
        let (_, violations) = validate(&two, &["pgx-local".to_owned()]);
        assert!(
            violations.iter().any(|v| v.code == "tier_count"),
            "{violations:?}"
        );

        // A platform hop must name local-llm and a customer-eligible provider.
        let mut wrong = local_doc();
        wrong["tiers"]["medium"][0]["model"] = json!("gemini");
        wrong["tiers"]["medium"][0]["target"]["provider"] = json!("deepseek");
        let (_, violations) = validate(&wrong, &["pgx-local".to_owned()]);
        let codes: Vec<&str> = violations.iter().map(|v| v.code.as_str()).collect();
        assert!(codes.contains(&"model_not_allowed"), "{codes:?}");
        assert!(codes.contains(&"provider_not_eligible"), "{codes:?}");

        // Setup 2 and 3 are not built in gate 1a.
        let mut owner = local_doc();
        owner["setup"] = json!("owner_commercial");
        owner["tiers"]["medium"][0]["target"]["kind"] = json!("owner");
        let (_, violations) = validate(&owner, &["pgx-local".to_owned()]);
        let codes: Vec<&str> = violations.iter().map(|v| v.code.as_str()).collect();
        assert!(codes.contains(&"setup2_disabled"), "{codes:?}");
        assert!(codes.contains(&"owner_provider_unconfigured"), "{codes:?}");

        let mut byo = local_doc();
        byo["setup"] = json!("byo");
        byo["tiers"]["medium"][0]["target"] = json!({"kind": "byo", "provider": "openai"});
        let (_, violations) = validate(&byo, &["pgx-local".to_owned()]);
        let codes: Vec<&str> = violations.iter().map(|v| v.code.as_str()).collect();
        assert!(codes.contains(&"byo_disabled"), "{codes:?}");
        assert!(codes.contains(&"credential_required"), "{codes:?}");
    }

    #[test]
    fn the_limits_are_range_checked() {
        let mut doc = local_doc();
        doc["limits"] = json!({"max_hops": 4, "rpm": 0});
        let (_, violations) = validate(&doc, &["pgx-local".to_owned()]);
        let paths: Vec<&str> = violations.iter().map(|v| v.path.as_str()).collect();
        assert!(paths.contains(&"limits.max_hops"), "{paths:?}");
        assert!(paths.contains(&"limits.rpm"), "{paths:?}");
    }

    #[test]
    fn the_canonical_document_is_sorted_compact_and_defaulted() {
        let (doc, _) = validate(&local_doc(), &["pgx-local".to_owned()]);
        let canonical = doc.expect("parsed").canonical();
        assert_eq!(
            canonical,
            r#"{"byo_mode":"local_allowed","cascade":{"enabled":false},"limits":{},"schema":1,"setup":"local","tier_inheritance":"nearest_larger_then_smaller","tiers":{"medium":[{"failover_on_error":true,"failover_on_validation":false,"model":"local-llm","target":{"kind":"platform","provider":"pgx-local"}}]}}"#
        );
    }

    #[test]
    fn the_same_document_hashes_the_same_regardless_of_key_order() {
        let (first, _) = validate(&local_doc(), &["pgx-local".to_owned()]);
        let mut reordered = json!({
            "tiers": {"medium": [{
                "model": "local-llm",
                "target": {"provider": "pgx-local", "kind": "platform"}
            }]},
            "setup": "local",
            "schema": 1
        });
        reordered["unused"] = Value::Null;
        reordered.as_object_mut().expect("object").remove("unused");
        let (second, _) = validate(&reordered, &["pgx-local".to_owned()]);
        assert_eq!(
            first.expect("first").doc_sha256(),
            second.expect("second").doc_sha256()
        );
    }

    #[test]
    fn the_isoformat_matches_pythons() {
        let at: DateTime<Utc> = "2026-11-13T10:20:46.123456Z".parse().expect("time");
        assert_eq!(isoformat(at), "2026-11-13T10:20:46.123456+00:00");
        let whole: DateTime<Utc> = "2026-11-13T10:20:46Z".parse().expect("time");
        assert_eq!(isoformat(whole), "2026-11-13T10:20:46+00:00");
    }
}
