//! Tenant access grants and per-tenant offering provisioning: six of the nineteen tenant
//! operations (#45).
//!
//! Ported from the pinned reference (commit `4e8a08d`):
//!
//! | Rust | reference |
//! |---|---|
//! | [`list_tenant_grants`] / [`put_tenant_grant`] / [`revoke_tenant_grant`] | `models.py:5666-5724`, `main.py:9254-9351` |
//! | [`list_offerings`] | `models.py:6200`, `main.py:9504` |
//! | [`resolve_tenant_for_app_setting`] | `main.py:710` (the per-app write guard) |
//! | [`authorize_app_admin`] | `main.py:874` |
//! | [`set_tenant_app_byo_mode`] / [`clear_tenant_app_byo_mode`] | `models.py:6111`, `models.py:6305` |
//! | [`get_offering`] | `models.py:6405` |
//! | [`provision_tenant_offering`] | `main.py:8458-8621` |
//! | [`unprovision_tenant_offering`] | `main.py:8631-8661` |
//! | the billing push | `main.py:8232-8458` (`_provision_offering_markup`, `_provision_access_charge`) |
//!
//! The predicates are the reference's, not improvements on them:
//!
//! - a grant is idempotent on the PARTIAL unique index
//!   `uq_tenant_app_grant_active (tenant_id, grantee_owner_key) WHERE revoked_at IS NULL`, so a
//!   revoked grant never blocks a fresh active one; revoking stamps `revoked_at` and keeps the
//!   history row;
//! - grants are OWNER-ONLY (`_resolve_owned_tenant`): a grantee gets 403 `tenant_owner_only`, and
//!   an unknown grantee owner is 404 `owner_not_found`; granting the tenant's own owner is 422
//!   `grant_to_owner`;
//! - the per-app offering guard is `_resolve_tenant_for_app_setting`: the tenant's owner may
//!   administer an app it ISSUES or one its own bundle brought onto the tenant (ACTIVE for a
//!   set/provision, ANY status to clear/unprovision); another owner must ISSUE the app and hold an
//!   ACTIVE grant, exactly the issuance rule; `cross_owner` bypasses, audited;
//! - provisioning writes the per-(tenant, product) posture row and pushes the offering's usage
//!   policy to billing (`POST /api/config/markup-rules`), then the access charge
//!   (`POST /api/config/access-charges`) when the offering is priced or bundles an allowance;
//! - **a failed billing write rolls back the local posture and answers 502** — both pushes, exactly
//!   as the reference does. The posture row is restored to what it was before this call (a prior
//!   value written back, or the row deleted when this call created it), and the 502 names the failed
//!   write and billing's status (or says billing could not be reached). Rollback is PARTIAL: a markup
//!   rule accepted earlier in the request SURVIVES a later access-charge failure — billing exposes no
//!   markup-rule delete — and that 502 says so explicitly rather than claiming a clean rollback.

use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde_json::{Map, Number, Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::auth::User;
use crate::billing::metering::MeteringProvisioner;
use crate::db::issuance::{app_issuers, authorize_issuance, require_known_owner};
use crate::db::tenants_crud::{self, Scoped, TenantRow};
use crate::error::Refusal;

/// The billing route `_provision_offering_markup` posts to.
pub const MARKUP_RULES_PATH: &str = "/api/config/markup-rules";
/// The billing route `_provision_access_charge` posts to.
pub const ACCESS_CHARGES_PATH: &str = "/api/config/access-charges";
/// One greppable marker for every markup push this service could not get accepted (the refusal is
/// followed by a posture restore and a 502 — it never reaches the caller as a `200`).
pub const MARKUP_RULE_PROVISION_FAILURE_MARKER: &str = "MARKUP_RULE_PROVISION_FAILED";

/// The per-app setting actions that only REMOVE state: a revoked bundle licence still admits the
/// tenant owner for these (offboarding cleanup), never for a set/provision (`main.py:707`).
const CLEAR_ACTIONS: [&str; 2] = ["tenant_app_byo_mode_clear", "tenant_offering_unprovision"];

/// `_OFFERING_USAGE_FIELDS`, in the reference's order — the keys the markup body carries.
const OFFERING_USAGE_FIELDS: [&str; 6] = [
    "markup_pct",
    "surcharge_input_per_1m_usd",
    "surcharge_output_per_1m_usd",
    "surcharge_cache_creation_per_1m_usd",
    "surcharge_cache_read_per_1m_usd",
    "surcharge_per_request_usd",
];

/// One `tenant_app_grant` row, rendered as `TenantAppGrant.to_dict` does.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TenantGrantRow {
    pub tenant_id: Uuid,
    pub grantee_owner_key: String,
    pub granted_by: String,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

const SELECT_GRANT: &str = "SELECT tenant_id, grantee_owner_key, granted_by, created_at, \
     revoked_at FROM tenant_app_grant";

/// `DatabaseManager.put_tenant_grant` — ensure an ACTIVE grant, returning `(grant, created)`.
///
/// The conflict target is the PARTIAL unique index on `(tenant_id, grantee_owner_key)` where
/// `revoked_at IS NULL`; a revoked row is history and does not block this insert.
pub async fn put_tenant_grant(
    pool: &PgPool,
    tenant_id: Uuid,
    grantee_owner_key: &str,
    granted_by: &str,
) -> Result<(TenantGrantRow, bool), sqlx::Error> {
    let inserted = sqlx::query(
        "INSERT INTO tenant_app_grant (tenant_id, grantee_owner_key, granted_by)
         VALUES ($1, $2, $3)
         ON CONFLICT (tenant_id, grantee_owner_key) WHERE revoked_at IS NULL DO NOTHING",
    )
    .bind(tenant_id)
    .bind(grantee_owner_key)
    .bind(granted_by)
    .execute(pool)
    .await?;
    let sql = format!(
        "{SELECT_GRANT} WHERE tenant_id = $1 AND grantee_owner_key = $2 AND revoked_at IS NULL"
    );
    let grant: TenantGrantRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant_id)
        .bind(grantee_owner_key)
        .fetch_one(pool)
        .await?;
    Ok((grant, inserted.rows_affected() > 0))
}

/// `DatabaseManager.revoke_tenant_grant` — stamp `revoked_at` on the ACTIVE grant.
///
/// False when there was no active grant. The row is locked (`with_for_update`) exactly as the
/// reference does, so two concurrent revokes cannot both report success.
pub async fn revoke_tenant_grant(
    pool: &PgPool,
    tenant_id: Uuid,
    grantee_owner_key: &str,
) -> Result<bool, sqlx::Error> {
    let mut tx: Transaction<'_, Postgres> = pool.begin().await?;
    let id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM tenant_app_grant
          WHERE tenant_id = $1 AND grantee_owner_key = $2 AND revoked_at IS NULL
          ORDER BY created_at
          LIMIT 1
          FOR UPDATE",
    )
    .bind(tenant_id)
    .bind(grantee_owner_key)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(id) = id else {
        tx.rollback().await?;
        return Ok(false);
    };
    sqlx::query("UPDATE tenant_app_grant SET revoked_at = now() WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}

/// `DatabaseManager.list_tenant_grants(active_only=True)` — the ACTIVE grants, oldest first.
pub async fn list_tenant_grants(
    pool: &PgPool,
    tenant_id: Uuid,
) -> Result<Vec<TenantGrantRow>, sqlx::Error> {
    let sql =
        format!("{SELECT_GRANT} WHERE tenant_id = $1 AND revoked_at IS NULL ORDER BY created_at");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant_id)
        .fetch_all(pool)
        .await
}

/// `DatabaseManager.owner_exists` — the grantee must be a `tenant_owner` key.
pub async fn owner_exists(pool: &PgPool, owner_key: &str) -> Result<bool, sqlx::Error> {
    let found: Option<String> = sqlx::query_scalar("SELECT key FROM tenant_owner WHERE key = $1")
        .bind(owner_key)
        .fetch_optional(pool)
        .await?;
    Ok(found.is_some())
}

/// One `offering` row with its numeric columns cast to `float8`, which is how Python's
/// `Offering.to_dict` renders `Numeric` (`float(v)`).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OfferingRow {
    pub id: Uuid,
    pub app_id: Uuid,
    pub code: String,
    pub name: String,
    pub byo_mode: String,
    pub markup_pct: Option<f64>,
    pub surcharge_input_per_1m_usd: Option<f64>,
    pub surcharge_output_per_1m_usd: Option<f64>,
    pub surcharge_cache_creation_per_1m_usd: Option<f64>,
    pub surcharge_cache_read_per_1m_usd: Option<f64>,
    pub surcharge_per_request_usd: Option<f64>,
    pub access_price_usd: Option<f64>,
    pub access_cadence: String,
    pub included_allowance_usd: Option<f64>,
}

const SELECT_OFFERING: &str = "SELECT o.id, o.app_id, o.code, o.name, o.byo_mode, \
     o.markup_pct::float8 AS markup_pct, \
     o.surcharge_input_per_1m_usd::float8 AS surcharge_input_per_1m_usd, \
     o.surcharge_output_per_1m_usd::float8 AS surcharge_output_per_1m_usd, \
     o.surcharge_cache_creation_per_1m_usd::float8 AS surcharge_cache_creation_per_1m_usd, \
     o.surcharge_cache_read_per_1m_usd::float8 AS surcharge_cache_read_per_1m_usd, \
     o.surcharge_per_request_usd::float8 AS surcharge_per_request_usd, \
     o.access_price_usd::float8 AS access_price_usd, o.access_cadence, \
     o.included_allowance_usd::float8 AS included_allowance_usd \
     FROM offering o JOIN app a ON a.id = o.app_id";

/// `DatabaseManager.get_offering` — one offering by `(app_key, code)`.
pub async fn get_offering(
    pool: &PgPool,
    app_key: &str,
    code: &str,
) -> Result<Option<OfferingRow>, sqlx::Error> {
    let sql = format!("{SELECT_OFFERING} WHERE a.key = $1 AND o.code = $2");
    sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(app_key)
        .bind(code)
        .fetch_optional(pool)
        .await
}

impl OfferingRow {
    /// `{f: offering[f] for f in _OFFERING_USAGE_FIELDS if offering.get(f) is not None}`.
    ///
    /// A NULL field is omitted; `0` is a real value and is carried.
    pub fn usage(&self) -> Map<String, Value> {
        let values: [(&str, Option<f64>); 6] = [
            ("markup_pct", self.markup_pct),
            (
                "surcharge_input_per_1m_usd",
                self.surcharge_input_per_1m_usd,
            ),
            (
                "surcharge_output_per_1m_usd",
                self.surcharge_output_per_1m_usd,
            ),
            (
                "surcharge_cache_creation_per_1m_usd",
                self.surcharge_cache_creation_per_1m_usd,
            ),
            (
                "surcharge_cache_read_per_1m_usd",
                self.surcharge_cache_read_per_1m_usd,
            ),
            ("surcharge_per_request_usd", self.surcharge_per_request_usd),
        ];
        let mut usage = Map::new();
        for (field, value) in values {
            debug_assert!(OFFERING_USAGE_FIELDS.contains(&field));
            if let Some(value) = value.and_then(Number::from_f64) {
                usage.insert(field.to_owned(), Value::Number(value));
            }
        }
        usage
    }
}

/// The value a rollback must restore: the posture row as it was BEFORE this request wrote it.
#[derive(Debug, Clone)]
pub struct PostureWrite {
    pub tenant_id: Uuid,
    pub tenant_name: String,
    pub byo_mode: String,
    pub prior_byo_mode: Option<String>,
    pub prior_offering_code: Option<String>,
}

/// `DatabaseManager.set_tenant_app_byo_mode` — upsert the per-(tenant, app) posture override.
///
/// Returns the prior values captured **inside the same statement** as the write, which is what
/// makes the upsert undoable: `prior_byo_mode = None` means the row was created (the undo is a
/// delete), a string means it was overwritten (the undo is to write that value back).
///
/// `None` means the tenant or the app key does not exist — the route answers
/// `404 "Tenant or app not found"`, exactly as the reference does.
pub async fn set_tenant_app_byo_mode(
    pool: &PgPool,
    tenant_id: Uuid,
    app_key: &str,
    byo_mode: &str,
    offering_code: Option<&str>,
) -> Result<Option<PostureWrite>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let tenant_name: Option<String> = sqlx::query_scalar("SELECT name FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(tenant_name) = tenant_name else {
        tx.rollback().await?;
        return Ok(None);
    };
    let app_id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM app WHERE key = $1")
        .bind(app_key)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(app_id) = app_id else {
        tx.rollback().await?;
        return Ok(None);
    };

    // One statement: the CTE reads the PRE-write snapshot, and the upsert writes. `prior` and the
    // INSERT therefore cannot disagree about what was there, and no other session can slip a write
    // between them.
    let row: (Option<String>, Option<String>) = sqlx::query_as(
        "WITH prior AS (
             SELECT byo_mode, offering_code
               FROM tenant_app_byo_mode
              WHERE tenant_id = $1 AND app_id = $2
         ), upsert AS (
             INSERT INTO tenant_app_byo_mode
                 (tenant_id, app_id, byo_mode, offering_code, created_at, updated_at)
             VALUES ($1, $2, $3, $4, now(), now())
             ON CONFLICT (tenant_id, app_id) DO UPDATE
                 SET byo_mode = EXCLUDED.byo_mode,
                     offering_code = EXCLUDED.offering_code,
                     updated_at = now()
             RETURNING tenant_id
         )
         SELECT prior.byo_mode, prior.offering_code FROM upsert LEFT JOIN prior ON true",
    )
    .bind(tenant_id)
    .bind(app_id)
    .bind(byo_mode)
    .bind(offering_code)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Some(PostureWrite {
        tenant_id,
        tenant_name,
        byo_mode: byo_mode.to_owned(),
        prior_byo_mode: row.0,
        prior_offering_code: row.1,
    }))
}

/// `DatabaseManager.clear_tenant_app_byo_mode` — delete the per-product override.
///
/// True iff a row was removed. An unknown app key deletes nothing and answers false.
pub async fn clear_tenant_app_byo_mode(
    pool: &PgPool,
    tenant_id: Uuid,
    app_key: &str,
) -> Result<bool, sqlx::Error> {
    let deleted = sqlx::query(
        "DELETE FROM tenant_app_byo_mode t
          USING app a
          WHERE a.id = t.app_id AND t.tenant_id = $1 AND a.key = $2",
    )
    .bind(tenant_id)
    .bind(app_key)
    .execute(pool)
    .await?;
    Ok(deleted.rows_affected() > 0)
}

/// `main.py::_restore_tenant_app_posture` — undo one [`set_tenant_app_byo_mode`] write.
///
/// `prior` is the captured `prior_byo_mode`: `None` means this request CREATED the row, so the
/// undo is a delete; a string means it overwrote an existing override, so the undo writes that
/// value (and its offering code) back.
///
/// A rollback that does nothing is logged at ERROR — the caller's very next act is a 502 claiming
/// the posture was restored, and silence would make that claim the misinformation. A database
/// fault propagates to the caller, which answers 500: strictly worse news than the 502 it
/// replaces, which is exactly what the reference does.
pub async fn restore_tenant_app_posture(
    pool: &PgPool,
    tenant_id: Uuid,
    app_key: &str,
    prior_byo_mode: Option<&str>,
    prior_offering_code: Option<&str>,
) -> Result<(), sqlx::Error> {
    match prior_byo_mode {
        None => {
            if !clear_tenant_app_byo_mode(pool, tenant_id, app_key).await? {
                tracing::error!(
                    tenant = %tenant_id,
                    app = app_key,
                    "posture rollback CLEARED NOTHING — the override row this request created is \
                     already gone; the 502 claims a rollback that did not happen"
                );
            }
        }
        Some(prior) => {
            if set_tenant_app_byo_mode(pool, tenant_id, app_key, prior, prior_offering_code)
                .await?
                .is_none()
            {
                tracing::error!(
                    tenant = %tenant_id,
                    app = app_key,
                    prior,
                    "posture rollback FAILED: the tenant or app no longer resolves"
                );
            }
        }
    }
    Ok(())
}

/// One per-product posture row for the offerings listing.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OfferingRecordRow {
    pub app_key: String,
    pub app_name: String,
    pub byo_mode: String,
    pub offering_code: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// `DatabaseManager.list_tenant_offerings` — newest write first.
///
/// `None` only when the tenant row itself is gone (the route already resolved it, so this is the
/// "deleted between the two lookups" arm). A tenant with no overrides is `Some(vec![])`.
pub async fn list_offerings(
    pool: &PgPool,
    tenant_id: Uuid,
) -> Result<Option<Vec<OfferingRecordRow>>, sqlx::Error> {
    let exists: Option<Uuid> = sqlx::query_scalar("SELECT id FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .fetch_optional(pool)
        .await?;
    if exists.is_none() {
        return Ok(None);
    }
    let rows = sqlx::query_as(
        "SELECT a.key AS app_key, a.name AS app_name, t.byo_mode, t.offering_code, \
                t.created_at, t.updated_at
           FROM tenant_app_byo_mode t
           JOIN app a ON a.id = t.app_id
          WHERE t.tenant_id = $1
          ORDER BY t.updated_at DESC",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;
    Ok(Some(rows))
}

/// `DatabaseManager.tenant_has_bundle_licence` — a licence for `app_id` on this tenant that a
/// bundle minted (`bundle_id` set), any status or only an effectively active one.
///
/// "Effectively active" is the estate's three-part rule: stored `status = 'active'` AND not
/// pending (`starts_at` NULL or past) AND not expired (`expires_at` NULL or future).
pub async fn tenant_has_bundle_licence(
    pool: &PgPool,
    tenant_id: Uuid,
    app_id: Uuid,
    active_only: bool,
) -> Result<bool, sqlx::Error> {
    let found: Option<Uuid> = if active_only {
        sqlx::query_scalar(
            "SELECT id FROM license
              WHERE tenant_id = $1 AND app_id = $2 AND bundle_id IS NOT NULL
                AND status = 'active'
                AND (starts_at IS NULL OR starts_at <= now())
                AND (expires_at IS NULL OR expires_at > now())
              LIMIT 1",
        )
        .bind(tenant_id)
        .bind(app_id)
        .fetch_optional(pool)
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT id FROM license
              WHERE tenant_id = $1 AND app_id = $2 AND bundle_id IS NOT NULL
              LIMIT 1",
        )
        .bind(tenant_id)
        .bind(app_id)
        .fetch_optional(pool)
        .await?
    };
    Ok(found.is_some())
}

/// `(id, key)` for one app key.
async fn get_app_by_key(
    pool: &PgPool,
    app_key: &str,
) -> Result<Option<(Uuid, String)>, sqlx::Error> {
    let row: Option<(Uuid, String)> =
        sqlx::query_as("SELECT id, key FROM app WHERE key = $1 LIMIT 1")
            .bind(app_key)
            .fetch_optional(pool)
            .await?;
    Ok(row)
}

fn tenant_not_found() -> Refusal {
    Refusal::detail(StatusCode::NOT_FOUND, "Tenant not found")
}

fn database_fault(error: sqlx::Error, operation: &'static str) -> Refusal {
    tracing::error!(%error, operation, "tenant offering database fault");
    Refusal::detail(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
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

/// `main.py::_authorize_app_admin` — the caller's owner must ISSUE the app.
///
/// `license-admin` is a role, not ownership. 403 `app_not_administrable_by_caller`;
/// `cross_owner` bypasses, audited.
pub async fn authorize_app_admin(
    pool: &PgPool,
    user: &User,
    app_id: Uuid,
    action: &str,
    tenant: Option<&TenantRow>,
) -> Result<(), Refusal> {
    let owner = require_known_owner(user)?;
    let issuers = app_issuers(pool, app_id)
        .await
        .map_err(|error| database_fault(error, "app_issuers"))?;
    if issuers.iter().any(|issuer| issuer == &owner) {
        return Ok(());
    }
    if user.cross_owner {
        if let Some(tenant) = tenant {
            tenants_crud::audit_cross_owner(user, action, tenant);
        } else {
            tracing::warn!(action, %owner, %app_id, "cross_owner_action");
        }
        return Ok(());
    }
    tracing::warn!(
        action = %action,
        owner = %owner,
        %app_id,
        azp = ?user.azp,
        "app_not_administrable_by_caller"
    );
    Err(Refusal::detail(
        StatusCode::FORBIDDEN,
        json!({
            "error": "app_not_administrable_by_caller",
            "message": format!("owner '{owner}' does not issue this app and may not administer it"),
        }),
    ))
}

/// `main.py::_resolve_tenant_for_app_setting` — the WRITE resolution for a PER-APP tenant setting
/// (per-app BYO mode, offering provision/unprovision — all keyed on `(tenant, app_key)`).
///
/// * the tenant's owner admits for an app it ISSUES, or for one its own bundle brought onto the
///   tenant (`tenant_has_bundle_licence`; ACTIVE for a set/provision, any status to clear or
///   unprovision); otherwise 403 `app_not_administrable_by_caller` (audited `cross_owner`);
/// * another owner admits ONLY for an app it ISSUES on a tenant it holds an ACTIVE grant on — the
///   issuance rule — else 403 `app_not_issuable_by_caller` / `tenant_not_accessible`;
/// * a by-id selector resolves any tenant; a NAME resolves only within the caller's own owner;
///   an unknown tenant is 404, an ambiguous cross-owner name is 409;
/// * an unknown app on the caller's OWN tenant is returned unchanged: no write can happen, so the
///   route keeps its own 404.
pub async fn resolve_tenant_for_app_setting(
    pool: &PgPool,
    user: &User,
    name_or_id: &str,
    app_key: &str,
    action: &str,
) -> Result<TenantRow, Refusal> {
    let owner = require_known_owner(user)?;
    let by_id = Uuid::try_parse(name_or_id).ok();
    let tenant = match (by_id, user.cross_owner) {
        (Some(id), false) => tenants_crud::get_tenant(pool, id)
            .await
            .map_err(|error| database_fault(error, "get_tenant"))?,
        _ => match tenants_crud::resolve_scoped(
            pool,
            name_or_id,
            Some(&owner),
            user.cross_owner,
            true,
        )
        .await
        .map_err(|error| database_fault(error, "get_tenant_scoped"))?
        {
            Scoped::Found(tenant) => Some(tenant),
            Scoped::NotFound => None,
            Scoped::Ambiguous => return Err(ambiguous_name(name_or_id)),
        },
    };
    let Some(tenant) = tenant else {
        return Err(tenant_not_found());
    };

    let app = get_app_by_key(pool, app_key)
        .await
        .map_err(|error| database_fault(error, "get_app_by_key"))?;

    if tenant.owner_key == owner {
        let Some((app_id, _)) = app else {
            // Unknown app on the caller's own tenant: the route answers its own
            // "Tenant or app not found".
            return Ok(tenant);
        };
        let issuers = app_issuers(pool, app_id)
            .await
            .map_err(|error| database_fault(error, "app_issuers"))?;
        if issuers.iter().any(|issuer| issuer == &owner) {
            return Ok(tenant);
        }
        let active_only = !CLEAR_ACTIONS.contains(&action);
        if tenant_has_bundle_licence(pool, tenant.id, app_id, active_only)
            .await
            .map_err(|error| database_fault(error, "tenant_has_bundle_licence"))?
        {
            return Ok(tenant);
        }
        authorize_app_admin(pool, user, app_id, action, Some(&tenant)).await?;
        return Ok(tenant);
    }

    if user.cross_owner {
        tenants_crud::audit_cross_owner(user, action, &tenant);
        return Ok(tenant);
    }
    let Some((app_id, _)) = app else {
        // Not an issuer of an app that does not exist: refused the same way, so a non-owner
        // cannot enumerate the catalogue through this route.
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            json!({
                "error": "app_not_issuable_by_caller",
                "message": format!("owner '{owner}' may not issue this app's licences"),
            }),
        ));
    };
    authorize_issuance(pool, user, app_id, Some(tenant.id), action).await?;
    Ok(tenant)
}

/// What a provision actually materialized. The two flags are the honest report: each is false
/// when the offering carried nothing to write on that axis.
#[derive(Debug, Clone)]
pub struct ProvisionResult {
    pub tenant_id: String,
    pub tenant_name: String,
    pub app_key: String,
    pub code: String,
    pub byo_mode: String,
    pub markup_rule_provisioned: bool,
    pub access_charge_provisioned: bool,
}

/// `_billing_write_succeeded` — an explicit 2xx range; `None` (billing never reached) is failure.
fn billing_write_succeeded(status: Option<u16>) -> bool {
    matches!(status, Some(status) if (200..300).contains(&status))
}

/// The billing push shared by the markup and access writes.
///
/// `None` (no client configured) is a recorded failure, not an early success: the reference logs
/// `METERING_SERVICE_URL unset` and returns `None`.
async fn push_config(
    metering: Option<&MeteringProvisioner>,
    path: &str,
    tenant_id: Uuid,
    body: &Value,
) -> Option<u16> {
    let Some(metering) = metering else {
        tracing::error!(
            path,
            "METERING_SERVICE_URL unset or unusable; cannot provision the billing rule"
        );
        return None;
    };
    metering.post_config_status(path, tenant_id, body).await
}

/// `provision_tenant_offering` — materialize an offering's usage policy (+ access charge) on a
/// tenant, plus the local posture row.
///
/// Both billing pushes keep the reference's rollback: a refusal or an unreachable billing restores
/// the posture this call overwrote (or deletes the row it created) and answers 502 with the
/// reference's exact detail. A markup rule accepted earlier SURVIVES a later access failure —
/// billing has no markup-rule delete — and that 502 says so.
pub async fn provision_tenant_offering(
    pool: &PgPool,
    user: &User,
    name_or_id: &str,
    app_key: &str,
    code: &str,
    metering: Option<&MeteringProvisioner>,
) -> Result<ProvisionResult, Refusal> {
    let tenant = resolve_tenant_for_app_setting(
        pool,
        user,
        name_or_id,
        app_key,
        "tenant_offering_provision",
    )
    .await?;
    let offering = get_offering(pool, app_key, code)
        .await
        .map_err(|error| database_fault(error, "get_offering"))?
        .ok_or_else(|| Refusal::detail(StatusCode::NOT_FOUND, "Offering not found"))?;

    let posture = set_tenant_app_byo_mode(pool, tenant.id, app_key, &offering.byo_mode, Some(code))
        .await
        .map_err(|error| database_fault(error, "set_tenant_app_byo_mode"))?
        .ok_or_else(|| Refusal::detail(StatusCode::NOT_FOUND, "Tenant or app not found"))?;

    let usage = offering.usage();
    let mut markup_rule_provisioned = false;
    if !usage.is_empty() {
        let mut body = Map::new();
        body.insert(
            "tenant_id".to_owned(),
            Value::String(posture.tenant_id.to_string()),
        );
        body.insert("service_key".to_owned(), Value::String(app_key.to_owned()));
        for (field, value) in usage {
            body.insert(field, value);
        }
        let status = push_config(
            metering,
            MARKUP_RULES_PATH,
            posture.tenant_id,
            &Value::Object(body),
        )
        .await;
        if billing_write_succeeded(status) {
            markup_rule_provisioned = true;
        } else {
            // The reference's rollback: undo the posture override so the tenant is not
            // half-provisioned, then report the failed write. `status` is billing's last HTTP
            // answer; `None` means billing was never reached (or no client is configured).
            tracing::error!(
                marker = MARKUP_RULE_PROVISION_FAILURE_MARKER,
                tenant = %posture.tenant_id,
                app = app_key,
                code,
                status = ?status,
                "billing rejected the markup-rule write; restoring the posture and answering 502"
            );
            restore_tenant_app_posture(
                pool,
                posture.tenant_id,
                app_key,
                posture.prior_byo_mode.as_deref(),
                posture.prior_offering_code.as_deref(),
            )
            .await
            .map_err(|error| database_fault(error, "restore_tenant_app_posture"))?;
            let detail = match status {
                Some(status) => format!(
                    "Billing rejected the markup-rule write (HTTP {status}); provisioning rolled \
                     back"
                ),
                None => {
                    "Could not reach billing to write the markup rule; provisioning rolled back"
                        .to_owned()
                }
            };
            return Err(Refusal::detail(StatusCode::BAD_GATEWAY, detail));
        }
    }

    let mut access_charge_provisioned = false;
    if offering.access_price_usd.is_some() || offering.included_allowance_usd.is_some() {
        let mut body = Map::new();
        body.insert(
            "tenant_id".to_owned(),
            Value::String(posture.tenant_id.to_string()),
        );
        body.insert("service_key".to_owned(), Value::String(app_key.to_owned()));
        body.insert(
            "amount_usd".to_owned(),
            Number::from_f64(offering.access_price_usd.unwrap_or(0.0))
                .map_or(Value::Null, Value::Number),
        );
        body.insert(
            "cadence".to_owned(),
            Value::String(if offering.access_cadence.is_empty() {
                "monthly".to_owned()
            } else {
                offering.access_cadence.clone()
            }),
        );
        if let Some(included) = offering.included_allowance_usd
            && let Some(included) = Number::from_f64(included)
        {
            body.insert("included_cost_usd".to_owned(), Value::Number(included));
        }
        let status = push_config(
            metering,
            ACCESS_CHARGES_PATH,
            posture.tenant_id,
            &Value::Object(body),
        )
        .await;
        if !billing_write_succeeded(status) {
            restore_tenant_app_posture(
                pool,
                posture.tenant_id,
                app_key,
                posture.prior_byo_mode.as_deref(),
                posture.prior_offering_code.as_deref(),
            )
            .await
            .map_err(|error| database_fault(error, "restore_tenant_app_posture"))?;
            // A markup rule written moments ago — if there was one — SURVIVES this failure:
            // billing has no markup-rule delete, so saying "rolled back" while a live rule bills
            // the tenant would be a lie in the one place an operator is looking.
            let residue = if markup_rule_provisioned {
                " The markup rule written earlier in this request was NOT removed (billing has no \
                 markup-rule delete) — re-provision to overwrite it."
            } else {
                ""
            };
            let detail = match status {
                Some(status) => format!(
                    "Billing rejected the access-charge write (HTTP {status}); the posture \
                     override was rolled back.{residue}"
                ),
                None => format!(
                    "Could not reach billing to write the access charge; the posture override \
                     was rolled back.{residue}"
                ),
            };
            return Err(Refusal::detail(StatusCode::BAD_GATEWAY, detail));
        }
        access_charge_provisioned = true;
    }

    tracing::info!(
        tenant = %posture.tenant_id,
        app = app_key,
        code,
        byo = offering.byo_mode,
        markup_rule = markup_rule_provisioned,
        access_charge = access_charge_provisioned,
        "Offering provisioned"
    );
    Ok(ProvisionResult {
        tenant_id: posture.tenant_id.to_string(),
        tenant_name: posture.tenant_name,
        app_key: app_key.to_owned(),
        code: code.to_owned(),
        byo_mode: offering.byo_mode,
        markup_rule_provisioned,
        access_charge_provisioned,
    })
}

/// `unprovision_tenant_offering` — clear the per-(tenant, product) posture override, reverting to
/// the tenant-wide `byo_mode`. Idempotent.
///
/// The billing markup rule is left in place: billing exposes no markup-rule delete, and
/// re-provisioning overwrites it. Markup only bills BYO usage, which the reverted posture governs.
pub async fn unprovision_tenant_offering(
    pool: &PgPool,
    user: &User,
    name_or_id: &str,
    app_key: &str,
    code: &str,
) -> Result<bool, Refusal> {
    let tenant = resolve_tenant_for_app_setting(
        pool,
        user,
        name_or_id,
        app_key,
        "tenant_offering_unprovision",
    )
    .await?;
    let cleared = clear_tenant_app_byo_mode(pool, tenant.id, app_key)
        .await
        .map_err(|error| database_fault(error, "clear_tenant_app_byo_mode"))?;
    tracing::info!(
        tenant = %name_or_id,
        app = app_key,
        code,
        posture_cleared = cleared,
        "Offering unprovisioned"
    );
    Ok(cleared)
}
