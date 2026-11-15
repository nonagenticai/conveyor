//! The ROW-WRITE half of bundle ensure: adoption's compare-and-set, the subject create-or-get,
//! the migration-029 tenant attribution, the licence insert, and the re-read that turns a lost
//! insert into `exists` or REFUSE `lost_race`.
//!
//! Ported from the reference's `services/bundle_ensure.py` (`_apply_one` :817, `_constraint_name`
//! :864, `_lost_insert` :869) and `models.py` (`create_or_get_subject` :4259,
//! `attribute_subject_to_tenant` :4518, `adopt_license_into_bundle` :6737, `create_license`
//! :6956). The plan/decision types live in [`super::bundle_ensure`]; this module writes rows and
//! leaves the action/refusal assignment to its orchestrator (a later card).
//!
//! ⛔ A licence insert that loses `uq_license_active_tenant_app_subject` is NOT an error. It is
//! re-read: a winner carrying this bundle (or, for a dependency insert, an unbundled winner)
//! makes the app `exists`; only any other winner is REFUSE `lost_race`.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

/// `SUBJECT_SUB_MAX_LEN` (`subject.keycloak_sub` is `VARCHAR(255)`).
const SUBJECT_SUB_MAX_LEN: usize = 255;

/// The partial unique index an ACTIVE licence may not duplicate: `(tenant_id, app_id, subject_id)
/// WHERE status = 'active'`.
const UQ_ACTIVE_TENANT_APP_SUBJECT: &str = "uq_license_active_tenant_app_subject";

/// A licence the ensure path is about to insert — the columns the reference's `create_license`
/// (:6956) writes for a bundle-ensure mint.
pub struct NewLicence {
    pub subject_id: Uuid,
    pub app_id: Uuid,
    pub plan_id: Uuid,
    pub tenant_id: Uuid,
    /// `None` for a dependency, which is shared by every bundle whose closure needs the app
    /// (`ck_license_dependency_unbundled`); the bundle id for a sold app.
    pub bundle_id: Option<Uuid>,
    pub is_dependency: bool,
    pub token_budget_override: Option<i32>,
    pub quota_override: Option<i32>,
    /// `license.metadata` (the reference's `license_metadata`): `provisioned_by` plus either
    /// `bundle` or `dependency_of`. Stored in the SAME INSERT, never a second write.
    pub metadata: Value,
}

/// Why a licence insert did not land.
#[derive(Debug, thiserror::Error)]
pub enum CreateLicenceError {
    /// The INSERT lost `uq_license_active_tenant_app_subject` to a concurrent writer. The caller
    /// re-reads the winner with [`resolve_lost_insert`]; this is never a 500.
    #[error("a concurrent writer won uq_license_active_tenant_app_subject")]
    LostRace,
    /// Any other database fault (a foreign key, a check, a connection) — a 500.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Why a subject was not created or returned.
#[derive(Debug, thiserror::Error)]
pub enum CreateSubjectError {
    /// `MalformedSubjectIdentifierError`: the identifier may never be STORED as a
    /// `keycloak_sub` (empty, whitespace/control characters, over-length, or an upper-case
    /// UUID). A 400, never a 409.
    #[error("{message}")]
    Malformed { message: String },
    /// `ClientSafeError`: the id is already a registered tenant and is about to BECOME a
    /// principal — the tenant/subject invariant's second door. A 409.
    #[error("{message}")]
    Refused { message: String },
    /// A database fault.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// What [`resolve_lost_insert`] found: the winner carries this bundle, or it does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LostRaceOutcome {
    /// The winner's licence already satisfies the app: the reference re-labels the plan `exists`.
    Exists { licence_id: Uuid, plan_id: Uuid },
    /// The winner is a licence not carrying this bundle: the reference refuses `lost_race`.
    Lost,
}

/// `adopt_license_into_bundle` (:6737): set `bundle_id` on an UNBUNDLED licence; True if it wrote.
///
/// ⛔ `WHERE bundle_id IS NULL` is the compare-and-set: a concurrent second adopter — or one that
/// raced a different bundle onto the row — is a no-op returning false instead of silently
/// re-pointing a licence another bundle already owns. Adopting a DEPENDENCY licence (migration
/// 059) PROMOTES it: `is_dependency` is cleared in the same UPDATE, so the tenant keeps its ONE
/// licence for the app.
pub async fn adopt_license_into_bundle(
    pool: &PgPool,
    license_id: Uuid,
    bundle_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE license
            SET bundle_id = $1, is_dependency = false, updated_at = now()
          WHERE id = $2 AND bundle_id IS NULL",
    )
    .bind(bundle_id)
    .bind(license_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// `create_or_get_subject` (:4259), the ensure path's call (`self_tenant_mint=false`).
///
/// Safe under concurrent first-login: the SELECT-then-INSERT is genuinely racy, so the loser's
/// unique violation on `subject_keycloak_sub_key` is absorbed by re-reading the winner, never
/// surfaced as an error.
///
/// Order is the reference's, and every step is load-bearing:
/// 1. refuse a malformed identifier before any lookup — never strip it (`'<sub>\n'` stripped IS
///    `<sub>`; storing it raw mints a second identity a stripping consumer would misread);
/// 2. an EXISTING subject is returned UNCHANGED — the rows that already violate are reported by
///    the gauge, never refused here;
/// 3. a UUID-shaped identifier that names a subject ROW ID resolves to that row (GET's order);
/// 4. an upper-case UUID that would be STORED is refused;
/// 5. a `keycloak_sub` that is already a registered TENANT is refused — unless the tenant is
///    already trading (it holds a licence of ANY status). ⛔ `self_tenant_mint` is a statement
///    about the CALL (portal-bff's `POST /licenses/user/{id}` with `tenant_id == subject_id`) and
///    the ensure path never makes it, so here the live-state exemption is the only one;
/// 6. INSERT, absorbing a concurrent winner.
pub async fn create_or_get_subject(
    pool: &PgPool,
    keycloak_sub: &str,
    keycloak_realm: &str,
) -> Result<Uuid, CreateSubjectError> {
    create_or_get_subject_for_mint(pool, keycloak_sub, keycloak_realm, false).await
}

/// [`create_or_get_subject`] with the `self_tenant_mint` statement exposed.
///
/// ⛔ Only `POST /licenses/user/{subject_id}` sets it, derived from `subject_id == tenant_id`
/// (compared case-folded, from THIS call and nothing else). The reference's guard refuses a
/// `keycloak_sub` that is already a `tenants.id` — except on that documented portal-bff path,
/// where the tenant the licence is minted for IS the subject id and the mint guard has already
/// established it is a registered tenant. Refusing it would turn a billing-hygiene guard into an
/// outage on a live path. Every other caller keeps the strict behaviour (`self_tenant_mint=false`).
pub async fn create_or_get_subject_for_mint(
    pool: &PgPool,
    keycloak_sub: &str,
    keycloak_realm: &str,
    self_tenant_mint: bool,
) -> Result<Uuid, CreateSubjectError> {
    let problem = subject_identifier_problem(keycloak_sub);
    let case_only = problem.is_some() && uuid_shaped_case(keycloak_sub).is_some();
    if let Some(problem) = problem
        && !case_only
    {
        return Err(CreateSubjectError::Malformed {
            message: format!("cannot register subject {keycloak_sub:?}: {problem}"),
        });
    }

    let existing: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM subject WHERE keycloak_sub = $1")
            .bind(keycloak_sub)
            .fetch_optional(pool)
            .await?;
    if let Some(id) = existing {
        return Ok(id);
    }

    // An identifier that is a subject ROW ID resolves to that row — it is never registered as a
    // NEW subject's keycloak_sub (admin callers hand back `subject_id` from a response).
    if uuid_shaped_case(keycloak_sub).is_some()
        && let Ok(row_id) = Uuid::parse_str(keycloak_sub)
    {
        let by_row_id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM subject WHERE id = $1")
            .bind(row_id)
            .fetch_optional(pool)
            .await?;
        if let Some(id) = by_row_id {
            return Ok(id);
        }
    }

    // An upper-case UUID that is not a row id would be STORED: refuse it.
    if let Some(problem) = problem {
        return Err(CreateSubjectError::Malformed {
            message: format!("cannot register subject {keycloak_sub:?}: {problem}"),
        });
    }

    // ⛔ A subject's `keycloak_sub` may not be an id that is ALREADY A REGISTERED TENANT.
    let collision: Option<String> = sqlx::query_scalar(
        "SELECT t.name::text FROM tenants t WHERE lower(t.id::text) = lower($1) LIMIT 1",
    )
    .bind(keycloak_sub)
    .fetch_optional(pool)
    .await?;
    if let Some(tenant_name) = collision {
        // ⛔ NOT AN ALLOWLIST: the exemption is derived from live state on every call — an id
        // that has ever traded (a licence under it, ANY status) is spared; one that never has is
        // refused. An exempted collision is still a violation being born, so it is logged ERROR.
        let tenant_id = Uuid::parse_str(keycloak_sub).ok();
        let exempt = self_tenant_mint
            || match tenant_id {
                Some(tenant_id) => tenant_has_licences(pool, tenant_id).await?,
                None => false,
            };
        if exempt {
            tracing::error!(
                keycloak_sub,
                keycloak_realm,
                tenant_name,
                exemption = if self_tenant_mint {
                    "self_tenant_mint"
                } else {
                    "tenant_has_licences"
                },
                "create_or_get_subject: ADMITTING a subject whose id is ALREADY a tenant — \
                 the identity is pre-existing (or this is the documented self-tenant mint), so \
                 refusing would break a live mint path; this IS the tenant/subject violation, \
                 reported not prevented"
            );
        } else {
            tracing::error!(
                keycloak_sub,
                keycloak_realm,
                tenant_name,
                "create_or_get_subject: REFUSING to register a subject whose id is ALREADY a \
                 tenant and holds NO licences — a genuinely new principal-as-subject registration"
            );
            return Err(CreateSubjectError::Refused {
                message: format!(
                    "cannot register subject {keycloak_sub:?}: that id is already registered \
                     tenant {tenant_name:?}. A tenant is an ORGANISATION and a subject is an \
                     identity that authenticates within one; registering a tenant id as a \
                     principal makes the authenticating identity the thing that pays, which \
                     nothing downstream can detect."
                ),
            });
        }
    }

    // Create new. A concurrent provision's unique violation is absorbed: re-read the winner.
    let inserted: Result<Uuid, sqlx::Error> = sqlx::query_scalar(
        "INSERT INTO subject (keycloak_sub, keycloak_realm) VALUES ($1, $2) RETURNING id",
    )
    .bind(keycloak_sub)
    .bind(keycloak_realm)
    .fetch_one(pool)
    .await;
    match inserted {
        Ok(id) => Ok(id),
        Err(error) => {
            if !is_unique_violation(&error) {
                return Err(CreateSubjectError::Database(error));
            }
            let won: Option<Uuid> =
                sqlx::query_scalar("SELECT id FROM subject WHERE keycloak_sub = $1")
                    .bind(keycloak_sub)
                    .fetch_optional(pool)
                    .await?;
            match won {
                Some(id) => Ok(id),
                // Not the race we absorb (e.g. a NOT NULL violation) — fail loud, as the
                // reference re-raises when the re-SELECT finds no winner.
                None => Err(CreateSubjectError::Database(error)),
            }
        }
    }
}

/// `attribute_subject_to_tenant` (:4518): state a subject's parent org, migration 029's column.
///
/// ⛔ The admission rule is migration 032's, verbatim, and it is the point of the method:
/// - `already_set` — never overwritten; a stated parentage is an operator fact, so the UPDATE
///   requires `tenant_id IS NULL` (a concurrent mint that set it first is `race_lost`, not an
///   error: this is best-effort bookkeeping and raises nothing the caller must handle);
/// - `tenant_unknown` / `tenant_is_machine` — the tenant must exist and must not be `kind =
///   'service'`; writing a service id as its own subject's parent would launder the very defect
///   the column exists to express;
/// - `ambiguous` / `no_licences` — set ONLY where the subject's licences (ALL statuses, exactly
///   as 032 counts) name exactly one tenant, and that tenant is `tenant_id`.
///
/// CALL IT AFTER THE LICENCE INSERT (the just-inserted licence is itself evidence of parentage).
/// The reference returns a reason string for the caller's log line; this port reproduces the ROW
/// EFFECT with the brief's `Result<(), sqlx::Error>` signature, so every non-`set` outcome is
/// simply "nothing written".
pub async fn attribute_subject_to_tenant(
    pool: &PgPool,
    subject_id: Uuid,
    tenant_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE subject s
            SET tenant_id = $2
          WHERE s.id = $1
            AND s.tenant_id IS NULL
            AND EXISTS (
                SELECT 1 FROM tenants t WHERE t.id = $2 AND t.kind <> 'service'
            )
            AND (
                SELECT count(DISTINCT l.tenant_id) FROM license l WHERE l.subject_id = s.id
            ) = 1
            AND EXISTS (
                SELECT 1 FROM license l WHERE l.subject_id = s.id AND l.tenant_id = $2
            )",
    )
    .bind(subject_id)
    .bind(tenant_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// `create_license` (:6956) for bundle ensure: INSERT `status='active'` with the reference's
/// columns and return the row id.
///
/// A unique violation of [`UQ_ACTIVE_TENANT_APP_SUBJECT`] is [`CreateLicenceError::LostRace`] —
/// the caller re-reads the winner with [`resolve_lost_insert`]; everything else is a real fault.
pub async fn create_license(pool: &PgPool, row: NewLicence) -> Result<Uuid, CreateLicenceError> {
    let result: Result<Uuid, sqlx::Error> = sqlx::query_scalar(
        "INSERT INTO license (
             subject_id, app_id, plan_id, tenant_id, bundle_id, is_dependency,
             token_budget_override, quota_override, metadata, status, starts_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'active', now())
         RETURNING id",
    )
    .bind(row.subject_id)
    .bind(row.app_id)
    .bind(row.plan_id)
    .bind(row.tenant_id)
    .bind(row.bundle_id)
    .bind(row.is_dependency)
    .bind(row.token_budget_override)
    .bind(row.quota_override)
    .bind(&row.metadata)
    .fetch_one(pool)
    .await;
    match result {
        Ok(id) => Ok(id),
        Err(error) => {
            if constraint_name(&error) == Some(UQ_ACTIVE_TENANT_APP_SUBJECT) {
                Err(CreateLicenceError::LostRace)
            } else {
                Err(CreateLicenceError::Database(error))
            }
        }
    }
}

/// One effectively-active licence the `_lost_insert` re-read found.
#[derive(Debug, sqlx::FromRow)]
struct ActiveLicenceRow {
    id: Uuid,
    plan_id: Uuid,
    bundle_id: Option<Uuid>,
}

/// `_lost_insert` (:869): the INSERT lost [`UQ_ACTIVE_TENANT_APP_SUBJECT`] to a concurrent writer.
///
/// Re-read the tenant's effectively-active licence for that app+subject (the same predicate the
/// reference's `list_tenant_app_licences` uses: `status='active'`, not soft-deleted, inside its
/// time window; one row at most, because the index is what was violated). If the winner carries
/// THIS bundle — or, for a dependency insert, is unbundled (a dependency licence is shared by
/// every bundle whose closure needs the app) — it is [`LostRaceOutcome::Exists`] with the
/// winner's id/plan, what a re-plan would say. Otherwise it is [`LostRaceOutcome::Lost`], and the
/// orchestrator REFUSEs the app `lost_race`, exactly as an adoption that lost its compare-and-set.
pub async fn resolve_lost_insert(
    pool: &PgPool,
    tenant_id: Uuid,
    app_id: Uuid,
    subject_id: Uuid,
    bundle_id: Uuid,
    dependency: bool,
) -> Result<LostRaceOutcome, sqlx::Error> {
    let now: DateTime<Utc> = Utc::now();
    let winner: Option<ActiveLicenceRow> = sqlx::query_as(
        "SELECT l.id, l.plan_id, l.bundle_id
           FROM license l
          WHERE l.tenant_id = $1
            AND l.app_id = $2
            AND l.subject_id = $3
            AND l.status = 'active'
            AND l.deleted_at IS NULL
            AND l.starts_at <= $4
            AND (l.expires_at IS NULL OR l.expires_at >= $4)
          ORDER BY l.created_at, l.id
          LIMIT 1",
    )
    .bind(tenant_id)
    .bind(app_id)
    .bind(subject_id)
    .bind(now)
    .fetch_optional(pool)
    .await?;
    Ok(match winner {
        Some(winner)
            if winner.bundle_id == Some(bundle_id)
                || (dependency && winner.bundle_id.is_none()) =>
        {
            LostRaceOutcome::Exists {
                licence_id: winner.id,
                plan_id: winner.plan_id,
            }
        }
        _ => LostRaceOutcome::Lost,
    })
}

/// `tenant_has_licences` (`services/tenant_subject_invariant.py`): any licence under the tenant,
/// ANY status. The question is "has this id ever traded as an organisation", not "is it trading
/// right now" — keying on `active` would make the exemption blink with ordinary lifecycle.
async fn tenant_has_licences(pool: &PgPool, tenant_id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM license WHERE tenant_id = $1)")
        .bind(tenant_id)
        .fetch_one(pool)
        .await
}

/// `subject_identifier_problem` (:1340): why `value` may not be stored as `keycloak_sub`.
///
/// ⛔ REFUSE, NEVER NORMALISE: an identifier carrying whitespace or control characters is refused
/// outright (stripping would silently alias another subject). A UUID-shaped identifier must be
/// the lower-case form Keycloak issues.
///
/// ⚠️ The reference also refuses Unicode FORMAT characters (category `Cf`); Rust's standard
/// library has no general-category query, so that arm is not reproduced here. It is not
/// reachable from bundle ensure, whose identifiers are `svc:<uuid>:<app key>`.
fn subject_identifier_problem(value: &str) -> Option<&'static str> {
    if value.is_empty() {
        return Some("subject identifier must be a non-empty string");
    }
    if value
        .chars()
        .any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return Some("subject identifier must not contain whitespace or control characters");
    }
    if value.chars().count() > SUBJECT_SUB_MAX_LEN {
        return Some("subject identifier must be at most 255 characters");
    }
    if uuid_shaped_case(value) == Some(false) {
        return Some(
            "a UUID-shaped subject identifier must be lower case (the form Keycloak issues)",
        );
    }
    None
}

/// Python's `_ANY_CASE_UUID_RE` / `_KEYCLOAK_SUB_RE`: `Some(true)` for a lower-case hyphenated
/// UUID, `Some(false)` for that shape with an upper-case hex digit, `None` when not UUID-shaped.
///
/// Deliberately NOT `Uuid::parse_str`, which also accepts braced, simple and URN forms the
/// reference's fullmatch rejects.
fn uuid_shaped_case(value: &str) -> Option<bool> {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return None;
    }
    for (index, byte) in bytes.iter().enumerate() {
        match index {
            8 | 13 | 18 | 23 => {
                if *byte != b'-' {
                    return None;
                }
            }
            _ => {
                if !byte.is_ascii_hexdigit() {
                    return None;
                }
            }
        }
    }
    Some(!bytes.iter().any(u8::is_ascii_uppercase))
}

/// `_constraint_name` (:864): the violated constraint's name, when the fault is a database fault.
fn constraint_name(error: &sqlx::Error) -> Option<&str> {
    error
        .as_database_error()
        .and_then(|database| database.constraint())
}

/// SQLSTATE `23505`.
fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(|database| database.is_unique_violation())
}
