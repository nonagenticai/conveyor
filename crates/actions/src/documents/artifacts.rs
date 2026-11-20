//! `artifacts.list` — the artifact catalogue **is** the conveyor's document store.
//!
//! V1 listed artifacts from a separate Python `artifact_service`. SPEC_V2 §11.3 says the V1
//! externals migrate onto the conveyor's own surface and "do not run beside it as a second
//! path"; §11.10 (D57) rules that the conveyor reads and writes its own bytes directly; §15.2
//! #17 names this action as the one T13 left open. This module is that decision executed: the
//! same rows `documents.list` reads, under the same tenant scoping, with no network call.
//!
//! The action keeps its V1 name so existing callers keep working, and returns the shared
//! [`DocumentOut`] shape so there is exactly one catalogue model in the workspace.

use conveyor_store::document::{DocumentFilter, DocumentPage};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use super::actions::{DocumentOut, documents_out_with_names};
use super::{document_error, invalid};
use crate::invoke::{ActionError, HandlerCtx};
use crate::views::{Page, PageIn};

/// The catalogue filter: the project or card the caller is looking at, and a page.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactListIn {
    /// Only this project's rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    /// Only this card's rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_id: Option<Uuid>,
    /// Only this kind (`document | report | export | attachment`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The cursor and page size.
    #[serde(flatten)]
    pub page: PageIn,
}

/// List this tenant's artifacts, newest first — a read of the conveyor's own catalogue.
#[crate::action(
    name = "artifacts.list",
    version = 1,
    kind = "internal",
    scopes = ["integrations:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn artifacts_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ArtifactListIn,
) -> Result<Page<DocumentOut>, ActionError> {
    if let Some(kind) = input.kind.as_deref()
        && !matches!(kind, "document" | "report" | "export" | "attachment")
    {
        return Err(invalid(
            "kind",
            "invalid_value",
            "must be document, report, export or attachment",
        ));
    }
    let page = DocumentPage {
        cursor: input.page.cursor,
        limit: i64::from(input.page.limit.unwrap_or(50).min(200)),
    };
    let filter = DocumentFilter {
        project_id: input.project_id,
        card_id: input.card_id,
        parent_document_id: None,
        kind: input.kind,
        publishable: None,
        category: None,
        unit: None,
        publish_requested: None,
    };
    let (rows, next_cursor) =
        conveyor_store::document::list_documents_tx(&mut *tx, ctx.tenant, &filter, &page)
            .await
            .map_err(|error| document_error(error, "artifacts.list"))?;
    Ok(Page {
        items: documents_out_with_names(&mut *tx, rows, "artifacts.list").await?,
        next_cursor,
    })
}
