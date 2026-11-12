#![forbid(unsafe_code)]
//! The inference ledger: the engine's write, the keyset list and the usage aggregate (S16).
//!
//! `inference_call` is written by the engine, never by a handler — the call site already owns the
//! response and the transaction, and a handler-side writer would be a second path for the same
//! fact (CONTRACT §18). The join keys carry no FK on purpose: an FK's RI check runs as the table
//! owner and bypasses RLS (data-arch §2.17), so the engine resolves them in the caller's tenant and
//! the `inference_call_join_key` CHECK keeps the row answerable.
//!
//! Every function takes the caller's already-scoped connection plus the tenant it binds.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use conveyor_domain::inference::{Usage, UsageGroup, UsageRow, hit_ratio};
use sqlx::PgConnection;
use uuid::Uuid;

/// Why an inference-ledger operation failed.
#[derive(Debug, thiserror::Error)]
pub enum InferenceStoreError {
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// One row to insert; the engine fills it from the request, the response and the caller's context.
#[derive(Clone, Debug)]
pub struct NewInferenceCall<'a> {
    /// The `action_invocation` this call was made inside.
    pub action_invocation_id: Option<Uuid>,
    /// The request's trace id.
    pub trace_id: Option<&'a str>,
    /// The router's `X-Correlation-Id`, echoed by the provider.
    pub correlation_id: Option<&'a str>,
    /// `copilot | process_step | workflow_step | agent_run | other`.
    pub caller_kind: &'a str,
    /// The run this call executed inside, when any.
    pub run_id: Option<Uuid>,
    /// The `run_step` this call executed for, when any.
    pub run_step_id: Option<Uuid>,
    /// The copilot turn, when any.
    pub conversation_turn_id: Option<Uuid>,
    /// The eval run, when the caller is an eval (`caller_kind = 'eval'`).
    pub eval_run_id: Option<Uuid>,
    /// The prompt family (`prompt_audit@1`, a process step's reference, `copilot`).
    pub prompt_ref: Option<&'a str>,
    /// The prompt's version within its family.
    pub prompt_version: Option<&'a str>,
    /// SHA-256 of the rendered prompt; never the body.
    pub prompt_sha256: Option<&'a str>,
    /// The requested model alias.
    pub model: &'a str,
    /// The provider's answer, when it names one.
    pub served_model: Option<&'a str>,
    /// The token counts; `None` stores the `unknown` cache class and zeros.
    pub usage: Option<Usage>,
    /// `hit | write | miss | unknown` (derived from `usage` by the domain).
    pub cache_status: &'a str,
    /// The call's wall time.
    pub latency_ms: i32,
    /// `ok | refused | error | timeout`.
    pub outcome: &'a str,
}

/// One ledger row as the read actions return it.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct InferenceCallRow {
    /// The row id.
    pub id: Uuid,
    /// The invocation it was made inside.
    pub action_invocation_id: Option<Uuid>,
    /// The request trace.
    pub trace_id: Option<String>,
    /// The provider correlation id.
    pub correlation_id: Option<String>,
    /// Who made the call.
    pub caller_kind: String,
    /// The owning run.
    pub run_id: Option<Uuid>,
    /// The owning step.
    pub run_step_id: Option<Uuid>,
    /// The owning copilot turn.
    pub conversation_turn_id: Option<Uuid>,
    /// The owning eval run (S19 §9; `caller_kind = 'eval'`).
    pub eval_run_id: Option<Uuid>,
    /// The prompt family.
    pub prompt_ref: Option<String>,
    /// The prompt version.
    pub prompt_version: Option<String>,
    /// The rendered prompt's hash.
    pub prompt_sha256: Option<String>,
    /// The requested model.
    pub model: String,
    /// The provider's model.
    pub served_model: Option<String>,
    /// Input tokens.
    pub prompt_tokens: i32,
    /// Output tokens.
    pub completion_tokens: i32,
    /// Total tokens.
    pub total_tokens: i32,
    /// Cache-write tokens.
    pub cache_creation_input_tokens: i32,
    /// Cache-read tokens.
    pub cache_read_input_tokens: i32,
    /// The four-way class.
    pub cache_status: String,
    /// Wall time.
    pub latency_ms: i32,
    /// The outcome.
    pub outcome: String,
    /// When the call was recorded.
    pub created_at: DateTime<Utc>,
}

const CALL_COLUMNS: &str =
    "id, action_invocation_id, trace_id, correlation_id, caller_kind, run_id,
    run_step_id, conversation_turn_id, eval_run_id, prompt_ref, prompt_version, prompt_sha256,
    model, served_model, prompt_tokens, completion_tokens, total_tokens, cache_creation_input_tokens,
    cache_read_input_tokens, cache_status, latency_ms, outcome, created_at";

/// Insert one call in the caller's scoped transaction.
pub async fn insert_call_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    call: &NewInferenceCall<'_>,
) -> Result<Uuid, InferenceStoreError> {
    let usage = call.usage.unwrap_or_default();
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO inference_call (
             tenant_id, action_invocation_id, trace_id, correlation_id, caller_kind,
             run_id, run_step_id, conversation_turn_id, eval_run_id,
             prompt_ref, prompt_version, prompt_sha256,
             model, served_model, prompt_tokens, completion_tokens, total_tokens,
             cache_creation_input_tokens, cache_read_input_tokens, cache_status,
             latency_ms, outcome
         ) VALUES (
             $1, $2, $3, $4, $5,
             $6, $7, $8, $9,
             $10, $11, $12,
             $13, $14, $15, $16, $17,
             $18, $19, $20,
             $21, $22
         ) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(call.action_invocation_id)
    .bind(call.trace_id)
    .bind(call.correlation_id)
    .bind(call.caller_kind)
    .bind(call.run_id)
    .bind(call.run_step_id)
    .bind(call.conversation_turn_id)
    .bind(call.eval_run_id)
    .bind(call.prompt_ref)
    .bind(call.prompt_version)
    .bind(call.prompt_sha256)
    .bind(call.model)
    .bind(call.served_model)
    .bind(usage.prompt_tokens)
    .bind(usage.completion_tokens)
    .bind(usage.total_tokens)
    .bind(usage.cache_creation_input_tokens)
    .bind(usage.cache_read_input_tokens)
    .bind(call.cache_status)
    .bind(call.latency_ms)
    .bind(call.outcome)
    .fetch_one(&mut *conn)
    .await?;
    Ok(id)
}

/// One page of the ledger, newest first.
#[derive(Clone, Debug, Default)]
pub struct ListCalls {
    /// Restrict to a caller kind.
    pub caller_kind: Option<String>,
    /// Restrict to one run.
    pub run_id: Option<Uuid>,
    /// Restrict to one eval run.
    pub eval_run_id: Option<Uuid>,
    /// Restrict to the requested model (exact).
    pub model: Option<String>,
    /// Restrict to one cache class.
    pub cache_status: Option<String>,
    /// Inclusive lower bound on `created_at`.
    pub since: Option<DateTime<Utc>>,
    /// The keyset cursor: the last row's `(created_at, id)`.
    pub before: Option<(DateTime<Utc>, Uuid)>,
    /// The page size; the action clamps it.
    pub limit: i64,
}

/// List the tenant's calls, newest first, with a keyset cursor.
pub async fn list_calls_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    filter: &ListCalls,
) -> Result<Vec<InferenceCallRow>, InferenceStoreError> {
    let sql = format!(
        "SELECT {CALL_COLUMNS}
           FROM inference_call
          WHERE tenant_id = $1
            AND ($2::text IS NULL OR caller_kind = $2)
            AND ($3::uuid IS NULL OR run_id = $3)
            AND ($4::text IS NULL OR model = $4)
            AND ($5::text IS NULL OR cache_status = $5)
            AND ($6::timestamptz IS NULL OR created_at >= $6)
            AND ($7::timestamptz IS NULL OR (created_at, id) < ($7, $8))
            AND ($10::uuid IS NULL OR eval_run_id = $10)
          ORDER BY created_at DESC, id DESC
          LIMIT $9"
    );
    let rows: Vec<InferenceCallRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(filter.caller_kind.as_deref())
        .bind(filter.run_id)
        .bind(filter.model.as_deref())
        .bind(filter.cache_status.as_deref())
        .bind(filter.since)
        .bind(filter.before.map(|(at, _)| at))
        .bind(filter.before.map(|(_, id)| id))
        .bind(filter.limit)
        .bind(filter.eval_run_id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows)
}

#[derive(sqlx::FromRow)]
struct UsageAggregateRow {
    bucket: String,
    calls: i64,
    prompt_tokens: i64,
    completion_tokens: i64,
    hits: i64,
    writes: i64,
    misses: i64,
    unknown: i64,
}

/// The aggregate behind `inference_usage.report`.
///
/// The bucket expression is chosen from the closed [`UsageGroup`] enum — never from input text —
/// so the dynamic SQL has no writable surface. `model` groups on the served model with the
/// requested one as the fallback (owner decision, 2026-11-09).
pub async fn usage_rows_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    group: UsageGroup,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
) -> Result<Vec<UsageRow>, InferenceStoreError> {
    let bucket = match group {
        UsageGroup::Day => {
            "to_char(date_trunc('day', created_at AT TIME ZONE 'UTC'), 'YYYY-MM-DD')"
        }
        UsageGroup::Model => "coalesce(served_model, model)",
        UsageGroup::CallerKind => "caller_kind",
        UsageGroup::CacheStatus => "cache_status",
        UsageGroup::PromptSha => "coalesce(prompt_sha256, '')",
    };
    let sql = format!(
        "SELECT {bucket} AS bucket,
                count(*)::bigint AS calls,
                coalesce(sum(prompt_tokens), 0)::bigint AS prompt_tokens,
                coalesce(sum(completion_tokens), 0)::bigint AS completion_tokens,
                count(*) FILTER (WHERE cache_status = 'hit')::bigint AS hits,
                count(*) FILTER (WHERE cache_status = 'write')::bigint AS writes,
                count(*) FILTER (WHERE cache_status = 'miss')::bigint AS misses,
                count(*) FILTER (WHERE cache_status = 'unknown')::bigint AS unknown
           FROM inference_call
          WHERE tenant_id = $1 AND created_at >= $2 AND created_at < $3
          GROUP BY 1
          ORDER BY 1"
    );
    let rows: Vec<UsageAggregateRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(since)
        .bind(until)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| UsageRow {
            bucket: row.bucket,
            calls: row.calls,
            prompt_tokens: row.prompt_tokens,
            completion_tokens: row.completion_tokens,
            hits: row.hits,
            writes: row.writes,
            misses: row.misses,
            unknown: row.unknown,
            hit_ratio: hit_ratio(row.hits, row.calls),
        })
        .collect())
}

/// What one capture attempt did. The engine logs it; a missing row is visible, never an error on
/// a call that already succeeded.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CaptureOutcome {
    /// True when the ledger row (and its `inference_call.recorded` event) committed.
    pub recorded: bool,
    /// The event-created audit, when this call crossed the tenant's threshold.
    pub audit_started: Option<Uuid>,
    /// Why capture or the cadence trigger failed; the caller's transaction is still usable.
    pub error: Option<String>,
}

/// Write the ledger row and run the after-N-executions trigger inside the CALLER's transaction.
///
/// **This function never returns an error.** Every statement runs under a savepoint: a capture
/// failure rolls back to it, logs, and leaves the action's own bookkeeping (the invocation row,
/// its audit event and the idempotency response) untouched — a model call that succeeded must not
/// become an error because bookkeeping did (S16 trap 2). The trade is a missing ledger row on a
/// genuine failure, logged.
///
/// The cadence trigger has its own savepoint: a failure there keeps the ledger row but removes
/// the queued audit, so the next call can try again rather than leaving an audit with no job.
pub async fn capture_and_maybe_audit_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    call: &NewInferenceCall<'_>,
) -> CaptureOutcome {
    let mut outcome = CaptureOutcome::default();

    if let Err(error) = sqlx::query("SAVEPOINT s16_capture")
        .execute(&mut *conn)
        .await
    {
        outcome.error = Some(format!("savepoint: {error}"));
        tracing::warn!(%error, "inference capture: savepoint failed");
        return outcome;
    }
    let recorded = async {
        let id = insert_call_tx(&mut *conn, tenant, call)
            .await
            .map_err(|error| error.to_string())?;
        emit_recorded_event(&mut *conn, tenant, call, id)
            .await
            .map_err(|error| error.to_string())?;
        sqlx::query("RELEASE SAVEPOINT s16_capture")
            .execute(&mut *conn)
            .await
            .map_err(|error| error.to_string())?;
        Ok::<Uuid, String>(id)
    }
    .await;
    match recorded {
        Ok(_) => outcome.recorded = true,
        Err(error) => {
            let _ = sqlx::query("ROLLBACK TO SAVEPOINT s16_capture")
                .execute(&mut *conn)
                .await;
            tracing::warn!(%error, "inference capture failed; the call is unaffected");
            outcome.error = Some(error);
            return outcome;
        }
    }

    let _ = sqlx::query("SAVEPOINT s16_trigger")
        .execute(&mut *conn)
        .await;
    match trigger_cadence(conn, tenant).await {
        Ok(audit) => {
            let _ = sqlx::query("RELEASE SAVEPOINT s16_trigger")
                .execute(&mut *conn)
                .await;
            outcome.audit_started = audit;
        }
        Err(error) => {
            let _ = sqlx::query("ROLLBACK TO SAVEPOINT s16_trigger")
                .execute(&mut *conn)
                .await;
            tracing::warn!(%error, "prompt-audit cadence trigger failed; the ledger row stands");
            outcome.error = Some(error);
        }
    }
    outcome
}

/// The after-N-executions trigger: the ledger is the counter, and the Nth captured call starts one
/// audit. The advisory lock serialises callers that cross the threshold together; the partial
/// unique index is the final guard, so exactly one event-created audit exists at a time.
async fn trigger_cadence(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Option<Uuid>, String> {
    let settings = crate::prompt_audit::audit_settings_tx(conn, tenant)
        .await
        .map_err(|error| error.to_string())?;
    if !settings.enabled {
        return Ok(None);
    }
    let calls = crate::prompt_audit::calls_since_last_audit_tx(conn, tenant)
        .await
        .map_err(|error| error.to_string())?;
    if settings.due(calls).is_none() {
        return Ok(None);
    }
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("prompt_audit:{}", tenant.as_uuid()))
        .execute(&mut *conn)
        .await
        .map_err(|error| error.to_string())?;
    // Re-read under the lock: the caller that held it before us may have created the audit.
    let calls = crate::prompt_audit::calls_since_last_audit_tx(conn, tenant)
        .await
        .map_err(|error| error.to_string())?;
    let Some(model) = settings.due(calls).map(str::to_owned) else {
        return Ok(None);
    };
    let audit = crate::prompt_audit::NewPromptAudit {
        scope: settings.automatic_scope().to_value(),
        target_model: model,
        prompt_ref: crate::prompt_audit::PROMPT_REF_V1.to_owned(),
        requested_by: None,
    };
    let id = crate::prompt_audit::insert_auto_audit_tx(conn, tenant, &audit)
        .await
        .map_err(|error| error.to_string())?;
    let Some(id) = id else {
        return Ok(None);
    };
    crate::jobs::enqueue_tx(
        conn,
        tenant,
        "prompt_audit.run",
        &format!("prompt_audit:{id}"),
        serde_json::json!({"audit_id": id}),
        None,
        3,
    )
    .await
    .map_err(|error| error.to_string())?;
    Ok(Some(id))
}

/// Append `inference_call.recorded` for one captured row.
async fn emit_recorded_event(
    conn: &mut PgConnection,
    tenant: TenantId,
    call: &NewInferenceCall<'_>,
    id: Uuid,
) -> Result<(), crate::outbox::OutboxError> {
    let event = crate::NewEvent {
        topic: format!("inference_call:{id}"),
        event_type: "inference_call.recorded".to_owned(),
        aggregate_type: "inference_call".to_owned(),
        aggregate_id: id,
        payload: serde_json::json!({
            "id": id,
            "caller_kind": call.caller_kind,
            "model": call.model,
            "cache_status": call.cache_status,
        }),
        correlation_id: None,
        causation_id: None,
    };
    crate::outbox::append(conn, tenant, &event)
        .await
        .map(|_| ())
}
