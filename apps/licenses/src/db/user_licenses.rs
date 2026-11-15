//! DB reads and writes for the per-subject licence surface:
//! `GET`, `POST` and `DELETE /licenses/user/{subject_id}`.
//!
//! Reads are ported from `models.py::DatabaseManager.get_subject`,
//! `get_subject_by_keycloak_sub` and `list_licenses` (subject + tenant filters,
//! `deleted_at IS NULL` default). The reference's `list_licenses` adds no ORDER BY, so
//! neither does this one: row order stays the server's choice, exactly as `query.all()`
//! leaves it.
//!
//! Writes are ported from `models.py`:
//! - `expire_stale_licenses` (:7069) — the lazy expire-on-write inside `find_active_license`;
//! - `find_active_license` (:7586) — the idempotency read;
//! - `create_license` (:6956) — the INSERT;
//! - `subject_licence_delete_radius` (:8352), `delete_licenses_for_subject` (:8396) and
//!   `soft_delete_licenses_for_subject` (:8420) — the revoke/delete pair.
//!
//! ⛔ A licence INSERT that loses `uq_license_active_tenant_app_subject` is NOT an error: the
//! handler re-reads the winner and returns it, exactly as the reference's `IntegrityError`
//! clause does.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

/// The partial unique index an ACTIVE licence may not duplicate: `(tenant_id, app_id,
/// subject_id) WHERE status = 'active'`.
const UQ_ACTIVE_TENANT_APP_SUBJECT: &str = "uq_license_active_tenant_app_subject";

/// One subject row: the two identifiers the route's authorization needs.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Subject {
    pub id: Uuid,
    pub keycloak_sub: Option<String>,
}

/// `get_subject_by_keycloak_sub`: the portal passes the Keycloak `sub` claim.
///
/// The reference uses `.first()`; `LIMIT 1` is the same arbitrary first row. A duplicate
/// `keycloak_sub` is a data defect, not a reason to widen the lookup.
pub async fn subject_by_keycloak_sub(
    pool: &PgPool,
    keycloak_sub: &str,
) -> Result<Option<Subject>, sqlx::Error> {
    sqlx::query_as("SELECT id, keycloak_sub FROM subject WHERE keycloak_sub = $1 LIMIT 1")
        .bind(keycloak_sub)
        .fetch_optional(pool)
        .await
}

/// `get_subject`: the internal `subject.id` UUID (the fallback lookup).
pub async fn subject_by_id(
    pool: &PgPool,
    subject_id: Uuid,
) -> Result<Option<Subject>, sqlx::Error> {
    sqlx::query_as("SELECT id, keycloak_sub FROM subject WHERE id = $1")
        .bind(subject_id)
        .fetch_optional(pool)
        .await
}

/// The stored realm of an existing subject, for the reference's `subject_realm_mismatch`
/// warning ("an EXISTING Subject keeps its realm").
pub async fn subject_realm(pool: &PgPool, subject_id: Uuid) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT keycloak_realm FROM subject WHERE id = $1")
        .bind(subject_id)
        .fetch_optional(pool)
        .await
}

/// One licence row plus the app and plan fields the response enriches with.
///
/// `starts_at`, `created_at` and `updated_at` are non-optional on purpose: the reference's
/// `to_dict()` emits `None` for a NULL one and Pydantic then refuses the `LicenseResponse`
/// field, which the handler's top-level `except` turns into a 500. Decoding into a
/// non-optional type reproduces that fail-closed behaviour instead of emitting a null the
/// schema forbids.
#[derive(Debug, sqlx::FromRow)]
pub struct LicenseRow {
    pub id: Uuid,
    pub subject_id: Uuid,
    pub tenant_id: Uuid,
    pub app_id: Uuid,
    pub plan_id: Uuid,
    pub region_id: Option<Uuid>,
    pub status: String,
    pub starts_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub license_metadata: Option<Value>,
    pub metering_provisioned_at: Option<DateTime<Utc>>,
    pub metering_verified_at: Option<DateTime<Utc>>,
    pub token_budget_override: Option<i32>,
    pub quota_override: Option<i32>,
    pub is_dependency: Option<bool>,
    pub bundle_id: Option<Uuid>,
    pub app_key: Option<String>,
    pub app_name: Option<String>,
    // `_enrich_license_response` populates BOTH nested objects; the GET route deliberately
    // omits the plan, the POST route uses these.
    pub plan_code: Option<String>,
    pub plan_name: Option<String>,
    pub plan_rate_limit_rpm: Option<i32>,
    pub plan_rate_limit_burst: Option<i32>,
    pub plan_quota_monthly: Option<i32>,
    pub plan_token_budget_monthly: Option<i32>,
}

/// The joined projection every licence read below returns.
const LICENSE_COLUMNS: &str = "SELECT l.id,
                l.subject_id,
                l.tenant_id,
                l.app_id,
                l.plan_id,
                l.region_id,
                l.status,
                l.starts_at,
                l.expires_at,
                l.created_at,
                l.updated_at,
                l.revoked_at,
                l.metadata AS license_metadata,
                l.metering_provisioned_at,
                l.metering_verified_at,
                l.token_budget_override,
                l.quota_override,
                l.is_dependency,
                l.bundle_id,
                a.key AS app_key,
                a.name AS app_name,
                p.code AS plan_code,
                p.name AS plan_name,
                p.rate_limit_rpm AS plan_rate_limit_rpm,
                p.rate_limit_burst AS plan_rate_limit_burst,
                p.quota_monthly AS plan_quota_monthly,
                p.token_budget_monthly AS plan_token_budget_monthly
           FROM license l
           LEFT JOIN app a ON a.id = l.app_id
           LEFT JOIN plan p ON p.id = l.plan_id";

/// `DatabaseManager.list_licenses(subject_id=…, tenant_id=…)`.
///
/// `tenant_id` is `None` only for `license-admin`; every other caller passes the tenant the
/// handler resolved with `get_tenant_id`, so a NULL tenant filter is the operator surface and
/// never a widening of an ordinary read. Soft-deleted rows are excluded because the reference
/// calls this with `include_deleted=False`, whose unconditional arm is `deleted_at IS NULL`.
pub async fn list_for_subject(
    pool: &PgPool,
    subject_id: Uuid,
    tenant_id: Option<Uuid>,
) -> Result<Vec<LicenseRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{LICENSE_COLUMNS}
          WHERE l.subject_id = $1
            AND l.deleted_at IS NULL
            AND ($2::uuid IS NULL OR l.tenant_id = $2)"
    )))
    .bind(subject_id)
    .bind(tenant_id)
    .fetch_all(pool)
    .await
}

/// `expire_stale_licenses` (:7069): retire `status='active'` rows whose `expires_at` has passed
/// for ONE `(tenant, app, subject)` key, freeing `uq_license_active_tenant_app_subject`.
///
/// The reference takes the rows `FOR UPDATE` and re-checks `status = 'active'` in the loop. Under
/// READ COMMITTED a single UPDATE reproduces that exactly: a concurrent replica's committed
/// `status='expired'` makes Postgres re-evaluate the predicate on the new row version
/// (EvalPlanQual) and filter the row out, so neither replica double-writes and neither resurrects
/// the row. `expires_at IS NULL` (perpetual) is never touched — the predicate is written
/// explicitly so a refactor cannot drop it.
pub async fn expire_stale_licenses(
    pool: &PgPool,
    tenant_id: Uuid,
    app_id: Uuid,
    subject_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let updated = sqlx::query(
        "UPDATE license
            SET status = 'expired', updated_at = now()
          WHERE tenant_id = $1
            AND app_id = $2
            AND subject_id = $3
            AND status = 'active'
            AND expires_at IS NOT NULL
            AND expires_at < now()",
    )
    .bind(tenant_id)
    .bind(app_id)
    .bind(subject_id)
    .execute(pool)
    .await?;
    Ok(updated.rows_affected())
}

/// `find_active_license` (:7586): the existing ACTIVE, NON-EXPIRED licence for
/// `(tenant, app, subject)`, newest first.
///
/// Lazy expire-on-write runs FIRST (the write path is the only caller), then the read applies
/// the reference's time window: `expires_at IS NULL OR expires_at > now()`. The predicate
/// deliberately does NOT filter `starts_at` — a future-dated row is genuinely the licence this
/// key will have and still occupies the unique-index slot.
pub async fn find_active_license(
    pool: &PgPool,
    tenant_id: Uuid,
    app_id: Uuid,
    subject_id: Uuid,
) -> Result<Option<LicenseRow>, sqlx::Error> {
    expire_stale_licenses(pool, tenant_id, app_id, subject_id).await?;
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{LICENSE_COLUMNS}
          WHERE l.tenant_id = $1
            AND l.app_id = $2
            AND l.subject_id = $3
            AND l.status = 'active'
            AND (l.expires_at IS NULL OR l.expires_at > $4)
          ORDER BY l.created_at DESC
          LIMIT 1"
    )))
    .bind(tenant_id)
    .bind(app_id)
    .bind(subject_id)
    .bind(Utc::now())
    .fetch_optional(pool)
    .await
}

/// One licence by row id, with the app and plan joined — what the POST returns as its 201 body.
pub async fn license_by_id(
    pool: &PgPool,
    license_id: Uuid,
) -> Result<Option<LicenseRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{LICENSE_COLUMNS} WHERE l.id = $1"
    )))
    .bind(license_id)
    .fetch_optional(pool)
    .await
}

/// The columns `create_license` (:6956) writes for a user-licence mint.
pub struct NewUserLicence {
    pub subject_id: Uuid,
    pub app_id: Uuid,
    pub plan_id: Uuid,
    pub tenant_id: Uuid,
    pub region_id: Option<Uuid>,
    /// `None` means `datetime.now(UTC)` at insert time.
    pub starts_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    /// `license.metadata`; `None` stays NULL.
    pub metadata: Option<Value>,
}

/// Why a user-licence INSERT did not land.
#[derive(Debug, thiserror::Error)]
pub enum CreateLicenceError {
    /// The INSERT lost `uq_license_active_tenant_app_subject`. The handler re-reads the winner
    /// with [`find_active_license`] and returns it; this is never a 500.
    #[error("a concurrent writer won uq_license_active_tenant_app_subject")]
    LostRace,
    /// Any other database fault.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// `create_license` (:6956) for the user-licence route: INSERT `status='active'`.
pub async fn create_license(
    pool: &PgPool,
    row: NewUserLicence,
) -> Result<Uuid, CreateLicenceError> {
    let result: Result<Uuid, sqlx::Error> = sqlx::query_scalar(
        "INSERT INTO license (
             subject_id, app_id, plan_id, tenant_id, region_id, status, starts_at, expires_at, metadata)
         VALUES ($1, $2, $3, $4, $5, 'active', COALESCE($6, now()), $7, $8)
         RETURNING id",
    )
    .bind(row.subject_id)
    .bind(row.app_id)
    .bind(row.plan_id)
    .bind(row.tenant_id)
    .bind(row.region_id)
    .bind(row.starts_at)
    .bind(row.expires_at)
    .bind(&row.metadata)
    .fetch_one(pool)
    .await;
    match result {
        Ok(id) => Ok(id),
        Err(error) => {
            if is_active_licence_race(&error) {
                Err(CreateLicenceError::LostRace)
            } else {
                Err(CreateLicenceError::Database(error))
            }
        }
    }
}

/// `get_tenant`'s two scalars the mint guard reads: `None` = no tenants row; `Some(None)` = a
/// registered, un-archived tenant; `Some(Some(at))` = archived.
pub async fn tenant_archived_at(
    pool: &PgPool,
    tenant_id: Uuid,
) -> Result<Option<Option<DateTime<Utc>>>, sqlx::Error> {
    sqlx::query_scalar("SELECT archived_at FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .fetch_optional(pool)
        .await
}

/// `get_plan`'s revoked flag, or `None` when no plan row exists (an unknown plan is left to the
/// INSERT's foreign key, exactly as the reference leaves it to `create_license`).
pub async fn plan_revoked(pool: &PgPool, plan_id: Uuid) -> Result<Option<bool>, sqlx::Error> {
    sqlx::query_scalar("SELECT revoked FROM plan WHERE id = $1")
        .bind(plan_id)
        .fetch_optional(pool)
        .await
}

/// `tenant_has_licence_on_plan` (:7636): ANY row counts — revoked, expired or soft-deleted.
/// CommonPractice changes terms by revoking and re-minting on the same plan, so at the re-mint
/// the tenant may hold no active licence on it at all.
pub async fn tenant_has_licence_on_plan(
    pool: &PgPool,
    tenant_id: Uuid,
    app_id: Uuid,
    plan_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM license
              WHERE tenant_id = $1 AND app_id = $2 AND plan_id = $3
         )",
    )
    .bind(tenant_id)
    .bind(app_id)
    .bind(plan_id)
    .fetch_one(pool)
    .await
}

/// `subject_licence_delete_radius` (:8352): what a delete of this `(tenant, subject)` destroys.
///
/// `licenses` counts EVERY row (soft-deleted included), while `active_licences` excludes rows
/// already soft-deleted — a second delete against the same subject must not overstate its radius.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeleteRadius {
    pub licenses: i64,
    pub active_licences: i64,
    pub tokens: i64,
}

pub async fn subject_licence_delete_radius(
    pool: &PgPool,
    subject_id: Uuid,
    tenant_id: Uuid,
) -> Result<DeleteRadius, sqlx::Error> {
    let rows: Vec<(Uuid, String, Option<DateTime<Utc>>)> = sqlx::query_as(
        "SELECT id, status, deleted_at FROM license WHERE subject_id = $1 AND tenant_id = $2",
    )
    .bind(subject_id)
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;
    let ids: Vec<Uuid> = rows.iter().map(|(id, _, _)| *id).collect();
    let active_licences = rows
        .iter()
        .filter(|(_, status, deleted_at)| status == "active" && deleted_at.is_none())
        .count();
    let tokens = if ids.is_empty() {
        0
    } else {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM token WHERE license_id = ANY($1)")
            .bind(ids.as_slice())
            .fetch_one(pool)
            .await?
    };
    Ok(DeleteRadius {
        licenses: i64::try_from(rows.len()).unwrap_or(i64::MAX),
        active_licences: i64::try_from(active_licences).unwrap_or(i64::MAX),
        tokens,
    })
}

/// `delete_licenses_for_subject` (:8396): the `LICENSE_SOFT_DELETE=false` path. Tokens cascade
/// through `token.license_id ON DELETE CASCADE`. Returns the number of licences destroyed.
pub async fn delete_licenses_for_subject(
    pool: &PgPool,
    subject_id: Uuid,
    tenant_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let deleted = sqlx::query("DELETE FROM license WHERE subject_id = $1 AND tenant_id = $2")
        .bind(subject_id)
        .bind(tenant_id)
        .execute(pool)
        .await?;
    Ok(deleted.rows_affected())
}

/// What a soft delete wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoftDelete {
    /// Licences stamped (rows already soft-deleted are skipped, never re-stamped).
    pub count: i64,
    /// Dependency licences released in the same transaction because no remaining active licence
    /// requires them.
    pub released: Vec<Uuid>,
}

/// `soft_delete_licenses_for_subject` (:8420): retain and stamp the rows, revoke their tokens and
/// release orphaned dependency licences, all in ONE transaction.
///
/// ⛔ ALL FOUR STAMPS MATTER: `deleted_at` (the instant `effectively_active_clauses` reads),
/// `deleted_from_status` (the status the row HELD), `status='revoked'` (a deleted licence must
/// stop authorizing AND release the partial unique index), and `deleted_by`/`delete_reason` (the
/// audit — this service has no audit table, so the retained row IS the record).
///
/// ⚠️ ALREADY-SOFT-DELETED ROWS ARE SKIPPED (`deleted_at IS NULL`): the first deletion stays the
/// first, so a repeated call cannot walk the timestamp forward and rewrite a closed period.
///
/// ⚠️ TOKENS ARE REVOKED EXPLICITLY: the row survives, so the FK cascade no longer fires; without
/// this a "deleted" licence would keep authorizing calls.
///
/// `release` is the reference's `released_out is not None`. When the tenant's bundle-ensure
/// lock cannot be taken, the reference logs `DEPENDENCY_RELEASE_DEFERRED` and still deletes; this
/// port restarts the transaction without the release, which is the same row effect.
pub async fn soft_delete_licenses_for_subject(
    pool: &PgPool,
    subject_id: Uuid,
    tenant_id: Uuid,
    actor: &str,
    reason: Option<&str>,
    release: bool,
) -> Result<SoftDelete, sqlx::Error> {
    let stamp = Utc::now();
    let mut tx: Transaction<'_, Postgres> = pool.begin().await?;
    let mut release = release;
    if release
        && !crate::db::dependencies::lock_tenant_for_dependency_release(&mut tx, tenant_id).await?
    {
        // The failed advisory-lock wait aborted this transaction; nothing has been written.
        tx.rollback().await?;
        tracing::error!(
            tenant_id = %tenant_id,
            subject_id = %subject_id,
            actor = %actor,
            "DEPENDENCY_RELEASE_DEFERRED: a bundle ensure held the tenant lock past {}; the \
             licences are deleted, orphaned dependency licences are NOT yet released (re-delete \
             to release)",
            crate::db::dependencies::DEPENDENCY_RELEASE_LOCK_TIMEOUT
        );
        release = false;
        tx = pool.begin().await?;
    }

    let live: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM license
          WHERE subject_id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
    )
    .bind(subject_id)
    .bind(tenant_id)
    .fetch_all(&mut *tx)
    .await?;
    if live.is_empty() {
        tx.commit().await?;
        return Ok(SoftDelete {
            count: 0,
            released: Vec::new(),
        });
    }

    // One UPDATE instead of the reference's per-row loop: Postgres evaluates every SET expression
    // against the OLD row, so `deleted_from_status` and the `COALESCE(revoked_at, …)` preserve an
    // EARLIER revocation exactly as the ORM's `if lic.revoked_at is None` does.
    sqlx::query(
        "UPDATE license
            SET deleted_from_status = status,
                deleted_at = $3,
                deleted_by = $4,
                delete_reason = $5,
                status = 'revoked',
                revoked_at = COALESCE(revoked_at, $3),
                updated_at = $3
          WHERE subject_id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
    )
    .bind(subject_id)
    .bind(tenant_id)
    .bind(stamp)
    .bind(actor)
    .bind(reason)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE token SET revoked = true, revoked_at = $2
          WHERE license_id = ANY($1) AND revoked = false",
    )
    .bind(live.as_slice())
    .bind(stamp)
    .execute(&mut *tx)
    .await?;

    let released = if release {
        // The reference passes the trigger as the string `subject:<id>`; this port's shared
        // dependency-release contract takes a licence UUID, so the SUBJECT id is recorded in
        // `metadata.released_by_revoke_of`. Row effects and the returned id list are identical.
        crate::db::dependencies::release_orphaned_dependencies(
            &mut tx, tenant_id, None, subject_id, actor, stamp,
        )
        .await?
    } else {
        Vec::new()
    };
    tx.commit().await?;
    Ok(SoftDelete {
        count: i64::try_from(live.len()).unwrap_or(i64::MAX),
        released,
    })
}

/// SQLSTATE `23505` on the active-licence partial unique index.
fn is_active_licence_race(error: &sqlx::Error) -> bool {
    error.as_database_error().is_some_and(|database| {
        database.is_unique_violation()
            && database.constraint() == Some(UQ_ACTIVE_TENANT_APP_SUBJECT)
    })
}
