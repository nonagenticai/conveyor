//! The corpus family — `ER-002`: the sealed membership later screening and exports anchor to.
//!
//! Ported from the reference's `api/routers/imports.py` (the three `corpus-versions` routes) and
//! `application/services/imports.py::seal_corpus_version` + `domain/corpus.py`.
//!
//! The seal has two halves and this module is the first. Here the members, the count, the
//! ordering rule and the manifest hash are written in one transaction; the second half is the
//! database, where `trg_corpus_versions_verify_seal` and `trg_corpus_members_verify_seal`
//! (deferred to COMMIT) recompute the hash from the member rows and refuse the transaction when
//! it disagrees. `ER-V011` / `DM-AT-05` as a constraint rather than a test.
//!
//! **The manifest is not an RFC 8785 hash of a JSON array.** It is `sha256` over the newline-
//! joined member version ids in `record_version_id_asc` order — the formula
//! `corpus_versions_verify_seal` implements in SQL and `tests/evidence_review_schema.rs` pins
//! (an empty membership hashes the empty string). `hash_value` is used where a schema-declared
//! canonical JSON hash is what the contract asks for — a record's `content_hash` — not here;
//! using it here would make every seal fail its own deferred trigger at commit.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::actor::Actor;
use crate::evidence_review::imports::current_record_version_ids;
use crate::evidence_review::reviews::{load_review, require_review_lead_for, require_review_open};
use crate::evidence_review::{hash_bytes, new_id, pool};
use crate::problem::Problem;
use crate::state::Shared;

/// The only ordering rule this release implements. Recorded on every corpus version, because a
/// manifest hash without its ordering is not reproducible.
pub const ORDERING_RULE: &str = "record_version_id_asc";

/// `record_version_id_asc`: ascending lexicographic on the TEXT form, which is what
/// `ORDER BY review_record_version_id::text ASC` does in the trigger.
pub fn order_members<'a, I>(member_version_ids: I) -> Vec<String>
where
    I: IntoIterator<Item = &'a Uuid>,
{
    let mut ordered: Vec<String> = member_version_ids
        .into_iter()
        .map(Uuid::to_string)
        .collect();
    ordered.sort();
    ordered
}

/// `sha256:<hex>` over the newline-joined ordered member version ids. An empty membership hashes
/// the empty string rather than being a special case — the trigger's `coalesce(string_agg(…), '')`
/// does the same, and a special case is where the two implementations would first diverge.
pub fn member_manifest_hash<'a, I>(member_version_ids: I) -> String
where
    I: IntoIterator<Item = &'a Uuid>,
{
    hash_bytes(order_members(member_version_ids).join("\n").as_bytes())
}

#[derive(Debug, sqlx::FromRow)]
struct CorpusRow {
    corpus_version_id: Uuid,
    review_id: Uuid,
    version_number: i32,
    supersedes_version_id: Option<Uuid>,
    member_count: i32,
    member_ordering_rule: String,
    member_manifest_hash: String,
    created_at: DateTime<Utc>,
}

fn iso(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn corpus_summary(corpus: &CorpusRow, replayed: bool) -> Value {
    json!({
        "corpus_version_id": corpus.corpus_version_id.to_string(),
        "review_id": corpus.review_id.to_string(),
        "version_number": corpus.version_number,
        "member_count": corpus.member_count,
        "member_ordering_rule": corpus.member_ordering_rule,
        "member_manifest_hash": corpus.member_manifest_hash,
        "supersedes_version_id": corpus.supersedes_version_id.map(|id| id.to_string()),
        "replayed": replayed,
        "created_at": iso(corpus.created_at),
    })
}

async fn corpus_by_id(
    tx: &mut Transaction<'_, Postgres>,
    corpus_version_id: Uuid,
) -> Result<Option<CorpusRow>, Problem> {
    sqlx::query_as::<_, CorpusRow>(
        "SELECT corpus_version_id, review_id, version_number, supersedes_version_id,
                member_count, member_ordering_rule, member_manifest_hash, created_at
           FROM corpus_versions WHERE corpus_version_id = $1",
    )
    .bind(corpus_version_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| Problem::from_db("reading the corpus version", error))
}

/// Seal the current record set into a new immutable `CorpusVersion`. **Lead only.**
///
/// The authority check is needed independently rather than as symmetry: sealing takes no input,
/// so a non-lead who could only seal could still move the membership every subsequent assignment,
/// decision and export is anchored to.
#[utoipa::path(post, path = "/api/reviews/{review_id}/corpus-versions", operation_id = "seal_corpus_version", responses((status = 201, description = "ok")))]
pub async fn seal_corpus(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let pool = pool(&state)?;
    let mut tx = crate::db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the corpus transaction", error))?;
    // Sealing establishes provenance; a machine cannot do it.
    actor.require_human()?;
    let subject =
        require_review_lead_for(&mut tx, &actor, review_id, "sealing a corpus version").await?;
    require_review_open(&mut tx, review_id, "sealing a corpus version").await?;

    let version_ids = current_record_version_ids(&mut tx, review_id).await?;
    if version_ids.is_empty() {
        return Err(Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "unprocessable_state",
            "Unprocessable State",
            "this review has no records to seal",
        ));
    }

    let previous = sqlx::query_as::<_, CorpusRow>(
        "SELECT corpus_version_id, review_id, version_number, supersedes_version_id,
                member_count, member_ordering_rule, member_manifest_hash, created_at
           FROM corpus_versions
          WHERE review_id = $1 ORDER BY version_number DESC LIMIT 1",
    )
    .bind(review_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading the previous corpus version", error))?;

    let mut members: Vec<(String, Uuid)> = version_ids
        .iter()
        .map(|version_id| (version_id.to_string(), *version_id))
        .collect();
    members.sort_by(|left, right| left.0.cmp(&right.0));
    let manifest = member_manifest_hash(&version_ids);
    if let Some(previous) = previous.as_ref()
        && previous.member_manifest_hash == manifest
    {
        // Identical membership is the same corpus; a second version with the same manifest
        // would make "which corpus was this run against?" ambiguous for no gain.
        let summary = corpus_summary(previous, true);
        tx.commit()
            .await
            .map_err(|error| Problem::from_db("committing the corpus replay", error))?;
        return Ok((StatusCode::CREATED, Json(summary)));
    }

    let corpus_version_id = new_id();
    let (version_number, supersedes) = match previous.as_ref() {
        Some(previous) => (
            previous.version_number + 1,
            Some(previous.corpus_version_id),
        ),
        None => (1, None),
    };
    sqlx::query(
        "INSERT INTO corpus_versions
             (corpus_version_id, review_id, tenant_id, version_number, supersedes_version_id,
              member_count, member_ordering_rule, member_manifest_hash, created_by_subject_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
    )
    .bind(corpus_version_id)
    .bind(review_id)
    .bind(actor.tenant)
    .bind(version_number)
    .bind(supersedes)
    .bind(i32::try_from(members.len()).map_err(|_| Problem::validation("too many members"))?)
    .bind(ORDERING_RULE)
    .bind(&manifest)
    .bind(&subject)
    .execute(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("sealing the corpus version", error))?;

    for (ordinal, (_, version_id)) in members.iter().enumerate() {
        let record_id: Uuid = sqlx::query_scalar(
            "SELECT review_record_id FROM review_record_versions
              WHERE review_record_version_id = $1",
        )
        .bind(version_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("reading the member's record", error))?;
        sqlx::query(
            "INSERT INTO corpus_version_members
                 (corpus_version_id, review_id, tenant_id, review_record_id,
                  review_record_version_id, member_ordinal)
             VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .bind(corpus_version_id)
        .bind(review_id)
        .bind(actor.tenant)
        .bind(record_id)
        .bind(version_id)
        .bind(i32::try_from(ordinal + 1).map_err(|_| Problem::validation("too many members"))?)
        .execute(&mut *tx)
        .await
        .map_err(|error| Problem::from_db("sealing a corpus member", error))?;
    }

    let corpus = corpus_by_id(&mut tx, corpus_version_id)
        .await?
        .ok_or_else(|| {
            Problem::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "unprocessable_state",
                "Unprocessable State",
                "the corpus version disappeared while being written",
            )
        })?;
    // The deferred seal triggers fire here and refuse the transaction on disagreement.
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the sealed corpus version", error))?;
    Ok((StatusCode::CREATED, Json(corpus_summary(&corpus, false))))
}

#[utoipa::path(get, path = "/api/reviews/{review_id}/corpus-versions", operation_id = "list_corpus_versions", responses((status = 200, description = "ok")))]
pub async fn list_corpus_versions(
    State(state): State<Shared>,
    actor: Actor,
    Path(review_id): Path<Uuid>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let pool = pool(&state)?;
    let mut tx = crate::db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the corpus transaction", error))?;
    load_review(&mut tx, review_id).await?;
    let rows = sqlx::query_as::<_, CorpusRow>(
        "SELECT corpus_version_id, review_id, version_number, supersedes_version_id,
                member_count, member_ordering_rule, member_manifest_hash, created_at
           FROM corpus_versions WHERE review_id = $1 ORDER BY version_number",
    )
    .bind(review_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("listing the corpus versions", error))?;
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the corpus read", error))?;
    Ok((
        StatusCode::OK,
        Json(json!({
            "items": rows.iter().map(|corpus| corpus_summary(corpus, false)).collect::<Vec<_>>(),
            "count": rows.len(),
        })),
    ))
}

/// The sealed membership, in its canonical order, with the recomputed manifest hash so a caller
/// can verify the seal against the members it was actually given.
#[utoipa::path(get, path = "/api/reviews/{review_id}/corpus-versions/{corpus_version_id}/members", operation_id = "list_corpus_members", responses((status = 200, description = "ok")))]
pub async fn list_corpus_members(
    State(state): State<Shared>,
    actor: Actor,
    Path((review_id, corpus_version_id)): Path<(Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), Problem> {
    let pool = pool(&state)?;
    let mut tx = crate::db::begin(pool, &actor)
        .await
        .map_err(|error| Problem::from_db("opening the corpus transaction", error))?;
    load_review(&mut tx, review_id).await?;
    let Some(corpus) = corpus_by_id(&mut tx, corpus_version_id).await? else {
        return Err(Problem::not_found("no such corpus version for this review"));
    };
    if corpus.review_id != review_id {
        return Err(Problem::not_found("no such corpus version for this review"));
    }
    let rows = sqlx::query(
        "SELECT member_ordinal, review_record_id, review_record_version_id
           FROM corpus_version_members
          WHERE corpus_version_id = $1 ORDER BY member_ordinal",
    )
    .bind(corpus_version_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| Problem::from_db("reading the corpus members", error))?;
    let version_ids: Vec<Uuid> = rows
        .iter()
        .map(|row| row.get::<Uuid, _>("review_record_version_id"))
        .collect();
    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "member_ordinal": row.get::<i32, _>("member_ordinal"),
                "review_record_id": row.get::<Uuid, _>("review_record_id").to_string(),
                "review_record_version_id":
                    row.get::<Uuid, _>("review_record_version_id").to_string(),
            })
        })
        .collect();
    tx.commit()
        .await
        .map_err(|error| Problem::from_db("committing the corpus read", error))?;
    Ok((
        StatusCode::OK,
        Json(json!({
            "corpus_version_id": corpus_version_id.to_string(),
            "member_ordering_rule": corpus.member_ordering_rule,
            "member_manifest_hash": corpus.member_manifest_hash,
            "recomputed_manifest_hash": member_manifest_hash(&version_ids),
            "items": items,
            "count": items.len(),
        })),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_empty_membership_hashes_the_empty_string() {
        // The value `tests/evidence_review_schema.rs` inserts for an empty corpus, and exactly
        // what `corpus_versions_verify_seal`'s `coalesce(string_agg(…), '')` computes.
        let empty: Vec<Uuid> = Vec::new();
        assert_eq!(
            member_manifest_hash(&empty),
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn members_are_ordered_by_their_text_form() {
        let low = Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid");
        let high = Uuid::parse_str("ffffffff-ffff-ffff-ffff-ffffffffffff").expect("uuid");
        assert_eq!(
            order_members([&high, &low]),
            vec![low.to_string(), high.to_string()]
        );
        let forward = member_manifest_hash([&low, &high]);
        let backward = member_manifest_hash([&high, &low]);
        assert_eq!(
            forward, backward,
            "the manifest is independent of insertion order"
        );
    }
}
