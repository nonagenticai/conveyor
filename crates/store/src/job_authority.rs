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
//! names none of them falls back to the job catalogue's `family`; a role outside the five has no
//! §6.7 row, so it is ungoverned here and the RBAC scope check remains its authority.
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
/// **`None` means the matrix does not govern this action for this role.** §6.7 is a table of
/// governed rows, not a closed allowlist for every registered action: an action the matrix does
/// not name stays governed by the RBAC scope check (`tenant_role_scope`) and the binding's own
/// `meta.authority` allowlist. Only the rows the SPEC names answer `Some(true | false)`, so a
/// delivery run may keep calling `documents.create`, `tool.*` and the rest of its work.
///
/// The rules, each citing its §6.7 row:
///
/// * **PM** — "Create/close project ✅", "Create/move card in project ✅", "Assign a job
///   binding ✅", "Open a kata ✅". The PM's own `pm.*` actions and the `projects.*` lifecycle it
///   owns are allowed; `coach.remedy`, `kata.adopt` and the `ops.*` set are denied.
/// * **Coach** — "Create/move card in project ✅ (remedy only)" and "Open a kata ✅". Its own
///   triage/escalation/interruption/remedy actions and opening a kata are allowed; the
///   `projects.*`/`pm.*` lifecycle, `kata.adopt`, general card create/move and the ops set are
///   denied.
/// * **Kata** — "Adopt a kata change: — (propose only)", "Open a kata: —". Its own loop
///   (`kata.propose`, `kata.measure`, …) is allowed; `kata.open`/`kata.adopt` and the PM's
///   lifecycle are denied.
/// * **Delivery** — "Create/move card in project — own card". Card create/move is allowed (the
///   ownership check is the action layer's); `coach.remedy`, the `projects.*`/`pm.*` lifecycle,
///   the kata gates, binding assignment and the ops set are denied.
/// * **Ops** — "Create/move card in project — own incident", "Open a kata ✅", and its own
///   `ops.*` actions (the destructive subset's human dual control is `approval_policy`, not this
///   matrix). The PM's lifecycle, `coach.remedy` and `kata.adopt` are denied.
///
/// An unknown role answers `None` for every action: the SPEC gives no matrix row for it, so this
/// guard has nothing to refuse and the RBAC scope check remains the authority.
pub fn compiled_default(meta_role: &str, action: &str) -> Option<bool> {
    let lifecycle = action.starts_with("projects.") || action.starts_with("pm.");
    let bindings = action.starts_with("job_bindings.");
    match meta_role {
        "pm" => {
            if action.starts_with("pm.")
                || action.starts_with("projects.")
                || matches!(
                    action,
                    "cards.create"
                        | "cards.move"
                        | "job_bindings.create"
                        | "job_bindings.retire"
                        | "kata.open"
                )
            {
                Some(true)
            } else if matches!(action, "coach.remedy" | "kata.adopt") || action.starts_with("ops.")
            {
                Some(false)
            } else {
                None
            }
        }
        "coach" => {
            if matches!(
                action,
                "coach.triage"
                    | "coach.escalate"
                    | "coach.interrupt"
                    | "coach.remedy"
                    | "kata.open"
            ) {
                Some(true)
            } else if lifecycle
                || bindings
                || matches!(action, "kata.adopt" | "cards.create" | "cards.move")
                || action.starts_with("ops.")
            {
                Some(false)
            } else {
                None
            }
        }
        "kata" => {
            if action.starts_with("kata.") {
                Some(!matches!(action, "kata.open" | "kata.adopt"))
            } else if lifecycle || bindings || action == "coach.remedy" {
                Some(false)
            } else {
                None
            }
        }
        "delivery" => {
            if matches!(action, "cards.create" | "cards.move") {
                Some(true)
            } else if lifecycle
                || bindings
                || matches!(action, "coach.remedy" | "kata.open" | "kata.adopt")
                || action.starts_with("ops.")
            {
                Some(false)
            } else {
                None
            }
        }
        "ops" => {
            if action.starts_with("ops.")
                || matches!(action, "kata.open" | "cards.create" | "cards.move")
            {
                Some(true)
            } else if lifecycle || bindings || matches!(action, "coach.remedy" | "kata.adopt") {
                Some(false)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// [`effective`]'s pure decision: a stored row wins; an absent row is the compiled default for a
/// governed row, and **allowed** for an action the §6.7 matrix does not name.
fn resolve_row(stored: Option<&JobAuthority>, meta_role: &str, action: &str) -> bool {
    match stored {
        Some(row) => row.allowed,
        None => compiled_default(meta_role, action).unwrap_or(true),
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
/// 4. The tenant row, else [`compiled_default`] for a governed row, must allow the action. A
///    role the matrix does not know, or an action it does not name, is ungoverned: allowed here,
///    with the RBAC scope check remaining its authority.
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

    /// The finding's floor: only the Coach's own row grants `coach.remedy`, and only the PM's
    /// row grants `projects.baseline` / `pm.*`. A delivery, kata or ops binding inherits neither
    /// without a tenant row (§6.7).
    #[test]
    fn the_floor_is_denied_outside_its_own_row() {
        for role in ["delivery", "kata", "ops", "pm"] {
            assert_eq!(
                compiled_default(role, "coach.remedy"),
                Some(false),
                "{role}"
            );
        }
        assert_eq!(compiled_default("coach", "coach.remedy"), Some(true));
        for role in ["delivery", "coach", "kata", "ops"] {
            assert_eq!(
                compiled_default(role, "projects.baseline"),
                Some(false),
                "{role}"
            );
            assert_eq!(compiled_default(role, "pm.charter"), Some(false), "{role}");
            assert_eq!(
                compiled_default(role, "pm.route_gate"),
                Some(false),
                "{role}"
            );
        }
        assert_eq!(compiled_default("pm", "projects.baseline"), Some(true));
    }

    /// §6.7's PM column: the project lifecycle, its own gate routing, card create/move, the
    /// binding assignment, and opening a kata.
    #[test]
    fn pm_owns_projects_and_its_own_actions() {
        assert_eq!(compiled_default("pm", "pm.charter"), Some(true));
        assert_eq!(compiled_default("pm", "pm.route_gate"), Some(true));
        assert_eq!(compiled_default("pm", "projects.baseline"), Some(true));
        assert_eq!(compiled_default("pm", "projects.close_request"), Some(true));
        assert_eq!(compiled_default("pm", "cards.create"), Some(true));
        assert_eq!(compiled_default("pm", "cards.move"), Some(true));
        assert_eq!(compiled_default("pm", "job_bindings.create"), Some(true));
        assert_eq!(compiled_default("pm", "kata.open"), Some(true));
        assert_eq!(compiled_default("pm", "coach.remedy"), Some(false));
        assert_eq!(compiled_default("pm", "ops.restart"), Some(false));
        // The matrix does not name ordinary work; RBAC owns it.
        assert_eq!(compiled_default("pm", "documents.create"), None);
    }

    /// §6.7's Coach column: "Create/move card in project — ✅ (remedy only)"; opening a kata is
    /// allowed and adopting one is not.
    #[test]
    fn the_coach_may_remedy_and_open_a_kata() {
        assert_eq!(compiled_default("coach", "coach.triage"), Some(true));
        assert_eq!(compiled_default("coach", "coach.escalate"), Some(true));
        assert_eq!(compiled_default("coach", "coach.interrupt"), Some(true));
        assert_eq!(compiled_default("coach", "coach.remedy"), Some(true));
        assert_eq!(compiled_default("coach", "kata.open"), Some(true));
        assert_eq!(compiled_default("coach", "kata.adopt"), Some(false));
        assert_eq!(compiled_default("coach", "cards.create"), Some(false));
        assert_eq!(compiled_default("coach", "projects.baseline"), Some(false));
    }

    /// §6.7's Kata column: its own loop, propose only for adoption, never opening itself.
    #[test]
    fn the_kata_proposes_but_never_opens_or_adopts() {
        assert_eq!(compiled_default("kata", "kata.propose"), Some(true));
        assert_eq!(compiled_default("kata", "kata.measure"), Some(true));
        assert_eq!(compiled_default("kata", "kata.open"), Some(false));
        assert_eq!(compiled_default("kata", "kata.adopt"), Some(false));
        assert_eq!(compiled_default("kata", "coach.remedy"), Some(false));
    }

    /// §6.7's Delivery column: create/move its own card; the governed cross-role rows are denied.
    /// This is the finding's exact acceptance: the delivery binding is refused `coach.remedy` and
    /// `projects.baseline` unless the tenant granted it, while ungoverned work stays with RBAC.
    #[test]
    fn delivery_moves_cards_and_is_denied_the_governed_rows() {
        assert_eq!(compiled_default("delivery", "cards.create"), Some(true));
        assert_eq!(compiled_default("delivery", "cards.move"), Some(true));
        assert_eq!(compiled_default("delivery", "coach.remedy"), Some(false));
        assert_eq!(
            compiled_default("delivery", "projects.baseline"),
            Some(false)
        );
        assert_eq!(compiled_default("delivery", "pm.plan"), Some(false));
        assert_eq!(compiled_default("delivery", "ops.restart"), Some(false));
        assert_eq!(compiled_default("delivery", "kata.open"), Some(false));
        // Ungoverned work is left to the RBAC scope check, not refused here.
        assert_eq!(compiled_default("delivery", "documents.create"), None);
        assert_eq!(compiled_default("delivery", "cards.get"), None);
    }

    /// §6.7's Ops column: its incident/ops actions and opening a kata.
    #[test]
    fn ops_owns_its_operations_actions() {
        assert_eq!(compiled_default("ops", "ops.incident"), Some(true));
        assert_eq!(compiled_default("ops", "ops.restart"), Some(true));
        assert_eq!(compiled_default("ops", "kata.open"), Some(true));
        assert_eq!(compiled_default("ops", "pm.plan"), Some(false));
        assert_eq!(compiled_default("ops", "coach.remedy"), Some(false));
        assert_eq!(compiled_default("ops", "kata.adopt"), Some(false));
    }

    /// An unknown or missing role has no §6.7 row: the matrix answers `None` (not governed), so
    /// an ordinary job binding keeps working and the RBAC scope check remains its authority.
    #[test]
    fn an_unknown_role_is_ungoverned() {
        assert_eq!(compiled_default("engineering", "cards.get"), None);
        assert_eq!(compiled_default("engineering", "coach.remedy"), None);
        assert_eq!(compiled_default("", "pm.charter"), None);
    }

    /// `effective`'s pure branch: a stored row wins (allow or deny); an absent row is the
    /// compiled default for a governed action and allowed for an action the matrix does not name.
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
        // The matrix does not name ordinary work, so an absent row does not deny it.
        assert!(resolve_row(None, "delivery", "documents.create"));
        // A tenant's explicit deny still wins on an ungoverned action.
        let denied = JobAuthority {
            meta_role: "delivery".to_owned(),
            action: "documents.create".to_owned(),
            allowed: false,
            updated_by: Some("dev:alice".to_owned()),
            updated_at: Utc::now(),
        };
        assert!(!resolve_row(Some(&denied), "delivery", "documents.create"));
    }
}
