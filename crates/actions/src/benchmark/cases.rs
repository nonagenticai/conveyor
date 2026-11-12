#![forbid(unsafe_code)]
//! `benchmark_cases.list` — one page of a run's cases.
//!
//! The view carries the producer's bounded patch and the grader's verdict, and **not** the
//! answers: `gold_patch`, `test_patch`, `fail_to_pass`, `pass_to_pass` and `eval_script` live on
//! `benchmark_instance` and are readable only by the grader (CONTRACT §19).

use conveyor_store as store;
use conveyor_store::actions::clamp_limit;
use sqlx::PgConnection;

use crate::benchmark::{
    CaseView, CasesListIn, VERDICTS, benchmark_error, case_view, decode_cursor, encode_cursor,
    invalid,
};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;
use crate::views::Page;

/// One page of cases, newest first.
#[crate::action(
    name = "benchmark_cases.list",
    version = 1,
    kind = "internal",
    scopes = ["benchmarks:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn benchmark_cases_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CasesListIn,
) -> Result<HandlerOutput<Page<CaseView>>, ActionError> {
    const ACTION: &str = "benchmark_cases.list";
    if let Some(verdict) = input.verdict.as_deref()
        && !VERDICTS.contains(&verdict)
    {
        return Err(invalid(
            "verdict",
            "invalid_value",
            "verdict must be pending, resolved, unresolved, apply_failed, eval_timeout or \
             infra_error",
        ));
    }
    let limit = clamp_limit(input.page.limit);
    let cursor = decode_cursor(input.page.cursor.as_deref())?;
    let mut rows = store::benchmark::list_cases_tx(
        &mut *tx,
        ctx.tenant,
        input.run_id,
        input.verdict.as_deref(),
        cursor,
        limit + 1,
    )
    .await
    .map_err(|error| benchmark_error(error, ACTION))?;
    let has_more = rows.len() as i64 > limit;
    if has_more {
        rows.truncate(limit as usize);
    }
    let next_cursor = if has_more {
        rows.last().map(|row| encode_cursor(row.created_at, row.id))
    } else {
        None
    };
    Ok(HandlerOutput::ok(Page {
        items: rows.iter().map(case_view).collect(),
        next_cursor,
    }))
}
