#![forbid(unsafe_code)]
//! The 16 internal actions of this slice (S04 §5.1), each one `#[action]` declaration.
//!
//! Every handler runs inside the invocation engine's transaction, scoped to the caller's
//! tenant. A parent id is resolved by a tenant-scoped SELECT before it reaches a foreign
//! key (S03's validator proved a single-column FK accepts another tenant's row); a miss is
//! `404`, never a write.

use chrono::Utc;
use conveyor_auth::Role;
use conveyor_store as store;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::invoke::{ActionError, HandlerCtx};
use crate::registry::registry;
use crate::spec::{FieldError, HandlerOutput};
use crate::views::{
    ActionManifest, ActionManifestEntry, ActionSpecView, ActionsDescribeIn, AuditListIn, AuditView,
    DepartmentOut, DepartmentsCreateIn, EmptyIn, EventView, EventsListIn, JobIdIn, JobView,
    JobsListIn, LicenceView, MeView, MemberView, MembersSetRoleIn, MembershipView, Page, PageIn,
    PrincipalView, ScheduleOut, SchedulesUpsertIn, TenantSettingsIn, TenantView,
};

/// A parent that did not resolve in this tenant: a `404`, never an FK violation.
fn parent_error(error: store::actions::ActionParentError, action: &str) -> ActionError {
    match error {
        store::actions::ActionParentError::NotFound(_) => {
            ActionError::resource_not_found("such parent")
        }
        store::actions::ActionParentError::Parent(parent_error) => match parent_error {
            store::parent::ParentError::NotFound { .. } => {
                ActionError::resource_not_found("such parent")
            }
            store::parent::ParentError::Database(error) => store_error(error, action),
        },
        store::actions::ActionParentError::Database(error) => store_error(error, action),
    }
}

/// A store failure: a unique violation is a `409`, everything else a scrubbed `500`.
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

fn member_view(row: &store::actions::MemberRow) -> MemberView {
    MemberView {
        user_id: row.user_id.to_string(),
        display_name: row.display_name.clone(),
        email: row.email.clone(),
        role: row.role.clone(),
        department_id: row.department_id.map(|id| id.to_string()),
        version: row.version,
        created_at: row.created_at,
    }
}

fn department_view(row: &store::actions::DepartmentRow) -> DepartmentOut {
    DepartmentOut {
        id: row.id.to_string(),
        name: row.name.clone(),
        parent_id: row.parent_id.map(|id| id.to_string()),
        industry_id: row.industry_id.map(|id| id.to_string()),
        ordinal: row.ordinal,
        is_primary: row.is_primary,
        version: row.version,
        created_at: row.created_at,
    }
}

fn tenant_view(row: &store::actions::TenantRow) -> TenantView {
    TenantView {
        id: row.id.to_string(),
        slug: row.slug.clone(),
        name: row.name.clone(),
        status: row.status.clone(),
        settings: row.settings.clone(),
        version: row.version,
    }
}

fn audit_view(row: &store::actions::AuditRow) -> AuditView {
    AuditView {
        id: row.id.to_string(),
        actor_id: row.actor_id.map(|id| id.to_string()),
        actor_kind: row.actor_kind.clone(),
        action: row.action.clone(),
        status: row.status.clone(),
        resource_type: row.resource_type.clone(),
        resource_id: row.resource_id.map(|id| id.to_string()),
        request_id: row.request_id.clone(),
        details: row.details.clone(),
        created_at: row.created_at,
    }
}

fn job_view(row: &store::actions::JobRow) -> JobView {
    JobView {
        id: row.id.to_string(),
        kind: row.kind.clone(),
        dedupe_key: row.dedupe_key.clone(),
        status: row.status.clone(),
        payload: row.payload.clone(),
        run_after: row.run_after,
        attempts: row.attempts,
        max_attempts: row.max_attempts,
        last_error: row.last_error.clone(),
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn schedule_view(row: &store::actions::ScheduleRowOut) -> ScheduleOut {
    ScheduleOut {
        id: row.id.to_string(),
        kind: row.kind.clone(),
        cron: row.cron.clone(),
        timezone: row.timezone.clone(),
        enabled: row.enabled,
        next_run_at: row.next_run_at,
        last_run_at: row.last_run_at,
        version: row.version,
    }
}

fn event_view(row: &store::actions::EventRowOut) -> EventView {
    EventView {
        seq: row.seq,
        id: row.id.to_string(),
        topic: row.topic.clone(),
        event_type: row.event_type.clone(),
        aggregate_type: row.aggregate_type.clone(),
        aggregate_id: row.aggregate_id.to_string(),
        payload: row.payload.clone(),
        created_at: row.created_at,
    }
}

fn cursor_error(detail: &str) -> ActionError {
    ActionError::invalid_input(vec![FieldError {
        field: "cursor".into(),
        code: "invalid_cursor".into(),
        detail: detail.to_owned(),
    }])
}

// ── registry actions ────────────────────────────────────────────────────────────────────

/// Every action this caller's role may invoke.
#[crate::action(
    name = "actions.list",
    version = 1,
    kind = "internal",
    scopes = ["actions:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn actions_list(
    _tx: &mut PgConnection,
    ctx: HandlerCtx,
    _input: EmptyIn,
) -> Result<ActionManifest, ActionError> {
    let registry = registry();
    let entries = registry
        .manifest(ctx.role)
        .into_iter()
        .filter_map(|entry| serde_json::from_value::<ActionManifestEntry>(entry).ok())
        .collect();
    Ok(ActionManifest {
        actions: entries,
        generated_at: Utc::now(),
    })
}

/// One action's full contract.
#[crate::action(
    name = "actions.describe",
    version = 1,
    kind = "internal",
    scopes = ["actions:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn actions_describe(
    _tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ActionsDescribeIn,
) -> Result<ActionSpecView, ActionError> {
    let registry = registry();
    let spec = registry
        .get(&input.name)
        .ok_or_else(|| ActionError::not_found(&input.name))?;
    if !spec.permits(ctx.role) {
        // A callable-looking name the role cannot use is absent, not forbidden (S4-D6
        // applies to the manifest and MCP; describe follows the same rule).
        return Err(ActionError::not_found(&input.name));
    }
    Ok(ActionSpecView {
        name: spec.name.to_owned(),
        version: spec.version,
        kind: spec.kind.as_str().to_owned(),
        scopes: spec.scope_names().into_iter().map(str::to_owned).collect(),
        idempotent: spec.idempotent,
        timeout_ms: spec.timeout_ms,
        effect: spec.effect.as_str().to_owned(),
        confirm: spec.confirm.as_str().to_owned(),
        tenancy: spec.tenancy.as_str().to_owned(),
        spec_hash: spec.spec_hash(),
        input: spec.input_schema(),
        output: spec.output_schema(),
    })
}

// ── identity and tenant ────────────────────────────────────────────────────────────────

/// Who the caller is, in the tenant it acts in.
#[crate::action(
    name = "me.get",
    version = 1,
    kind = "internal",
    scopes = ["me:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "self"
)]
async fn me_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    _input: EmptyIn,
) -> Result<MeView, ActionError> {
    let Some((id, subject, display_name, email)) =
        store::actions::identity_view(&mut *tx, ctx.actor_id)
            .await
            .map_err(|error| store_error(error, "me.get"))?
    else {
        return Err(ActionError::resource_not_found("user"));
    };
    let Some((tenant_id, slug, name, role)) =
        store::actions::membership_view(&mut *tx, ctx.actor_id)
            .await
            .map_err(|error| store_error(error, "me.get"))?
    else {
        return Err(ActionError::resource_not_found("membership"));
    };
    let membership = MembershipView {
        tenant_id: tenant_id.to_string(),
        slug,
        name,
        role,
    };
    Ok(MeView {
        principal: PrincipalView {
            id,
            subject,
            display_name,
            email,
        },
        active_tenant: membership.clone(),
        memberships: vec![membership],
    })
}

/// The tenant's own row.
#[crate::action(
    name = "tenants.get",
    version = 1,
    kind = "internal",
    scopes = ["tenants:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tenants_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    _input: EmptyIn,
) -> Result<TenantView, ActionError> {
    let row = store::actions::tenant_get(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| store_error(error, "tenants.get"))?
        .ok_or_else(|| ActionError::resource_not_found("tenant"))?;
    Ok(tenant_view(&row))
}

/// Replace the tenant's settings document.
#[crate::action(
    name = "tenants.update_settings",
    version = 1,
    kind = "internal",
    scopes = ["tenants:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tenants_update_settings(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: TenantSettingsIn,
) -> Result<TenantView, ActionError> {
    let Some(version) = ctx.version else {
        return Err(ActionError::precondition(
            "tenants.update_settings replaces an existing row: If-Match (or `_version`) is required",
        ));
    };
    if !input.settings.is_object() {
        return Err(ActionError::invalid_input(vec![FieldError {
            field: "settings".into(),
            code: "invalid_type".into(),
            detail: "must be a JSON object".into(),
        }]));
    }
    match store::actions::tenant_update_settings(
        &mut *tx,
        ctx.tenant,
        &input.settings,
        Some(version),
    )
    .await
    .map_err(|error| store_error(error, "tenants.update_settings"))?
    {
        Ok(row) => Ok(tenant_view(&row)),
        Err(store::VersionMismatch::NotFound) => Err(ActionError::resource_not_found("tenant")),
        Err(store::VersionMismatch::Mismatch { current }) => Err(ActionError::precondition(
            format!("the tenant moved to version {current}"),
        )),
    }
}

// ── members and departments ────────────────────────────────────────────────────────────

/// One page of members.
#[crate::action(
    name = "members.list",
    version = 1,
    kind = "internal",
    scopes = ["members:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn members_list(
    tx: &mut PgConnection,
    _ctx: HandlerCtx,
    input: PageIn,
) -> Result<Page<MemberView>, ActionError> {
    let limit = store::actions::clamp_limit(input.limit);
    let after = match input.cursor.as_deref() {
        None => None,
        Some(cursor) => Some(
            Uuid::parse_str(cursor)
                .map_err(|_| cursor_error("a members cursor is a bare user id"))?,
        ),
    };
    // One extra row is read to decide whether a next page exists; it is not returned.
    let mut rows = store::actions::members_list(&mut *tx, limit + 1, after)
        .await
        .map_err(|error| store_error(error, "members.list"))?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last().map(|row| row.user_id.to_string())
    } else {
        None
    };
    Ok(Page {
        items: rows.iter().map(member_view).collect(),
        next_cursor,
    })
}

/// Set one member's role.
#[crate::action(
    name = "members.set_role",
    version = 1,
    kind = "internal",
    scopes = ["members:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn members_set_role(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: MembersSetRoleIn,
) -> Result<MemberView, ActionError> {
    let role = Role::parse(&input.role).ok_or_else(|| {
        ActionError::invalid_input(vec![FieldError {
            field: "role".into(),
            code: "invalid_value".into(),
            detail: "must be one of admin, member, viewer, kata_executor".into(),
        }])
    })?;
    // `kata_executor` is the synthetic tenant-admin (admin-equivalent scopes) and belongs to a
    // machine principal only; a human must not acquire it through a role change.
    if role.is_kata_executor() {
        let kind = store::actions::app_user_kind(&mut *tx, input.user_id)
            .await
            .map_err(|error| store_error(error, "members.set_role"))?;
        if !matches!(kind.as_deref(), Some("agent" | "service")) {
            return Err(ActionError::invalid_input(vec![FieldError {
                field: "role".into(),
                code: "invalid_value".into(),
                detail: "kata_executor may only be held by a machine principal (agent or service)"
                    .into(),
            }]));
        }
    }
    let Some(version) = ctx.version else {
        return Err(ActionError::precondition(
            "members.set_role replaces an existing membership: If-Match (or `_version`) is required",
        ));
    };
    let row = store::rbac::member_set_role_tx(
        &mut *tx,
        ctx.tenant,
        input.user_id,
        role.as_str(),
        Some(version),
    )
    .await
    .map_err(|error| crate::rbac::member_error(error, "members.set_role"))?;
    Ok(member_view(&row))
}

/// Every department of the tenant.
#[crate::action(
    name = "departments.list",
    version = 1,
    kind = "internal",
    scopes = ["departments:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn departments_list(
    tx: &mut PgConnection,
    _ctx: HandlerCtx,
    _input: EmptyIn,
) -> Result<Vec<DepartmentOut>, ActionError> {
    let rows = store::actions::departments_list(&mut *tx)
        .await
        .map_err(|error| store_error(error, "departments.list"))?;
    Ok(rows.iter().map(department_view).collect())
}

/// Create a department.
#[crate::action(
    name = "departments.create",
    version = 1,
    kind = "internal",
    scopes = ["departments:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn departments_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DepartmentsCreateIn,
) -> Result<HandlerOutput<DepartmentOut>, ActionError> {
    if input.name.trim().is_empty() {
        return Err(ActionError::invalid_input(vec![FieldError {
            field: "name".into(),
            code: "required".into(),
            detail: "must not be empty".into(),
        }]));
    }
    // The store resolves the parent in THIS tenant before the FK sees it (the RI check
    // bypasses RLS and would otherwise accept another tenant's id).
    let row = store::actions::department_create(
        &mut *tx,
        ctx.tenant,
        &store::actions::NewDepartment {
            name: input.name.trim(),
            parent_id: input.parent_id,
            industry_id: input.industry_id,
            ordinal: input.ordinal,
            is_primary: input.is_primary.unwrap_or(false),
        },
    )
    .await
    .map_err(|error| parent_error(error, "departments.create"))?;
    let version = conveyor_domain::Version::new(u64::try_from(row.version).unwrap_or(1))
        .unwrap_or(conveyor_domain::Version::INITIAL);
    Ok(HandlerOutput::created(department_view(&row)).with_etag(version.etag()))
}

// ── audit, jobs, schedules, events, licences ───────────────────────────────────────────

/// One page of the append-only audit log.
#[crate::action(
    name = "audit.list",
    version = 1,
    kind = "internal",
    scopes = ["audit:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn audit_list(
    tx: &mut PgConnection,
    _ctx: HandlerCtx,
    input: AuditListIn,
) -> Result<Page<AuditView>, ActionError> {
    let limit = store::actions::clamp_limit(input.page.limit);
    let cursor = match input.page.cursor.as_deref() {
        None => None,
        Some(raw) => Some(
            store::actions::decode_time_cursor(raw)
                .ok_or_else(|| cursor_error("not a cursor this service issued"))?,
        ),
    };
    let mut rows = store::actions::audit_list(&mut *tx, input.since, cursor, limit + 1)
        .await
        .map_err(|error| store_error(error, "audit.list"))?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last()
            .map(|row| store::actions::encode_time_cursor(row.created_at, row.id))
    } else {
        None
    };
    Ok(Page {
        items: rows.iter().map(audit_view).collect(),
        next_cursor,
    })
}

/// One page of jobs.
#[crate::action(
    name = "jobs.list",
    version = 1,
    kind = "internal",
    scopes = ["jobs:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn jobs_list(
    tx: &mut PgConnection,
    _ctx: HandlerCtx,
    input: JobsListIn,
) -> Result<Page<JobView>, ActionError> {
    if let Some(status) = input.status.as_deref()
        && !matches!(
            status,
            "pending" | "running" | "succeeded" | "failed" | "dead"
        )
    {
        return Err(ActionError::invalid_input(vec![FieldError {
            field: "status".into(),
            code: "invalid_value".into(),
            detail: "must be pending, running, succeeded, failed or dead".into(),
        }]));
    }
    let limit = store::actions::clamp_limit(input.page.limit);
    let cursor = match input.page.cursor.as_deref() {
        None => None,
        Some(raw) => Some(
            store::actions::decode_time_cursor(raw)
                .ok_or_else(|| cursor_error("not a cursor this service issued"))?,
        ),
    };
    let mut rows = store::actions::jobs_list(&mut *tx, input.status.as_deref(), cursor, limit + 1)
        .await
        .map_err(|error| store_error(error, "jobs.list"))?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last()
            .map(|row| store::actions::encode_time_cursor(row.created_at, row.id))
    } else {
        None
    };
    Ok(Page {
        items: rows.iter().map(job_view).collect(),
        next_cursor,
    })
}

/// Re-queue a failed or dead job.
#[crate::action(
    name = "jobs.retry",
    version = 1,
    kind = "internal",
    scopes = ["jobs:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn jobs_retry(
    tx: &mut PgConnection,
    _ctx: HandlerCtx,
    input: JobIdIn,
) -> Result<JobView, ActionError> {
    // Resolve first: a foreign tenant's id must look absent, and the current state decides
    // between 404 and 409.
    let Some(current) = store::actions::job_get(&mut *tx, input.id)
        .await
        .map_err(|error| store_error(error, "jobs.retry"))?
    else {
        return Err(ActionError::resource_not_found("such job"));
    };
    if !matches!(current.status.as_str(), "failed" | "dead") {
        return Err(ActionError::new(
            409,
            "conflict",
            format!(
                "job {} is {}; only a failed or dead job can be retried",
                current.id, current.status
            ),
        ));
    }
    let row = store::actions::job_retry(&mut *tx, input.id)
        .await
        .map_err(|error| store_error(error, "jobs.retry"))?
        .ok_or_else(|| ActionError::new(409, "conflict", "the job moved while retrying"))?;
    Ok(job_view(&row))
}

/// Every schedule of the tenant.
#[crate::action(
    name = "schedules.list",
    version = 1,
    kind = "internal",
    scopes = ["schedules:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn schedules_list(
    tx: &mut PgConnection,
    _ctx: HandlerCtx,
    _input: EmptyIn,
) -> Result<Vec<ScheduleOut>, ActionError> {
    let rows = store::actions::schedules_list(&mut *tx)
        .await
        .map_err(|error| store_error(error, "schedules.list"))?;
    Ok(rows.iter().map(schedule_view).collect())
}

/// Insert or replace a schedule.
#[crate::action(
    name = "schedules.upsert",
    version = 1,
    kind = "internal",
    scopes = ["schedules:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn schedules_upsert(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: SchedulesUpsertIn,
) -> Result<ScheduleOut, ActionError> {
    let cron = store::Cron::parse(&input.cron).map_err(|error| {
        ActionError::invalid_input(vec![FieldError {
            field: "cron".into(),
            code: "invalid_value".into(),
            detail: error.to_string(),
        }])
    })?;
    // The same cron value stored and used for the arithmetic: one source.
    let next_run_at = cron.next_after(Utc::now()).map_err(|error| {
        ActionError::invalid_input(vec![FieldError {
            field: "cron".into(),
            code: "invalid_value".into(),
            detail: error.to_string(),
        }])
    })?;
    let row = store::actions::schedule_upsert(
        &mut *tx,
        ctx.tenant,
        &input.kind,
        &input.cron,
        input.enabled,
        next_run_at,
        &input.payload,
    )
    .await
    .map_err(|error| store_error(error, "schedules.upsert"))?;
    Ok(schedule_view(&row))
}

/// One page of the outbox.
#[crate::action(
    name = "events.list",
    version = 1,
    kind = "internal",
    scopes = ["events:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn events_list(
    tx: &mut PgConnection,
    _ctx: HandlerCtx,
    input: EventsListIn,
) -> Result<Page<EventView>, ActionError> {
    if let Some(topics) = input.topics.as_deref() {
        let invalid: Vec<FieldError> = topics
            .iter()
            .filter(|topic| !store::outbox::valid_topic(topic))
            .map(|topic| FieldError {
                field: "topics".into(),
                code: "invalid_value".into(),
                detail: format!("{topic:?} is not <aggregate> or <aggregate>:<uuid>"),
            })
            .collect();
        if !invalid.is_empty() {
            return Err(ActionError::invalid_input(invalid));
        }
    }
    let limit = store::actions::clamp_limit(input.page.limit);
    let before_seq = match input.page.cursor.as_deref() {
        None => None,
        Some(raw) => Some(
            raw.parse::<i64>()
                .map_err(|_| cursor_error("an events cursor is a sequence number"))?,
        ),
    };
    let mut rows =
        store::actions::events_list(&mut *tx, input.topics.as_deref(), before_seq, limit + 1)
            .await
            .map_err(|error| store_error(error, "events.list"))?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last().map(|row| row.seq.to_string())
    } else {
        None
    };
    Ok(Page {
        items: rows.iter().map(event_view).collect(),
        next_cursor,
    })
}

/// The tenant's synced entitlement.
#[crate::action(
    name = "licences.get",
    version = 1,
    kind = "internal",
    scopes = ["licences:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn licences_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    _input: EmptyIn,
) -> Result<LicenceView, ActionError> {
    let row = store::actions::licence_get(&mut *tx, &ctx.app_key)
        .await
        .map_err(|error| store_error(error, "licences.get"))?
        .ok_or_else(|| ActionError::resource_not_found("licence row"))?;
    Ok(LicenceView {
        app_key: row.app_key,
        status: row.status,
        expires_at: row.expires_at,
        checked_at: row.checked_at,
    })
}
