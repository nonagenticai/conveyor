//! `analyse.brand` — one stored page measured against the caller's brand facts.
//!
//! The facts are the caller's assertions; the page's colours, families and visible text are read
//! from the stored bytes. No row is written and no event is emitted, so a replay returns the same
//! report.

use conveyor_analysis::brand::{BrandFacts, analyse as analyse_page};
use conveyor_analysis::model::{Document, Report};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::documents::actions::fetch_bytes;
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// The action's name, used in error bodies so a refusal names the action that refused.
const ACTION: &str = "analyse.brand";

/// What the caller asks for: the page and the brand facts it must express.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AnalyseBrandIn {
    /// The stored page to read. A document with no stored bytes is a `404`.
    pub document_id: Uuid,
    /// The brand facts the page is measured against.
    #[serde(default)]
    pub brand: BrandFacts,
}

/// Check the page's consistency with the supplied brand facts.
#[crate::action(
    name = "analyse.brand",
    version = 1,
    kind = "internal",
    scopes = ["documents:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn analyse_brand(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AnalyseBrandIn,
) -> Result<HandlerOutput<Report>, ActionError> {
    let loaded = fetch_bytes(&mut *tx, ctx.tenant, input.document_id, ACTION).await?;
    Ok(HandlerOutput::ok(analyse_page(
        &document_from(&loaded.bytes),
        &input.brand,
    )))
}

/// The adapter's decode: lossy on purpose, so a stray non-UTF-8 byte does not refuse the real web.
fn document_from(bytes: &[u8]) -> Document {
    Document::new(String::new(), String::from_utf8_lossy(bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_non_utf8_page_is_analysed_not_refused() {
        let document = document_from(b"<h1>Plumber in \xff Leeds</h1>");
        assert!(document.url.is_empty());
    }

    #[test]
    fn the_same_evidence_twice_is_the_same_report() {
        let brand = BrandFacts {
            palette: vec!["#ff0000".to_owned()],
            typography: vec!["Inter".to_owned()],
            ..BrandFacts::default()
        };
        let page = document_from(b"<div style=\"color:#ff0000;font-family:Inter\">x</div>");
        let first = analyse_page(&page, &brand);
        let second = analyse_page(&page, &brand);
        assert_eq!(
            serde_json::to_string(&first).expect("serialisable"),
            serde_json::to_string(&second).expect("serialisable")
        );
    }
}
