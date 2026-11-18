//! `/api/reviews/{review_id}` — accepted evidence (`ER-006`).
//!
//! Ported from the reference's `api/routers/evidence.py` + `application/services/evidence.py`.
//! Research objects, manual extraction, research assertions and acceptance. The authority split
//! lives here, not in the routes: recording is a reviewer's work, accepting is the lead's, and
//! `decide_acceptance` is the only handler behind the review-lead gate.
//!
//! Three rules decide every write:
//!
//! * **a machine cannot decide** — every handler here records an attributable human judgement and
//!   calls [`Actor::require_human`] before its first statement (the acceptance route holds the
//!   review-lead gate instead, which requires a human itself);
//! * **evidence is judged against a protocol that cannot still change** — every version row pins
//!   the review's locked protocol version, and the database trigger `er_pins_locked_protocol`
//!   refuses an unlocked one even when this module forgets;
//! * **corrections are new versions** — an existing `variable_key`, `dataset_key` or
//!   `assertion_key` produces a superseding row, never an edit (the tables are append-only).
//!
//! ## Inclusion is asked of the same ledger the export asks
//!
//! A research object may only cite a report the review INCLUDED. `flow.rs` owns that derivation
//! (`derive`/`collect`) but does not expose it; [`included_record_ids`] below mirrors the
//! terminal-bucket rule exactly — it is the same rule, not a second opinion, and it reads the
//! ledger under the same lead elevation `flow::collect` uses. When `flow.rs` grows a public seam
//! this function should be replaced by a call to it rather than kept in step by hand.

use std::collections::{HashMap, HashSet};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::actor::Actor;
use crate::db;
use crate::evidence_review::reviews::{load_review, require_review_open};
use crate::evidence_review::{hash_value, new_id, pool};
use crate::problem::Problem;
use crate::state::Shared;

/// The schema every version in this family declares itself against. The reference used a
/// `urn:datacompany:` name; this estate's schema refs spell the same family `urn:ahtapol:`.
pub const EVIDENCE_SCHEMA_REF: &str = "urn:ahtapol:evidence-review-accepted-evidence:unreleased";

/// Ceiling on observations in one `POST …/extractions` — the reference's `500`, an order of
/// magnitude above any real extraction form and far below `int4`.
pub const MAX_OBSERVATIONS_PER_EXTRACTION: usize = 500;

/// Ceiling on cited observations in one assertion. Same reasoning.
pub const MAX_SUPPORT_REFS: usize = 500;

pub(crate) fn unprocessable(detail: impl Into<String>) -> Problem {
    Problem::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "unprocessable_state",
        "Unprocessable State",
        detail,
    )
}

fn iso(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Micros, true)
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

fn bounded(body: &Value, name: &str, max: usize) -> Result<Option<String>, Problem> {
    match field(body, name) {
        None | Some("") => Ok(None),
        Some(value) if value.chars().count() > max => Err(Problem::validation(format!(
            "{name} must be at most {max} characters"
        ))),
        Some(value) => Ok(Some(value.to_owned())),
    }
}

fn uuid_field(body: &Value, name: &str) -> Result<Uuid, Problem> {
    field(body, name)
        .ok_or_else(|| Problem::validation(format!("{name} is required")))?
        .parse::<Uuid>()
        .map_err(|_| Problem::validation(format!("{name} must be a UUID")))
}

fn optional_uuid(body: &Value, name: &str) -> Result<Option<Uuid>, Problem> {
    match body.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(_)) => uuid_field(body, name).map(Some),
        Some(_) => Err(Problem::validation(format!("{name} must be a UUID"))),
    }
}

fn object_content(body: &Value, name: &str) -> Result<Value, Problem> {
    match body.get(name) {
        Some(value) if value.is_object() => Ok(value.clone()),
        Some(_) => Err(Problem::validation(format!("{name} must be an object"))),
        None => Ok(json!({})),
    }
}

/// The protocol version the review was most recently activated against, and it must be LOCKED.
/// The reference's `_locked_protocol_id`: the database trigger is the guarantee, this is the
/// readable refusal.
pub(crate) async fn locked_protocol_id(
    tx: &mut Transaction<'_, Postgres>,
    review_id: Uuid,
) -> Result<Uuid, Problem> {
    let row = sqlx::query(
        "SELECT p.protocol_version_id, p.locked_at
           FROM review_protocol_versions p
          WHERE p.protocol_version_id = (
              SELECT protocol_version_id FROM review_state_history h
               WHERE h.review_id = $1 AND h.to_state = 'active' AND h.protocol_version_id IS NOT NULL
               ORDER BY h.revision_number DESC LIMIT 1)",
    )
    .bind(review_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the review's locked protocol", error))?;
    match row {
        Some(row) if row.get::<Option<DateTime<Utc>>, _>("locked_at").is_some() => {
            Ok(row.get("protocol_version_id"))
        }
        _ => Err(unprocessable(
            "this review has no locked protocol version; evidence cannot be judged against a \
             protocol that can still change",
        )),
    }
}

// ── the inclusion ledger ─────────────────────────────────────────────────────────────────────

/// `(stage_order, stage_key, round_number)` — one piece of screening work. Same ordering as
/// `flow::Position`, which is what makes the settling pick identical.
type Position = (i32, String, i32);

type Vote = (String, Option<String>);

/// The records this review has actually INCLUDED, derived from the immutable decision history.
///
/// Mirrors `flow::derive`'s terminal bucket for the record ids the caller will cite. It reads
/// every reviewer's work — `flow::collect` does the same elevation for the same reason: an
/// aggregate that must see all votes cannot be computed through the blinded policy.
pub(crate) async fn included_record_ids(
    tx: &mut Transaction<'_, Postgres>,
    review_id: Uuid,
) -> Result<HashSet<Uuid>, Problem> {
    sqlx::query("SELECT set_config('app.actor_role', 'lead', true)")
        .execute(&mut **tx)
        .await
        .map_err(|error| Problem::from_db("elevating the ledger read", error))?;

    let record_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT review_record_id FROM review_record_versions WHERE review_id = $1",
    )
    .bind(review_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading record versions", error))?;

    // Records removed by a duplicate merge are never included; they are not the surviving report.
    let mut removed_by_merge: HashSet<Uuid> = HashSet::new();
    let merges = sqlx::query(
        "SELECT c.cluster_id, r.survivor_record_id
           FROM duplicate_clusters c
           JOIN duplicate_resolutions r ON r.cluster_id = c.cluster_id
          WHERE c.review_id = $1 AND r.resolution_type = 'merge'
            AND NOT EXISTS (SELECT 1 FROM duplicate_resolutions x
                             WHERE x.supersedes_resolution_id = r.resolution_id)",
    )
    .bind(review_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading duplicate merges", error))?;
    for row in merges {
        let cluster_id: Uuid = row.get("cluster_id");
        let survivor: Option<Uuid> = row.get("survivor_record_id");
        let members: Vec<Uuid> = sqlx::query_scalar(
            "SELECT review_record_id FROM duplicate_cluster_members WHERE cluster_id = $1",
        )
        .bind(cluster_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(|error| Problem::from_db("reading duplicate cluster members", error))?;
        for member in members {
            if Some(member) != survivor {
                removed_by_merge.insert(member);
            }
        }
    }

    let stages: HashMap<String, i32> =
        sqlx::query("SELECT stage_key, reviewers_required FROM review_stages WHERE review_id = $1")
            .bind(review_id)
            .fetch_all(&mut **tx)
            .await
            .map_err(|error| Problem::from_db("reading the review's stages", error))?
            .into_iter()
            .map(|row| (row.get("stage_key"), row.get("reviewers_required")))
            .collect();
    let protocol_required: i32 = sqlx::query_scalar(
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

    let mut decisions: HashMap<(Position, Uuid), Vec<Vote>> = HashMap::new();
    let mut worked_at: HashMap<(Position, Uuid), DateTime<Utc>> = HashMap::new();
    let rows = sqlx::query(
        "SELECT d.review_record_id, d.disposition, d.exclusion_reason_code, d.decided_at,
                s.stage_order, s.stage_key, d.round_number
           FROM screening_decisions d
           JOIN review_stages s ON s.stage_id = d.stage_id
          WHERE d.review_id = $1
            AND NOT EXISTS (SELECT 1 FROM screening_decisions x
                             WHERE x.supersedes_decision_id = d.decision_id)",
    )
    .bind(review_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading current decisions", error))?;
    for row in rows {
        let record_id: Uuid = row.get("review_record_id");
        let position: Position = (
            row.get("stage_order"),
            row.get("stage_key"),
            row.get("round_number"),
        );
        let decided_at: DateTime<Utc> = row.get("decided_at");
        decisions
            .entry((position.clone(), record_id))
            .or_default()
            .push((row.get("disposition"), row.get("exclusion_reason_code")));
        let key = (position, record_id);
        if worked_at.get(&key).is_none_or(|seen| decided_at > *seen) {
            worked_at.insert(key, decided_at);
        }
    }

    let mut adjudications: HashMap<(Position, Uuid), Vote> = HashMap::new();
    let rows = sqlx::query(
        "SELECT c.review_record_id, a.disposition, a.exclusion_reason_code,
                s.stage_order, s.stage_key, c.round_number
           FROM screening_conflicts c
           JOIN screening_adjudications a ON a.conflict_id = c.conflict_id
           JOIN review_stages s ON s.stage_id = c.stage_id
          WHERE c.review_id = $1
            AND NOT EXISTS (SELECT 1 FROM screening_adjudications x
                             WHERE x.supersedes_adjudication_id = a.adjudication_id)",
    )
    .bind(review_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading current adjudications", error))?;
    for row in rows {
        let position: Position = (
            row.get("stage_order"),
            row.get("stage_key"),
            row.get("round_number"),
        );
        adjudications.insert(
            (position, row.get("review_record_id")),
            (row.get("disposition"), row.get("exclusion_reason_code")),
        );
    }

    let reviewers_required_at = |position: &Position| -> i32 {
        stages
            .get(&position.1)
            .copied()
            .filter(|required| *required > 0)
            .unwrap_or(protocol_required)
    };

    let mut positions: HashMap<Uuid, HashSet<Position>> = HashMap::new();
    for (position, record_id) in decisions.keys().chain(adjudications.keys()) {
        positions
            .entry(*record_id)
            .or_default()
            .insert(position.clone());
    }

    let mut included = HashSet::new();
    for record_id in record_ids {
        if removed_by_merge.contains(&record_id) {
            continue;
        }
        let Some(reached) = positions.get(&record_id) else {
            continue;
        };
        // The newest work on the record; partial timestamp coverage has no honest answer.
        let timed: Vec<&Position> = reached
            .iter()
            .filter(|position| worked_at.contains_key(&((*position).clone(), record_id)))
            .collect();
        let settled: Option<Position> = if !reached.is_empty() && timed.len() == reached.len() {
            timed
                .into_iter()
                .max_by_key(|position| {
                    (
                        worked_at.get(&((*position).clone(), record_id)).copied(),
                        (*position).clone(),
                    )
                })
                .cloned()
        } else if timed.is_empty() {
            reached.iter().max().cloned()
        } else {
            None
        };
        let Some(position) = settled else {
            continue;
        };
        if let Some((disposition, _)) = adjudications.get(&(position.clone(), record_id)) {
            if disposition == "include" {
                included.insert(record_id);
            }
            continue;
        }
        let votes = decisions
            .get(&(position.clone(), record_id))
            .cloned()
            .unwrap_or_default();
        if (votes.len() as i32) < reviewers_required_at(&position) {
            continue;
        }
        let unique: HashSet<Vote> = votes.into_iter().collect();
        if unique.len() == 1 {
            let (disposition, _) = unique.into_iter().next().expect("one");
            if disposition == "include" {
                included.insert(record_id);
            }
        }
    }
    Ok(included)
}

// ── research objects (`ER-006.1`) ────────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
struct ObjectRow {
    research_object_id: Uuid,
    review_id: Uuid,
    object_key: String,
}

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
struct ObjectVersionRow {
    research_object_version_id: Uuid,
    research_object_id: Uuid,
    review_id: Uuid,
    version_number: i32,
    supersedes_version_id: Option<Uuid>,
    protocol_version_id: Uuid,
    schema_ref: String,
    content_hash: String,
    content: Value,
    revision_kind: String,
    rationale: String,
    created_by_subject_id: String,
    created_by_display_name: Option<String>,
    created_at: DateTime<Utc>,
}

struct RecordLink {
    review_record_id: Uuid,
    review_record_version_id: Uuid,
    corpus_version_id: Uuid,
    link_role: String,
    rationale: Option<String>,
}

const OBJECT_VERSION_COLUMNS: &str = "research_object_version_id, research_object_id, review_id, version_number, \
     supersedes_version_id, protocol_version_id, schema_ref, content_hash, content, \
     revision_kind, rationale, created_by_subject_id, created_by_display_name, created_at";

async fn object_by_id(
    tx: &mut Transaction<'_, Postgres>,
    review_id: Uuid,
    research_object_id: Uuid,
) -> Result<ObjectRow, Problem> {
    sqlx::query_as::<_, ObjectRow>(
        "SELECT research_object_id, review_id, object_key FROM research_objects
          WHERE research_object_id = $1 AND review_id = $2",
    )
    .bind(research_object_id)
    .bind(review_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("loading the research object", error))?
    .ok_or_else(|| Problem::not_found("no such research object in this review"))
}

async fn version_head(
    tx: &mut Transaction<'_, Postgres>,
    research_object_id: Uuid,
) -> Result<ObjectVersionRow, Problem> {
    sqlx::query_as::<_, ObjectVersionRow>(sqlx::AssertSqlSafe(format!(
        "SELECT {OBJECT_VERSION_COLUMNS} FROM research_object_versions
          WHERE research_object_id = $1 ORDER BY version_number DESC LIMIT 1"
    )))
    .bind(research_object_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("loading the current object version", error))?
    .ok_or_else(|| Problem::not_found("research object has no versions"))
}

async fn version_by_id(
    tx: &mut Transaction<'_, Postgres>,
    research_object_version_id: Uuid,
) -> Result<ObjectVersionRow, Problem> {
    sqlx::query_as::<_, ObjectVersionRow>(sqlx::AssertSqlSafe(format!(
        "SELECT {OBJECT_VERSION_COLUMNS} FROM research_object_versions
          WHERE research_object_version_id = $1"
    )))
    .bind(research_object_version_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("loading the object version", error))?
    .ok_or_else(|| Problem::not_found("no such research object version"))
}

#[allow(clippy::too_many_arguments)]
async fn write_object_version(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    object: &ObjectRow,
    content: &Value,
    rationale: &str,
    revision_kind: &str,
    version_number: i32,
    supersedes_version_id: Option<Uuid>,
    protocol_version_id: Uuid,
) -> Result<ObjectVersionRow, Problem> {
    let subject = actor.require_human()?;
    // The hash covers the content AND the identity it belongs to, and nothing recorded after the
    // fact — no later acceptance may be hashed back into this version.
    let content_hash = hash_value(&json!({
        "research_object_id": object.research_object_id.to_string(),
        "version_number": version_number,
        "content": content,
    }));
    let version_id = new_id();
    sqlx::query(
        "INSERT INTO research_object_versions
             (research_object_version_id, research_object_id, review_id, tenant_id, version_number,
              supersedes_version_id, protocol_version_id, schema_ref, content_hash, content,
              revision_kind, rationale, created_by_subject_id, created_by_display_name)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
    )
    .bind(version_id)
    .bind(object.research_object_id)
    .bind(object.review_id)
    .bind(actor.tenant)
    .bind(version_number)
    .bind(supersedes_version_id)
    .bind(protocol_version_id)
    .bind(EVIDENCE_SCHEMA_REF)
    .bind(&content_hash)
    .bind(content)
    .bind(revision_kind)
    .bind(rationale)
    .bind(subject)
    .bind(actor.display_name.as_deref())
    .execute(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("writing the research object version", error))?;
    version_by_id(tx, version_id).await
}

async fn object_summary(
    tx: &mut Transaction<'_, Postgres>,
    object: &ObjectRow,
    version: &ObjectVersionRow,
) -> Result<Value, Problem> {
    let records = sqlx::query(
        "SELECT review_record_id, review_record_version_id, corpus_version_id, link_role
           FROM research_object_records WHERE research_object_id = $1
          ORDER BY linked_at, research_object_record_id",
    )
    .bind(object.research_object_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the object's record links", error))?;
    let items: Vec<Value> = records
        .iter()
        .map(|row| {
            json!({
                "review_record_id": row.get::<Uuid, _>("review_record_id").to_string(),
                "review_record_version_id": row.get::<Uuid, _>("review_record_version_id").to_string(),
                "corpus_version_id": row.get::<Uuid, _>("corpus_version_id").to_string(),
                "link_role": row.get::<String, _>("link_role"),
            })
        })
        .collect();
    Ok(json!({
        "research_object_id": object.research_object_id.to_string(),
        "review_id": object.review_id.to_string(),
        "object_key": &object.object_key,
        "current_version": {
            "research_object_version_id": version.research_object_version_id.to_string(),
            "version_number": version.version_number,
            "revision_kind": &version.revision_kind,
            "content_hash": &version.content_hash,
            "content": &version.content,
            "rationale": &version.rationale,
            "protocol_version_id": version.protocol_version_id.to_string(),
            "created_by": version
                .created_by_display_name
                .clone()
                .unwrap_or_else(|| version.created_by_subject_id.clone()),
        },
        "records": items,
    }))
}

fn parse_record_links(body: &Value, object_rationale: &str) -> Result<Vec<RecordLink>, Problem> {
    let raw = body
        .get("records")
        .and_then(Value::as_array)
        .ok_or_else(|| Problem::validation("records must be a list of record links"))?;
    if raw.is_empty() || raw.len() > 100 {
        return Err(Problem::validation(
            "records must carry between 1 and 100 links",
        ));
    }
    let mut links = Vec::with_capacity(raw.len());
    for link in raw {
        let link_role = field(link, "link_role").unwrap_or_default().to_owned();
        if !matches!(link_role.as_str(), "primary" | "companion") {
            return Err(Problem::validation(
                "link_role must be 'primary' or 'companion'",
            ));
        }
        let rationale = bounded(link, "rationale", 4000)?.filter(|value| !value.trim().is_empty());
        links.push(RecordLink {
            review_record_id: uuid_field(link, "review_record_id")?,
            review_record_version_id: uuid_field(link, "review_record_version_id")?,
            corpus_version_id: uuid_field(link, "corpus_version_id")?,
            link_role,
            rationale: rationale.or_else(|| Some(object_rationale.to_owned())),
        });
    }
    Ok(links)
}

/// One study, and the reports of it — `ER-006.1`.
#[utoipa::path(post, path = "/api/reviews/{review_id}/research-objects", operation_id = "create_research_object", responses((status = 201, description = "ok")))]
pub async fn create_research_object(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let subject = actor.require_human()?.to_owned();
    let object_key = required(&body, "object_key")?;
    if object_key.chars().count() > 200 {
        return Err(Problem::validation(
            "object_key must be at most 200 characters",
        ));
    }
    let rationale = required(&body, "rationale")?;
    if rationale.chars().count() > 4000 {
        return Err(Problem::validation(
            "rationale must be at most 4000 characters",
        ));
    }
    let content = object_content(&body, "content")?;
    let links = parse_record_links(&body, &rationale)?;
    if links
        .iter()
        .filter(|link| link.link_role == "primary")
        .count()
        != 1
    {
        return Err(unprocessable(
            "exactly one linked record must be the 'primary' report: with none the study cannot \
             be cited, with two there is no single citation",
        ));
    }

    let pool = pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the research object transaction", error))?;
    load_review(&mut tx, review_id).await?;
    require_review_open(&mut tx, review_id, "creating a research object").await?;
    let protocol_version_id = locked_protocol_id(&mut tx, review_id).await?;

    let included = included_record_ids(&mut tx, review_id).await?;
    for link in &links {
        if !included.contains(&link.review_record_id) {
            return Err(unprocessable(format!(
                "review record {} is not included by this review's flow ledger; evidence may \
                 only be extracted from reports the review included",
                link.review_record_id
            )));
        }
    }

    let object_id = new_id();
    sqlx::query(
        "INSERT INTO research_objects
             (research_object_id, review_id, tenant_id, object_key, created_by_subject_id)
         VALUES ($1,$2,$3,$4,$5)",
    )
    .bind(object_id)
    .bind(review_id)
    .bind(actor.tenant)
    .bind(&object_key)
    .bind(&subject)
    .execute(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("creating the research object", error))?;
    let object = ObjectRow {
        research_object_id: object_id,
        review_id,
        object_key,
    };

    let version = write_object_version(
        &mut tx,
        &actor,
        &object,
        &content,
        &rationale,
        "initial",
        1,
        None,
        protocol_version_id,
    )
    .await?;

    for link in &links {
        sqlx::query(
            "INSERT INTO research_object_records
                 (research_object_record_id, research_object_id, review_id, tenant_id,
                  review_record_id, review_record_version_id, corpus_version_id, link_role,
                  rationale, linked_by_subject_id, linked_by_display_name)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        )
        .bind(new_id())
        .bind(object_id)
        .bind(review_id)
        .bind(actor.tenant)
        .bind(link.review_record_id)
        .bind(link.review_record_version_id)
        .bind(link.corpus_version_id)
        .bind(&link.link_role)
        .bind(link.rationale.as_deref().unwrap_or(&rationale))
        .bind(&subject)
        .bind(actor.display_name.as_deref())
        .execute(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("linking the study's reports", error))?;
    }

    let value = object_summary(&mut tx, &object, &version).await?;
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the research object", error))?;
    Ok((StatusCode::CREATED, Json(value)))
}

#[utoipa::path(get, path = "/api/reviews/{review_id}/research-objects", operation_id = "list_research_objects", responses((status = 200, description = "ok")))]
pub async fn list_research_objects(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
) -> Result<Json<Value>, Problem> {
    let pool = pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the research object transaction", error))?;
    load_review(&mut tx, review_id).await?;
    let objects = sqlx::query_as::<_, ObjectRow>(
        "SELECT research_object_id, review_id, object_key FROM research_objects
          WHERE review_id = $1 ORDER BY object_key",
    )
    .bind(review_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("listing research objects", error))?;
    let mut items = Vec::with_capacity(objects.len());
    for object in &objects {
        let version = version_head(&mut tx, object.research_object_id).await?;
        items.push(object_summary(&mut tx, object, &version).await?);
    }
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the research object read", error))?;
    Ok(Json(json!({"items": items})))
}

/// A new version superseding the current one — never an edit.
#[utoipa::path(post, path = "/api/reviews/{review_id}/research-objects/{research_object_id}:revise", operation_id = "revise_research_object", responses((status = 201, description = "ok")))]
pub async fn revise_research_object(
    State(state): State<Shared>,
    actor: Actor,
    Path((review_id, research_object_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    actor.require_human()?;
    let content = object_content(&body, "content")?;
    let rationale = required(&body, "rationale")?;
    if rationale.chars().count() > 4000 {
        return Err(Problem::validation(
            "rationale must be at most 4000 characters",
        ));
    }
    let revision_kind = required(&body, "revision_kind")?;
    if revision_kind == "initial" {
        return Err(unprocessable("only the first version may be 'initial'"));
    }
    if !matches!(revision_kind.as_str(), "correction" | "material_change") {
        return Err(Problem::validation(
            "revision_kind must be 'correction' or 'material_change'",
        ));
    }

    let pool = pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the revision transaction", error))?;
    load_review(&mut tx, review_id).await?;
    require_review_open(&mut tx, review_id, "revising a research object").await?;
    let object = object_by_id(&mut tx, review_id, research_object_id).await?;
    let protocol_version_id = locked_protocol_id(&mut tx, review_id).await?;
    let head = version_head(&mut tx, research_object_id).await?;
    let version = write_object_version(
        &mut tx,
        &actor,
        &object,
        &content,
        &rationale,
        &revision_kind,
        head.version_number + 1,
        Some(head.research_object_version_id),
        protocol_version_id,
    )
    .await?;
    let value = object_summary(&mut tx, &object, &version).await?;
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the revision", error))?;
    Ok((StatusCode::CREATED, Json(value)))
}

// ── manual extraction and observations (`ER-006.2` / `ER-006.4`) ─────────────────────────────

struct Observation {
    variable_key: String,
    value: Value,
    unit: Option<String>,
    expert_rationale: String,
    expert_subject_id: Option<String>,
    expert_role: Option<String>,
}

fn parse_observations(body: &Value) -> Result<Vec<Observation>, Problem> {
    let raw = body
        .get("observations")
        .and_then(Value::as_array)
        .ok_or_else(|| Problem::validation("observations must be a list"))?;
    if raw.is_empty() {
        return Err(Problem::validation(
            "an extraction with no observations records nothing",
        ));
    }
    if raw.len() > MAX_OBSERVATIONS_PER_EXTRACTION {
        return Err(Problem::validation(format!(
            "an extraction may carry at most {MAX_OBSERVATIONS_PER_EXTRACTION} observations"
        )));
    }
    let mut observations = Vec::with_capacity(raw.len());
    for observed in raw {
        let variable_key = required(observed, "variable_key")?;
        if variable_key.chars().count() > 200 {
            return Err(Problem::validation(
                "variable_key must be at most 200 characters",
            ));
        }
        let value = observed
            .get("value")
            .cloned()
            .ok_or_else(|| Problem::validation(format!("value is required for {variable_key}")))?;
        let unit = bounded(observed, "unit", 100)?;
        let expert_rationale = required(observed, "expert_rationale")?;
        if expert_rationale.chars().count() > 4000 {
            return Err(Problem::validation(
                "expert_rationale must be at most 4000 characters",
            ));
        }
        let expert_subject_id = bounded(observed, "expert_subject_id", 200)?;
        let expert_role = bounded(observed, "expert_role", 100)?;
        let basis_kind = field(observed, "basis_kind").unwrap_or("expert_assertion");
        match basis_kind {
            "expert_assertion" => {}
            "evidence_located" => {
                return Err(unprocessable(
                    "evidence_location_unverifiable: an exact EvidenceLocation is owned by \
                     Ingestion/Documents and this service cannot yet verify a reference into it, \
                     so a located claim would be unverifiable evidence. Record the value as an \
                     expert assertion, with who is asserting it and why",
                ));
            }
            other => {
                return Err(unprocessable(format!(
                    "basis_kind must be one of ('evidence_located', 'expert_assertion'), got {other:?}"
                )));
            }
        }
        observations.push(Observation {
            variable_key,
            value,
            unit,
            expert_rationale,
            expert_subject_id: expert_subject_id.filter(|value| !value.trim().is_empty()),
            expert_role: expert_role.filter(|value| !value.trim().is_empty()),
        });
    }
    Ok(observations)
}

/// One dataset version and the observations in it — `ER-006.2`, `ER-006.4`.
#[utoipa::path(post, path = "/api/reviews/{review_id}/research-objects/{research_object_id}/extractions", operation_id = "record_extraction", responses((status = 201, description = "ok")))]
pub async fn record_extraction(
    State(state): State<Shared>,
    actor: Actor,
    Path((review_id, research_object_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let subject = actor.require_human()?.to_owned();
    let dataset_key = required(&body, "dataset_key")?;
    if dataset_key.chars().count() > 200 {
        return Err(Problem::validation(
            "dataset_key must be at most 200 characters",
        ));
    }
    let rationale = required(&body, "rationale")?;
    if rationale.chars().count() > 4000 {
        return Err(Problem::validation(
            "rationale must be at most 4000 characters",
        ));
    }
    let observations = parse_observations(&body)?;

    let pool = pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the extraction transaction", error))?;
    load_review(&mut tx, review_id).await?;
    require_review_open(&mut tx, review_id, "recording an extraction").await?;
    let protocol_version_id = locked_protocol_id(&mut tx, review_id).await?;
    object_by_id(&mut tx, review_id, research_object_id).await?;

    let dataset: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT extraction_dataset_id, dataset_key FROM extraction_datasets
          WHERE research_object_id = $1 AND dataset_key = $2",
    )
    .bind(research_object_id)
    .bind(&dataset_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading the extraction dataset", error))?;
    let dataset_id = match dataset {
        Some((id, _)) => id,
        None => {
            let id = new_id();
            sqlx::query(
                "INSERT INTO extraction_datasets
                     (extraction_dataset_id, review_id, tenant_id, created_by_subject_id,
                      research_object_id, dataset_key)
                 VALUES ($1,$2,$3,$4,$5,$6)",
            )
            .bind(id)
            .bind(review_id)
            .bind(actor.tenant)
            .bind(&subject)
            .bind(research_object_id)
            .bind(&dataset_key)
            .execute(&mut *tx)
            .await
            .map_err(|error| Problem::from_db("creating the extraction dataset", error))?;
            id
        }
    };

    let head: Option<(Uuid, i32)> = sqlx::query_as(
        "SELECT extraction_dataset_version_id, version_number FROM extraction_dataset_versions
          WHERE extraction_dataset_id = $1 ORDER BY version_number DESC LIMIT 1",
    )
    .bind(dataset_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading the dataset version head", error))?;
    let (supersedes, version_number) = match head {
        Some((id, number)) => (Some(id), number + 1),
        None => (None, 1),
    };
    let dataset_version_id = new_id();
    let dataset_hash = hash_value(&json!({
        "extraction_dataset_id": dataset_id.to_string(),
        "version_number": version_number,
        "observations": observations
            .iter()
            .map(|observed| json!({
                "variable_key": &observed.variable_key,
                "value": &observed.value,
                "unit": &observed.unit,
            }))
            .collect::<Vec<Value>>(),
    }));
    sqlx::query(
        "INSERT INTO extraction_dataset_versions
             (extraction_dataset_version_id, extraction_dataset_id, review_id, tenant_id,
              version_number, supersedes_version_id, protocol_version_id, schema_ref,
              content_hash, extraction_method, rationale, created_by_subject_id,
              created_by_display_name)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'manual',$10,$11,$12)",
    )
    .bind(dataset_version_id)
    .bind(dataset_id)
    .bind(review_id)
    .bind(actor.tenant)
    .bind(version_number)
    .bind(supersedes)
    .bind(protocol_version_id)
    .bind(EVIDENCE_SCHEMA_REF)
    .bind(&dataset_hash)
    .bind(&rationale)
    .bind(&subject)
    .bind(actor.display_name.as_deref())
    .execute(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("writing the dataset version", error))?;

    let mut written = Vec::with_capacity(observations.len());
    for observed in &observations {
        let existing: Option<Uuid> = sqlx::query_scalar(
            "SELECT observation_id FROM observations
              WHERE extraction_dataset_id = $1 AND variable_key = $2",
        )
        .bind(dataset_id)
        .bind(&observed.variable_key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("reading the observation head", error))?;
        let (observation_id, observation_supersedes, observation_number) = match existing {
            Some(id) => {
                let head: Option<(Uuid, i32)> = sqlx::query_as(
                    "SELECT observation_version_id, version_number FROM observation_versions
                      WHERE observation_id = $1 ORDER BY version_number DESC LIMIT 1",
                )
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|error| Problem::from_db("reading the observation version head", error))?;
                match head {
                    Some((version, number)) => (id, Some(version), number + 1),
                    None => (id, None, 1),
                }
            }
            None => {
                let id = new_id();
                sqlx::query(
                    "INSERT INTO observations
                         (observation_id, review_id, tenant_id, created_by_subject_id,
                          extraction_dataset_id, variable_key)
                     VALUES ($1,$2,$3,$4,$5,$6)",
                )
                .bind(id)
                .bind(review_id)
                .bind(actor.tenant)
                .bind(&subject)
                .bind(dataset_id)
                .bind(&observed.variable_key)
                .execute(&mut *tx)
                .await
                .map_err(|error| Problem::from_db("creating the observation", error))?;
                (id, None, 1)
            }
        };
        let observation_version_id = new_id();
        let content_hash = hash_value(&json!({
            "observation_id": observation_id.to_string(),
            "version_number": observation_number,
            "variable_key": &observed.variable_key,
            "value": &observed.value,
            "unit": &observed.unit,
        }));
        let expert_subject_id = observed
            .expert_subject_id
            .clone()
            .unwrap_or_else(|| subject.clone());
        let expert_role = observed
            .expert_role
            .clone()
            .unwrap_or_else(|| actor.role.as_str().to_owned());
        sqlx::query(
            "INSERT INTO observation_versions
                 (observation_version_id, observation_id, review_id, tenant_id, version_number,
                  supersedes_version_id, protocol_version_id, extraction_dataset_version_id,
                  schema_ref, content_hash, value, unit, basis_kind, expert_subject_id,
                  expert_role, expert_rationale, expert_decision_ref, created_by_subject_id,
                  created_by_display_name)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,'expert_assertion',$13,$14,$15,$16,$17,$18)",
        )
        .bind(observation_version_id)
        .bind(observation_id)
        .bind(review_id)
        .bind(actor.tenant)
        .bind(observation_number)
        .bind(observation_supersedes)
        .bind(protocol_version_id)
        .bind(dataset_version_id)
        .bind(EVIDENCE_SCHEMA_REF)
        .bind(&content_hash)
        .bind(&observed.value)
        .bind(observed.unit.as_deref())
        .bind(&expert_subject_id)
        .bind(&expert_role)
        .bind(&observed.expert_rationale)
        .bind(dataset_version_id)
        .bind(&subject)
        .bind(actor.display_name.as_deref())
        .execute(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("writing the observation version", error))?;
        written.push(json!({
            "observation_id": observation_id.to_string(),
            "observation_version_id": observation_version_id.to_string(),
            "variable_key": &observed.variable_key,
            "version_number": observation_number,
            "value": &observed.value,
            "unit": &observed.unit,
            "content_hash": content_hash,
            "basis_kind": "expert_assertion",
        }));
    }

    let value = json!({
        "extraction_dataset_id": dataset_id.to_string(),
        "dataset_key": dataset_key,
        "research_object_id": research_object_id.to_string(),
        "current_version": {
            "extraction_dataset_version_id": dataset_version_id.to_string(),
            "version_number": version_number,
            "content_hash": dataset_hash,
            "extraction_method": "manual",
        },
        "observations": written,
    });
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the extraction", error))?;
    Ok((StatusCode::CREATED, Json(value)))
}

// ── research assertions (`ER-006.5`) ─────────────────────────────────────────────────────────

struct SupportRef {
    observation_version_id: Uuid,
    stance: String,
}

fn parse_support(body: &Value) -> Result<Vec<SupportRef>, Problem> {
    let raw = body
        .get("support")
        .and_then(Value::as_array)
        .ok_or_else(|| Problem::validation("support must be a list of observation refs"))?;
    if raw.is_empty() {
        return Err(unprocessable(
            "an assertion must cite at least one observation; a claim supported by nothing \
             recorded is not a reviewed claim",
        ));
    }
    if raw.len() > MAX_SUPPORT_REFS {
        return Err(Problem::validation(format!(
            "an assertion may cite at most {MAX_SUPPORT_REFS} observations"
        )));
    }
    let mut refs = Vec::with_capacity(raw.len());
    let mut seen = HashSet::new();
    for cited in raw {
        let observation_version_id = uuid_field(cited, "observation_version_id")?;
        let stance = field(cited, "stance").unwrap_or_default().to_owned();
        if !matches!(stance.as_str(), "supports" | "contradicts") {
            return Err(Problem::validation(
                "stance must be 'supports' or 'contradicts'",
            ));
        }
        if !seen.insert(observation_version_id) {
            return Err(Problem::validation(
                "an observation may be cited once per assertion",
            ));
        }
        refs.push(SupportRef {
            observation_version_id,
            stance,
        });
    }
    Ok(refs)
}

fn assertion_key_is_valid(key: &str) -> bool {
    key.len() <= 128
        && key.split('.').all(|part| {
            !part.is_empty()
                && part.starts_with(|c: char| c.is_ascii_lowercase())
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
}

/// A reviewed claim, citing the observations for and against it — `ER-006.5`.
#[utoipa::path(post, path = "/api/reviews/{review_id}/assertions", operation_id = "record_assertion", responses((status = 201, description = "ok")))]
pub async fn record_assertion(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let subject = actor.require_human()?.to_owned();
    let assertion_key = required(&body, "assertion_key")?;
    if !assertion_key_is_valid(&assertion_key) {
        return Err(Problem::validation(
            "assertion_key must match ^[a-z][a-z0-9_]*(\\.[a-z][a-z0-9_]*)*$ and be at most 128 characters",
        ));
    }
    let statement = required(&body, "statement")?;
    if statement.chars().count() > 8000 {
        return Err(Problem::validation(
            "statement must be at most 8000 characters",
        ));
    }
    let content = object_content(&body, "content")?;
    let support = parse_support(&body)?;

    let pool = pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the assertion transaction", error))?;
    load_review(&mut tx, review_id).await?;
    require_review_open(&mut tx, review_id, "recording an assertion").await?;
    let protocol_version_id = locked_protocol_id(&mut tx, review_id).await?;

    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT research_assertion_id FROM research_assertions
          WHERE review_id = $1 AND assertion_key = $2",
    )
    .bind(review_id)
    .bind(&assertion_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading the assertion head", error))?;
    let (assertion_id, supersedes, version_number) = match existing {
        Some(id) => {
            let head: Option<(Uuid, i32)> = sqlx::query_as(
                "SELECT research_assertion_version_id, version_number
                   FROM research_assertion_versions
                  WHERE research_assertion_id = $1 ORDER BY version_number DESC LIMIT 1",
            )
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|error| Problem::from_db("reading the assertion version head", error))?;
            match head {
                Some((version, number)) => (id, Some(version), number + 1),
                None => (id, None, 1),
            }
        }
        None => {
            let id = new_id();
            sqlx::query(
                "INSERT INTO research_assertions
                     (research_assertion_id, review_id, tenant_id, created_by_subject_id,
                      assertion_key)
                 VALUES ($1,$2,$3,$4,$5)",
            )
            .bind(id)
            .bind(review_id)
            .bind(actor.tenant)
            .bind(&subject)
            .bind(&assertion_key)
            .execute(&mut *tx)
            .await
            .map_err(|error| Problem::from_db("creating the assertion", error))?;
            (id, None, 1)
        }
    };

    // The SUPPORT is part of what the claim is, so it is hashed; the observation versions it
    // cites are not re-hashed into it, only their ids.
    let mut cited: Vec<(String, String)> = support
        .iter()
        .map(|reference| {
            (
                reference.observation_version_id.to_string(),
                reference.stance.clone(),
            )
        })
        .collect();
    cited.sort();
    let content_hash = hash_value(&json!({
        "research_assertion_id": assertion_id.to_string(),
        "version_number": version_number,
        "statement": &statement,
        "content": &content,
        "support": cited
            .iter()
            .map(|(id, stance)| json!([id, stance]))
            .collect::<Vec<Value>>(),
    }));
    let version_id = new_id();
    sqlx::query(
        "INSERT INTO research_assertion_versions
             (research_assertion_version_id, research_assertion_id, review_id, tenant_id,
              version_number, supersedes_version_id, protocol_version_id, schema_ref,
              content_hash, statement, content, created_by_subject_id, created_by_display_name)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
    )
    .bind(version_id)
    .bind(assertion_id)
    .bind(review_id)
    .bind(actor.tenant)
    .bind(version_number)
    .bind(supersedes)
    .bind(protocol_version_id)
    .bind(EVIDENCE_SCHEMA_REF)
    .bind(&content_hash)
    .bind(&statement)
    .bind(&content)
    .bind(&subject)
    .bind(actor.display_name.as_deref())
    .execute(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("writing the assertion version", error))?;

    for reference in &support {
        let owner: Option<Uuid> = sqlx::query_scalar(
            "SELECT review_id FROM observation_versions WHERE observation_version_id = $1",
        )
        .bind(reference.observation_version_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("reading the cited observation", error))?;
        if owner != Some(review_id) {
            return Err(Problem::not_found(format!(
                "no such observation version in this review: {}",
                reference.observation_version_id
            )));
        }
        sqlx::query(
            "INSERT INTO research_assertion_support
                 (support_id, research_assertion_version_id, observation_version_id, review_id,
                  tenant_id, stance)
             VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .bind(new_id())
        .bind(version_id)
        .bind(reference.observation_version_id)
        .bind(review_id)
        .bind(actor.tenant)
        .bind(&reference.stance)
        .execute(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("writing the assertion support", error))?;
    }

    let value = json!({
        "research_assertion_id": assertion_id.to_string(),
        "assertion_key": assertion_key,
        "current_version": {
            "research_assertion_version_id": version_id.to_string(),
            "version_number": version_number,
            "statement": statement,
            "content_hash": content_hash,
        },
        "support": support
            .iter()
            .map(|reference| json!({
                "observation_version_id": reference.observation_version_id.to_string(),
                "stance": reference.stance,
            }))
            .collect::<Vec<Value>>(),
    });
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the assertion", error))?;
    Ok((StatusCode::CREATED, Json(value)))
}

// ── acceptance, the authority act (`ER-006`) ──────────────────────────────────────────────────

const ACCEPTANCE_SUBJECTS: [&str; 3] = [
    "research_object_version",
    "extraction_dataset_version",
    "observation_version",
];

fn subject_column(subject_kind: &str) -> Option<&'static str> {
    match subject_kind {
        "research_object_version" => Some("research_object_version_id"),
        "extraction_dataset_version" => Some("extraction_dataset_version_id"),
        "observation_version" => Some("observation_version_id"),
        _ => None,
    }
}

/// The version an acceptance is about, scoped to this review, and its content hash — the witness
/// the database trigger `evidence_acceptances_witnesses_its_subject` refuses to see drift.
async fn subject_content_hash(
    tx: &mut Transaction<'_, Postgres>,
    review_id: Uuid,
    subject_kind: &str,
    subject_version_id: Uuid,
) -> Result<String, Problem> {
    let query = match subject_kind {
        "research_object_version" => {
            "SELECT content_hash FROM research_object_versions WHERE research_object_version_id = $1 AND review_id = $2"
        }
        "extraction_dataset_version" => {
            "SELECT content_hash FROM extraction_dataset_versions WHERE extraction_dataset_version_id = $1 AND review_id = $2"
        }
        "observation_version" => {
            "SELECT content_hash FROM observation_versions WHERE observation_version_id = $1 AND review_id = $2"
        }
        _ => {
            return Err(unprocessable(
                "subject_kind must name an acceptable version kind",
            ));
        }
    };
    sqlx::query_scalar::<_, String>(query)
        .bind(subject_version_id)
        .bind(review_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|error| Problem::from_db("loading the acceptance subject", error))?
        .ok_or_else(|| Problem::not_found(format!("no such {subject_kind} in this review")))
}

/// Accept or withdraw an exact version — the one lead-only act in this module.
#[utoipa::path(post, path = "/api/reviews/{review_id}/acceptances", operation_id = "decide_acceptance", responses((status = 201, description = "ok")))]
pub async fn decide_acceptance(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let subject_kind = required(&body, "subject_kind")?;
    if !ACCEPTANCE_SUBJECTS.contains(&subject_kind.as_str()) {
        return Err(unprocessable(format!(
            "subject_kind must be one of {ACCEPTANCE_SUBJECTS:?}"
        )));
    }
    let subject_version_id = uuid_field(&body, "subject_version_id")?;
    let decision = required(&body, "decision")?;
    if !matches!(decision.as_str(), "accept" | "withdraw") {
        return Err(unprocessable(
            "decision must be one of ('accept', 'withdraw')",
        ));
    }
    let rationale = required(&body, "rationale")?;
    if rationale.chars().count() > 4000 {
        return Err(Problem::validation(
            "rationale must be at most 4000 characters",
        ));
    }
    let supersedes_acceptance_id = optional_uuid(&body, "supersedes_acceptance_id")?;
    if decision == "withdraw" && supersedes_acceptance_id.is_none() {
        return Err(unprocessable(
            "a withdrawal cites the acceptance it withdraws; supersedes_acceptance_id is required",
        ));
    }

    let pool = pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the acceptance transaction", error))?;
    // The SAME lock snapshot creation takes: a snapshot's gates read whether a version is
    // currently accepted, and this handler changes that.
    sqlx::query("SELECT review_id FROM reviews WHERE review_id = $1 FOR UPDATE")
        .bind(review_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("locking the review", error))?;
    let subject = crate::evidence_review::reviews::require_review_lead_for(
        &mut tx,
        &actor,
        review_id,
        "accept evidence",
    )
    .await?;
    require_review_open(&mut tx, review_id, "recording an acceptance").await?;
    let subject_content_hash =
        subject_content_hash(&mut tx, review_id, &subject_kind, subject_version_id).await?;
    let protocol_version_id = locked_protocol_id(&mut tx, review_id).await?;

    let column = subject_column(&subject_kind).expect("validated above");
    let acceptance_id = new_id();
    let sql = "INSERT INTO evidence_acceptances
             (acceptance_id, review_id, tenant_id, subject_kind, research_object_version_id,
              extraction_dataset_version_id, observation_version_id, subject_content_hash,
              protocol_version_id, decision, rationale, supersedes_acceptance_id,
              decided_by_subject_id, decided_by_display_name)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)";
    let (object_slot, dataset_slot, observation_slot) = match column {
        "research_object_version_id" => (Some(subject_version_id), None, None),
        "extraction_dataset_version_id" => (None, Some(subject_version_id), None),
        _ => (None, None, Some(subject_version_id)),
    };
    sqlx::query(sql)
        .bind(acceptance_id)
        .bind(review_id)
        .bind(actor.tenant)
        .bind(&subject_kind)
        .bind(object_slot)
        .bind(dataset_slot)
        .bind(observation_slot)
        .bind(&subject_content_hash)
        .bind(protocol_version_id)
        .bind(&decision)
        .bind(&rationale)
        .bind(supersedes_acceptance_id)
        .bind(&subject)
        .bind(actor.display_name.as_deref())
        .execute(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("recording the acceptance", error))?;

    let value = json!({
        "acceptance_id": acceptance_id.to_string(),
        "subject_kind": subject_kind,
        "subject_version_id": subject_version_id.to_string(),
        "subject_content_hash": subject_content_hash,
        "decision": decision,
        "rationale": rationale,
        "supersedes_acceptance_id": supersedes_acceptance_id.map(|id| id.to_string()),
        "decided_by": actor
            .display_name
            .clone()
            .unwrap_or_else(|| subject.clone()),
    });
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the acceptance", error))?;
    Ok((StatusCode::CREATED, Json(value)))
}

#[utoipa::path(get, path = "/api/reviews/{review_id}/acceptances", operation_id = "list_acceptances", responses((status = 200, description = "ok")))]
pub async fn list_acceptances(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
) -> Result<Json<Value>, Problem> {
    let pool = pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the acceptance transaction", error))?;
    load_review(&mut tx, review_id).await?;
    let rows = sqlx::query(
        "SELECT acceptance_id, subject_kind, subject_version_id, subject_content_hash, decision,
                rationale, supersedes_acceptance_id, decided_by_subject_id,
                decided_by_display_name, decided_at
           FROM evidence_acceptances WHERE review_id = $1 ORDER BY decided_at, acceptance_id",
    )
    .bind(review_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("listing acceptances", error))?;
    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            let decided_by = row
                .get::<Option<String>, _>("decided_by_display_name")
                .unwrap_or_else(|| row.get::<String, _>("decided_by_subject_id"));
            json!({
                "acceptance_id": row.get::<Uuid, _>("acceptance_id").to_string(),
                "subject_kind": row.get::<String, _>("subject_kind"),
                "subject_version_id": row.get::<Option<Uuid>, _>("subject_version_id").map(|id| id.to_string()),
                "subject_content_hash": row.get::<String, _>("subject_content_hash"),
                "decision": row.get::<String, _>("decision"),
                "rationale": row.get::<String, _>("rationale"),
                "supersedes_acceptance_id": row.get::<Option<Uuid>, _>("supersedes_acceptance_id").map(|id| id.to_string()),
                "decided_by": decided_by,
                "decided_at": row.get::<Option<DateTime<Utc>>, _>("decided_at").map(iso),
            })
        })
        .collect();
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the acceptance read", error))?;
    Ok(Json(json!({"items": items})))
}
