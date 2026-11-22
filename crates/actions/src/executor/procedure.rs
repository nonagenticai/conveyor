#![forbid(unsafe_code)]
//! The `procedure` run executor: one typed refusal.
//!
//! A procedure binding executes through `process_runs.start`, which owns the one process start
//! path — readiness before the run row, the materialised `run_step` trace, the first
//! `process.step` job. A plain `runs.create` run (which is what `run.execute` drives) has no
//! process to materialise, so this handler refuses rather than growing a second start path
//! (S21 §14, ADR 0010). The handler exists so dispatch stays generic: the runtime never asks
//! which kind it is serving.

use std::future::Future;
use std::pin::Pin;

use conveyor_domain::executor::ExecutorKind;
use conveyor_store as store;

use super::{ExecutorFailure, RunExecutor, RunRequest, finish_failed};

/// The built-in `procedure` executor.
pub struct ProcedureExecutor;

impl RunExecutor for ProcedureExecutor {
    fn kind(&self) -> ExecutorKind {
        ExecutorKind::procedure()
    }

    fn execute<'a>(
        &'a self,
        request: RunRequest<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ExecutorFailure>> + Send + 'a>> {
        Box::pin(async move {
            let RunRequest {
                pool,
                tenant,
                app_key,
                run,
                ..
            } = request;
            let mut tx = store::begin_scoped(pool, tenant)
                .await
                .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
            finish_failed(
                &mut tx,
                tenant,
                app_key,
                run.id,
                "executor_kind_unsupported",
                "a procedure binding executes through process_runs.start; a plain run has no \
                 published process to materialise",
            )
            .await?;
            tx.commit()
                .await
                .map_err(|error| ExecutorFailure::retryable(format!("database: {error}")))?;
            Ok(())
        })
    }
}
