#![forbid(unsafe_code)]
//! `eval_runs.export` — a run's results and traces as one interop bundle (S19 Part C step 15;
//! SPEC D25).
//!
//! The bundle is the method's `results.jsonl` plus its traces, gzip-compressed and stored as an
//! S09 `document` — never hand-rolled HTML, because case text is untrusted input and the one
//! escaping surface this estate allows is a view, not a document.
//!
//! **The answer key never leaves.** A result row already carries no `expected`; this module
//! selects no case column at all, so the bundle cannot contain one even by accident — the
//! import path (`eval_cases.import`) is a copy of case *inputs*, and a used eval is exported for
//! interop, not for re-grading from a leaked key.
//!
//! The doc's `meta` carries `content_type: application/gzip` and the size; the download route
//! serves the bytes exactly as written, so an importer gets a byte-identical bundle (the hash is
//! the row's `content_hash`).

use std::io::Write as _;

use conveyor_store as store;
use flate2::Compression;
use flate2::write::GzEncoder;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::documents::{DOCUMENT_CREATED, document_error, emit_document_event};
use crate::integrations::objects::{Objects, key_for};
use crate::invoke::{ActionError, HandlerCtx};

use super::{eval_error, runs::RunIdIn};

/// The bundle's cap. Larger than an upload: a run is bounded by ≤100 cases × reps, and a trace
/// is capped at 256 KiB by the runner, so 64 MiB is already generous.
const MAX_BUNDLE_BYTES: usize = 64 * 1024 * 1024;

/// One export's result.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ExportOut {
    /// The document that now holds the bundle.
    pub document_id: Uuid,
    /// The stored object's content hash.
    pub content_hash: String,
    /// The uncompressed JSONL's size.
    pub jsonl_bytes: usize,
    /// The compressed bundle's size.
    pub gzip_bytes: usize,
    /// How many result lines and trace lines it carries.
    pub results: usize,
    /// How many traces it carries.
    pub traces: usize,
}

/// One run's results and traces as a gzip JSONL document.
#[crate::action(
    name = "eval_runs.export",
    version = 1,
    kind = "internal",
    scopes = ["evals:read"],
    idempotent = false,
    timeout_ms = 60000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_runs_export(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RunIdIn,
) -> Result<ExportOut, ActionError> {
    let run = store::eval::get_run_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;

    let mut jsonl = String::new();
    jsonl.push_str(
        &serde_json::to_string(&json!({
            "kind": "run",
            "run_id": run.id,
            "eval_id": run.eval_id,
            "version_id": run.version_id,
            "variant": run.variant.as_str(),
            "status": run.status.as_str(),
            "reps": run.reps,
            "case_count": run.case_count,
            "mode": run.mode.as_str(),
        }))
        .unwrap_or_default(),
    );
    jsonl.push('\n');

    let results = all_results(&mut *tx, ctx.tenant, run.id).await?;
    let mut traces = 0_usize;
    for row in &results {
        // The result line carries the grade object and the usage, never the case.
        jsonl.push_str(
            &serde_json::to_string(&json!({
                "kind": "result",
                "case_id": row.case_id,
                "rep": row.rep,
                "status": row.status,
                "stop_reason": row.stop_reason,
                "grade": row.grade,
                "model": row.model,
                "usage": row.usage,
                "judge_model": row.judge_model,
                "judge_usage": row.judge_usage,
                "latency_ms": row.latency_ms,
                "tool_calls": row.tool_calls,
            }))
            .unwrap_or_default(),
        );
        jsonl.push('\n');
        let trace = store::eval::get_trace_tx(&mut *tx, ctx.tenant, run.id, row.case_id, row.rep)
            .await
            .map_err(|error| eval_error(error, ctx.action))?;
        if let Some(trace) = trace {
            traces += 1;
            jsonl.push_str(
                &serde_json::to_string(&json!({
                    "kind": "trace",
                    "case_id": trace.case_id,
                    "rep": trace.rep,
                    "turns": trace.turns,
                }))
                .unwrap_or_default(),
            );
            jsonl.push('\n');
        }
    }

    let jsonl_bytes = jsonl.len();
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(jsonl.as_bytes()).map_err(|error| {
        tracing::error!(%error, "compress an eval run bundle");
        ActionError::internal()
    })?;
    let bytes = encoder.finish().map_err(|error| {
        tracing::error!(%error, "compress an eval run bundle");
        ActionError::internal()
    })?;
    if bytes.len() > MAX_BUNDLE_BYTES {
        return Err(ActionError::new(
            413,
            "payload_too_large",
            format!(
                "the bundle is {} bytes; the cap is {MAX_BUNDLE_BYTES}",
                bytes.len()
            ),
        ));
    }

    let content_hash = format!("{:x}", Sha256::digest(&bytes));
    let objects = Objects::from_env().ok_or_else(|| {
        crate::documents::object_error(
            crate::integrations::objects::ObjectError::Unavailable,
            ctx.action,
        )
    })?;
    let id = Uuid::now_v7();
    let key = key_for(ctx.tenant.as_uuid(), id, &content_hash);
    objects
        .put(&key, bytes.clone().into())
        .await
        .map_err(|error| crate::documents::object_error(error, ctx.action))?;

    let created = store::document::create_document_tx(
        &mut *tx,
        ctx.tenant,
        &store::document::NewDocument {
            id,
            project_id: None,
            kind: "export".to_owned(),
            title: format!("eval run {} ({})", run.id, run.variant.as_str()),
            status: "active".to_owned(),
            object_key: Some(key.clone()),
            content_hash: Some(content_hash.clone()),
            card_id: None,
            body: None,
            external_ref: None,
            meta: json!({
                "content_type": "application/gzip",
                "size_bytes": bytes.len() as i64,
                "eval_id": run.eval_id,
                "eval_run_id": run.id,
                "variant": run.variant.as_str(),
            }),
            publishable: false,
        },
    )
    .await
    .map_err(|error| {
        let objects = objects.clone();
        let key = key.clone();
        tokio::spawn(async move {
            if let Err(error) = objects.delete(&key).await {
                tracing::error!(%error, "remove an eval export whose document row failed");
            }
        });
        document_error(error, ctx.action)
    })?;

    emit_document_event(
        &mut *tx,
        &ctx,
        DOCUMENT_CREATED,
        created.id,
        json!({"id": created.id, "version": created.version.get()}),
    )
    .await?;

    Ok(ExportOut {
        document_id: created.id,
        content_hash,
        jsonl_bytes,
        gzip_bytes: bytes.len(),
        results: results.len(),
        traces,
    })
}

/// Every result of one run, read to exhaustion (the same page size the report uses).
async fn all_results(
    tx: &mut PgConnection,
    tenant: conveyor_domain::TenantId,
    run_id: Uuid,
) -> Result<Vec<store::eval::ResultRow>, ActionError> {
    const PAGE: i64 = 500;
    let mut out = Vec::new();
    let mut cursor: Option<(Uuid, i32)> = None;
    loop {
        let page = store::eval::list_results_tx(
            tx,
            tenant,
            run_id,
            store::eval::ResultFilter::default(),
            cursor,
            PAGE,
        )
        .await
        .map_err(|error| eval_error(error, "eval_runs.export"))?;
        let short = (page.len() as i64) < PAGE;
        if let Some(last) = page.last() {
            cursor = Some((last.case_id, last.rep));
        }
        out.extend(page);
        if short {
            return Ok(out);
        }
    }
}
