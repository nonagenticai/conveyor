//! `feedback.create` · `.list` — the tenant's feedback about the platform (#842).
//!
//! A submission is validated before anything is written: a known kind, a non-empty title and
//! body, and a route that is a path, not a paragraph. The row is the tenant's own; the read is
//! the same tenant's rows, newest first. The operator's cross-tenant surface is the admin CLI.

use chrono::{DateTime, Utc};
use conveyor_store as store;
use conveyor_store::feedback::{FeedbackRow, NewFeedback};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::{FieldError, HandlerOutput};

/// The title cap, in characters.
const MAX_TITLE: usize = 200;
/// The body cap, in characters. A bug report is text, not an upload.
const MAX_BODY: usize = 20_000;
/// The route cap, in characters.
const MAX_ROUTE: usize = 512;
/// The page cap for a list.
const MAX_LIMIT: i64 = 200;

fn invalid(field: &str, code: &str, detail: impl Into<String>) -> ActionError {
    ActionError::invalid_input(vec![FieldError {
        field: field.to_owned(),
        code: code.to_owned(),
        detail: detail.into(),
    }])
}

/// A feedback row as every read returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct FeedbackOut {
    /// The row id.
    pub id: String,
    /// `bug | feature | other`.
    pub kind: String,
    /// The one-line summary.
    pub title: String,
    /// The report itself.
    pub body: String,
    /// The app route the submission came from, when the client sent one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    /// `open | triaged | accepted | declined | duplicate`.
    pub status: String,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
}

impl FeedbackOut {
    fn from_row(row: FeedbackRow) -> Self {
        Self {
            id: row.id.to_string(),
            kind: row.kind,
            title: row.title,
            body: row.body,
            route: row.route,
            status: row.status,
            created_at: row.created_at,
        }
    }
}

/// The create input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct FeedbackCreateIn {
    /// `bug | feature | other`.
    pub kind: String,
    /// The one-line summary.
    pub title: String,
    /// The report itself.
    pub body: String,
    /// The app route the user was on, when the client sends one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
}

/// Submit feedback about the platform.
#[crate::action(
    name = "feedback.create",
    version = 1,
    kind = "internal",
    scopes = ["feedback:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn feedback_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: FeedbackCreateIn,
) -> Result<HandlerOutput<FeedbackOut>, ActionError> {
    let kind = input.kind.trim().to_ascii_lowercase();
    if !store::feedback::KINDS.contains(&kind.as_str()) {
        return Err(invalid(
            "kind",
            "invalid_value",
            "must be bug, feature or other",
        ));
    }
    let title = input.title.trim();
    if title.is_empty() {
        return Err(invalid("title", "required", "must not be empty"));
    }
    if title.chars().count() > MAX_TITLE {
        return Err(invalid(
            "title",
            "too_long",
            format!("must be at most {MAX_TITLE} characters"),
        ));
    }
    let body = input.body.trim();
    if body.is_empty() {
        return Err(invalid("body", "required", "must not be empty"));
    }
    if body.chars().count() > MAX_BODY {
        return Err(invalid(
            "body",
            "too_long",
            format!("must be at most {MAX_BODY} characters"),
        ));
    }
    let route = input
        .route
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(route) = route
        && route.chars().count() > MAX_ROUTE
    {
        return Err(invalid(
            "route",
            "too_long",
            format!("must be at most {MAX_ROUTE} characters"),
        ));
    }
    let row = store::feedback::insert_tx(
        &mut *tx,
        ctx.tenant,
        &NewFeedback {
            author_id: ctx.actor_id,
            kind: &kind,
            title,
            body,
            route,
        },
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "feedback.create: insert");
        ActionError::internal()
    })?;
    Ok(HandlerOutput::ok(FeedbackOut::from_row(row)))
}

/// The list filter.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct FeedbackListIn {
    /// Only this status (`open | triaged | accepted | declined | duplicate`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// The page size (default 50, capped at 200).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// The tenant's feedback rows, newest first.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct FeedbackListOut {
    /// The rows.
    pub items: Vec<FeedbackOut>,
}

/// List this tenant's feedback, newest first.
#[crate::action(
    name = "feedback.list",
    version = 1,
    kind = "internal",
    scopes = ["feedback:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn feedback_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: FeedbackListIn,
) -> Result<HandlerOutput<FeedbackListOut>, ActionError> {
    let status = input
        .status
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(status) = status
        && !store::feedback::STATUSES.contains(&status)
    {
        return Err(invalid(
            "status",
            "invalid_value",
            "must be open, triaged, accepted, declined or duplicate",
        ));
    }
    let limit = i64::from(input.limit.unwrap_or(50).min(MAX_LIMIT as u32));
    let rows = store::feedback::list_tx(&mut *tx, ctx.tenant, status, limit)
        .await
        .map_err(|error| {
            tracing::error!(%error, "feedback.list");
            ActionError::internal()
        })?;
    Ok(HandlerOutput::ok(FeedbackListOut {
        items: rows.into_iter().map(FeedbackOut::from_row).collect(),
    }))
}
