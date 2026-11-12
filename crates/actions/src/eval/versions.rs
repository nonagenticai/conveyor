#![forbid(unsafe_code)]
//! `eval_versions.*` — the instrument, frozen by copy (S19 step 4).
//!
//! A version is immutable once minted: `create` copies the eval's current case and metric rows
//! verbatim under a new id, inherits `prices` and `harness_paths`, and recomputes the digest
//! over what it copied — all inside the handler's transaction. There is therefore no
//! `eval_versions.update` and no `.delete`; the way to change an instrument is to mint the next
//! version, which is also what makes an old sign-off provably about the old rows.
//!
//! A fresh version carries **no** `split_sha256` even though its cases were copied with their
//! sides: the split is drawn per version, so a new version is unsplit until `evals.draw_split`
//! locks one. Readiness reports exactly that (`split_missing`).

use conveyor_store as store;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;
use crate::views::Page;

use super::{VersionDetailView, VersionView, eval_error, metric_view, version_view};

/// `eval_versions.create` — the eval whose current rows are frozen into the next version.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct VersionCreateIn {
    /// The eval to version, resolved in the caller's tenant.
    pub eval_id: Uuid,
}

/// `eval_versions.get` — one version by id.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct VersionIdIn {
    /// The version id.
    pub id: Uuid,
}

/// `eval_versions.list` — every version of one eval, newest number first.
///
/// There is deliberately no page envelope: an eval has one version per authored change, the
/// store returns the whole family, and a cursor that silently did nothing would be a lie in
/// the contract.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct VersionListIn {
    /// The eval whose versions are listed.
    pub eval_id: Uuid,
}

/// Freeze the eval's current instrument into the next version and point the eval at it.
#[crate::action(
    name = "eval_versions.create",
    version = 1,
    kind = "internal",
    scopes = ["evals:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_versions_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: VersionCreateIn,
) -> Result<HandlerOutput<VersionView>, ActionError> {
    let version =
        store::eval::create_version_tx(&mut *tx, ctx.tenant, input.eval_id, Some(ctx.actor_id))
            .await
            .map_err(|error| eval_error(error, ctx.action))?;
    Ok(HandlerOutput::created(version_view(&version)))
}

/// One version by id.
#[crate::action(
    name = "eval_versions.get",
    version = 1,
    kind = "internal",
    scopes = ["evals:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_versions_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: VersionIdIn,
) -> Result<VersionDetailView, ActionError> {
    let version = store::eval::get_version_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    let metrics = store::eval::list_metrics_tx(&mut *tx, ctx.tenant, version.id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    Ok(VersionDetailView {
        version: version_view(&version),
        metrics: metrics.iter().map(metric_view).collect(),
    })
}

/// Every version of one eval, newest number first.
#[crate::action(
    name = "eval_versions.list",
    version = 1,
    kind = "internal",
    scopes = ["evals:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn eval_versions_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: VersionListIn,
) -> Result<Page<VersionView>, ActionError> {
    // The eval is resolved first so a foreign or absent id is a `404`, not an empty page: an
    // empty page for an eval the caller cannot see would read as "it has no versions".
    let eval = store::eval::get_eval_tx(&mut *tx, ctx.tenant, input.eval_id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    let versions = store::eval::list_versions_tx(&mut *tx, ctx.tenant, eval.id)
        .await
        .map_err(|error| eval_error(error, ctx.action))?;
    Ok(Page {
        items: versions.iter().map(version_view).collect(),
        next_cursor: None,
    })
}
