//! The screening family: deduplication (`ER-003`), assignments, blinded decisions,
//! conflicts, unblinding and adjudication (`ER-004`).
//!
//! Ported from `evidence_review_service` at revision `bac4f66`. Three invariants decide
//! every handler here:
//!
//! * **a machine cannot decide** — every decision, resolution, unblinding and adjudication
//!   calls [`Actor::require_human`] before it touches the database;
//! * **reviewer B cannot see reviewer A** — the queue, the decision list and the conflict
//!   panel are scoped by the caller's own assignments and by the caller's own
//!   `app.actor_subject_id`; a row the caller may not see is `404`, never `403`; and the
//!   aggregates in a payload are page- and stage-scoped over the caller's own work;
//! * **history is append-only** — no `UPDATE`/`DELETE` on a decision, resolution,
//!   unblinding event or adjudication table; a correction is a superseding row.
//!
//! The flow derivation is used in exactly two places here — scoping a stage-N assignment
//! round to what settled `included` at stage N-1, and deciding whether an open conflict is
//! still outstanding — and both are answered from the same chain tails the ledger reads.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use sqlx::{AssertSqlSafe, Postgres, Transaction};
use uuid::Uuid;

use super::{REVIEW_NOT_FOUND, hash_value, new_id, pool};
use crate::actor::Actor;
use crate::problem::Problem;
use crate::state::Shared;

// ---------------------------------------------------------------------------
// Bounds
// ---------------------------------------------------------------------------

/// Ceiling on `round_number` on every route that carries one. The column is `integer`;
/// 1000 is ~250x the realistic ceiling (a re-screen after an amendment is round 3) and
/// keeps a hostile literal a `422` at the edge instead of a driver overflow.
const MAX_ROUND_NUMBER: i64 = 1000;

/// Ceiling on how many reviewers ONE `POST /assignments` may name.
const MAX_REVIEWERS_PER_ASSIGNMENT: usize = 20;

/// Ceiling on ONE reviewer subject id.
const MAX_SUBJECT_ID_LENGTH: usize = 256;

/// Page bounds for `GET /decisions`, matching `/records` and `/conflicts`.
const MAX_DECISIONS_PAGE: i64 = 500;
const DEFAULT_DECISIONS_PAGE: i64 = 200;

/// Assignment rows per multi-VALUES INSERT statement.
const ASSIGNMENT_INSERT_CHUNK: usize = 500;

/// Deepest a caller-supplied free-form body (the dedup config) may nest.
const MAX_BODY_DEPTH: usize = 64;

const ENGINE_ID: &str = "exact-identifier-baseline";
const ENGINE_VERSION: &str = "1.0.0";
const EXACT_SCORE: f64 = 1.0;

/// What a conflict-detail response says before an unblinding is recorded. A literal
/// string rather than an omitted key, so a client cannot mistake "redacted" for "nothing".
const REDACTED: &str = "redacted";

const RECORD_SCHEMA_REF: &str =
    "https://contracts.nonagentic.ai/evidence/review-record.v1.schema.json";
const ENVELOPE_SCHEMA_REF: &str = "urn:ahtapol:research-event-envelope:1";
const OUTBOX_PRODUCER: &str = "evidence-review";

const OPEN_STATES: [&str; 4] = ["draft", "configured", "active", "paused"];

/// The fields a merge's survivorship may choose between — `ck_survivorship_field_name`.
const SURVIVABLE_FIELDS: [&str; 11] = [
    "arxiv_id",
    "abstract",
    "authors",
    "container_title",
    "doi",
    "pmcid",
    "pmid",
    "publication_year",
    "record_type",
    "title",
    "url",
];

/// The hashed content view of a record version.
const RECORD_CONTENT_FIELDS: [&str; 11] = [
    "record_type",
    "title",
    "abstract",
    "publication_year",
    "authors",
    "container_title",
    "doi",
    "pmid",
    "pmcid",
    "arxiv_id",
    "url",
];

/// The pre-ADR regulatory denylist's fallback vocabulary (`ER-002.7`).
const FORBIDDEN_KEYS: [&str; 11] = [
    "work_type",
    "regulatory_instrument",
    "regulatory_provision",
    "regulatory_effect",
    "legal_event",
    "legal_status",
    "regulatory_applicability",
    "regulatory_change",
    "normative_citation",
    "consolidation_snapshot",
    "recommendation_regulatory_link",
];

type Tx<'a> = Transaction<'a, Postgres>;

// ---------------------------------------------------------------------------
// Row shapes
// ---------------------------------------------------------------------------

#[derive(Debug, sqlx::FromRow)]
struct ReviewRow {
    created_by_subject_id: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct StageRow {
    stage_id: Uuid,
    stage_order: i32,
    stage_key: String,
    name: String,
    reviewers_required: i32,
}

#[derive(Debug, sqlx::FromRow)]
struct ProtocolRow {
    reviewers_required: i32,
    exclusion_reason_vocabulary: Value,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct VersionRow {
    review_record_id: Uuid,
    review_record_version_id: Uuid,
    version_number: i32,
    review_id: Uuid,
    record_type: String,
    title: Option<String>,
    #[sqlx(rename = "abstract_text")]
    abstract_text: Option<String>,
    publication_year: Option<i32>,
    authors: Option<Value>,
    container_title: Option<String>,
    doi: Option<String>,
    pmid: Option<String>,
    pmcid: Option<String>,
    arxiv_id: Option<String>,
    url: Option<String>,
    source_import_row_id: Option<Uuid>,
}

#[derive(Debug, sqlx::FromRow)]
struct DedupRunRow {
    dedup_run_id: Uuid,
    review_id: Uuid,
    corpus_version_id: Uuid,
    engine_id: String,
    engine_version: String,
    config: Value,
    input_digest: String,
    cluster_count: i32,
    pair_count: i32,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ResolutionRow {
    resolution_id: Uuid,
    cluster_id: Uuid,
    resolution_type: String,
    supersedes_resolution_id: Option<Uuid>,
    survivor_record_id: Option<Uuid>,
    rationale: String,
    decided_by_subject_id: String,
    decided_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct AssignmentRow {
    assignment_id: Uuid,
    review_record_id: Uuid,
    corpus_version_id: Uuid,
    assigned_at: DateTime<Utc>,
    #[allow(dead_code)]
    reviewer_subject_id: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct DecisionRow {
    decision_id: Uuid,
    review_record_id: Uuid,
    review_record_version_id: Uuid,
    stage_id: Uuid,
    round_number: i32,
    disposition: String,
    exclusion_reason_code: Option<String>,
    rationale: Option<String>,
    decided_at: DateTime<Utc>,
    supersedes_decision_id: Option<Uuid>,
    reviewer_subject_id: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ConflictRow {
    conflict_id: Uuid,
    stage_id: Uuid,
    round_number: i32,
    review_record_id: Uuid,
    opened_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct AdjudicationRow {
    adjudication_id: Uuid,
    conflict_id: Uuid,
    disposition: String,
    exclusion_reason_code: Option<String>,
    rationale: String,
    adjudicator_subject_id: String,
    adjudicator_display_name: Option<String>,
    supersedes_adjudication_id: Option<Uuid>,
    decided_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Small shapes used by the deduplication engine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Candidate {
    record_id: Uuid,
    version_id: Uuid,
    normalized_doi: Option<String>,
    normalized_pmid: Option<String>,
    normalized_title: Option<String>,
    publication_year: Option<i32>,
    title: Option<String>,
    doi: Option<String>,
    pmid: Option<String>,
}

#[derive(Debug, Clone)]
struct Pair {
    left: Uuid,
    right: Uuid,
    matched_on: &'static str,
}

#[derive(Debug, Clone)]
struct Cluster {
    cluster_key: String,
    members: Vec<Uuid>,
}

#[derive(Debug, Clone)]
struct MergedGroup {
    cluster_id: Uuid,
    members: BTreeSet<Uuid>,
    survivor: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct StagePosition {
    stage_order: i32,
    stage_key: String,
    round_number: i32,
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

fn uuid_value(value: Uuid) -> Value {
    Value::String(value.to_string())
}

fn opt_uuid_value(value: Option<Uuid>) -> Value {
    value.map_or(Value::Null, uuid_value)
}

fn opt_string_value(value: &Option<String>) -> Value {
    value.clone().map_or(Value::Null, Value::String)
}

fn timestamp(value: &DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn unprocessable(detail: impl Into<String>) -> Problem {
    Problem::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "unprocessable_state",
        "Unprocessable State",
        detail,
    )
}

/// The caller's `subject`, or a refusal that names what was being recorded.
fn human(actor: &Actor) -> Result<&str, Problem> {
    actor.require_human()
}

async fn set_role(tx: &mut Tx<'_>, role: &str) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT set_config('app.actor_role', $1, true)")
        .bind(role)
        .execute(&mut **tx)
        .await
        .map(|_| ())
}

async fn current_role(tx: &mut Tx<'_>) -> Result<String, sqlx::Error> {
    let role: Option<String> = sqlx::query_scalar("SELECT current_setting('app.actor_role', true)")
        .fetch_optional(&mut **tx)
        .await?
        .flatten();
    Ok(role.unwrap_or_default())
}

/// Raise `app.actor_role` to `lead` for ONE aggregate read and restore it afterwards.
///
/// Used only by conflict detection and by L6: both must count rows the caller's own
/// binding hides. Transaction-scoped (`is_local => true`), and restored on every path.
async fn elevate(tx: &mut Tx<'_>) -> Result<String, Problem> {
    let previous = current_role(tx)
        .await
        .map_err(|error| Problem::from_db("reading the actor role", error))?;
    set_role(tx, "lead")
        .await
        .map_err(|error| Problem::from_db("elevating the actor role", error))?;
    Ok(previous)
}

async fn restore_role(tx: &mut Tx<'_>, previous: &str) -> Result<(), Problem> {
    set_role(tx, previous)
        .await
        .map_err(|error| Problem::from_db("restoring the actor role", error))
}

// ---------------------------------------------------------------------------
// Request-body validation
// ---------------------------------------------------------------------------

fn body_object(body: &Value) -> Result<&Map<String, Value>, Problem> {
    body.as_object()
        .ok_or_else(|| Problem::validation("the request body must be a JSON object"))
}

fn body_value<'a>(body: &'a Map<String, Value>, name: &str) -> Option<&'a Value> {
    body.get(name).filter(|value| !value.is_null())
}

fn required_uuid(body: &Map<String, Value>, name: &str) -> Result<Uuid, Problem> {
    let raw = body_value(body, name)
        .and_then(Value::as_str)
        .ok_or_else(|| {
            Problem::validation(format!("{name} is required and must be a UUID string"))
        })?;
    Uuid::parse_str(raw).map_err(|_| Problem::validation(format!("{name} must be a UUID")))
}

fn optional_uuid(body: &Map<String, Value>, name: &str) -> Result<Option<Uuid>, Problem> {
    match body_value(body, name) {
        None => Ok(None),
        Some(Value::String(raw)) => Uuid::parse_str(raw)
            .map(Some)
            .map_err(|_| Problem::validation(format!("{name} must be a UUID"))),
        Some(_) => Err(Problem::validation(format!("{name} must be a UUID string"))),
    }
}

fn required_string(body: &Map<String, Value>, name: &str) -> Result<String, Problem> {
    match body_value(body, name) {
        Some(Value::String(raw)) if !raw.trim().is_empty() => Ok(raw.clone()),
        _ => Err(Problem::validation(format!(
            "{name} is required and must not be blank"
        ))),
    }
}

fn optional_string(body: &Map<String, Value>, name: &str) -> Result<Option<String>, Problem> {
    match body_value(body, name) {
        None => Ok(None),
        Some(Value::String(raw)) => Ok(Some(raw.clone())),
        Some(_) => Err(Problem::validation(format!("{name} must be a string"))),
    }
}

fn round_number(body: &Map<String, Value>) -> Result<i32, Problem> {
    let value = match body_value(body, "round_number") {
        None => 1,
        Some(Value::Number(number)) => number
            .as_i64()
            .ok_or_else(|| Problem::validation("round_number must be an integer"))?,
        Some(_) => return Err(Problem::validation("round_number must be an integer")),
    };
    if !(1..=MAX_ROUND_NUMBER).contains(&value) {
        return Err(Problem::validation(format!(
            "round_number must be between 1 and {MAX_ROUND_NUMBER}"
        )));
    }
    Ok(value as i32)
}

fn one_of(value: &str, name: &str, allowed: &[&str]) -> Result<String, Problem> {
    if allowed.contains(&value) {
        Ok(value.to_owned())
    } else {
        Err(Problem::validation(format!(
            "{name} must be one of {}",
            allowed.join(", ")
        )))
    }
}

/// Iterative depth + denylist guard for the one free-form body in this family.
fn guard_body(value: &Value, what: &str) -> Result<(), Problem> {
    let mut stack: Vec<(&Value, usize)> = vec![(value, 1)];
    while let Some((node, depth)) = stack.pop() {
        if depth > MAX_BODY_DEPTH {
            return Err(Problem::validation(format!(
                "{what} nests deeper than {MAX_BODY_DEPTH} levels"
            )));
        }
        match node {
            Value::Object(map) => {
                for (key, child) in map {
                    if is_forbidden_key(key) {
                        return Err(Problem::new(
                            StatusCode::BAD_REQUEST,
                            "forbidden_discriminator",
                            "Forbidden Discriminator",
                            format!("{what} carries the guarded key {key:?}"),
                        ));
                    }
                    stack.push((child, depth + 1));
                }
            }
            Value::Array(items) => {
                for child in items {
                    stack.push((child, depth + 1));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn is_forbidden_key(key: &str) -> bool {
    let trimmed = key.trim();
    // An `x-`/`x_`/`x.`/`x ` extension prefix is a real convention, not a guarded key.
    let mut chars = trimmed.chars();
    if matches!(chars.next(), Some('x') | Some('X'))
        && matches!(chars.next(), Some('-' | '_' | '.' | ' '))
    {
        return false;
    }
    let normalized: String = key
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect();
    FORBIDDEN_KEYS.iter().any(|candidate| {
        let forbidden: String = candidate
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .flat_map(|ch| ch.to_lowercase())
            .collect();
        forbidden == normalized
    })
}

// ---------------------------------------------------------------------------
// Review, protocol, stage and corpus reads
// ---------------------------------------------------------------------------

async fn load_review(tx: &mut Tx<'_>, review_id: Uuid) -> Result<ReviewRow, Problem> {
    sqlx::query_as::<_, ReviewRow>("SELECT created_by_subject_id FROM reviews WHERE review_id = $1")
        .bind(review_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|error| Problem::from_db("reading the review", error))?
        .ok_or_else(|| Problem::not_found(REVIEW_NOT_FOUND))
}

async fn current_state(tx: &mut Tx<'_>, review_id: Uuid) -> Result<Option<(String, i32)>, Problem> {
    let row = sqlx::query_as::<_, (String, i32)>(
        "SELECT to_state, revision_number FROM review_state_history \
         WHERE review_id = $1 ORDER BY revision_number DESC LIMIT 1",
    )
    .bind(review_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the review state", error))?;
    Ok(row)
}

/// Lead authority over ONE review: the realm role, or having created it.
async fn require_review_lead(
    actor: &Actor,
    review: &ReviewRow,
    what: &str,
) -> Result<String, Problem> {
    let subject = human(actor)?.to_owned();
    if actor.is_lead() || review.created_by_subject_id == subject {
        return Ok(subject);
    }
    Err(Problem::forbidden(format!(
        "{what} requires the review lead role, or having created this review"
    )))
}

async fn require_review_open(
    tx: &mut Tx<'_>,
    review_id: Uuid,
    what: &str,
) -> Result<String, Problem> {
    let (state, _) = current_state(tx, review_id)
        .await?
        .ok_or_else(|| Problem::not_found(REVIEW_NOT_FOUND))?;
    if OPEN_STATES.contains(&state.as_str()) {
        return Ok(state);
    }
    let remedy = if state == "closed" {
        "reopen it first, which needs an approval attestation bound to the closed revision"
    } else {
        "an archived review cannot be reopened"
    };
    Err(Problem::conflict(format!(
        "this review is {state}, so {what} would add to the history of a review that has \
         concluded — and an inclusion set may already have been published from it. To continue \
         working on it, {remedy}."
    )))
}

async fn active_protocol(tx: &mut Tx<'_>, review_id: Uuid) -> Result<Option<ProtocolRow>, Problem> {
    let protocol_version_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT protocol_version_id FROM review_state_history \
         WHERE review_id = $1 AND to_state = 'active' AND protocol_version_id IS NOT NULL \
         ORDER BY revision_number DESC LIMIT 1",
    )
    .bind(review_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the active protocol", error))?
    .flatten();
    let Some(protocol_version_id) = protocol_version_id else {
        return Ok(None);
    };
    sqlx::query_as::<_, ProtocolRow>(
        "SELECT reviewers_required, exclusion_reason_vocabulary \
         FROM review_protocol_versions WHERE protocol_version_id = $1",
    )
    .bind(protocol_version_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the active protocol", error))
}

fn vocabulary_contains(vocabulary: &Value, code: &str) -> bool {
    vocabulary
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(code)))
}

async fn stage_row(tx: &mut Tx<'_>, review_id: Uuid, stage_id: Uuid) -> Result<StageRow, Problem> {
    sqlx::query_as::<_, StageRow>(
        "SELECT stage_id, stage_order, stage_key, name, reviewers_required \
         FROM review_stages WHERE stage_id = $1 AND review_id = $2",
    )
    .bind(stage_id)
    .bind(review_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the review stage", error))?
    .ok_or_else(|| Problem::not_found("no such stage for this review"))
}

async fn latest_corpus(tx: &mut Tx<'_>, review_id: Uuid) -> Result<Uuid, Problem> {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT corpus_version_id FROM corpus_versions WHERE review_id = $1 \
         ORDER BY version_number DESC LIMIT 1",
    )
    .bind(review_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the sealed corpus", error))?
    .ok_or_else(|| Problem::not_found("this review has no sealed corpus version"))
}

const VERSION_COLUMNS: &str = "review_record_id, review_record_version_id, version_number, review_id, \
     record_type, title, abstract AS abstract_text, publication_year, authors, container_title, doi, \
     pmid, pmcid, arxiv_id, url, normalized_title, normalized_doi, normalized_pmid, source_import_row_id";

/// The same list, qualified for a join in which `corpus_version_members` also carries
/// `review_id` — an unqualified select of it is ambiguous, not a different column.
const VERSION_COLUMNS_QUALIFIED: &str = "rv.review_record_id, rv.review_record_version_id, rv.version_number, rv.review_id, \
     rv.record_type, rv.title, rv.abstract AS abstract_text, rv.publication_year, rv.authors, \
     rv.container_title, rv.doi, rv.pmid, rv.pmcid, rv.arxiv_id, rv.url, rv.normalized_title, \
     rv.normalized_doi, rv.normalized_pmid, rv.source_import_row_id";

/// The tail of each record's version chain, for a named subset, in one statement.
async fn current_record_versions(
    tx: &mut Tx<'_>,
    review_id: Uuid,
    record_ids: &[Uuid],
) -> Result<HashMap<Uuid, VersionRow>, Problem> {
    if record_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query_as::<_, VersionRow>(AssertSqlSafe(format!(
        "SELECT {VERSION_COLUMNS} FROM review_record_versions rv \
         WHERE rv.review_id = $1 AND rv.review_record_id = ANY($2) \
           AND rv.version_number = (SELECT max(v2.version_number) FROM review_record_versions v2 \
                                    WHERE v2.review_record_id = rv.review_record_id)"
    )))
    .bind(review_id)
    .bind(record_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading record versions", error))?;
    Ok(rows
        .into_iter()
        .map(|row| (row.review_record_id, row))
        .collect())
}

/// The version of each named record AS SEALED INTO `corpus_version_id`.
async fn corpus_member_versions(
    tx: &mut Tx<'_>,
    corpus_version_id: Uuid,
    record_ids: &[Uuid],
) -> Result<HashMap<Uuid, VersionRow>, Problem> {
    if record_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query_as::<_, VersionRow>(AssertSqlSafe(format!(
        "SELECT {VERSION_COLUMNS_QUALIFIED} FROM review_record_versions rv \
         JOIN corpus_version_members m \
           ON m.review_record_version_id = rv.review_record_version_id \
         WHERE m.corpus_version_id = $1 AND rv.review_record_id = ANY($2)"
    )))
    .bind(corpus_version_id)
    .bind(record_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the sealed corpus members", error))?;
    Ok(rows
        .into_iter()
        .map(|row| (row.review_record_id, row))
        .collect())
}

// ---------------------------------------------------------------------------
// The deduplication engine (`ER-003.2`), pure and deterministic
// ---------------------------------------------------------------------------

fn find_pairs(candidates: &[Candidate], config: &Value) -> Vec<Pair> {
    let default_rules = vec!["normalized_doi", "normalized_pmid", "normalized_title_year"];
    let rules: Vec<&str> = config
        .get("rules")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).collect())
        .unwrap_or(default_rules);

    let mut ordered: Vec<&Candidate> = candidates.iter().collect();
    ordered.sort_by_key(|candidate| candidate.record_id);

    let mut found: BTreeMap<(Uuid, Uuid), Pair> = BTreeMap::new();

    let mut emit = |group: &[&Candidate], matched_on: &'static str| {
        for (index, left) in group.iter().enumerate() {
            for right in &group[index + 1..] {
                let (lo, hi) = if left.record_id < right.record_id {
                    (left.record_id, right.record_id)
                } else {
                    (right.record_id, left.record_id)
                };
                found.entry((lo, hi)).or_insert(Pair {
                    left: lo,
                    right: hi,
                    matched_on,
                });
            }
        }
    };

    if rules.contains(&"normalized_doi") {
        let mut groups: BTreeMap<String, Vec<&Candidate>> = BTreeMap::new();
        for candidate in &ordered {
            if let Some(value) = &candidate.normalized_doi {
                groups.entry(value.clone()).or_default().push(candidate);
            }
        }
        for group in groups.values() {
            if group.len() > 1 {
                emit(group, "normalized_doi");
            }
        }
    }

    if rules.contains(&"normalized_pmid") {
        let mut groups: BTreeMap<String, Vec<&Candidate>> = BTreeMap::new();
        for candidate in &ordered {
            if let Some(value) = &candidate.normalized_pmid {
                groups.entry(value.clone()).or_default().push(candidate);
            }
        }
        for group in groups.values() {
            if group.len() > 1 {
                emit(group, "normalized_pmid");
            }
        }
    }

    if rules.contains(&"normalized_title_year") {
        // BOTH required. A null year cannot participate: null-to-null would cluster the
        // whole untitled-year corpus into one group.
        let mut groups: BTreeMap<(String, i32), Vec<&Candidate>> = BTreeMap::new();
        for candidate in &ordered {
            if let (Some(title), Some(year)) =
                (&candidate.normalized_title, candidate.publication_year)
            {
                groups
                    .entry((title.clone(), year))
                    .or_default()
                    .push(candidate);
            }
        }
        for group in groups.values() {
            if group.len() > 1 {
                emit(group, "normalized_title_year");
            }
        }
    }

    found.into_values().collect()
}

fn build_clusters(pairs: &[Pair]) -> Vec<Cluster> {
    fn find(parent: &mut HashMap<Uuid, Uuid>, node: Uuid) -> Uuid {
        let entry = *parent.entry(node).or_insert(node);
        if entry == node {
            return node;
        }
        let root = find(parent, entry);
        parent.insert(node, root);
        root
    }

    let mut parent: HashMap<Uuid, Uuid> = HashMap::new();
    let mut ordered = pairs.to_vec();
    ordered.sort_by_key(|pair| (pair.left, pair.right));
    for pair in &ordered {
        let left = find(&mut parent, pair.left);
        let right = find(&mut parent, pair.right);
        if left == right {
            continue;
        }
        let (lo, hi) = if left < right {
            (left, right)
        } else {
            (right, left)
        };
        parent.insert(hi, lo);
    }

    let nodes: Vec<Uuid> = parent.keys().copied().collect();
    let mut groups: BTreeMap<Uuid, Vec<Uuid>> = BTreeMap::new();
    for node in nodes {
        let root = find(&mut parent, node);
        groups.entry(root).or_default().push(node);
    }

    let mut clusters = Vec::new();
    for members in groups.into_values() {
        let mut members = members;
        members.sort();
        if members.len() < 2 {
            continue;
        }
        let joined = members
            .iter()
            .map(Uuid::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        let digest = Sha256::digest(joined.as_bytes());
        let mut key = String::with_capacity(32);
        for byte in digest.iter().take(16) {
            use std::fmt::Write as _;
            let _ = write!(key, "{byte:02x}");
        }
        clusters.push(Cluster {
            cluster_key: key,
            members,
        });
    }
    clusters.sort_by(|a, b| a.cluster_key.cmp(&b.cluster_key));
    clusters
}

/// Merge groups that would leave a real study in no bucket at all.
fn components_without_survivor(merges: &[MergedGroup]) -> Vec<(BTreeSet<Uuid>, BTreeSet<Uuid>)> {
    fn find(parent: &mut HashMap<Uuid, Uuid>, node: Uuid) -> Uuid {
        let entry = *parent.entry(node).or_insert(node);
        if entry == node {
            return node;
        }
        let root = find(parent, entry);
        parent.insert(node, root);
        root
    }

    let mut ordered = merges.to_vec();
    ordered.sort_by_key(|merge| merge.cluster_id);

    let mut parent: HashMap<Uuid, Uuid> = HashMap::new();
    for merge in &ordered {
        let members: Vec<Uuid> = merge.members.iter().copied().collect();
        for member in members.iter().skip(1) {
            let left = find(&mut parent, members[0]);
            let right = find(&mut parent, *member);
            if left == right {
                continue;
            }
            let (lo, hi) = if left < right {
                (left, right)
            } else {
                (right, left)
            };
            parent.insert(hi, lo);
        }
    }

    let removed: BTreeSet<Uuid> = ordered
        .iter()
        .flat_map(|merge| {
            merge
                .members
                .iter()
                .copied()
                .filter(|member| *member != merge.survivor)
        })
        .collect();

    let mut components: BTreeMap<Uuid, BTreeSet<Uuid>> = BTreeMap::new();
    let mut clusters_of: BTreeMap<Uuid, BTreeSet<Uuid>> = BTreeMap::new();
    for merge in &ordered {
        let Some(first) = merge.members.iter().next().copied() else {
            continue;
        };
        let root = find(&mut parent, first);
        components
            .entry(root)
            .or_default()
            .extend(merge.members.iter().copied());
        clusters_of
            .entry(root)
            .or_default()
            .insert(merge.cluster_id);
    }

    let mut emptied = Vec::new();
    for (root, members) in components {
        if members.difference(&removed).next().is_none() {
            emptied.push((members, clusters_of.get(&root).cloned().unwrap_or_default()));
        }
    }
    emptied.sort_by_key(|(members, _)| {
        members
            .iter()
            .map(Uuid::to_string)
            .collect::<Vec<_>>()
            .join(",")
    });
    emptied
}

fn presentation_value(candidate: &Candidate, field: &str) -> Option<Value> {
    match field {
        "normalized_doi" => candidate.normalized_doi.clone().map(Value::String),
        "normalized_pmid" => candidate.normalized_pmid.clone().map(Value::String),
        "normalized_title" => candidate.normalized_title.clone().map(Value::String),
        "title" => candidate.title.clone().map(Value::String),
        "doi" => candidate.doi.clone().map(Value::String),
        "pmid" => candidate.pmid.clone().map(Value::String),
        "publication_year" => candidate.publication_year.map(Value::from),
        _ => None,
    }
}

fn canonical_presentation(members: &[Candidate]) -> Value {
    let fields = [
        "normalized_doi",
        "normalized_pmid",
        "normalized_title",
        "title",
        "doi",
        "pmid",
        "publication_year",
    ];
    let mut ordered: Vec<&Candidate> = members.iter().collect();
    ordered.sort_by_key(|candidate| candidate.record_id);
    let mut presentation = Map::new();
    for field in fields {
        let mut seen: BTreeMap<String, (Value, Vec<Value>)> = BTreeMap::new();
        for member in &ordered {
            let Some(value) = presentation_value(member, field) else {
                continue;
            };
            let key = value.to_string();
            seen.entry(key)
                .or_insert_with(|| (value, Vec::new()))
                .1
                .push(uuid_value(member.version_id));
        }
        presentation.insert(
            field.to_owned(),
            Value::Array(
                seen.into_values()
                    .map(|(value, sources)| json!({"value": value, "sources": sources}))
                    .collect(),
            ),
        );
    }
    Value::Object(presentation)
}

fn default_config() -> Value {
    json!({
        "rules": ["normalized_doi", "normalized_pmid", "normalized_title_year"],
        "engine_id": ENGINE_ID,
        "engine_version": ENGINE_VERSION,
    })
}

fn effective_config(overlay: Option<&Value>) -> Value {
    let mut effective = default_config();
    if let (Some(base), Some(extra)) = (
        effective.as_object_mut(),
        overlay.and_then(Value::as_object),
    ) {
        for (key, value) in extra {
            base.insert(key.clone(), value.clone());
        }
    }
    effective
}

fn input_digest(corpus_version_id: Uuid, config: &Value) -> String {
    hash_value(&json!({
        "corpus_version_id": corpus_version_id.to_string(),
        "engine_id": config.get("engine_id").cloned().unwrap_or_else(|| json!(ENGINE_ID)),
        "engine_version": config.get("engine_version").cloned().unwrap_or_else(|| json!(ENGINE_VERSION)),
        "config": config,
    }))
}

fn advisory_lock(parts: &str) -> i64 {
    let digest = Sha256::digest(parts.as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    i64::from_be_bytes(bytes)
}

// ---------------------------------------------------------------------------
// Resolution chain helpers
// ---------------------------------------------------------------------------

fn resolution_tail(rows: &[ResolutionRow]) -> Option<&ResolutionRow> {
    let mut by_supersedes: HashMap<Uuid, &ResolutionRow> = HashMap::new();
    let mut head: Option<&ResolutionRow> = None;
    for row in rows {
        match row.supersedes_resolution_id {
            Some(previous) => {
                by_supersedes.insert(previous, row);
            }
            None => {
                if head.is_none() {
                    head = Some(row);
                }
            }
        }
    }
    let mut node = head?;
    let mut seen: HashSet<Uuid> = HashSet::from([node.resolution_id]);
    while let Some(next) = by_supersedes.get(&node.resolution_id) {
        if !seen.insert(next.resolution_id) {
            break;
        }
        node = next;
    }
    Some(node)
}

fn adjudication_tail(rows: &[AdjudicationRow]) -> Option<&AdjudicationRow> {
    let mut by_supersedes: HashMap<Uuid, &AdjudicationRow> = HashMap::new();
    let mut head: Option<&AdjudicationRow> = None;
    for row in rows {
        match row.supersedes_adjudication_id {
            Some(previous) => {
                by_supersedes.insert(previous, row);
            }
            None => {
                if head.is_none() {
                    head = Some(row);
                }
            }
        }
    }
    let mut node = head?;
    let mut seen: HashSet<Uuid> = HashSet::from([node.adjudication_id]);
    while let Some(next) = by_supersedes.get(&node.adjudication_id) {
        if !seen.insert(next.adjudication_id) {
            break;
        }
        node = next;
    }
    Some(node)
}

async fn current_resolution(
    tx: &mut Tx<'_>,
    cluster_id: Uuid,
) -> Result<Option<ResolutionRow>, Problem> {
    let rows = sqlx::query_as::<_, ResolutionRow>(
        "SELECT resolution_id, cluster_id, review_id, resolution_type, supersedes_resolution_id, \
         survivor_record_id, rationale, decided_by_subject_id, decided_at \
         FROM duplicate_resolutions WHERE cluster_id = $1 ORDER BY decided_at, resolution_id",
    )
    .bind(cluster_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading duplicate resolutions", error))?;
    Ok(resolution_tail(&rows).cloned())
}

async fn current_adjudication(
    tx: &mut Tx<'_>,
    conflict_id: Uuid,
) -> Result<Option<AdjudicationRow>, Problem> {
    let rows = sqlx::query_as::<_, AdjudicationRow>(
        "SELECT adjudication_id, conflict_id, review_id, disposition, exclusion_reason_code, \
         rationale, adjudicator_subject_id, adjudicator_display_name, supersedes_adjudication_id, \
         decided_at FROM screening_adjudications WHERE conflict_id = $1 \
         ORDER BY decided_at, adjudication_id",
    )
    .bind(conflict_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading adjudications", error))?;
    Ok(adjudication_tail(&rows).cloned())
}

fn resolution_summary(row: &ResolutionRow, applied_version_id: Option<Uuid>) -> Value {
    json!({
        "applied_version_id": opt_uuid_value(applied_version_id),
        "resolution_id": uuid_value(row.resolution_id),
        "cluster_id": uuid_value(row.cluster_id),
        "resolution_type": row.resolution_type,
        "supersedes_resolution_id": opt_uuid_value(row.supersedes_resolution_id),
        "survivor_record_id": opt_uuid_value(row.survivor_record_id),
        "rationale": row.rationale,
        "decided_by_subject_id": row.decided_by_subject_id,
        "decided_at": timestamp(&row.decided_at),
    })
}

fn adjudication_summary(row: &AdjudicationRow) -> Value {
    json!({
        "adjudication_id": uuid_value(row.adjudication_id),
        "conflict_id": uuid_value(row.conflict_id),
        "disposition": row.disposition,
        "exclusion_reason_code": opt_string_value(&row.exclusion_reason_code),
        "rationale": row.rationale,
        "adjudicator_subject_id": row.adjudicator_subject_id,
        "adjudicator_display_name": opt_string_value(&row.adjudicator_display_name),
        "supersedes_adjudication_id": opt_uuid_value(row.supersedes_adjudication_id),
        "decided_at": timestamp(&row.decided_at),
    })
}

// ---------------------------------------------------------------------------
// Merged records, current adjudications and unresolved conflicts
// ---------------------------------------------------------------------------

async fn merged_groups(tx: &mut Tx<'_>, review_id: Uuid) -> Result<Vec<MergedGroup>, Problem> {
    let cluster_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT cluster_id FROM duplicate_clusters WHERE review_id = $1 ORDER BY cluster_id",
    )
    .bind(review_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading duplicate clusters", error))?;
    if cluster_ids.is_empty() {
        return Ok(Vec::new());
    }

    let resolutions = sqlx::query_as::<_, ResolutionRow>(
        "SELECT resolution_id, cluster_id, review_id, resolution_type, supersedes_resolution_id, \
         survivor_record_id, rationale, decided_by_subject_id, decided_at \
         FROM duplicate_resolutions WHERE cluster_id = ANY($1)",
    )
    .bind(&cluster_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading duplicate resolutions", error))?;
    let mut by_cluster: HashMap<Uuid, Vec<ResolutionRow>> = HashMap::new();
    for row in resolutions {
        by_cluster.entry(row.cluster_id).or_default().push(row);
    }
    let mut tails: HashMap<Uuid, ResolutionRow> = HashMap::new();
    for (cluster_id, rows) in by_cluster {
        if let Some(tail) = resolution_tail(&rows) {
            tails.insert(cluster_id, tail.clone());
        }
    }

    let members = sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT cluster_id, review_record_id FROM duplicate_cluster_members WHERE cluster_id = ANY($1)",
    )
    .bind(&cluster_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading duplicate cluster members", error))?;
    let mut by_cluster_members: HashMap<Uuid, BTreeSet<Uuid>> = HashMap::new();
    for (cluster_id, record_id) in members {
        by_cluster_members
            .entry(cluster_id)
            .or_default()
            .insert(record_id);
    }

    let mut groups = Vec::new();
    for cluster_id in cluster_ids {
        let Some(resolution) = tails.get(&cluster_id) else {
            continue;
        };
        if resolution.resolution_type != "merge" {
            continue;
        }
        let Some(survivor) = resolution.survivor_record_id else {
            continue;
        };
        groups.push(MergedGroup {
            cluster_id,
            members: by_cluster_members
                .get(&cluster_id)
                .cloned()
                .unwrap_or_default(),
            survivor,
        });
    }
    Ok(groups)
}

/// Records whose CURRENT duplicate resolution is a merge that dropped them.
async fn records_removed_by_a_merge(
    tx: &mut Tx<'_>,
    review_id: Uuid,
) -> Result<HashSet<Uuid>, Problem> {
    let mut removed = HashSet::new();
    for group in merged_groups(tx, review_id).await? {
        for member in group.members {
            if member != group.survivor {
                removed.insert(member);
            }
        }
    }
    Ok(removed)
}

/// Current adjudication per conflict id, in one read.
async fn current_adjudications(
    tx: &mut Tx<'_>,
    conflict_ids: &[Uuid],
) -> Result<HashMap<Uuid, AdjudicationRow>, Problem> {
    if conflict_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query_as::<_, AdjudicationRow>(
        "SELECT adjudication_id, conflict_id, review_id, disposition, exclusion_reason_code, \
         rationale, adjudicator_subject_id, adjudicator_display_name, supersedes_adjudication_id, \
         decided_at FROM screening_adjudications WHERE conflict_id = ANY($1) \
         ORDER BY decided_at, adjudication_id",
    )
    .bind(conflict_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading adjudications", error))?;
    let mut by_conflict: HashMap<Uuid, Vec<AdjudicationRow>> = HashMap::new();
    for row in rows {
        by_conflict.entry(row.conflict_id).or_default().push(row);
    }
    Ok(by_conflict
        .into_iter()
        .filter_map(|(conflict_id, rows)| {
            adjudication_tail(&rows).map(|tail| (conflict_id, tail.clone()))
        })
        .collect())
}

fn agreement_key(
    disposition: &str,
    exclusion_reason_code: Option<&str>,
) -> (String, Option<String>) {
    (
        disposition.to_owned(),
        if disposition == "exclude" {
            exclusion_reason_code.map(str::to_owned)
        } else {
            None
        },
    )
}

/// Conflicts that still need a lead: not adjudicated, and not settled by consensus.
async fn unresolved_conflict_ids(
    tx: &mut Tx<'_>,
    review_id: Uuid,
) -> Result<HashSet<Uuid>, Problem> {
    let conflicts = sqlx::query_as::<_, ConflictRow>(
        "SELECT conflict_id, stage_id, round_number, review_record_id, opened_at \
         FROM screening_conflicts WHERE review_id = $1 ORDER BY opened_at, conflict_id",
    )
    .bind(review_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading screening conflicts", error))?;
    if conflicts.is_empty() {
        return Ok(HashSet::new());
    }

    let protocol = active_protocol(tx, review_id).await?;
    let fallback = protocol.as_ref().map_or(2, |row| row.reviewers_required);
    let stages = stage_map(tx, review_id).await?;
    let required_at = |stage_id: Uuid| -> i32 {
        stages
            .get(&stage_id)
            .map(|stage| {
                if stage.reviewers_required > 0 {
                    stage.reviewers_required
                } else {
                    fallback
                }
            })
            .unwrap_or(fallback)
    };

    let conflict_ids: Vec<Uuid> = conflicts
        .iter()
        .map(|conflict| conflict.conflict_id)
        .collect();
    let adjudicated = current_adjudications(tx, &conflict_ids).await?;

    let votes = sqlx::query_as::<_, (Uuid, i32, Uuid, String, Option<String>)>(
        "SELECT d.stage_id, d.round_number, d.review_record_id, d.disposition, d.exclusion_reason_code \
         FROM screening_decisions d WHERE d.review_id = $1 \
           AND NOT EXISTS (SELECT 1 FROM screening_decisions s WHERE s.supersedes_decision_id = d.decision_id)",
    )
    .bind(review_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading screening decisions", error))?;
    #[allow(clippy::type_complexity)]
    let mut by_position: HashMap<(Uuid, i32, Uuid), Vec<(String, Option<String>)>> = HashMap::new();
    for (stage_id, round_number, record_id, disposition, reason) in votes {
        by_position
            .entry((stage_id, round_number, record_id))
            .or_default()
            .push((disposition, reason));
    }

    let mut unresolved = HashSet::new();
    for conflict in conflicts {
        if adjudicated.contains_key(&conflict.conflict_id) {
            continue;
        }
        let cast = by_position
            .get(&(
                conflict.stage_id,
                conflict.round_number,
                conflict.review_record_id,
            ))
            .cloned()
            .unwrap_or_default();
        let required = required_at(conflict.stage_id);
        let distinct: HashSet<(String, Option<String>)> = cast
            .iter()
            .map(|(disposition, reason)| agreement_key(disposition, reason.as_deref()))
            .collect();
        if cast.len() as i32 >= required && distinct.len() == 1 {
            continue;
        }
        unresolved.insert(conflict.conflict_id);
    }
    Ok(unresolved)
}

// ---------------------------------------------------------------------------
// Flow derivation, limited to what screening needs
// ---------------------------------------------------------------------------

async fn stage_map(tx: &mut Tx<'_>, review_id: Uuid) -> Result<HashMap<Uuid, StageRow>, Problem> {
    let rows = sqlx::query_as::<_, StageRow>(
        "SELECT stage_id, stage_order, stage_key, name, reviewers_required \
         FROM review_stages WHERE review_id = $1 ORDER BY stage_order, stage_key",
    )
    .bind(review_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading review stages", error))?;
    Ok(rows.into_iter().map(|row| (row.stage_id, row)).collect())
}

/// Record ids whose settled bucket is `included` at the stage whose order is given —
/// `create_assignments`' scope for the next stage.
async fn records_settled_included_at(
    tx: &mut Tx<'_>,
    review_id: Uuid,
    stage_order: i32,
) -> Result<HashSet<Uuid>, Problem> {
    let record_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT review_record_id FROM review_record_versions WHERE review_id = $1",
    )
    .bind(review_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading review records", error))?;
    if record_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let stages = stage_map(tx, review_id).await?;
    let protocol = active_protocol(tx, review_id).await?;
    let fallback = protocol.as_ref().map_or(2, |row| row.reviewers_required);

    let decisions = sqlx::query_as::<_, (Uuid, i32, Uuid, String, Option<String>, DateTime<Utc>)>(
        "SELECT d.stage_id, d.round_number, d.review_record_id, d.disposition, \
         d.exclusion_reason_code, d.decided_at FROM screening_decisions d \
         WHERE d.review_id = $1 \
           AND NOT EXISTS (SELECT 1 FROM screening_decisions s WHERE s.supersedes_decision_id = d.decision_id)",
    )
    .bind(review_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading screening decisions", error))?;

    let adjudications = sqlx::query_as::<_, (Uuid, i32, Uuid, String, Option<String>)>(
        "SELECT c.stage_id, c.round_number, c.review_record_id, a.disposition, a.exclusion_reason_code \
         FROM screening_conflicts c JOIN screening_adjudications a ON a.conflict_id = c.conflict_id \
         WHERE c.review_id = $1 \
           AND NOT EXISTS (SELECT 1 FROM screening_adjudications s \
                           WHERE s.supersedes_adjudication_id = a.adjudication_id)",
    )
    .bind(review_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading adjudications", error))?;

    let removed = records_removed_by_a_merge(tx, review_id).await?;

    // position -> votes
    #[allow(clippy::type_complexity)]
    let mut votes: HashMap<(StagePosition, Uuid), Vec<(String, Option<String>)>> = HashMap::new();
    let mut worked_at: HashMap<(StagePosition, Uuid), DateTime<Utc>> = HashMap::new();
    let mut reached: HashMap<Uuid, BTreeSet<StagePosition>> = HashMap::new();
    for (stage_id, round_number, record_id, disposition, reason, decided_at) in decisions {
        let Some(stage) = stages.get(&stage_id) else {
            continue;
        };
        let position = StagePosition {
            stage_order: stage.stage_order,
            stage_key: stage.stage_key.clone(),
            round_number,
        };
        reached
            .entry(record_id)
            .or_default()
            .insert(position.clone());
        votes
            .entry((position.clone(), record_id))
            .or_default()
            .push((disposition, reason));
        let entry = worked_at.entry((position, record_id)).or_insert(decided_at);
        if decided_at > *entry {
            *entry = decided_at;
        }
    }
    let mut adjudicated: HashMap<(StagePosition, Uuid), (String, Option<String>)> = HashMap::new();
    for (stage_id, round_number, record_id, disposition, reason) in adjudications {
        let Some(stage) = stages.get(&stage_id) else {
            continue;
        };
        let position = StagePosition {
            stage_order: stage.stage_order,
            stage_key: stage.stage_key.clone(),
            round_number,
        };
        reached
            .entry(record_id)
            .or_default()
            .insert(position.clone());
        // Adjudications deliberately contribute no time — see the reference's `_collect`.
        adjudicated.insert((position, record_id), (disposition, reason));
    }

    let required_for = |position: &StagePosition| -> i32 {
        stages
            .values()
            .find(|stage| {
                stage.stage_order == position.stage_order && stage.stage_key == position.stage_key
            })
            .map(|stage| {
                if stage.reviewers_required > 0 {
                    stage.reviewers_required
                } else {
                    fallback
                }
            })
            .unwrap_or(fallback)
    };

    let mut included = HashSet::new();
    for record_id in record_ids {
        if removed.contains(&record_id) {
            continue;
        }
        let Some(positions) = reached.get(&record_id) else {
            continue;
        };
        let timed: BTreeMap<&StagePosition, DateTime<Utc>> = positions
            .iter()
            .filter_map(|position| {
                worked_at
                    .get(&(position.clone(), record_id))
                    .map(|when| (position, *when))
            })
            .collect();
        let settling = if timed.is_empty() {
            positions.iter().max().cloned()
        } else if timed.len() < positions.len() {
            None // partially timed: unattributable, hence not settled
        } else {
            timed
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then_with(|| a.0.cmp(b.0)))
                .map(|(position, _)| (*position).clone())
        };
        let Some(position) = settling else {
            continue;
        };
        let bucket = if let Some((disposition, _)) = adjudicated.get(&(position.clone(), record_id))
        {
            Some(disposition.clone())
        } else {
            let cast = votes
                .get(&(position.clone(), record_id))
                .cloned()
                .unwrap_or_default();
            let distinct: HashSet<(String, Option<String>)> = cast
                .iter()
                .map(|(disposition, reason)| agreement_key(disposition, reason.as_deref()))
                .collect();
            if cast.len() as i32 >= required_for(&position) && distinct.len() == 1 {
                cast.first().map(|(disposition, _)| disposition.clone())
            } else {
                None
            }
        };
        if bucket.as_deref() == Some("include") && position.stage_order == stage_order {
            included.insert(record_id);
        }
    }
    Ok(included)
}

/// Which of `reviewer_subject_ids` still owe a decision somewhere in the given stages.
async fn reviewers_owing_any_of(
    tx: &mut Tx<'_>,
    review_id: Uuid,
    stage_ids: &[Uuid],
    reviewer_subject_ids: &[String],
) -> Result<HashSet<String>, Problem> {
    if stage_ids.is_empty() || reviewer_subject_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let assigned = sqlx::query_as::<_, (String, Uuid, i32, Uuid)>(
        "SELECT reviewer_subject_id, stage_id, round_number, review_record_id \
         FROM screening_assignments WHERE review_id = $1 AND stage_id = ANY($2) \
           AND reviewer_subject_id = ANY($3)",
    )
    .bind(review_id)
    .bind(stage_ids)
    .bind(reviewer_subject_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading screening assignments", error))?;
    if assigned.is_empty() {
        return Ok(HashSet::new());
    }
    let decided = sqlx::query_as::<_, (String, Uuid, i32, Uuid)>(
        "SELECT reviewer_subject_id, stage_id, round_number, review_record_id \
         FROM screening_decisions WHERE review_id = $1 AND stage_id = ANY($2) \
           AND reviewer_subject_id = ANY($3) AND supersedes_decision_id IS NULL",
    )
    .bind(review_id)
    .bind(stage_ids)
    .bind(reviewer_subject_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading screening decisions", error))?;
    let assigned_set: HashSet<(String, Uuid, i32, Uuid)> = assigned.into_iter().collect();
    let decided_set: HashSet<(String, Uuid, i32, Uuid)> = decided.into_iter().collect();
    Ok(assigned_set
        .difference(&decided_set)
        .map(|position| position.0.clone())
        .collect())
}

// ---------------------------------------------------------------------------
// Blinded reads (L2/L3/L4)
// ---------------------------------------------------------------------------

/// `(stage_id, round_number, review_record_id)` this subject still owes a decision on.
///
/// Per POSITION, not per record: one decision anywhere in the review does not discharge the
/// debt at another stage or another round.
async fn records_the_caller_still_owes(
    tx: &mut Tx<'_>,
    review_id: Uuid,
    actor: &Actor,
) -> Result<HashSet<(Uuid, i32, Uuid)>, Problem> {
    let subject = actor.actor_subject();
    if subject.is_empty() {
        return Ok(HashSet::new());
    }
    let assigned = sqlx::query_as::<_, (Uuid, i32, Uuid)>(
        "SELECT stage_id, round_number, review_record_id FROM screening_assignments \
         WHERE review_id = $1 AND reviewer_subject_id = $2",
    )
    .bind(review_id)
    .bind(subject)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the caller's assignments", error))?;
    if assigned.is_empty() {
        return Ok(HashSet::new());
    }
    let decided = sqlx::query_as::<_, (Uuid, i32, Uuid)>(
        "SELECT stage_id, round_number, review_record_id FROM screening_decisions \
         WHERE review_id = $1 AND reviewer_subject_id = $2 AND supersedes_decision_id IS NULL",
    )
    .bind(review_id)
    .bind(subject)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the caller's decisions", error))?;
    let assigned_set: HashSet<(Uuid, i32, Uuid)> = assigned.into_iter().collect();
    let decided_set: HashSet<(Uuid, i32, Uuid)> = decided.into_iter().collect();
    Ok(assigned_set.difference(&decided_set).copied().collect())
}

fn decision_public(row: &DecisionRow, attribute: bool) -> Value {
    let mut body = json!({
        "decision_id": uuid_value(row.decision_id),
        "review_record_id": uuid_value(row.review_record_id),
        "review_record_version_id": uuid_value(row.review_record_version_id),
        "stage_id": uuid_value(row.stage_id),
        "round_number": row.round_number,
        "disposition": row.disposition,
        "exclusion_reason_code": opt_string_value(&row.exclusion_reason_code),
        "rationale": opt_string_value(&row.rationale),
        "decided_at": timestamp(&row.decided_at),
        "supersedes_decision_id": opt_uuid_value(row.supersedes_decision_id),
    });
    if attribute && let Some(object) = body.as_object_mut() {
        object.insert(
            "reviewer_subject_id".to_owned(),
            Value::String(row.reviewer_subject_id.clone()),
        );
    }
    body
}

const CURRENT_DECISION_PREDICATE: &str = "NOT EXISTS (SELECT 1 FROM screening_decisions s WHERE s.supersedes_decision_id = d.decision_id)";

async fn visible_decisions(
    tx: &mut Tx<'_>,
    review_id: Uuid,
    actor: &Actor,
    stage_id: Option<Uuid>,
    round_number: Option<i32>,
    review_record_id: Option<Uuid>,
    still_owed: &HashSet<(Uuid, i32, Uuid)>,
) -> Result<Vec<DecisionRow>, Problem> {
    let sql = format!(
        "SELECT d.decision_id, d.review_record_id, d.review_record_version_id, d.stage_id, \
         d.round_number, d.disposition, d.exclusion_reason_code, d.rationale, d.decided_at, \
         d.supersedes_decision_id, d.reviewer_subject_id FROM screening_decisions d \
         WHERE d.review_id = $1 AND ($2::uuid IS NULL OR d.stage_id = $2) \
           AND ($3::integer IS NULL OR d.round_number = $3) \
           AND ($4::uuid IS NULL OR d.review_record_id = $4) AND {CURRENT_DECISION_PREDICATE} \
         ORDER BY d.decided_at, d.decision_id"
    );
    let rows = sqlx::query_as::<_, DecisionRow>(AssertSqlSafe(sql))
        .bind(review_id)
        .bind(stage_id)
        .bind(round_number)
        .bind(review_record_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(|error| Problem::from_db("reading screening decisions", error))?;
    let subject = actor.actor_subject();
    Ok(rows
        .into_iter()
        .filter(|row| {
            let position = (row.stage_id, row.round_number, row.review_record_id);
            !still_owed.contains(&position) || row.reviewer_subject_id == subject
        })
        .collect())
}

/// The CALLER'S OWN current decision on one record, or `None`.
async fn own_decision_for(
    tx: &mut Tx<'_>,
    review_id: Uuid,
    actor: &Actor,
    stage_id: Uuid,
    round_number: i32,
    review_record_id: Uuid,
) -> Result<Option<DecisionRow>, Problem> {
    let subject = actor.actor_subject();
    if subject.is_empty() {
        return Ok(None);
    }
    let sql = format!(
        "SELECT d.decision_id, d.review_record_id, d.review_record_version_id, d.stage_id, \
         d.round_number, d.disposition, d.exclusion_reason_code, d.rationale, d.decided_at, \
         d.supersedes_decision_id, d.reviewer_subject_id FROM screening_decisions d \
         WHERE d.review_id = $1 AND d.stage_id = $2 AND d.round_number = $3 \
           AND d.review_record_id = $4 AND d.reviewer_subject_id = $5 \
           AND {CURRENT_DECISION_PREDICATE} ORDER BY d.decided_at DESC, d.decision_id DESC LIMIT 1"
    );
    sqlx::query_as::<_, DecisionRow>(AssertSqlSafe(sql))
        .bind(review_id)
        .bind(stage_id)
        .bind(round_number)
        .bind(review_record_id)
        .bind(subject)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|error| Problem::from_db("reading the caller's decision", error))
}

/// The caller's own current decisions for a page of records, in ONE statement.
async fn own_decisions_for(
    tx: &mut Tx<'_>,
    review_id: Uuid,
    actor: &Actor,
    stage_id: Uuid,
    round_number: i32,
    record_ids: &[Uuid],
) -> Result<HashMap<Uuid, DecisionRow>, Problem> {
    let subject = actor.actor_subject();
    if subject.is_empty() || record_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let sql = format!(
        "SELECT d.decision_id, d.review_record_id, d.review_record_version_id, d.stage_id, \
         d.round_number, d.disposition, d.exclusion_reason_code, d.rationale, d.decided_at, \
         d.supersedes_decision_id, d.reviewer_subject_id FROM screening_decisions d \
         WHERE d.review_id = $1 AND d.stage_id = $2 AND d.round_number = $3 \
           AND d.review_record_id = ANY($4) AND d.reviewer_subject_id = $5 \
           AND {CURRENT_DECISION_PREDICATE}"
    );
    let rows = sqlx::query_as::<_, DecisionRow>(AssertSqlSafe(sql))
        .bind(review_id)
        .bind(stage_id)
        .bind(round_number)
        .bind(record_ids)
        .bind(subject)
        .fetch_all(&mut **tx)
        .await
        .map_err(|error| Problem::from_db("reading the caller's decisions", error))?;
    Ok(rows
        .into_iter()
        .map(|row| (row.review_record_id, row))
        .collect())
}

/// `(assigned, decided)` for THIS caller in THIS stage and round — both stage-wide,
/// both excluding records a merge has removed, both from one statement.
async fn own_progress_counts(
    tx: &mut Tx<'_>,
    review_id: Uuid,
    actor: &Actor,
    stage_id: Uuid,
    round_number: i32,
    removed: &[Uuid],
) -> Result<(i64, i64), Problem> {
    let subject = actor.actor_subject();
    if subject.is_empty() {
        return Ok((0, 0));
    }
    let row = sqlx::query_as::<_, (i64, i64)>(
        "SELECT \
           (SELECT count(*) FROM screening_assignments a WHERE a.review_id = $1 \
              AND a.stage_id = $2 AND a.round_number = $3 AND a.reviewer_subject_id = $4 \
              AND a.review_record_id <> ALL($5)) AS assigned, \
           (SELECT count(*) FROM screening_decisions d WHERE d.review_id = $1 \
              AND d.stage_id = $2 AND d.round_number = $3 AND d.reviewer_subject_id = $4 \
              AND d.review_record_id <> ALL($5) AND d.supersedes_decision_id IS NULL) AS decided",
    )
    .bind(review_id)
    .bind(stage_id)
    .bind(round_number)
    .bind(subject)
    .bind(removed)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the caller's progress", error))?;
    Ok(row)
}

// ---------------------------------------------------------------------------
// Cursors (L4) — opaque, URL-safe, and over the caller's own assignments
// ---------------------------------------------------------------------------

const B64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn b64url_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let a = chunk[0] as u32;
        let b = chunk.get(1).copied().unwrap_or(0) as u32;
        let c = chunk.get(2).copied().unwrap_or(0) as u32;
        let packed = (a << 16) | (b << 8) | c;
        out.push(B64_ALPHABET[((packed >> 18) & 63) as usize] as char);
        out.push(B64_ALPHABET[((packed >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64_ALPHABET[((packed >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(B64_ALPHABET[(packed & 63) as usize] as char);
        }
    }
    out
}

fn b64url_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        } as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xFF) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn encode_cursor(assigned_at: DateTime<Utc>, assignment_id: Uuid) -> String {
    let raw = format!(
        "{}|{}",
        assigned_at.to_rfc3339_opts(SecondsFormat::Micros, false),
        assignment_id
    );
    b64url_encode(raw.as_bytes())
}

fn decode_cursor(cursor: &str) -> Result<(DateTime<Utc>, Uuid), Problem> {
    let bytes = b64url_decode(cursor).ok_or_else(|| unprocessable("malformed cursor"))?;
    let text = String::from_utf8(bytes).map_err(|_| unprocessable("malformed cursor"))?;
    let (raw, id) = text
        .split_once('|')
        .ok_or_else(|| unprocessable("malformed cursor"))?;
    let assigned_at = DateTime::parse_from_rfc3339(raw)
        .map_err(|_| unprocessable("malformed cursor"))?
        .with_timezone(&Utc);
    let assignment_id = Uuid::parse_str(id).map_err(|_| unprocessable("malformed cursor"))?;
    Ok((assigned_at, assignment_id))
}

// ---------------------------------------------------------------------------
// Query-string helpers
// ---------------------------------------------------------------------------

fn query_value<'a>(query: &'a Value, name: &str) -> Option<&'a str> {
    query.get(name).and_then(Value::as_str)
}

fn query_uuid(query: &Value, name: &str) -> Result<Option<Uuid>, Problem> {
    match query_value(query, name) {
        None => Ok(None),
        Some(raw) => Uuid::parse_str(raw)
            .map(Some)
            .map_err(|_| Problem::validation(format!("{name} must be a UUID"))),
    }
}

fn query_required_uuid(query: &Value, name: &str) -> Result<Uuid, Problem> {
    query_uuid(query, name)?.ok_or_else(|| Problem::validation(format!("{name} is required")))
}

fn query_int(query: &Value, name: &str, default: i64, min: i64, max: i64) -> Result<i64, Problem> {
    let value = match query_value(query, name) {
        None => default,
        Some(raw) => raw
            .parse::<i64>()
            .map_err(|_| Problem::validation(format!("{name} must be an integer")))?,
    };
    if value < min || value > max {
        return Err(Problem::validation(format!(
            "{name} must be between {min} and {max}"
        )));
    }
    Ok(value)
}

// ---------------------------------------------------------------------------
// Outbox
// ---------------------------------------------------------------------------

async fn emit_event(
    tx: &mut Tx<'_>,
    event_type: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    tenant_id: Uuid,
    payload: Value,
) -> Result<(), Problem> {
    sqlx::query(
        "INSERT INTO domain_outbox \
         (event_id, event_type, schema_ref, producer, resource_scope, tenant_id, aggregate_type, \
          aggregate_id, aggregate_version, payload) \
         VALUES ($1, $2, $3, $4, 'tenant', $5, $6, $7, 1, $8)",
    )
    .bind(new_id())
    .bind(event_type)
    .bind(ENVELOPE_SCHEMA_REF)
    .bind(OUTBOX_PRODUCER)
    .bind(tenant_id)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(payload)
    .execute(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("writing the domain outbox", error))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Normalization and survivorship
// ---------------------------------------------------------------------------

/// NFKC/casefold is approximated: lowercase, non-alphanumerics to spaces, whitespace
/// collapsed. The dependency that would carry true NFKC casefolding is not part of this
/// crate's manifest. The normalized columns remain a deterministic function of stored
/// bytes, which is the property the dedup baseline relies on.
fn normalize_text(value: Option<&str>) -> Option<String> {
    let value = value?;
    let mut folded = String::with_capacity(value.len());
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            folded.push(ch);
        } else if !folded.ends_with(' ') {
            folded.push(' ');
        }
    }
    let collapsed = folded.split_whitespace().collect::<Vec<_>>().join(" ");
    (!collapsed.is_empty()).then_some(collapsed)
}

fn normalize_doi(value: Option<&str>) -> Option<String> {
    let mut raw = value?.trim().to_lowercase();
    for prefix in [
        "https://doi.org/",
        "http://doi.org/",
        "https://dx.doi.org/",
        "http://dx.doi.org/",
        "doi:",
        "doi ",
    ] {
        if let Some(stripped) = raw.strip_prefix(prefix) {
            raw = stripped.trim().to_owned();
            break;
        }
    }
    (raw.starts_with("10.") && raw.contains('/')).then_some(raw)
}

fn normalize_pmid(value: Option<&str>) -> Option<String> {
    let raw = value?.trim().to_lowercase();
    let raw = raw
        .strip_prefix("pmid:")
        .or_else(|| raw.strip_prefix("pmid "))
        .or_else(|| raw.strip_prefix("pmid-"))
        .unwrap_or(&raw)
        .trim();
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    (!digits.is_empty()).then_some(digits)
}

fn content_of(version: &VersionRow) -> Map<String, Value> {
    let mut content = Map::new();
    for field in RECORD_CONTENT_FIELDS {
        content.insert(field.to_owned(), version_field(version, field));
    }
    content
}

fn version_field(version: &VersionRow, field: &str) -> Value {
    match field {
        "record_type" => Value::String(version.record_type.clone()),
        "title" => opt_string_value(&version.title),
        "abstract" => opt_string_value(&version.abstract_text),
        "publication_year" => version.publication_year.map_or(Value::Null, Value::from),
        "authors" => version.authors.clone().unwrap_or(Value::Null),
        "container_title" => opt_string_value(&version.container_title),
        "doi" => opt_string_value(&version.doi),
        "pmid" => opt_string_value(&version.pmid),
        "pmcid" => opt_string_value(&version.pmcid),
        "arxiv_id" => opt_string_value(&version.arxiv_id),
        "url" => opt_string_value(&version.url),
        _ => Value::Null,
    }
}

fn value_text(value: &Value) -> Option<String> {
    match value {
        Value::String(raw) => Some(raw.clone()),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

fn value_number(value: &Value) -> Option<i32> {
    value.as_i64().map(|number| number as i32)
}

/// Append the survivor's new version after a merge chose its fields, or `None` when the
/// choice changed nothing. `review_record_versions` is append-only, so "apply" is an INSERT.
#[allow(clippy::too_many_arguments)]
async fn apply_survivorship(
    tx: &mut Tx<'_>,
    actor: &Actor,
    review_id: Uuid,
    survivor_record_id: Uuid,
    sources: &BTreeMap<String, VersionRow>,
    resolution_id: Uuid,
    cluster_id: Uuid,
    subject: &str,
) -> Result<Option<Uuid>, Problem> {
    // Serialise two merges naming the same survivor; the loser re-reads the tail.
    sqlx::query(
        "SELECT review_record_id FROM review_records WHERE review_record_id = $1 FOR UPDATE",
    )
    .bind(survivor_record_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("locking the surviving record", error))?;

    let previous = sqlx::query_as::<_, VersionRow>(AssertSqlSafe(format!(
        "SELECT {VERSION_COLUMNS} FROM review_record_versions WHERE review_record_id = $1 \
         ORDER BY version_number DESC LIMIT 1"
    )))
    .bind(survivor_record_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the surviving record version", error))?
    .ok_or_else(|| unprocessable("the surviving record has no version to supersede"))?;

    let mut content = content_of(&previous);
    for (field, source) in sources {
        content.insert(field.clone(), version_field(source, field));
    }

    if content == content_of(&previous) {
        // The lead re-affirmed the survivor's own values: byte-identical content would make
        // "which version is current?" a distinction without a difference.
        return Ok(None);
    }

    let field_values: BTreeMap<String, String> = sources
        .iter()
        .map(|(field, source)| (field.clone(), source.review_record_version_id.to_string()))
        .collect();
    let provenance = json!({
        "normalized_title": "title",
        "normalized_doi": "doi",
        "normalized_pmid": "pmid",
        "source_row": previous.source_import_row_id.map(|id| id.to_string()),
        "derived_from": {
            "kind": "duplicate_merge_survivorship",
            "cluster_id": cluster_id.to_string(),
            "resolution_id": resolution_id.to_string(),
            "supersedes_version_id": previous.review_record_version_id.to_string(),
            "fields": field_values,
        },
    });

    let text = |field: &str| content.get(field).and_then(value_text);
    let number = content.get("publication_year").and_then(value_number);
    let authors = content
        .get("authors")
        .filter(|value| !value.is_null())
        .cloned();

    let version_id = new_id();
    sqlx::query(
        "INSERT INTO review_record_versions \
         (review_record_version_id, review_record_id, review_id, tenant_id, version_number, \
          supersedes_version_id, schema_ref, content_hash, record_type, title, abstract, \
          publication_year, authors, container_title, doi, pmid, pmcid, arxiv_id, url, \
          normalized_title, normalized_doi, normalized_pmid, field_provenance, \
          source_import_row_id, created_by_subject_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, \
                 $18, $19, $20, $21, $22, $23, $24, $25)",
    )
    .bind(version_id)
    .bind(survivor_record_id)
    .bind(previous.review_id)
    .bind(actor.tenant)
    .bind(previous.version_number + 1)
    .bind(previous.review_record_version_id)
    .bind(RECORD_SCHEMA_REF)
    .bind(hash_value(&Value::Object(content.clone())))
    .bind(text("record_type").unwrap_or_else(|| "other".to_owned()))
    .bind(text("title"))
    .bind(text("abstract"))
    .bind(number)
    .bind(authors)
    .bind(text("container_title"))
    .bind(text("doi"))
    .bind(text("pmid"))
    .bind(text("pmcid"))
    .bind(text("arxiv_id"))
    .bind(text("url"))
    .bind(normalize_text(text("title").as_deref()))
    .bind(normalize_doi(text("doi").as_deref()))
    .bind(normalize_pmid(text("pmid").as_deref()))
    .bind(provenance)
    .bind(previous.source_import_row_id)
    .bind(subject)
    .execute(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("appending the survivor version", error))?;
    let _ = review_id;
    Ok(Some(version_id))
}

// ---------------------------------------------------------------------------
// ER-003 — deduplication
// ---------------------------------------------------------------------------

fn run_summary(row: &DedupRunRow, replayed: bool) -> Value {
    json!({
        "dedup_run_id": uuid_value(row.dedup_run_id),
        "review_id": uuid_value(row.review_id),
        "corpus_version_id": uuid_value(row.corpus_version_id),
        "engine_id": row.engine_id,
        "engine_version": row.engine_version,
        "config": row.config,
        "input_digest": row.input_digest,
        "cluster_count": row.cluster_count,
        "pair_count": row.pair_count,
        "replayed": replayed,
        "created_at": timestamp(&row.created_at),
    })
}

const DEDUP_RUN_COLUMNS: &str = "dedup_run_id, review_id, corpus_version_id, engine_id, \
     engine_version, config, input_digest, cluster_count, pair_count, created_at";

/// `POST /api/reviews/{review_id}/deduplication-runs` — freeze a run against the sealed
/// corpus. **Lead only.** Same corpus, same engine and same config is the same run.
#[utoipa::path(post, path = "/api/reviews/{review_id}/deduplication-runs", operation_id = "create_deduplication_run", responses((status = 201, description = "ok")))]
pub async fn create_deduplication_run(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
    body: Option<Json<Value>>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let config = body
        .as_ref()
        .and_then(|Json(body)| body.get("config"))
        .filter(|value| !value.is_null())
        .cloned();
    if let Some(config) = &config {
        if !config.is_object() {
            return Err(Problem::validation("config must be a JSON object"));
        }
        guard_body(config, "the deduplication config")?;
    }

    let pool = pool(&state)?;
    let mut tx = crate::db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the review transaction", error))?;

    let review = load_review(&mut tx, review_id).await?;
    require_review_lead(&actor, &review, "starting a deduplication run").await?;
    require_review_open(&mut tx, review_id, "starting a deduplication run").await?;

    let corpus_version_id = latest_corpus(&mut tx, review_id).await?;
    let effective = effective_config(config.as_ref());
    let digest = input_digest(corpus_version_id, &effective);

    let existing = sqlx::query_as::<_, DedupRunRow>(AssertSqlSafe(format!(
        "SELECT {DEDUP_RUN_COLUMNS} FROM deduplication_runs \
         WHERE review_id = $1 AND input_digest = $2"
    )))
    .bind(review_id)
    .bind(&digest)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading the deduplication run", error))?;
    if let Some(existing) = existing {
        tx.commit()
            .await
            .map_err(|error| Problem::from_db("committing the deduplication run", error))?;
        return Ok((StatusCode::CREATED, Json(run_summary(&existing, true))));
    }

    let candidate_rows = sqlx::query_as::<
        _,
        (
            Uuid,
            Uuid,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<i32>,
        ),
    >(
        "SELECT rv.review_record_id, rv.review_record_version_id, rv.normalized_doi, \
         rv.normalized_pmid, rv.normalized_title, rv.publication_year \
         FROM review_record_versions rv JOIN corpus_version_members m \
           ON m.review_record_version_id = rv.review_record_version_id \
         WHERE m.corpus_version_id = $1 ORDER BY rv.review_record_id",
    )
    .bind(corpus_version_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading the sealed corpus membership", error))?;
    let candidates: Vec<Candidate> = candidate_rows
        .into_iter()
        .map(
            |(record_id, version_id, doi, pmid, title, year)| Candidate {
                record_id,
                version_id,
                normalized_doi: doi,
                normalized_pmid: pmid,
                normalized_title: title,
                publication_year: year,
                title: None,
                doi: None,
                pmid: None,
            },
        )
        .collect();
    let pairs = find_pairs(&candidates, &effective);
    let clusters = build_clusters(&pairs);
    let by_record: HashMap<Uuid, Uuid> = candidates
        .iter()
        .map(|candidate| (candidate.record_id, candidate.version_id))
        .collect();

    let engine_id = effective
        .get("engine_id")
        .and_then(Value::as_str)
        .unwrap_or(ENGINE_ID);
    let engine_version = effective
        .get("engine_version")
        .and_then(Value::as_str)
        .unwrap_or(ENGINE_VERSION);
    let run_id = new_id();
    let inserted = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO deduplication_runs \
         (dedup_run_id, review_id, tenant_id, corpus_version_id, engine_id, engine_version, \
          config, input_digest, cluster_count, pair_count, created_by_subject_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
         ON CONFLICT (review_id, input_digest) DO NOTHING RETURNING dedup_run_id",
    )
    .bind(run_id)
    .bind(review_id)
    .bind(actor.tenant)
    .bind(corpus_version_id)
    .bind(engine_id)
    .bind(engine_version)
    .bind(&effective)
    .bind(&digest)
    .bind(clusters.len() as i32)
    .bind(pairs.len() as i32)
    .bind(actor.actor_subject())
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("writing the deduplication run", error))?;

    let Some(written_run_id) = inserted else {
        // The check-then-insert race: the loser returns the winner's run.
        let winner = sqlx::query_as::<_, DedupRunRow>(AssertSqlSafe(format!(
            "SELECT {DEDUP_RUN_COLUMNS} FROM deduplication_runs \
             WHERE review_id = $1 AND input_digest = $2"
        )))
        .bind(review_id)
        .bind(&digest)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("reading the deduplication run", error))?
        .ok_or_else(|| unprocessable("the deduplication run disappeared while being written"))?;
        tx.commit()
            .await
            .map_err(|error| Problem::from_db("committing the deduplication run", error))?;
        return Ok((StatusCode::CREATED, Json(run_summary(&winner, true))));
    };

    for pair in &pairs {
        sqlx::query(
            "INSERT INTO duplicate_pair_scores \
             (pair_score_id, dedup_run_id, review_id, tenant_id, left_record_id, right_record_id, \
              matched_on, score) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(new_id())
        .bind(written_run_id)
        .bind(review_id)
        .bind(actor.tenant)
        .bind(pair.left)
        .bind(pair.right)
        .bind(pair.matched_on)
        .bind(EXACT_SCORE)
        .execute(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("writing duplicate pair scores", error))?;
    }
    for cluster in &clusters {
        let cluster_id = new_id();
        sqlx::query(
            "INSERT INTO duplicate_clusters \
             (cluster_id, dedup_run_id, review_id, tenant_id, cluster_key, member_count) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(cluster_id)
        .bind(written_run_id)
        .bind(review_id)
        .bind(actor.tenant)
        .bind(&cluster.cluster_key)
        .bind(cluster.members.len() as i32)
        .execute(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("writing duplicate clusters", error))?;
        for record_id in &cluster.members {
            sqlx::query(
                "INSERT INTO duplicate_cluster_members \
                 (cluster_id, review_record_id, review_id, tenant_id, review_record_version_id) \
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(cluster_id)
            .bind(record_id)
            .bind(review_id)
            .bind(actor.tenant)
            .bind(by_record.get(record_id).copied().unwrap_or_default())
            .execute(&mut *tx)
            .await
            .map_err(|error| Problem::from_db("writing duplicate cluster members", error))?;
        }
    }

    emit_event(
        &mut tx,
        "review.duplicate_cluster.proposed.v1",
        "deduplication_run",
        written_run_id,
        actor.tenant,
        json!({
            "review_id": review_id.to_string(),
            "dedup_run_id": written_run_id.to_string(),
            "corpus_version_id": corpus_version_id.to_string(),
            "cluster_count": clusters.len(),
            "pair_count": pairs.len(),
            "input_digest": digest,
        }),
    )
    .await?;

    let run = sqlx::query_as::<_, DedupRunRow>(AssertSqlSafe(format!(
        "SELECT {DEDUP_RUN_COLUMNS} FROM deduplication_runs WHERE dedup_run_id = $1"
    )))
    .bind(written_run_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading the deduplication run", error))?;

    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the deduplication run", error))?;
    Ok((StatusCode::CREATED, Json(run_summary(&run, false))))
}

/// `GET /api/reviews/{review_id}/deduplication-runs`
#[utoipa::path(get, path = "/api/reviews/{review_id}/deduplication-runs", operation_id = "list_deduplication_runs", responses((status = 200, description = "ok")))]
pub async fn list_deduplication_runs(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let pool = pool(&state)?;
    let mut tx = crate::db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the review transaction", error))?;
    load_review(&mut tx, review_id).await?;
    let rows = sqlx::query_as::<_, DedupRunRow>(AssertSqlSafe(format!(
        "SELECT {DEDUP_RUN_COLUMNS} FROM deduplication_runs WHERE review_id = $1 ORDER BY created_at"
    )))
    .bind(review_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading deduplication runs", error))?;
    let items: Vec<Value> = rows.iter().map(|row| run_summary(row, false)).collect();
    let body = json!({"items": items, "count": items.len()});
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the read", error))?;
    Ok((StatusCode::OK, Json(body)))
}

/// `GET /api/reviews/{review_id}/duplicate-clusters`
#[utoipa::path(get, path = "/api/reviews/{review_id}/duplicate-clusters", operation_id = "list_duplicate_clusters", responses((status = 200, description = "ok")))]
pub async fn list_duplicate_clusters(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
    Query(query): Query<Value>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let requested_run = query_uuid(&query, "dedup_run_id")?;
    let pool = pool(&state)?;
    let mut tx = crate::db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the review transaction", error))?;
    load_review(&mut tx, review_id).await?;

    let dedup_run_id = match requested_run {
        Some(run) => run,
        None => sqlx::query_scalar::<_, Uuid>(
            "SELECT dedup_run_id FROM deduplication_runs WHERE review_id = $1 \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(review_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("reading the deduplication run", error))?
        .ok_or_else(|| Problem::not_found("this review has no deduplication run"))?,
    };

    let clusters = sqlx::query_as::<_, (Uuid, String, i32)>(
        "SELECT cluster_id, cluster_key, member_count FROM duplicate_clusters \
         WHERE review_id = $1 AND dedup_run_id = $2 ORDER BY cluster_key",
    )
    .bind(review_id)
    .bind(dedup_run_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading duplicate clusters", error))?;

    let mut items = Vec::with_capacity(clusters.len());
    for (cluster_id, cluster_key, member_count) in clusters {
        let member_rows = sqlx::query_as::<_, (Uuid, Uuid, Option<String>, Option<String>, Option<String>, Option<i32>, Option<String>, Option<String>, Option<String>)>(
            "SELECT rv.review_record_id, rv.review_record_version_id, rv.normalized_doi, \
             rv.normalized_pmid, rv.normalized_title, rv.publication_year, rv.title, rv.doi, rv.pmid \
             FROM review_record_versions rv JOIN duplicate_cluster_members m \
               ON m.review_record_version_id = rv.review_record_version_id \
             WHERE m.cluster_id = $1 ORDER BY rv.review_record_id",
        )
        .bind(cluster_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("reading duplicate cluster members", error))?;

        let mut candidates = Vec::with_capacity(member_rows.len());
        let mut members = Vec::with_capacity(member_rows.len());
        for (
            record_id,
            version_id,
            normalized_doi,
            normalized_pmid,
            normalized_title,
            year,
            title,
            doi,
            pmid,
        ) in member_rows
        {
            candidates.push(Candidate {
                record_id,
                version_id,
                normalized_doi: normalized_doi.clone(),
                normalized_pmid: normalized_pmid.clone(),
                normalized_title,
                publication_year: year,
                title: title.clone(),
                doi: doi.clone(),
                pmid: pmid.clone(),
            });
            members.push(json!({
                "review_record_id": uuid_value(record_id),
                "review_record_version_id": uuid_value(version_id),
                "title": title,
                "doi": doi,
                "pmid": pmid,
                "normalized_doi": normalized_doi,
                "normalized_pmid": normalized_pmid,
                "publication_year": year,
            }));
        }
        let resolution = current_resolution(&mut tx, cluster_id).await?;
        items.push(json!({
            "cluster_id": uuid_value(cluster_id),
            "cluster_key": cluster_key,
            "member_count": member_count,
            "members": members,
            "canonical_presentation": canonical_presentation(&candidates),
            "current_resolution": resolution.as_ref().map(|row| resolution_summary(row, None)),
        }));
    }

    let body = json!({
        "dedup_run_id": uuid_value(dedup_run_id),
        "items": items,
        "count": items.len(),
    });
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the read", error))?;
    Ok((StatusCode::OK, Json(body)))
}

/// `POST /api/reviews/{review_id}/duplicate-clusters/{cluster_id}:resolve` — **Lead only.**
/// Reversal is a further row in the chain; the merge itself is never unwritten.
#[utoipa::path(post, path = "/api/reviews/{review_id}/duplicate-clusters/{cluster_id}:resolve", operation_id = "resolve_duplicate_cluster", responses((status = 200, description = "ok")))]
pub async fn resolve_duplicate_cluster(
    State(state): State<Shared>,
    actor: Actor,
    Path((review_id, cluster_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let body = body_object(&body)?;
    let resolution_type = one_of(
        &required_string(body, "resolution_type")?,
        "resolution_type",
        &["merge", "split", "no_match"],
    )?;
    let rationale = required_string(body, "rationale")?;
    let survivor_record_id = optional_uuid(body, "survivor_record_id")?;
    let survivorship = match body_value(body, "survivorship") {
        None => BTreeMap::new(),
        Some(Value::Object(map)) => {
            let mut parsed = BTreeMap::new();
            for (field, raw) in map {
                let raw = raw.as_str().ok_or_else(|| {
                    Problem::validation(format!("survivorship.{field} must be a UUID"))
                })?;
                let version_id = Uuid::parse_str(raw).map_err(|_| {
                    Problem::validation(format!("survivorship.{field} must be a UUID"))
                })?;
                parsed.insert(field.clone(), version_id);
            }
            parsed
        }
        Some(_) => return Err(Problem::validation("survivorship must be a JSON object")),
    };

    let pool = pool(&state)?;
    let mut tx = crate::db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the review transaction", error))?;
    let review = load_review(&mut tx, review_id).await?;
    let subject = require_review_lead(&actor, &review, "resolving a duplicate cluster").await?;
    require_review_open(&mut tx, review_id, "resolving a duplicate cluster").await?;

    let cluster_review: Option<Uuid> =
        sqlx::query_scalar("SELECT review_id FROM duplicate_clusters WHERE cluster_id = $1")
            .bind(cluster_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|error| Problem::from_db("reading the duplicate cluster", error))?;
    if cluster_review != Some(review_id) {
        return Err(Problem::not_found(
            "no such duplicate cluster for this review",
        ));
    }

    let member_rows = sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT review_record_id, review_record_version_id FROM duplicate_cluster_members \
         WHERE cluster_id = $1",
    )
    .bind(cluster_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading duplicate cluster members", error))?;
    let members: BTreeSet<Uuid> = member_rows.iter().map(|row| row.0).collect();
    let member_versions: HashSet<Uuid> = member_rows.iter().map(|row| row.1).collect();

    if resolution_type == "merge" {
        let survivor = survivor_record_id
            .ok_or_else(|| unprocessable("a merge must name the surviving record"))?;
        if !members.contains(&survivor) {
            return Err(unprocessable(
                "the survivor must be a member of the cluster being merged",
            ));
        }
        let mut proposed: Vec<MergedGroup> = merged_groups(&mut tx, review_id)
            .await?
            .into_iter()
            .filter(|group| group.cluster_id != cluster_id)
            .collect();
        proposed.push(MergedGroup {
            cluster_id,
            members: members.clone(),
            survivor,
        });
        if let Some((component, clusters)) =
            components_without_survivor(&proposed).into_iter().next()
        {
            let others: Vec<String> = clusters
                .iter()
                .filter(|id| **id != cluster_id)
                .map(Uuid::to_string)
                .collect();
            return Err(unprocessable(format!(
                "this merge would remove every record of a duplicate group, so the study would \
                 leave the review with no exclusion reason and no decision. Records: [{}]. \
                 Conflicting resolutions on clusters: [{}]. Split one of those clusters, or name \
                 the record they already keep as this survivor.",
                component
                    .iter()
                    .map(Uuid::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
                others.join(", "),
            )));
        }
    } else {
        if survivor_record_id.is_some() {
            return Err(unprocessable(format!(
                "a {resolution_type} resolution has no survivor"
            )));
        }
        if !survivorship.is_empty() {
            return Err(unprocessable(format!(
                "a {resolution_type} resolution has no survivor, so it cannot carry survivorship fields"
            )));
        }
    }

    let previous = current_resolution(&mut tx, cluster_id).await?;
    if let Some(previous) = &previous
        && previous.resolution_type == resolution_type
        && resolution_type != "merge"
    {
        return Err(Problem::conflict(format!(
            "cluster is already resolved as {resolution_type}"
        )));
    }

    let resolution_id = new_id();
    sqlx::query(
        "INSERT INTO duplicate_resolutions \
         (resolution_id, cluster_id, review_id, tenant_id, resolution_type, \
          supersedes_resolution_id, survivor_record_id, rationale, decided_by_subject_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(resolution_id)
    .bind(cluster_id)
    .bind(review_id)
    .bind(actor.tenant)
    .bind(&resolution_type)
    .bind(previous.as_ref().map(|row| row.resolution_id))
    .bind(survivor_record_id)
    .bind(&rationale)
    .bind(&subject)
    .execute(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("writing the duplicate resolution", error))?;

    let mut sources: BTreeMap<String, VersionRow> = BTreeMap::new();
    for (field_name, source_version_id) in &survivorship {
        let source = sqlx::query_as::<_, VersionRow>(AssertSqlSafe(format!(
            "SELECT {VERSION_COLUMNS} FROM review_record_versions \
             WHERE review_record_version_id = $1"
        )))
        .bind(source_version_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("reading the survivorship source version", error))?
        .ok_or_else(|| {
            unprocessable(format!(
                "survivorship field {field_name:?} names a record version outside this review"
            ))
        })?;
        if source.review_id != review_id {
            return Err(unprocessable(format!(
                "survivorship field {field_name:?} names a record version outside this review"
            )));
        }
        if !member_versions.contains(source_version_id) {
            return Err(unprocessable(format!(
                "survivorship field {field_name:?} names a record version that is not a member of \
                 this cluster; a survivor field is a choice between the values the reviewer was \
                 shown for these records"
            )));
        }
        if !SURVIVABLE_FIELDS.contains(&field_name.as_str()) {
            return Err(unprocessable(format!(
                "survivorship cannot choose {field_name:?}: a survivor field is a choice between \
                 values a reviewer was shown, and identity, tenancy and provenance are not among \
                 them"
            )));
        }
        sqlx::query(
            "INSERT INTO duplicate_survivorship_fields \
             (survivorship_field_id, resolution_id, review_id, tenant_id, field_name, value, \
              source_record_version_id) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(new_id())
        .bind(resolution_id)
        .bind(review_id)
        .bind(actor.tenant)
        .bind(field_name)
        .bind(json!({"value": version_field(&source, field_name)}))
        .bind(source_version_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("writing the survivorship field", error))?;
        sources.insert(field_name.clone(), source);
    }

    let applied = if !sources.is_empty() && survivor_record_id.is_some() {
        apply_survivorship(
            &mut tx,
            &actor,
            review_id,
            survivor_record_id.unwrap_or_default(),
            &sources,
            resolution_id,
            cluster_id,
            &subject,
        )
        .await?
    } else {
        None
    };

    let resolution = sqlx::query_as::<_, ResolutionRow>(
        "SELECT resolution_id, cluster_id, review_id, resolution_type, supersedes_resolution_id, \
         survivor_record_id, rationale, decided_by_subject_id, decided_at \
         FROM duplicate_resolutions WHERE resolution_id = $1",
    )
    .bind(resolution_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading the duplicate resolution", error))?;

    emit_event(
        &mut tx,
        "review.duplicate_cluster.resolved.v1",
        "duplicate_resolution",
        resolution_id,
        actor.tenant,
        json!({
            "review_id": review_id.to_string(),
            "resolution_id": resolution_id.to_string(),
            "cluster_id": cluster_id.to_string(),
            "resolution_type": resolution.resolution_type,
            "survivor_record_id": resolution.survivor_record_id.map(|id| id.to_string()),
            "supersedes_resolution_id": resolution.supersedes_resolution_id.map(|id| id.to_string()),
        }),
    )
    .await?;

    let body = resolution_summary(&resolution, applied);
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the resolution", error))?;
    Ok((StatusCode::OK, Json(body)))
}

/// `GET /api/reviews/{review_id}/duplicate-clusters/{cluster_id}/resolutions` — the full
/// append-only chain: the merge -> split -> merge evidence.
#[utoipa::path(get, path = "/api/reviews/{review_id}/duplicate-clusters/{cluster_id}/resolutions", operation_id = "get_resolution_chain", responses((status = 200, description = "ok")))]
pub async fn get_resolution_chain(
    State(state): State<Shared>,
    actor: Actor,
    Path((review_id, cluster_id)): Path<(Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let pool = pool(&state)?;
    let mut tx = crate::db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the review transaction", error))?;
    load_review(&mut tx, review_id).await?;

    let cluster_review: Option<Uuid> =
        sqlx::query_scalar("SELECT review_id FROM duplicate_clusters WHERE cluster_id = $1")
            .bind(cluster_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|error| Problem::from_db("reading the duplicate cluster", error))?;
    if cluster_review != Some(review_id) {
        return Err(Problem::not_found(
            "no such duplicate cluster for this review",
        ));
    }

    let rows = sqlx::query_as::<_, ResolutionRow>(
        "SELECT resolution_id, cluster_id, review_id, resolution_type, supersedes_resolution_id, \
         survivor_record_id, rationale, decided_by_subject_id, decided_at \
         FROM duplicate_resolutions WHERE cluster_id = $1 ORDER BY decided_at, resolution_id",
    )
    .bind(cluster_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading duplicate resolutions", error))?;

    let mut survivorship: HashMap<Uuid, Vec<Value>> = HashMap::new();
    if !rows.is_empty() {
        let resolution_ids: Vec<Uuid> = rows.iter().map(|row| row.resolution_id).collect();
        let fields = sqlx::query_as::<_, (Uuid, String, Option<Value>, Uuid)>(
            "SELECT resolution_id, field_name, value, source_record_version_id \
             FROM duplicate_survivorship_fields WHERE resolution_id = ANY($1) \
             ORDER BY resolution_id, field_name",
        )
        .bind(&resolution_ids)
        .fetch_all(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("reading survivorship fields", error))?;
        for (resolution_id, field_name, value, source) in fields {
            survivorship.entry(resolution_id).or_default().push(json!({
                "field_name": field_name,
                "value": value.as_ref().and_then(|v| v.get("value")).cloned().unwrap_or(Value::Null),
                "source_record_version_id": uuid_value(source),
            }));
        }
    }

    let mut items = Vec::with_capacity(rows.len());
    for row in &rows {
        let mut summary = resolution_summary(row, None);
        let fields = survivorship.remove(&row.resolution_id).unwrap_or_default();
        if let Some(object) = summary.as_object_mut() {
            object.insert("survivorship".to_owned(), Value::Array(fields));
        }
        items.push(summary);
    }
    let body = json!({
        "cluster_id": uuid_value(cluster_id),
        "items": items,
        "count": items.len(),
    });
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the read", error))?;
    Ok((StatusCode::OK, Json(body)))
}

// ---------------------------------------------------------------------------
// ER-004 — assignments
// ---------------------------------------------------------------------------

/// `POST /api/reviews/{review_id}/assignments` — assign the sealed corpus to every named
/// reviewer. **Lead only.** Stage order N > 1 scopes to records that settled `included` at
/// stage N-1; a reviewer who still owes N-1 is left out and named in the response.
#[utoipa::path(post, path = "/api/reviews/{review_id}/assignments", operation_id = "create_assignments", responses((status = 201, description = "ok")))]
pub async fn create_assignments(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let body = body_object(&body)?;
    let stage_id = required_uuid(body, "stage_id")?;
    let round_number = round_number(body)?;
    let corpus_version_id = optional_uuid(body, "corpus_version_id")?;
    let reviewer_subject_ids = match body_value(body, "reviewer_subject_ids") {
        Some(Value::Array(items)) => {
            if items.is_empty() || items.len() > MAX_REVIEWERS_PER_ASSIGNMENT {
                return Err(Problem::validation(format!(
                    "reviewer_subject_ids must name between 1 and {MAX_REVIEWERS_PER_ASSIGNMENT} reviewers"
                )));
            }
            let mut names = Vec::with_capacity(items.len());
            for item in items {
                let raw = item
                    .as_str()
                    .ok_or_else(|| Problem::validation("reviewer_subject_ids must be strings"))?;
                if raw.trim().is_empty() || raw.chars().count() > MAX_SUBJECT_ID_LENGTH {
                    return Err(Problem::validation(format!(
                        "each reviewer subject id must be non-blank and at most \
                         {MAX_SUBJECT_ID_LENGTH} characters"
                    )));
                }
                names.push(raw.to_owned());
            }
            names
        }
        _ => {
            return Err(Problem::validation(
                "reviewer_subject_ids is required and must be an array",
            ));
        }
    };

    let pool = pool(&state)?;
    let mut tx = crate::db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the review transaction", error))?;
    let review = load_review(&mut tx, review_id).await?;
    require_review_lead(&actor, &review, "assigning reviewers").await?;
    require_review_open(&mut tx, review_id, "assigning reviewers").await?;

    let stage = stage_row(&mut tx, review_id, stage_id).await?;
    let protocol = active_protocol(&mut tx, review_id).await?.ok_or_else(|| {
        unprocessable(
            "the review is not activated against a locked protocol; assignments would have \
                 no criteria",
        )
    })?;
    let required = if stage.reviewers_required > 0 {
        stage.reviewers_required
    } else {
        protocol.reviewers_required
    };
    let distinct_reviewers: BTreeSet<&String> = reviewer_subject_ids.iter().collect();
    if (distinct_reviewers.len() as i32) < required {
        return Err(unprocessable(format!(
            "this stage requires {required} reviewers; {} were named",
            distinct_reviewers.len()
        )));
    }

    let corpus_version_id = match corpus_version_id {
        Some(id) => {
            let corpus_review: Option<Uuid> = sqlx::query_scalar(
                "SELECT review_id FROM corpus_versions WHERE corpus_version_id = $1",
            )
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|error| Problem::from_db("reading the corpus version", error))?;
            if corpus_review != Some(review_id) {
                return Err(Problem::not_found("no such corpus version for this review"));
            }
            id
        }
        None => latest_corpus(&mut tx, review_id).await?,
    };

    // A round is bound to one corpus for its whole life: `uq_assignment_once` cannot be
    // re-pointed, and the insert below is ON CONFLICT DO NOTHING.
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(advisory_lock(&format!(
            "{review_id}:{stage_id}:{round_number}"
        )))
        .execute(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("locking the assignment round", error))?;
    let existing_rows = sqlx::query_as::<_, (Uuid, i64)>(
        "SELECT corpus_version_id, count(*) FROM screening_assignments \
         WHERE review_id = $1 AND stage_id = $2 AND round_number = $3 \
         GROUP BY corpus_version_id",
    )
    .bind(review_id)
    .bind(stage_id)
    .bind(round_number)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading the round's assignments", error))?;
    let existing: BTreeMap<Uuid, i64> = existing_rows.into_iter().collect();
    let mut foreign: Vec<String> = existing
        .keys()
        .filter(|id| **id != corpus_version_id)
        .map(Uuid::to_string)
        .collect();
    foreign.sort();
    if !foreign.is_empty() {
        let highest: Option<i32> = sqlx::query_scalar(
            "SELECT max(round_number) FROM screening_assignments \
             WHERE review_id = $1 AND stage_id = $2",
        )
        .bind(review_id)
        .bind(stage_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("reading the highest round", error))?
        .flatten();
        let next_round = highest.unwrap_or(0) + 1;
        return Err(Problem::conflict(format!(
            "round {round_number} of this stage is already screening corpus version {} \
             ({} assignments). Assignments are append-only, so this round cannot be re-pointed at \
             corpus version {corpus_version_id}; screen it as round {next_round} instead.",
            foreign.join(", "),
            existing.values().sum::<i64>(),
        ))
        .with(
            "bound_corpus_version_ids",
            Value::Array(foreign.iter().cloned().map(Value::String).collect()),
        )
        .with(
            "requested_corpus_version_id",
            Value::String(corpus_version_id.to_string()),
        )
        .with(
            "existing_assignments",
            json!(existing.values().sum::<i64>()),
        )
        .with("next_round_number", json!(next_round))
        .with("round_number", json!(round_number)));
    }

    let mut members: Vec<Uuid> = sqlx::query_scalar(
        "SELECT review_record_id FROM corpus_version_members WHERE corpus_version_id = $1",
    )
    .bind(corpus_version_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading the sealed corpus membership", error))?;
    if members.is_empty() {
        return Err(unprocessable("the sealed corpus has no members to assign"));
    }

    let mut owing: BTreeSet<String> = BTreeSet::new();
    if stage.stage_order > 1 {
        let eligible =
            records_settled_included_at(&mut tx, review_id, stage.stage_order - 1).await?;
        members.retain(|record_id| eligible.contains(record_id));

        let previous_stage_ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT stage_id FROM review_stages WHERE review_id = $1 AND stage_order = $2",
        )
        .bind(review_id)
        .bind(stage.stage_order - 1)
        .fetch_all(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("reading the previous stage", error))?;
        owing = reviewers_owing_any_of(
            &mut tx,
            review_id,
            &previous_stage_ids,
            &reviewer_subject_ids,
        )
        .await?
        .into_iter()
        .collect();
    }

    let mut reviewers: Vec<String> = reviewer_subject_ids
        .iter()
        .filter(|reviewer| !owing.contains(*reviewer))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    reviewers.sort();

    // One multi-VALUES statement per chunk, not one statement per (record, reviewer).
    let mut rows: Vec<(Uuid, String)> = Vec::with_capacity(members.len() * reviewers.len());
    for record_id in &members {
        for reviewer in &reviewers {
            rows.push((*record_id, reviewer.clone()));
        }
    }
    let mut created: i64 = 0;
    for chunk in rows.chunks(ASSIGNMENT_INSERT_CHUNK) {
        let mut sql = String::from(
            "INSERT INTO screening_assignments \
             (assignment_id, review_id, tenant_id, stage_id, round_number, review_record_id, \
              corpus_version_id, reviewer_subject_id, assigned_by_subject_id) VALUES ",
        );
        let mut values = Vec::with_capacity(chunk.len());
        for offset in 0..chunk.len() {
            let base = offset * 9;
            values.push(format!(
                "(${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${})",
                base + 1,
                base + 2,
                base + 3,
                base + 4,
                base + 5,
                base + 6,
                base + 7,
                base + 8,
                base + 9
            ));
        }
        sql.push_str(&values.join(", "));
        sql.push_str(
            " ON CONFLICT (review_id, stage_id, round_number, review_record_id, reviewer_subject_id) \
              DO NOTHING RETURNING assignment_id",
        );
        let mut query = sqlx::query_scalar::<_, Uuid>(AssertSqlSafe(sql));
        for (record_id, reviewer) in chunk {
            query = query
                .bind(new_id())
                .bind(review_id)
                .bind(actor.tenant)
                .bind(stage_id)
                .bind(round_number)
                .bind(record_id)
                .bind(corpus_version_id)
                .bind(reviewer)
                .bind(actor.actor_subject());
        }
        let inserted = query
            .fetch_all(&mut *tx)
            .await
            .map_err(|error| Problem::from_db("writing screening assignments", error))?;
        created += inserted.len() as i64;
    }

    let stored_corpus = existing.keys().next().copied().unwrap_or(corpus_version_id);
    let body = json!({
        "review_id": uuid_value(review_id),
        "stage_id": uuid_value(stage_id),
        "round_number": round_number,
        "corpus_version_id": uuid_value(stored_corpus),
        "reviewers": reviewers,
        "reviewers_excluded_owing_prior_stage": owing.into_iter().collect::<Vec<_>>(),
        "records": members.len(),
        "assignments_existing": existing.values().sum::<i64>(),
        "assignments_created": created,
    });
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the assignments", error))?;
    Ok((StatusCode::CREATED, Json(body)))
}

// ---------------------------------------------------------------------------
// ER-004 — the blinded queue (L3 + L4)
// ---------------------------------------------------------------------------

/// `GET /api/reviews/{review_id}/screening/queue`. Only the caller's own assignments and
/// their own decisions. `items`/`count`/`next_cursor` describe the PAGE; `total`,
/// `assigned_to_me` and `decided_by_me` describe the STAGE.
#[utoipa::path(get, path = "/api/reviews/{review_id}/screening/queue", operation_id = "get_screening_queue", responses((status = 200, description = "ok")))]
pub async fn screening_queue(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
    Query(query): Query<Value>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let stage_id = query_required_uuid(&query, "stage_id")?;
    let round_number = query_int(&query, "round_number", 1, 1, MAX_ROUND_NUMBER)? as i32;
    let limit = query_int(&query, "limit", 50, 1, 200)? as usize;
    let cursor = query_value(&query, "cursor").map(str::to_owned);

    let subject = human(&actor)?.to_owned();
    let pool = pool(&state)?;
    let mut tx = crate::db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the review transaction", error))?;
    load_review(&mut tx, review_id).await?;
    stage_row(&mut tx, review_id, stage_id).await?;

    let removed: Vec<Uuid> = records_removed_by_a_merge(&mut tx, review_id)
        .await?
        .into_iter()
        .collect();

    let mut sql = String::from(
        "SELECT assignment_id, review_record_id, corpus_version_id, assigned_at, reviewer_subject_id \
         FROM screening_assignments \
         WHERE review_id = $1 AND stage_id = $2 AND round_number = $3 AND reviewer_subject_id = $4 \
           AND review_record_id <> ALL($5)",
    );
    let mut next_index = 6;
    let decoded = match cursor {
        Some(cursor) => {
            let (assigned_at, assignment_id) = decode_cursor(&cursor)?;
            sql.push_str(&format!(
                " AND (assigned_at, assignment_id) > (${next_index}, ${})",
                next_index + 1
            ));
            next_index += 2;
            Some((assigned_at, assignment_id))
        }
        None => None,
    };
    sql.push_str(" ORDER BY assigned_at, assignment_id LIMIT $");
    sql.push_str(&next_index.to_string());

    let mut query = sqlx::query_as::<_, AssignmentRow>(AssertSqlSafe(sql))
        .bind(review_id)
        .bind(stage_id)
        .bind(round_number)
        .bind(&subject)
        .bind(&removed);
    if let Some((assigned_at, assignment_id)) = decoded {
        query = query.bind(assigned_at).bind(assignment_id);
    }
    let mut rows = query
        .bind(limit as i64 + 1)
        .fetch_all(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("reading the screening queue", error))?;

    let has_more = rows.len() > limit;
    rows.truncate(limit);

    let page_record_ids: Vec<Uuid> = rows.iter().map(|row| row.review_record_id).collect();
    let mut by_corpus: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for row in &rows {
        by_corpus
            .entry(row.corpus_version_id)
            .or_default()
            .push(row.review_record_id);
    }
    let mut versions: HashMap<Uuid, VersionRow> = HashMap::new();
    for (corpus_version_id, record_ids) in by_corpus {
        versions.extend(corpus_member_versions(&mut tx, corpus_version_id, &record_ids).await?);
    }
    let own_decisions = own_decisions_for(
        &mut tx,
        review_id,
        &actor,
        stage_id,
        round_number,
        &page_record_ids,
    )
    .await?;

    let attribute = actor.is_lead();
    let mut items = Vec::with_capacity(rows.len());
    for row in &rows {
        let version = versions.get(&row.review_record_id);
        let own = own_decisions.get(&row.review_record_id);
        items.push(json!({
            "assignment_id": uuid_value(row.assignment_id),
            "review_record_id": uuid_value(row.review_record_id),
            "review_record_version_id": version.map_or(Value::Null, |v| uuid_value(v.review_record_version_id)),
            "title": version.map_or(Value::Null, |v| opt_string_value(&v.title)),
            "abstract": version.map_or(Value::Null, |v| opt_string_value(&v.abstract_text)),
            "publication_year": version.map_or(Value::Null, |v| v.publication_year.map_or(Value::Null, Value::from)),
            "authors": version.map_or(Value::Null, |v| v.authors.clone().unwrap_or(Value::Null)),
            "container_title": version.map_or(Value::Null, |v| opt_string_value(&v.container_title)),
            "doi": version.map_or(Value::Null, |v| opt_string_value(&v.doi)),
            "record_type": version.map_or(Value::Null, |v| Value::String(v.record_type.clone())),
            "my_decision": own.map_or(Value::Null, |row| decision_public(row, attribute)),
        }));
    }

    let (assigned_to_me, decided_by_me) =
        own_progress_counts(&mut tx, review_id, &actor, stage_id, round_number, &removed).await?;

    let next_cursor = if has_more {
        rows.last()
            .map(|row| encode_cursor(row.assigned_at, row.assignment_id))
    } else {
        None
    };
    let body = json!({
        "review_id": uuid_value(review_id),
        "stage_id": uuid_value(stage_id),
        "round_number": round_number,
        "items": items,
        "count": items.len(),
        "total": assigned_to_me,
        "assigned_to_me": assigned_to_me,
        "decided_by_me": decided_by_me,
        "next_cursor": next_cursor,
    });
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the read", error))?;
    Ok((StatusCode::OK, Json(body)))
}

// ---------------------------------------------------------------------------
// ER-004 — decisions
// ---------------------------------------------------------------------------

/// Open exactly one conflict per disagreement, in the same transaction.
async fn maybe_open_conflict(
    tx: &mut Tx<'_>,
    actor: &Actor,
    review_id: Uuid,
    stage_id: Uuid,
    round_number: i32,
    review_record_id: Uuid,
    reviewers_required: i32,
) -> Result<Option<Uuid>, Problem> {
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(advisory_lock(&format!(
            "{review_id}:{stage_id}:{round_number}:{review_record_id}"
        )))
        .execute(&mut **tx)
        .await
        .map_err(|error| Problem::from_db("locking the conflict position", error))?;

    let previous_role = elevate(tx).await?;
    let rows_result = sqlx::query_as::<_, (Uuid, String, Option<String>)>(
        "SELECT d.review_record_id, d.disposition, d.exclusion_reason_code \
         FROM screening_decisions d WHERE d.review_id = $1 AND d.stage_id = $2 \
           AND d.round_number = $3 AND d.review_record_id = $4 \
           AND NOT EXISTS (SELECT 1 FROM screening_decisions s \
                           WHERE s.supersedes_decision_id = d.decision_id)",
    )
    .bind(review_id)
    .bind(stage_id)
    .bind(round_number)
    .bind(review_record_id)
    .fetch_all(&mut **tx)
    .await;
    restore_role(tx, &previous_role).await?;
    let rows =
        rows_result.map_err(|error| Problem::from_db("counting the current decisions", error))?;

    if (rows.len() as i32) < reviewers_required {
        return Ok(None);
    }
    let distinct: HashSet<(String, Option<String>)> = rows
        .iter()
        .map(|(_, disposition, reason)| agreement_key(disposition, reason.as_deref()))
        .collect();
    if distinct.len() == 1 {
        return Ok(None); // agreement opens no conflict
    }

    let inserted = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO screening_conflicts \
         (conflict_id, review_id, tenant_id, stage_id, round_number, review_record_id) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         ON CONFLICT (review_id, stage_id, round_number, review_record_id) DO NOTHING \
         RETURNING conflict_id",
    )
    .bind(new_id())
    .bind(review_id)
    .bind(actor.tenant)
    .bind(stage_id)
    .bind(round_number)
    .bind(review_record_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("opening the screening conflict", error))?;

    if let Some(conflict_id) = inserted {
        emit_event(
            tx,
            "review.conflict.opened.v1",
            "screening_conflict",
            conflict_id,
            actor.tenant,
            json!({
                "review_id": review_id.to_string(),
                "conflict_id": conflict_id.to_string(),
                "stage_id": stage_id.to_string(),
                "round_number": round_number,
            }),
        )
        .await?;
        return Ok(Some(conflict_id));
    }
    // The conflict already existed; announcing it again would tell a consumer a second one
    // opened on the same record.
    sqlx::query_scalar::<_, Uuid>(
        "SELECT conflict_id FROM screening_conflicts \
         WHERE review_id = $1 AND stage_id = $2 AND round_number = $3 AND review_record_id = $4",
    )
    .bind(review_id)
    .bind(stage_id)
    .bind(round_number)
    .bind(review_record_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the screening conflict", error))
}

/// `POST /api/reviews/{review_id}/decisions` — an append-only decision, attributed to the
/// authenticated human. There is no reviewer field in the body, deliberately.
#[utoipa::path(post, path = "/api/reviews/{review_id}/decisions", operation_id = "record_decision", responses((status = 201, description = "ok")))]
pub async fn record_decision(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let body = body_object(&body)?;
    let stage_id = required_uuid(body, "stage_id")?;
    let round_number = round_number(body)?;
    let review_record_id = required_uuid(body, "review_record_id")?;
    let disposition = one_of(
        &required_string(body, "disposition")?,
        "disposition",
        &["include", "exclude", "uncertain"],
    )?;
    let exclusion_reason_code = optional_string(body, "exclusion_reason_code")?;
    let rationale = optional_string(body, "rationale")?;
    let supersedes_decision_id = optional_uuid(body, "supersedes_decision_id")?;

    let subject = human(&actor)?.to_owned();
    let pool = pool(&state)?;
    let mut tx = crate::db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the review transaction", error))?;
    load_review(&mut tx, review_id).await?;
    require_review_open(&mut tx, review_id, "recording a decision").await?;
    let stage = stage_row(&mut tx, review_id, stage_id).await?;

    let assignment = sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT assignment_id, corpus_version_id FROM screening_assignments \
         WHERE review_id = $1 AND stage_id = $2 AND round_number = $3 \
           AND review_record_id = $4 AND reviewer_subject_id = $5",
    )
    .bind(review_id)
    .bind(stage_id)
    .bind(round_number)
    .bind(review_record_id)
    .bind(&subject)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading the assignment", error))?;
    // L5: 404, and the record identifier is NOT echoed — a 403 would confirm the record
    // exists in this review, which is itself the disclosure.
    let (assignment_id, corpus_version_id) =
        assignment.ok_or_else(|| Problem::not_found("no such assignment"))?;

    // A verdict that provably cannot land is refused, not accepted and discarded.
    if records_removed_by_a_merge(&mut tx, review_id)
        .await?
        .contains(&review_record_id)
    {
        return Err(unprocessable(
            "this record was removed from the review by a duplicate merge, so a decision on it \
             cannot be counted: the flow ledger buckets it as duplicate_removed whatever the \
             verdict says. Screen the surviving record of the cluster instead, or ask the lead to \
             split the merge if it was wrong.",
        )
        .with(
            "review_record_id",
            Value::String(review_record_id.to_string()),
        ));
    }

    let protocol = active_protocol(&mut tx, review_id)
        .await?
        .ok_or_else(|| unprocessable("the review is not activated against a locked protocol"))?;
    if disposition == "exclude"
        && !vocabulary_contains(
            &protocol.exclusion_reason_vocabulary,
            exclusion_reason_code.as_deref().unwrap_or(""),
        )
    {
        return Err(unprocessable(
            "exclusion reason is not in the locked protocol's closed vocabulary",
        )
        .with("allowed", protocol.exclusion_reason_vocabulary.clone()));
    }

    // THROUGH the assignment's corpus, never the review's tail.
    let version = corpus_member_versions(&mut tx, corpus_version_id, &[review_record_id])
        .await?
        .remove(&review_record_id)
        .ok_or_else(|| {
            unprocessable(
                "this record is not a member of the corpus version this assignment was minted \
                 from, so there is no version of it that you were shown to attribute a decision to",
            )
            .with(
                "corpus_version_id",
                Value::String(corpus_version_id.to_string()),
            )
        })?;

    let previous = own_decision_for(
        &mut tx,
        review_id,
        &actor,
        stage_id,
        round_number,
        review_record_id,
    )
    .await?;
    match (&previous, supersedes_decision_id) {
        (Some(previous), None) => {
            return Err(Problem::conflict(
                "you have already decided this record; a correction must supersede the previous \
                 decision",
            )
            .with(
                "current_decision_id",
                Value::String(previous.decision_id.to_string()),
            ));
        }
        (None, Some(_)) | (Some(_), Some(_))
            if previous.as_ref().map(|row| row.decision_id) != supersedes_decision_id =>
        {
            return Err(Problem::conflict(
                "the decision being superseded is not your current decision",
            ));
        }
        _ => {}
    }

    let decision_id = new_id();
    let decided_at: DateTime<Utc> = sqlx::query_scalar(
        "INSERT INTO screening_decisions \
         (decision_id, review_id, tenant_id, stage_id, round_number, review_record_id, \
          review_record_version_id, assignment_id, reviewer_subject_id, disposition, \
          exclusion_reason_code, rationale, supersedes_decision_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) RETURNING decided_at",
    )
    .bind(decision_id)
    .bind(review_id)
    .bind(actor.tenant)
    .bind(stage_id)
    .bind(round_number)
    .bind(review_record_id)
    .bind(version.review_record_version_id)
    .bind(assignment_id)
    .bind(&subject)
    .bind(&disposition)
    .bind(&exclusion_reason_code)
    .bind(&rationale)
    .bind(supersedes_decision_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("writing the screening decision", error))?;

    let reviewers_required = if stage.reviewers_required > 0 {
        stage.reviewers_required
    } else {
        protocol.reviewers_required
    };
    maybe_open_conflict(
        &mut tx,
        &actor,
        review_id,
        stage_id,
        round_number,
        review_record_id,
        reviewers_required,
    )
    .await?;

    // ⛔ Carries NO disposition and NO reviewer: the outbox has no blinding layer.
    emit_event(
        &mut tx,
        "review.decision.recorded.v1",
        "screening_decision",
        decision_id,
        actor.tenant,
        json!({
            "review_id": review_id.to_string(),
            "decision_id": decision_id.to_string(),
            "stage_id": stage_id.to_string(),
            "round_number": round_number,
        }),
    )
    .await?;

    let body = json!({
        "decision_id": uuid_value(decision_id),
        "review_record_id": uuid_value(review_record_id),
        "review_record_version_id": uuid_value(version.review_record_version_id),
        "disposition": disposition,
        "exclusion_reason_code": exclusion_reason_code,
        "supersedes_decision_id": opt_uuid_value(supersedes_decision_id),
        "decided_at": timestamp(&decided_at),
    });
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the decision", error))?;
    Ok((StatusCode::CREATED, Json(body)))
}

/// `GET /api/reviews/{review_id}/decisions` — every decision the CALLER may see.
///
/// `items`/`count`/`offset`/`has_more` describe the PAGE; `total` describes the review.
/// The page is cut in Rust after the read, because the L2 debt filter is applied to the
/// rows after they come back.
#[utoipa::path(get, path = "/api/reviews/{review_id}/decisions", operation_id = "list_visible_decisions", responses((status = 200, description = "ok")))]
pub async fn list_decisions(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
    Query(query): Query<Value>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let review_record_id = query_uuid(&query, "review_record_id")?;
    let limit = query_int(
        &query,
        "limit",
        DEFAULT_DECISIONS_PAGE,
        1,
        MAX_DECISIONS_PAGE,
    )? as usize;
    let offset = query_int(&query, "offset", 0, 0, i64::MAX)? as usize;

    let pool = pool(&state)?;
    let mut tx = crate::db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the review transaction", error))?;
    load_review(&mut tx, review_id).await?;

    let still_owed = records_the_caller_still_owes(&mut tx, review_id, &actor).await?;
    let visible = visible_decisions(
        &mut tx,
        review_id,
        &actor,
        None,
        None,
        review_record_id,
        &still_owed,
    )
    .await?;

    let total = visible.len();
    let attribute = actor.is_lead();
    let page: Vec<Value> = visible
        .iter()
        .skip(offset)
        .take(limit)
        .map(|row| decision_public(row, attribute))
        .collect();
    let body = json!({
        "items": page,
        "count": page.len(),
        "total": total,
        "offset": offset,
        "has_more": offset + page.len() < total,
    });
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the read", error))?;
    Ok((StatusCode::OK, Json(body)))
}

// ---------------------------------------------------------------------------
// ER-004 — conflicts, unblinding and adjudication
// ---------------------------------------------------------------------------

/// `GET /api/reviews/{review_id}/conflicts` — **lead only**, with dispositions redacted
/// until an unblinding. A conflict at a position the caller still owes is withheld from the
/// rows AND the counters, unless an unblinding named the caller.
#[utoipa::path(get, path = "/api/reviews/{review_id}/conflicts", operation_id = "list_conflicts", responses((status = 200, description = "ok")))]
pub async fn list_conflicts(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
    Query(query): Query<Value>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let limit = query_int(&query, "limit", 200, 1, 500)? as usize;

    let pool = pool(&state)?;
    let mut tx = crate::db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the review transaction", error))?;
    let review = load_review(&mut tx, review_id).await?;
    require_review_lead(&actor, &review, "listing conflicts").await?;

    let still_owed = records_the_caller_still_owes(&mut tx, review_id, &actor).await?;

    let all_conflicts = sqlx::query_as::<_, ConflictRow>(
        "SELECT conflict_id, stage_id, round_number, review_record_id, opened_at \
         FROM screening_conflicts WHERE review_id = $1 ORDER BY opened_at, conflict_id",
    )
    .bind(review_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading screening conflicts", error))?;

    let audience_rows = sqlx::query_as::<_, (Uuid, Vec<String>)>(
        "SELECT conflict_id, disclosed_to_subject_ids FROM screening_unblinding_events \
         WHERE review_id = $1",
    )
    .bind(review_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading unblinding events", error))?;
    let mut audiences: HashMap<Uuid, HashSet<String>> = HashMap::new();
    for (conflict_id, disclosed_to) in audience_rows {
        audiences
            .entry(conflict_id)
            .or_default()
            .extend(disclosed_to);
    }
    let subject = actor.actor_subject();
    let unblinded_to_caller: HashSet<Uuid> = audiences
        .iter()
        .filter(|(_, audience)| !subject.is_empty() && audience.contains(subject))
        .map(|(conflict_id, _)| *conflict_id)
        .collect();

    // THE ONE PLACE THE OWED FILTER IS APPLIED. Everything downstream — the page, `total`,
    // `total_open`, `has_more` — is computed from `visible`.
    let visible: Vec<&ConflictRow> = all_conflicts
        .iter()
        .filter(|conflict| {
            let position = (
                conflict.stage_id,
                conflict.round_number,
                conflict.review_record_id,
            );
            !still_owed.contains(&position) || unblinded_to_caller.contains(&conflict.conflict_id)
        })
        .collect();
    let owed_hidden = all_conflicts.len() - visible.len();
    let total_conflicts = visible.len();
    let has_more = visible.len() > limit;
    let page: Vec<&ConflictRow> = visible.iter().take(limit).copied().collect();

    let unresolved = unresolved_conflict_ids(&mut tx, review_id).await?;
    let total_open = page
        .iter()
        .filter(|conflict| unresolved.contains(&conflict.conflict_id))
        .count();
    let total_open_review = visible
        .iter()
        .filter(|conflict| unresolved.contains(&conflict.conflict_id))
        .count();

    let conflict_ids: Vec<Uuid> = page.iter().map(|conflict| conflict.conflict_id).collect();
    let adjudications = current_adjudications(&mut tx, &conflict_ids).await?;
    let record_ids: Vec<Uuid> = page
        .iter()
        .map(|conflict| conflict.review_record_id)
        .collect();
    let versions = current_record_versions(&mut tx, review_id, &record_ids).await?;

    let attribute = actor.is_lead();
    let mut items = Vec::with_capacity(page.len());
    for conflict in &page {
        let position = (
            conflict.stage_id,
            conflict.round_number,
            conflict.review_record_id,
        );
        let unblinded = audiences.contains_key(&conflict.conflict_id);
        let owes_this_conflict = still_owed.contains(&position);
        let adjudication = adjudications.get(&conflict.conflict_id);
        let version = versions.get(&conflict.review_record_id);

        let dispositions = if unblinded && !owes_this_conflict {
            let decisions = visible_decisions(
                &mut tx,
                review_id,
                &actor,
                Some(conflict.stage_id),
                Some(conflict.round_number),
                Some(conflict.review_record_id),
                &still_owed,
            )
            .await?;
            Value::Array(
                decisions
                    .iter()
                    .map(|row| decision_public(row, attribute))
                    .collect(),
            )
        } else {
            Value::String(REDACTED.to_owned())
        };

        let adjudication_value = match adjudication {
            Some(row) if !owes_this_conflict => adjudication_summary(row),
            Some(_) => Value::String(REDACTED.to_owned()),
            None => Value::Null,
        };

        items.push(json!({
            "conflict_id": uuid_value(conflict.conflict_id),
            "review_record_id": uuid_value(conflict.review_record_id),
            "stage_id": uuid_value(conflict.stage_id),
            "round_number": conflict.round_number,
            "title": version.map_or(Value::Null, |v| opt_string_value(&v.title)),
            "opened_at": timestamp(&conflict.opened_at),
            "unblinded": unblinded,
            "dispositions": dispositions,
            "resolved": !unresolved.contains(&conflict.conflict_id),
            "adjudication": adjudication_value,
        }));
    }

    let body = json!({
        "items": items,
        "count": items.len(),
        "open": total_open,
        "has_more": has_more,
        "total": total_conflicts,
        "total_open": total_open_review,
        "owed_hidden": owed_hidden,
    });
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the read", error))?;
    Ok((StatusCode::OK, Json(body)))
}

/// `POST /api/reviews/{review_id}/conflicts/{conflict_id}:unblind` — the ONLY door through
/// the blinding policy's third clause, and it leaves an attributable row.
#[utoipa::path(post, path = "/api/reviews/{review_id}/conflicts/{conflict_id}:unblind", operation_id = "unblind_conflict", responses((status = 200, description = "ok")))]
pub async fn unblind_conflict(
    State(state): State<Shared>,
    actor: Actor,
    Path((review_id, conflict_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let body = body_object(&body)?;
    let reason = required_string(body, "reason")?;
    let named = match body_value(body, "disclose_to") {
        None => Vec::new(),
        Some(Value::Array(items)) => {
            let mut names = Vec::with_capacity(items.len());
            for item in items {
                let raw = item.as_str().ok_or_else(|| {
                    Problem::validation("disclose_to must be an array of strings")
                })?;
                names.push(raw.trim().to_owned());
            }
            names
        }
        Some(_) => {
            return Err(Problem::validation(
                "disclose_to must be an array of strings",
            ));
        }
    };
    if named.iter().any(String::is_empty) {
        return Err(unprocessable(
            "a disclosure audience cannot contain an empty subject id: an empty string matches \
             every caller that presents no subject, which discloses the record to anyone in the \
             tenant",
        ));
    }

    let pool = pool(&state)?;
    let mut tx = crate::db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the review transaction", error))?;
    let review = load_review(&mut tx, review_id).await?;
    let subject = require_review_lead(&actor, &review, "unblinding a conflict").await?;
    require_review_open(&mut tx, review_id, "unblinding a conflict").await?;

    let conflict = sqlx::query_as::<_, ConflictRow>(
        "SELECT conflict_id, stage_id, round_number, review_record_id, opened_at \
         FROM screening_conflicts WHERE conflict_id = $1 AND review_id = $2",
    )
    .bind(conflict_id)
    .bind(review_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading the screening conflict", error))?
    .ok_or_else(|| Problem::not_found("no such conflict for this review"))?;

    // YOU MAY NOT UNBLIND WHAT YOU MAY NOT SEE.
    if records_the_caller_still_owes(&mut tx, review_id, &actor)
        .await?
        .contains(&(
            conflict.stage_id,
            conflict.round_number,
            conflict.review_record_id,
        ))
    {
        return Err(Problem::conflict(
            "you are assigned to this record at this stage and have not recorded your own \
             decision, so unblinding it would let you read the dispositions it is currently \
             withholding from you. Record your own decision first, or ask a lead who is not \
             screening this record to unblind it.",
        ));
    }

    let mut audience: BTreeSet<String> = named.into_iter().collect();
    audience.insert(subject.clone());
    let audience: Vec<String> = audience.into_iter().collect();

    let event_id = new_id();
    let occurred_at: DateTime<Utc> = sqlx::query_scalar(
        "INSERT INTO screening_unblinding_events \
         (unblinding_event_id, conflict_id, review_id, tenant_id, actor_subject_id, reason, \
          disclosed_to_subject_ids) VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING occurred_at",
    )
    .bind(event_id)
    .bind(conflict_id)
    .bind(review_id)
    .bind(actor.tenant)
    .bind(&subject)
    .bind(&reason)
    .bind(&audience)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("writing the unblinding event", error))?;

    let body = json!({
        "unblinding_event_id": uuid_value(event_id),
        "conflict_id": uuid_value(conflict_id),
        "actor_subject_id": subject,
        "reason": reason,
        "disclosed_to_subject_ids": audience,
        "occurred_at": timestamp(&occurred_at),
    });
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the unblinding", error))?;
    Ok((StatusCode::OK, Json(body)))
}

/// `POST /api/reviews/{review_id}/conflicts/{conflict_id}:resolve` — attributable
/// adjudication under separation of duty (the trigger is the backstop).
#[utoipa::path(post, path = "/api/reviews/{review_id}/conflicts/{conflict_id}:resolve", operation_id = "adjudicate_conflict", responses((status = 200, description = "ok")))]
pub async fn adjudicate_conflict(
    State(state): State<Shared>,
    actor: Actor,
    Path((review_id, conflict_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let body = body_object(&body)?;
    let disposition = one_of(
        &required_string(body, "disposition")?,
        "disposition",
        &["include", "exclude", "uncertain"],
    )?;
    let rationale = required_string(body, "rationale")?;
    let exclusion_reason_code = optional_string(body, "exclusion_reason_code")?;
    let supersedes_adjudication_id = optional_uuid(body, "supersedes_adjudication_id")?;

    let pool = pool(&state)?;
    let mut tx = crate::db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the review transaction", error))?;
    let review = load_review(&mut tx, review_id).await?;
    let subject = require_review_lead(&actor, &review, "adjudicating a conflict").await?;
    require_review_open(&mut tx, review_id, "adjudicating a conflict").await?;

    let conflict = sqlx::query_as::<_, ConflictRow>(
        "SELECT conflict_id, stage_id, round_number, review_record_id, opened_at \
         FROM screening_conflicts WHERE conflict_id = $1 AND review_id = $2",
    )
    .bind(conflict_id)
    .bind(review_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading the screening conflict", error))?
    .ok_or_else(|| Problem::not_found("no such conflict for this review"))?;

    // YOU MAY NOT RULE ON WHAT YOU MAY NOT SEE.
    if records_the_caller_still_owes(&mut tx, review_id, &actor)
        .await?
        .contains(&(
            conflict.stage_id,
            conflict.round_number,
            conflict.review_record_id,
        ))
    {
        return Err(Problem::conflict(
            "you are assigned to this record at this stage and have not recorded your own \
             decision, so the dispositions you would be ruling on are withheld from you. \
             Recording one does NOT open this door — separation of duty then refuses you as an \
             adjudicator who took part. Either a lead who is not screening this record \
             adjudicates it, or the reviewers reach consensus by superseding their own decisions, \
             which needs no lead at all.",
        ));
    }

    if disposition == "exclude"
        && let Some(protocol) = active_protocol(&mut tx, review_id).await?
        && !vocabulary_contains(
            &protocol.exclusion_reason_vocabulary,
            exclusion_reason_code.as_deref().unwrap_or(""),
        )
    {
        return Err(unprocessable(
            "exclusion reason is not in the locked protocol's closed vocabulary",
        )
        .with("allowed", protocol.exclusion_reason_vocabulary.clone()));
    }

    let existing = current_adjudication(&mut tx, conflict_id).await?;
    match (&existing, supersedes_adjudication_id) {
        (Some(existing), None) => {
            return Err(Problem::conflict(
                "this conflict is already adjudicated; a revision must supersede the previous \
                 adjudication",
            )
            .with(
                "current_adjudication_id",
                Value::String(existing.adjudication_id.to_string()),
            ));
        }
        (None, Some(_)) | (Some(_), Some(_))
            if existing.as_ref().map(|row| row.adjudication_id) != supersedes_adjudication_id =>
        {
            return Err(Problem::conflict(
                "the adjudication being superseded is not the current one",
            ));
        }
        _ => {}
    }

    let adjudication_id = new_id();
    sqlx::query(
        "INSERT INTO screening_adjudications \
         (adjudication_id, conflict_id, review_id, tenant_id, disposition, \
          exclusion_reason_code, rationale, adjudicator_subject_id, adjudicator_display_name, \
          supersedes_adjudication_id) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(adjudication_id)
    .bind(conflict_id)
    .bind(review_id)
    .bind(actor.tenant)
    .bind(&disposition)
    .bind(&exclusion_reason_code)
    .bind(&rationale)
    .bind(&subject)
    .bind(&actor.display_name)
    .bind(supersedes_adjudication_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("writing the adjudication", error))?;

    emit_event(
        &mut tx,
        "review.conflict.resolved.v1",
        "screening_adjudication",
        adjudication_id,
        actor.tenant,
        json!({
            "review_id": review_id.to_string(),
            "adjudication_id": adjudication_id.to_string(),
            "conflict_id": conflict_id.to_string(),
            "disposition": disposition,
            "supersedes_adjudication_id": supersedes_adjudication_id.map(|id| id.to_string()),
        }),
    )
    .await?;

    let adjudication = current_adjudication(&mut tx, conflict_id)
        .await?
        .ok_or_else(|| unprocessable("the adjudication disappeared while being written"))?;
    let body = adjudication_summary(&adjudication);
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the adjudication", error))?;
    Ok((StatusCode::OK, Json(body)))
}

/// `GET /api/reviews/{review_id}/stages`
#[utoipa::path(get, path = "/api/reviews/{review_id}/stages", operation_id = "list_stages", responses((status = 200, description = "ok")))]
pub async fn list_stages(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let pool = pool(&state)?;
    let mut tx = crate::db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the review transaction", error))?;
    load_review(&mut tx, review_id).await?;
    let rows = sqlx::query_as::<_, StageRow>(
        "SELECT stage_id, stage_order, stage_key, name, reviewers_required \
         FROM review_stages WHERE review_id = $1 ORDER BY stage_order, stage_key",
    )
    .bind(review_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading review stages", error))?;
    let items: Vec<Value> = rows
        .iter()
        .map(|stage| {
            json!({
                "stage_id": uuid_value(stage.stage_id),
                "stage_key": stage.stage_key,
                "stage_order": stage.stage_order,
                "name": stage.name,
                "reviewers_required": stage.reviewers_required,
            })
        })
        .collect();
    let body = json!({"items": items, "count": items.len()});
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the read", error))?;
    Ok((StatusCode::OK, Json(body)))
}

// ---------------------------------------------------------------------------
// L6 — the aggregate gate, used by flow and the export routes
// ---------------------------------------------------------------------------

/// Refuse a caller who still owes a screening decision where another reviewer has already
/// judged. A conserved aggregate cannot be partly suppressed: at a small corpus a bucket
/// total names the partner's verdict on the record this caller has yet to judge.
///
/// Fires only where there is something to disclose (another subject holds a CURRENT
/// decision at a position this caller still owes), not merely because the caller owes
/// something.
pub async fn require_no_owed_decisions(
    tx: &mut Tx<'_>,
    review_id: Uuid,
    actor: &Actor,
) -> Result<(), Problem> {
    let owed = records_the_caller_still_owes(tx, review_id, actor).await?;
    if owed.is_empty() {
        return Ok(());
    }
    // The question is about rows the caller's own binding hides, so read as a lead for the
    // duration of one statement and restore afterwards.
    let previous_role = elevate(tx).await?;
    let positions_result = sqlx::query_as::<_, (Uuid, i32, Uuid)>(
        "SELECT d.stage_id, d.round_number, d.review_record_id FROM screening_decisions d \
         WHERE d.review_id = $1 AND d.reviewer_subject_id <> $2 \
           AND NOT EXISTS (SELECT 1 FROM screening_decisions s \
                           WHERE s.supersedes_decision_id = d.decision_id)",
    )
    .bind(review_id)
    .bind(actor.actor_subject())
    .fetch_all(&mut **tx)
    .await;
    restore_role(tx, &previous_role).await?;
    let positions = positions_result
        .map_err(|error| Problem::from_db("reading the review's decisions", error))?;

    if positions.iter().any(|position| owed.contains(position)) {
        return Err(Problem::forbidden(
            "reporting every reviewer's work would leak a decision you still owe: another \
             reviewer has already judged a record in this review and your own verdict there is \
             outstanding, so a bucket total would name their verdict on the record you have yet \
             to judge. Record your own decisions first; the ledger opens as soon as you owe \
             nothing there.",
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(
        record_id: Uuid,
        doi: Option<&str>,
        title: Option<&str>,
        year: Option<i32>,
    ) -> Candidate {
        Candidate {
            record_id,
            version_id: Uuid::now_v7(),
            normalized_doi: doi.map(str::to_owned),
            normalized_pmid: None,
            normalized_title: title.map(str::to_owned),
            publication_year: year,
            title: title.map(str::to_owned),
            doi: doi.map(str::to_owned),
            pmid: None,
        }
    }

    #[test]
    fn the_engine_clusters_a_shared_doi_and_orders_pairs() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let candidates = vec![
            candidate(a, Some("10.1/x"), None, None),
            candidate(b, Some("10.1/x"), None, None),
        ];
        let pairs = find_pairs(&candidates, &default_config());
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].matched_on, "normalized_doi");
        assert!(pairs[0].left < pairs[0].right);
        let clusters = build_clusters(&pairs);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].members, vec![a, b]);
    }

    #[test]
    fn title_year_needs_both_and_cannot_match_null_years() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let candidates = vec![
            candidate(a, None, Some("same"), None),
            candidate(b, None, Some("same"), None),
        ];
        assert!(find_pairs(&candidates, &default_config()).is_empty());
        let candidates = vec![
            candidate(a, None, Some("same"), Some(2020)),
            candidate(b, None, Some("same"), Some(2021)),
        ];
        assert!(find_pairs(&candidates, &default_config()).is_empty());
    }

    #[test]
    fn a_merge_that_empties_a_component_is_reported() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let merges = vec![
            MergedGroup {
                cluster_id: Uuid::from_u128(10),
                members: BTreeSet::from([a, b]),
                survivor: a,
            },
            MergedGroup {
                cluster_id: Uuid::from_u128(11),
                members: BTreeSet::from([a, b]),
                survivor: b,
            },
        ];
        let emptied = components_without_survivor(&merges);
        assert_eq!(emptied.len(), 1);
        assert_eq!(emptied[0].0, BTreeSet::from([a, b]));
    }

    #[test]
    fn the_cursor_round_trips_and_is_url_safe() {
        let id = Uuid::now_v7();
        let when = DateTime::parse_from_rfc3339("2026-01-02T03:04:05.123456+00:00")
            .expect("timestamp")
            .with_timezone(&Utc);
        let cursor = encode_cursor(when, id);
        assert!(!cursor.contains(['+', '/', '=']));
        let (decoded_at, decoded_id) = decode_cursor(&cursor).expect("valid cursor");
        assert_eq!(decoded_at, when);
        assert_eq!(decoded_id, id);
    }

    #[test]
    fn a_query_string_deserializes_into_json() {
        let query = axum::extract::Query::<Value>::try_from_uri(
            &"/api/x?stage_id=11111111-1111-1111-1111-111111111111&limit=50"
                .parse()
                .expect("uri"),
        )
        .expect("query");
        assert_eq!(query.0.get("limit").and_then(Value::as_str), Some("50"));
        assert_eq!(
            query.0.get("stage_id").and_then(Value::as_str),
            Some("11111111-1111-1111-1111-111111111111")
        );
    }

    #[test]
    fn a_body_nesting_past_the_bound_is_refused() {
        let mut value = json!({});
        for _ in 0..MAX_BODY_DEPTH + 2 {
            value = json!({"a": value});
        }
        assert!(guard_body(&value, "config").is_err());
    }

    #[test]
    fn a_guarded_discriminator_is_refused_at_any_case() {
        assert!(guard_body(&json!({"workType": "x"}), "config").is_err());
        assert!(guard_body(&json!({"nested": [{"work_type": "x"}]}), "config").is_err());
        assert!(guard_body(&json!({"xref": "x"}), "config").is_ok());
        assert!(guard_body(&json!({"rules": ["normalized_doi"]}), "config").is_ok());
    }
}
