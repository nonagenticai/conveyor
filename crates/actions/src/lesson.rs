#![forbid(unsafe_code)]
//! T8 — the organisational-memory action family (SPEC_V2 §7.6, §10.3, §10.4, §12 T8, D36;
//! issue #79).
//!
//! One gated lifecycle over the `lesson` row T10 created:
//!
//! - `lessons.propose` — a proposal names its evidence refs; every ref is **resolved** to a live
//!   card or run in this tenant, and the independent count is the number of distinct resolved
//!   objects. Fewer than two is `422 lesson_evidence_insufficient` and nothing is stored. The
//!   statement's hash identifies the proposal, so an identical re-proposal finds its row.
//! - `lessons.review` — a person holding `lessons:review` approves or rejects. Approval promotes
//!   in the same act: the lesson becomes `promoted` and a `governed_asset(kind =
//!   'lesson_source', status = 'draft')` source draft exists. Rejection is durable.
//! - `lessons.promote` — the explicit promotion of a reviewed lesson, and an idempotent read of
//!   an already-promoted one. Always a person's act; a job's executor cannot reach it.
//!
//! Nothing here publishes a pack, writes a `capability_knowledge` pin, stores an embedding or
//! records an `inference_call`: the conveyor hosts neither a model nor retrieval (§7.6, last
//! rule).

use chrono::{DateTime, Utc};
use conveyor_store as store;
use conveyor_store::lesson::{
    EvidenceRef, LessonRow, LessonStoreError, NewLesson, independent_anchors, independent_count,
    resolve_evidence_tx,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::control::{invalid, store_error};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// One evidence reference as a proposal names it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LessonEvidenceIn {
    /// `card` or `run`. Resolved to a live row in the caller's tenant.
    pub kind: String,
    /// The referenced row's id.
    pub id: Uuid,
}

/// `lessons.propose`'s input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LessonProposeIn {
    /// The lesson: one declarative statement the organisation should learn.
    pub statement: String,
    /// Its evidence refs. At least two distinct cards, runs (or, once T7 lands, incidents).
    pub evidence: Vec<LessonEvidenceIn>,
    /// The programme this lesson belongs to, when any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub programme_id: Option<Uuid>,
    /// The cadence whose retro surfaced it, when any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cadence_id: Option<Uuid>,
    /// The retro finding key, when any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finding_key: Option<String>,
    /// Free-form metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// `lessons.review`'s input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LessonReviewIn {
    /// The lesson being decided.
    pub lesson_id: Uuid,
    /// `approve` (promotes to a source draft) or `reject` (durable).
    pub decision: String,
    /// The reviewer's note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// `lessons.promote`'s input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LessonPromoteIn {
    /// The reviewed lesson to promote.
    pub lesson_id: Uuid,
}

/// One lesson as the actions return it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LessonView {
    /// Row id.
    pub id: String,
    /// The statement.
    pub statement: String,
    /// The evidence refs, as stored.
    pub evidence: Value,
    /// Distinct independent refs.
    pub independent_count: i64,
    /// `proposed | reviewed | promoted | rejected`.
    pub state: String,
    /// `retro | propose | execution`.
    pub source: String,
    /// The programme context, when any.
    pub programme_id: Option<String>,
    /// The cadence context, when any.
    pub cadence_id: Option<String>,
    /// The retro finding key, when any.
    pub finding_key: Option<String>,
    /// The proposing principal, when known.
    pub proposer_id: Option<String>,
    /// The proposer's kind.
    pub proposer_kind: Option<String>,
    /// The reviewing person, when reviewed.
    pub reviewer_id: Option<String>,
    /// When the review decision was recorded.
    pub reviewed_at: Option<DateTime<Utc>>,
    /// The reviewer's note.
    pub review_note: Option<String>,
    /// The governed-asset source draft promotion produced.
    pub source_asset_id: Option<String>,
    /// When promotion happened.
    pub promoted_at: Option<DateTime<Utc>>,
    /// The row version.
    pub version: i64,
    /// When it was recorded.
    pub created_at: DateTime<Utc>,
    /// When it last changed.
    pub updated_at: DateTime<Utc>,
}

fn hex(id: Uuid) -> String {
    id.to_string()
}

fn view(row: LessonRow) -> LessonView {
    LessonView {
        id: hex(row.id),
        statement: row.statement,
        evidence: row.evidence,
        independent_count: i64::from(row.independent_count),
        state: row.state,
        source: row.source,
        programme_id: row.programme_id.map(hex),
        cadence_id: row.cadence_id.map(hex),
        finding_key: if row.finding_key.trim().is_empty() {
            None
        } else {
            Some(row.finding_key)
        },
        proposer_id: row.proposer_id.map(hex),
        proposer_kind: row.proposer_kind,
        reviewer_id: row.reviewed_by.map(hex),
        reviewed_at: row.reviewed_at,
        review_note: row.review_note,
        source_asset_id: row.promoted_asset_id.map(hex),
        promoted_at: row.promoted_at,
        version: row.version,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn lesson_error(error: LessonStoreError, action: &str) -> ActionError {
    match error {
        LessonStoreError::NotFound(id) => ActionError::new(
            404,
            "lesson_not_found",
            format!("{action}: no lesson {id} in this tenant"),
        ),
        LessonStoreError::EvidenceUnknown { kind, id } => ActionError::new(
            422,
            "lesson_evidence_unknown",
            format!("{action}: the {kind} evidence reference {id} does not exist in this tenant"),
        ),
        LessonStoreError::EvidenceKindUnsupported(kind) => ActionError::new(
            422,
            "lesson_evidence_kind_unsupported",
            format!(
                "{action}: evidence kind {kind:?} cannot be verified by this build; \
                 use card or run"
            ),
        ),
        LessonStoreError::EvidenceInsufficient(count) => ActionError::new(
            422,
            "lesson_evidence_insufficient",
            format!(
                "{action}: counted {count} independent evidence reference(s); a lesson needs at \
                 least two (different cards, runs or incidents), and two refs to the same card \
                 or run are one"
            ),
        ),
        LessonStoreError::FingerprintTaken { id, state } => ActionError::new(
            409,
            "lesson_exists",
            format!("{action}: lesson {id} already holds this statement in state {state}"),
        ),
        LessonStoreError::RejectedWithoutNewEvidence(id) => ActionError::new(
            409,
            "lesson_rejected",
            format!(
                "{action}: lesson {id} was rejected; only a new independent evidence reference \
                 reopens it"
            ),
        ),
        LessonStoreError::IllegalState {
            id,
            state,
            expected,
        } => ActionError::new(
            409,
            "lesson_not_reviewable",
            format!("{action}: lesson {id} is {state}; the action requires {expected}"),
        ),
        LessonStoreError::ProgrammeMissing(id) => invalid(
            "programme_id",
            "not_found",
            format!("no programme {id} in this tenant"),
        ),
        LessonStoreError::CadenceMissing(id) => invalid(
            "cadence_id",
            "not_found",
            format!("no cadence {id} in this tenant"),
        ),
        LessonStoreError::Database(error) => store_error(error, action),
    }
}

/// The statement hash that identifies a re-proposal. Only the statement is hashed: new evidence
/// must find the rejected row it belongs to, not create a second one.
fn statement_fingerprint(statement: &str) -> String {
    crate::spec::sha256_hex(format!("lesson:v1\0{}", statement.trim()).as_bytes())
}

fn meta_object(meta: Option<Value>) -> Value {
    match meta {
        Some(value) if value.is_object() => value,
        _ => json!({}),
    }
}

/// Propose a lesson with its independent evidence references.
#[crate::action(
    name = "lessons.propose",
    version = 1,
    kind = "internal",
    scopes = ["lessons:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn lessons_propose(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: LessonProposeIn,
) -> Result<HandlerOutput<LessonView>, ActionError> {
    let statement = input.statement.trim();
    if statement.is_empty() {
        return Err(invalid("statement", "required", "must not be empty"));
    }
    if let Some(programme_id) = input.programme_id
        && !store::lesson::programme_exists_tx(&mut *tx, ctx.tenant, programme_id)
            .await
            .map_err(|error| lesson_error(error, "lessons.propose"))?
    {
        return Err(lesson_error(
            LessonStoreError::ProgrammeMissing(programme_id),
            "lessons.propose",
        ));
    }
    if let Some(cadence_id) = input.cadence_id
        && !store::lesson::cadence_exists_tx(&mut *tx, ctx.tenant, cadence_id)
            .await
            .map_err(|error| lesson_error(error, "lessons.propose"))?
    {
        return Err(lesson_error(
            LessonStoreError::CadenceMissing(cadence_id),
            "lessons.propose",
        ));
    }

    let refs: Vec<EvidenceRef> = input
        .evidence
        .iter()
        .map(|reference| EvidenceRef {
            kind: reference.kind.clone(),
            id: reference.id,
        })
        .collect();
    let resolved = resolve_evidence_tx(&mut *tx, ctx.tenant, &refs)
        .await
        .map_err(|error| lesson_error(error, "lessons.propose"))?;
    let count = independent_count(&store::lesson::evidence_json(&resolved));
    if count < 2 {
        return Err(lesson_error(
            LessonStoreError::EvidenceInsufficient(count),
            "lessons.propose",
        ));
    }

    let fingerprint = statement_fingerprint(statement);
    let meta = meta_object(input.meta);
    let finding_key = input.finding_key.as_deref().unwrap_or("").trim().to_owned();

    // A rejected proposal is durable. The same statement reopens it only when the caller brings
    // an independent reference the rejected row did not already hold.
    if let Some(existing) =
        store::lesson::find_by_fingerprint_tx(&mut *tx, ctx.tenant, &fingerprint)
            .await
            .map_err(|error| lesson_error(error, "lessons.propose"))?
    {
        if existing.state != "rejected" {
            return Err(lesson_error(
                LessonStoreError::FingerprintTaken {
                    id: existing.id,
                    state: existing.state,
                },
                "lessons.propose",
            ));
        }
        let known = independent_anchors(&existing.evidence);
        let mut merged: Vec<Value> = existing.evidence.as_array().cloned().unwrap_or_default();
        for reference in &resolved {
            if !known.contains(&reference.anchor) {
                merged.push(json!({"kind": reference.kind, "id": reference.id}));
            }
        }
        let merged = Value::Array(merged);
        let reopened_count = independent_count(&merged);
        if reopened_count < 2 {
            return Err(lesson_error(
                LessonStoreError::EvidenceInsufficient(reopened_count),
                "lessons.propose",
            ));
        }
        let row = store::lesson::reopen_rejected_tx(
            &mut *tx,
            ctx.tenant,
            existing.id,
            merged,
            reopened_count,
            Some((ctx.actor_id, ctx.actor_kind.clone())),
            meta,
        )
        .await
        .map_err(|error| lesson_error(error, "lessons.propose"))?;
        return Ok(HandlerOutput::ok(view(row)));
    }

    let row = store::lesson::insert_tx(
        &mut *tx,
        ctx.tenant,
        NewLesson {
            programme_id: input.programme_id,
            cadence_id: input.cadence_id,
            finding_key,
            statement: statement.to_owned(),
            evidence: resolved,
            independent_count: count,
            source: "propose".to_owned(),
            fingerprint: Some(fingerprint),
            proposer_id: Some(ctx.actor_id),
            proposer_kind: Some(ctx.actor_kind.clone()),
            meta,
        },
    )
    .await
    .map_err(|error| lesson_error(error, "lessons.propose"))?;
    Ok(HandlerOutput::created(view(row)))
}

/// Review a lesson; approval promotes it to a knowledge-source draft.
#[crate::action(
    name = "lessons.review",
    version = 1,
    kind = "internal",
    scopes = ["lessons:review"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn lessons_review(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: LessonReviewIn,
) -> Result<HandlerOutput<LessonView>, ActionError> {
    if ctx.actor_kind != "user" {
        return Err(ActionError::new(
            403,
            "lesson_review_requires_human",
            format!(
                "lessons.review: D36 requires a person's review, not a {} principal's",
                ctx.actor_kind
            ),
        ));
    }
    let decision = input.decision.trim().to_ascii_lowercase();
    let approve = match decision.as_str() {
        "approve" | "approved" => true,
        "reject" | "rejected" => false,
        _ => {
            return Err(invalid(
                "decision",
                "invalid_value",
                "expected approve or reject",
            ));
        }
    };
    let lesson = store::lesson::get_tx(&mut *tx, ctx.tenant, input.lesson_id)
        .await
        .map_err(|error| lesson_error(error, "lessons.review"))?;
    if lesson.proposer_id == Some(ctx.actor_id) {
        return Err(ActionError::new(
            409,
            "lesson_self_review",
            format!(
                "lessons.review: {} proposed this lesson; the proposer cannot review it",
                ctx.actor_id
            ),
        ));
    }
    let note = input
        .note
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty());
    let reviewed = store::lesson::review_tx(
        &mut *tx,
        ctx.tenant,
        lesson.id,
        &decision,
        ctx.actor_id,
        note,
    )
    .await
    .map_err(|error| lesson_error(error, "lessons.review"))?;
    if !approve {
        return Ok(HandlerOutput::ok(view(reviewed)));
    }
    let promoted = store::lesson::promote_tx(&mut *tx, ctx.tenant, lesson.id, Some(ctx.actor_id))
        .await
        .map_err(|error| lesson_error(error, "lessons.review"))?;
    Ok(HandlerOutput::ok(view(promoted)))
}

/// Promote a reviewed lesson to a source draft. Idempotent on an already-promoted lesson.
#[crate::action(
    name = "lessons.promote",
    version = 1,
    kind = "internal",
    scopes = ["lessons:review"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn lessons_promote(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: LessonPromoteIn,
) -> Result<HandlerOutput<LessonView>, ActionError> {
    if ctx.actor_kind != "user" {
        return Err(ActionError::new(
            403,
            "lesson_promote_requires_human",
            format!(
                "lessons.promote: promotion follows a person's review, not a {} principal's",
                ctx.actor_kind
            ),
        ));
    }
    let lesson = store::lesson::get_tx(&mut *tx, ctx.tenant, input.lesson_id)
        .await
        .map_err(|error| lesson_error(error, "lessons.promote"))?;
    if lesson.state != "reviewed" && lesson.state != "promoted" {
        return Err(ActionError::new(
            409,
            "lesson_not_reviewed",
            format!(
                "lessons.promote: lesson {} is {}; a person must approve it before promotion",
                lesson.id, lesson.state
            ),
        ));
    }
    let row = store::lesson::promote_tx(&mut *tx, ctx.tenant, lesson.id, Some(ctx.actor_id))
        .await
        .map_err(|error| lesson_error(error, "lessons.promote"))?;
    Ok(HandlerOutput::ok(view(row)))
}
