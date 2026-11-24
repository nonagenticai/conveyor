#![forbid(unsafe_code)]
//! The tenant's meta-role × action authority matrix and the run-bound resolver
//! (conveyor#526, SPEC_V2 §6.7; ledger finding v1-w1b-5).
//!
//! §6.7 states the matrix as defaults and then says: "The matrix is data (`tenant_role_scope`
//! and the binding's `authority`), not branches in code." This module is the store half of that
//! sentence:
//!
//! * [`compiled_default`] holds the §6.7 cell, so an untouched tenant behaves exactly like the
//!   SPEC's default and the migration stays empty for every existing tenant.
//! * [`get`] / [`list`] / [`set`] are the tenant's overrides. A missing row is NEVER a grant:
//!   [`effective`] answers with the compiled default, and only [`set`] writes a row.
//! * [`enforce_run_tx`] is the run path's question: it resolves the run's binding, derives the
//!   binding's meta-role, and checks the binding's own `meta.authority` first. A binding narrows
//!   the tenant matrix; it can never widen it (§4.4).
//!
//! The role names are the §6.7 columns (`pm`, `coach`, `kata`, `delivery`, `ops`). A binding that
//! names none of them falls back to the job catalogue's `family`; an unknown role matches no
//! compiled cell and is therefore refused until the tenant grants it.
//!
//! Enforcement lives in the action layer (another slice); this module only answers the question
//! and returns a typed refusal the caller can map to `403 job_authority_forbidden`.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

/// Why an authority read, write or run check failed.
#[derive(Debug, thiserror::Error)]
pub enum JobAuthorityError {
    /// The run does not exist in this tenant. An absent *binding* on an existing run is allowed
    /// (the run predates the binding model); an absent run is a caller bug, not a permission.
    #[error("run {run_id} does not exist in this tenant")]
    RunNotFound {
        /// The run the caller named.
        run_id: Uuid,
    },
    /// §6.7: the meta-role is not granted this action — either the tenant stored `false`, no row
    /// exists and the compiled cell is `false`, or the binding's `meta.authority` does not name
    /// the action. This is the refusal variant the action layer maps to `403`.
    #[error("SPEC_V2 §6.7: meta-role {meta_role:?} is not granted {action}")]
    Forbidden {
        /// The resolved meta-role (empty when neither `meta_role` nor `family` names one).
        meta_role: String,
        /// The action the run tried to call.
        action: String,
    },
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// One tenant override: a meta-role's permission for one action.
#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct JobAuthority {
    /// The §6.7 meta-role (`pm`, `coach`, `kata`, `delivery`, `ops`).
    pub meta_role: String,
    /// The action's registry name (`coach.remedy`).
    pub action: String,
    /// The tenant's decision: `true` granted, `false` revoked.
    pub allowed: bool,
    /// The caller's principal string, for audit. `None` only for a hand-inserted row.
    pub updated_by: Option<String>,
    /// When the decision was last written.
    pub updated_at: DateTime<Utc>,
}

/// The §6.7 cell for `(meta_role, action)` when the tenant has stored no row.
///
/// Fail-closed: a role or an action the matrix does not name answers `false`. This is a
/// **default, never a grant** — [`set`] is the only way to widen it, and a tenant that has
/// decided nothing behaves exactly as the SPEC's default column says.
///
/// The rules, each citing its §6.7 row:
///
/// * **PM** — "Create/close project ✅", "Create/move card in project ✅", "Assign a job
///   binding ✅", "Open a kata ✅". The PM's own `pm.*` actions and the `projects.*` lifecycle it
///   owns are allowed. `crates/actions/src/pm/mod.rs::require_pm_binding` separately proves the
///   calling binding is a live `pm` job; this matrix does not branch on `job_key`.
/// * **Coach** — "Create/move card in project ✅ (remedy only)" and "Open a kata ✅". Its own
///   triage/escalation/interruption actions and opening a kata are allowed. `coach.remedy` is
///   deliberately NOT compiled: the finding's floor is that no non-granted meta-role — delivery
///   included — may call it, and sharing the `coach.` prefix does not make a role the coach. A
///   tenant that wants its coach to remedy stores the grant.
/// * **Kata** — "Adopt a kata change: — (propose only)", and "Open a kata: —". The kata job's own
///   loop (`kata.propose`, `kata.measure`, …) is allowed; `kata.open` and `kata.adopt` are not.
/// * **Delivery** — "Create/move card in project — own card", and nothing else. The ownership
///   check ("own card") is the action layer's, because card identity is not an argument here.
/// * **Ops** — "Create/move card in project — own incident", "Open a kata ✅", and the
///   destructive ops action it proposes with a dry-run (human dual control is `approval_policy`,
///   not this matrix). Its own `ops.*` actions and opening a kata are allowed.
///
/// Nothing else is allowed: every unknown role, and `coach.remedy` / `projects.baseline` / any
/// `pm.*` action for the four non-PM roles, answers `false`.
pub fn compiled_default(meta_role: &str, action: &str) -> bool {
    match meta_role {
        "pm" => action.starts_with("pm.") || action.starts_with("projects."),
        "coach" => matches!(
            action,
            "coach.triage" | "coach.escalate" | "coach.interrupt" | "kata.open"
        ),
        "kata" => action.starts_with("kata.") && !matches!(action, "kata.open" | "kata.adopt"),
        "delivery" => matches!(action, "cards.create" | "cards.move"),
        "ops" => action.starts_with("ops.") || action == "kata.open",
        _ => false,
    }
}

/// [`effective`]'s pure decision: a stored row wins; an absent row is the compiled default.
fn resolve_row(stored: Option<&JobAuthority>, meta_role: &str, action: &str) -> bool {
    match stored {
        Some(row) => row.allowed,
        None => compiled_default(meta_role, action),
    }
}

const COLUMNS: &str = "meta_role, action, allowed, updated_by, updated_at";

/// The tenant's stored decision for one `(meta_role, action)`, or `None` when it stored none.
///
/// An absent row is not a policy: the caller uses [`effective`], which substitutes the compiled
/// default. This function exists for the screen that lists what the tenant has actually changed.
pub async fn get(
    conn: &mut PgConnection,
    tenant: TenantId,
    meta_role: &str,
    action: &str,
) -> Result<Option<JobAuthority>, JobAuthorityError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM job_authority \
         WHERE tenant_id = $1 AND meta_role = $2 AND action = $3"
    );
    let row: Option<JobAuthority> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(meta_role)
        .bind(action)
        .fetch_optional(conn)
        .await?;
    Ok(row)
}

/// Every override the tenant has stored, ordered by meta-role and action.
pub async fn list(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Vec<JobAuthority>, JobAuthorityError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM job_authority WHERE tenant_id = $1 ORDER BY meta_role, action"
    );
    let rows: Vec<JobAuthority> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .fetch_all(conn)
        .await?;
    Ok(rows)
}

/// Insert or replace the tenant's decision. The only write that can widen [`compiled_default`].
///
/// `updated_by` is the caller's principal string (for example `dev:alice`); it is an audit fact,
/// not a foreign key, so it survives a principal rebind.
pub async fn set(
    conn: &mut PgConnection,
    tenant: TenantId,
    meta_role: &str,
    action: &str,
    allowed: bool,
    updated_by: &str,
) -> Result<JobAuthority, JobAuthorityError> {
    let sql = format!(
        "INSERT INTO job_authority (tenant_id, meta_role, action, allowed, updated_by, updated_at) \
         VALUES ($1, $2, $3, $4, $5, now()) \
         ON CONFLICT (tenant_id, meta_role, action) DO UPDATE \
            SET allowed = excluded.allowed, \
                updated_by = excluded.updated_by, \
                updated_at = now() \
         RETURNING {COLUMNS}"
    );
    let row: JobAuthority = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(meta_role)
        .bind(action)
        .bind(allowed)
        .bind(updated_by)
        .fetch_one(conn)
        .await?;
    Ok(row)
}

/// Whether this tenant lets `meta_role` call `action`: the stored row when present, else the
/// compiled §6.7 cell. **The resolver's read.**
pub async fn effective(
    conn: &mut PgConnection,
    tenant: TenantId,
    meta_role: &str,
    action: &str,
) -> Result<bool, JobAuthorityError> {
    let stored = get(&mut *conn, tenant, meta_role, action).await?;
    Ok(resolve_row(stored.as_ref(), meta_role, action))
}

/// Refuse the action unless `run`'s binding may call it. **The run path's check.**
///
/// Resolution, in order (SPEC_V2 §4.4 and §6.7):
///
/// 1. A run with no binding is allowed — the run predates the binding model and carries no
///    meta-role to judge.
/// 2. The meta-role is `binding.meta->>'meta_role'`, else the job catalogue's `family`.
/// 3. When `binding.meta->'authority'` is a string or an array, it NARROWS: the action must be
///    named there (`"*"` is the existing wildcard spelling, as in `store::meetings`). A
///    disallowed or absent action is refused even when the tenant row allows it. A free-form
///    value of another JSON type does not narrow.
/// 4. The tenant row, else [`compiled_default`], must allow the action.
///
/// The caller passes a `begin_scoped` connection (the `_tx` convention); both reads run under
/// the tenant's RLS.
pub async fn enforce_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    action: &str,
) -> Result<(), JobAuthorityError> {
    let found: Option<(Option<Uuid>, Option<Value>, Option<String>)> = sqlx::query_as(
        "SELECT r.job_binding_id, b.meta, j.family
           FROM run r
           LEFT JOIN job_binding b
             ON b.id = r.job_binding_id AND b.tenant_id = r.tenant_id
           LEFT JOIN job_catalogue j
             ON j.id = b.job_id AND j.tenant_id = b.tenant_id
          WHERE r.tenant_id = $1 AND r.id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((binding_id, binding_meta, family)) = found else {
        return Err(JobAuthorityError::RunNotFound { run_id });
    };
    if binding_id.is_none() {
        // An absent binding is allowed (the card's rule); there is no meta-role to narrow.
        return Ok(());
    }

    let meta = binding_meta.unwrap_or_else(|| Value::Object(serde_json::Map::new()));
    let meta_role = meta
        .get("meta_role")
        .and_then(Value::as_str)
        .filter(|role| !role.is_empty())
        .map(str::to_owned)
        .or_else(|| family.filter(|job_family| !job_family.is_empty()))
        .unwrap_or_default();

    if let Some(authority) = meta.get("authority") {
        let names_action = match authority {
            Value::String(value) => value == "*" || value == action,
            Value::Array(items) => items.iter().any(|item| item.as_str() == Some(action)),
            _ => true,
        };
        if !names_action {
            return Err(JobAuthorityError::Forbidden {
                meta_role,
                action: action.to_owned(),
            });
        }
    }

    if !effective(&mut *conn, tenant, &meta_role, action).await? {
        return Err(JobAuthorityError::Forbidden {
            meta_role,
            action: action.to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The finding's floor: the four non-PM meta-roles may never call the coach's remedy, the
    /// project baseline, or any `pm.*` action without a tenant grant.
    #[test]
    fn the_floor_is_denied_for_the_four_non_pm_roles() {
        for role in ["delivery", "coach", "kata", "ops"] {
            assert!(!compiled_default(role, "coach.remedy"), "{role}");
            assert!(!compiled_default(role, "projects.baseline"), "{role}");
            assert!(!compiled_default(role, "pm.charter"), "{role}");
            assert!(!compiled_default(role, "pm.route_gate"), "{role}");
        }
    }

    /// §6.7's PM column: the project lifecycle and its own gate routing.
    #[test]
    fn pm_owns_projects_and_its_own_actions() {
        assert!(compiled_default("pm", "pm.charter"));
        assert!(compiled_default("pm", "pm.route_gate"));
        assert!(compiled_default("pm", "projects.baseline"));
        assert!(compiled_default("pm", "projects.close_request"));
        assert!(!compiled_default("pm", "coach.remedy"));
        assert!(!compiled_default("pm", "ops.restart"));
    }

    /// §6.7's Coach column: remedy only where granted; opening a kata is allowed.
    #[test]
    fn the_coach_may_coach_and_open_a_kata() {
        assert!(compiled_default("coach", "coach.triage"));
        assert!(compiled_default("coach", "coach.escalate"));
        assert!(compiled_default("coach", "coach.interrupt"));
        assert!(compiled_default("coach", "kata.open"));
        assert!(!compiled_default("coach", "coach.remedy"));
        assert!(!compiled_default("coach", "kata.adopt"));
    }

    /// §6.7's Kata column: its own loop, propose only for adoption, never opening itself.
    #[test]
    fn the_kata_proposes_but_never_opens_or_adopts() {
        assert!(compiled_default("kata", "kata.propose"));
        assert!(compiled_default("kata", "kata.measure"));
        assert!(!compiled_default("kata", "kata.open"));
        assert!(!compiled_default("kata", "kata.adopt"));
    }

    /// §6.7's Delivery column: create/move its own card, and nothing across roles.
    #[test]
    fn delivery_moves_cards_only() {
        assert!(compiled_default("delivery", "cards.create"));
        assert!(compiled_default("delivery", "cards.move"));
        assert!(!compiled_default("delivery", "cards.assign"));
        assert!(!compiled_default("delivery", "ops.restart"));
        assert!(!compiled_default("delivery", "kata.open"));
    }

    /// §6.7's Ops column: its incident/ops actions and opening a kata.
    #[test]
    fn ops_owns_its_operations_actions() {
        assert!(compiled_default("ops", "ops.incident"));
        assert!(compiled_default("ops", "ops.restart"));
        assert!(compiled_default("ops", "kata.open"));
        assert!(!compiled_default("ops", "pm.plan"));
        assert!(!compiled_default("ops", "coach.remedy"));
    }

    /// An unknown or missing role is refused: a job family such as `engineering` is not a
    /// §6.7 meta-role, and silence is never consent.
    #[test]
    fn an_unknown_role_is_refused() {
        assert!(!compiled_default("engineering", "cards.get"));
        assert!(!compiled_default("", "pm.charter"));
    }

    /// `effective`'s pure branch: a stored row wins, and an absent row is the compiled default.
    #[test]
    fn a_stored_row_wins_and_an_absent_row_is_the_compiled_default() {
        let stored = JobAuthority {
            meta_role: "delivery".to_owned(),
            action: "coach.remedy".to_owned(),
            allowed: true,
            updated_by: Some("dev:alice".to_owned()),
            updated_at: Utc::now(),
        };
        assert!(resolve_row(Some(&stored), "delivery", "coach.remedy"));
        assert!(!resolve_row(None, "delivery", "coach.remedy"));
        assert!(resolve_row(None, "pm", "pm.plan"));
    }
}
