//! `analyse.site` — the deterministic SEO report for one stored document.

use conveyor_analysis::model::{Document, Report};
use conveyor_analysis::site::analyse;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::documents::actions::fetch_bytes;
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// The action's name, used in error bodies so a refusal names the action that refused.
const ACTION: &str = "analyse.site";

/// What the caller asks for: one document, and the URL its bytes were captured from.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AnalyseSiteIn {
    /// The document whose bytes are the page to analyse. It must be an active document with
    /// stored bytes — a report or a tombstone has none, and both are a `404`.
    pub document_id: Uuid,
    /// The URL the page was captured from, as the caller knows it.
    ///
    /// Empty is allowed and means "unknown": the checks that need a URL (internal links with
    /// `rel="nofollow"`, a canonical protocol mismatch, URL structure) then stand down rather
    /// than invent findings from an empty string.
    #[serde(default)]
    pub url: String,
}

/// Analyse a stored page.
///
/// A read, not a write: no row is created, no event is emitted, and the report is a pure function
/// of the document's bytes plus the URL. Two calls with the same bytes return the same report.
#[crate::action(
    name = "analyse.site",
    version = 1,
    kind = "internal",
    scopes = ["documents:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn analyse_site(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AnalyseSiteIn,
) -> Result<HandlerOutput<Report>, ActionError> {
    let loaded = fetch_bytes(&mut *tx, ctx.tenant, input.document_id, ACTION).await?;
    Ok(HandlerOutput::ok(analyse_bytes(&loaded.bytes, &input.url)))
}

/// The adapter's whole job, as a function: bytes in, report out.
///
/// Decoding is lossy on purpose. A page with one stray non-UTF-8 byte is a page to analyse, not a
/// `500`: the alternative is an operation that refuses the real web, whose encodings are not
/// always what their headers claim.
fn analyse_bytes(bytes: &[u8], url: &str) -> Report {
    let html = String::from_utf8_lossy(bytes);
    analyse(&Document::new(url.to_owned(), html.into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_adapter_hands_the_bytes_and_url_to_the_library() {
        let report = analyse_bytes(
            b"<title>Emergency plumber in Leeds today</title><h1>Emergency plumber in Leeds</h1>",
            "https://example.test/",
        );
        assert_eq!(report.url, "https://example.test/");
        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.check_id() == "seo-missing-h1")
        );
        assert!(report.scorecard.overall_score > 0.0);
    }

    #[test]
    fn a_non_utf8_page_is_analysed_not_refused() {
        let report = analyse_bytes(
            b"<title>Plumber in \xff Leeds today</title><h1>Plumber in Leeds</h1>",
            "",
        );
        assert!(report.url.is_empty());
    }
}
