#![forbid(unsafe_code)]
//! `tools.list|get`, `toolkits.list`, `tool_calls.list` — the catalogue and ledger reads
//! (SPEC_V2 §11.8, §11.9; issue #557).
//!
//! Every read is tenant-scoped, cursor-paged with the shared `(created_at, id)` cursor, and
//! returns only **published** rows: a draft or retired toolkit is indistinguishable from a
//! missing one, exactly as `store::tool::resolve` treats it. `tools.get` is the one read that
//! returns the full closed contract — input and output schemas, idempotency, redaction and the
//! verification declaration — because that is what a caller needs before a call.

use chrono::{DateTime, Utc};
use conveyor_store as store;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::FieldError;
use crate::views::{Page, PageIn};

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 200;

fn invalid(field: &str, code: &str, detail: String) -> ActionError {
    ActionError::invalid_input(vec![FieldError {
        field: field.to_owned(),
        code: code.to_owned(),
        detail,
    }])
}

/// The decoded cursor and page size a catalogue read applies.
type PageCursor = (Option<(DateTime<Utc>, Uuid)>, i64);

fn page(input: &PageIn) -> Result<PageCursor, ActionError> {
    let limit = i64::from(input.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT));
    let cursor = match input.cursor.as_deref() {
        None => None,
        Some(raw) => Some(store::actions::decode_time_cursor(raw).ok_or_else(|| {
            invalid(
                "cursor",
                "invalid",
                "not a cursor this service issued".to_owned(),
            )
        })?),
    };
    Ok((cursor, limit))
}

/// `tools.list`'s input.
#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
pub struct ToolsListIn {
    /// The shared page envelope.
    #[serde(default)]
    pub page: PageIn,
}

/// One catalogue row.
#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct ToolListRow {
    /// The toolkit key.
    pub toolkit: String,
    /// The pinned version.
    pub toolkit_version: i64,
    /// The operation key.
    pub tool: String,
    /// The human summary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// `read | write | spend | destructive`.
    pub effect: String,
    /// `required | optional | forbidden`.
    pub idempotency: String,
    /// `published`.
    pub status: String,
    /// The row's creation time.
    pub created_at: DateTime<Utc>,
}

#[crate::action(
    name = "tools.list",
    version = 1,
    kind = "internal",
    scopes = ["tools:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tools_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ToolsListIn,
) -> Result<Page<ToolListRow>, ActionError> {
    let (after, limit) = page(&input.page)?;
    let mut rows = store::tool::published_tools_page(&mut *tx, ctx.tenant, after, limit + 1)
        .await
        .map_err(|error| {
            tracing::error!(%error, "tools.list: read the catalogue");
            ActionError::internal()
        })?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last()
            .map(|row| store::actions::encode_time_cursor(row.created_at, row.tool_id))
    } else {
        None
    };
    Ok(Page {
        items: rows
            .into_iter()
            .map(|row| ToolListRow {
                toolkit: row.toolkit_key,
                toolkit_version: row.toolkit_version,
                tool: row.tool_key,
                summary: row.summary,
                effect: row.effect,
                idempotency: row.idempotency,
                status: row.status,
                created_at: row.created_at,
            })
            .collect(),
        next_cursor,
    })
}

/// `tools.get`'s input.
#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
pub struct ToolsGetIn {
    /// The toolkit key.
    pub toolkit: String,
    /// The operation key.
    pub tool: String,
    /// Pin a toolkit version; absent means the newest published version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolkit_version: Option<i64>,
}

/// The full published contract of one tool.
#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct ToolGetOut {
    /// The toolkit key.
    pub toolkit: String,
    /// The pinned version.
    pub toolkit_version: i64,
    /// The operation key.
    pub tool: String,
    /// The human summary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// The closed input schema.
    pub input_schema: Value,
    /// The closed output schema.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
    /// `read | write | spend | destructive`.
    pub effect: String,
    /// `required | optional | forbidden`.
    pub idempotency: String,
    /// The timeout, in seconds.
    pub timeout_secs: i32,
    /// The redacted result fields.
    pub redact: Value,
    /// The verification declaration, when the tool has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<String>,
    /// `published`.
    pub status: String,
}

#[crate::action(
    name = "tools.get",
    version = 1,
    kind = "internal",
    scopes = ["tools:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tools_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ToolsGetIn,
) -> Result<ToolGetOut, ActionError> {
    let row = store::tool::published_tool(
        &mut *tx,
        ctx.tenant,
        &input.toolkit,
        &input.tool,
        input.toolkit_version,
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "tools.get: read the tool");
        ActionError::internal()
    })?
    .ok_or_else(|| ActionError::resource_not_found("tool"))?;
    Ok(ToolGetOut {
        toolkit: row.toolkit_key,
        toolkit_version: row.toolkit_version,
        tool: row.tool_key,
        summary: row.summary,
        input_schema: row.input_schema,
        output_schema: row.output_schema,
        effect: row.effect,
        idempotency: row.idempotency,
        timeout_secs: row.timeout_secs,
        redact: row.redact,
        verification: row
            .meta
            .get("verification")
            .and_then(Value::as_str)
            .map(str::to_owned),
        status: row.status,
    })
}

/// `toolkits.list`'s input.
#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
pub struct ToolkitsListIn {
    /// The shared page envelope.
    #[serde(default)]
    pub page: PageIn,
    /// Only this status (`draft | published | retired`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// One toolkit-version row.
#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct ToolkitListRow {
    /// The row.
    pub id: Uuid,
    /// The toolkit key.
    pub key: String,
    /// The semantic version.
    pub toolkit_version: i64,
    /// The destination alias.
    pub service: String,
    /// The human title.
    pub title: String,
    /// The owner.
    pub owner: String,
    /// `draft | published | retired`.
    pub status: String,
    /// `bearer | none | sealed-profile`.
    pub auth_adapter: String,
    /// The licence app, when gated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub licence_app: Option<String>,
    /// The content hash and health path.
    pub meta: Value,
    /// The version row's creation time.
    pub created_at: DateTime<Utc>,
}

#[crate::action(
    name = "toolkits.list",
    version = 1,
    kind = "internal",
    scopes = ["tools:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn toolkits_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ToolkitsListIn,
) -> Result<Page<ToolkitListRow>, ActionError> {
    if let Some(status) = input.status.as_deref()
        && !matches!(status, "draft" | "published" | "retired")
    {
        return Err(invalid(
            "status",
            "value",
            format!("{status:?} is not draft|published|retired"),
        ));
    }
    let (after, limit) = page(&input.page)?;
    let mut rows = store::tool::toolkits_page(
        &mut *tx,
        ctx.tenant,
        input.status.as_deref(),
        after,
        limit + 1,
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "toolkits.list: read the toolkit versions");
        ActionError::internal()
    })?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last()
            .map(|row| store::actions::encode_time_cursor(row.created_at, row.id))
    } else {
        None
    };
    Ok(Page {
        items: rows
            .into_iter()
            .map(|row| ToolkitListRow {
                id: row.id,
                key: row.key,
                toolkit_version: row.toolkit_version,
                service: row.service,
                title: row.title,
                owner: row.owner,
                status: row.status,
                auth_adapter: row.auth_adapter,
                licence_app: row.licence_app,
                meta: row.meta,
                created_at: row.created_at,
            })
            .collect(),
        next_cursor,
    })
}

/// `tool_calls.list`'s input.
#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
pub struct ToolCallsListIn {
    /// The shared page envelope.
    #[serde(default)]
    pub page: PageIn,
    /// Only this run's calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<Uuid>,
    /// Only this tool version's calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_id: Option<Uuid>,
    /// Only this terminal status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// One ledger row.
#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct ToolCallListRow {
    /// The ledger row.
    pub id: Uuid,
    /// The run, when the call belongs to one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<Uuid>,
    /// The exact tool version.
    pub tool_id: Uuid,
    /// The canonical name the caller used.
    pub tool: String,
    /// The toolkit key.
    pub toolkit: String,
    /// The pinned toolkit version.
    pub toolkit_version: i64,
    /// The input hash.
    pub input_hash: String,
    /// `ok | precondition_failed | check_failed | pending | backend_error`.
    pub status: String,
    /// The destination's HTTP status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<i32>,
    /// Latency in milliseconds.
    pub latency_ms: i32,
    /// The grant's credential alias, when the call was authorised by a binding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_alias: Option<String>,
    /// The run step's returned evidence, when the call recorded one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Value>,
    /// The failure class, when the call failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_class: Option<String>,
    /// The failure message, when the call failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// When the call was recorded.
    pub created_at: DateTime<Utc>,
}

#[crate::action(
    name = "tool_calls.list",
    version = 1,
    kind = "internal",
    scopes = ["tools:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tool_calls_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ToolCallsListIn,
) -> Result<Page<ToolCallListRow>, ActionError> {
    if let Some(status) = input.status.as_deref()
        && !matches!(
            status,
            "ok" | "precondition_failed" | "check_failed" | "pending" | "backend_error"
        )
    {
        return Err(invalid(
            "status",
            "value",
            format!("{status:?} is not a ledger status"),
        ));
    }
    let (after, limit) = page(&input.page)?;
    let mut rows = store::tool::calls_page(
        &mut *tx,
        ctx.tenant,
        &store::tool::ToolCallFilter {
            run_id: input.run_id,
            tool_id: input.tool_id,
            status: input.status.as_deref(),
        },
        after,
        limit + 1,
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "tool_calls.list: read the ledger");
        ActionError::internal()
    })?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last()
            .map(|row| store::actions::encode_time_cursor(row.created_at, row.id))
    } else {
        None
    };
    Ok(Page {
        items: rows
            .into_iter()
            .map(|row| ToolCallListRow {
                id: row.id,
                run_id: row.run_id,
                tool_id: row.tool_id,
                tool: row.tool_key,
                toolkit: row.toolkit_key,
                toolkit_version: row.toolkit_version,
                input_hash: row.input_hash,
                status: row.status,
                http_status: row.http_status,
                latency_ms: row.latency_ms,
                credential_alias: row.credential_alias,
                evidence: row.evidence,
                error_class: row.error_class,
                error_message: row.error_message,
                created_at: row.created_at,
            })
            .collect(),
        next_cursor,
    })
}
