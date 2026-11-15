//! `media_items` — the one table every other operation reads through.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use sqlx::types::Json;
use uuid::Uuid;

use super::timestamp;
use crate::media::models::{Components, MediaItem};

/// The columns `MediaItemResponse` carries. `metadata` and `embedding` are deliberately absent:
/// neither is in the captured response, and `embedding` is fetched only by the related query.
#[derive(Debug, sqlx::FromRow)]
pub struct Row {
    pub id: String,
    pub tenant_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub url: String,
    pub media_type: String,
    pub source: String,
    pub external_ids: Json<Value>,
    pub duration_seconds: Option<i32>,
    pub thumbnail_url: Option<String>,
    pub author_name: Option<String>,
    pub author_channel_url: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub language: Option<String>,
    pub tags: Json<Value>,
    pub view_count: Option<i32>,
    pub like_count: Option<i32>,
    pub quality_score: Option<f64>,
    pub quality_components: Option<Json<Value>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// The columns `MediaItemResponse` carries, as a literal so every query that uses them is a
/// compile-time `&'static str` — sqlx 0.9 refuses a `format!`-built statement, and that is a
/// feature: it makes dynamic SQL an explicit act rather than an accident.
macro_rules! columns {
    () => {
        "id, tenant_id, title, description, url, media_type, source, external_ids,
         duration_seconds, thumbnail_url, author_name, author_channel_url, published_at, language,
         tags, view_count, like_count, quality_score, quality_components, created_at, updated_at"
    };
}
pub(crate) use columns;

/// The same list, for callers that need it as a value.
pub const COLUMNS: &str = columns!();

impl Row {
    /// The captured response shape.
    ///
    /// `description` is a non-nullable string in the reference's schema over a nullable column, so
    /// a row with no description makes the reference answer a 500 (measured 2026-11-15). The port
    /// answers an empty string instead; the deviation is declared in `contract/media/README.md`.
    pub fn into_response(self) -> MediaItem {
        MediaItem {
            id: self.id,
            tenant_id: self.tenant_id.to_string(),
            title: self.title,
            description: self.description.unwrap_or_default(),
            url: self.url,
            media_type: self.media_type,
            source: self.source,
            external_ids: self.external_ids.0,
            duration_seconds: self.duration_seconds,
            thumbnail_url: self.thumbnail_url,
            author_name: self.author_name,
            author_channel_url: self.author_channel_url,
            published_at: self.published_at.map(timestamp),
            language: self.language,
            tags: self.tags.0,
            view_count: self.view_count,
            like_count: self.like_count,
            quality_score: self.quality_score,
            quality_components: self.quality_components.map(|json| json.0),
            created_at: timestamp(self.created_at),
            updated_at: timestamp(self.updated_at),
        }
    }
}

/// One item, or `None` when this tenant has no such row.
///
/// Tenant is a predicate, not a filter applied afterwards: the reference's own `select(...).where`
/// names both columns, and a caller that omits the second one reads another customer's rows.
pub async fn find(
    pool: &PgPool,
    item_id: &str,
    tenant_id: Uuid,
) -> Result<Option<Row>, sqlx::Error> {
    sqlx::query_as::<_, Row>(concat!(
        "SELECT ",
        columns!(),
        " FROM media_items WHERE id = $1 AND tenant_id = $2"
    ))
    .bind(item_id)
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
}

/// How many embedded candidates the in-Python cosine scans. The reference caps this at 500
/// because embeddings are `LargeBinary`, not `pgvector` — there is no SQL `ORDER BY distance`.
const RELATED_CANDIDATE_CAP: i64 = 500;

/// Items related to `subject`, in the reference's two-stage order: semantic first, then the
/// metadata top-up.
///
/// Stage 1 runs only when the subject has an embedding, and keeps only positive similarities.
/// Stage 2 fills whatever slots remain with same-`media_type` **or** same-`author_name` rows,
/// newest first — or same-`media_type` alone when the subject has no author. Both stages exclude
/// the subject and anything stage 1 already returned.
pub async fn related(
    pool: &PgPool,
    subject: &Row,
    limit: i64,
    tenant_id: Uuid,
) -> Result<Vec<Row>, sqlx::Error> {
    let mut out: Vec<Row> = Vec::new();
    let mut exclude: Vec<String> = vec![subject.id.clone()];

    let embedding: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT embedding FROM media_items WHERE id = $1 AND tenant_id = $2")
            .bind(&subject.id)
            .bind(tenant_id)
            .fetch_optional(pool)
            .await?
            .flatten();

    if let Some(target) = embedding.as_deref() {
        #[derive(Debug, sqlx::FromRow)]
        struct Candidate {
            #[sqlx(flatten)]
            row: Row,
            embedding: Option<Vec<u8>>,
        }

        let candidates = sqlx::query_as::<_, Candidate>(concat!(
            "SELECT ",
            columns!(),
            ", embedding FROM media_items
              WHERE tenant_id = $1 AND id <> $2 AND embedding IS NOT NULL
              ORDER BY created_at DESC LIMIT $3"
        ))
        .bind(tenant_id)
        .bind(&subject.id)
        .bind(RELATED_CANDIDATE_CAP)
        .fetch_all(pool)
        .await?;

        let mut scored: Vec<(f64, Row)> = candidates
            .into_iter()
            .filter_map(|candidate| {
                let similarity = cosine_similarity(target, candidate.embedding.as_deref()?);
                (similarity > 0.0).then_some((similarity, candidate.row))
            })
            .collect();
        // Descending similarity. `sort_by` on a partial order: NaN cannot reach here, because
        // cosine_similarity never divides by zero.
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        for (_, row) in scored.into_iter().take(limit as usize) {
            exclude.push(row.id.clone());
            out.push(row);
        }
    }

    let remaining = limit - out.len() as i64;
    if remaining > 0 {
        let rows = if subject.author_name.is_some() {
            sqlx::query_as::<_, Row>(concat!(
                "SELECT ",
                columns!(),
                " FROM media_items
                  WHERE tenant_id = $1 AND NOT (id = ANY($2))
                    AND (media_type = $3 OR author_name = $4)
                  ORDER BY created_at DESC LIMIT $5"
            ))
            .bind(tenant_id)
            .bind(&exclude)
            .bind(&subject.media_type)
            .bind(&subject.author_name)
            .bind(remaining)
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query_as::<_, Row>(concat!(
                "SELECT ",
                columns!(),
                " FROM media_items
                  WHERE tenant_id = $1 AND NOT (id = ANY($2)) AND media_type = $3
                  ORDER BY created_at DESC LIMIT $4"
            ))
            .bind(tenant_id)
            .bind(&exclude)
            .bind(&subject.media_type)
            .bind(remaining)
            .fetch_all(pool)
            .await?
        };
        out.extend(rows);
    }

    Ok(out)
}

/// Persist a computed score onto the row, bumping `updated_at` — the reference relies on
/// SQLAlchemy's `onupdate=func.now()`, and this table has no trigger (the capture records none).
pub async fn set_score(
    pool: &PgPool,
    item_id: &str,
    tenant_id: Uuid,
    score: f64,
    components: &Components,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE media_items SET quality_score = $1, quality_components = $2, updated_at = now()
          WHERE id = $3 AND tenant_id = $4",
    )
    .bind(score)
    .bind(Json(Value::Object(components.clone())))
    .bind(item_id)
    .bind(tenant_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Cosine similarity of two packed little-endian `f32` embeddings (`struct.unpack(f"{n}f")` on the
/// reference side). Zero for an empty vector, a length mismatch or a zero norm.
pub fn cosine_similarity(a: &[u8], b: &[u8]) -> f64 {
    if a.is_empty() || a.len() != b.len() || !a.len().is_multiple_of(4) {
        return 0.0;
    }
    let (a_chunks, _) = a.as_chunks::<4>();
    let (b_chunks, _) = b.as_chunks::<4>();
    let mut dot = 0.0f64;
    let mut na = 0.0f64;
    let mut nb = 0.0f64;
    for (chunk_a, chunk_b) in a_chunks.iter().zip(b_chunks.iter()) {
        let x = f32::from_le_bytes(*chunk_a) as f64;
        let y = f32::from_le_bytes(*chunk_b) as f64;
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}
