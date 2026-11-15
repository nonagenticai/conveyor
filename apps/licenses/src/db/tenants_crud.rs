//! The tenant registry: the eight CRUD/settings operations of issue #45, slice 45.
//!
//! Ported from the pinned reference (commit `4e8a08d`):
//!
//! | Rust | reference |
//! |---|---|
//! | [`list_tenants`] | `models.py::DatabaseManager.list_tenants` |
//! | [`get_tenant`] / [`resolve_scoped`] | `models.py::get_tenant` / `get_tenant_scoped` |
//! | [`create_tenant`] | `models.py::create_tenant` |
//! | [`ensure_tenant`] | `models.py::ensure_tenant` |
//! | [`rename_tenant`] | `models.py::rename_tenant` |
//! | [`set_tenant_archived`] | `models.py::set_tenant_archived` |
//! | [`set_tenant_budget_currency`] | `models.py::set_tenant_budget_currency` |
//! | [`set_tenant_cost_centre`] | `models.py::set_tenant_cost_centre` |
//!
//! The predicates are the reference's, not improvements on them:
//!
//! - names are unique per OWNER among LIVE tenants only (migrations 050/053): a name
//!   resolves to a live row of the caller's owner, never to an archived one and never to
//!   another owner's row. A bare name matching live tenants of several owners is
//!   AMBIGUOUS (409 for a cross-owner caller), never a pick;
//! - `owner_key` scopes a listing to what that owner may ACT ON — its own tenants plus
//!   tenants it holds an ACTIVE (`revoked_at IS NULL`) grant on. Visibility and action
//!   must not diverge, so this is the same predicate `owner_can_access_tenant` states;
//! - archive/restore writes `archived_at` only: no licence, token, budget or grant row is
//!   touched. Un-archive refuses when a LIVE tenant of the same owner took the archived
//!   row's name or `external_ref` in the meantime (both are unique among live rows);
//! - `budget_currency` moves null -> a currency freely, currency -> a DIFFERENT currency
//!   only with `allow_change`, and never back to null; an archived tenant is refused;
//! - creating a tenant inserts the `platform` grant in the SAME transaction
//!   (`PLATFORM_GRANT_BY`), so a consumer never observes a tenant without it.
//!
//! `_ensure_owner_resolved` has no analogue: the middleware attaches `owner_key` from the
//! verified `(realm, azp)` before any handler runs (see `db::issuance`).

use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::auth::User;
use crate::db::issuance::require_known_owner;
use crate::db::owners::owner_can_access_tenant;
use crate::error::Refusal;

/// `models.TENANT_KINDS` / `ck_tenants_kind`.
pub const TENANT_KINDS: [&str; 5] = ["customer", "internal", "service", "test", "unclassified"];
/// `models.BUDGET_CURRENCIES` / `ck_tenants_budget_currency`.
pub const BUDGET_CURRENCIES: [&str; 3] = ["EUR", "USD", "CAD"];
/// `models.COST_CENTRES` / `ck_tenants_cost_centre`. A CLOSED vocabulary: this is a
/// join key, and an unlisted value cannot be told apart from a typo.
pub const COST_CENTRES: [&str; 6] = [
    "service",
    "tests",
    "customers",
    "ci",
    "improvement",
    "merqi",
];
/// `models.PLATFORM_OWNER` — never granted its own tenants (it owns them).
pub const PLATFORM_OWNER: &str = "platform";
/// `models.PLATFORM_GRANT_BY` — the `granted_by` on the platform's automatic grant.
pub const PLATFORM_GRANT_BY: &str = "platform-infrastructure";

/// One `tenants` row, in the reference's `Tenant.to_dict` terms.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TenantRow {
    pub id: Uuid,
    pub name: String,
    pub keycloak_realm: Option<String>,
    pub byo_mode: String,
    pub archived_at: Option<DateTime<Utc>>,
    pub kind: String,
    pub cost_centre: Option<String>,
    /// `char(3)`; cast to text so the value arrives unpadded.
    pub budget_currency: Option<String>,
    pub owner_key: String,
    pub external_ref: Option<String>,
    pub created_at: DateTime<Utc>,
}

const SELECT_TENANT: &str = "SELECT t.id, t.name, t.keycloak_realm, t.byo_mode, t.archived_at, \
     t.kind, t.cost_centre, t.budget_currency::text AS budget_currency, t.owner_key, \
     t.external_ref, t.created_at FROM tenants t";

const RETURNING_TENANT: &str = "RETURNING id, name, keycloak_realm, byo_mode, archived_at, kind, \
     cost_centre, budget_currency::text AS budget_currency, owner_key, external_ref, created_at";

/// The reference's `_TENANT_NAME_SHAPE`, decided by hand rather than by a regex crate:
/// lowercase letters, digits and hyphens, and it may not start with a hyphen.
pub fn valid_tenant_name(name: &str) -> bool {
    let mut chars = name.chars();
    let first_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    first_ok
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// `DatabaseManager.list_tenants` — newest first.
///
/// Archived tenants and `kind='service'` principals are excluded by default; `owner_key`
/// scopes to owned + actively granted rows (the same predicate `owner_can_access_tenant`
/// states, kept in step deliberately).
pub async fn list_tenants(
    pool: &PgPool,
    include_archived: bool,
    include_service_actors: bool,
    owner_key: Option<&str>,
) -> Result<Vec<TenantRow>, sqlx::Error> {
    let sql = format!(
        "{SELECT_TENANT}
          WHERE ($3::text IS NULL OR t.owner_key = $3 OR EXISTS (
                    SELECT 1 FROM tenant_app_grant g
                     WHERE g.tenant_id = t.id
                       AND g.grantee_owner_key = $3
                       AND g.revoked_at IS NULL))
            AND ($1::boolean OR t.archived_at IS NULL)
            AND ($2::boolean OR t.kind <> 'service')
          ORDER BY t.created_at DESC"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(include_archived)
        .bind(include_service_actors)
        .bind(owner_key)
        .fetch_all(pool)
        .await
}

/// `DatabaseManager.get_tenant` by id — archived rows included (a name is what never
/// reaches an archived row, not an id).
pub async fn get_tenant(pool: &PgPool, id: Uuid) -> Result<Option<TenantRow>, sqlx::Error> {
    let sql = format!("{SELECT_TENANT} WHERE t.id = $1");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// `get_tenant_by_external_ref` — the owner's LIVE tenant carrying `external_ref`.
/// An archived tenant never satisfies an idempotent retry (migration 053).
pub async fn get_tenant_by_external_ref(
    pool: &PgPool,
    owner_key: &str,
    external_ref: &str,
) -> Result<Option<TenantRow>, sqlx::Error> {
    let sql = format!(
        "{SELECT_TENANT}
          WHERE t.owner_key = $1 AND t.external_ref = $2 AND t.archived_at IS NULL"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(owner_key)
        .bind(external_ref)
        .fetch_optional(pool)
        .await
}

/// The three outcomes of an owner-scoped name/id resolution.
///
/// Variant-size asymmetry is deliberate: the row is 232 bytes but this value is built once
/// per request and consumed immediately, so boxing it would only add derefs.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Scoped {
    Found(TenantRow),
    NotFound,
    /// A NAME matching LIVE tenants of several owners (cross-owner callers only).
    Ambiguous,
}

/// `DatabaseManager.get_tenant_scoped` (migrations 050/053).
///
/// * by id: own row, or (`allow_grant`) a row carrying the caller's ACTIVE grant; anything
///   else `NotFound` — never 403, because existence must not leak;
/// * by name: only within the caller's own owner, and only a LIVE row;
/// * `cross_owner`: any row by id; by name only when the name is unique across owners,
///   else [`Scoped::Ambiguous`].
pub async fn resolve_scoped(
    pool: &PgPool,
    name_or_id: &str,
    owner_key: Option<&str>,
    cross_owner: bool,
    allow_grant: bool,
) -> Result<Scoped, sqlx::Error> {
    if let Ok(id) = Uuid::try_parse(name_or_id) {
        let Some(tenant) = get_tenant(pool, id).await? else {
            return Ok(Scoped::NotFound);
        };
        if cross_owner {
            return Ok(Scoped::Found(tenant));
        }
        let Some(owner) = owner_key else {
            return Ok(Scoped::NotFound);
        };
        if tenant.owner_key == owner {
            return Ok(Scoped::Found(tenant));
        }
        if allow_grant && owner_can_access_tenant(pool, owner, name_or_id).await? {
            return Ok(Scoped::Found(tenant));
        }
        return Ok(Scoped::NotFound);
    }
    if cross_owner {
        let sql = format!("{SELECT_TENANT} WHERE t.name = $1 AND t.archived_at IS NULL LIMIT 2");
        let rows: Vec<TenantRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(name_or_id)
            .fetch_all(pool)
            .await?;
        return Ok(match rows.len() {
            0 => Scoped::NotFound,
            1 => Scoped::Found(rows.into_iter().next().expect("one row")),
            _ => Scoped::Ambiguous,
        });
    }
    let Some(owner) = owner_key else {
        return Ok(Scoped::NotFound);
    };
    let live = live_tenant_by_name(pool, name_or_id, owner).await?;
    Ok(match live {
        Some(row) => Scoped::Found(row),
        None => Scoped::NotFound,
    })
}

/// `_tenant_by_name` scoped to one owner: LIVE only (unique by `uq_tenants_owner_name`).
///
/// Generic over the executor so the archive pre-check runs on the SAME row-locking
/// transaction as its write.
async fn live_tenant_by_name<'e, E>(
    executor: E,
    name: &str,
    owner_key: &str,
) -> Result<Option<TenantRow>, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    let sql = format!(
        "{SELECT_TENANT}
          WHERE t.name = $1 AND t.archived_at IS NULL AND t.owner_key = $2"
    );
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(name)
        .bind(owner_key)
        .fetch_optional(executor)
        .await
}

fn ambiguous_name(selector: &str) -> Refusal {
    Refusal::detail(
        StatusCode::CONFLICT,
        json!({
            "error": "tenant_name_ambiguous",
            "name": selector,
            "message": "this tenant name is used by several owning apps; use the id",
        }),
    )
}

fn tenant_not_found() -> Refusal {
    Refusal::detail(StatusCode::NOT_FOUND, "Tenant not found")
}

fn database_fault(error: sqlx::Error, operation: &'static str) -> Refusal {
    tracing::error!(%error, operation, "tenant database fault");
    Refusal::detail(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
}

/// `_audit_cross_owner`: this service has no audit table, the log line IS the record.
pub(crate) fn audit_cross_owner(user: &User, action: &str, tenant: &TenantRow) {
    tracing::warn!(
        action = %action,
        tenant = %tenant.id,
        tenant_owner = %tenant.owner_key,
        caller_owner = ?user.owner_key,
        azp = ?user.azp,
        realm = ?user.realm(),
        sub = %user.sub,
        at = %Utc::now().to_rfc3339(),
        "cross_owner_action"
    );
}

/// `main.py::_resolve_owned_tenant` — the OWNER-ONLY write resolution for tenant-wide
/// settings. A grantee resolves (by id) and is then refused 403 `tenant_owner_only`;
/// another owner's tenant is 404; `cross_owner` is admitted, audited.
pub async fn resolve_owned_tenant(
    pool: &PgPool,
    user: &User,
    name_or_id: &str,
    action: &str,
) -> Result<TenantRow, Refusal> {
    let owner = require_known_owner(user)?;
    let tenant = resolve_scoped(pool, name_or_id, Some(&owner), user.cross_owner, true)
        .await
        .map_err(|error| database_fault(error, "get_tenant_scoped"))?;
    let tenant = match tenant {
        Scoped::Found(tenant) => tenant,
        Scoped::NotFound => return Err(tenant_not_found()),
        Scoped::Ambiguous => return Err(ambiguous_name(name_or_id)),
    };
    if tenant.owner_key != owner {
        if user.cross_owner {
            audit_cross_owner(user, action, &tenant);
            return Ok(tenant);
        }
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            json!({
                "error": "tenant_owner_only",
                "message": "only the tenant's owning app may do this; a grant does not",
            }),
        ));
    }
    Ok(tenant)
}

/// `main.py::_resolve_tenant_for_caller` for the owned-client (`license-admin`) arm.
pub async fn resolve_for_licensed_admin(
    pool: &PgPool,
    user: &User,
    name_or_id: &str,
    action: &str,
) -> Result<TenantRow, Refusal> {
    let tenant = resolve_scoped(
        pool,
        name_or_id,
        user.owner_key.as_deref(),
        user.cross_owner,
        true,
    )
    .await
    .map_err(|error| database_fault(error, "get_tenant_scoped"))?;
    match tenant {
        Scoped::Found(tenant) => {
            if user.cross_owner && Some(tenant.owner_key.as_str()) != user.owner_key.as_deref() {
                audit_cross_owner(user, action, &tenant);
            }
            Ok(tenant)
        }
        Scoped::NotFound => Err(tenant_not_found()),
        Scoped::Ambiguous => Err(ambiguous_name(name_or_id)),
    }
}

/// The non-admin half of `_read_tenant_for_caller`: its OWN tenant only, 404 otherwise.
/// A foreign tenant is indistinguishable from an absent one — that is the point.
pub async fn read_own_tenant(
    pool: &PgPool,
    caller_tenant: &str,
    name_or_id: &str,
) -> Result<TenantRow, Refusal> {
    let caller = Uuid::try_parse(caller_tenant)
        .map_err(|_| Refusal::detail(StatusCode::UNAUTHORIZED, "Missing tenant context"))?;
    let tenant = match Uuid::try_parse(name_or_id) {
        Ok(id) => get_tenant(pool, id)
            .await
            .map_err(|error| database_fault(error, "get_tenant"))?,
        Err(_) => {
            let own = get_tenant(pool, caller)
                .await
                .map_err(|error| database_fault(error, "get_tenant"))?;
            own.filter(|row| row.name == name_or_id)
        }
    };
    match tenant {
        Some(row) if row.id == caller => Ok(row),
        _ => Err(tenant_not_found()),
    }
}

/// A write-layer refusal, mapped by each handler to its own route semantics.
#[derive(Debug, thiserror::Error)]
pub enum TenantWriteError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    /// The caller's owner already holds this name on a LIVE tenant.
    #[error("tenant name already in use")]
    NameTaken,
    /// A LIVE tenant of the same owner already holds this `external_ref`.
    #[error("tenant external_ref already in use")]
    ExternalRefTaken,
    /// `_require_owner_row`: the named owner is not a `tenant_owner` key.
    #[error("unknown tenant owner: not a tenant_owner key")]
    UnknownOwner,
    /// `ensure_tenant`'s principal-id guard (a tenant id may not be a Keycloak subject).
    #[error("{0}")]
    PrincipalId(String),
    /// A value outside a closed vocabulary, refused by the DAL's own validation.
    #[error("{0}")]
    InvalidValue(String),
    /// The DAL's own name check (unreachable through a Pydantic-validated body).
    #[error("invalid tenant name")]
    InvalidName,
    /// A cost centre outside the closed vocabulary.
    #[error("{0}")]
    InvalidCostCentre(String),
}

fn unique_violation(error: &sqlx::Error) -> Option<&str> {
    let database = error.as_database_error()?;
    if database.code().as_deref() != Some("23505") {
        return None;
    }
    database.constraint()
}

/// `_require_owner_row` — a named owner must exist (`tenant_owner.key`).
async fn require_owner_row(pool: &PgPool, owner_key: &str) -> Result<(), TenantWriteError> {
    let found: Option<String> = sqlx::query_scalar("SELECT key FROM tenant_owner WHERE key = $1")
        .bind(owner_key)
        .fetch_optional(pool)
        .await?;
    match found {
        Some(_) => Ok(()),
        None => Err(TenantWriteError::UnknownOwner),
    }
}

/// `_add_platform_grant`: every tenant owned by another app carries an ACTIVE grant to
/// `platform`, in the SAME transaction as the tenant row.
async fn add_platform_grant(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    owner_key: &str,
) -> Result<(), sqlx::Error> {
    if owner_key == PLATFORM_OWNER {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO tenant_app_grant (tenant_id, grantee_owner_key, granted_by)
         VALUES ($1, $2, $3)",
    )
    .bind(tenant_id)
    .bind(PLATFORM_OWNER)
    .bind(PLATFORM_GRANT_BY)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// The inputs of `create_tenant`, named rather than seven positional arguments.
pub struct NewTenant<'a> {
    pub name: &'a str,
    pub keycloak_realm: Option<&'a str>,
    pub cost_centre: Option<&'a str>,
    pub budget_currency: Option<&'a str>,
    pub owner_key: &'a str,
    pub external_ref: Option<&'a str>,
}

/// `DatabaseManager.create_tenant` — mints a UUIDv7 and the platform grant atomically.
///
/// A unique violation means either a concurrent create won on `(owner, external_ref)`
/// (the retry-safe path: the route re-reads and answers 200) or the name is taken within
/// this owner (409). It is never another owner's row — names are unique per owner.
pub async fn create_tenant(
    pool: &PgPool,
    new: &NewTenant<'_>,
) -> Result<TenantRow, TenantWriteError> {
    if let Some(currency) = new.budget_currency
        && !BUDGET_CURRENCIES.contains(&currency)
    {
        return Err(TenantWriteError::InvalidValue(format!(
            "invalid budget_currency: {currency:?}"
        )));
    }
    require_owner_row(pool, new.owner_key).await?;
    let id = Uuid::now_v7();
    let mut tx = pool.begin().await?;
    let inserted = sqlx::query(
        "INSERT INTO tenants
             (id, name, keycloak_realm, owner_key, external_ref, cost_centre,
              budget_currency, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, now())",
    )
    .bind(id)
    .bind(new.name)
    .bind(new.keycloak_realm)
    .bind(new.owner_key)
    .bind(new.external_ref)
    .bind(new.cost_centre)
    .bind(new.budget_currency)
    .execute(&mut *tx)
    .await;
    if let Err(error) = inserted {
        let _ = tx.rollback().await;
        // A unique index fired: either a concurrent create with the same external_ref
        // won (idempotent -> return it) or the name is taken within this owner (409).
        // Never another owner's row.
        if unique_violation(&error).is_some() {
            if let Some(external_ref) = new.external_ref
                && let Some(prior) =
                    get_tenant_by_external_ref(pool, new.owner_key, external_ref).await?
            {
                return Ok(prior);
            }
            if live_tenant_by_name(pool, new.name, new.owner_key)
                .await?
                .is_some()
            {
                return Err(TenantWriteError::NameTaken);
            }
        }
        return Err(TenantWriteError::Database(error));
    }
    add_platform_grant(&mut tx, id, new.owner_key).await?;
    tx.commit().await?;
    let sql = format!("{SELECT_TENANT} WHERE t.id = $1");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(TenantWriteError::Database(sqlx::Error::RowNotFound))
}

/// `DatabaseManager.ensure_tenant` with the route's inputs (tenant-wide BYO mode is not
/// part of this body; a `kind='service'` row is still born `local_allowed`, migration 033).
pub struct EnsureTenant<'a> {
    pub id: Uuid,
    pub name: &'a str,
    pub keycloak_realm: Option<&'a str>,
    pub kind: Option<&'a str>,
    pub cost_centre: Option<&'a str>,
    pub budget_currency: Option<&'a str>,
    pub owner_key: &'a str,
}

/// The reference's principal-id guard (`services.tenant_subject_invariant`): a tenant is an
/// ORGANISATION, never an identity that authenticates.
async fn refuse_principal_id(pool: &PgPool, id: Uuid) -> Result<(), TenantWriteError> {
    let candidate = id.to_string();
    let hit: Option<String> = sqlx::query_scalar(
        "SELECT match_kind FROM (
             SELECT 'keycloak_sub' AS match_kind FROM subject s
              WHERE lower(s.keycloak_sub) = lower($1)
             UNION ALL
             SELECT 'subject_id' AS match_kind FROM subject s
              WHERE lower(s.id::text) = lower($1)
         ) hit ORDER BY match_kind LIMIT 1",
    )
    .bind(&candidate)
    .fetch_optional(pool)
    .await?;
    match hit {
        None => Ok(()),
        Some(match_kind) => Err(TenantWriteError::PrincipalId(format!(
            "cannot register tenant {id}: that UUID is already an existing Keycloak \
             principal id via subject.{match_kind}. A tenant is an ORGANISATION and a \
             subject is an identity that authenticates within one; registering a principal \
             id as a tenant makes the authenticating identity the thing that pays, which \
             nothing downstream can detect. Register the ORGANISATION's id instead, or \
             attach the subject to a tenant via subject.tenant_id."
        ))),
    }
}

/// `DatabaseManager.ensure_tenant`: register an explicit id, idempotently.
pub async fn ensure_tenant(
    pool: &PgPool,
    new: &EnsureTenant<'_>,
) -> Result<TenantRow, TenantWriteError> {
    if let Some(kind) = new.kind
        && !TENANT_KINDS.contains(&kind)
    {
        return Err(TenantWriteError::InvalidValue(format!(
            "invalid kind: {kind:?}"
        )));
    }
    if let Some(currency) = new.budget_currency
        && !BUDGET_CURRENCIES.contains(&currency)
    {
        return Err(TenantWriteError::InvalidValue(format!(
            "invalid budget_currency: {currency:?}"
        )));
    }
    if let Some(existing) = get_tenant(pool, new.id).await? {
        // Idempotent: an existing id is returned UNCHANGED whatever its owner; the route
        // is what compares owners and 409s `tenant_owned_by_other_app`.
        return Ok(existing);
    }
    refuse_principal_id(pool, new.id).await?;
    require_owner_row(pool, new.owner_key).await?;
    if live_tenant_by_name(pool, new.name, new.owner_key)
        .await?
        .is_some()
    {
        return Err(TenantWriteError::NameTaken);
    }
    let kind = new.kind.unwrap_or("unclassified");
    // `byo_mode` is INSERT-only and not in this body. The captured column default is
    // 'byo_required'; a `kind='service'` row is born 'local_allowed' (migration 033).
    let byo_mode = if kind == "service" {
        "local_allowed"
    } else {
        "byo_required"
    };
    let mut tx = pool.begin().await?;
    let inserted = sqlx::query(
        "INSERT INTO tenants
             (id, name, keycloak_realm, owner_key, kind, byo_mode, cost_centre,
              budget_currency, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now())",
    )
    .bind(new.id)
    .bind(new.name)
    .bind(new.keycloak_realm)
    .bind(new.owner_key)
    .bind(kind)
    .bind(byo_mode)
    .bind(new.cost_centre)
    .bind(new.budget_currency)
    .execute(&mut *tx)
    .await;
    if let Err(error) = inserted {
        let _ = tx.rollback().await;
        // Lost a race on the id (return the winner) or on the live per-owner name (409).
        if unique_violation(&error).is_some() {
            if let Some(won) = get_tenant(pool, new.id).await? {
                return Ok(won);
            }
            if live_tenant_by_name(pool, new.name, new.owner_key)
                .await?
                .is_some()
            {
                return Err(TenantWriteError::NameTaken);
            }
        }
        return Err(TenantWriteError::Database(error));
    }
    add_platform_grant(&mut tx, new.id, new.owner_key).await?;
    tx.commit().await?;
    let sql = format!("{SELECT_TENANT} WHERE t.id = $1");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(new.id)
        .fetch_optional(pool)
        .await?
        .ok_or(TenantWriteError::Database(sqlx::Error::RowNotFound))
}

/// The result of a rename: the row, or the reference's `None` (404).
#[allow(clippy::large_enum_variant)]
pub enum Mutated {
    Row(TenantRow),
    NotFound,
}

/// `DatabaseManager.rename_tenant` — a LABEL change; the UUID never moves.
///
/// Idempotent: renaming to the current name returns the row untouched. A name held by
/// ANOTHER owner is not a clash (names are unique per owner since migration 050).
pub async fn rename_tenant(
    pool: &PgPool,
    tenant_id: Uuid,
    new_name: &str,
) -> Result<Mutated, TenantWriteError> {
    if !valid_tenant_name(new_name) {
        return Err(TenantWriteError::InvalidName);
    }
    let Some(tenant) = get_tenant(pool, tenant_id).await? else {
        return Ok(Mutated::NotFound);
    };
    if tenant.name == new_name {
        return Ok(Mutated::Row(tenant));
    }
    if live_tenant_by_name(pool, new_name, &tenant.owner_key)
        .await?
        .is_some()
    {
        return Err(TenantWriteError::NameTaken);
    }
    let sql = format!("UPDATE tenants SET name = $2 WHERE id = $1 {RETURNING_TENANT}");
    let updated: Option<TenantRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant_id)
        .bind(new_name)
        .fetch_optional(pool)
        .await
        .map_err(|error| match unique_violation(&error) {
            // The pre-check lost a race; the unique index is the arbiter (SQLSTATE 23505).
            Some(_) => TenantWriteError::NameTaken,
            None => TenantWriteError::Database(error),
        })?;
    match updated {
        Some(row) => Ok(Mutated::Row(row)),
        None => Ok(Mutated::NotFound),
    }
}

/// `DatabaseManager.set_tenant_archived` — retire or restore, `archived_at` only.
///
/// Idempotent both ways: re-archiving keeps the ORIGINAL timestamp (it answers "when was
/// this retired"), and un-archiving a live tenant is a no-op. Restoring refuses when a
/// LIVE tenant of the same owner now holds the archived row's name or `external_ref`
/// (migration 053) — from the pre-check under the row lock AND from the unique index.
pub async fn set_tenant_archived(
    pool: &PgPool,
    tenant_id: Uuid,
    archived: bool,
) -> Result<Mutated, TenantWriteError> {
    let mut tx = pool.begin().await?;
    let sql = format!("{SELECT_TENANT} WHERE t.id = $1 FOR UPDATE OF t");
    let Some(tenant): Option<TenantRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await?
    else {
        let _ = tx.rollback().await;
        return Ok(Mutated::NotFound);
    };
    let restoring = !archived && tenant.archived_at.is_some();
    if restoring {
        if let Some(clash) = live_tenant_by_name(&mut *tx, &tenant.name, &tenant.owner_key).await?
            && clash.id != tenant.id
        {
            let _ = tx.rollback().await;
            return Err(TenantWriteError::NameTaken);
        }
        if let Some(external_ref) = tenant.external_ref.as_deref() {
            let ref_clash: Option<Uuid> = sqlx::query_scalar(
                "SELECT id FROM tenants
                  WHERE owner_key = $1 AND external_ref = $2
                    AND archived_at IS NULL AND id <> $3",
            )
            .bind(&tenant.owner_key)
            .bind(external_ref)
            .bind(tenant.id)
            .fetch_optional(&mut *tx)
            .await?;
            if ref_clash.is_some() {
                let _ = tx.rollback().await;
                return Err(TenantWriteError::ExternalRefTaken);
            }
        }
    }
    let updated: Option<TenantRow> = if archived && tenant.archived_at.is_none() {
        let sql =
            format!("UPDATE tenants SET archived_at = now() WHERE id = $1 {RETURNING_TENANT}");
        sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(tenant.id)
            .fetch_optional(&mut *tx)
            .await?
    } else if !archived {
        let sql = format!("UPDATE tenants SET archived_at = NULL WHERE id = $1 {RETURNING_TENANT}");
        sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(tenant.id)
            .fetch_optional(&mut *tx)
            .await?
    } else {
        Some(tenant)
    };
    let outcome = match tx.commit().await {
        Ok(()) => updated,
        Err(error) => {
            return Err(match unique_violation(&error) {
                Some("uq_tenants_owner_external_ref") if restoring => {
                    TenantWriteError::ExternalRefTaken
                }
                Some(_) if restoring => TenantWriteError::NameTaken,
                _ => TenantWriteError::Database(error),
            });
        }
    };
    match outcome {
        Some(row) => Ok(Mutated::Row(row)),
        None => Ok(Mutated::NotFound),
    }
}

/// `DatabaseManager.set_tenant_budget_currency`'s refusals.
#[derive(Debug, thiserror::Error)]
pub enum BudgetCurrencyError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("tenant is archived")]
    Archived,
    #[error("budget_currency is immutable")]
    Immutable,
}

/// `DatabaseManager.set_tenant_budget_currency` (migration 043).
///
/// * null -> a currency: any caller;
/// * the value the row already holds: an idempotent no-op, nothing written or audited;
/// * a currency -> a DIFFERENT currency: only with `allow_change`;
/// * a currency -> null: impossible from this route (the body has no null), and refused
///   regardless — no token budgets were pushed for a money tenant;
/// * an ARCHIVED tenant: refused even when it already holds the value.
///
/// Every actual write is audited by one `budget_currency_changed` line (WARNING).
pub async fn set_tenant_budget_currency(
    pool: &PgPool,
    tenant_id: Uuid,
    budget_currency: &str,
    actor: &str,
    via: Option<&str>,
    allow_change: bool,
) -> Result<Option<TenantRow>, BudgetCurrencyError> {
    if !BUDGET_CURRENCIES.contains(&budget_currency) {
        return Err(BudgetCurrencyError::Immutable);
    }
    let mut tx = pool.begin().await?;
    let sql = format!("{SELECT_TENANT} WHERE t.id = $1 FOR UPDATE OF t");
    let Some(tenant): Option<TenantRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await?
    else {
        let _ = tx.rollback().await;
        return Ok(None);
    };
    if tenant.archived_at.is_some() {
        let _ = tx.rollback().await;
        return Err(BudgetCurrencyError::Archived);
    }
    let current = tenant.budget_currency.clone();
    if current.as_deref() == Some(budget_currency) {
        tx.commit().await?;
        return Ok(Some(tenant));
    }
    if current.is_some() && !allow_change {
        let _ = tx.rollback().await;
        return Err(BudgetCurrencyError::Immutable);
    }
    let sql = format!("UPDATE tenants SET budget_currency = $2 WHERE id = $1 {RETURNING_TENANT}");
    let updated: Option<TenantRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant_id)
        .bind(budget_currency)
        .fetch_optional(&mut *tx)
        .await?;
    tx.commit().await?;
    if let Some(row) = &updated {
        tracing::warn!(
            "budget_currency_changed: tenant={} name={} from={:?} to={} actor={} via={:?} at={}",
            row.id,
            row.name,
            current,
            budget_currency,
            actor,
            via,
            Utc::now().to_rfc3339()
        );
    }
    Ok(updated)
}

/// `DatabaseManager.set_tenant_cost_centre` — UPDATE of ONE column, `None` CLEARS it.
///
/// The vocabulary is validated before the write (the route's body only bounds the length):
/// `None` is a deliberate state ("apportionment undecided"), not a malformed request.
pub async fn set_tenant_cost_centre(
    pool: &PgPool,
    tenant_id: Uuid,
    cost_centre: Option<&str>,
) -> Result<Option<TenantRow>, TenantWriteError> {
    if let Some(centre) = cost_centre
        && !COST_CENTRES.contains(&centre)
    {
        return Err(TenantWriteError::InvalidCostCentre(format!(
            "invalid cost_centre: {centre:?} (expected one of {}, or null to clear)",
            COST_CENTRES.join(", ")
        )));
    }
    let sql = format!("UPDATE tenants SET cost_centre = $2 WHERE id = $1 {RETURNING_TENANT}");
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant_id)
        .bind(cost_centre)
        .fetch_optional(pool)
        .await?)
}
