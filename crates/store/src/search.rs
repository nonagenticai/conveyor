//! The ranked search arms (S07 §5.6).
//!
//! Three arms — [`SearchArm::GovernedAsset`], [`SearchArm::Card`] and
//! [`SearchArm::CatalogueEntry`] — are `UNION ALL`ed in **one statement**, and only the
//! requested arms are emitted. Every predicate is SQL: the tenant, the kind/status/project
//! filters and both arms' own restrictions. Rust sees at most `limit` rows, never a table
//! (S07 trap 2).
//!
//! **Score.** `W_RANK · ts_rank_cd(tsv, tsq) + W_TRGM · word_similarity(q, <label>) +
//! KEY_EXACT_BONUS · [<key> = q]`, where `tsq` is `websearch_to_tsquery('english', q)` and
//! the three weights are the committed constants below (S07 §5.6's "one committed constant
//! tuned by grid search, not a learned model"). A catalogue entry has no `tsv`, so its rank
//! term is omitted.
//!
//! **Trigram, not similarity.** Measured on a real `postgres:18`: `similarity('authentcation',
//! 'Authentication service for enterprise tenants')` is **0.255** — below the `%` threshold,
//! so a typo is never found — while `word_similarity` over the same pair is **0.706**. The
//! arms therefore use the `<%` operator (word similarity against the best-matching word of
//! the label), which is also what `gin (<label> gin_trgm_ops)` serves: `EXPLAIN` shows
//! `Bitmap Index Scan … Index Cond: (title %> 'paymnet')`.
//!
//! **This only works because two operators are marked LEAKPROOF.** Every table here is
//! `FORCE ROW LEVEL SECURITY`, and under a policy the planner refuses an index whose qual is
//! not leakproof. `ts_match_vq` (`@@`) and `pg_trgm.word_similarity_op` (`<%`) both ship
//! `proleakproof = false`, so before 2026-11-08 this exact statement planned as
//! `Bitmap Index Scan on <tenant btree>` + a per-row filter — a silent O(rows-per-tenant)
//! sweep. `scripts/db-roles.sql` (superuser, run before migrating) creates `pg_trgm`, marks
//! both leakproof and raises if it cannot; with that in place the same statement, role and
//! policy plan as a `BitmapOr` over `governed_asset_tsv_idx` and
//! `governed_asset_title_trgm_idx` (1.6 ms over 50 000 rows, measured). There is no
//! application-level fix for this: `conveyor_migrate` is NOSUPERUSER, so the `ALTER FUNCTION`
//! cannot live in a migration. `crates/api/tests/search_index.rs` fails if the step is skipped.
//!
//! **The threshold is lowered transaction-locally.** `pg_trgm.word_similarity_threshold`
//! defaults to **0.6**, and `word_similarity('paymnet','Payment reconciliation ledger')` is
//! **0.500** — so at the default a one-token typo is missed. [`search_tx`] sets it to
//! [`WORD_SIMILARITY_THRESHOLD`] with `set_config(..., true)` **before** the query runs: the
//! third argument is the bind-parameter form of `SET LOCAL`, and a session-level `SET` must
//! never appear here (this database is reached through a transaction pooler, and a session
//! GUC is inherited by the next client — measured on the estate 2026-09-22). Setting it
//! before the extension's library is loaded is safe: PostgreSQL stores the value as a
//! placeholder and applies it when `pg_trgm` registers the variable in the same transaction
//! (verified on `postgres:18`).
//!
//! Because of that `SET LOCAL`, **`search_tx` must run inside the caller's transaction** —
//! every `_tx` function in this crate does, and under autocommit the setting would die with
//! its own statement and the typo arm would silently fall back to 0.6.

use conveyor_domain::TenantId;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::scope::set_guc;

/// The full-text rank weight.
pub const W_RANK: f64 = 1.0;

/// The trigram word-similarity weight.
pub const W_TRGM: f64 = 0.6;

/// The bonus for a label whose key (`governed_asset.key`, `catalogue_entry.code`) is the
/// query verbatim — an exact key beats any lexical match.
pub const KEY_EXACT_BONUS: f64 = 3.0;

/// The `pg_trgm.word_similarity_threshold` the typo arm runs at.
///
/// Below the extension's 0.6 default on purpose: `paymnet` scores exactly 0.500 against
/// `Payment reconciliation ledger`, so the default would miss it.
pub const WORD_SIMILARITY_THRESHOLD: &str = "0.40";

/// The longest query text the action accepts; the store trusts its caller but documents the
/// bound the rank is tuned for.
pub const MAX_Q_CHARS: usize = 200;

/// Which table an arm reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SearchArm {
    /// `governed_asset` — requirements, principles and ADRs.
    GovernedAsset,
    /// `card` — board cards.
    Card,
    /// `catalogue_entry` — the per-tenant vocabulary.
    CatalogueEntry,
}

impl SearchArm {
    /// Every arm, in the order the score ties break (`entity_type` alphabetical).
    pub const ALL: [SearchArm; 3] = [
        SearchArm::CatalogueEntry,
        SearchArm::Card,
        SearchArm::GovernedAsset,
    ];

    /// The wire spelling, and the `entity_type` a hit carries.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GovernedAsset => "governed_asset",
            Self::Card => "card",
            Self::CatalogueEntry => "catalogue_entry",
        }
    }

    /// Parse the wire spelling; `None` is a value outside the vocabulary.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|arm| arm.as_str() == raw)
    }
}

/// What to search for, over which arms, and how many rows.
#[derive(Clone, Debug, Default)]
pub struct SearchRequest {
    /// The query text (the action trims and bounds it; the store binds it verbatim).
    pub q: String,
    /// Which arms to search. Duplicates collapse and the order is irrelevant; an empty list
    /// requests no arm and returns no hit.
    pub arms: Vec<SearchArm>,
    /// Restrict `governed_asset` to this `kind`. Ignored by the other two arms.
    pub kind: Option<String>,
    /// Restrict `governed_asset` to this `status`. Ignored by the other two arms.
    pub status: Option<String>,
    /// Restrict `governed_asset` to this `project_id`. Ignored by the other two arms.
    pub project_id: Option<Uuid>,
    /// The maximum number of hits, per arm and in total.
    pub limit: i64,
}

/// One ranked row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct SearchHit {
    /// The arm that produced it: `governed_asset`, `card` or `catalogue_entry`.
    pub entity_type: String,
    /// The row id in that arm's table.
    pub id: Uuid,
    /// The arm's display text: an asset's `title`, a card's `name`, a catalogue `label`.
    pub title: String,
    /// An asset's `key` or a catalogue entry's `code`; `None` for a card.
    pub key: Option<String>,
    /// An asset's `kind` or a catalogue entry's `kind`; `None` for a card.
    pub kind: Option<String>,
    /// An asset's `status`; `active` for a card or a catalogue entry.
    pub status: Option<String>,
    /// The highlighted excerpt (`ts_headline`, 50 words, one fragment); `None` for a
    /// catalogue entry, which has no body.
    pub snippet: Option<String>,
    /// The ranking score; higher is better.
    pub score: f64,
}

/// Why a search failed. The only failure mode is the database: every predicate is SQL and
/// the caller has already validated its vocabulary.
#[derive(Debug, thiserror::Error)]
pub enum SearchStoreError {
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// The `governed_asset` arm.
///
/// Its `tsv` is the generated `key || ' ' || title || ' ' || body`, so a body phrase and a
/// stemmed title word are both reachable; `ts_headline` reads the body only. The third
/// disjunct is what makes [`KEY_EXACT_BONUS`] reachable: a key the `english` configuration
/// drops (a stopword-only key, measured: `websearch_to_tsquery('english','a')` is the empty
/// query and `'a' <% title` is false) is invisible to both lexical arms, so without
/// `key = $2` the row would be filtered out before its bonus could score it.
const GOVERNED_ASSET_ARM: &str = "(SELECT 'governed_asset'::text AS entity_type, \
        id, \
        title, \
        key, \
        kind, \
        status, \
        ts_headline('english', body, websearch_to_tsquery('english', $2::text), \
                    'MaxWords=50, MinWords=10, MaxFragments=1') AS snippet, \
        ($4::float8 * ts_rank_cd(tsv, websearch_to_tsquery('english', $2::text)) \
         + $5::float8 * word_similarity($2::text, title) \
         + $6::float8 * (CASE WHEN key = $2::text THEN 1 ELSE 0 END))::float8 AS score \
   FROM governed_asset \
  WHERE tenant_id = $1 \
    AND (tsv @@ websearch_to_tsquery('english', $2::text) OR $2::text <% title \
         OR key = $2::text) \
    AND ($7::text IS NULL OR kind = $7) \
    AND ($8::text IS NULL OR status = $8) \
    AND ($9::uuid IS NULL OR project_id = $9) \
  ORDER BY score DESC, id \
  LIMIT $3)";

/// The `card` arm. An archived card is excluded rather than reported as `archived`: S07
/// searches the board, not the archive, and a card has no status column — the literal
/// `active` is what the hit reports.
const CARD_ARM: &str = "(SELECT 'card'::text AS entity_type, \
        id, \
        name AS title, \
        NULL::text AS key, \
        NULL::text AS kind, \
        'active'::text AS status, \
        ts_headline('english', description, websearch_to_tsquery('english', $2::text), \
                    'MaxWords=50, MinWords=10, MaxFragments=1') AS snippet, \
        ($4::float8 * ts_rank_cd(tsv, websearch_to_tsquery('english', $2::text)) \
         + $5::float8 * word_similarity($2::text, name))::float8 AS score \
   FROM card \
  WHERE tenant_id = $1 AND archived_at IS NULL \
    AND (tsv @@ websearch_to_tsquery('english', $2::text) OR $2::text <% name) \
  ORDER BY score DESC, id \
  LIMIT $3)";

/// The `catalogue_entry` arm. No `tsv` column and no body, so the score is the trigram term
/// plus the key-exact bonus and the rank term is omitted. As on the asset arm, `code = $2`
/// keeps the exact-key bonus reachable for a code the trigram arm cannot see.
const CATALOGUE_ENTRY_ARM: &str = "(SELECT 'catalogue_entry'::text AS entity_type, \
        id, \
        label AS title, \
        code AS key, \
        kind, \
        'active'::text AS status, \
        NULL::text AS snippet, \
        ($5::float8 * word_similarity($2::text, label) \
         + $6::float8 * (CASE WHEN code = $2::text THEN 1 ELSE 0 END))::float8 AS score \
   FROM catalogue_entry \
  WHERE tenant_id = $1 AND is_active \
    AND ($2::text <% label OR code = $2::text) \
  ORDER BY score DESC, id \
  LIMIT $3)";

/// The one constant SQL statement per arm. A caller-supplied arm can therefore never reach
/// the statement text: [`search_tx`] picks a fragment by `match` and interpolates nothing
/// but these literals.
fn arm_sql(arm: SearchArm) -> &'static str {
    match arm {
        SearchArm::GovernedAsset => GOVERNED_ASSET_ARM,
        SearchArm::Card => CARD_ARM,
        SearchArm::CatalogueEntry => CATALOGUE_ENTRY_ARM,
    }
}

/// Rank every arm the request names, in one statement, inside the caller's tenant.
///
/// Each arm orders by `score DESC, id` and takes `limit` rows before the union, so one arm
/// cannot crowd another out; the union is then ordered by `score DESC, entity_type, id` and
/// takes `limit` rows again, which is the page the caller sees. The `entity_type`/`id`
/// tie-break makes a page with equal scores reproducible.
///
/// `q` is bound as `$2::text` everywhere — the explicit cast is what lets an untyped
/// parameter be used on both sides of `tsv @@ tsquery` and of the `<%` operator.
pub async fn search_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    request: &SearchRequest,
) -> Result<Vec<SearchHit>, SearchStoreError> {
    // Duplicates collapse and the order is irrelevant; an empty request reads nothing.
    let mut arms: Vec<SearchArm> = Vec::new();
    for arm in &request.arms {
        if !arms.contains(arm) {
            arms.push(*arm);
        }
    }
    if arms.is_empty() {
        return Ok(Vec::new());
    }

    // Transaction-local, and before the statement: `paymnet` scores 0.500 against
    // `Payment reconciliation ledger`, so the 0.6 default would miss the typo arm entirely.
    set_guc(
        conn,
        "pg_trgm.word_similarity_threshold",
        WORD_SIMILARITY_THRESHOLD,
    )
    .await?;

    let mut sql = String::from("SELECT * FROM (");
    for (index, arm) in arms.iter().enumerate() {
        if index > 0 {
            sql.push_str(" UNION ALL ");
        }
        sql.push_str(arm_sql(*arm));
    }
    sql.push_str(") hits ORDER BY score DESC, entity_type, id LIMIT $3");

    let rows: Vec<SearchHit> = sqlx::query_as(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(tenant.as_uuid())
        .bind(&request.q)
        .bind(request.limit.max(1))
        .bind(W_RANK)
        .bind(W_TRGM)
        .bind(KEY_EXACT_BONUS)
        .bind(request.kind.as_deref())
        .bind(request.status.as_deref())
        .bind(request.project_id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_spellings_round_trip_and_the_vocabulary_is_closed() {
        assert_eq!(SearchArm::GovernedAsset.as_str(), "governed_asset");
        assert_eq!(SearchArm::Card.as_str(), "card");
        assert_eq!(SearchArm::CatalogueEntry.as_str(), "catalogue_entry");
        for arm in SearchArm::ALL {
            assert_eq!(SearchArm::parse(arm.as_str()), Some(arm));
        }
        assert_eq!(SearchArm::parse("governed_assets"), None);
        assert_eq!(SearchArm::parse(""), None);
        assert_eq!(SearchArm::parse("CARD"), None);
    }

    #[test]
    fn every_arm_has_its_own_constant_statement() {
        // Three distinct fragments, all parameterised on `$1`/`$2`/`$3` and each naming its
        // own table — the match cannot hand back another arm's SQL.
        assert_ne!(arm_sql(SearchArm::GovernedAsset), arm_sql(SearchArm::Card));
        assert_ne!(arm_sql(SearchArm::Card), arm_sql(SearchArm::CatalogueEntry));
        assert!(arm_sql(SearchArm::GovernedAsset).contains("FROM governed_asset"));
        assert!(arm_sql(SearchArm::Card).contains("FROM card"));
        assert!(arm_sql(SearchArm::CatalogueEntry).contains("FROM catalogue_entry"));
    }

    #[test]
    fn the_weights_are_the_committed_constants() {
        assert_eq!(W_RANK, 1.0);
        assert_eq!(W_TRGM, 0.6);
        assert_eq!(KEY_EXACT_BONUS, 3.0);
        assert_eq!(WORD_SIMILARITY_THRESHOLD, "0.40");
    }
}
