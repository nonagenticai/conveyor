//! `analyse.report` — each claim in a report checked against the page it cites.
//!
//! The claim table is the caller's; the page is loaded through the existing document path and
//! decoded lossily. A read: no row is written, no event is emitted, and a fully referenced claim
//! table produces a clean report.

use conveyor_analysis::model::{Document, Report};
use conveyor_analysis::report::{ClaimRef, analyse as analyse_claims};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::documents::actions::fetch_bytes;
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// The action's name, used in error bodies so a refusal names the action that refused.
const ACTION: &str = "analyse.report";

/// What the caller asks for: the page and the claims it makes.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AnalyseReportIn {
    /// The stored page the claims cite. A document with no stored bytes is a `404`.
    pub document_id: Uuid,
    /// One row per claim; a reference that does not occur in the page is a failure.
    #[serde(default)]
    pub claims: Vec<ClaimRef>,
}

/// Check every claim against the page's visible text.
#[crate::action(
    name = "analyse.report",
    version = 1,
    kind = "internal",
    scopes = ["documents:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn analyse_report(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AnalyseReportIn,
) -> Result<HandlerOutput<Report>, ActionError> {
    let loaded = fetch_bytes(&mut *tx, ctx.tenant, input.document_id, ACTION).await?;
    Ok(HandlerOutput::ok(analyse_claims(
        &document_from(&loaded.bytes),
        &input.claims,
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
        let document = document_from(b"<p>Plumber in \xff Leeds</p>");
        assert!(document.url.is_empty());
    }

    #[test]
    fn the_same_evidence_twice_is_the_same_report() {
        let page = document_from(b"<p>The plumber covers Leeds.</p>");
        let claims = [ClaimRef {
            claim: "the page names Leeds".to_owned(),
            reference: "plumber covers Leeds".to_owned(),
        }];
        let first = analyse_claims(&page, &claims);
        let second = analyse_claims(&page, &claims);
        assert_eq!(
            serde_json::to_string(&first).expect("serialisable"),
            serde_json::to_string(&second).expect("serialisable")
        );
        assert!(first.findings.is_empty(), "{:?}", first.findings);
    }
}
