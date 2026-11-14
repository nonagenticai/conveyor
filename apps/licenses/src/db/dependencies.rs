//! Dependency release: a dependency licence lives exactly as long as some active non-dependency
//! licence on the tenant still reaches its app through `app_requires`.
//!
//! Ported from `models.py::DatabaseManager` (`revoke_license_releasing_dependencies` at :8151,
//! `_lock_tenant_for_dependency_release` :7888, `_licences_requiring_app` :7955,
//! `_dependency_requirements` :7976 and `_release_orphaned_dependencies` :8019), together with
//! the pure walk in `services/app_requires.py::closure`. The reference's `models.py` is the pinned
//! byte-identical copy, so these functions are the contract, not a reinterpretation.
//!
//! The rule being ported:
//!
//! - bundle ensure (migration 059) provisions an `app_requires` closure as SHARED dependency
//!   licences (`is_dependency`, `bundle_id` NULL); nothing removed them when the last licence that
//!   needed them went away;
//! - every licence revoke re-evaluates the tenant's dependency licences IN THE SAME TRANSACTION,
//!   under the tenant's bundle-ensure advisory lock (`bundle_ensure:{tenant_id}`), so a concurrent
//!   ensure that planned `exists` on a dependency cannot have it released underneath it;
//! - "active" for a ROOT is the stored `status = 'active'` and not soft-deleted, without the time
//!   window: an expired-but-unrevoked licence may be renewed, so releasing too little is the safe
//!   direction;
//! - a dependency licence is never a root, otherwise kag-as-dependency would keep workers alive
//!   forever;
//! - a cycle in `app_requires` reachable from the roots releases NOTHING (fail safe), and a cycle
//!   encountered while answering "who requires this app" makes that root count as requiring it.
//!
//! The audit record is the retained `license.metadata` (`released_by`, `release_reason`,
//! `released_at`, `released_by_revoke_of`), exactly as the reference writes it; `deleted_*` is not
//! touched because a release is a revoke, not a deletion.

use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

/// `DEPENDENCY_RELEASE_REASON`.
pub const DEPENDENCY_RELEASE_REASON: &str = "dependency released: no remaining dependent licence";
/// `DEPENDENCY_EXPLICIT_REVOKE_REASON`.
pub const DEPENDENCY_EXPLICIT_REVOKE_REASON: &str = "dependency revoked explicitly";
/// `DEPENDENCY_RELEASE_LOCK_TIMEOUT`: applies to the advisory-lock wait only.
pub const DEPENDENCY_RELEASE_LOCK_TIMEOUT: &str = "10s";

/// One active non-dependency licence that still reaches a dependency's app (the reference's
/// `[{license_id, tenant_id, app_key}]`, where `app_key` is the REQUIRING licence's app).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequiredBy {
    pub license_id: Uuid,
    pub tenant_id: Uuid,
    pub app_key: String,
}

/// The reference's `(outcome, released dependency licence ids, required_by)` tuple.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The licence was revoked. `released_ids` names only the dependency licences released IN
    /// ADDITION to the one revoked (oldest first); an explicitly revoked dependency is not listed.
    Revoked { released_ids: Vec<Uuid> },
    /// No licence row named by the caller exists.
    NotFound,
    /// `refuse_required_dependency` was set and the licence is an ACTIVE dependency licence that a
    /// remaining active non-dependency licence still needs. NOTHING was written.
    DependencyStillRequired { required_by: Vec<RequiredBy> },
    /// `refuse_required_dependency` was set and the tenant's bundle-ensure lock could not be taken,
    /// so "still required" cannot be decided safely. NOTHING was written.
    EnsureInProgress,
}

/// One `app_requires` row: `app` requires `required`, and the edge is minted by `caller`.
#[derive(Debug, Clone)]
struct Edge {
    app: String,
    required: String,
    caller: String,
}

#[derive(Debug, Default)]
struct Closure {
    deps: Vec<String>,
    walked: Vec<Edge>,
}

/// `(required app keys, required (owner, app key) grants)` for the tenant's remaining active
/// non-dependency licences. `None` when `app_requires` holds a cycle reachable from them.
type Requirements = (HashSet<String>, HashSet<(String, String)>);

/// `(licence id, app key, bundle id)` of one dependency root.
type RootRow = (Uuid, String, Option<Uuid>);

/// `(licence id, app id, app key)` of one dependency-release candidate.
type CandidateRow = (Uuid, Uuid, String);

/// `(app id, app key, is_dependency, status)` of the licence being revoked.
type LicenceRow = (Uuid, String, bool, String);

/// `(app key, required app key, caller owner key)` of one `app_requires` edge.
type EdgeRow = (String, String, String);

/// `(owner key, app key)` of one held bundle's caller edge.
type BundleCallerRow = (String, String);

/// `(owner key, app id)` of one `tenant_dependency_grant` row.
type GrantRow = (String, Uuid);

/// Take the tenant's bundle-ensure transaction-scoped advisory lock FIRST in `tx`.
///
/// `Ok(false)` means the bounded `lock_timeout` expired with SQLSTATE 55P03 (the reference's
/// deferred path). The caller must then roll the transaction back: Postgres has aborted it, and no
/// further statement can run until it does. The lock must be taken before any row lock: ensure
/// holds it while it updates licence rows, so taking a row lock first could deadlock.
pub async fn lock_tenant_for_dependency_release(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
) -> Result<bool, sqlx::Error> {
    // The bounded timeout applies to the advisory-lock wait ONLY: it is set transaction-locally
    // and restored to the session's previous value right after the lock is taken, so the row locks
    // the revoke then takes wait as they would anywhere else.
    let previous: String = sqlx::query_scalar("SELECT current_setting('lock_timeout')")
        .fetch_one(&mut **tx)
        .await?;
    sqlx::query("SELECT set_config('lock_timeout', $1, true)")
        .bind(DEPENDENCY_RELEASE_LOCK_TIMEOUT)
        .execute(&mut **tx)
        .await?;

    let key = format!("bundle_ensure:{tenant_id}");
    match sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&key)
        .execute(&mut **tx)
        .await
    {
        Ok(_) => {}
        Err(sqlx::Error::Database(error)) if error.code().as_deref() == Some("55P03") => {
            return Ok(false);
        }
        Err(error) => return Err(error),
    }

    sqlx::query("SELECT set_config('lock_timeout', $1, true)")
        .bind(&previous)
        .execute(&mut **tx)
        .await?;
    Ok(true)
}

/// `_dependency_roots`: `(licence id, app key, bundle id)` of the tenant's ACTIVE NON-dependency
/// licences, ordered `created_at, id`.
async fn dependency_roots(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
) -> Result<Vec<RootRow>, sqlx::Error> {
    sqlx::query_as::<_, RootRow>(
        "SELECT l.id, a.key, l.bundle_id
           FROM license l
           JOIN app a ON a.id = l.app_id
          WHERE l.tenant_id = $1
            AND l.status = 'active'
            AND l.deleted_at IS NULL
            AND NOT l.is_dependency
          ORDER BY l.created_at, l.id",
    )
    .bind(tenant_id)
    .fetch_all(&mut **tx)
    .await
}

/// `_app_requires_edges`: every `app_requires` edge between non-revoked apps.
async fn app_requires_edges(tx: &mut Transaction<'_, Postgres>) -> Result<Vec<Edge>, sqlx::Error> {
    let rows: Vec<EdgeRow> = sqlx::query_as(
        "SELECT a.key, r.key, ar.caller_owner_key
           FROM app_requires ar
           JOIN app a ON a.id = ar.app_id
           JOIN app r ON r.id = ar.required_app_id
          WHERE NOT a.revoked AND NOT r.revoked
          ORDER BY 1, 2",
    )
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(app, required, caller)| Edge {
            app,
            required,
            caller,
        })
        .collect())
}

/// `services/app_requires.py::closure`: every app reachable from `roots` (excluding the roots) and
/// the edges walked, both deterministic. A reachable cycle is returned as the cycle's path.
fn closure(roots: &[String], edges: &[Edge]) -> Result<Closure, Vec<String>> {
    let mut adjacency: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, edge) in edges.iter().enumerate() {
        adjacency.entry(edge.app.clone()).or_default().push(index);
    }
    for indices in adjacency.values_mut() {
        indices.sort_by(|left, right| edges[*left].required.cmp(&edges[*right].required));
    }

    let root_set: HashSet<String> = roots.iter().cloned().collect();
    let mut done: HashSet<String> = HashSet::new();
    let mut result = Closure::default();
    let mut sorted_roots: Vec<String> = root_set.iter().cloned().collect();
    sorted_roots.sort();
    for root in &sorted_roots {
        visit(
            root,
            &mut Vec::new(),
            &adjacency,
            edges,
            &root_set,
            &mut done,
            &mut result,
        )?;
    }
    Ok(result)
}

fn visit(
    app: &str,
    path: &mut Vec<String>,
    adjacency: &BTreeMap<String, Vec<usize>>,
    edges: &[Edge],
    root_set: &HashSet<String>,
    done: &mut HashSet<String>,
    result: &mut Closure,
) -> Result<(), Vec<String>> {
    if let Some(position) = path.iter().position(|entry| entry == app) {
        let mut cycle = path[position..].to_vec();
        cycle.push(app.to_owned());
        return Err(cycle);
    }
    if done.contains(app) {
        return Ok(());
    }
    path.push(app.to_owned());
    if let Some(indices) = adjacency.get(app) {
        for &index in indices {
            let edge = &edges[index];
            result.walked.push(edge.clone());
            visit(
                &edge.required,
                path,
                adjacency,
                edges,
                root_set,
                done,
                result,
            )?;
            if !root_set.contains(&edge.required) && !result.deps.contains(&edge.required) {
                result.deps.push(edge.required.clone());
            }
        }
    }
    path.pop();
    done.insert(app.to_owned());
    Ok(())
}

/// `_licences_requiring_app`: the tenant's active non-dependency licences whose `app_requires`
/// closure reaches `app_id`, ordered as the roots are (`created_at, id`). A root on a cycle counts
/// as requiring it.
pub async fn licences_requiring_app(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    app_id: Uuid,
) -> Result<Vec<RequiredBy>, sqlx::Error> {
    let app_key: Option<String> = sqlx::query_scalar("SELECT key FROM app WHERE id = $1")
        .bind(app_id)
        .fetch_optional(&mut **tx)
        .await?;
    let Some(app_key) = app_key else {
        return Ok(Vec::new());
    };

    let edges = app_requires_edges(tx).await?;
    let mut out = Vec::new();
    for (license_id, root_key, _bundle_id) in dependency_roots(tx, tenant_id).await? {
        let reached = match closure(std::slice::from_ref(&root_key), &edges) {
            Ok(reached) => reached.deps.contains(&app_key),
            // A cycle reachable from this root makes the root count as requiring the app.
            Err(_) => true,
        };
        if reached {
            out.push(RequiredBy {
                license_id,
                tenant_id,
                app_key: root_key,
            });
        }
    }
    Ok(out)
}

/// `_dependency_requirements`: `(required app keys, required grants)` for the tenant's remaining
/// active non-dependency licences, or `None` on a reachable `app_requires` cycle (the caller then
/// releases NOTHING).
async fn dependency_requirements(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
) -> Result<Option<Requirements>, sqlx::Error> {
    let rows = dependency_roots(tx, tenant_id).await?;
    let mut required: HashSet<String> = HashSet::new();
    let mut bundle_ids: Vec<Uuid> = Vec::new();
    for (_license_id, app_key, bundle_id) in &rows {
        required.insert(app_key.clone());
        if let Some(bundle_id) = bundle_id {
            bundle_ids.push(*bundle_id);
        }
    }
    bundle_ids.sort();
    bundle_ids.dedup();

    let edges = app_requires_edges(tx).await?;
    let roots: Vec<String> = required.iter().cloned().collect();
    let reached = match closure(&roots, &edges) {
        Ok(reached) => reached,
        Err(path) => {
            tracing::error!(
                tenant_id = %tenant_id,
                path = %path.join(" -> "),
                "dependency_release_skipped: app_requires cycle; no dependency licence released"
            );
            return Ok(None);
        }
    };

    required.extend(reached.deps.iter().cloned());
    let mut grants: HashSet<(String, String)> = reached
        .walked
        .iter()
        .map(|edge| (edge.caller.clone(), edge.required.clone()))
        .collect();
    if !bundle_ids.is_empty() {
        // A held bundle's `bundle_caller` edge keeps the grant even when no remaining licence
        // walks it.
        let rows: Vec<BundleCallerRow> = sqlx::query_as(
            "SELECT c.owner_key, a.key
               FROM bundle_caller c
               JOIN app a ON a.id = c.app_id
              WHERE c.bundle_id = ANY($1)",
        )
        .bind(bundle_ids.as_slice())
        .fetch_all(&mut **tx)
        .await?;
        grants.extend(rows);
    }
    Ok(Some((required, grants)))
}

/// `_release_orphaned_dependencies`: revoke every ACTIVE dependency licence on the tenant that no
/// remaining active non-dependency licence requires. Runs inside the caller's transaction (it does
/// NOT commit); returns the released licence ids, oldest first.
///
/// `revoked_app_id`: the app of a dependency licence the caller has just revoked BY ID (already
/// `revoked` in this transaction, so the candidate query no longer sees it). It gets the explicit
/// audit stamp and the same grant cleanup, but is NOT in the returned list. `revoked_license_id`
/// is the licence that triggered this release; the reference records it as
/// `released_by_revoke_of`, and for the explicit licence that value is its own id.
pub async fn release_orphaned_dependencies(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    revoked_app_id: Option<Uuid>,
    revoked_license_id: Uuid,
    actor: &str,
    now: DateTime<Utc>,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let mut released_apps: BTreeMap<Uuid, String> = BTreeMap::new();
    if let Some(app_id) = revoked_app_id {
        stamp_explicit_release(tx, revoked_license_id, actor, now, revoked_license_id).await?;
        let app_key: Option<String> = sqlx::query_scalar("SELECT key FROM app WHERE id = $1")
            .bind(app_id)
            .fetch_optional(&mut **tx)
            .await?;
        if let Some(app_key) = app_key {
            released_apps.insert(app_id, app_key);
        }
        tracing::warn!(
            tenant_id = %tenant_id,
            licence_id = %revoked_license_id,
            app_id = %app_id,
            actor = %actor,
            "dependency_revoked_explicitly"
        );
    }

    let Some((required, required_grants)) = dependency_requirements(tx, tenant_id).await? else {
        return Ok(Vec::new());
    };
    let required: Vec<String> = required.into_iter().collect();

    // `a.key <> ALL('{}')` is true, so the empty-required case needs no separate branch.
    let candidates: Vec<CandidateRow> = sqlx::query_as(
        "SELECT l.id, l.app_id, a.key
           FROM license l
           JOIN app a ON a.id = l.app_id
          WHERE l.tenant_id = $1
            AND l.is_dependency
            AND l.status = 'active'
            AND l.deleted_at IS NULL
            AND a.key <> ALL($2)
          ORDER BY l.created_at, l.id",
    )
    .bind(tenant_id)
    .bind(required.as_slice())
    .fetch_all(&mut **tx)
    .await?;

    let mut released: Vec<Uuid> = Vec::new();
    for (license_id, app_id, app_key) in candidates {
        release_candidate(tx, license_id, actor, now, revoked_license_id).await?;
        released.push(license_id);
        released_apps.insert(app_id, app_key);
    }
    if released_apps.is_empty() {
        return Ok(Vec::new());
    }

    if !released.is_empty() {
        sqlx::query(
            "UPDATE token SET revoked = true, revoked_at = $2
              WHERE license_id = ANY($1) AND revoked = false",
        )
        .bind(released.as_slice())
        .bind(now)
        .execute(&mut **tx)
        .await?;
    }

    // GRANTS. A `tenant_dependency_grant` row exists only so the owner's minter can mint that
    // dependency for the tenant; once the licence is released the row is removed unless the
    // remaining licences still walk that (caller, app) edge or a held bundle's `bundle_caller`
    // names it. Only released apps' grant rows are touched.
    let released_app_ids: Vec<Uuid> = released_apps.keys().copied().collect();
    let grants: Vec<GrantRow> = sqlx::query_as(
        "SELECT owner_key, app_id
           FROM tenant_dependency_grant
          WHERE tenant_id = $1 AND app_id = ANY($2)",
    )
    .bind(tenant_id)
    .bind(released_app_ids.as_slice())
    .fetch_all(&mut **tx)
    .await?;
    let mut removed_grants: Vec<String> = Vec::new();
    for (owner_key, app_id) in grants {
        let app_key = released_apps[&app_id].clone();
        if required_grants.contains(&(owner_key.clone(), app_key.clone())) {
            continue;
        }
        removed_grants.push(format!("{owner_key}:{app_key}"));
        sqlx::query(
            "DELETE FROM tenant_dependency_grant
              WHERE tenant_id = $1 AND owner_key = $2 AND app_id = $3",
        )
        .bind(tenant_id)
        .bind(&owner_key)
        .bind(app_id)
        .execute(&mut **tx)
        .await?;
    }

    for license_id in &released {
        tracing::warn!(
            tenant_id = %tenant_id,
            licence_id = %license_id,
            actor = %actor,
            trigger = %revoked_license_id,
            reason = DEPENDENCY_RELEASE_REASON,
            "dependency_released"
        );
    }
    if !removed_grants.is_empty() {
        tracing::warn!(
            tenant_id = %tenant_id,
            grants = %removed_grants.join(","),
            actor = %actor,
            trigger = %revoked_license_id,
            "dependency_grants_removed"
        );
    }
    Ok(released)
}

/// `revoke_license_releasing_dependencies`: `revoke_license` + `release_orphaned_dependencies` in
/// ONE transaction.
///
/// `refuse_required_dependency` is the route's gate: with it set, an explicit revoke of an ACTIVE
/// dependency licence that a remaining active licence still needs writes NOTHING and answers
/// [`Outcome::DependencyStillRequired`]; without it the revoke is forced (the reference's
/// cross-owner break-glass) and logged.
///
/// Idempotent: re-revoking an already-revoked licence keeps its `revoked_at` and re-runs the
/// release, so a release skipped earlier is completed by the retry. On a lock timeout a plain
/// revoke proceeds WITHOUT the release (security-critical revoke first), exactly as the reference.
pub async fn revoke_license_releasing_dependencies(
    pool: &PgPool,
    license_id: Uuid,
    actor: &str,
    refuse_required_dependency: bool,
) -> Result<Outcome, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let tenant_id: Option<Uuid> = sqlx::query_scalar("SELECT tenant_id FROM license WHERE id = $1")
        .bind(license_id)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(tenant_id) = tenant_id else {
        return Ok(Outcome::NotFound);
    };

    if !lock_tenant_for_dependency_release(&mut tx, tenant_id).await? {
        // The failed advisory-lock wait aborted this transaction; nothing has been written.
        tx.rollback().await?;
        if refuse_required_dependency {
            return Ok(Outcome::EnsureInProgress);
        }
        tracing::error!(
            tenant_id = %tenant_id,
            licence_id = %license_id,
            actor = %actor,
            "DEPENDENCY_RELEASE_DEFERRED: a bundle ensure held the tenant lock past the timeout; \
             the licence is revoked, its orphaned dependency licences are NOT yet released \
             (re-revoke to release)"
        );
        return plain_revoke(pool, license_id).await;
    }

    let licence: Option<LicenceRow> = sqlx::query_as(
        "SELECT l.app_id, a.key, l.is_dependency, l.status
           FROM license l
           JOIN app a ON a.id = l.app_id
          WHERE l.id = $1",
    )
    .bind(license_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((app_id, app_key, is_dependency, status)) = licence else {
        return Ok(Outcome::NotFound);
    };

    let explicit_dependency = is_dependency && status == "active";
    if explicit_dependency {
        let required_by = licences_requiring_app(&mut tx, tenant_id, app_id).await?;
        if !required_by.is_empty() {
            if refuse_required_dependency {
                tx.rollback().await?;
                return Ok(Outcome::DependencyStillRequired { required_by });
            }
            // Break-glass force: allowed, but never silent.
            let names = required_by
                .iter()
                .map(|required| required.license_id.to_string())
                .collect::<Vec<_>>()
                .join(",");
            tracing::warn!(
                tenant_id = %tenant_id,
                licence_id = %license_id,
                app = %app_key,
                actor = %actor,
                required_by = %names,
                "dependency_force_revoked"
            );
        }
    }

    let now = Utc::now();
    sqlx::query(
        "UPDATE license
            SET status = 'revoked',
                revoked_at = COALESCE(revoked_at, $2),
                updated_at = $2
          WHERE id = $1",
    )
    .bind(license_id)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE token SET revoked = true, revoked_at = $2
          WHERE license_id = $1 AND revoked = false",
    )
    .bind(license_id)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    let released_ids = release_orphaned_dependencies(
        &mut tx,
        tenant_id,
        explicit_dependency.then_some(app_id),
        license_id,
        actor,
        now,
    )
    .await?;
    tx.commit().await?;
    Ok(Outcome::Revoked { released_ids })
}

/// `revoke_license` alone: the reference's deferred path (lock timeout, no refusal) revokes the
/// licence in its own transaction and leaves the release to a later revoke.
async fn plain_revoke(pool: &PgPool, license_id: Uuid) -> Result<Outcome, sqlx::Error> {
    let now = Utc::now();
    let mut tx = pool.begin().await?;
    let updated = sqlx::query(
        "UPDATE license
            SET status = 'revoked',
                revoked_at = COALESCE(revoked_at, $2),
                updated_at = $2
          WHERE id = $1",
    )
    .bind(license_id)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        return Ok(Outcome::NotFound);
    }
    sqlx::query(
        "UPDATE token SET revoked = true, revoked_at = $2
          WHERE license_id = $1 AND revoked = false",
    )
    .bind(license_id)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Outcome::Revoked {
        released_ids: Vec::new(),
    })
}

/// The explicit dependency's audit stamp: metadata only (its status was written by the caller).
async fn stamp_explicit_release(
    tx: &mut Transaction<'_, Postgres>,
    license_id: Uuid,
    actor: &str,
    now: DateTime<Utc>,
    revoked_license_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE license
            SET metadata = COALESCE(metadata, '{}'::jsonb) || jsonb_build_object(
                    'released_by', $2::text,
                    'release_reason', $3::text,
                    'released_at', $4::text,
                    'released_by_revoke_of', $5::text)
          WHERE id = $1",
    )
    .bind(license_id)
    .bind(actor)
    .bind(DEPENDENCY_EXPLICIT_REVOKE_REASON)
    .bind(py_iso(now))
    .bind(revoked_license_id.to_string())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// One released candidate: audit stamp + revoke, keeping an earlier `revoked_at` (it is the audit
/// record of when the credential was actually killed) while `updated_at` advances.
async fn release_candidate(
    tx: &mut Transaction<'_, Postgres>,
    license_id: Uuid,
    actor: &str,
    now: DateTime<Utc>,
    revoked_license_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE license
            SET metadata = COALESCE(metadata, '{}'::jsonb) || jsonb_build_object(
                    'released_by', $2::text,
                    'release_reason', $3::text,
                    'released_at', $4::text,
                    'released_by_revoke_of', $5::text),
                status = 'revoked',
                revoked_at = COALESCE(revoked_at, $6),
                updated_at = $6
          WHERE id = $1",
    )
    .bind(license_id)
    .bind(actor)
    .bind(DEPENDENCY_RELEASE_REASON)
    .bind(py_iso(now))
    .bind(revoked_license_id.to_string())
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Python's `datetime.isoformat()` on a UTC instant: no fraction when the microsecond is zero,
/// otherwise exactly six digits, with the `+00:00` suffix. The reference stores this string in
/// `license.metadata.released_at`.
fn py_iso(value: DateTime<Utc>) -> String {
    if value.timestamp_subsec_micros() == 0 {
        value.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
    } else {
        value.format("%Y-%m-%dT%H:%M:%S%.6f%:z").to_string()
    }
}
