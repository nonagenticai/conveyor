#![forbid(unsafe_code)]
//! `card_types.list` — the tenant's card-type catalogue, read whole.
//!
//! The console's intake picker and its per-card expected-outputs column need each type's
//! `code`, `label` and `definition.outputs`. `catalogues.list` cannot serve that read: the
//! eleven-namespace vocabulary deliberately excludes `card_type`
//! (`conveyor_domain::catalogue::CatalogueKind`), so a kind-scoped request is a `422` and an
//! unscoped one lists `catalogue_entry` rows only. `processes.*` labels a row with its type
//! code but carries no definition. Without this read a client either hardcodes a type or
//! renders no expected outputs at all.
//!
//! The list is bounded by the tenant's own seeded catalogue and issues no cursor, exactly
//! like `catalogues.list`: the store takes no keyset, and a cursor the server cannot honour
//! is a `422` rather than a page that silently repeats.
//!
//! Reads nothing else and writes nothing; `effect = read`.

use sqlx::PgConnection;

use conveyor_store as store;

use crate::board::views::{CardTypeOut, CardTypesListIn};
use crate::invoke::{ActionError, HandlerCtx};
use crate::views::Page;

/// One whole page of the tenant's card types, active only unless `include_inactive` is set.
#[crate::action(
    name = "card_types.list",
    version = 1,
    kind = "internal",
    scopes = ["cards:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn card_types_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CardTypesListIn,
) -> Result<Page<CardTypeOut>, ActionError> {
    if input.page.cursor.is_some() {
        return Err(crate::board::invalid(
            "cursor",
            "invalid_value",
            "card_types.list returns one whole page and issues no cursor; a cursor is a \
             client error, not a page boundary",
        ));
    }
    let rows = store::board::list_card_types_tx(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| {
            tracing::error!(%error, "card_types.list: read the tenant's card types");
            ActionError::internal()
        })?;
    Ok(Page {
        items: rows
            .into_iter()
            .filter(|card_type| input.include_inactive || card_type.is_active)
            .map(|card_type| CardTypeOut {
                id: card_type.id.to_string(),
                code: card_type.code.to_string(),
                label: card_type.label,
                is_active: card_type.is_active,
                definition: card_type.definition,
            })
            .collect(),
        next_cursor: None,
    })
}
