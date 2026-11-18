//! The derived flow ledger and the sealed inclusion-set export (`ER-005`, `OUT-001`).
//!
//! Ported from the reference's `application/services/flow.py` and
//! `domain/policies/flow_derivation.py`. The ledger is DERIVED on every read from immutable
//! history — import rows, record versions, current dispositions, current adjudications — and is
//! never stored as truth; `flow_projections` is a cache and `projection_stale` makes a disagreement
//! visible rather than silently served.
//!
//! Blinding layer L6 lives here: a caller who still owes a decision reads no aggregate of the
//! work they are blinded from, and the same gate fronts `POST /exports` because the seal carries
//! the same counts.

use std::collections::{BTreeMap, HashMap, HashSet};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use crate::actor::Actor;
use crate::db;
use crate::evidence_review::reviews::require_review_lead_for;
use crate::evidence_review::{hash_value, new_id, pool};
use crate::problem::Problem;
use crate::state::Shared;

pub const LEDGER_SCHEMA_REF: &str =
    "https://contracts.nonagentic.ai/evidence/flow-ledger.v1.schema.json";
pub const EXPORT_SCHEMA_REF: &str =
    "https://contracts.nonagentic.ai/evidence/inclusion-set-export.v2.schema.json";
pub const SEALED_WITH: &str = "canonical_sha256";

/// `(stage_order, stage_key, round_number)` — the identity of one piece of screening work.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Position {
    order: i32,
    key: String,
    round: i32,
}

#[derive(Debug, Clone)]
struct Stage {
    order: i32,
    key: String,
    name: String,
    reviewers_required: i32,
}

/// One reviewer's `(disposition, exclusion_reason_code)`.
type Vote = (String, Option<String>);
type DecisionMap = HashMap<(Position, Uuid), Vec<Vote>>;
type AdjudicationMap = HashMap<(Position, Uuid), Vote>;

#[derive(Debug, Default)]
struct Inputs {
    statuses: Vec<String>,
    record_ids: HashSet<Uuid>,
    removed_by_merge: HashMap<Uuid, Uuid>,
    decisions: DecisionMap,
    adjudications: AdjudicationMap,
    worked_at: HashMap<(Position, Uuid), DateTime<Utc>>,
    stages: Vec<Stage>,
    reviewers_required: i32,
}

#[derive(Debug, Clone)]
struct Disposition {
    record_id: Uuid,
    bucket: &'static str,
    reason: Option<String>,
    settled_at: Option<Position>,
    adjudicated: bool,
    unattributable: bool,
}

fn bucket(disposition: &str) -> &'static str {
    match disposition {
        "include" => "included",
        "exclude" => "excluded",
        _ => "uncertain",
    }
}

fn reviewers_required_at(inputs: &Inputs, position: &Position) -> i32 {
    inputs
        .stages
        .iter()
        .find(|stage| stage.key == position.key)
        .map(|stage| stage.reviewers_required)
        .filter(|required| *required > 0)
        .unwrap_or(inputs.reviewers_required)
}

/// The newest work on a record. All-or-nothing on timestamps: partial coverage has no answer that
/// is not a guess about where the untimed position sits.
enum Settled {
    At(Position),
    Unattributable,
}

fn settling_position(inputs: &Inputs, record_id: Uuid, reached: &HashSet<Position>) -> Settled {
    let timed: Vec<&Position> = reached
        .iter()
        .filter(|position| {
            inputs
                .worked_at
                .keys()
                .any(|(p, r)| p == *position && *r == record_id)
        })
        .collect();
    if timed.len() == reached.len() && !reached.is_empty() {
        return Settled::At(
            timed
                .into_iter()
                .max_by_key(|position| {
                    (
                        inputs
                            .worked_at
                            .get(&((*position).clone(), record_id))
                            .copied(),
                        (*position).clone(),
                    )
                })
                .cloned()
                .expect("non-empty"),
        );
    }
    if timed.is_empty() {
        return Settled::At(reached.iter().max().cloned().expect("non-empty"));
    }
    Settled::Unattributable
}

fn derive(inputs: &Inputs) -> Result<(Value, Vec<Disposition>), Problem> {
    let identified_total = inputs.statuses.len() as i64;
    let quarantined = inputs
        .statuses
        .iter()
        .filter(|status| status.as_str() == "quarantined")
        .count() as i64;
    let rejected = inputs
        .statuses
        .iter()
        .filter(|status| status.as_str() == "rejected")
        .count() as i64;

    let mut positions: HashMap<Uuid, HashSet<Position>> = HashMap::new();
    for (position, record_id) in inputs.decisions.keys() {
        positions
            .entry(*record_id)
            .or_default()
            .insert(position.clone());
    }
    for (position, record_id) in inputs.adjudications.keys() {
        positions
            .entry(*record_id)
            .or_default()
            .insert(position.clone());
    }

    let mut records: Vec<Uuid> = inputs.record_ids.iter().copied().collect();
    records.sort();

    let mut dispositions = Vec::with_capacity(records.len());
    for record_id in records {
        if inputs.removed_by_merge.contains_key(&record_id) {
            dispositions.push(Disposition {
                record_id,
                bucket: "duplicate_removed",
                reason: None,
                settled_at: None,
                adjudicated: false,
                unattributable: false,
            });
            continue;
        }
        let Some(reached) = positions.get(&record_id) else {
            dispositions.push(Disposition {
                record_id,
                bucket: "not_yet_screened",
                reason: None,
                settled_at: None,
                adjudicated: false,
                unattributable: false,
            });
            continue;
        };
        match settling_position(inputs, record_id, reached) {
            Settled::Unattributable => {
                dispositions.push(Disposition {
                    record_id,
                    bucket: "not_yet_screened",
                    reason: None,
                    settled_at: None,
                    adjudicated: false,
                    unattributable: true,
                });
                continue;
            }
            Settled::At(position) => {
                if let Some((disposition, reason)) =
                    inputs.adjudications.get(&(position.clone(), record_id))
                {
                    dispositions.push(Disposition {
                        record_id,
                        bucket: bucket(disposition),
                        reason: reason.clone(),
                        settled_at: Some(position),
                        adjudicated: true,
                        unattributable: false,
                    });
                    continue;
                }
                let votes = inputs
                    .decisions
                    .get(&(position.clone(), record_id))
                    .cloned()
                    .unwrap_or_default();
                if (votes.len() as i32) < reviewers_required_at(inputs, &position) {
                    dispositions.push(Disposition {
                        record_id,
                        bucket: "not_yet_screened",
                        reason: None,
                        settled_at: None,
                        adjudicated: false,
                        unattributable: false,
                    });
                    continue;
                }
                let unique: HashSet<(String, Option<String>)> = votes.iter().cloned().collect();
                if unique.len() != 1 {
                    dispositions.push(Disposition {
                        record_id,
                        bucket: "not_yet_screened",
                        reason: None,
                        settled_at: None,
                        adjudicated: false,
                        unattributable: false,
                    });
                    continue;
                }
                let (disposition, reason) = unique.into_iter().next().expect("one");
                dispositions.push(Disposition {
                    record_id,
                    bucket: bucket(&disposition),
                    reason,
                    settled_at: Some(position),
                    adjudicated: false,
                    unattributable: false,
                });
            }
        }
    }

    let mut included = 0i64;
    let mut excluded = 0i64;
    let mut uncertain = 0i64;
    let mut duplicates_removed = 0i64;
    let mut not_yet_screened = 0i64;
    let mut adjudicated = 0i64;
    let mut reasons: BTreeMap<String, i64> = BTreeMap::new();
    for disposition in &dispositions {
        match disposition.bucket {
            "included" => included += 1,
            "excluded" => {
                excluded += 1;
                if let Some(reason) = &disposition.reason {
                    *reasons.entry(reason.clone()).or_insert(0) += 1;
                }
            }
            "uncertain" => uncertain += 1,
            "duplicate_removed" => duplicates_removed += 1,
            _ => not_yet_screened += 1,
        }
        if disposition.adjudicated {
            adjudicated += 1;
        }
    }
    let records_screened = included + excluded + uncertain;
    let records_total = inputs.record_ids.len() as i64;
    if identified_total != quarantined + rejected + records_total {
        return Err(Problem::unexpected(format!(
            "identified_total {identified_total} != quarantined {quarantined} + rejected {rejected} + records_total {records_total}"
        )));
    }
    if records_total != duplicates_removed + records_screened + not_yet_screened {
        return Err(Problem::unexpected(format!(
            "records_total {records_total} != duplicates_removed {duplicates_removed} + records_screened {records_screened} + not_yet_screened {not_yet_screened}"
        )));
    }

    let counts = json!({
        "identified_total": identified_total,
        "quarantined": quarantined,
        "rejected": rejected,
        "records_total": records_total,
        "duplicates_removed": duplicates_removed,
        "records_screened": records_screened,
        "not_yet_screened": not_yet_screened,
        "included": included,
        "excluded": excluded,
        "uncertain": uncertain,
        "exclusion_reasons": reasons,
        "adjudicated": adjudicated,
    });

    let mut by_stage: BTreeMap<String, Value> = BTreeMap::new();
    for stage in &inputs.stages {
        let reached: HashSet<Uuid> = positions
            .iter()
            .filter(|(_, reached_positions)| {
                reached_positions
                    .iter()
                    .any(|position| position.key == stage.key)
            })
            .map(|(record_id, _)| *record_id)
            .collect();
        let settled: Vec<&Disposition> = dispositions
            .iter()
            .filter(|disposition| {
                disposition
                    .settled_at
                    .as_ref()
                    .is_some_and(|position| position.key == stage.key)
            })
            .collect();
        let mut stage_reasons: BTreeMap<String, i64> = BTreeMap::new();
        for disposition in &settled {
            if disposition.bucket == "excluded"
                && let Some(reason) = &disposition.reason
            {
                *stage_reasons.entry(reason.clone()).or_insert(0) += 1;
            }
        }
        by_stage.insert(
            stage.key.clone(),
            json!({
                "stage_key": stage.key,
                "stage_order": stage.order,
                "name": stage.name,
                "reached": reached.len(),
                "settled_include": settled.iter().filter(|d| d.bucket == "included").count(),
                "settled_exclude": settled.iter().filter(|d| d.bucket == "excluded").count(),
                "settled_uncertain": settled.iter().filter(|d| d.bucket == "uncertain").count(),
                "exclusion_reasons": stage_reasons,
            }),
        );
    }

    Ok((
        json!({
            "counts": counts,
            "counts_by_stage": by_stage.values().cloned().collect::<Vec<_>>(),
        }),
        dispositions,
    ))
}

// ── collection and the owed gate ─────────────────────────────────────────────────────────────

/// L6: a caller who still owes a decision reads no aggregate of the work they are blinded from.
///
/// ONE predicate, in `screening`: the flow ledger and the screening routes must not be able to
/// disagree about what "owed" means. This wrapper only adds the operation name to the message.
pub async fn require_no_owed_decisions(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    review_id: Uuid,
    actor: &Actor,
    what: &str,
) -> Result<(), Problem> {
    match crate::evidence_review::screening::require_no_owed_decisions(tx, review_id, actor).await {
        Ok(()) => Ok(()),
        Err(problem) => Err(Problem {
            detail: format!("{what}: {}", problem.detail),
            ..problem
        }),
    }
}

/// Aggregate reads need every reviewer's decisions; the RLS policy shows a non-lead only their
/// own. Authorisation has already been decided by the caller of this function; this widens
/// VISIBILITY for the remainder of the transaction, which is the same read the reference performs.
async fn read_every_reviewers_work(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<(), Problem> {
    sqlx::query("SELECT set_config('app.actor_role', 'lead', true)")
        .execute(&mut **tx)
        .await
        .map_err(|error| Problem::from_db("elevating the ledger read", error))?;
    Ok(())
}

async fn collect(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: &Actor,
    review_id: Uuid,
) -> Result<Inputs, Problem> {
    let statuses: Vec<String> =
        sqlx::query_scalar("SELECT status FROM import_rows WHERE review_id = $1")
            .bind(review_id)
            .fetch_all(&mut **tx)
            .await
            .map_err(|error| Problem::from_db("reading import rows", error))?;
    let record_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT review_record_id FROM review_record_versions WHERE review_id = $1",
    )
    .bind(review_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading record versions", error))?;

    let mut removed_by_merge = HashMap::new();
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
                removed_by_merge.insert(member, cluster_id);
            }
        }
    }

    let stages: Vec<Stage> = sqlx::query(
        "SELECT stage_key, stage_order, name, reviewers_required FROM review_stages
          WHERE review_id = $1 ORDER BY stage_order",
    )
    .bind(review_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the review's stages", error))?
    .into_iter()
    .map(|row| Stage {
        key: row.get("stage_key"),
        order: row.get("stage_order"),
        name: row.get("name"),
        reviewers_required: row.get("reviewers_required"),
    })
    .collect();
    let reviewers_required: i32 = sqlx::query_scalar(
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

    read_every_reviewers_work(tx).await?;

    let mut decisions: DecisionMap = HashMap::new();
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
        let position = Position {
            order: row.get("stage_order"),
            key: row.get("stage_key"),
            round: row.get("round_number"),
        };
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

    let mut adjudications: AdjudicationMap = HashMap::new();
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
        let position = Position {
            order: row.get("stage_order"),
            key: row.get("stage_key"),
            round: row.get("round_number"),
        };
        adjudications.insert(
            (position, row.get("review_record_id")),
            (row.get("disposition"), row.get("exclusion_reason_code")),
        );
    }

    let _ = actor;
    Ok(Inputs {
        statuses,
        record_ids: record_ids.into_iter().collect(),
        removed_by_merge,
        decisions,
        adjudications,
        worked_at,
        stages,
        reviewers_required,
    })
}

async fn current_flow_revision(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: &Actor,
    review_id: Uuid,
) -> Result<(Uuid, i32), Problem> {
    let existing: Option<(Uuid, i32)> = sqlx::query_as(
        "SELECT flow_revision_id, revision_number FROM flow_revisions
          WHERE review_id = $1 AND closed_at IS NULL
          ORDER BY revision_number DESC LIMIT 1",
    )
    .bind(review_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the flow revision", error))?;
    if let Some(existing) = existing {
        return Ok(existing);
    }
    let reopens: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM review_state_history
          WHERE review_id = $1 AND from_state = 'closed' AND to_state = 'active'",
    )
    .bind(review_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("counting reopens", error))?;
    let number = (reopens + 1) as i32;
    let id = new_id();
    sqlx::query(
        "INSERT INTO flow_revisions (flow_revision_id, review_id, tenant_id, revision_number)
         VALUES ($1,$2,$3,$4)",
    )
    .bind(id)
    .bind(review_id)
    .bind(actor.tenant)
    .bind(number)
    .execute(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("opening the flow revision", error))?;
    Ok((id, number))
}

struct Flow {
    body: Value,
    dispositions: Vec<Disposition>,
    counts: Value,
    revision_id: Uuid,
    revision_number: i32,
}

async fn compute(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: &Actor,
    review_id: Uuid,
    persist: bool,
) -> Result<Flow, Problem> {
    let inputs = collect(tx, actor, review_id).await?;
    let (derived, dispositions) = derive(&inputs)?;
    let counts = derived.get("counts").cloned().unwrap_or(Value::Null);
    let digest = hash_value(&counts);
    let (revision_id, revision_number) = current_flow_revision(tx, actor, review_id).await?;
    if persist {
        sqlx::query(
            "INSERT INTO flow_projections
                 (projection_id, review_id, tenant_id, flow_revision_id, counts, canonical_counts_hash)
             VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .bind(new_id())
        .bind(review_id)
        .bind(actor.tenant)
        .bind(revision_id)
        .bind(&counts)
        .bind(&digest)
        .execute(&mut **tx)
        .await
        .map_err(|error| Problem::from_db("refreshing the flow projection", error))?;
    }
    let cached: Option<(String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT canonical_counts_hash, computed_at FROM flow_projections
          WHERE review_id = $1 ORDER BY computed_at DESC LIMIT 1",
    )
    .bind(review_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the flow projection", error))?;
    let stale = cached.as_ref().is_some_and(|(hash, _)| hash != &digest);
    let body = json!({
        "review_id": review_id.to_string(),
        "flow_revision_id": revision_id.to_string(),
        "flow_revision_number": revision_number,
        "counts": counts,
        "counts_by_stage": derived.get("counts_by_stage").cloned().unwrap_or(Value::Null),
        "canonical_counts_hash": digest,
        "schema_ref": LEDGER_SCHEMA_REF,
        "projection_stale": stale,
        "projection_computed_at": cached.map(|(_, at)| at.to_rfc3339()),
    });
    Ok(Flow {
        body,
        dispositions,
        counts,
        revision_id,
        revision_number,
    })
}

// ── flow handlers ────────────────────────────────────────────────────────────────────────────

#[utoipa::path(get, path = "/api/reviews/{review_id}/flow", operation_id = "get_review_flow", responses((status = 200, description = "ok")))]
pub async fn get_flow(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
) -> Result<Json<Value>, Problem> {
    let pool = pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the flow transaction", error))?;
    require_review_lead_for(&mut tx, &actor, review_id, "reading the flow ledger").await?;
    require_no_owed_decisions(&mut tx, review_id, &actor, "the flow ledger").await?;
    let flow = compute(&mut tx, &actor, review_id, false).await?;
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the flow read", error))?;
    Ok(Json(flow.body))
}

#[utoipa::path(post, path = "/api/reviews/{review_id}/flow:rebuild", operation_id = "rebuild_review_flow", responses((status = 200, description = "ok")))]
pub async fn rebuild_flow(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
) -> Result<Json<Value>, Problem> {
    let pool = pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the flow transaction", error))?;
    require_review_lead_for(&mut tx, &actor, review_id, "rebuilding the flow ledger").await?;
    require_no_owed_decisions(&mut tx, review_id, &actor, "the flow ledger").await?;
    let flow = compute(&mut tx, &actor, review_id, true).await?;
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the flow rebuild", error))?;
    Ok(Json(flow.body))
}

#[utoipa::path(post, path = "/api/reviews/{review_id}/flow:audit", operation_id = "audit_review_flow", responses((status = 200, description = "ok")))]
pub async fn audit_flow(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
) -> Result<Json<Value>, Problem> {
    let pool = pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the flow transaction", error))?;
    require_review_lead_for(&mut tx, &actor, review_id, "auditing the flow ledger").await?;
    require_no_owed_decisions(&mut tx, review_id, &actor, "the flow ledger").await?;
    let flow = compute(&mut tx, &actor, review_id, false).await?;
    let digest = flow.body["canonical_counts_hash"].clone();
    let sealed: Vec<String> = sqlx::query_scalar(
        "SELECT canonical_counts_hash FROM flow_projections WHERE review_id = $1",
    )
    .bind(review_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading the sealed projections", error))?;
    let mismatched = sealed.iter().filter(|hash| **hash != digest).count();
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the flow audit", error))?;
    Ok(Json(json!({
        "review_id": review_id.to_string(),
        "computed_hash": digest,
        "sealed_projections": sealed.len(),
        "mismatched_projections": mismatched,
        "consistent": mismatched == 0,
    })))
}

// ── exports ──────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
struct ExportRow {
    export_id: Uuid,
    review_id: Uuid,
    corpus_version_id: Uuid,
    flow_revision_id: Uuid,
    schema_ref: String,
    manifest_hash: String,
    payload_digest: String,
    payload: Value,
    omissions: Value,
    created_by_subject_id: String,
    created_at: DateTime<Utc>,
    sealed_with: String,
}

fn export_summary(row: &ExportRow, with_payload: bool) -> Value {
    let included = row
        .payload
        .get("included")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let omissions = row.omissions.as_array().map(Vec::len).unwrap_or(0);
    let mut body = json!({
        "replayed": false,
        "export_id": row.export_id.to_string(),
        "review_id": row.review_id.to_string(),
        "corpus_version_id": row.corpus_version_id.to_string(),
        "flow_revision_id": row.flow_revision_id.to_string(),
        "schema_ref": row.schema_ref,
        "manifest_hash": row.manifest_hash,
        "sealed_with": row.sealed_with,
        "payload_digest": row.payload_digest,
        "included_count": included,
        "omissions_count": omissions,
        "created_by_subject_id": row.created_by_subject_id,
        "created_at": row.created_at.to_rfc3339(),
    });
    if with_payload && let Some(object) = body.as_object_mut() {
        object.insert("payload".to_owned(), row.payload.clone());
        object.insert("omissions".to_owned(), row.omissions.clone());
    }
    body
}

async fn load_export(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    review_id: Uuid,
    export_id: Uuid,
) -> Result<ExportRow, Problem> {
    sqlx::query_as::<_, ExportRow>(
        "SELECT export_id, review_id, corpus_version_id, flow_revision_id, schema_ref, manifest_hash,
                payload_digest, payload, omissions, created_by_subject_id, created_at, sealed_with
           FROM inclusion_exports WHERE export_id = $1 AND review_id = $2",
    )
    .bind(export_id)
    .bind(review_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("loading the export", error))?
    .ok_or_else(|| Problem::not_found("no such export for this review"))
}

#[utoipa::path(post, path = "/api/reviews/{review_id}/exports", operation_id = "create_export", responses((status = 200, description = "ok")))]
pub async fn create_export(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let pool = pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the export transaction", error))?;
    let subject =
        require_review_lead_for(&mut tx, &actor, review_id, "sealing the inclusion set").await?;
    require_no_owed_decisions(&mut tx, review_id, &actor, "sealing the inclusion set").await?;
    let flow = compute(&mut tx, &actor, review_id, true).await?;

    let unattributable = flow
        .dispositions
        .iter()
        .filter(|disposition| disposition.unattributable)
        .count();
    if unattributable > 0 {
        return Err(Problem::validation(format!(
            "{unattributable} records reached stages the ledger cannot order: some of the work on them \
             carries a decision time and some does not, so no stage can be shown to have settled them. \
             Screening them again will not clear this."
        )));
    }
    let not_yet = flow
        .counts
        .get("not_yet_screened")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if not_yet > 0 {
        return Err(Problem::validation(format!(
            "{not_yet} records are not yet screened; an export now would present an incomplete review \
             as a complete inclusion set"
        ))
        .with("not_yet_screened", json!(not_yet)));
    }
    let unresolved: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM screening_conflicts c
          WHERE c.review_id = $1
            AND NOT EXISTS (SELECT 1 FROM screening_adjudications a WHERE a.conflict_id = c.conflict_id)",
    )
    .bind(review_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading unresolved disagreements", error))?;
    if unresolved > 0 {
        return Err(Problem::validation(format!(
            "{unresolved} disagreements are unresolved; a sealed inclusion set requires every conflict \
             adjudicated"
        ))
        .with("unresolved_conflicts", json!(unresolved)));
    }

    let corpus: Option<(Uuid, i32)> = sqlx::query_as(
        "SELECT corpus_version_id, member_count FROM corpus_versions
          WHERE review_id = $1 ORDER BY version_number DESC LIMIT 1",
    )
    .bind(review_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading the corpus version", error))?;
    let Some((corpus_version_id, _)) = corpus else {
        return Err(Problem::validation(
            "seal a corpus version before sealing an inclusion set",
        ));
    };

    let included_ids: Vec<Uuid> = flow
        .dispositions
        .iter()
        .filter(|disposition| disposition.bucket == "included")
        .map(|disposition| disposition.record_id)
        .collect();
    let mut included = Vec::with_capacity(included_ids.len());
    for record_id in &included_ids {
        let row: Option<(Uuid, String, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT review_record_version_id, review_record_id::text, title, doi
               FROM review_record_versions
              WHERE review_id = $1 AND review_record_id = $2
              ORDER BY version_number DESC LIMIT 1",
        )
        .bind(review_id)
        .bind(record_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("reading the included record", error))?;
        if let Some((version_id, record_text, title, doi)) = row {
            included.push(json!({
                "review_record_id": record_text,
                "review_record_version_id": version_id.to_string(),
                "title": title,
                "doi": doi,
            }));
        }
    }
    let omissions: Vec<Value> = flow
        .dispositions
        .iter()
        .filter(|disposition| {
            matches!(disposition.bucket, "excluded" | "uncertain")
        })
        .map(|disposition| {
            json!({
                "review_record_id": disposition.record_id.to_string(),
                "disposition": if disposition.bucket == "excluded" { "exclude" } else { "uncertain" },
                "exclusion_reason_code": disposition.reason,
            })
        })
        .collect();

    let manifest = json!({
        "schema_ref": EXPORT_SCHEMA_REF,
        "review_id": review_id.to_string(),
        "corpus_version_id": corpus_version_id.to_string(),
        "flow_revision_id": flow.revision_id.to_string(),
        "flow_revision_number": flow.revision_number,
        "counts": flow.counts,
        "included_count": included.len(),
        "omissions_count": omissions.len(),
    });
    let manifest_hash = hash_value(&manifest);
    let payload = json!({"manifest": manifest, "included": included, "omissions": omissions});
    // RFC 8785 canonical bytes, not `serde_json::to_vec`: the payload is stored in `jsonb`,
    // which reorders keys, so only a canonical digest can be recomputed from the stored value.
    let payload_digest = format!(
        "sha256:{}",
        conveyor_domain::compare::content_hash(&payload)
    );
    // `uq_export_manifest_per_review`: the same inclusion set seals ONCE. A repeat request is a
    // replay, answered with the recorded row and `replayed: true` — never a second insert.
    if let Some(existing) = sqlx::query_as::<_, ExportRow>(
        "SELECT export_id, review_id, corpus_version_id, flow_revision_id, schema_ref, manifest_hash,
                payload_digest, payload, omissions, created_by_subject_id, created_at, sealed_with
           FROM inclusion_exports WHERE review_id = $1 AND manifest_hash = $2",
    )
    .bind(review_id)
    .bind(&manifest_hash)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("looking up the sealed inclusion set", error))?
    {
        let mut body = export_summary(&existing, true);
        if let Some(object) = body.as_object_mut() {
            object.insert("replayed".to_owned(), Value::Bool(true));
        }
        tx.commit()
            .await
            .map_err(|error| Problem::from_db("committing the export replay", error))?;
        return Ok((StatusCode::OK, Json(body)));
    }
    let export_id = new_id();
    sqlx::query(
        "INSERT INTO inclusion_exports
             (export_id, review_id, tenant_id, flow_revision_id, corpus_version_id, schema_ref,
              manifest_hash, payload_digest, payload, omissions, created_by_subject_id, sealed_with)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
    )
    .bind(export_id)
    .bind(review_id)
    .bind(actor.tenant)
    .bind(flow.revision_id)
    .bind(corpus_version_id)
    .bind(EXPORT_SCHEMA_REF)
    .bind(&manifest_hash)
    .bind(&payload_digest)
    .bind(&payload)
    .bind(json!(omissions))
    .bind(&subject)
    .bind(SEALED_WITH)
    .execute(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("sealing the inclusion set", error))?;
    let row = load_export(&mut tx, review_id, export_id).await?;
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the export", error))?;
    Ok((StatusCode::CREATED, Json(export_summary(&row, true))))
}

#[utoipa::path(get, path = "/api/reviews/{review_id}/exports", operation_id = "list_exports", responses((status = 200, description = "ok")))]
pub async fn list_exports(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
) -> Result<Json<Value>, Problem> {
    let pool = pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the export transaction", error))?;
    require_review_lead_for(&mut tx, &actor, review_id, "listing exports").await?;
    require_no_owed_decisions(&mut tx, review_id, &actor, "listing exports").await?;
    let rows = sqlx::query_as::<_, ExportRow>(
        "SELECT export_id, review_id, corpus_version_id, flow_revision_id, schema_ref, manifest_hash,
                payload_digest, payload, omissions, created_by_subject_id, created_at, sealed_with
           FROM inclusion_exports WHERE review_id = $1 ORDER BY created_at",
    )
    .bind(review_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("listing exports", error))?;
    let items: Vec<Value> = rows.iter().map(|row| export_summary(row, false)).collect();
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the export list", error))?;
    Ok(Json(json!({"items": items, "count": items.len()})))
}

#[utoipa::path(get, path = "/api/reviews/{review_id}/exports/{export_id}", operation_id = "get_export", responses((status = 200, description = "ok")))]
pub async fn get_export(
    State(state): State<Shared>,
    actor: Actor,
    Path((review_id, export_id)): Path<(Uuid, Uuid)>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Value>, Problem> {
    let with_payload = query
        .get("with_payload")
        .map(|raw| matches!(raw.as_str(), "true" | "1" | "yes"))
        .unwrap_or(false);
    let pool = pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the export transaction", error))?;
    require_review_lead_for(&mut tx, &actor, review_id, "reading an export").await?;
    require_no_owed_decisions(&mut tx, review_id, &actor, "reading an export").await?;
    let row = load_export(&mut tx, review_id, export_id).await?;
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the export read", error))?;
    Ok(Json(export_summary(&row, with_payload)))
}

#[utoipa::path(get, path = "/api/reviews/{review_id}/exports/{export_id}:verify", operation_id = "verify_export", responses((status = 200, description = "ok")))]
pub async fn verify_export(
    State(state): State<Shared>,
    actor: Actor,
    Path((review_id, export_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, Problem> {
    let pool = pool(&state)?;
    let mut tx = db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the export transaction", error))?;
    require_review_lead_for(&mut tx, &actor, review_id, "verifying an export").await?;
    require_no_owed_decisions(&mut tx, review_id, &actor, "verifying an export").await?;
    let row = load_export(&mut tx, review_id, export_id).await?;
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the export read", error))?;

    let manifest = row.payload.get("manifest").cloned().unwrap_or(Value::Null);
    let recomputed_manifest = hash_value(&manifest);
    let recomputed_payload = format!(
        "sha256:{}",
        conveyor_domain::compare::content_hash(&row.payload)
    );
    let manifest_ok = recomputed_manifest == row.manifest_hash;
    let payload_ok = recomputed_payload == row.payload_digest;
    Ok(Json(json!({
        "export_id": row.export_id.to_string(),
        "review_id": row.review_id.to_string(),
        "verified": manifest_ok && payload_ok,
        "manifest_hash": row.manifest_hash,
        "recomputed_manifest_hash": recomputed_manifest,
        "payload_digest": row.payload_digest,
        "recomputed_payload_digest": recomputed_payload,
        "sealed_with": row.sealed_with,
    })))
}
