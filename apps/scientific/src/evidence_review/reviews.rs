//! `/api/reviews` — review lifecycle, protocol locking and stage derivation (`ER-001`).
//!
//! Ported from the reference's `api/routers/reviews.py` + `application/services/reviews.py`.
//! The lifecycle rules live in the database (`review_state_history` is append-only and its tail
//! is the state); these handlers pre-check so the caller gets a typed 409 with a message, while
//! the database remains the guarantee.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::actor::Actor;
use crate::db;
use crate::evidence_review::hash_value;
use crate::problem::Problem;
use crate::state::Shared;

/// The schema pointer the reference hands to clients verbatim. It does not resolve, deliberately:
/// no released document exists, and saying so is truer than minting a URN for nothing.
pub const PROTOCOL_SCHEMA_REF: &str =
    "https://contracts.nonagentic.ai/evidence/review-protocol.v1.schema.json";

/// Every legal lifecycle edge. `archived` has no outgoing edge.
const LEGAL_TRANSITIONS: [(&str, &str); 9] = [
    ("draft", "configured"),
    ("configured", "active"),
    ("active", "paused"),
    ("paused", "active"),
    ("active", "closed"),
    ("paused", "closed"),
    ("closed", "active"),
    ("closed", "archived"),
    ("draft", "archived"),
];

/// The states in which a review's history may still be added to.
const OPEN_STATES: [&str; 4] = ["draft", "configured", "active", "paused"];

#[derive(Debug, sqlx::FromRow)]
pub struct ReviewRow {
    pub review_id: Uuid,
    pub tenant_id: Uuid,
    pub project_id: String,
    pub external_key: String,
    pub title: String,
    pub created_by_subject_id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct ProtocolRow {
    pub protocol_version_id: Uuid,
    pub review_id: Uuid,
    pub version_number: i32,
    pub supersedes_version_id: Option<Uuid>,
    pub schema_ref: String,
    pub content_hash: String,
    pub inclusion_criteria: Value,
    pub exclusion_criteria: Value,
    pub stage_definitions: Value,
    pub exclusion_reason_vocabulary: Value,
    pub reviewers_required: i32,
    pub locked_at: Option<DateTime<Utc>>,
    pub locked_by_subject_id: Option<String>,
    pub created_by_subject_id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct StateEventRow {
    state_event_id: Uuid,
    revision_number: i32,
    from_state: Option<String>,
    to_state: String,
    rationale: String,
    actor_subject_id: String,
    protocol_version_id: Option<Uuid>,
    approval_attestation_ref: Option<String>,
    occurred_at: DateTime<Utc>,
}

fn iso(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn iso_opt(value: Option<DateTime<Utc>>) -> Option<String> {
    value.map(iso)
}

fn field<'a>(body: &'a Value, name: &str) -> Option<&'a str> {
    body.get(name).and_then(Value::as_str)
}

fn required(body: &Value, name: &str) -> Result<String, Problem> {
    field(body, name)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| Problem::validation(format!("{name} is required")))
}

fn optional_revision(body: &Value) -> Result<Option<i64>, Problem> {
    match body.get("expected_revision") {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_i64()
            .map(Some)
            .ok_or_else(|| Problem::validation("expected_revision must be an integer")),
    }
}

/// The tail of the append-only history: `(state, revision)`.
pub async fn current_state(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    review_id: Uuid,
) -> Result<(Option<String>, i64), Problem> {
    let row = sqlx::query(
        "SELECT to_state, revision_number FROM review_state_history
          WHERE review_id = $1 ORDER BY revision_number DESC LIMIT 1",
    )
    .bind(review_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the review's state", error))?;
    Ok(match row {
        Some(row) => (
            Some(row.get::<String, _>("to_state")),
            i64::from(row.get::<i32, _>("revision_number")),
        ),
        None => (None, 0),
    })
}

pub async fn load_review(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    review_id: Uuid,
) -> Result<ReviewRow, Problem> {
    sqlx::query_as::<_, ReviewRow>(
        "SELECT review_id, tenant_id, project_id, external_key, title, created_by_subject_id, created_at
           FROM reviews WHERE review_id = $1",
    )
    .bind(review_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("loading the review", error))?
    .ok_or_else(|| Problem::not_found("no such review"))
}

/// The protocol the review was most recently activated against.
async fn active_protocol(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    review_id: Uuid,
) -> Result<Option<ProtocolRow>, Problem> {
    let id: Option<Uuid> = sqlx::query_scalar(
        "SELECT protocol_version_id FROM review_state_history
          WHERE review_id = $1 AND to_state = 'active' AND protocol_version_id IS NOT NULL
          ORDER BY revision_number DESC LIMIT 1",
    )
    .bind(review_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the active protocol", error))?
    .flatten();
    let Some(id) = id else {
        return Ok(None);
    };
    Ok(Some(protocol_by_id(tx, id).await?))
}

pub async fn protocol_by_id(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    protocol_version_id: Uuid,
) -> Result<ProtocolRow, Problem> {
    sqlx::query_as::<_, ProtocolRow>(
        "SELECT protocol_version_id, review_id, version_number, supersedes_version_id, schema_ref,
                content_hash, inclusion_criteria, exclusion_criteria, stage_definitions,
                exclusion_reason_vocabulary, reviewers_required, locked_at, locked_by_subject_id,
                created_by_subject_id, created_at
           FROM review_protocol_versions WHERE protocol_version_id = $1",
    )
    .bind(protocol_version_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("loading the protocol version", error))?
    .ok_or_else(|| Problem::not_found("no such protocol version for this review"))
}

/// The review lead is a role OR the review's creator. Evaluated per row: a creator who cannot
/// correct their own review is the case the role-less branch exists for.
pub async fn require_review_lead_for(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: &Actor,
    review_id: Uuid,
    what: &str,
) -> Result<String, Problem> {
    let subject = actor.require_human()?.to_owned();
    let review = load_review(tx, review_id).await?;
    if actor.is_lead() || review.created_by_subject_id == subject {
        return Ok(subject);
    }
    Err(Problem::forbidden(format!(
        "{what} requires the review lead role, or having created this review"
    )))
}

/// Refuse a write that would add to a concluded review's history.
pub async fn require_review_open(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    review_id: Uuid,
    what: &str,
) -> Result<String, Problem> {
    let (state, _) = current_state(tx, review_id).await?;
    let Some(state) = state else {
        return Err(Problem::not_found("no such review"));
    };
    if OPEN_STATES.contains(&state.as_str()) {
        return Ok(state);
    }
    let remedy = if state == "closed" {
        "reopen it first, which needs an approval attestation bound to the closed revision"
    } else {
        "an archived review cannot be reopened"
    };
    Err(Problem::conflict(format!(
        "this review is {state}, so {what} would add to the history of a review that has concluded — \
         and an inclusion set may already have been published from it. To continue working on it, {remedy}."
    ))
    .with("review_state", json!(state)))
}

fn protocol_json(protocol: &ProtocolRow) -> Value {
    json!({
        "protocol_version_id": protocol.protocol_version_id.to_string(),
        "review_id": protocol.review_id.to_string(),
        "version_number": protocol.version_number,
        "supersedes_version_id": protocol.supersedes_version_id.map(|id| id.to_string()),
        "schema_ref": protocol.schema_ref,
        "content_hash": protocol.content_hash,
        "inclusion_criteria": protocol.inclusion_criteria,
        "exclusion_criteria": protocol.exclusion_criteria,
        "stage_definitions": protocol.stage_definitions,
        "exclusion_reason_vocabulary": protocol.exclusion_reason_vocabulary,
        "reviewers_required": protocol.reviewers_required,
        "locked_at": iso_opt(protocol.locked_at),
        "locked_by_subject_id": protocol.locked_by_subject_id,
        "created_by_subject_id": protocol.created_by_subject_id,
        "created_at": iso(protocol.created_at),
    })
}

fn summary_json(
    review: &ReviewRow,
    state: Option<&str>,
    revision: i64,
    protocol: Option<&ProtocolRow>,
) -> Value {
    json!({
        "review_id": review.review_id.to_string(),
        "tenant_id": review.tenant_id.to_string(),
        "project_id": review.project_id,
        "external_key": review.external_key,
        "title": review.title,
        "state": state,
        "revision": revision,
        "created_at": iso(review.created_at),
        "created_by_subject_id": review.created_by_subject_id,
        "active_protocol_version_id": protocol.map(|p| p.protocol_version_id.to_string()),
        "reviewers_required": protocol.map(|p| p.reviewers_required),
        "exclusion_reason_vocabulary": protocol.map(|p| p.exclusion_reason_vocabulary.clone()).unwrap_or_else(|| json!([])),
    })
}

async fn summary(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    review: &ReviewRow,
) -> Result<Value, Problem> {
    let (state, revision) = current_state(tx, review.review_id).await?;
    let protocol = active_protocol(tx, review.review_id).await?;
    Ok(summary_json(
        review,
        state.as_deref(),
        revision,
        protocol.as_ref(),
    ))
}

fn etag(status: StatusCode, value: Value, revision: i64) -> Response {
    let mut response = (status, Json(value)).into_response();
    if let Ok(value) = header::HeaderValue::from_str(&format!("\"{revision}\"")) {
        response.headers_mut().insert(header::ETAG, value);
    }
    response
}

// ── handlers ─────────────────────────────────────────────────────────────────────────────────

#[utoipa::path(post, path = "/api/reviews", operation_id = "create_review", responses((status = 201, description = "ok")))]
pub async fn create_review(
    State(state): State<Shared>,
    actor: Actor,
    Json(body): Json<Value>,
) -> Result<Response, Problem> {
    let subject = actor.require_human()?.to_owned();
    let project_id = required(&body, "project_id")?;
    let external_key = required(&body, "external_key")?;
    let title = required(&body, "title")?;
    if external_key.chars().count() > 256 {
        return Err(Problem::validation(
            "external_key must be at most 256 characters",
        ));
    }
    let pool: &PgPool = crate::evidence_review::pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the review transaction", error))?;

    let existing = sqlx::query_as::<_, ReviewRow>(
        "SELECT review_id, tenant_id, project_id, external_key, title, created_by_subject_id, created_at
           FROM reviews WHERE tenant_id = $1 AND project_id = $2 AND external_key = $3",
    )
    .bind(actor.tenant)
    .bind(&project_id)
    .bind(&external_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("checking the review key", error))?;

    let review = match existing {
        Some(review) if review.created_by_subject_id == subject => review,
        Some(_) => {
            return Err(Problem::conflict(
                "a review with this external_key already exists in this project and was created by another subject",
            )
            .with("code", json!("idempotency_conflict"))
            .with("project_id", json!(project_id))
            .with("external_key", json!(external_key)));
        }
        None => {
            let review_id = crate::evidence_review::new_id();
            sqlx::query(
                "INSERT INTO reviews (review_id, tenant_id, project_id, external_key, title, created_by_subject_id)
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(review_id)
            .bind(actor.tenant)
            .bind(&project_id)
            .bind(&external_key)
            .bind(&title)
            .bind(&subject)
            .execute(&mut *tx)
            .await
            .map_err(|error| Problem::from_db("creating the review", error))?;
            sqlx::query(
                "INSERT INTO review_state_history
                     (state_event_id, review_id, tenant_id, revision_number, from_state, to_state,
                      rationale, actor_subject_id)
                 VALUES ($1, $2, $3, 1, NULL, 'draft', 'review created', $4)",
            )
            .bind(crate::evidence_review::new_id())
            .bind(review_id)
            .bind(actor.tenant)
            .bind(&subject)
            .execute(&mut *tx)
            .await
            .map_err(|error| Problem::from_db("writing the review's first state event", error))?;
            load_review(&mut tx, review_id).await?
        }
    };

    let value = summary(&mut tx, &review).await?;
    let revision = value.get("revision").and_then(Value::as_i64).unwrap_or(1);
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the review", error))?;
    Ok(etag(StatusCode::CREATED, value, revision))
}

#[utoipa::path(get, path = "/api/reviews", operation_id = "list_reviews", responses((status = 200, description = "ok")))]
pub async fn list_reviews(
    State(state): State<Shared>,
    actor: Actor,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, Problem> {
    let limit: i64 = query
        .get("limit")
        .map(|raw| raw.parse::<i64>())
        .transpose()
        .map_err(|_| Problem::validation("limit must be an integer"))?
        .unwrap_or(50)
        .clamp(1, 200);
    let include_archived = query
        .get("include_archived")
        .map(|raw| matches!(raw.as_str(), "true" | "1" | "yes"))
        .unwrap_or(false);
    let project_id = query.get("project_id").cloned().filter(|v| !v.is_empty());

    let pool = crate::evidence_review::pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the review transaction", error))?;
    let rows = sqlx::query_as::<_, ReviewRow>(
        "SELECT review_id, tenant_id, project_id, external_key, title, created_by_subject_id, created_at
           FROM reviews r
          WHERE ($1::text IS NULL OR r.project_id = $1)
            AND ($2::bool OR NOT EXISTS (
                  SELECT 1 FROM review_state_history h
                   WHERE h.review_id = r.review_id AND h.to_state = 'archived'))
          ORDER BY r.created_at DESC
          LIMIT $3",
    )
    .bind(project_id.as_deref())
    .bind(include_archived)
    .bind(limit)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("listing reviews", error))?;

    let mut items = Vec::with_capacity(rows.len());
    for row in &rows {
        items.push(summary(&mut tx, row).await?);
    }
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the review list", error))?;
    Ok(Json(json!({"items": items, "count": items.len()})))
}

#[utoipa::path(get, path = "/api/reviews/{review_id}", operation_id = "get_review", responses((status = 200, description = "ok")))]
pub async fn get_review(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
) -> Result<Response, Problem> {
    let pool = crate::evidence_review::pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the review transaction", error))?;
    let review = load_review(&mut tx, review_id).await?;
    let value = summary(&mut tx, &review).await?;
    let revision = value.get("revision").and_then(Value::as_i64).unwrap_or(0);
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the review read", error))?;
    Ok(etag(StatusCode::OK, value, revision))
}

#[utoipa::path(patch, path = "/api/reviews/{review_id}", operation_id = "patch_review", responses((status = 200, description = "ok")))]
pub async fn patch_review(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<Response, Problem> {
    let title = required(&body, "title")?;
    let expected_revision = optional_revision(&body)?;
    let pool = crate::evidence_review::pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the review transaction", error))?;
    require_review_lead_for(&mut tx, &actor, review_id, "correcting a review").await?;
    let (state_name, revision) = current_state(&mut tx, review_id).await?;
    let Some(state_name) = state_name else {
        return Err(Problem::not_found("no such review"));
    };
    if !matches!(state_name.as_str(), "draft" | "configured") {
        return Err(Problem::conflict(format!(
            "a review can only be corrected before activation; this one is {state_name}"
        ))
        .with("review_state", json!(state_name))
        .with("allowed_from", json!(["draft", "configured"])));
    }
    if let Some(expected) = expected_revision
        && expected != revision
    {
        return Err(Problem::new(
            StatusCode::PRECONDITION_FAILED,
            "precondition_failed",
            "Precondition Failed",
            format!("review is at revision {revision}; caller expected {expected}"),
        )
        .with("current_revision", json!(revision)));
    }
    sqlx::query("UPDATE reviews SET title = $1 WHERE review_id = $2")
        .bind(&title)
        .bind(review_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("correcting the review title", error))?;
    let review = load_review(&mut tx, review_id).await?;
    let value = summary(&mut tx, &review).await?;
    let revision = value.get("revision").and_then(Value::as_i64).unwrap_or(0);
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the correction", error))?;
    Ok(etag(StatusCode::OK, value, revision))
}

#[utoipa::path(get, path = "/api/reviews/{review_id}/state-history", operation_id = "get_review_state_history", responses((status = 200, description = "ok")))]
pub async fn state_history(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
) -> Result<Json<Value>, Problem> {
    let pool = crate::evidence_review::pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the review transaction", error))?;
    load_review(&mut tx, review_id).await?;
    let rows = sqlx::query_as::<_, StateEventRow>(
        "SELECT state_event_id, revision_number, from_state, to_state, rationale, actor_subject_id,
                protocol_version_id, approval_attestation_ref, occurred_at
           FROM review_state_history WHERE review_id = $1 ORDER BY revision_number",
    )
    .bind(review_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading the state history", error))?;
    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "state_event_id": row.state_event_id.to_string(),
                "revision_number": row.revision_number,
                "from_state": row.from_state,
                "to_state": row.to_state,
                "rationale": row.rationale,
                "actor_subject_id": row.actor_subject_id,
                "protocol_version_id": row.protocol_version_id.map(|id| id.to_string()),
                "approval_attestation_ref": row.approval_attestation_ref,
                "occurred_at": iso(row.occurred_at),
            })
        })
        .collect();
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the history read", error))?;
    Ok(Json(json!({"items": items, "count": items.len()})))
}

#[utoipa::path(post, path = "/api/reviews/{review_id}/protocol-versions", operation_id = "create_protocol_version", responses((status = 201, description = "ok")))]
pub async fn create_protocol_version(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let inclusion = body
        .get("inclusion_criteria")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let exclusion = body
        .get("exclusion_criteria")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let stages = body
        .get("stage_definitions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let vocabulary: Vec<String> = body
        .get("exclusion_reason_vocabulary")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let reviewers_required = body
        .get("reviewers_required")
        .and_then(Value::as_i64)
        .unwrap_or(2);
    if stages.is_empty() {
        return Err(Problem::validation("a protocol needs at least one stage"));
    }
    if vocabulary.is_empty() {
        return Err(Problem::validation(
            "the exclusion-reason vocabulary is closed and must be non-empty; an exclusion has to name a reason from it",
        ));
    }
    if !(1..=10).contains(&reviewers_required) {
        return Err(Problem::validation(
            "reviewers_required must be between 1 and 10",
        ));
    }

    let pool = crate::evidence_review::pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the protocol transaction", error))?;
    let subject =
        require_review_lead_for(&mut tx, &actor, review_id, "creating a protocol version").await?;
    require_review_open(&mut tx, review_id, "creating a protocol version").await?;

    let previous: Option<(Uuid, i32)> = sqlx::query_as(
        "SELECT protocol_version_id, version_number FROM review_protocol_versions
          WHERE review_id = $1 ORDER BY version_number DESC LIMIT 1",
    )
    .bind(review_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading the previous protocol version", error))?;

    let mut sorted_vocabulary = vocabulary;
    sorted_vocabulary.sort();
    let content = json!({
        "inclusion_criteria": inclusion,
        "exclusion_criteria": exclusion,
        "stage_definitions": stages.clone(),
        "exclusion_reason_vocabulary": sorted_vocabulary.clone(),
        "reviewers_required": reviewers_required,
    });
    let content_hash = hash_value(&content);
    let (version_number, supersedes) = match previous {
        Some((id, number)) => (number + 1, Some(id)),
        None => (1, None),
    };
    let protocol_version_id = crate::evidence_review::new_id();
    sqlx::query(
        "INSERT INTO review_protocol_versions
             (protocol_version_id, review_id, tenant_id, version_number, supersedes_version_id,
              schema_ref, content_hash, inclusion_criteria, exclusion_criteria, stage_definitions,
              exclusion_reason_vocabulary, reviewers_required, created_by_subject_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
    )
    .bind(protocol_version_id)
    .bind(review_id)
    .bind(actor.tenant)
    .bind(version_number)
    .bind(supersedes)
    .bind(PROTOCOL_SCHEMA_REF)
    .bind(&content_hash)
    .bind(&inclusion)
    .bind(&exclusion)
    .bind(json!(stages))
    .bind(json!(sorted_vocabulary))
    .bind(reviewers_required as i32)
    .bind(&subject)
    .execute(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("creating the protocol version", error))?;
    let protocol = protocol_by_id(&mut tx, protocol_version_id).await?;
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the protocol version", error))?;
    Ok((StatusCode::CREATED, Json(protocol_json(&protocol))))
}

#[utoipa::path(get, path = "/api/reviews/{review_id}/protocol-versions", operation_id = "list_protocol_versions", responses((status = 200, description = "ok")))]
pub async fn list_protocol_versions(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
) -> Result<Json<Value>, Problem> {
    let pool = crate::evidence_review::pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the protocol transaction", error))?;
    load_review(&mut tx, review_id).await?;
    let rows = sqlx::query_as::<_, ProtocolRow>(
        "SELECT protocol_version_id, review_id, version_number, supersedes_version_id, schema_ref,
                content_hash, inclusion_criteria, exclusion_criteria, stage_definitions,
                exclusion_reason_vocabulary, reviewers_required, locked_at, locked_by_subject_id,
                created_by_subject_id, created_at
           FROM review_protocol_versions WHERE review_id = $1 ORDER BY version_number",
    )
    .bind(review_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("listing protocol versions", error))?;
    let items: Vec<Value> = rows.iter().map(protocol_json).collect();
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the protocol read", error))?;
    Ok(Json(json!({"items": items, "count": items.len()})))
}

/// Nothing a stage set gives meaning to may exist. Mirrors `0013`'s correction window.
async fn stage_set_is_correctable(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    review_id: Uuid,
) -> Result<bool, Problem> {
    let correctable: bool = sqlx::query_scalar(
        "SELECT NOT (
             EXISTS (SELECT 1 FROM screening_assignments a WHERE a.review_id = $1)
          OR EXISTS (SELECT 1 FROM screening_decisions d WHERE d.review_id = $1)
          OR EXISTS (SELECT 1 FROM screening_conflicts c WHERE c.review_id = $1)
          OR EXISTS (SELECT 1 FROM inclusion_exports e WHERE e.review_id = $1)
          OR EXISTS (SELECT 1 FROM review_state_history h WHERE h.review_id = $1 AND h.to_state = 'active')
        )",
    )
    .bind(review_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the correction window", error))?;
    Ok(correctable)
}

#[utoipa::path(post, path = "/api/reviews/{review_id}/protocol-versions/{protocol_version_id}:lock", operation_id = "lock_protocol_version", responses((status = 200, description = "ok")))]
pub async fn lock_protocol_version(
    State(state): State<Shared>,
    actor: Actor,
    Path((review_id, protocol_version_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, Problem> {
    let pool = crate::evidence_review::pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the protocol transaction", error))?;
    let subject =
        require_review_lead_for(&mut tx, &actor, review_id, "locking a protocol version").await?;
    require_review_open(&mut tx, review_id, "locking a protocol version").await?;
    let protocol = protocol_by_id(&mut tx, protocol_version_id).await?;
    if protocol.review_id != review_id {
        return Err(Problem::not_found(
            "no such protocol version for this review",
        ));
    }
    if protocol.locked_at.is_some() {
        return Err(Problem::conflict("protocol version is already locked"));
    }

    let existing_orders: Vec<i32> =
        sqlx::query_scalar("SELECT stage_order FROM review_stages WHERE review_id = $1")
            .bind(review_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(|error| Problem::from_db("reading the review's stages", error))?;
    let stages = protocol
        .stage_definitions
        .as_array()
        .cloned()
        .unwrap_or_default();
    let incoming: Vec<i32> = (1..=stages.len() as i32).collect();
    let collisions: Vec<i32> = existing_orders
        .iter()
        .filter(|order| incoming.contains(order))
        .copied()
        .collect();
    if !collisions.is_empty() && !stage_set_is_correctable(&mut tx, review_id).await? {
        return Err(Problem::conflict(
            "this review already has stages at these positions, and the ledger orders every decision in \
             the review by stage position — locking a second stage set would leave two stages sharing a \
             position and an alphabetical accident deciding which one settles a record. The stage set \
             can only be corrected while the review has never been activated and holds no assignment, \
             decision, conflict or sealed export; this one does.",
        )
        .with("stage_orders", json!(collisions)));
    }
    if !existing_orders.is_empty() {
        sqlx::query("DELETE FROM review_stages WHERE review_id = $1")
            .bind(review_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| Problem::from_db("replacing the review's stages", error))?;
    }

    sqlx::query(
        "UPDATE review_protocol_versions SET locked_at = now(), locked_by_subject_id = $1
          WHERE protocol_version_id = $2",
    )
    .bind(&subject)
    .bind(protocol_version_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("locking the protocol version", error))?;

    for (index, definition) in stages.iter().enumerate() {
        let order = (index + 1) as i32;
        let key = definition
            .get("key")
            .or_else(|| definition.get("stage_key"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("stage-{order}"));
        let name = definition
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| key.clone());
        let required = definition
            .get("reviewers_required")
            .and_then(Value::as_i64)
            .unwrap_or(i64::from(protocol.reviewers_required)) as i32;
        sqlx::query(
            "INSERT INTO review_stages
                 (stage_id, review_id, tenant_id, protocol_version_id, stage_key, stage_order, name, reviewers_required)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(crate::evidence_review::new_id())
        .bind(review_id)
        .bind(actor.tenant)
        .bind(protocol_version_id)
        .bind(&key)
        .bind(order)
        .bind(&name)
        .bind(required)
        .execute(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("materialising the protocol's stages", error))?;
    }
    let locked = protocol_by_id(&mut tx, protocol_version_id).await?;
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the protocol lock", error))?;
    Ok(Json(protocol_json(&locked)))
}

/// The one transition writer every lifecycle command funnels through.
#[allow(clippy::too_many_arguments)]
async fn transition(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: &Actor,
    review_id: Uuid,
    to_state: &str,
    rationale: &str,
    expected_revision: Option<i64>,
    protocol_version_id: Option<Uuid>,
    approval_attestation_ref: Option<&str>,
) -> Result<i64, Problem> {
    let subject = require_review_lead_for(tx, actor, review_id, "a lifecycle transition").await?;
    let (state, revision) = current_state(tx, review_id).await?;
    let Some(state) = state else {
        return Err(Problem::not_found("no such review"));
    };
    if let Some(expected) = expected_revision
        && expected != revision
    {
        return Err(Problem::new(
            StatusCode::PRECONDITION_FAILED,
            "precondition_failed",
            "Precondition Failed",
            format!("review is at revision {revision}; caller expected {expected}"),
        )
        .with("current_revision", json!(revision)));
    }
    if !LEGAL_TRANSITIONS
        .iter()
        .any(|(from, to)| *from == state.as_str() && *to == to_state)
    {
        return Err(
            Problem::conflict(format!("illegal transition {state} -> {to_state}"))
                .with("current_state", json!(state)),
        );
    }
    if state == "closed"
        && to_state == "active"
        && approval_attestation_ref.unwrap_or("").trim().is_empty()
    {
        return Err(Problem::validation(
            "a reopen is always attested; supply the FR approval attestation bound to the closed revision",
        ));
    }
    if to_state == "closed" {
        let unresolved: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM screening_conflicts c
              WHERE c.review_id = $1
                AND NOT EXISTS (SELECT 1 FROM screening_adjudications a WHERE a.conflict_id = c.conflict_id)",
        )
        .bind(review_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(|error| Problem::from_db("reading unresolved disagreements", error))?;
        if unresolved > 0 {
            return Err(Problem::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "unprocessable_state",
                "Unprocessable State",
                format!(
                    "{unresolved} disagreements are unresolved, and closing over them would strand them: \
                     a closed review takes no further writes, so adjudicating afterwards needs an attested \
                     reopen. Resolve them first, or reopen later and accept the attestation."
                ),
            )
            .with("unresolved_conflicts", json!(unresolved)));
        }
    }
    let protocol = if to_state == "active" && protocol_version_id.is_none() {
        let carried = active_protocol(tx, review_id).await?;
        Some(
            carried
                .ok_or_else(|| {
                    Problem::validation(
                        "this review has never been activated against a locked protocol",
                    )
                })?
                .protocol_version_id,
        )
    } else {
        protocol_version_id
    };
    sqlx::query(
        "INSERT INTO review_state_history
             (state_event_id, review_id, tenant_id, revision_number, from_state, to_state,
              rationale, actor_subject_id, protocol_version_id, approval_attestation_ref)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
    )
    .bind(crate::evidence_review::new_id())
    .bind(review_id)
    .bind(actor.tenant)
    .bind((revision + 1) as i32)
    .bind(&state)
    .bind(to_state)
    .bind(rationale)
    .bind(&subject)
    .bind(protocol)
    .bind(approval_attestation_ref)
    .execute(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("writing the state transition", error))?;
    Ok(revision + 1)
}

async fn lifecycle(
    state: Shared,
    actor: Actor,
    review_id: Uuid,
    body: Value,
    to_state: &str,
    from_states: &[&str],
) -> Result<Response, Problem> {
    let rationale = required(&body, "rationale")?;
    let expected_revision = optional_revision(&body)?;
    let attestation = field(&body, "approval_attestation_ref").map(str::to_owned);
    let pool = crate::evidence_review::pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the lifecycle transaction", error))?;
    let (current, _) = current_state(&mut tx, review_id).await?;
    let Some(current) = current else {
        return Err(Problem::not_found("no such review"));
    };
    if !from_states.contains(&current.as_str()) {
        return Err(Problem::conflict(format!(
            "{to_state} applies to a review in {}; this review is {current}",
            from_states.join(" or ")
        ))
        .with("current_state", json!(current))
        .with("allowed_from", json!(from_states)));
    }
    let revision = transition(
        &mut tx,
        &actor,
        review_id,
        to_state,
        &rationale,
        expected_revision,
        None,
        attestation.as_deref(),
    )
    .await?;
    let review = load_review(&mut tx, review_id).await?;
    let value = summary(&mut tx, &review).await?;
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the lifecycle transition", error))?;
    Ok(etag(StatusCode::OK, value, revision))
}

#[utoipa::path(post, path = "/api/reviews/{review_id}:activate", operation_id = "activate_review", responses((status = 200, description = "ok")))]
pub async fn activate(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<Response, Problem> {
    let rationale = required(&body, "rationale")?;
    let expected_revision = optional_revision(&body)?;
    let protocol_version_id = field(&body, "protocol_version_id")
        .ok_or_else(|| Problem::validation("protocol_version_id is required"))?
        .parse::<Uuid>()
        .map_err(|_| Problem::validation("protocol_version_id must be a UUID"))?;
    let pool = crate::evidence_review::pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the activation transaction", error))?;
    require_review_lead_for(&mut tx, &actor, review_id, "activating a review").await?;
    let protocol = protocol_by_id(&mut tx, protocol_version_id).await?;
    if protocol.review_id != review_id {
        return Err(Problem::not_found(
            "no such protocol version for this review",
        ));
    }
    if protocol.locked_at.is_none() {
        return Err(Problem::validation(
            "activation requires a LOCKED protocol version (DM-AT-02); lock it first",
        ));
    }
    let (current, _) = current_state(&mut tx, review_id).await?;
    let mut expected = expected_revision;
    if current.as_deref() == Some("draft") {
        transition(
            &mut tx,
            &actor,
            review_id,
            "configured",
            &format!("protocol {} locked", protocol.version_number),
            expected,
            None,
            None,
        )
        .await?;
        expected = None;
    }
    let revision = transition(
        &mut tx,
        &actor,
        review_id,
        "active",
        &rationale,
        expected,
        Some(protocol_version_id),
        None,
    )
    .await?;
    let review = load_review(&mut tx, review_id).await?;
    let value = summary(&mut tx, &review).await?;
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the activation", error))?;
    Ok(etag(StatusCode::OK, value, revision))
}

#[utoipa::path(post, path = "/api/reviews/{review_id}:pause", operation_id = "pause_review", responses((status = 200, description = "ok")))]
pub async fn pause(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<Response, Problem> {
    lifecycle(state, actor, review_id, body, "paused", &["active"]).await
}

#[utoipa::path(post, path = "/api/reviews/{review_id}:resume", operation_id = "resume_review", responses((status = 200, description = "ok")))]
pub async fn resume(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<Response, Problem> {
    lifecycle(state, actor, review_id, body, "active", &["paused"]).await
}

#[utoipa::path(post, path = "/api/reviews/{review_id}:close", operation_id = "close_review", responses((status = 200, description = "ok")))]
pub async fn close(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<Response, Problem> {
    lifecycle(
        state,
        actor,
        review_id,
        body,
        "closed",
        &["active", "paused"],
    )
    .await
}

#[utoipa::path(post, path = "/api/reviews/{review_id}:reopen", operation_id = "reopen_review", responses((status = 200, description = "ok")))]
pub async fn reopen(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<Response, Problem> {
    lifecycle(state, actor, review_id, body, "active", &["closed"]).await
}

#[utoipa::path(post, path = "/api/reviews/{review_id}:archive", operation_id = "archive_review", responses((status = 200, description = "ok")))]
pub async fn archive(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<Response, Problem> {
    lifecycle(
        state,
        actor,
        review_id,
        body,
        "archived",
        &["draft", "closed"],
    )
    .await
}

#[utoipa::path(post, path = "/api/reviews/{review_id}/archive", operation_id = "archive_review_slash", responses((status = 200, description = "ok")))]
pub async fn archive_slash(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<Response, Problem> {
    lifecycle(
        state,
        actor,
        review_id,
        body,
        "archived",
        &["draft", "closed"],
    )
    .await
}
