//! The WRITE orchestration of bundle ensure: the tenant advisory lock, `apply`'s per-app writes
//! and metering, and `plan_and_apply`.
//!
//! Ported from `services/bundle_ensure.py`: [`TenantEnsureLock`] (:135), `_PendingMetering`
//! (:808), `_apply_one` (:817), `_lost_insert` (:869), `_record_metering` (:903), [`apply`] (:909)
//! and [`plan_and_apply`] (:998). The decision layer is [`crate::db::bundle_ensure`]; the row
//! writes it composes are [`crate::db::bundle_apply_rows`] and [`crate::db::bundle_grants`]. The
//! HTTP route is a later card — nothing here mounts one.
//!
//! ⛔ ORDER IS THE CONTRACT. The bundle hold and the grants are written in ONE transaction (their
//! own) BEFORE any licence; then each app is independent, so one app's refusal never aborts
//! another's insert; and a metering failure (billing down, a bad URL, a missing plan row) is
//! RECORDED on the plan — `metering_pending` / `unmetered` — and never returned as a request
//! failure, because the licence is live and the reconcile sweep repairs it.
//!
//! ⛔ The lock is a SEPARATE, DEDICATED holder transaction (the reference's `_TenantEnsureLock`).
//! Every write `apply` makes opens and commits its OWN session, so one transaction could not span
//! them without a threaded second copy of each. Instead one pooled connection opens a transaction,
//! takes the TRANSACTION-scoped advisory lock, and stays open (idle) until the ensure ends; the
//! ROLLBACK is what releases it. A dropped lock rolls back with its connection (sqlx's
//! `Transaction` drop queues the rollback), so it can never leak — and `SET LOCAL lock_timeout`
//! dies with the transaction, so it never leaks to the next pooled client either.

use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::billing::metering::MeteringProvisioner;
use crate::db::bundle_apply_rows::{
    CreateLicenceError, CreateSubjectError, LostRaceOutcome, NewLicence, adopt_license_into_bundle,
    attribute_subject_to_tenant, create_license, create_or_get_subject, resolve_lost_insert,
};
use crate::db::bundle_ensure::{
    AppAction, AppPlan, EnsureError, EnsureErrorCode, EnsurePlan, RefuseReason, plan,
};
use crate::db::bundle_grants::ensure_bundle_grants;

/// `ENSURE_LOCK_TIMEOUT`: how long an ensure waits for another ensure of the SAME tenant
/// (Postgres `lock_timeout` syntax). The environment variable of the same name overrides it, which
/// is what lets a test shorten the wait. Bounded so a stuck holder cannot park a request forever;
/// on expiry the request is refused `ensure_in_progress`.
pub const ENSURE_LOCK_TIMEOUT: &str = "60s";

/// `SUBJECT_REALM`: the realm an ensure-minted service subject is registered in.
pub const SUBJECT_REALM: &str = "service";

/// The environment variable that overrides [`ENSURE_LOCK_TIMEOUT`].
const LOCK_TIMEOUT_ENV: &str = "ENSURE_LOCK_TIMEOUT";

/// SQLSTATE `55P03` `lock_not_available`: the bounded wait expired.
const LOCK_NOT_AVAILABLE: &str = "55P03";

/// `_lost_insert`'s note when the winner's licence carries this bundle.
const LOST_INSERT_EXISTS_NOTE: &str =
    "insert lost a concurrent race; the winner's licence carries this bundle";

/// `_lost_insert`'s note when the winner's licence does not carry this bundle.
const LOST_INSERT_REFUSE_NOTE: &str =
    "insert lost a concurrent race to a licence not carrying this bundle";

/// Recorded as `metering_pending` when no provisioner exists (`METERING_SERVICE_URL` unset): the
/// licence is live and unmetered, and the nightly reconcile sweep is its only repair path.
const METERING_NOT_CONFIGURED: &str =
    "metering not configured (METERING_SERVICE_URL unset); the reconcile sweep repairs it";

/// `tenant_lock_key`: the text hashed (`hashtextextended(_, 0)`) into the tenant's lock key.
///
/// The dependency release path takes the SAME key (`crate::db::dependencies`), deliberately: an
/// ensure that planned `exists` on a dependency must not have it released underneath it.
pub fn tenant_lock_key(tenant_id: Uuid) -> String {
    format!("bundle_ensure:{tenant_id}")
}

/// [`ENSURE_LOCK_TIMEOUT`], or the environment's override.
fn ensure_lock_timeout() -> String {
    match std::env::var(LOCK_TIMEOUT_ENV) {
        Ok(value) if !value.trim().is_empty() => value,
        _ => ENSURE_LOCK_TIMEOUT.to_owned(),
    }
}

/// `_TenantEnsureLock`: hold `pg_advisory_xact_lock` for one tenant across a whole plan + apply.
///
/// ⛔ pgbouncer (transaction pooling) is SAFE here and a session-level `pg_advisory_lock` would
/// NOT be: an open transaction pins its client to ONE server backend until COMMIT/ROLLBACK, so
/// the lock's acquisition and release both live on that backend. `lock_timeout` is set with
/// `set_config(..., true)` (= `SET LOCAL`), so it dies with the transaction.
///
/// Cost: one extra server connection per in-flight ensure. Release is by ROLLBACK; dropping the
/// value without [`TenantEnsureLock::release`] is also safe — sqlx queues a rollback when the
/// transaction drops — so a cancelled ensure cannot leave the tenant's lock held.
pub struct TenantEnsureLock {
    tx: Option<Transaction<'static, Postgres>>,
}

impl TenantEnsureLock {
    /// Take the tenant's lock, or refuse `ensure_in_progress` when the bounded wait expires.
    pub async fn acquire(pool: &PgPool, tenant_id: Uuid) -> Result<Self, EnsureError> {
        let mut tx = pool.begin().await?;
        let timeout = ensure_lock_timeout();
        sqlx::query("SELECT set_config('lock_timeout', $1, true)")
            .bind(&timeout)
            .execute(&mut *tx)
            .await?;

        let key = tenant_lock_key(tenant_id);
        let acquired = sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(&key)
            .execute(&mut *tx)
            .await;
        match acquired {
            // The holder transaction stays open on purpose: the xact lock lives as long as it does.
            Ok(_) => Ok(Self { tx: Some(tx) }),
            Err(error) => {
                // No lock was taken; dropping the aborted transaction rolls it back.
                drop(tx);
                if lock_not_available(&error) {
                    Err(EnsureError::Refused {
                        code: EnsureErrorCode::EnsureInProgress,
                        message: format!(
                            "another bundle ensure for tenant {tenant_id} is still running; \
                             retry once it has finished"
                        ),
                    })
                } else {
                    Err(EnsureError::Database(error))
                }
            }
        }
    }

    /// End the holder transaction, releasing the lock. A failed rollback is logged, not raised:
    /// the lock went with the connection either way.
    pub async fn release(mut self) {
        let Some(tx) = self.tx.take() else {
            return;
        };
        if let Err(error) = tx.rollback().await {
            tracing::warn!(
                %error,
                "bundle-ensure lock holder ended abnormally; the lock was released with its \
                 connection"
            );
        }
    }
}

/// The values every per-app write shares for the whole apply.
struct ApplyContext<'a> {
    tenant_id: Uuid,
    bundle_id: Uuid,
    bundle_key: &'a str,
    provisioned_by: &'a str,
    token_budget_override: Option<i32>,
    quota_override: Option<i32>,
}

/// Write what `plan` decided. Each app is independent; REFUSE apps are skipped.
///
/// Fills `ep.grants_created` (the hold and the grants, before any licence), `ep.lost_race`
/// (adoptions that lost their compare-and-set; the app is then REFUSE `lost_race`) and
/// `ep.unmetered` (inserted licences whose billing push did not complete; the reconcile sweep
/// repairs them). Metering failures never fail the call.
///
/// NOT locked by itself: the reference's `apply` is the lock-free write half, and
/// [`plan_and_apply`] is what holds [`TenantEnsureLock`] across plan + apply.
pub async fn apply(
    pool: &PgPool,
    ep: &mut EnsurePlan,
    provisioned_by: &str,
    granted_by: &str,
    meter: &MeteringProvisioner,
) -> Result<(), EnsureError> {
    apply_with(pool, ep, provisioned_by, granted_by, Some(meter)).await
}

/// [`apply`] with an OPTIONAL provisioner: `None` means `METERING_SERVICE_URL` is unset, so every
/// inserted licence is recorded metering-pending (only the reconcile sweep can repair it) rather
/// than failing a licence that is already live.
#[allow(clippy::too_many_arguments)]
async fn apply_with(
    pool: &PgPool,
    ep: &mut EnsurePlan,
    provisioned_by: &str,
    granted_by: &str,
    meter: Option<&MeteringProvisioner>,
) -> Result<(), EnsureError> {
    // ⛔ The hold and the grants FIRST, in their own single transaction, before any licence.
    ep.grants_created = ensure_bundle_grants(
        pool,
        ep.tenant_id,
        ep.bundle_id,
        granted_by,
        !ep.holds,
        &ep.dependency_grants,
    )
    .await?;

    // Copy the plan-wide values out before `ep.apps` is borrowed mutably by the loop.
    let bundle_key = ep.bundle_key.clone();
    let context = ApplyContext {
        tenant_id: ep.tenant_id,
        bundle_id: ep.bundle_id,
        bundle_key: &bundle_key,
        provisioned_by,
        token_budget_override: ep.token_budget_override,
        quota_override: ep.quota_override,
    };

    let mut lost_race: Vec<String> = Vec::new();
    let mut unmetered: Vec<String> = Vec::new();
    for app in ep.apps.iter_mut() {
        let Some((license_id, plan_id)) = apply_one(pool, &context, app, &mut lost_race).await?
        else {
            continue;
        };
        // ⛔ A metering failure must NEVER fail the request: the licence is live, the stamp stays
        // NULL, and the reconcile sweep repairs it. With no provisioner configured there was no
        // push at all, which is the same unmetered state.
        let failure = match meter {
            Some(meter) => meter
                .provision(
                    pool,
                    license_id,
                    context.tenant_id,
                    plan_id,
                    Some(app.app_id),
                )
                .await
                .err()
                .map(|error| error.to_string()),
            None => Some(METERING_NOT_CONFIGURED.to_owned()),
        };
        if let Some(why) = failure {
            app.metering_pending = why.clone();
            unmetered.push(format!("{} {license_id}: {why}", app.app_key));
        }
    }
    ep.lost_race = lost_race;
    ep.unmetered = unmetered;
    ep.applied = true;
    Ok(())
}

/// `_apply_one`: the DB writes for ONE app, returning the metering push it owes, if any.
async fn apply_one(
    pool: &PgPool,
    context: &ApplyContext<'_>,
    app: &mut AppPlan,
    lost_race: &mut Vec<String>,
) -> Result<Option<(Uuid, Uuid)>, EnsureError> {
    match app.action {
        AppAction::Adopt => {
            let license_id = app.license_id.ok_or_else(|| {
                EnsureError::Inconsistent(format!(
                    "the adopt decision for '{}' carries no licence id",
                    app.app_key
                ))
            })?;
            // The compare-and-set: `WHERE bundle_id IS NULL`. A concurrent adopter (or a licence
            // another bundle already owns) turns this app into REFUSE `lost_race`.
            if !adopt_license_into_bundle(pool, license_id, context.bundle_id).await? {
                lost_race.push(app.app_key.clone());
                app.action = AppAction::Refuse;
                app.refuse_reason = Some(RefuseReason::LostRace);
            }
            Ok(None)
        }
        AppAction::Insert => insert_one(pool, context, app, lost_race).await,
        AppAction::Exists | AppAction::Refuse => Ok(None),
    }
}

/// The INSERT path of `_apply_one`: subject, licence (absorbing a lost race), attribution.
async fn insert_one(
    pool: &PgPool,
    context: &ApplyContext<'_>,
    app: &mut AppPlan,
    lost_race: &mut Vec<String>,
) -> Result<Option<(Uuid, Uuid)>, EnsureError> {
    let plan_id = app.plan_id.ok_or_else(|| {
        EnsureError::Inconsistent(format!(
            "the insert decision for '{}' carries no plan id",
            app.app_key
        ))
    })?;
    let subject_id = create_or_get_subject(pool, &app.subject, SUBJECT_REALM)
        .await
        .map_err(|error| subject_error(&app.subject, error))?;

    let metadata = if app.dependency {
        json!({"provisioned_by": context.provisioned_by, "dependency_of": context.bundle_key})
    } else {
        json!({"provisioned_by": context.provisioned_by, "bundle": context.bundle_key})
    };
    let row = NewLicence {
        subject_id,
        app_id: app.app_id,
        plan_id,
        tenant_id: context.tenant_id,
        // A dependency licence is SHARED by every bundle whose closure needs the app, so it
        // names none (`ck_license_dependency_unbundled`).
        bundle_id: (!app.dependency).then_some(context.bundle_id),
        is_dependency: app.dependency,
        // Stored in the SAME INSERT, so the reconcile sweep that repairs an unmetered licence
        // pushes the override, never the plan's value.
        token_budget_override: context.token_budget_override,
        quota_override: context.quota_override,
        metadata,
    };
    match create_license(pool, row).await {
        Ok(license_id) => {
            // Same as the mint route: state the subject's parent org (migration 029). AFTER the
            // insert, because the just-inserted licence is itself evidence of parentage.
            attribute_subject_to_tenant(pool, subject_id, context.tenant_id).await?;
            app.license_id = Some(license_id);
            Ok(Some((license_id, plan_id)))
        }
        Err(CreateLicenceError::LostRace) => {
            lost_insert(pool, context, app, subject_id, lost_race).await?;
            Ok(None)
        }
        Err(CreateLicenceError::Database(error)) => Err(EnsureError::Database(error)),
    }
}

/// `_lost_insert`: the INSERT lost `uq_license_active_tenant_app_subject` to a concurrent writer.
///
/// Defence in depth behind the tenant lock — a writer outside it (the operator script's separate
/// plan/apply, a future caller) can still race. The failed INSERT ran in its own session, so no
/// other app's work is affected. Re-read the winner: carrying THIS bundle (or, for a dependency,
/// unbundled) it is `exists`, what a re-plan would say; anything else is REFUSE `lost_race`.
async fn lost_insert(
    pool: &PgPool,
    context: &ApplyContext<'_>,
    app: &mut AppPlan,
    subject_id: Uuid,
    lost_race: &mut Vec<String>,
) -> Result<(), EnsureError> {
    let outcome = resolve_lost_insert(
        pool,
        context.tenant_id,
        app.app_id,
        subject_id,
        context.bundle_id,
        app.dependency,
    )
    .await?;
    match outcome {
        LostRaceOutcome::Exists {
            licence_id,
            plan_id,
        } => {
            app.action = AppAction::Exists;
            app.license_id = Some(licence_id);
            app.existing_plan_id = Some(plan_id);
            app.plan_code = String::new();
            app.plan_id = None;
            app.note = LOST_INSERT_EXISTS_NOTE.to_owned();
        }
        LostRaceOutcome::Lost => {
            lost_race.push(app.app_key.clone());
            app.action = AppAction::Refuse;
            app.refuse_reason = Some(RefuseReason::LostRace);
            app.note = LOST_INSERT_REFUSE_NOTE.to_owned();
        }
    }
    Ok(())
}

/// `plan` and, when `apply`, `apply` — under the tenant lock when applying.
///
/// ⛔ A DRY RUN TAKES NO LOCK: it writes nothing and must not queue behind a live ensure.
#[allow(clippy::too_many_arguments)] // the reference's surface: the route card passes them all
pub async fn plan_and_apply(
    pool: &PgPool,
    tenant_id: Uuid,
    bundle_key: &str,
    plans: &[String],
    quota_override: Option<i32>,
    apply: bool,
    token_budget_override: Option<i32>,
    provisioned_by: &str,
    granted_by: &str,
    meter: &MeteringProvisioner,
) -> Result<EnsurePlan, EnsureError> {
    plan_and_apply_with(
        pool,
        tenant_id,
        bundle_key,
        plans,
        quota_override,
        apply,
        token_budget_override,
        provisioned_by,
        granted_by,
        Some(meter),
    )
    .await
}

/// [`plan_and_apply`] with an OPTIONAL provisioner, for the HTTP route: `None` (no
/// `METERING_SERVICE_URL`) records every insert metering-pending instead of pushing.
#[allow(clippy::too_many_arguments)] // the reference's surface: the route passes them all
pub async fn plan_and_apply_with(
    pool: &PgPool,
    tenant_id: Uuid,
    bundle_key: &str,
    plans: &[String],
    quota_override: Option<i32>,
    apply: bool,
    token_budget_override: Option<i32>,
    provisioned_by: &str,
    granted_by: &str,
    meter: Option<&MeteringProvisioner>,
) -> Result<EnsurePlan, EnsureError> {
    if !apply {
        return plan(
            pool,
            tenant_id,
            bundle_key,
            plans,
            quota_override,
            token_budget_override,
        )
        .await;
    }

    let lock = TenantEnsureLock::acquire(pool, tenant_id).await?;
    // Release on EVERY path (including a refusal from `plan`): the lock struct's drop is the
    // cancellation backstop, this is the ordinary path.
    let outcome = async {
        let mut ep = plan(
            pool,
            tenant_id,
            bundle_key,
            plans,
            quota_override,
            token_budget_override,
        )
        .await?;
        apply_with(pool, &mut ep, provisioned_by, granted_by, meter).await?;
        Ok::<EnsurePlan, EnsureError>(ep)
    }
    .await;
    lock.release().await;
    outcome
}

/// A failed subject registration, as the whole-request error this layer can carry.
///
/// The reference raises a 400 (malformed identifier) or a 409 (client-safe refusal) before any
/// licence is written. The only reachable arm from ensure is malformed: the subject is
/// `svc:<tenant>:<app key>` and the plan gates only the app key's LENGTH, so an app key carrying
/// whitespace or control characters reaches here. The tenant-collision refusal cannot be reached
/// by a `svc:` identifier, and a database fault keeps its own error.
fn subject_error(subject: &str, error: CreateSubjectError) -> EnsureError {
    match error {
        CreateSubjectError::Database(error) => EnsureError::Database(error),
        CreateSubjectError::Malformed { message } | CreateSubjectError::Refused { message } => {
            EnsureError::Inconsistent(format!(
                "cannot register subject {subject:?}: {message} (nothing further was written for \
                 this request)"
            ))
        }
    }
}

/// Whether `error` is SQLSTATE `55P03` (`lock_not_available`) — the bounded wait expired.
fn lock_not_available(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(database) if database.code().as_deref() == Some(LOCK_NOT_AVAILABLE))
}
