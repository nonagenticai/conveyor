#![forbid(unsafe_code)]
//! `workers.list` and `providers.list` — the inventory of the conveyor's own execution plane.
//!
//! V1 read these from a separate `worker-management` service. SPEC_V2 §11.2 rules that service
//! legacy — "the conveyor's own durable queue replaces it, so it is not bound" — and no such
//! workload exists in the estate any more. The capability the console's Workers tab shows is
//! therefore served here, from the two things the conveyor actually owns:
//!
//! - the **durable queue**: `job` rows per kind and status (`crates/store/src/jobs.rs`);
//! - the **host registry**: the bare-metal hosts the conveyor may reach (`host_registry`,
//!   §10.4.1), without their credentials.
//!
//! `graphs.list` is not replaced. Nothing in the conveyor models a worker graph; the V1 action
//! described a service that no longer exists, and §11.2 says the queue replaces it.

use conveyor_store as store;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;

use crate::invoke::{ActionError, HandlerCtx};

/// No input: both inventories describe the caller's own tenant.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct InventoryIn {}

/// One `(kind, status)` bucket of the durable queue.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct QueueCountOut {
    /// The job kind.
    pub kind: String,
    /// `pending | running | succeeded | failed | dead`.
    pub status: String,
    /// How many rows are in the bucket.
    pub count: i64,
}

/// One host the conveyor may reach.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct HostOut {
    /// The name an `ops.host_*` action may pass.
    pub name: String,
    /// What the host is for.
    pub role: String,
    /// `launchctl | systemctl`.
    pub mechanism: String,
    /// The units the agent may act on.
    pub units: Vec<String>,
    /// A retired host stays registered and disabled.
    pub enabled: bool,
}

/// The worker inventory: the queue's depth and the hosts it may dispatch to.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WorkersOut {
    /// One row per kind and status present in the queue.
    pub queue: Vec<QueueCountOut>,
    /// Every registered host, enabled or not.
    pub hosts: Vec<HostOut>,
}

/// The worker inventory, read from the conveyor's own queue and host registry.
#[crate::action(
    name = "workers.list",
    version = 1,
    kind = "internal",
    scopes = ["integrations:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn workers_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    _input: InventoryIn,
) -> Result<WorkersOut, ActionError> {
    let queue = store::jobs::counts_tx(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| store_failure(error, ctx.action))?
        .into_iter()
        .map(|row| QueueCountOut {
            kind: row.kind,
            status: row.status,
            count: row.count,
        })
        .collect();
    let hosts = store::host::list_tx(&mut *tx)
        .await
        .map_err(|error| store_failure(error, ctx.action))?
        .into_iter()
        .map(|row| HostOut {
            name: row.name,
            role: row.role,
            mechanism: row.mechanism,
            units: row.units,
            enabled: row.enabled,
        })
        .collect();
    Ok(WorkersOut { queue, hosts })
}

/// One provider surface the tenant has configured.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProviderOut {
    /// The surface (`inference`).
    pub key: String,
    /// Its destination when the tenant has registered one; `null` means "the platform default".
    pub destination: Option<String>,
}

/// The providers this tenant may call.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProvidersOut {
    /// One row per configured surface.
    pub providers: Vec<ProviderOut>,
}

/// The tenant's provider inventory.
///
/// The conveyor owns exactly one provider fact — the tenant's inference destination
/// (`tenant_inference_endpoint`, #160) — so that is what this reports. A model catalogue would
/// be a second owner of inference routing, which §11.2 rules out.
#[crate::action(
    name = "providers.list",
    version = 1,
    kind = "internal",
    scopes = ["integrations:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn providers_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    _input: InventoryIn,
) -> Result<ProvidersOut, ActionError> {
    let destination = store::tenant_inference::destination(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| store_failure(error, ctx.action))?;
    Ok(ProvidersOut {
        providers: vec![ProviderOut {
            key: "inference".to_owned(),
            destination,
        }],
    })
}

/// A store failure is a `500` with a logged cause — an inventory read has no caller-fixable shape.
fn store_failure(error: impl std::fmt::Display, action: &str) -> ActionError {
    tracing::error!(%error, action, "the inventory read failed");
    ActionError::internal()
}
