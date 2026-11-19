#![forbid(unsafe_code)]
//! `tools.usage` — the zero-use report (SPEC_V2 §11.9, §3 P10, §14 R13; T14, #83).
//!
//! One row per **published tool version** with its call count over a bounded window, and the
//! zero-use subset named separately. The window is the caller's (`window_days`, default 30,
//! clamped to a year) rather than "since the beginning of time": a report with no window is a
//! health claim about a number nobody can reproduce, and the programme cycle is the caller's to
//! choose.
//!
//! The count comes from `tool_call` — the ledger the broker writes after every call that reached
//! a resolved tool — never from a counter the dispatcher keeps. A tool with no calls is therefore
//! a fact about the ledger, not about a process that may have restarted since.

use chrono::{Duration, Utc};
use conveyor_store as store;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::FieldError;

/// The default window: one programme cycle is a month.
const DEFAULT_WINDOW_DAYS: i64 = 30;
/// The shortest window the report accepts.
const MIN_WINDOW_DAYS: i64 = 1;
/// The longest window the report accepts.
const MAX_WINDOW_DAYS: i64 = 365;

/// `tools.usage`'s input.
#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
pub struct ToolsUsageIn {
    /// The window, in days (1..=365; default 30).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_days: Option<i64>,
}

/// One published tool version and its use in the window.
#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct ToolUsage {
    /// The toolkit key.
    pub toolkit: String,
    /// The operation key.
    pub tool: String,
    /// The toolkit's owner — who is asked about a zero-use tool.
    pub owner: String,
    /// The pinned version.
    pub toolkit_version: i64,
    /// Calls inside the window.
    pub calls: i64,
    /// The newest call inside the window, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_call_at: Option<chrono::DateTime<Utc>>,
    /// True when the tool had no call in the window — the retirement flag.
    pub zero_use: bool,
}

/// The report.
#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct ToolsUsageOut {
    /// The window the counts cover.
    pub window_days: i64,
    /// The window's lower bound.
    pub since: chrono::DateTime<Utc>,
    /// Every published tool version, in toolkit order.
    pub rows: Vec<ToolUsage>,
    /// The zero-use subset — the tools a retirement review starts from.
    pub zero_use: Vec<ToolUsage>,
}

#[crate::action(
    name = "tools.usage",
    version = 1,
    kind = "internal",
    scopes = ["tools:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tools_usage(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ToolsUsageIn,
) -> Result<ToolsUsageOut, ActionError> {
    let window_days = input.window_days.unwrap_or(DEFAULT_WINDOW_DAYS);
    if !(MIN_WINDOW_DAYS..=MAX_WINDOW_DAYS).contains(&window_days) {
        return Err(ActionError::invalid_input(vec![FieldError {
            field: "window_days".to_owned(),
            code: "range".to_owned(),
            detail: format!("{window_days} is outside {MIN_WINDOW_DAYS}..={MAX_WINDOW_DAYS}"),
        }]));
    }
    let since = Utc::now() - Duration::days(window_days);
    let rows = store::tool::usage_since(&mut *tx, ctx.tenant, since)
        .await
        .map_err(|error| {
            tracing::error!(%error, "usage: aggregate the tool ledger");
            ActionError::internal()
        })?;
    let rows: Vec<ToolUsage> = rows
        .into_iter()
        .map(|row| ToolUsage {
            toolkit: row.toolkit_key,
            tool: row.tool_key,
            owner: row.owner,
            toolkit_version: row.toolkit_version,
            calls: row.calls,
            last_call_at: row.last_call_at,
            zero_use: row.calls == 0,
        })
        .collect();
    let zero_use = rows.iter().filter(|row| row.zero_use).cloned().collect();
    Ok(ToolsUsageOut {
        window_days,
        since,
        rows,
        zero_use,
    })
}
