#![forbid(unsafe_code)]
//! `publish.start` · `publish.status` — the console's one publish button.
//!
//! A publish is a **job**, not a request: `publish.start` enqueues `publish.design` and returns
//! the job id; the worker builds the pack and opens a pull request on the homepage repository;
//! `publish.status` reports the job and the PR URL the worker recorded.
//!
//! The target is a `repository` row with a sealed credential secret. The action refuses a
//! repository without one (`422 publish_not_configured`) rather than attempting an anonymous
//! push. The token never leaves the process: it is opened only inside the job, by secret id.

use conveyor_store as store;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::integrations::github::RepoRef;
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// The worker job kind a publish enqueues.
pub const PUBLISH_KIND: &str = "publish.design";

/// The job's dedupe key: one live publish per repository.
pub fn dedupe_key(repository_id: Uuid) -> String {
    format!("repo:{repository_id}")
}

/// The start input: the repository to publish to.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PublishStartIn {
    /// The `repository` row naming the homepage repository and its credential secret.
    pub repository_id: Uuid,
}

/// The start result.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PublishStartOut {
    /// The job id, whether this call enqueued it or one was already live.
    pub job_id: Uuid,
    /// `pending | running | succeeded | failed | dead`.
    pub state: String,
    /// True when a live job with the same dedupe key already existed.
    pub already_running: bool,
}

/// Start the design publish for one repository.
///
/// One live job per repository: a second click joins the first rather than racing it with a
/// second branch and a second PR.
#[crate::action(
    name = "publish.start",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn publish_start(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PublishStartIn,
) -> Result<HandlerOutput<PublishStartOut>, ActionError> {
    let action = "publish.start";
    let repository =
        store::repository::get_repository_tx(&mut *tx, ctx.tenant, input.repository_id)
            .await
            .map_err(|error| {
                tracing::error!(%error, "publish.start: read repository");
                ActionError::internal()
            })?
            .ok_or_else(|| ActionError::resource_not_found("such repository"))?;
    if repository.credential_secret_id.is_none() {
        return Err(ActionError::new(
            422,
            "publish_not_configured",
            "this repository has no credential secret; add a fine-grained GitHub token in the Publishing panel",
        ));
    }
    if RepoRef::parse(&repository.url).is_none() {
        return Err(ActionError::new(
            422,
            "publish_not_configured",
            "the publish target must be a GitHub repository URL",
        ));
    }
    if repository.default_branch.trim().is_empty() {
        return Err(ActionError::new(
            422,
            "publish_not_configured",
            "this repository has no default branch",
        ));
    }
    let tenant_slug = store::tenants::slug_by_id_tx(&mut *tx, ctx.tenant.as_uuid())
        .await
        .map_err(|error| {
            tracing::error!(%error, "publish.start: read tenant slug");
            ActionError::internal()
        })?
        .ok_or_else(ActionError::internal)?;
    let dedupe = dedupe_key(input.repository_id);
    let payload = json!({
        "repository_id": input.repository_id,
        "dedupe_key": dedupe,
        "tenant_slug": tenant_slug,
        "requested_by": ctx.actor_id,
    });
    let enqueued = store::jobs::enqueue_tx(
        &mut *tx,
        ctx.tenant,
        PUBLISH_KIND,
        &dedupe,
        payload,
        None,
        1,
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "publish.start: enqueue");
        ActionError::internal()
    })?;
    let (job_id, state, already_running) = match enqueued {
        Some(job_id) => (job_id, "pending".to_owned(), false),
        None => {
            // A live job already exists (or it finished between the insert and this read).
            let job = store::jobs::by_dedupe(&mut *tx, ctx.tenant, PUBLISH_KIND, &dedupe)
                .await
                .map_err(|error| {
                    tracing::error!(%error, "publish.start: read live job");
                    ActionError::internal()
                })?
                .into_iter()
                .next()
                .ok_or_else(ActionError::internal)?;
            (job.id, job.status, true)
        }
    };
    if !already_running {
        let event = store::NewEvent {
            topic: format!("publish:{job_id}"),
            event_type: "publish.requested".to_owned(),
            aggregate_type: "publish".to_owned(),
            aggregate_id: job_id,
            payload: json!({
                "job_id": job_id,
                "repository_id": input.repository_id,
                "requested_by": ctx.actor_id,
            }),
            correlation_id: None,
            causation_id: None,
        };
        store::outbox::append(&mut *tx, ctx.tenant, &event)
            .await
            .map_err(|error| {
                tracing::error!(%error, action, "publish.start: append event");
                ActionError::internal()
            })?;
    }
    Ok(HandlerOutput::ok(PublishStartOut {
        job_id,
        state,
        already_running,
    }))
}

/// The status input: the repository whose latest publish is asked about.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PublishStatusIn {
    /// The `repository` row the publish targets.
    pub repository_id: Uuid,
}

/// The status result.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PublishStatusOut {
    /// `none` when this repository has never been published.
    pub state: String,
    /// The latest job id, when one exists.
    pub job_id: Option<Uuid>,
    /// The PR the worker opened.
    pub pr_url: Option<String>,
    /// The failure, when the latest job failed.
    pub error: Option<String>,
    /// When the pack was generated.
    pub generated_at: Option<String>,
    /// The snapshot payload hash the pack was built from.
    pub snapshot_sha256: Option<String>,
    /// The number of published outputs in the pack.
    pub outputs: Option<i64>,
}

/// Read the latest publish job for one repository.
#[crate::action(
    name = "publish.status",
    version = 1,
    kind = "internal",
    scopes = ["jobs:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn publish_status(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PublishStatusIn,
) -> Result<HandlerOutput<PublishStatusOut>, ActionError> {
    let dedupe = dedupe_key(input.repository_id);
    let job = store::jobs::by_dedupe(&mut *tx, ctx.tenant, PUBLISH_KIND, &dedupe)
        .await
        .map_err(|error| {
            tracing::error!(%error, "publish.status: read jobs");
            ActionError::internal()
        })?
        .into_iter()
        .next();
    let Some(job) = job else {
        return Ok(HandlerOutput::ok(PublishStatusOut {
            state: "none".to_owned(),
            job_id: None,
            pr_url: None,
            error: None,
            generated_at: None,
            snapshot_sha256: None,
            outputs: None,
        }));
    };
    let result = job.payload.get("result").cloned().unwrap_or(Value::Null);
    let text = |key: &str| result.get(key).and_then(Value::as_str).map(str::to_owned);
    Ok(HandlerOutput::ok(PublishStatusOut {
        state: job.status,
        job_id: Some(job.id),
        pr_url: text("pr_url"),
        error: job.last_error,
        generated_at: text("generated_at"),
        snapshot_sha256: text("snapshot_sha256"),
        outputs: result.get("outputs").and_then(Value::as_i64),
    }))
}
