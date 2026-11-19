#![forbid(unsafe_code)]
//! `tools.probe` — read-only health for published toolkits (SPEC_V2 §11.9; T14, #83).
//!
//! A probe answers one question per toolkit: does the destination this toolkit declares actually
//! serve? The destination is never invented here — the service alias comes from the toolkit row,
//! its base URL from the same `CONVEYOR_EXTERNAL_<SERVICE>_URL` the dispatcher reads, and the
//! health path from the manifest that published the toolkit. A toolkit whose URL is unset is
//! `unconfigured`: a visible refusal, never a call to a guessable default.
//!
//! A failed probe disables the toolkit's bindings, which is what turns `tools.call` into a typed
//! `409 tool_not_ready` before any network call; a later green probe re-enables **only** the
//! bindings a probe disabled (`meta.probe_disabled`), so a binding an operator switched off by
//! hand stays off. In-flight runs are untouched: the broker resolves a binding once, at the call,
//! and a run already past that point finishes.
//!
//! ⚠️ The health call runs inside the action's transaction, which the broker's own path never
//! does. The probe is bounded to `PROBE_TIMEOUT` per toolkit and to `MAX_TARGETS`, so the
//! connection is held for a couple of seconds of waiting with no query in flight and no lock
//! taken; making this a pool-dispatched handler would need a new `HandlerRef` shape for one
//! action, which is a worse trade. `dry_run` exists so an operator can look before unbinding.

use std::time::{Duration, Instant};

use conveyor_store as store;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;

use crate::invoke::{ActionError, HandlerCtx};

/// The longest a single destination is given to answer its health path.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// The most toolkits one probe call walks (the action's 60 s deadline bounds the product).
const MAX_TARGETS: usize = 24;

/// `tools.probe`'s input.
#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
pub struct ToolsProbeIn {
    /// Probe one toolkit by key; every published toolkit when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolkit: Option<String>,
    /// Report only — never disable or re-enable a binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
}

/// One toolkit's probe result.
#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct ToolkitProbe {
    /// The toolkit key.
    pub key: String,
    /// The version that was probed.
    pub toolkit_version: i64,
    /// The destination service alias.
    pub service: String,
    /// `live | unreachable | unconfigured`.
    pub probe: String,
    /// The destination's HTTP status, when it answered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<i32>,
    /// Wall-clock time the destination took to answer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<i64>,
    /// Bindings this probe disabled.
    pub bindings_disabled: u64,
    /// Bindings this probe restored.
    pub bindings_restored: u64,
    /// Why the probe is not `live`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// The probe's answer: one row per toolkit.
#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct ToolsProbeOut {
    /// Whether the run was report-only.
    pub dry_run: bool,
    /// Every toolkit the probe walked.
    pub toolkits: Vec<ToolkitProbe>,
}

#[crate::action(
    name = "tools.probe",
    version = 1,
    kind = "internal",
    scopes = ["tools:write"],
    idempotent = true,
    timeout_ms = 60000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tools_probe(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ToolsProbeIn,
) -> Result<ToolsProbeOut, ActionError> {
    let dry_run = input.dry_run.unwrap_or(false);
    let targets = match input.toolkit.as_deref() {
        Some(key) => match store::tool::latest_toolkit(&mut *tx, ctx.tenant, key).await {
            Ok(Some(row)) => vec![row],
            Ok(None) => {
                return Err(ActionError::invalid_input(vec![crate::spec::FieldError {
                    field: "toolkit".to_owned(),
                    code: "unknown".to_owned(),
                    detail: format!("no toolkit {key:?} is published to this tenant"),
                }]));
            }
            Err(error) => {
                tracing::error!(%error, "probe: read the named toolkit");
                return Err(ActionError::internal());
            }
        },
        None => store::tool::published_toolkits(&mut *tx, ctx.tenant)
            .await
            .map_err(|error| {
                tracing::error!(%error, "probe: read the published toolkits");
                ActionError::internal()
            })?,
    };
    if targets.len() > MAX_TARGETS {
        return Err(ActionError::invalid_input(vec![crate::spec::FieldError {
            field: "toolkit".to_owned(),
            code: "too_many".to_owned(),
            detail: format!(
                "{} published toolkits exceeds the {MAX_TARGETS} one probe walks; probe by key",
                targets.len()
            ),
        }]));
    }

    let client = reqwest::Client::builder()
        .timeout(PROBE_TIMEOUT)
        .build()
        .map_err(|error| {
            tracing::error!(%error, "probe: build the HTTP client");
            ActionError::internal()
        })?;

    let mut out = Vec::with_capacity(targets.len());
    for target in targets {
        out.push(probe_one(&mut *tx, ctx.clone(), &client, &target, dry_run).await?);
    }
    Ok(ToolsProbeOut {
        dry_run,
        toolkits: out,
    })
}

async fn probe_one(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    client: &reqwest::Client,
    target: &store::tool::ToolkitRow,
    dry_run: bool,
) -> Result<ToolkitProbe, ActionError> {
    let health_path = target
        .meta
        .get("health_path")
        .and_then(Value::as_str)
        .unwrap_or("/health")
        .to_owned();
    let mut row = ToolkitProbe {
        key: target.key.clone(),
        toolkit_version: target.toolkit_version,
        service: target.service.clone(),
        probe: "unconfigured".to_owned(),
        http_status: None,
        latency_ms: None,
        bindings_disabled: 0,
        bindings_restored: 0,
        detail: None,
    };

    match crate::invoke::service_url(&target.service) {
        None => {
            row.detail = Some(format!(
                "CONVEYOR_EXTERNAL_{}_URL is not set",
                target.service.to_ascii_uppercase()
            ));
            if !dry_run {
                row.bindings_disabled =
                    store::tool::disable_bindings(&mut *tx, ctx.tenant, target.id)
                        .await
                        .map_err(|error| {
                            tracing::error!(%error, toolkit = %target.key, "probe: unbind");
                            ActionError::internal()
                        })?;
            }
        }
        Some(base) => {
            let url = format!("{}{}", base.trim_end_matches('/'), health_path);
            let started = Instant::now();
            match client.get(&url).send().await {
                Ok(response) => {
                    let status = response.status();
                    row.latency_ms =
                        Some(i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX));
                    row.http_status = Some(i32::from(status.as_u16()));
                    if status.is_success() {
                        row.probe = "live".to_owned();
                        if !dry_run {
                            row.bindings_restored = store::tool::reenable_probe_disabled(
                                &mut *tx, ctx.tenant, target.id,
                            )
                            .await
                            .map_err(|error| {
                                tracing::error!(%error, toolkit = %target.key, "probe: rebind");
                                ActionError::internal()
                            })?;
                        }
                    } else {
                        row.probe = "unreachable".to_owned();
                        row.detail = Some(format!("{url} answered {status}"));
                        if !dry_run {
                            row.bindings_disabled = store::tool::disable_bindings(
                                &mut *tx, ctx.tenant, target.id,
                            )
                            .await
                            .map_err(|error| {
                                tracing::error!(%error, toolkit = %target.key, "probe: unbind");
                                ActionError::internal()
                            })?;
                        }
                    }
                }
                Err(error) => {
                    row.latency_ms =
                        Some(i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX));
                    row.probe = "unreachable".to_owned();
                    row.detail = Some(format!("{url}: {error}"));
                    if !dry_run {
                        row.bindings_disabled =
                            store::tool::disable_bindings(&mut *tx, ctx.tenant, target.id)
                                .await
                                .map_err(|error| {
                                    tracing::error!(%error, toolkit = %target.key, "probe: unbind");
                                    ActionError::internal()
                                })?;
                    }
                }
            }
        }
    }
    Ok(row)
}
