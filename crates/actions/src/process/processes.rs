#![forbid(unsafe_code)]
//! `processes.create` · `.get` · `.list` · `.update` · `.publish` · `.retire`.
//!
//! The version family is the model (S11 §5.3): `create` is the only way a version appears — version 1 for a
//! new key, `max(version) + 1` when the key already exists — and it always produces a **draft**.
//! `update` edits a draft only. `publish` freezes it: it validates against the live manifest and the
//! tenant's gate codes, flips the status and writes the `revision` snapshot in one transaction.
//!
//! Preconditions follow CONTRACT §3 exactly: an **edit** (`update`) requires `If-Match` and answers a
//! stale one with `412` carrying the current representation; a **state transition** (`publish`, `retire`)
//! is status-guarded and takes no precondition.

use std::collections::{BTreeMap, HashMap};

use conveyor_domain::RevisionStatus;
use conveyor_domain::compare::content_hash;
use conveyor_domain::plan::{self, GateFacts};
use conveyor_domain::process::{ActionFacts, PublishFacts, StepKind, validate_publish};
use conveyor_store as store;
use conveyor_store::governance::{SnapshotRevision, write_snapshot_revision};
use conveyor_store::process::{DraftPatch, NewProcess, ProcessStoreError};
use conveyor_store::revision::get_revision_by_seq_tx;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::board::{etag_of, require_precondition_with};
use crate::invoke::{ActionError, HandlerCtx};
use crate::process::{
    ENTITY_TYPE, ProcessCreateIn, ProcessGetIn, ProcessIdIn, ProcessListIn, ProcessOut,
    ProcessPublishIn, ProcessPublishOut, ProcessUpdateIn, cursor_of, decode_cursor,
    emit_process_event, frozen_content, invalid, parse_status, plan_refusal, process_error,
    process_out, publish_refusal, step_inputs_from_records, validated_steps,
};
use crate::registry::registry;
use crate::spec::HandlerOutput;
use crate::views::Page;

/// The event a successful publish appends.
const PROCESS_PUBLISHED: &str = "process.published";
/// The event a successful retire appends.
const PROCESS_RETIRED: &str = "process.retired";

/// The most gate codes a publish will read. A tenant's gate vocabulary is a handful of rows; this is a
/// ceiling that cannot silently truncate one, not a page size.
const GATE_SCAN_LIMIT: i64 = 500;

/// Create a draft: version 1 of a new key, or the next version of an existing one.
///
/// On an existing key the source version's steps are copied forward, and only the members the caller
/// actually supplies replace them — so `{key,title}` starts an edit of the published version as it stands.
#[crate::action(
    name = "processes.create",
    version = 1,
    kind = "internal",
    scopes = ["processes:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn processes_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProcessCreateIn,
) -> Result<HandlerOutput<ProcessOut>, ActionError> {
    let key = input.key.trim();
    if key.is_empty() {
        return Err(invalid("key", "required", "must not be empty"));
    }
    let title = input.title.trim();
    if title.is_empty() {
        return Err(invalid("title", "required", "must not be empty"));
    }
    let steps = input.steps.as_deref().map(validated_steps).transpose()?;
    let card_type_id = match input.card_type_code.as_deref() {
        None => None,
        Some(code) => Some(resolve_card_type(&mut *tx, ctx.tenant, code).await?.id),
    };

    let existing = store::process::get_process_by_key_tx(&mut *tx, ctx.tenant, key, None)
        .await
        .map_err(|error| process_error(error, "processes.create"))?;

    let record = match existing {
        Some(row) if row.status == "draft" => {
            return Err(ActionError::new(
                409,
                "process.draft_exists",
                format!(
                    "the key {key:?} already has a draft; edit it, or publish it before creating another version"
                ),
            ));
        }
        Some(row) => {
            // Copy forward, then apply only what the caller named. The fork inherited the source's
            // title/description/card type; an omitted member means "keep the version I am editing".
            let forked =
                store::process::fork_version_tx(&mut *tx, ctx.tenant, row.id, Some(ctx.actor_id))
                    .await
                    .map_err(|error| process_error(error, "processes.create"))?;
            store::process::update_draft_tx(
                &mut *tx,
                ctx.tenant,
                forked.id,
                &DraftPatch {
                    expected: None,
                    title: Some(title),
                    description: input.description.as_deref(),
                    card_type_id: card_type_id.map(Some),
                    steps: steps.as_deref(),
                },
            )
            .await
            .map_err(|error| process_error(error, "processes.create"))?
        }
        None => {
            let created = store::process::create_draft_tx(
                &mut *tx,
                ctx.tenant,
                &NewProcess {
                    key: key.to_owned(),
                    title: title.to_owned(),
                    description: input.description.clone().unwrap_or_default(),
                    card_type_id,
                    // The catalogue's list kind / project type / outputs are seeded through the
                    // store (`seed::processes`); the action surface adds them when S12 authors a
                    // process from the UI.
                    list_kind: None,
                    project_type_id: None,
                    outputs: serde_json::json!([]),
                    capability_id: None,
                    owner_user_id: Some(ctx.actor_id),
                },
            )
            .await
            .map_err(|error| process_error(error, "processes.create"))?;
            if let Some(steps) = steps.as_deref() {
                store::process::insert_steps_tx(&mut *tx, ctx.tenant, created.id, steps)
                    .await
                    .map_err(|error| process_error(error, "processes.create"))?;
            }
            created
        }
    };

    let out = read_one(&mut *tx, ctx.tenant, &record).await?;
    Ok(HandlerOutput::created(out).with_etag(etag_of(record.lock_version)))
}

/// One version, by id or by key.
#[crate::action(
    name = "processes.get",
    version = 1,
    kind = "internal",
    scopes = ["processes:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn processes_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProcessGetIn,
) -> Result<ProcessOut, ActionError> {
    let record = match (input.process_id, input.key.as_deref()) {
        (Some(id), None) => {
            if input.version.is_some() {
                return Err(invalid(
                    "version",
                    "invalid_value",
                    "an exact version is addressed by key, not by process_id",
                ));
            }
            store::process::get_process_tx(&mut *tx, ctx.tenant, id)
                .await
                .map_err(|error| process_error(error, "processes.get"))?
                .ok_or_else(|| ActionError::resource_not_found("process"))?
        }
        (None, Some(key)) => match input.version {
            Some(version) => {
                store::process::get_process_by_key_tx(&mut *tx, ctx.tenant, key, Some(version))
                    .await
                    .map_err(|error| process_error(error, "processes.get"))?
                    .ok_or_else(|| ActionError::resource_not_found("process"))?
            }
            // No version: the key's PUBLISHED version is what a caller means by "the process".
            None => store::process::published_by_key_tx(&mut *tx, ctx.tenant, key)
                .await
                .map_err(|error| process_error(error, "processes.get"))?
                .ok_or_else(|| ActionError::resource_not_found("process"))?,
        },
        (Some(_), Some(_)) => {
            return Err(invalid(
                "key",
                "invalid_value",
                "name either process_id or key, not both",
            ));
        }
        (None, None) => {
            return Err(invalid(
                "process_id",
                "required",
                "name a process_id or a key",
            ));
        }
    };
    read_one(&mut *tx, ctx.tenant, &record).await
}

/// One page of processes, newest version last within each key.
#[crate::action(
    name = "processes.list",
    version = 1,
    kind = "internal",
    scopes = ["processes:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn processes_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProcessListIn,
) -> Result<Page<ProcessOut>, ActionError> {
    let status = input.status.as_deref().map(parse_status).transpose()?;
    let status = status.map(|status| status.as_str());
    let card_type_id = match input.card_type_code.as_deref() {
        None => None,
        Some(code) => Some(resolve_card_type(&mut *tx, ctx.tenant, code).await?.id),
    };
    let cursor = decode_cursor(input.page.cursor.as_deref())?;
    let limit = store::actions::clamp_limit(input.page.limit);
    // One extra row decides whether a next page exists; it is not returned.
    let mut rows = store::process::list_processes_tx(
        &mut *tx,
        ctx.tenant,
        status,
        card_type_id,
        cursor
            .as_ref()
            .map(|(key, version)| (key.as_str(), *version)),
        limit + 1,
    )
    .await
    .map_err(|error| process_error(error, "processes.list"))?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last().map(cursor_of)
    } else {
        None
    };

    let ids: Vec<Uuid> = rows.iter().map(|row| row.id).collect();
    let mut steps: HashMap<Uuid, Vec<store::process::ProcessStepRecord>> = HashMap::new();
    for step in store::process::list_steps_for_processes_tx(&mut *tx, ctx.tenant, &ids)
        .await
        .map_err(|error| process_error(error, "processes.list"))?
    {
        steps.entry(step.process_id).or_default().push(step);
    }
    let codes = card_type_codes(&mut *tx, ctx.tenant).await?;
    let items = rows
        .iter()
        .map(|row| {
            let code = row.card_type_id.and_then(|id| codes.get(&id).cloned());
            process_out(
                row,
                code,
                steps.get(&row.id).map(Vec::as_slice).unwrap_or(&[]),
            )
        })
        .collect();
    Ok(Page { items, next_cursor })
}

/// Change a draft's title, description or whole step list.
#[crate::action(
    name = "processes.update",
    version = 1,
    kind = "internal",
    scopes = ["processes:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn processes_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProcessUpdateIn,
) -> Result<HandlerOutput<ProcessOut>, ActionError> {
    let current = store::process::get_process_tx(&mut *tx, ctx.tenant, input.process_id)
        .await
        .map_err(|error| process_error(error, "processes.update"))?
        .ok_or_else(|| ActionError::resource_not_found("process"))?;
    // Status before precondition. An edit of a published version cannot become legal whatever `If-Match`
    // says, so `409 not_draft` is the answer that tells the caller to fork instead; answering `412` would
    // send them round a GET-and-retry loop for a request that can never succeed.
    if current.status != "draft" {
        return Err(ActionError::new(
            409,
            "process.not_draft",
            format!(
                "the process is {}; only a draft is editable — create a new version from it",
                current.status
            ),
        ));
    }
    let before = read_one(tx, ctx.tenant, &current).await?;
    let expected = require_precondition_with(
        &ctx,
        "processes.update",
        serde_json::to_value(&before).unwrap_or(serde_json::Value::Null),
    )?;

    if let Some(title) = input.title.as_deref()
        && title.trim().is_empty()
    {
        return Err(invalid("title", "required", "must not be empty"));
    }
    let steps = input.steps.as_deref().map(validated_steps).transpose()?;
    let record = store::process::update_draft_tx(
        &mut *tx,
        ctx.tenant,
        current.id,
        &DraftPatch {
            expected: Some(expected),
            title: input.title.as_deref().map(str::trim),
            description: input.description.as_deref(),
            // The contract does not expose the card type on an edit, so it is never touched here.
            card_type_id: None,
            steps: steps.as_deref(),
        },
    )
    .await
    .map_err(|error| match error {
        // A stale `If-Match` must answer with the representation the caller should now be looking at
        // (CONTRACT §3), so a client can retry without a second GET. The row was read at the top of this
        // handler, so that answer is already in hand — the generic mapper cannot supply it.
        ProcessStoreError::VersionMismatch { current } => ActionError::precondition_with(
            serde_json::to_value(&before).unwrap_or(serde_json::Value::Null),
            format!(
                "the process moved to version {} before this call",
                current.get()
            ),
        ),
        other => process_error(other, "processes.update"),
    })?;
    let out = read_one(&mut *tx, ctx.tenant, &record).await?;
    Ok(HandlerOutput::ok(out).with_etag(etag_of(record.lock_version)))
}

/// Freeze a draft: validate it against the live manifest, publish it, and write its `revision`.
///
/// A second publish of an already-published version returns the SAME triple rather than a `409`: the
/// caller asked for a state, and the state already holds (§5.2 "re-publish returns the same").
#[crate::action(
    name = "processes.publish",
    version = 1,
    kind = "internal",
    scopes = ["processes:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn processes_publish(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProcessPublishIn,
) -> Result<HandlerOutput<ProcessPublishOut>, ActionError> {
    let current = store::process::get_process_tx(&mut *tx, ctx.tenant, input.process_id)
        .await
        .map_err(|error| process_error(error, "processes.publish"))?
        .ok_or_else(|| ActionError::resource_not_found("process"))?;

    if current.status == "published" {
        return Ok(HandlerOutput::ok(
            published_triple(&mut *tx, ctx.tenant, &current).await?,
        ));
    }
    if current.status != "draft" {
        return Err(ActionError::new(
            409,
            "process.not_draft",
            format!("the process is {}, not a draft", current.status),
        ));
    }

    let steps = store::process::list_steps_tx(&mut *tx, ctx.tenant, current.id)
        .await
        .map_err(|error| process_error(error, "processes.publish"))?;
    let step_inputs = step_inputs_from_records(&steps);
    // One registry pass supplies both facts: `validate_publish` needs the idempotency flag, and the
    // plan projection needs the output schema a declared path must resolve in.
    let mut action_outputs: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    let facts: Vec<ActionFacts> = registry()
        .iter()
        .map(|spec| {
            action_outputs.insert(spec.name.to_owned(), spec.output_schema());
            ActionFacts {
                name: spec.name.to_owned(),
                idempotent: spec.idempotent,
            }
        })
        .collect();
    let gate_facts: Vec<GateFacts> = store::catalogue::list_entries_tx(
        &mut *tx,
        ctx.tenant,
        Some("gate"),
        false,
        GATE_SCAN_LIMIT,
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "read the tenant's gate codes");
        ActionError::internal()
    })?
    .into_iter()
    .map(|entry| {
        // A valid timestamp (past or future) is not a defect: the runtime opens the gate and an
        // expiry applies the step's `on_result`. Only a value the runtime cannot read as a timestamp
        // is recorded — a string that fails RFC3339, or a present non-string the runtime's
        // `and_then(as_str)` would silently ignore.
        let bad_expiry = match entry.meta.get("expires_at") {
            None => None,
            Some(serde_json::Value::String(text)) => chrono::DateTime::parse_from_rfc3339(text)
                .err()
                .map(|_| text.clone()),
            Some(other) => Some(other.to_string()),
        };
        GateFacts {
            code: entry.code.to_string(),
            bad_expiry,
        }
    })
    .collect();
    let gates: Vec<String> = gate_facts.iter().map(|facts| facts.code.clone()).collect();

    // The process-model facts: the live process agents a `agent` step may name, the published
    // SOPs every step needs, and the process/workflow keys a `call` step may target.
    let process_agents = store::process::live_process_agent_ids_tx(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| {
            tracing::error!(%error, "read live process agents");
            ActionError::internal()
        })?;
    let procedures = store::procedure::published_ids_tx(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| {
            tracing::error!(%error, "read published procedures");
            ActionError::internal()
        })?;
    let mut call_targets = store::process::published_keys_tx(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| {
            tracing::error!(%error, "read published process keys");
            ActionError::internal()
        })?;
    call_targets.extend(
        store::workflow::workflow::published_keys_tx(&mut *tx, ctx.tenant)
            .await
            .map_err(|error| {
                tracing::error!(%error, "read published workflow keys");
                ActionError::internal()
            })?,
    );
    // An `eval` inspection pins one eval; a pin that names a missing eval is unrunnable, and the
    // executor would only find out by parking the step on it. Publish is where it is a 422.
    let evals: std::collections::BTreeSet<uuid::Uuid> =
        store::eval::evals::all_ids_tx(&mut *tx, ctx.tenant)
            .await
            .map_err(|error| {
                tracing::error!(%error, "read the tenant's evals");
                ActionError::internal()
            })?
            .into_iter()
            .collect();
    let mut procedure_evals = Vec::new();
    for procedure_id in &procedures {
        let Some(sop) = store::procedure::get_sop_tx(&mut *tx, ctx.tenant, *procedure_id)
            .await
            .map_err(|error| {
                tracing::error!(%error, "read the SOP an eval pin lives on");
                ActionError::internal()
            })?
        else {
            continue;
        };
        if let Ok(conveyor_domain::procedure::Inspection::Eval { eval_id, .. }) =
            conveyor_domain::procedure::validate_inspection(&sop.inspection)
        {
            procedure_evals.push((*procedure_id, eval_id));
        }
    }
    let publish_facts = PublishFacts {
        actions: facts,
        gates,
        process_agents,
        procedures,
        call_targets,
        self_key: current.key.clone(),
        evals,
        procedure_evals,
    };
    validate_publish(&step_inputs, &publish_facts).map_err(publish_refusal)?;

    // S21 step 2: the plan projection. `validate_publish` refuses a reference that can never be
    // legal at any run; this refuses one that is legal in isolation but cannot be satisfied by
    // the plan — an undeclared output, a producer that may be skipped, a self/forward reference, or
    // a gate whose catalogue expiry cannot be read.
    let findings = plan::findings(&step_inputs, &action_outputs, &gate_facts);
    if !findings.is_empty() {
        return Err(plan_refusal(&findings));
    }

    // S11 step 8: a process may only contain actions its PUBLISHER can invoke. The executor runs
    // as an internal `service` principal with the synthetic tenant-admin role (`kata_executor`,
    // admin-equivalent), so without this check a member — who holds `processes:write` — could
    // author a process that performs admin-scoped writes (`cards.*` is admin-only). Publishing is
    // where the runner is certified, so it is where the certification is checked; `invoke` performs
    // the identical check for an ordinary call.
    for (index, step) in step_inputs.iter().enumerate() {
        // Only a `computed` step invokes a registered action. An `agent`, `gate` or `call` step
        // has no action by design, and looking one up would refuse every one of them here —
        // after `validate_publish` had already accepted it for its own kind.
        if step.kind() != Some(StepKind::Computed) {
            continue;
        }
        let Some(spec) = registry().get(&step.action) else {
            // `validate_publish` already refused an unknown action; fail closed if it ever does not.
            return Err(ActionError::new(
                422,
                "process.action_unknown",
                format!(
                    "step {}: {:?} is not a registered action",
                    index + 1,
                    step.action
                ),
            ));
        };
        // S06d: the publisher is checked with the authority it actually acts with. A machine
        // connector is bounded by its tenant grant set and the machine floor, exactly as an
        // invocation would be — otherwise a channel with `processes.publish` could freeze a
        // step it could not itself invoke, and the internal executor would run it for it.
        let permitted = match &ctx.channel {
            Some(authority) => spec.permits_channel(authority),
            None => spec.permits(ctx.role),
        };
        if !permitted {
            return Err(ActionError::new(
                403,
                "process.action_forbidden",
                format!(
                    "step {}: {:?} requires authority the publisher does not hold",
                    index + 1,
                    step.action
                ),
            ));
        }
    }

    // Status flip, previous-version retirement, the frozen revision and the event are ONE transaction:
    // a published process can never exist without the snapshot that says what was published.
    // S16: a draft is audited AT PUBLISH. The check is deterministic, synchronous and holds no
    // write scope over the process it audits; only a `high`-confidence finding refuses.
    let check = crate::prompt_audit::audits::check_draft(&current, &steps);
    if check.blocking && !input.acknowledge_findings {
        let locations: Vec<&str> = check
            .findings
            .iter()
            .filter(|finding| finding.confidence == "high")
            .map(|finding| finding.location.as_str())
            .collect();
        return Err(ActionError::new(
            422,
            "process_audit_findings",
            format!(
                "the draft has high-confidence prompt-audit findings at {}; fix them, or publish with acknowledge_findings=true",
                locations.join(", ")
            ),
        ));
    }
    if input.acknowledge_findings {
        let mut entry = store::AuditEntry::service("processes.publish", "ok");
        entry.actor_id = Some(ctx.actor_id);
        entry.actor_kind = ctx.actor_kind.clone();
        entry.request_id = ctx.request_id.clone();
        entry.resource_type = Some("process".to_owned());
        entry.resource_id = Some(current.id);
        entry.details = serde_json::json!({
            "acknowledge_findings": true,
            "findings": check
                .findings
                .iter()
                .filter(|finding| finding.confidence == "high")
                .map(|finding| serde_json::json!({
                    "location": finding.location,
                    "pattern": finding.pattern,
                    "confidence": finding.confidence,
                }))
                .collect::<Vec<_>>(),
        });
        store::audit::record(&mut *tx, ctx.tenant, &entry)
            .await
            .map_err(|error| {
                tracing::error!(%error, "record the prompt-audit acknowledgement");
                ActionError::internal()
            })?;
    }
    let published = store::process::publish_tx(&mut *tx, ctx.tenant, current.id, None)
        .await
        .map_err(|error| process_error(error, "processes.publish"))?;
    let content = frozen_content(&step_inputs)?;
    let hash = content_hash(&content);
    let seq = i32::try_from(published.version).map_err(|error| {
        tracing::error!(%error, version = published.version, "process version out of i32 range");
        ActionError::internal()
    })?;
    let revision_id = write_snapshot_revision(
        &mut *tx,
        ctx.tenant,
        &SnapshotRevision {
            entity_type: ENTITY_TYPE,
            entity_id: published.id,
            seq,
            label: "published",
            content: &content,
            content_hash: &hash,
            author_id: Some(ctx.actor_id),
            status: RevisionStatus::Published,
        },
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "freeze the process revision");
        ActionError::internal()
    })?;
    emit_process_event(&mut *tx, &ctx, PROCESS_PUBLISHED, &published).await?;

    Ok(HandlerOutput::ok(ProcessPublishOut {
        version: published.version,
        revision_id: revision_id.to_string(),
        content_hash: hash,
    }))
}

/// Retire a published version. A run already underway is unaffected — it resolves against the frozen
/// version it started on (§5.2, §5.5).
#[crate::action(
    name = "processes.retire",
    version = 1,
    kind = "internal",
    scopes = ["processes:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn processes_retire(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProcessIdIn,
) -> Result<HandlerOutput<ProcessOut>, ActionError> {
    let retired = store::process::retire_tx(&mut *tx, ctx.tenant, input.process_id, None)
        .await
        .map_err(|error| process_error(error, "processes.retire"))?;
    emit_process_event(&mut *tx, &ctx, PROCESS_RETIRED, &retired).await?;
    let out = read_one(&mut *tx, ctx.tenant, &retired).await?;
    Ok(HandlerOutput::ok(out).with_etag(etag_of(retired.lock_version)))
}

// ── helpers ───────────────────────────────────────────────────────────────────────────────────

/// A row plus its steps and card-type code, as every single-row response returns it.
async fn read_one(
    conn: &mut PgConnection,
    tenant: conveyor_domain::TenantId,
    record: &store::process::ProcessRecord,
) -> Result<ProcessOut, ActionError> {
    let steps = store::process::list_steps_tx(&mut *conn, tenant, record.id)
        .await
        .map_err(|error| process_error(error, "processes.read"))?;
    let code = match record.card_type_id {
        None => None,
        Some(id) => store::board::get_card_type_tx(&mut *conn, tenant, id)
            .await
            .map_err(|error| {
                tracing::error!(%error, "read the process's card type");
                ActionError::internal()
            })?
            .map(|card_type| card_type.code.to_string()),
    };
    Ok(process_out(record, code, &steps))
}

/// The stored revision of an already-published version.
async fn published_triple(
    conn: &mut PgConnection,
    tenant: conveyor_domain::TenantId,
    record: &store::process::ProcessRecord,
) -> Result<ProcessPublishOut, ActionError> {
    let seq = i32::try_from(record.version).map_err(|error| {
        tracing::error!(%error, version = record.version, "process version out of i32 range");
        ActionError::internal()
    })?;
    let revision = get_revision_by_seq_tx(&mut *conn, tenant, ENTITY_TYPE, record.id, seq)
        .await
        .map_err(|error| {
            tracing::error!(%error, "read the frozen process revision");
            ActionError::internal()
        })?
        .ok_or_else(|| {
            // A published row with no revision means the two writes came apart, which the single
            // transaction exists to prevent. Say so loudly instead of returning a plausible triple.
            tracing::error!(
                process_id = %record.id,
                version = record.version,
                "a published process has no frozen revision"
            );
            ActionError::internal()
        })?;
    Ok(ProcessPublishOut {
        version: record.version,
        revision_id: revision.id.to_string(),
        content_hash: revision.content_hash,
    })
}

/// Resolve a card-type code inside the caller's tenant.
async fn resolve_card_type(
    conn: &mut PgConnection,
    tenant: conveyor_domain::TenantId,
    code: &str,
) -> Result<conveyor_domain::CardType, ActionError> {
    store::board::get_card_type_by_code_tx(&mut *conn, tenant, code)
        .await
        .map_err(|error| {
            tracing::error!(%error, "read card type");
            ActionError::internal()
        })?
        .ok_or_else(|| {
            invalid(
                "card_type_code",
                "invalid_value",
                "no card type with this code exists in this tenant",
            )
        })
}

/// Every card type in the tenant, by id. One query serves a whole page's `card_type_code`.
async fn card_type_codes(
    conn: &mut PgConnection,
    tenant: conveyor_domain::TenantId,
) -> Result<HashMap<Uuid, String>, ActionError> {
    store::board::list_card_types_tx(&mut *conn, tenant)
        .await
        .map(|rows| {
            rows.into_iter()
                .map(|card_type| (card_type.id, card_type.code.to_string()))
                .collect()
        })
        .map_err(|error| {
            tracing::error!(%error, "read the tenant's card types");
            ActionError::internal()
        })
}
