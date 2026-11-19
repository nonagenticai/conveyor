#![forbid(unsafe_code)]
//! `toolkits.publish` — the estate import wave (SPEC_V2 §11.2, §11.8; T14, #83).
//!
//! Publishing is idempotent by **content**, not by call: each manifest's content hash is stored
//! on the toolkit version it produced, so a second publish of unchanged manifests writes nothing
//! and returns `unchanged`. A changed manifest inserts the next `toolkit_version`; a published
//! row is never updated except for its status transition (`draft` → `published` → `retired`).
//!
//! The input schemas are copied from the live registry at publish time. That is the whole point:
//! the broker validates a call against `tool.input_schema`, so a hand-written copy would drift the
//! first time an action changed — and the drift would surface as a `422` on a legitimate call.

use conveyor_store as store;
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgConnection;

use super::manifest::{self, ToolkitManifest};
use crate::invoke::{ActionError, HandlerCtx};
use crate::views::EmptyIn;

/// One toolkit's outcome.
#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct PublishedToolkit {
    /// The toolkit key.
    pub key: String,
    /// The version this publish created, advanced, or found current.
    pub toolkit_version: i64,
    /// The status the catalogue now carries.
    pub status: String,
    /// `created | version_bumped | status_changed | unchanged`.
    pub outcome: String,
}

/// The publish answer: one row per embedded toolkit.
#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct ToolkitsPublishOut {
    /// Every toolkit the wave touched, in manifest order.
    pub toolkits: Vec<PublishedToolkit>,
}

#[crate::action(
    name = "toolkits.publish",
    version = 1,
    kind = "internal",
    scopes = ["tools:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn toolkits_publish(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    _input: EmptyIn,
) -> Result<ToolkitsPublishOut, ActionError> {
    let manifests = manifest::manifests().map_err(|error| {
        tracing::error!(%error, "publish: the embedded toolkit manifests are invalid");
        ActionError::internal()
    })?;
    let mut out = Vec::with_capacity(manifests.len());
    for entry in manifests {
        out.push(publish_one(&mut *tx, ctx.clone(), entry).await?);
    }
    Ok(ToolkitsPublishOut { toolkits: out })
}

async fn publish_one(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    entry: &ToolkitManifest,
) -> Result<PublishedToolkit, ActionError> {
    let hash = manifest::content_hash(entry);
    let latest = store::tool::latest_toolkit(&mut *tx, ctx.tenant, &entry.key)
        .await
        .map_err(|error| {
            tracing::error!(%error, toolkit = %entry.key, "publish: read the newest version");
            ActionError::internal()
        })?;

    let meta = json!({
        "content_hash": hash,
        "health_path": entry.health_path,
    });

    match latest {
        None => {
            let version = 1;
            let id = insert_version(&mut *tx, ctx, entry, version, &meta).await?;
            let _ = id;
            Ok(PublishedToolkit {
                key: entry.key.clone(),
                toolkit_version: version,
                status: entry.status.clone(),
                outcome: "created".to_owned(),
            })
        }
        Some(current) => {
            let same_content = current
                .meta
                .get("content_hash")
                .and_then(Value::as_str)
                .is_some_and(|stored| stored == hash);
            if same_content {
                if current.status == entry.status {
                    return Ok(PublishedToolkit {
                        key: entry.key.clone(),
                        toolkit_version: current.toolkit_version,
                        status: current.status,
                        outcome: "unchanged".to_owned(),
                    });
                }
                store::tool::set_toolkit_status(&mut *tx, ctx.tenant, current.id, &entry.status)
                    .await
                    .map_err(|error| {
                        tracing::error!(%error, toolkit = %entry.key, "publish: move the status");
                        ActionError::internal()
                    })?;
                return Ok(PublishedToolkit {
                    key: entry.key.clone(),
                    toolkit_version: current.toolkit_version,
                    status: entry.status.clone(),
                    outcome: "status_changed".to_owned(),
                });
            }
            let version = current.toolkit_version + 1;
            insert_version(&mut *tx, ctx, entry, version, &meta).await?;
            Ok(PublishedToolkit {
                key: entry.key.clone(),
                toolkit_version: version,
                status: entry.status.clone(),
                outcome: "version_bumped".to_owned(),
            })
        }
    }
}

/// Insert one toolkit version and its tool rows, schemas taken from the registry.
async fn insert_version(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    entry: &ToolkitManifest,
    version: i64,
    meta: &Value,
) -> Result<uuid::Uuid, ActionError> {
    let id = store::tool::insert_toolkit(
        &mut *tx,
        ctx.tenant,
        &store::tool::NewToolkit {
            key: &entry.key,
            toolkit_version: version,
            title: &entry.title,
            service: &entry.service,
            auth_adapter: &entry.auth_adapter,
            licence_app: entry.licence_app.as_deref(),
            owner: &entry.owner,
            status: &entry.status,
            meta,
        },
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, toolkit = %entry.key, "publish: insert the toolkit version");
        ActionError::internal()
    })?;

    for tool in &entry.tools {
        // `manifests()` validated that this action exists and is external; a miss here would mean
        // the registry changed between validation and this loop, which cannot happen in one
        // process — so a miss is an internal error, not a caller's.
        let spec = crate::registry::registry().get(tool.action.as_str()).ok_or_else(|| {
            tracing::error!(action = %tool.action, "publish: the manifest names an absent action");
            ActionError::internal()
        })?;
        let schema = serde_json::to_value((spec.input)()).map_err(|error| {
            tracing::error!(%error, action = %tool.action, "publish: project the input schema");
            ActionError::internal()
        })?;
        let timeout_secs = i32::try_from(spec.timeout_ms.div_ceil(1000))
            .unwrap_or(300)
            .clamp(1, 300);
        store::tool::insert_tool(
            &mut *tx,
            ctx.tenant,
            &store::tool::NewTool {
                toolkit_id: id,
                key: &tool.key,
                handler_action: &tool.action,
                input_schema: &schema,
                effect: spec.effect.as_str(),
                timeout_secs,
            },
        )
        .await
        .map_err(|error| {
            tracing::error!(%error, toolkit = %entry.key, tool = %tool.key, "publish: insert the tool");
            ActionError::internal()
        })?;
    }
    Ok(id)
}
