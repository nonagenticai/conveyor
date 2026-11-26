//! `analyse.content` — one report per stored page, joining the CRO/marketing scan with the
//! heading, source and filler checks.
//!
//! The pages are loaded through the existing document path and decoded lossily. A read: no row is
//! written, no event is emitted, and the reports are a pure function of the stored bytes.

use conveyor_analysis::content::analyse_quality;
use conveyor_analysis::model::{Document, Report};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::documents::actions::fetch_bytes;
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::{FieldError, HandlerOutput};

/// The action's name, used in error bodies so a refusal names the action that refused.
const ACTION: &str = "analyse.content";

/// What the caller asks for: the pages to analyse, in report order.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AnalyseContentIn {
    /// The stored pages to read. An empty list is a `422`; a document with no stored bytes is a
    /// `404`.
    #[serde(default)]
    pub document_ids: Vec<Uuid>,
}

/// Analyse each page for content-shaped quality.
#[crate::action(
    name = "analyse.content",
    version = 1,
    kind = "internal",
    scopes = ["documents:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn analyse_content(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AnalyseContentIn,
) -> Result<HandlerOutput<Vec<Report>>, ActionError> {
    if input.document_ids.is_empty() {
        return Err(ActionError::invalid_input(vec![FieldError {
            field: "document_ids".to_owned(),
            code: "invalid_value".to_owned(),
            detail: "must name at least one document".to_owned(),
        }]));
    }
    let mut pages = Vec::with_capacity(input.document_ids.len());
    for id in input.document_ids {
        let loaded = fetch_bytes(&mut *tx, ctx.tenant, id, ACTION).await?;
        pages.push(loaded.bytes);
    }
    Ok(HandlerOutput::ok(reports_from(pages)))
}

/// The adapter's whole job, as a function: bytes in, one report per page out.
fn reports_from(pages: Vec<Vec<u8>>) -> Vec<Report> {
    pages
        .iter()
        .map(|bytes| {
            analyse_quality(&Document::new(
                String::new(),
                String::from_utf8_lossy(bytes).into_owned(),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_pages_produce_two_reports() {
        let reports = reports_from(vec![b"<h1>One</h1>".to_vec(), b"<h1>Two</h1>".to_vec()]);
        assert_eq!(reports.len(), 2);
        assert!(reports.iter().all(|report| report.url.is_empty()));
    }

    #[test]
    fn a_non_utf8_page_is_analysed_not_refused() {
        let reports = reports_from(vec![b"<h1>Plumber in \xff Leeds</h1>".to_vec()]);
        assert_eq!(reports.len(), 1);
    }
}
