#![forbid(unsafe_code)]
//! `inference_usage.report` — the ledger aggregated over a window and one dimension.
//!
//! A cache report over a quarter is the point of the ledger (data-arch §2.17); the `prompt_sha`
//! dimension is what answers *which prompt* is cache-hostile, which model and caller alone cannot.

use conveyor_domain::inference::UsageGroup;
use conveyor_store as store;
use sqlx::PgConnection;

use super::{InferenceUsageReportIn, UsageReportOut, invalid};
use crate::invoke::{ActionError, HandlerCtx};

#[crate::action(
    name = "inference_usage.report",
    version = 1,
    kind = "internal",
    scopes = ["inference:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn inference_usage_report(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: InferenceUsageReportIn,
) -> Result<UsageReportOut, ActionError> {
    if input.until <= input.since {
        return Err(invalid(
            "until",
            "range",
            "until must be strictly after since",
        ));
    }
    let group = UsageGroup::parse(&input.group_by).ok_or_else(|| {
        invalid(
            "group_by",
            "unknown",
            "one of day, model, caller_kind, cache_status, prompt_sha",
        )
    })?;
    let rows =
        store::inference::usage_rows_tx(&mut *tx, ctx.tenant, group, input.since, input.until)
            .await
            .map_err(|error| {
                tracing::error!(%error, "aggregate the inference ledger");
                ActionError::internal()
            })?;
    Ok(UsageReportOut {
        group_by: group.as_str().to_owned(),
        since: input.since,
        until: input.until,
        rows: rows.into_iter().map(Into::into).collect(),
    })
}
