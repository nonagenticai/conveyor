#![forbid(unsafe_code)]
//! `search.query` — one ranked read over three tables (S07 §5.6).
//!
//! The action validates and delegates: it trims and bounds `q`, maps `types` onto the store's
//! [`store::search::SearchArm`] vocabulary, checks `kind`/`status` against the domain, clamps
//! the limit and calls [`store::search::search_tx`] once. Every predicate — the tenant, the
//! three filters, both lexical arms — is SQL, so Rust never sees more than `limit` rows
//! (S07 trap 2).
//!
//! **No cursor.** Ranked results have no stable sort key to page on (`score` is a float over
//! a corpus that changes underneath), so the action will not mint a cursor it cannot honour.
//! A supplied `cursor` is a `422` and `next_cursor` is always absent; a caller narrows the
//! query or lowers `limit` instead.
//!
//! **`kind` and `status` narrow governed assets only.** A card has no kind or status column
//! and a catalogue entry has no status, so the filters apply to the one arm that has them —
//! `S07 §5.6` names them under `governed_asset`. A `types` value outside the vocabulary is a
//! `422` naming `types`, never an empty result.
//!
//! Nothing here is a mutation: no row is written, no event is emitted, and the read runs in
//! the caller's already-scoped transaction.

use std::str::FromStr;

use conveyor_domain::{AssetKind, AssetStatus};
use conveyor_store as store;
use conveyor_store::search::{MAX_Q_CHARS, SearchArm, SearchHit, SearchRequest, SearchStoreError};
use sqlx::PgConnection;

use crate::invoke::{ActionError, HandlerCtx};

use super::{Page, SearchHitOut, SearchQueryIn, invalid, store_error};

/// The three statuses, as every refusal names them.
const STATUSES: &str = "draft, in_review, approved, superseded or archived";

/// The three kinds, as every refusal names them.
const KINDS: &str = "requirement, principle or adr";

/// One hit as the action returns it.
fn hit_out(hit: &SearchHit) -> SearchHitOut {
    SearchHitOut {
        entity_type: hit.entity_type.clone(),
        id: hit.id.to_string(),
        title: hit.title.clone(),
        key: hit.key.clone(),
        kind: hit.kind.clone(),
        status: hit.status.clone(),
        snippet: hit.snippet.clone(),
        score: hit.score,
    }
}

/// The arm vocabulary, as a refusal lists it.
fn arm_vocabulary() -> String {
    SearchArm::ALL
        .iter()
        .map(|arm| arm.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// A search-store failure: the only variant is a database error, so it is the caller's `500`
/// exactly as any other store failure is.
fn search_error(error: SearchStoreError, action: &str) -> ActionError {
    match error {
        SearchStoreError::Database(error) => store_error(error, action),
    }
}

/// The arms a request asks for: every arm when `types` is absent, otherwise exactly the ones
/// named, deduplicated in request order.
///
/// An explicitly empty `types` array asks for no arm and returns no hit — the absent member
/// and the empty array are different requests, and pretending otherwise would make a typo in
/// a client's list silently search everything.
fn arms_for(types: Option<&[String]>) -> Result<Vec<SearchArm>, ActionError> {
    let Some(raw) = types else {
        return Ok(SearchArm::ALL.to_vec());
    };
    let mut arms: Vec<SearchArm> = Vec::new();
    for name in raw {
        let arm = SearchArm::parse(name).ok_or_else(|| {
            invalid(
                "types",
                "invalid_value",
                format!(
                    "{name:?} is not a search arm; expected one of {}",
                    arm_vocabulary()
                ),
            )
        })?;
        if !arms.contains(&arm) {
            arms.push(arm);
        }
    }
    Ok(arms)
}

/// Rank assets, cards and catalogue entries against one query.
///
/// `q` is trimmed before it is measured, so `"  "` is `422 required` rather than a search for
/// whitespace; the bound is [`MAX_Q_CHARS`] characters. `kind` and `status` are checked here
/// rather than in SQL so an unknown value names its field instead of returning zero rows.
#[crate::action(
    name = "search.query",
    version = 1,
    kind = "internal",
    scopes = ["search:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn search_query(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: SearchQueryIn,
) -> Result<Page<SearchHitOut>, ActionError> {
    if input.page.cursor.is_some() {
        return Err(invalid(
            "cursor",
            "invalid_value",
            "ranked results are not paginated",
        ));
    }

    let q = input.q.trim();
    if q.is_empty() {
        return Err(invalid("q", "required", "must not be empty or whitespace"));
    }
    if q.chars().count() > MAX_Q_CHARS {
        return Err(invalid(
            "q",
            "too_long",
            format!(
                "must be at most {MAX_Q_CHARS} characters, got {}",
                q.chars().count()
            ),
        ));
    }

    let arms = arms_for(input.types.as_deref())?;

    let kind = match input.kind.as_deref() {
        None => None,
        Some(raw) => Some(
            AssetKind::from_str(raw)
                .map_err(|_| invalid("kind", "invalid_value", format!("must be {KINDS}")))?
                .as_str()
                .to_owned(),
        ),
    };
    let status = match input.status.as_deref() {
        None => None,
        Some(raw) => Some(
            AssetStatus::from_str(raw)
                .map_err(|_| invalid("status", "invalid_value", format!("must be {STATUSES}")))?
                .as_str()
                .to_owned(),
        ),
    };

    let request = SearchRequest {
        q: q.to_owned(),
        arms,
        kind,
        status,
        project_id: input.project_id,
        limit: store::actions::clamp_limit(input.page.limit),
    };
    let hits = store::search::search_tx(&mut *tx, ctx.tenant, &request)
        .await
        .map_err(|error| search_error(error, "search.query"))?;

    Ok(Page {
        items: hits.iter().map(hit_out).collect(),
        next_cursor: None,
    })
}
