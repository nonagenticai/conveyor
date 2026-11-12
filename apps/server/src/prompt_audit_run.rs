//! `prompt_audit.run` — the audit's one bounded model call (S16).
//!
//! The job is enqueued by `prompt_audits.start` (manual or kata) or by the capture seam's
//! after-N-executions event. This arm reads the audit's scope, builds the deterministic inventory
//! with [`conveyor_actions::prompt_audit::scan`], calls `completions.create` exactly once, validates
//! and persists the model's findings, and advances `queued → running → succeeded | failed`.
//!
//! **The audit is not a second runtime:** no tool loop, no capability agent, no fourth executor
//! (CONTRACT §17/§18). It is one `completions.create` under the same service principal the
//! process executor uses.
//!
//! A transient upstream failure returns a retryable job failure and leaves the audit `running`;
//! the retry re-enters at the same place. A malformed model answer is recorded as the audit's
//! `error` and dead-letters — a bad answer is not retried into a different one.

use conveyor_actions::invoke::{Authority, InvokeRequest, UsageContext, invoke};
use conveyor_actions::prompt_audit::{COPILOT_SYSTEM_PROMPT, scan};
use conveyor_actions::registry::registry;
use conveyor_auth::Role;
use conveyor_domain::TenantId;
use conveyor_domain::inference::CallerKind;
use conveyor_domain::prompt_audit::{
    AuditScope, AuditStatus, Confidence, FindingAction, FindingDraft, Pattern, Surface,
};
use conveyor_store as store;
use conveyor_store::prompt_audit::{
    PROMPT_REF_V1, get_audit_tx, insert_finding_tx, transition_audit_tx,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::worker::JobFailure;

/// The most surface items one audit sends to the model.
const MAX_ITEMS: usize = 200;
/// The most characters of inventory text one audit sends.
const MAX_INVENTORY_CHARS: usize = 60_000;
/// The most findings one answer may carry; a longer list is a malformed answer, not a discovery.
const MAX_FINDINGS: usize = 200;

/// Run one `prompt_audit.run` job.
pub async fn run(
    pool: &PgPool,
    tenant: TenantId,
    app_key: &str,
    payload: &Value,
) -> Result<(), JobFailure> {
    let audit_id = payload
        .get("audit_id")
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .ok_or_else(|| JobFailure::permanent("prompt_audit.run needs an audit_id"))?;
    let audit = {
        let mut tx = store::begin_scoped(pool, tenant)
            .await
            .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
        let Some(audit) = get_audit_tx(&mut tx, tenant, audit_id)
            .await
            .map_err(|error| JobFailure::retryable(format!("database: {error}")))?
        else {
            // The tenant was restored or deleted: there is no audit to run.
            return Ok(());
        };
        let status = AuditStatus::parse(&audit.status).ok_or_else(|| {
            JobFailure::permanent(format!("unknown audit status {}", audit.status))
        })?;
        if status.is_terminal() {
            return Ok(());
        }
        let running =
            transition_audit_tx(&mut tx, tenant, audit_id, AuditStatus::Running, None, None)
                .await
                .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
        tx.commit()
            .await
            .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
        running
    };

    let scope = AuditScope::from_value(&audit.scope)
        .map_err(|error| JobFailure::permanent(format!("audit scope: {error}")))?;
    let items = match inventory(pool, tenant, &scope).await {
        Ok(items) => items,
        Err(error) => {
            fail_audit(pool, tenant, audit_id, &error).await;
            return Err(JobFailure::permanent(error));
        }
    };
    if items.is_empty() {
        finish_succeeded(pool, tenant, audit_id, &[]).await?;
        return Ok(());
    }

    let (inventory_text, truncated) = render_inventory(&items);
    // SPEC D27: the audit's own instruction is a catalogue row (`prompt_audit@1`), compiled like
    // every other prompt. A missing or retired row is a permanent configuration failure — retrying
    // would never fix it — while a database error is retryable.
    let instruction =
        match conveyor_actions::prompt::compile_for_audit(pool, tenant, PROMPT_REF_V1).await {
            Ok(compiled) => compiled.text,
            Err(error) if matches!(error, conveyor_actions::prompt::PromptError::Store(_)) => {
                return Err(JobFailure::retryable(format!("prompt catalogue: {error}")));
            }
            Err(error) => {
                fail_audit(pool, tenant, audit_id, &error.to_string()).await;
                return Err(JobFailure::permanent(format!("prompt_audit.run: {error}")));
            }
        };
    let target_model = audit.target_model.clone();
    let user = json!({
        "target_model": target_model,
        "inventory_truncated": truncated,
        "items": inventory_text,
    });
    let usage = Some(UsageContext {
        caller_kind: CallerKind::Other,
        run_id: None,
        run_step_id: None,
        conversation_turn_id: None,
        eval_run_id: None,
        prompt_ref: Some(PROMPT_REF_V1.to_owned()),
        prompt_version: Some("1".to_owned()),
        correlation_id: Some(format!("prompt_audit:{audit_id}")),
    });
    let request = InvokeRequest {
        action: "completions.create".to_owned(),
        input: json!({
            "model": audit.target_model,
            "messages": [                {"role": "system", "content": instruction},
                {"role": "user", "content": user.to_string()},
            ],
            "max_tokens": 8_000,
            "response_format": {"type": "json_object"},
        }),
        precondition: None,
        idempotency_key: Some(format!("prompt_audit.run:{audit_id}")),
        actor_id: Uuid::nil(),
        actor_kind: "service".to_owned(),
        role: Role::KataExecutor,
        policy: Default::default(),
        authority: Authority::Internal,
        request_id: Some(format!("prompt_audit.run:{audit_id}")),
        trace_id: Some(format!("prompt_audit:{audit_id}")),
        depth: 1,
        app_key: app_key.to_owned(),
        usage,
    };
    let outcome = match invoke(pool, registry(), tenant, request).await {
        Ok(outcome) => outcome,
        Err(error) if error.status >= 500 => {
            return Err(JobFailure::retryable(format!(
                "{}: {}",
                error.code, error.detail
            )));
        }
        Err(error) => {
            let message = format!("{}: {}", error.code, error.detail);
            fail_audit(pool, tenant, audit_id, &message).await;
            return Err(JobFailure::permanent(message));
        }
    };

    let output = outcome.body.get("output").cloned().unwrap_or(Value::Null);
    let findings = match parse_findings(&output) {
        Ok(findings) => findings,
        Err(error) => {
            fail_audit(pool, tenant, audit_id, &error).await;
            return Err(JobFailure::permanent(error));
        }
    };
    finish_succeeded(pool, tenant, audit_id, &findings).await?;
    Ok(())
}

/// The inventory named by the audit's scope. The `actions` surface is empty by construction
/// (S16 CCR-7): `ActionSpec` carries no authored description.
async fn inventory(
    pool: &PgPool,
    tenant: TenantId,
    scope: &AuditScope,
) -> Result<Vec<scan::SurfaceItem>, String> {
    let mut items = Vec::new();
    if scope.surfaces.contains(&Surface::Processes) {
        let mut tx = store::begin_scoped(pool, tenant)
            .await
            .map_err(|error| format!("database: {error}"))?;
        let processes = if scope.process_ids.is_empty() {
            store::process::list_processes_tx(&mut tx, tenant, None, None, None, 500)
                .await
                .map_err(|error| format!("read processes: {error}"))?
        } else {
            let mut rows = Vec::new();
            for id in &scope.process_ids {
                if let Some(row) = store::process::get_process_tx(&mut tx, tenant, *id)
                    .await
                    .map_err(|error| format!("read a process: {error}"))?
                {
                    rows.push(row);
                }
            }
            rows
        };
        let ids: Vec<Uuid> = processes.iter().map(|row| row.id).collect();
        let steps = store::process::list_steps_for_processes_tx(&mut tx, tenant, &ids)
            .await
            .map_err(|error| format!("read process steps: {error}"))?;
        for process in &processes {
            let own: Vec<_> = steps
                .iter()
                .filter(|step| step.process_id == process.id)
                .cloned()
                .collect();
            items.extend(scan::process_items(process, &own));
        }
        tx.commit()
            .await
            .map_err(|error| format!("database: {error}"))?;
    }
    if scope.surfaces.contains(&Surface::Copilot) {
        items.extend(scan::copilot_items(COPILOT_SYSTEM_PROMPT));
    }
    Ok(items)
}

/// Render the inventory as a JSON array of `{location, text}`, bounded in count and characters.
fn render_inventory(items: &[scan::SurfaceItem]) -> (Value, bool) {
    let mut kept = Vec::new();
    let mut chars = 0usize;
    let mut truncated = items.len() > MAX_ITEMS;
    for item in items.iter().take(MAX_ITEMS) {
        if chars + item.text.len() > MAX_INVENTORY_CHARS {
            truncated = true;
            break;
        }
        chars += item.text.len();
        kept.push(json!({"location": item.location, "text": item.text}));
    }
    (Value::Array(kept), truncated)
}

/// Extract and validate the model's findings. Strict on purpose: an unknown code, a missing
/// replacement or a non-array answer fails the audit with a recorded error rather than dropping
/// findings silently.
fn parse_findings(output: &Value) -> Result<Vec<FindingDraft>, String> {
    let content = output
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .ok_or_else(|| "the model answer carries no choices[0].message.content".to_owned())?;
    let document: Value = serde_json::from_str(content)
        .map_err(|error| format!("the model answer is not JSON: {error}"))?;
    let rows = document
        .get("findings")
        .and_then(Value::as_array)
        .ok_or_else(|| "the model answer has no findings array".to_owned())?;
    if rows.len() > MAX_FINDINGS {
        return Err(format!("the model returned {} findings", rows.len()));
    }
    let mut findings = Vec::with_capacity(rows.len());
    for row in rows {
        let location = row
            .get("location")
            .and_then(Value::as_str)
            .filter(|value| {
                value.starts_with("process:")
                    || value.starts_with("process_step:")
                    || value.starts_with("copilot:")
            })
            .ok_or_else(|| format!("bad finding location: {row}"))?;
        let evidence = row
            .get("evidence")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| format!("bad finding evidence: {row}"))?;
        let why = row
            .get("why_obsolete")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| format!("bad finding why_obsolete: {row}"))?;
        let pattern = Pattern::parse(
            row.get("pattern")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("bad finding pattern: {row}"))?,
        )
        .ok_or_else(|| format!("bad finding pattern: {row}"))?;
        let confidence = Confidence::parse(
            row.get("confidence")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("bad finding confidence: {row}"))?,
        )
        .ok_or_else(|| format!("bad finding confidence: {row}"))?;
        let action = FindingAction::parse(
            row.get("action")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("bad finding action: {row}"))?,
        )
        .ok_or_else(|| format!("bad finding action: {row}"))?;
        let replacement = row
            .get("replacement")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let draft = FindingDraft {
            location: location.to_owned(),
            evidence: evidence.to_owned(),
            pattern,
            why_obsolete: why.to_owned(),
            confidence,
            action,
            replacement,
        };
        draft.validate().map_err(|error| error.to_string())?;
        findings.push(draft);
    }
    Ok(findings)
}

/// Persist the findings, stamp the audit succeeded, and emit `prompt_audit.completed`.
async fn finish_succeeded(
    pool: &PgPool,
    tenant: TenantId,
    audit_id: Uuid,
    findings: &[FindingDraft],
) -> Result<(), JobFailure> {
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    for finding in findings {
        insert_finding_tx(&mut tx, tenant, audit_id, finding)
            .await
            .map_err(|error| JobFailure::retryable(format!("persist a finding: {error}")))?;
    }
    let count = i32::try_from(findings.len()).unwrap_or(i32::MAX);
    transition_audit_tx(
        &mut tx,
        tenant,
        audit_id,
        AuditStatus::Succeeded,
        Some(count),
        None,
    )
    .await
    .map_err(|error| JobFailure::retryable(format!("finish the audit: {error}")))?;
    emit_event(
        &mut tx,
        tenant,
        audit_id,
        "prompt_audit.completed",
        json!({"id": audit_id, "status": "succeeded", "finding_count": count}),
    )
    .await
    .map_err(|error| JobFailure::retryable(format!("emit prompt_audit.completed: {error}")))?;
    tx.commit()
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    Ok(())
}

/// Record a failure on the audit row. Best effort: the job's own error is the caller's to report.
async fn fail_audit(pool: &PgPool, tenant: TenantId, audit_id: Uuid, error: &str) {
    let short: String = error.chars().take(2_000).collect();
    let Ok(mut tx) = store::begin_scoped(pool, tenant).await else {
        return;
    };
    let _ = transition_audit_tx(
        &mut tx,
        tenant,
        audit_id,
        AuditStatus::Failed,
        None,
        Some(&short),
    )
    .await;
    let _ = emit_event(
        &mut tx,
        tenant,
        audit_id,
        "prompt_audit.completed",
        json!({"id": audit_id, "status": "failed"}),
    )
    .await;
    let _ = tx.commit().await;
}

async fn emit_event(
    conn: &mut sqlx::PgConnection,
    tenant: TenantId,
    audit_id: Uuid,
    event_type: &str,
    payload: Value,
) -> Result<(), store::outbox::OutboxError> {
    let event = store::NewEvent {
        topic: format!("prompt_audit:{audit_id}"),
        event_type: event_type.to_owned(),
        aggregate_type: "prompt_audit".to_owned(),
        aggregate_id: audit_id,
        payload,
        correlation_id: None,
        causation_id: None,
    };
    store::outbox::append(conn, tenant, &event)
        .await
        .map(|_| ())
}
