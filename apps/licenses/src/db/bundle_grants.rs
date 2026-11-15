//! The WRITE half of the bundle-ensure route: the bundle hold and the least-privilege grants, in
//! ONE transaction and before any licence is inserted.
//!
//! Ported from `models.py`: [`bundle_grantees`] (:3733, the least-privilege matrix),
//! `DatabaseManager._GRANT_INSERT` (:5660) and `DatabaseManager.ensure_bundle_grants` (:5727).
//! `dependency_grants` is assembled by the read-only plan ([`crate::db::bundle_ensure`]).
//!
//! ⛔ Everything below runs in ONE transaction. A failure leaves neither the hold nor a grant
//! behind, and a second identical call writes and returns nothing.
//!
//! This is how an owner sanctions another app on its tenant through a bundle (merqi ensuring the
//! `w2v` bundle grants `w2v`). Nothing here mounts a route; the later compose card calls it with
//! the licence writes.

use std::collections::{BTreeMap, BTreeSet};

use sqlx::PgPool;
use uuid::Uuid;

/// `PLATFORM_OWNER` (`models.py`:1763).
pub const PLATFORM_OWNER: &str = "platform";

/// The owners ONE bundled app grants on a tenant — `bundle_grantees` (`models.py`:3733),
/// LEAST PRIVILEGE:
///
/// * the tenant's own owner is an issuer of the app -> nobody (it serves that app);
/// * a SINGLE issuer -> that issuer;
/// * SEVERAL issuers -> [`PLATFORM_OWNER`] only, and only if it is one of them. The `bundle` table
///   names no preferred issuer, so a multi-issuer app never grants an app owner by itself
///   (`inference-router` = `{fr, commonpractice}` grants nobody; it used to grant both).
pub fn bundle_grantees(tenant_owner: Option<&str>, issuers: &BTreeSet<String>) -> BTreeSet<String> {
    if tenant_owner.is_some_and(|owner| issuers.contains(owner)) {
        return BTreeSet::new();
    }
    if issuers.len() == 1 {
        return issuers.clone();
    }
    let mut grantees = BTreeSet::new();
    if issuers.contains(PLATFORM_OWNER) {
        grantees.insert(PLATFORM_OWNER.to_owned());
    }
    grantees
}

/// Record the bundle hold (`hold` only) and grant the bundled apps' issuers access to the tenant,
/// then write the app-scoped dependency grants — all in ONE transaction.
///
/// Returns the owner keys newly granted, followed by `"<owner>:<app>"` for each newly inserted
/// `tenant_dependency_grant`. Idempotent: a row the conflict clauses keep out is never returned.
///
/// `_GRANT_INSERT`'s conflict target is the PARTIAL unique index
/// `uq_tenant_app_grant_active (tenant_id, grantee_owner_key) WHERE revoked_at IS NULL`: a revoked
/// grant does not block a fresh active one.
///
/// A dependency grant's owner that owns the tenant is skipped, and its `(owner, app)` point is
/// written with `INSERT .. SELECT a.id FROM app a WHERE a.key = $k`, so an unknown app key writes
/// nothing (and is therefore not returned). Duplicate `(owner, app)` pairs collapse.
pub async fn ensure_bundle_grants(
    pool: &PgPool,
    tenant_id: Uuid,
    bundle_id: Uuid,
    granted_by: &str,
    hold: bool,
    dependency_grants: &[(String, String)],
) -> Result<Vec<String>, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let owner: Option<String> = sqlx::query_scalar("SELECT owner_key FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await?;

    if hold {
        sqlx::query(
            "INSERT INTO tenant_bundle (tenant_id, bundle_id) VALUES ($1, $2)
             ON CONFLICT DO NOTHING",
        )
        .bind(tenant_id)
        .bind(bundle_id)
        .execute(&mut *tx)
        .await?;
    }

    // Per bundled app, its `app_issuer` owners. The reference's join has no `revoked` filter.
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT ba.app_id, ai.owner_key
           FROM bundle_app ba
           JOIN app_issuer ai ON ai.app_id = ba.app_id
          WHERE ba.bundle_id = $1",
    )
    .bind(bundle_id)
    .fetch_all(&mut *tx)
    .await?;
    let mut issuers_by_app: BTreeMap<Uuid, BTreeSet<String>> = BTreeMap::new();
    for (app_id, issuer) in rows {
        issuers_by_app.entry(app_id).or_default().insert(issuer);
    }

    let mut created: Vec<String> = Vec::new();
    let grantees: BTreeSet<String> = issuers_by_app
        .values()
        .flat_map(|issuers| bundle_grantees(owner.as_deref(), issuers))
        .collect();
    for grantee in grantees {
        let result = sqlx::query(
            "INSERT INTO tenant_app_grant (tenant_id, grantee_owner_key, granted_by)
             VALUES ($1, $2, $3)
             ON CONFLICT (tenant_id, grantee_owner_key) WHERE revoked_at IS NULL DO NOTHING",
        )
        .bind(tenant_id)
        .bind(&grantee)
        .bind(granted_by)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() > 0 {
            created.push(grantee);
        }
    }

    // `sorted(set(dependency_grants))`: sort then collapse adjacent duplicates.
    let mut dependencies: Vec<&(String, String)> = dependency_grants.iter().collect();
    dependencies.sort();
    dependencies.dedup();
    for (dep_owner, app_key) in dependencies {
        if Some(dep_owner.as_str()) == owner.as_deref() {
            continue;
        }
        let result = sqlx::query(
            "INSERT INTO tenant_dependency_grant (tenant_id, owner_key, app_id, granted_by)
             SELECT $1, $2, a.id, $3 FROM app a WHERE a.key = $4
             ON CONFLICT DO NOTHING",
        )
        .bind(tenant_id)
        .bind(dep_owner)
        .bind(granted_by)
        .bind(app_key)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() > 0 {
            created.push(format!("{dep_owner}:{app_key}"));
        }
    }

    tx.commit().await?;
    Ok(created)
}
