//! The RBAC surface: the tenant's role policy, invites, and member removal.
//!
//! Three decisions shape this module:
//!
//! - **Absent row = compiled default.** `roles.policy_get` returns both, so the screen can mark
//!   an override with a dot and show what the default was. `policy_set` stores only the entries
//!   the admin submitted; `policy_reset` deletes them all.
//! - **Admin is not editable.** Only `member` and `viewer` may carry an override. An admin who
//!   could narrow the admin role would be able to lock the tenant out of the screen that
//!   restores it, and `kata_executor` is admin-equivalent for a machine principal.
//! - **The kernel cannot be revoked.** `me`, `tenants`, `actions`, `events` and `members` are
//!   refused in `policy_set`; a tenant that revoked `members:write` from itself could never
//!   grant it back.
//!
//! The last-admin guard lives in the store (`rbac::member_set_role_tx` / `member_remove_tx`),
//! inside the same transaction and advisory lock as the write.

use conveyor_auth::Role;
use conveyor_store as store;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::{FieldError, Scope};

/// The roles a tenant may configure. `admin` and `kata_executor` are compiled-only.
const EDITABLE_ROLES: [&str; 2] = ["member", "viewer"];

/// The scopes nobody may revoke, by resource.
const KERNEL_RESOURCES: [&str; 5] = ["me", "tenants", "actions", "events", "members"];

fn invalid(field: &str, detail: impl Into<String>) -> ActionError {
    ActionError::invalid_input(vec![FieldError {
        field: field.to_owned(),
        code: "invalid_value".into(),
        detail: detail.into(),
    }])
}

fn store_error(error: sqlx::Error, action: &str) -> ActionError {
    if let Some(database) = error.as_database_error()
        && database.is_unique_violation()
    {
        return ActionError::new(
            409,
            "conflict",
            format!("{action:?} would create a duplicate row in this tenant"),
        );
    }
    tracing::error!(%error, action, "store failure");
    ActionError::internal()
}

pub(crate) fn member_error(error: store::rbac::MemberStoreError, action: &str) -> ActionError {
    match error {
        store::rbac::MemberStoreError::NotFound => ActionError::resource_not_found("such member"),
        store::rbac::MemberStoreError::VersionMismatch { current } => {
            ActionError::precondition(format!("the membership moved to version {current}"))
        }
        store::rbac::MemberStoreError::LastAdmin => ActionError::new(
            409,
            "last_admin",
            "this is the tenant's last admin; designate another admin first",
        ),
        store::rbac::MemberStoreError::Database(error) => store_error(error, action),
    }
}

// ── views ────────────────────────────────────────────────────────────────────────────────

/// One cell of the role × scope matrix.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PolicyCell {
    pub role: String,
    pub scope: String,
    /// The effective answer (`override` when stored, else `default`).
    pub allowed: bool,
    /// The compiled default for this role and scope.
    pub default: bool,
    /// True when the tenant stored this cell.
    pub overridden: bool,
}

/// `roles.policy_get|policy_set|policy_reset` output — the whole matrix in one document.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PolicyView {
    /// Every scope this build knows, sorted.
    pub scopes: Vec<String>,
    /// The editable roles, in the order the screen renders them.
    pub roles: Vec<String>,
    /// One cell per (role, scope) the screen may show.
    pub cells: Vec<PolicyCell>,
}

/// `roles.policy_set` input — the complete set of overrides the tenant wants.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PolicySetIn {
    /// Only overrides; an omitted cell means "use the compiled default".
    pub entries: Vec<PolicyEntryIn>,
}

/// One override.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PolicyEntryIn {
    /// `member` or `viewer`.
    pub role: String,
    /// A scope's wire spelling, e.g. `projects:write`.
    pub scope: String,
    pub allowed: bool,
}

/// `roles.policy_reset` takes nothing.
pub type PolicyResetIn = crate::views::EmptyIn;

/// `invites.create` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct InviteCreateIn {
    /// The address the invitee signs in with (matched case-insensitively, verified required).
    pub email: String,
    /// `admin | member | viewer`.
    pub role: String,
}

/// `invites.list` input.
pub type InviteListIn = crate::views::PageIn;

/// `invites.revoke` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct InviteRevokeIn {
    pub id: Uuid,
}

/// One invite.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct InviteView {
    pub id: String,
    pub email: String,
    pub role: String,
    pub status: String,
    pub invited_by: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub accepted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub version: i64,
}

/// `members.remove` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MemberRemoveIn {
    pub user_id: Uuid,
}

/// `members.remove` output.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MemberRemoved {
    pub user_id: String,
    pub removed: bool,
}

fn invite_view(row: &store::invite::InviteRow) -> InviteView {
    InviteView {
        id: row.id.to_string(),
        email: row.email.clone(),
        role: row.role.clone(),
        status: row.status.clone(),
        invited_by: row.invited_by.map(|id| id.to_string()),
        created_at: row.created_at,
        accepted_at: row.accepted_at,
        version: row.version,
    }
}

/// The role names the matrix renders: the editable pair first, then the fixed ones.
fn matrix_roles() -> Vec<Role> {
    vec![Role::Member, Role::Viewer, Role::Admin, Role::KataExecutor]
}

/// The kernel rule: the scope's resource half.
fn is_kernel(scope: &str) -> bool {
    scope
        .split_once(':')
        .map(|(resource, _)| KERNEL_RESOURCES.contains(&resource))
        .unwrap_or(false)
}

fn build_policy_view(overrides: &[(String, String, bool)]) -> PolicyView {
    let scopes: Vec<String> = Scope::ALL.iter().map(|s| s.as_str().to_owned()).collect();
    let roles: Vec<String> = matrix_roles()
        .iter()
        .map(|r| r.as_str().to_owned())
        .collect();
    let mut cells = Vec::with_capacity(roles.len() * scopes.len());
    for role in matrix_roles() {
        for scope in Scope::ALL {
            let stored = overrides
                .iter()
                .find(|(r, s, _)| r == role.as_str() && s == scope.as_str());
            let default = scope.granted_to(role);
            cells.push(PolicyCell {
                role: role.as_str().to_owned(),
                scope: scope.as_str().to_owned(),
                allowed: stored.map(|(_, _, allowed)| *allowed).unwrap_or(default),
                default,
                overridden: stored.is_some(),
            });
        }
    }
    PolicyView {
        scopes,
        roles,
        cells,
    }
}

/// Read every stored override of the tenant (all roles).
async fn stored_overrides(
    tx: &mut PgConnection,
) -> Result<Vec<(String, String, bool)>, sqlx::Error> {
    sqlx::query_as("SELECT role, scope, allowed FROM tenant_role_scope ORDER BY role, scope")
        .fetch_all(&mut *tx)
        .await
}

// ── actions ──────────────────────────────────────────────────────────────────────────────

/// The tenant's role policy: compiled defaults plus stored overrides.
#[crate::action(
    name = "roles.policy_get",
    version = 1,
    kind = "internal",
    scopes = ["members:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn roles_policy_get(
    tx: &mut PgConnection,
    _ctx: HandlerCtx,
    _input: crate::views::EmptyIn,
) -> Result<PolicyView, ActionError> {
    let overrides = stored_overrides(&mut *tx)
        .await
        .map_err(|error| store_error(error, "roles.policy_get"))?;
    Ok(build_policy_view(&overrides))
}

/// Replace the tenant's overrides.
#[crate::action(
    name = "roles.policy_set",
    version = 1,
    kind = "internal",
    scopes = ["members:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn roles_policy_set(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PolicySetIn,
) -> Result<PolicyView, ActionError> {
    let mut entries: Vec<(&str, &str, bool)> = Vec::with_capacity(input.entries.len());
    for entry in &input.entries {
        if !EDITABLE_ROLES.contains(&entry.role.as_str()) {
            return Err(invalid(
                "role",
                format!(
                    "only {} may carry a policy override; admin is fixed",
                    EDITABLE_ROLES.join(" and ")
                ),
            ));
        }
        // The scope must be one this build knows; an unknown spelling would be stored and then
        // silently never consulted by `permits_session`.
        let Some(scope) = Scope::ALL.iter().find(|s| s.as_str() == entry.scope) else {
            return Err(invalid("scope", format!("unknown scope {:?}", entry.scope)));
        };
        if is_kernel(entry.scope.as_str()) {
            return Err(invalid(
                "scope",
                format!("{} is a kernel scope and cannot be revoked", entry.scope),
            ));
        }
        // Pin the static spelling so the stored row cannot differ by case.
        entries.push((entry.role.as_str(), scope.as_str(), entry.allowed));
    }
    store::rbac::set_policy_tx(&mut *tx, ctx.tenant, &entries)
        .await
        .map_err(|error| store_error(error, "roles.policy_set"))?;
    let overrides = stored_overrides(&mut *tx)
        .await
        .map_err(|error| store_error(error, "roles.policy_set"))?;
    Ok(build_policy_view(&overrides))
}

/// Delete every override; the compiled table applies again.
#[crate::action(
    name = "roles.policy_reset",
    version = 1,
    kind = "internal",
    scopes = ["members:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn roles_policy_reset(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    _input: PolicyResetIn,
) -> Result<PolicyView, ActionError> {
    store::rbac::reset_policy_tx(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| store_error(error, "roles.policy_reset"))?;
    Ok(build_policy_view(&[]))
}

/// Invite an email to this tenant.
#[crate::action(
    name = "invites.create",
    version = 1,
    kind = "internal",
    scopes = ["members:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn invites_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: InviteCreateIn,
) -> Result<InviteView, ActionError> {
    let email = input.email.trim();
    if email.len() < 3 || !email.contains('@') {
        return Err(invalid("email", "an email address is required"));
    }
    if !matches!(input.role.as_str(), "admin" | "member" | "viewer") {
        return Err(invalid("role", "must be one of admin, member, viewer"));
    }
    let row =
        store::invite::create_tx(&mut *tx, ctx.tenant, email, &input.role, Some(ctx.actor_id))
            .await
            .map_err(|error| store_error(error, "invites.create"))?;
    Ok(invite_view(&row))
}

/// Every invite of this tenant.
#[crate::action(
    name = "invites.list",
    version = 1,
    kind = "internal",
    scopes = ["members:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn invites_list(
    tx: &mut PgConnection,
    _ctx: HandlerCtx,
    input: InviteListIn,
) -> Result<crate::views::Page<InviteView>, ActionError> {
    let limit = store::actions::clamp_limit(input.limit);
    let after = match input.cursor.as_deref() {
        None => None,
        Some(cursor) => Some(
            Uuid::parse_str(cursor)
                .map_err(|_| invalid("cursor", "an invites cursor is a bare invite id"))?,
        ),
    };
    // The table is small; the cursor is applied in memory so the listing keeps one shape.
    let rows = store::invite::list_tx(&mut *tx)
        .await
        .map_err(|error| store_error(error, "invites.list"))?;
    let start = rows
        .iter()
        .position(|row| after.is_some_and(|after| row.id == after))
        .map(|position| position + 1)
        .unwrap_or(0);
    let mut page: Vec<_> = rows
        .into_iter()
        .skip(start)
        .take(limit as usize + 1)
        .collect();
    let next_cursor = if page.len() as i64 > limit {
        page.pop();
        page.last().map(|row| row.id.to_string())
    } else {
        None
    };
    Ok(crate::views::Page {
        items: page.iter().map(invite_view).collect(),
        next_cursor,
    })
}

/// Revoke a pending invite.
#[crate::action(
    name = "invites.revoke",
    version = 1,
    kind = "internal",
    scopes = ["members:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn invites_revoke(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: InviteRevokeIn,
) -> Result<MemberRemoved, ActionError> {
    let revoked = store::invite::revoke_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| store_error(error, "invites.revoke"))?;
    if !revoked {
        return Err(ActionError::resource_not_found("such pending invite"));
    }
    Ok(MemberRemoved {
        user_id: input.id.to_string(),
        removed: true,
    })
}

/// Remove a member. The last human admin cannot be removed.
#[crate::action(
    name = "members.remove",
    version = 1,
    kind = "internal",
    scopes = ["members:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn members_remove(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: MemberRemoveIn,
) -> Result<MemberRemoved, ActionError> {
    store::rbac::member_remove_tx(&mut *tx, ctx.tenant, input.user_id)
        .await
        .map_err(|error| member_error(error, "members.remove"))?;
    Ok(MemberRemoved {
        user_id: input.user_id.to_string(),
        removed: true,
    })
}
