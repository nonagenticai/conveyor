#![forbid(unsafe_code)]
//! `catalogues.list`, `catalogues.import` (S07 §5.4).
//!
//! Import reads the checked-in `seed/catalogue.json` through the same export `migrate --seed`
//! uses ([`store::seed::catalogue_export`]), so the two paths cannot diverge: there is one
//! embedded document, one parser and one importer.
//!
//! `dry_run: true` runs that same importer inside a SAVEPOINT the action rolls back, so the
//! counts are exactly what WOULD change and nothing is written — and, because a dry run is
//! not a mutation, `catalogue.imported` is emitted only for a real one.
//!
//! `kind` narrows a run to one vocabulary. `card_type` is **not** one of the eleven
//! catalogue kinds (S07 §5.4), so a kind-scoped run imports no card types; a run without a
//! kind imports both, card types first, because a project type's matrix is keyed by their
//! codes.

use std::str::FromStr;

use conveyor_domain::{CatalogueKind, TenantId};
use conveyor_store as store;
use conveyor_store::catalogue::{
    CardTypeImport, CatalogueRecord, CatalogueStoreError, ImportCounts, ImportEntry,
};
use sqlx::{Connection, PgConnection};

use crate::board::events::{RawEvent, append_raw_event};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

use super::{
    CatalogueEntryOut, CatalogueImportIn, CatalogueImportOut, CatalogueListIn, Page, invalid,
    store_error,
};

/// The event a real (non-dry) import announces (S07 §5.8).
const CATALOGUE_IMPORTED: &str = "catalogue.imported";

/// One catalogue row as the read returns it.
fn catalogue_out(record: &CatalogueRecord) -> CatalogueEntryOut {
    CatalogueEntryOut {
        id: record.id.to_string(),
        kind: record.kind.as_str().to_owned(),
        code: record.code.as_str().to_owned(),
        label: record.label.clone(),
        meta: record.meta.clone(),
        sort: record.sort,
        is_active: record.is_active,
        version: i64::try_from(record.version.get()).unwrap_or(i64::MAX),
    }
}

/// A catalogue store failure as the status the caller can act on.
///
/// Both vocabulary failures carry a `field`, so the `422` names what to fix; only a database
/// refusal is a `500`.
fn catalogue_error(error: CatalogueStoreError, action: &str) -> ActionError {
    match error {
        CatalogueStoreError::UnknownKind(kind) => invalid(
            "kind",
            "invalid_value",
            format!("{kind:?} is not one of the eleven catalogue kinds"),
        ),
        CatalogueStoreError::InvalidCode(error) => {
            invalid("code", "invalid_value", error.to_string())
        }
        // S15 moved the vocabulary to the `capability` aggregate. A caller that still imports a
        // `kind='capability'` row is naming a retired kind: refuse the field, do not re-create it.
        CatalogueStoreError::CapabilityMoved => invalid(
            "kind",
            "invalid_value",
            "capability is an aggregate, not a catalogue kind (S15)".to_owned(),
        ),
        CatalogueStoreError::Database(error) => store_error(error, action),
    }
}

/// Run both importers in one order: `card_type` first, then the catalogue rows.
///
/// The counts are summed over the two tables, so `created + updated + skipped` is the number
/// of rows the run handed the importer, not the number of catalogue entries alone.
async fn import_all(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_types: &[CardTypeImport],
    entries: &[ImportEntry],
) -> Result<ImportCounts, CatalogueStoreError> {
    let types = store::catalogue::import_card_types_tx(&mut *conn, tenant, card_types).await?;
    let rows = store::catalogue::import_entries_tx(&mut *conn, tenant, entries).await?;
    Ok(ImportCounts {
        created: types.created + rows.created,
        updated: types.updated + rows.updated,
        skipped: types.skipped + rows.skipped,
    })
}

/// One page of catalogue rows ordered by `(kind, sort, code)`, every kind when `kind` is
/// absent and only active rows unless `include_inactive` is true.
///
/// **No cursor.** `store::catalogue::list_entries_tx` takes a limit and no keyset, and the
/// action will not mint a cursor it cannot honour. A supplied `cursor` is a `422`; the
/// vocabulary is bounded by the checked-in export, so one page is the whole list at any
/// limit at or above the row count.
#[crate::action(
    name = "catalogues.list",
    version = 1,
    kind = "internal",
    scopes = ["catalogues:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn catalogues_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CatalogueListIn,
) -> Result<Page<CatalogueEntryOut>, ActionError> {
    if input.page.cursor.is_some() {
        return Err(invalid(
            "cursor",
            "invalid_value",
            "catalogues.list returns one bounded page and issues no cursor; narrow by kind \
             instead",
        ));
    }
    let limit = store::actions::clamp_limit(input.page.limit);
    let rows = store::catalogue::list_entries_tx(
        &mut *tx,
        ctx.tenant,
        input.kind.as_deref(),
        input.include_inactive.unwrap_or(false),
        limit,
    )
    .await
    .map_err(|error| catalogue_error(error, "catalogues.list"))?;

    Ok(Page {
        items: rows.iter().map(catalogue_out).collect(),
        next_cursor: None,
    })
}

/// Import the embedded catalogue export into the caller's tenant.
///
/// The export is a build-time constant, so a parse failure is a deployment defect and a `500`;
/// a caller-supplied `kind` is validated through the domain vocabulary before anything runs,
/// and a kind that is not one of the eleven is a `422` naming the field.
///
/// A real import commits with the caller's transaction and takes `catalogue.imported` with it
/// (CONTRACT §7 — the row and the doorbell are one write). A dry run writes inside a savepoint
/// and rolls it back, so it leaves no row, no version bump and no event.
#[crate::action(
    name = "catalogues.import",
    version = 1,
    kind = "internal",
    scopes = ["catalogues:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn catalogues_import(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CatalogueImportIn,
) -> Result<HandlerOutput<CatalogueImportOut>, ActionError> {
    let action = "catalogues.import";
    let dry_run = input.dry_run.unwrap_or(false);
    let export = store::seed::catalogue_export().map_err(|error| {
        tracing::error!(%error, "parse the embedded catalogue export");
        ActionError::internal()
    })?;

    let (entries, card_types) = match input.kind.as_deref() {
        None => (export.entries, export.card_types),
        Some(raw) => {
            CatalogueKind::from_str(raw).map_err(|_| {
                invalid(
                    "kind",
                    "invalid_value",
                    format!("{raw:?} is not one of the eleven catalogue kinds"),
                )
            })?;
            let entries = export
                .entries
                .into_iter()
                .filter(|entry| entry.kind == raw)
                .collect();
            (entries, Vec::new())
        }
    };

    let counts = if dry_run {
        // A savepoint, not a second transaction: the handler is already inside the
        // invocation's transaction, and `Connection::begin` nests as `SAVEPOINT` on an
        // active one. Rolling back restores the pre-run state and leaves the outer
        // transaction usable for the bookkeeping the engine writes after this returns.
        let mut savepoint = tx.begin().await.map_err(|error| {
            tracing::error!(%error, action, "begin the dry-run savepoint");
            ActionError::internal()
        })?;
        let counts = import_all(&mut savepoint, ctx.tenant, &card_types, &entries)
            .await
            .map_err(|error| catalogue_error(error, action))?;
        savepoint.rollback().await.map_err(|error| {
            tracing::error!(%error, action, "roll back the dry-run savepoint");
            ActionError::internal()
        })?;
        counts
    } else {
        import_all(&mut *tx, ctx.tenant, &card_types, &entries)
            .await
            .map_err(|error| catalogue_error(error, action))?
    };

    let out = CatalogueImportOut {
        created: counts.created,
        updated: counts.updated,
        skipped: counts.skipped,
        dry_run,
    };

    if !dry_run {
        append_raw_event(
            &mut *tx,
            &ctx,
            &RawEvent {
                topic: "catalogue".to_owned(),
                board_id: None,
                aggregate_type: "catalogue",
                aggregate_id: ctx.tenant.as_uuid(),
                event_type: CATALOGUE_IMPORTED,
                payload: serde_json::json!({
                    "kind": input.kind,
                    "created": out.created,
                    "updated": out.updated,
                    "skipped": out.skipped,
                }),
                data: serde_json::to_value(&out).unwrap_or(serde_json::Value::Null),
                prev_data: serde_json::Value::Null,
            },
        )
        .await?;
    }

    Ok(HandlerOutput::ok(out))
}
