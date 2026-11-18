//! `/api/reviews/{review_id}/evidence-snapshots` — the immutable analysis input set (`ER-007`).
//!
//! Ported from the reference's `api/routers/snapshots.py` + `application/services/snapshots.py`
//! and `domain/snapshot_manifest.py`. A snapshot names the exact evidence versions one run was
//! entitled to read; `GET` reproduces it from its stored members rather than replaying a cache,
//! and refuses a manifest whose recomputed digest differs from the sealed one.
//!
//! ## The build is synchronous; the job row is a record, not a claim
//!
//! The reference enqueues a durable `evidence_snapshot` job and then finishes it inline. There is
//! no worker here, so this port writes the snapshot, its member rows and its artifact row in the
//! caller's transaction and leaves the job row `queued`. That is honest: the snapshot is real and
//! reproducible, the job is a durable, readable, cancellable record that nothing has executed.
//!
//! ## Both routes are lead-only, including the reads
//!
//! A snapshot's MEMBER LIST is a complete statement of what survived screening — the same
//! inference `ER-004.2` forbids as a count, arriving as ids. The read holds the same gate the
//! creation does.
//!
//! ## The caller supplies the members; this service never resolves "latest"
//!
//! Every pin is looked up, its `content_hash` and `version_number` are read from the ROW (never
//! from the request), and a version that is not currently accepted is refused — the tail of its
//! acceptance chain must be an `accept`, never an earlier row.

use std::collections::HashSet;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::actor::Actor;
use crate::db;
use crate::evidence_review::evidence::{locked_protocol_id, unprocessable};
use crate::evidence_review::reviews::require_review_lead_for;
use crate::evidence_review::{hash_bytes, hash_value, new_id, pool};
use crate::problem::Problem;
use crate::state::Shared;

/// The released manifest grammar; mirrors the `ck_evidence_snapshots_manifest_pinned` CHECK.
pub const MANIFEST_SCHEMA_REF: &str = "urn:ahtapol:evidence-snapshot-manifest:1";

/// The order key every snapshot records, so a consumer can recompute the digest under the rule
/// that produced it.
pub const PINNED_REF_ORDERING_RULE: &str = "pinned_ref_key_asc";

/// Which serializer produced an artifact's bytes. `14-…:364` requires byte-identical
/// reconstruction "under the same serializer version".
pub const SERIALIZER_VERSION: &str = "rfc8785/1";

pub const CANONICAL_MANIFEST_ARTIFACT: &str = "canonical_manifest";
pub const CANONICAL_MANIFEST_MEDIA_TYPE: &str = "application/json";

/// The bounded context every member belongs to. HYPHEN, not underscore: `identity-ref` constrains
/// `owner_context` to `^[a-z][a-z0-9-]*$`.
pub const OWNER_CONTEXT: &str = "evidence-review";

/// The released immutable-version grammar a pinned member declares itself against.
pub const MEMBER_SCHEMA_REF: &str = "urn:ahtapol:immutable-version:1";

/// The job type `FND-006` registered for this operation.
pub const JOB_TYPE: &str = "evidence_snapshot";

/// The five gates `14-…:345` names, in the document's order.
pub const SNAPSHOT_GATES: [&str; 5] = [
    "unresolved_conflicts",
    "uncommitted_protocol",
    "unresolved_critical_proposals",
    "denied_rights",
    "stale_mandatory_evidence",
];

/// The gates that have subject matter here and are therefore the only ones a producer may record.
pub const IMPLEMENTED_GATES: [&str; 2] = ["uncommitted_protocol", "unresolved_conflicts"];

const HASH_PLACEHOLDER: &str =
    "sha256:0000000000000000000000000000000000000000000000000000000000000000";

fn iso(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn field<'a>(body: &'a Value, name: &str) -> Option<&'a str> {
    body.get(name).and_then(Value::as_str)
}

/// `^[a-z][a-z0-9_]*$` and no estate-guarded token — validated at the boundary, never left to the
/// schema: a schema refusal here is a 500 whose body names eleven innocent fields.
fn intended_use_is_valid(use_token: &str) -> bool {
    let mut chars = use_token.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn carries_guarded_token(use_token: &str) -> bool {
    ["regulatory", "legal_", "normative", "consolidation"]
        .iter()
        .any(|token| use_token.contains(token))
}

// ── the manifest ─────────────────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn pin(
    entity_type: &str,
    entity_id: &str,
    version_id: &str,
    version_number: i32,
    schema_ref: &str,
    content_hash: &str,
    tenant_id: &str,
    resolved_at: &str,
) -> Value {
    json!({
        "owner_context": OWNER_CONTEXT,
        "entity_type": entity_type,
        "entity_id": entity_id,
        "resource_scope": "tenant",
        "resource_tenant_id": tenant_id,
        "version_id": version_id,
        "version_number": version_number,
        "schema_ref": schema_ref,
        "content_hash": content_hash,
        "resolved_at": resolved_at,
    })
}

fn order_key(member: &Value) -> (String, String, String, String) {
    (
        member["owner_context"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        member["entity_type"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        member["entity_id"].as_str().unwrap_or_default().to_owned(),
        member["version_id"].as_str().unwrap_or_default().to_owned(),
    )
}

fn order_members(members: &[Value]) -> Vec<Value> {
    let mut ordered = members.to_vec();
    ordered.sort_by_key(order_key);
    ordered
}

/// `sha256:<hex>` over the ordered member list with `resolved_at` removed. That field records
/// when THIS producer looked the member up, not a fact about the evidence; including it would make
/// two snapshots over identical evidence hash differently.
fn member_manifest_hash(members: &[Value]) -> String {
    let stripped: Vec<Value> = order_members(members)
        .into_iter()
        .map(|mut member| {
            if let Some(object) = member.as_object_mut() {
                object.remove("resolved_at");
            }
            member
        })
        .collect();
    hash_value(&Value::Array(stripped))
}

#[allow(clippy::too_many_arguments)]
fn canonical_manifest(
    snapshot_id: Uuid,
    entity_id: Uuid,
    review_id: Uuid,
    version_number: i32,
    supersedes_snapshot_id: Option<Uuid>,
    intended_use: &str,
    created_at: &str,
    created_by: &str,
    members: &[Value],
    member_manifest_digest: &str,
) -> Value {
    let ordered = order_members(members);
    let mut manifest = json!({
        "schema_ref": MANIFEST_SCHEMA_REF,
        // Present as a PLACEHOLDER: the released grammar requires the field, and the hash view
        // removes it, so the placeholder cannot reach the digest.
        "content_hash": HASH_PLACEHOLDER,
        "version_id": snapshot_id.to_string(),
        "entity_id": entity_id.to_string(),
        "version_number": version_number,
        "created_at": created_at,
        "created_by": created_by,
        "review_id": review_id.to_string(),
        "intended_use": intended_use,
        "members": ordered,
        "member_count": ordered.len(),
        "member_manifest_hash": member_manifest_digest,
    });
    if let Some(supersedes) = supersedes_snapshot_id {
        manifest["supersedes_version_id"] = json!(supersedes.to_string());
    }
    manifest
}

/// The manifest's own hash: the hash view strips `content_hash` (an object's own hash is never
/// part of it) and each member's `resolved_at`. Nested member hashes stay in.
fn snapshot_content_hash(manifest: &Value) -> String {
    let mut view = manifest.clone();
    if let Some(object) = view.as_object_mut() {
        object.remove("content_hash");
        if let Some(Value::Array(members)) = object.get_mut("members") {
            for member in members {
                if let Some(member) = member.as_object_mut() {
                    member.remove("resolved_at");
                }
            }
        }
    }
    hash_value(&view)
}

fn seal(manifest: &Value) -> (Value, String) {
    let digest = snapshot_content_hash(manifest);
    let mut sealed = manifest.clone();
    sealed["content_hash"] = json!(digest);
    (sealed, digest)
}

// ── gates ────────────────────────────────────────────────────────────────────────────────────

/// Conflicts that still need a lead: not adjudicated, and not settled by consensus. Mirrors the
/// rule the flow ledger and the screening routes use — a conflict is closed by an adjudication OR
/// by the reviewers' current decisions agreeing again with the required number of votes.
async fn unresolved_conflict_count(
    tx: &mut Transaction<'_, Postgres>,
    review_id: Uuid,
) -> Result<i64, Problem> {
    // The gate must see every reviewer's vote; a non-lead review creator is blinded by policy.
    sqlx::query("SELECT set_config('app.actor_role', 'lead', true)")
        .execute(&mut **tx)
        .await
        .map_err(|error| Problem::from_db("elevating the conflict read", error))?;

    let conflicts = sqlx::query(
        "SELECT conflict_id, stage_id, round_number, review_record_id
           FROM screening_conflicts WHERE review_id = $1 ORDER BY opened_at, conflict_id",
    )
    .bind(review_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading screening conflicts", error))?;
    if conflicts.is_empty() {
        return Ok(0);
    }

    let fallback: i32 = sqlx::query_scalar(
        "SELECT reviewers_required FROM review_protocol_versions p
          WHERE p.protocol_version_id = (
              SELECT protocol_version_id FROM review_state_history h
               WHERE h.review_id = $1 AND h.to_state = 'active' AND h.protocol_version_id IS NOT NULL
               ORDER BY h.revision_number DESC LIMIT 1)
         UNION ALL SELECT 2 LIMIT 1",
    )
    .bind(review_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the protocol's reviewer count", error))?;
    let stages: std::collections::HashMap<Uuid, i32> =
        sqlx::query("SELECT stage_id, reviewers_required FROM review_stages WHERE review_id = $1")
            .bind(review_id)
            .fetch_all(&mut **tx)
            .await
            .map_err(|error| Problem::from_db("reading the review's stages", error))?
            .into_iter()
            .map(|row| (row.get("stage_id"), row.get("reviewers_required")))
            .collect();

    let conflict_ids: Vec<Uuid> = conflicts.iter().map(|row| row.get("conflict_id")).collect();
    let adjudicated: HashSet<Uuid> = sqlx::query_scalar(
        "SELECT a.conflict_id FROM screening_adjudications a
          WHERE a.conflict_id = ANY($1)
            AND NOT EXISTS (SELECT 1 FROM screening_adjudications x
                             WHERE x.supersedes_adjudication_id = a.adjudication_id)",
    )
    .bind(&conflict_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading current adjudications", error))?
    .into_iter()
    .collect();

    let decisions = sqlx::query(
        "SELECT stage_id, round_number, review_record_id, disposition, exclusion_reason_code
           FROM screening_decisions d
          WHERE d.review_id = $1
            AND NOT EXISTS (SELECT 1 FROM screening_decisions x
                             WHERE x.supersedes_decision_id = d.decision_id)",
    )
    .bind(review_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading current decisions", error))?;
    #[allow(clippy::type_complexity)]
    let mut votes: std::collections::HashMap<(Uuid, i32, Uuid), Vec<(String, Option<String>)>> =
        std::collections::HashMap::new();
    for row in decisions {
        let key = (
            row.get::<Uuid, _>("stage_id"),
            row.get::<i32, _>("round_number"),
            row.get::<Uuid, _>("review_record_id"),
        );
        votes
            .entry(key)
            .or_default()
            .push((row.get("disposition"), row.get("exclusion_reason_code")));
    }

    let mut unresolved = 0i64;
    for conflict in &conflicts {
        let conflict_id: Uuid = conflict.get("conflict_id");
        if adjudicated.contains(&conflict_id) {
            continue;
        }
        let stage_id: Uuid = conflict.get("stage_id");
        let required = stages
            .get(&stage_id)
            .copied()
            .filter(|required| *required > 0)
            .unwrap_or(fallback);
        let cast = votes
            .get(&(
                stage_id,
                conflict.get::<i32, _>("round_number"),
                conflict.get::<Uuid, _>("review_record_id"),
            ))
            .cloned()
            .unwrap_or_default();
        let distinct: HashSet<(String, Option<String>)> = cast
            .iter()
            .map(|(disposition, reason)| {
                (
                    disposition.clone(),
                    if disposition == "exclude" {
                        reason.clone()
                    } else {
                        None
                    },
                )
            })
            .collect();
        if !((cast.len() as i32) >= required && distinct.len() == 1) {
            unresolved += 1;
        }
    }
    Ok(unresolved)
}

async fn run_gates(
    tx: &mut Transaction<'_, Postgres>,
    review_id: Uuid,
) -> Result<(Vec<String>, Uuid), Problem> {
    let protocol_version_id = locked_protocol_id(tx, review_id).await?;
    let unresolved = unresolved_conflict_count(tx, review_id).await?;
    if unresolved > 0 {
        return Err(unprocessable(format!(
            "review {review_id} has {unresolved} unresolved conflict(s); a snapshot taken over an \
             open disagreement records a result nobody agreed"
        )));
    }
    Ok((
        IMPLEMENTED_GATES
            .iter()
            .map(|gate| (*gate).to_owned())
            .collect(),
        protocol_version_id,
    ))
}

// ── member resolution ───────────────────────────────────────────────────────────────────────

struct RequestedMember {
    entity_type: String,
    version_id: Uuid,
}

struct ResolvedMember {
    entity_type: String,
    entity_id: Uuid,
    version_id: Uuid,
    version_number: i32,
    content_hash: String,
    pin: Value,
}

fn member_source(entity_type: &str) -> Option<(&'static str, &'static str, &'static str)> {
    match entity_type {
        "research_object_version" => Some((
            "research_object_versions",
            "research_object_version_id",
            "research_object_id",
        )),
        "extraction_dataset_version" => Some((
            "extraction_dataset_versions",
            "extraction_dataset_version_id",
            "extraction_dataset_id",
        )),
        "observation_version" => Some((
            "observation_versions",
            "observation_version_id",
            "observation_id",
        )),
        "review_record_version" => Some((
            "review_record_versions",
            "review_record_version_id",
            "review_record_id",
        )),
        _ => None,
    }
}

/// Refuse a member whose acceptance chain does not end in `accept`. `review_record_version` is
/// exempt: no acceptance can exist for it, its warrant is transitive.
async fn require_accepted(
    tx: &mut Transaction<'_, Postgres>,
    review_id: Uuid,
    entity_type: &str,
    version_id: Uuid,
) -> Result<(), Problem> {
    if entity_type == "review_record_version" {
        return Ok(());
    }
    let decision: Option<String> = sqlx::query_scalar(
        "WITH RECURSIVE chain AS (
             SELECT acceptance_id, supersedes_acceptance_id, decision
               FROM evidence_acceptances
              WHERE review_id = $1 AND subject_version_id = $2
                AND supersedes_acceptance_id IS NULL
             UNION ALL
             SELECT a.acceptance_id, a.supersedes_acceptance_id, a.decision
               FROM evidence_acceptances a
               JOIN chain c ON a.supersedes_acceptance_id = c.acceptance_id
              WHERE a.review_id = $1
         )
         SELECT c.decision FROM chain c
          WHERE NOT EXISTS (SELECT 1 FROM evidence_acceptances s
                             WHERE s.supersedes_acceptance_id = c.acceptance_id)",
    )
    .bind(review_id)
    .bind(version_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the acceptance chain", error))?;
    if decision.as_deref() != Some("accept") {
        let reason = match decision.as_deref() {
            None => "never accepted".to_owned(),
            Some(other) => format!("latest decision is {other}"),
        };
        return Err(unprocessable(format!(
            "{entity_type} {version_id} is not currently accepted ({reason}); a snapshot pins \
             what the review agreed to, not what it happens to hold"
        )));
    }
    Ok(())
}

async fn resolve_member(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    review_id: Uuid,
    requested: &Value,
    at: &str,
) -> Result<ResolvedMember, Problem> {
    let entity_type = field(requested, "entity_type")
        .map(str::trim)
        .unwrap_or_default()
        .to_owned();
    let Some((table, id_column, entity_column)) = member_source(&entity_type) else {
        return Err(Problem::validation(format!(
            "'{entity_type}' is not a pinnable member kind; this service can pin \
             extraction_dataset_version, observation_version, research_object_version, \
             review_record_version"
        )));
    };
    let raw_version_id = field(requested, "version_id").unwrap_or_default();
    let version_id = raw_version_id.trim().parse::<Uuid>().map_err(|_| {
        Problem::validation(format!(
            "member version_id {raw_version_id:?} is not a uuid"
        ))
    })?;

    let sql = format!(
        "SELECT {id_column} AS version_id, {entity_column} AS entity_id, version_number, content_hash
           FROM {table} WHERE {id_column} = $1 AND review_id = $2"
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(version_id)
        .bind(review_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|error| Problem::from_db("resolving a snapshot member", error))?;
    // NOT a 404: the review is resolved and visible; this is a caller naming something outside it.
    let Some(row) = row else {
        return Err(unprocessable(format!(
            "{entity_type} {version_id} does not belong to review {review_id}"
        )));
    };
    require_accepted(tx, review_id, &entity_type, version_id).await?;

    let entity_id: Uuid = row.get("entity_id");
    let version_number: i32 = row.get("version_number");
    let content_hash: String = row.get("content_hash");
    let member = pin(
        &entity_type,
        &entity_id.to_string(),
        &version_id.to_string(),
        version_number,
        MEMBER_SCHEMA_REF,
        &content_hash,
        &actor.tenant.to_string(),
        at,
    );
    Ok(ResolvedMember {
        entity_type,
        entity_id,
        version_id,
        version_number,
        content_hash,
        pin: member,
    })
}

// ── job row ─────────────────────────────────────────────────────────────────────────────────

struct QueuedJob {
    job_id: Uuid,
}

/// Record the build as a durable job, idempotently on `(tenant, review, type, key)`.
async fn enqueue_job(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    review_id: Uuid,
    command_hash: &str,
    idempotency_key: &str,
) -> Result<QueuedJob, Problem> {
    let existing: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT job_id, command_hash FROM durable_jobs
          WHERE tenant_id = $1 AND review_id = $2 AND job_type = $3 AND idempotency_key = $4",
    )
    .bind(actor.tenant)
    .bind(review_id)
    .bind(JOB_TYPE)
    .bind(idempotency_key)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the job's idempotency key", error))?;
    if let Some((job_id, existing_hash)) = existing {
        if existing_hash != command_hash {
            return Err(Problem::conflict(format!(
                "idempotency key is already bound to job {job_id} with a different command"
            ))
            .with("code", json!("idempotency_conflict")));
        }
        return Ok(QueuedJob { job_id });
    }
    let job_id = new_id();
    sqlx::query(
        "INSERT INTO durable_jobs
             (job_id, tenant_id, job_type, review_id, state, command_hash, idempotency_key,
              originating_subject_id, originating_actor_role)
         VALUES ($1,$2,$3,$4,'queued',$5,$6,$7,$8)",
    )
    .bind(job_id)
    .bind(actor.tenant)
    .bind(JOB_TYPE)
    .bind(review_id)
    .bind(command_hash)
    .bind(idempotency_key)
    .bind(actor.subject.as_deref())
    .bind(actor.role.as_str())
    .execute(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("recording the snapshot job", error))?;
    Ok(QueuedJob { job_id })
}

// ── rows and rendering ──────────────────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
struct SnapshotRow {
    snapshot_id: Uuid,
    tenant_id: Uuid,
    review_id: Uuid,
    version_number: i32,
    supersedes_snapshot_id: Option<Uuid>,
    schema_ref: String,
    intended_use: String,
    protocol_version_id: Uuid,
    corpus_version_id: Option<Uuid>,
    member_count: i32,
    member_manifest_hash: String,
    member_ordering_rule: String,
    content_hash: String,
    gates_evaluated: Vec<String>,
    job_id: Uuid,
    created_by_subject_id: String,
    created_at: DateTime<Utc>,
}

const SNAPSHOT_COLUMNS: &str = "snapshot_id, tenant_id, review_id, version_number, \
     supersedes_snapshot_id, schema_ref, intended_use, protocol_version_id, corpus_version_id, \
     member_count, member_manifest_hash, member_ordering_rule, content_hash, gates_evaluated, \
     job_id, created_by_subject_id, created_at";

#[derive(Debug, sqlx::FromRow)]
struct ArtifactRow {
    artifact_kind: String,
    media_type: String,
    payload_digest: String,
    byte_size: i64,
    serializer_version: String,
}

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
struct MemberRow {
    owner_context: String,
    entity_type: String,
    entity_id: Uuid,
    version_id: Uuid,
    version_number: i32,
    member_schema_ref: String,
    content_hash: String,
    tenant_id: Uuid,
}

async fn artifacts(
    tx: &mut Transaction<'_, Postgres>,
    snapshot_id: Uuid,
) -> Result<Vec<ArtifactRow>, Problem> {
    sqlx::query_as::<_, ArtifactRow>(
        "SELECT artifact_kind, media_type, payload_digest, byte_size, serializer_version
           FROM snapshot_artifacts WHERE snapshot_id = $1 ORDER BY artifact_kind",
    )
    .bind(snapshot_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the snapshot's artifacts", error))
}

fn render(snapshot: &SnapshotRow, artifacts: &[ArtifactRow]) -> Value {
    let evaluated: HashSet<&str> = snapshot
        .gates_evaluated
        .iter()
        .map(String::as_str)
        .collect();
    let gates_not_run: Vec<&str> = {
        let mut pending: Vec<&str> = SNAPSHOT_GATES
            .iter()
            .copied()
            .filter(|gate| !evaluated.contains(gate))
            .collect();
        pending.sort();
        pending
    };
    json!({
        "snapshot_id": snapshot.snapshot_id.to_string(),
        "review_id": snapshot.review_id.to_string(),
        "version_number": snapshot.version_number,
        "supersedes_snapshot_id": snapshot.supersedes_snapshot_id.map(|id| id.to_string()),
        "schema_ref": &snapshot.schema_ref,
        "intended_use": &snapshot.intended_use,
        "member_count": snapshot.member_count,
        "member_manifest_hash": &snapshot.member_manifest_hash,
        "member_ordering_rule": &snapshot.member_ordering_rule,
        "content_hash": &snapshot.content_hash,
        "gates_evaluated": &snapshot.gates_evaluated,
        "fully_gated": gates_not_run.is_empty(),
        "gates_not_run": gates_not_run,
        "job_id": snapshot.job_id.to_string(),
        "created_by": &snapshot.created_by_subject_id,
        "created_at": iso(snapshot.created_at),
        "artifacts": artifacts
            .iter()
            .map(|artifact| json!({
                "artifact_kind": artifact.artifact_kind,
                "media_type": artifact.media_type,
                "payload_digest": artifact.payload_digest,
                "byte_size": artifact.byte_size,
                "serializer_version": artifact.serializer_version,
            }))
            .collect::<Vec<Value>>(),
    })
}

async fn load_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    review_id: Uuid,
    snapshot_id: Uuid,
) -> Result<SnapshotRow, Problem> {
    sqlx::query_as::<_, SnapshotRow>(sqlx::AssertSqlSafe(format!(
        "SELECT {SNAPSHOT_COLUMNS} FROM evidence_snapshots
          WHERE snapshot_id = $1 AND review_id = $2"
    )))
    .bind(snapshot_id)
    .bind(review_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("loading the snapshot", error))?
    .ok_or_else(|| {
        Problem::not_found(format!(
            "snapshot {snapshot_id} was not found in review {review_id}"
        ))
    })
}

/// The stable entity id shared by every version of a review's snapshot chain: the root's id.
async fn entity_of(tx: &mut Transaction<'_, Postgres>, snapshot_id: Uuid) -> Result<Uuid, Problem> {
    let root: Option<Uuid> = sqlx::query_scalar(
        "WITH RECURSIVE chain AS (
             SELECT snapshot_id, supersedes_snapshot_id FROM evidence_snapshots
              WHERE snapshot_id = $1
             UNION ALL
             SELECT s.snapshot_id, s.supersedes_snapshot_id FROM evidence_snapshots s
               JOIN chain c ON s.snapshot_id = c.supersedes_snapshot_id
         )
         SELECT snapshot_id FROM chain WHERE supersedes_snapshot_id IS NULL",
    )
    .bind(snapshot_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("walking the snapshot chain", error))?;
    Ok(root.unwrap_or(snapshot_id))
}

/// Rebuild the manifest from the stored rows and refuse if it does not match the sealed digest.
/// `14-…:364` is checked on EVERY read, not in a test only.
async fn reproduce_manifest(
    tx: &mut Transaction<'_, Postgres>,
    snapshot: &SnapshotRow,
) -> Result<Value, Problem> {
    let rows = sqlx::query_as::<_, MemberRow>(
        "SELECT owner_context, entity_type, entity_id, version_id, version_number,
                member_schema_ref, content_hash, tenant_id
           FROM evidence_snapshot_members WHERE snapshot_id = $1
          ORDER BY owner_context, entity_type, entity_id, version_id",
    )
    .bind(snapshot.snapshot_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the snapshot's members", error))?;
    let at = iso(snapshot.created_at);
    let members: Vec<Value> = rows
        .iter()
        .map(|row| {
            pin(
                &row.entity_type,
                &row.entity_id.to_string(),
                &row.version_id.to_string(),
                row.version_number,
                &row.member_schema_ref,
                &row.content_hash,
                &row.tenant_id.to_string(),
                &at,
            )
        })
        .collect();
    let entity_id = entity_of(tx, snapshot.snapshot_id).await?;
    let manifest = canonical_manifest(
        snapshot.snapshot_id,
        entity_id,
        snapshot.review_id,
        snapshot.version_number,
        snapshot.supersedes_snapshot_id,
        &snapshot.intended_use,
        &at,
        &snapshot.created_by_subject_id,
        &members,
        &snapshot.member_manifest_hash,
    );
    let (sealed, recomputed) = seal(&manifest);
    if recomputed != snapshot.content_hash {
        return Err(Problem::conflict(format!(
            "snapshot {} does not reproduce: stored {}, recomputed {recomputed}. Its members or \
             its serializer have moved since it was sealed.",
            snapshot.snapshot_id, snapshot.content_hash
        )));
    }
    Ok(sealed)
}

// ── handlers ────────────────────────────────────────────────────────────────────────────────

/// Build a snapshot from EXACT pins the caller names.
#[utoipa::path(post, path = "/api/reviews/{review_id}/evidence-snapshots", operation_id = "create_evidence_snapshot", responses((status = 201, description = "ok")))]
pub async fn create_evidence_snapshot(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let pool = pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the snapshot transaction", error))?;
    let subject =
        require_review_lead_for(&mut tx, &actor, review_id, "preparing an evidence snapshot")
            .await?;

    let intended_use = field(&body, "intended_use")
        .map(str::trim)
        .unwrap_or_default()
        .to_owned();
    if intended_use.is_empty() {
        return Err(Problem::validation(
            "intended_use is required; a snapshot must say what it is for",
        ));
    }
    if !intended_use_is_valid(&intended_use) {
        return Err(Problem::validation(format!(
            "intended_use {intended_use:?} must be a lowercase snake_case token; \
             'meta-analysis' is spelled 'meta_analysis'"
        )));
    }
    if carries_guarded_token(&intended_use) {
        return Err(Problem::validation(format!(
            "intended_use {intended_use:?} carries a guarded token; a snapshot may not name \
             itself as an authority this estate has not granted"
        )));
    }
    let raw_members = body
        .get("members")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            Problem::validation(
                "members must be a list of {entity_type, version_id} pins; a snapshot never \
                 infers its own inputs",
            )
        })?;
    if raw_members.is_empty() {
        return Err(Problem::validation(
            "a snapshot names at least one member; an empty snapshot pins nothing and cannot \
             make a run reproducible",
        ));
    }
    let requested: Vec<RequestedMember> = {
        let mut parsed = Vec::with_capacity(raw_members.len());
        for member in raw_members {
            let entity_type = field(member, "entity_type")
                .map(str::trim)
                .unwrap_or_default()
                .to_owned();
            let version_id = field(member, "version_id")
                .map(str::trim)
                .unwrap_or_default()
                .parse::<Uuid>()
                .map_err(|_| Problem::validation("each member version_id must be a uuid"))?;
            parsed.push(RequestedMember {
                entity_type,
                version_id,
            });
        }
        parsed
    };

    // Taken BEFORE the gates: a concurrent acceptance withdraw would otherwise slip between the
    // gate and the member read. `decide_acceptance` takes the same lock.
    sqlx::query("SELECT review_id FROM reviews WHERE review_id = $1 FOR UPDATE")
        .bind(review_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("locking the review", error))?;

    let at = Utc::now();
    let at_iso = iso(at);
    let mut resolved = Vec::with_capacity(requested.len());
    for member in &requested {
        resolved.push(
            resolve_member(
                &mut tx,
                &actor,
                review_id,
                &json!({
                    "entity_type": &member.entity_type,
                    "version_id": member.version_id.to_string(),
                }),
                &at_iso,
            )
            .await?,
        );
    }
    let seen: HashSet<(String, Uuid)> = resolved
        .iter()
        .map(|member| (member.entity_type.clone(), member.entity_id))
        .collect();
    if seen.len() != resolved.len() {
        return Err(Problem::validation(
            "a snapshot names each entity once; two versions of one entity in a single snapshot \
             would make the manifest ambiguous about which one a run may read",
        ));
    }

    let mut command_identities: Vec<String> = resolved
        .iter()
        .map(|member| format!("{}:{}", member.entity_type, member.version_id))
        .collect();
    command_identities.sort();
    let command_hash = hash_value(&json!({
        "review_id": review_id.to_string(),
        "intended_use": intended_use,
        "members": command_identities,
    }));
    let idempotency_key = field(&body, "idempotency_key")
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            command_hash
                .strip_prefix("sha256:")
                .unwrap_or(&command_hash)
                .chars()
                .take(64)
                .collect()
        });
    let job = enqueue_job(&mut tx, &actor, review_id, &command_hash, &idempotency_key).await?;

    // The job may already have built this snapshot: that replay is the queue's idempotency.
    let existing: Option<SnapshotRow> = sqlx::query_as::<_, SnapshotRow>(sqlx::AssertSqlSafe(
        format!("SELECT {SNAPSHOT_COLUMNS} FROM evidence_snapshots WHERE job_id = $1"),
    ))
    .bind(job.job_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("checking the snapshot's job", error))?;
    if let Some(snapshot) = existing {
        let artifacts = artifacts(&mut tx, snapshot.snapshot_id).await?;
        let value = render(&snapshot, &artifacts);
        tx.commit()
            .await
            .map_err(|error| Problem::from_db("committing the snapshot replay", error))?;
        return Ok((StatusCode::CREATED, Json(value)));
    }

    let (gates, protocol_version_id) = run_gates(&mut tx, review_id).await?;

    let previous: Option<(Uuid, i32)> = sqlx::query_as(
        "SELECT snapshot_id, version_number FROM evidence_snapshots
          WHERE review_id = $1 ORDER BY version_number DESC LIMIT 1",
    )
    .bind(review_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading the snapshot head", error))?;
    let snapshot_id = new_id();
    let (version_number, supersedes, entity_id) = match previous {
        Some((previous_id, number)) => (
            number + 1,
            Some(previous_id),
            entity_of(&mut tx, previous_id).await?,
        ),
        None => (1, None, snapshot_id),
    };
    let member_digest = member_manifest_hash(
        &resolved
            .iter()
            .map(|member| member.pin.clone())
            .collect::<Vec<Value>>(),
    );
    let corpus_version_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT corpus_version_id FROM corpus_versions
          WHERE review_id = $1 ORDER BY version_number DESC LIMIT 1",
    )
    .bind(review_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading the review's corpus head", error))?;

    let manifest = canonical_manifest(
        snapshot_id,
        entity_id,
        review_id,
        version_number,
        supersedes,
        &intended_use,
        &at_iso,
        &subject,
        &resolved
            .iter()
            .map(|member| member.pin.clone())
            .collect::<Vec<Value>>(),
        &member_digest,
    );
    let (sealed, content_hash) = seal(&manifest);
    let payload = conveyor_domain::compare::canonical_json(&sealed).into_bytes();

    sqlx::query(
        "INSERT INTO evidence_snapshots
             (snapshot_id, tenant_id, review_id, version_number, supersedes_snapshot_id,
              schema_ref, intended_use, protocol_version_id, corpus_version_id, member_count,
              member_manifest_hash, member_ordering_rule, content_hash, gates_evaluated, job_id,
              created_by_subject_id, created_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)",
    )
    .bind(snapshot_id)
    .bind(actor.tenant)
    .bind(review_id)
    .bind(version_number)
    .bind(supersedes)
    .bind(MANIFEST_SCHEMA_REF)
    .bind(&intended_use)
    .bind(protocol_version_id)
    .bind(corpus_version_id)
    .bind(resolved.len() as i32)
    .bind(&member_digest)
    .bind(PINNED_REF_ORDERING_RULE)
    .bind(&content_hash)
    .bind(&gates)
    .bind(job.job_id)
    .bind(&subject)
    .bind(at)
    .execute(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("writing the snapshot", error))?;

    for member in &resolved {
        sqlx::query(
            "INSERT INTO evidence_snapshot_members
                 (member_id, tenant_id, snapshot_id, owner_context, entity_type, entity_id,
                  version_id, version_number, member_schema_ref, content_hash, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        )
        .bind(new_id())
        .bind(actor.tenant)
        .bind(snapshot_id)
        .bind(OWNER_CONTEXT)
        .bind(&member.entity_type)
        .bind(member.entity_id)
        .bind(member.version_id)
        .bind(member.version_number)
        .bind(MEMBER_SCHEMA_REF)
        .bind(&member.content_hash)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("writing a snapshot member", error))?;
    }

    sqlx::query(
        "INSERT INTO snapshot_artifacts
             (artifact_id, tenant_id, snapshot_id, artifact_kind, media_type, payload_digest,
              byte_size, serializer_version, created_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
    )
    .bind(new_id())
    .bind(actor.tenant)
    .bind(snapshot_id)
    .bind(CANONICAL_MANIFEST_ARTIFACT)
    .bind(CANONICAL_MANIFEST_MEDIA_TYPE)
    .bind(hash_bytes(&payload))
    .bind(payload.len() as i64)
    .bind(SERIALIZER_VERSION)
    .bind(at)
    .execute(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("writing the manifest artifact", error))?;

    // The job row stays `queued`: the snapshot is real, the job is a record, not a claim that a
    // worker ran.
    let snapshot = load_snapshot(&mut tx, review_id, snapshot_id).await?;
    let artifacts = artifacts(&mut tx, snapshot_id).await?;
    let value = render(&snapshot, &artifacts);
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the snapshot", error))?;
    Ok((StatusCode::CREATED, Json(value)))
}

#[utoipa::path(get, path = "/api/reviews/{review_id}/evidence-snapshots", operation_id = "list_evidence_snapshots", responses((status = 200, description = "ok")))]
pub async fn list_evidence_snapshots(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
) -> Result<Json<Value>, Problem> {
    let pool = pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the snapshot transaction", error))?;
    require_review_lead_for(&mut tx, &actor, review_id, "listing evidence snapshots").await?;
    let rows = sqlx::query_as::<_, SnapshotRow>(sqlx::AssertSqlSafe(format!(
        "SELECT {SNAPSHOT_COLUMNS} FROM evidence_snapshots
          WHERE review_id = $1 ORDER BY version_number"
    )))
    .bind(review_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("listing evidence snapshots", error))?;
    let mut items = Vec::with_capacity(rows.len());
    for row in &rows {
        let artifacts = artifacts(&mut tx, row.snapshot_id).await?;
        items.push(render(row, &artifacts));
    }
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the snapshot read", error))?;
    Ok(Json(json!({"items": items})))
}

#[utoipa::path(get, path = "/api/reviews/{review_id}/evidence-snapshots/{snapshot_id}", operation_id = "get_evidence_snapshot", responses((status = 200, description = "ok")))]
pub async fn get_evidence_snapshot(
    State(state): State<Shared>,
    actor: Actor,
    Path((review_id, snapshot_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, Problem> {
    let pool = pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the snapshot transaction", error))?;
    require_review_lead_for(&mut tx, &actor, review_id, "reading an evidence snapshot").await?;
    let snapshot = load_snapshot(&mut tx, review_id, snapshot_id).await?;
    let artifacts = artifacts(&mut tx, snapshot_id).await?;
    let manifest = reproduce_manifest(&mut tx, &snapshot).await?;
    let mut value = render(&snapshot, &artifacts);
    value["manifest"] = manifest;
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the snapshot read", error))?;
    Ok(Json(value))
}
