//! `count_tenant_seats` — the billable-seat predicate and its subject partition.
//!
//! Ported from `models.py::DatabaseManager.count_tenant_seats` and the two clause builders it
//! uses: `effectively_active_clauses` (shared there with `/internal/licenses`) and
//! `active_during_clauses` (which nothing else reads).
//!
//! ⛔ THE PREDICATE IS THE BILLED NUMBER. A seat is a licence that is STORED-ACTIVE, STARTED and
//! UNEXPIRED at `as_of` — the same three-part rule as `License.effective_status`. The stored
//! `status` column alone is not validity: nothing in the estate transitions it when `expires_at`
//! passes, so filtering on it alone reads expired licences as revenue. Both arms of the soft
//! delete (migration 036) are live — `LICENSE_SOFT_DELETE` now defaults true — so a row deleted
//! AFTER `as_of` was still a seat at `as_of`, and one deleted before it was not.
//!
//! A period is a DIFFERENT question ("held at any instant in `[start, end)`"), so it gets its own
//! predicate; the instant predicate is deliberately not widened to serve it.
//!
//! `seats` counts LICENCES (one row per seat); `subjects` counts distinct `license.subject_id`.
//! The person basis is `human_subjects` — `subjects` minus the realms in
//! `NON_HUMAN_SUBJECT_REALMS` — and `service_subjects` + `unresolved_subjects` + `human_subjects`
//! partition `subjects` exactly. The partition is a second SELECT over the already-selected id
//! set, never a JOIN on the licence query: a join could add or remove a licence from the count
//! billing reads, and an outer join would silently decide the unresolved case.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// `NON_HUMAN_SUBJECT_REALMS` (`models.py`). `db::tokens` keeps the same private copy.
const NON_HUMAN_SUBJECT_REALMS: [&str; 2] = ["service", "cp-tenant"];

/// `effectively_active_clauses($3)`: "this licence was a seat at the instant `$3`".
///
/// The soft-delete arm is required because a soft-deleted row's CURRENT status is `revoked` by
/// construction, so `status` cannot answer what the row was before the delete; the timestamp
/// alone cannot either without `deleted_from_status`.
const POINT_IN_TIME_SQL: &str = "\
SELECT l.subject_id
  FROM license l
 WHERE l.tenant_id = $1
   AND ($2::uuid IS NULL OR l.app_id = $2)
   AND (
         (l.deleted_at IS NULL AND l.status = 'active')
      OR (l.deleted_at IS NOT NULL AND l.deleted_at > $3 AND l.deleted_from_status = 'active')
       )
   AND (l.starts_at IS NULL OR l.starts_at <= $3)
   AND (l.expires_at IS NULL OR l.expires_at > $3)";

/// `active_during_clauses($3, $4)`: "this licence was a seat at SOME instant in `[$3, $4)`".
///
/// The revocation arm is why this predicate exists: a row revoked AFTER `period_start` was a
/// seat during the period, and `revoked_at` is stamped on the FIRST transition only.
const PERIOD_SQL: &str = "\
SELECT l.subject_id
  FROM license l
 WHERE l.tenant_id = $1
   AND ($2::uuid IS NULL OR l.app_id = $2)
   AND l.created_at < $4
   AND (l.starts_at IS NULL OR l.starts_at < $4)
   AND (l.expires_at IS NULL OR l.expires_at > $3)
   AND (
         (l.deleted_at IS NULL AND l.status IN ('active', 'expired'))
      OR (l.deleted_at IS NULL AND l.status = 'revoked'
          AND l.revoked_at IS NOT NULL AND l.revoked_at > $3)
      OR (l.deleted_at IS NOT NULL AND l.deleted_at > $3
          AND l.deleted_from_status = 'active')
      OR (l.deleted_at IS NOT NULL AND l.deleted_from_status = 'revoked'
          AND l.revoked_at IS NOT NULL AND l.revoked_at > $3)
       )";

/// The tenant fields the count discloses (`tenant_kind` and `tenant_archived`).
#[derive(Debug, sqlx::FromRow)]
struct TenantRow {
    id: Uuid,
    kind: String,
    archived_at: Option<DateTime<Utc>>,
}

/// The counts, with the tenant metadata the response discloses beside them.
#[derive(Debug, Clone)]
pub struct Seats {
    pub tenant_id: String,
    pub tenant_kind: String,
    pub tenant_archived: bool,
    pub seats: i64,
    pub subjects: i64,
    pub human_subjects: i64,
    pub service_subjects: i64,
    pub unresolved_subjects: i64,
    /// `active_licences` | `active_during_period` | `tenant_archived`.
    pub basis: &'static str,
}

/// What the reference returns as `None`, `{"app_not_found": …}`, or a count.
#[derive(Debug, Clone)]
pub enum CountOutcome {
    TenantNotFound,
    AppNotFound,
    Counted(Seats),
}

/// Resolve the path parameter the way the reference does: a UUID is looked up by ID (archived
/// tenants included — they are reachable by ID), anything else by live NAME.
///
/// A name that matches live tenants of more than one owner resolves to nothing, exactly like
/// `_tenant_by_name`; picking one would silently mis-attribute a seat count.
async fn resolve_tenant(pool: &PgPool, raw: &str) -> Result<Option<TenantRow>, sqlx::Error> {
    if let Ok(id) = Uuid::try_parse(raw) {
        return sqlx::query_as::<_, TenantRow>(
            "SELECT id, kind, archived_at FROM tenants WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(pool)
        .await;
    }
    let rows: Vec<TenantRow> = sqlx::query_as(
        "SELECT id, kind, archived_at FROM tenants
          WHERE name = $1 AND archived_at IS NULL
          LIMIT 2",
    )
    .bind(raw)
    .fetch_all(pool)
    .await?;
    if rows.len() > 1 {
        tracing::warn!(
            tenant_name = raw,
            "tenant_name_ambiguous: name matches tenants of several owners; refusing to pick one"
        );
        return Ok(None);
    }
    Ok(rows.into_iter().next())
}

/// `DatabaseManager.count_tenant_seats`, as one call.
///
/// `as_of` is always the effective instant (the caller defaults it to now, once, so the value
/// echoed in the response and the value counted are the same instant). `period` replaces the
/// instant predicate and ignores `as_of` for counting, exactly as the reference does.
pub async fn count_tenant_seats(
    pool: &PgPool,
    tenant_id: &str,
    as_of: DateTime<Utc>,
    app_key: Option<&str>,
    period: Option<(DateTime<Utc>, DateTime<Utc>)>,
) -> Result<CountOutcome, sqlx::Error> {
    let Some(tenant) = resolve_tenant(pool, tenant_id).await? else {
        return Ok(CountOutcome::TenantNotFound);
    };

    // The app lookup runs BEFORE the archived-tenant branch: an unknown key is a caller typo
    // (422) regardless of the tenant's state, and the reference orders it the same way.
    let app_id = match app_key {
        Some(key) => {
            let found: Option<Uuid> =
                sqlx::query_scalar::<_, Uuid>("SELECT id FROM app WHERE key = $1")
                    .bind(key)
                    .fetch_optional(pool)
                    .await?;
            match found {
                Some(id) => Some(id),
                None => return Ok(CountOutcome::AppNotFound),
            }
        }
        None => None,
    };

    let archived = tenant.archived_at.is_some();
    if archived {
        // Said with a basis rather than a silent 0: "turned off" and "has no seats" are
        // different facts. Every key the active branch returns appears here too.
        return Ok(CountOutcome::Counted(Seats {
            tenant_id: tenant.id.to_string(),
            tenant_kind: tenant.kind,
            tenant_archived: true,
            seats: 0,
            subjects: 0,
            human_subjects: 0,
            service_subjects: 0,
            unresolved_subjects: 0,
            basis: "tenant_archived",
        }));
    }

    // `Option<Uuid>` mirrors the reference's tolerance of a NULL subject_id: it would count as
    // a seat and as unresolved, never promoted to a person or a robot. The schema makes it
    // impossible today (NOT NULL + FK ON DELETE CASCADE); the field exists so it stays visible.
    let rows: Vec<Option<Uuid>> = match period {
        Some((period_start, period_end)) => {
            sqlx::query_scalar(PERIOD_SQL)
                .bind(tenant.id)
                .bind(app_id)
                .bind(period_start)
                .bind(period_end)
                .fetch_all(pool)
                .await?
        }
        None => {
            sqlx::query_scalar(POINT_IN_TIME_SQL)
                .bind(tenant.id)
                .bind(app_id)
                .bind(as_of)
                .fetch_all(pool)
                .await?
        }
    };
    let seats = rows.len() as i64;
    let subject_ids: HashSet<Option<Uuid>> = rows.into_iter().collect();

    let resolved_ids: Vec<Uuid> = subject_ids.iter().flatten().copied().collect();
    let realms: HashMap<Uuid, Option<String>> = if resolved_ids.is_empty() {
        HashMap::new()
    } else {
        sqlx::query_as::<_, (Uuid, Option<String>)>(
            "SELECT id, keycloak_realm FROM subject WHERE id = ANY($1)",
        )
        .bind(resolved_ids.as_slice())
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect()
    };

    let service_subjects = subject_ids
        .iter()
        .filter(|subject_id| match subject_id {
            Some(id) => realms
                .get(id)
                .and_then(|realm| realm.as_deref())
                .is_some_and(|realm| NON_HUMAN_SUBJECT_REALMS.contains(&realm)),
            None => false,
        })
        .count() as i64;
    let unresolved_subjects = subject_ids
        .iter()
        .filter(|subject_id| match subject_id {
            Some(id) => !realms.contains_key(id),
            None => true,
        })
        .count() as i64;
    let subjects = subject_ids.len() as i64;

    Ok(CountOutcome::Counted(Seats {
        tenant_id: tenant.id.to_string(),
        tenant_kind: tenant.kind,
        tenant_archived: false,
        seats,
        subjects,
        human_subjects: subjects - service_subjects - unresolved_subjects,
        service_subjects,
        unresolved_subjects,
        basis: if period.is_some() {
            "active_during_period"
        } else {
            "active_licences"
        },
    }))
}
