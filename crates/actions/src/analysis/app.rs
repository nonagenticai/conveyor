//! `analyse.app` — the §10.5 destination coverage report over supplied routes and stored pages.
//!
//! The route table and the pages are the caller's evidence: the action does not probe the
//! application. It loads the named documents through the existing document path, decodes them
//! lossily and hands both inputs to the pure library. A read: no row is written, no event is
//! emitted, and a replay of the same evidence returns the same report.

use conveyor_analysis::app::{RouteEvidence, coverage};
use conveyor_analysis::model::{Document, Report};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::documents::actions::fetch_bytes;
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// The action's name, used in error bodies so a refusal names the action that refused.
const ACTION: &str = "analyse.app";

/// What the caller asks for: the application's route table and the captured pages.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AnalyseAppIn {
    /// Stored pages to read. Empty means the route table alone is measured; a document with no
    /// stored bytes is a `404`.
    #[serde(default)]
    pub document_ids: Vec<Uuid>,
    /// The route table the caller observed in the application.
    #[serde(default)]
    pub routes: Vec<RouteEvidence>,
}

/// Measure the §10.5 destinations against the supplied route table and pages.
#[crate::action(
    name = "analyse.app",
    version = 1,
    kind = "internal",
    scopes = ["documents:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn analyse_app(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AnalyseAppIn,
) -> Result<HandlerOutput<Report>, ActionError> {
    let mut documents = Vec::with_capacity(input.document_ids.len());
    for id in input.document_ids {
        let loaded = fetch_bytes(&mut *tx, ctx.tenant, id, ACTION).await?;
        documents.push(document_from(&loaded.bytes));
    }
    Ok(HandlerOutput::ok(coverage(&input.routes, &documents)))
}

/// The adapter's decode: lossy on purpose, so a stray non-UTF-8 byte does not refuse the real web.
fn document_from(bytes: &[u8]) -> Document {
    Document::new(String::new(), String::from_utf8_lossy(bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use conveyor_analysis::app::DESTINATIONS;

    #[test]
    fn a_non_utf8_page_is_analysed_not_refused() {
        let document = document_from(b"<h1>Plumber in \xff Leeds</h1>");
        assert!(document.url.is_empty());
        assert!(document.html.contains("Plumber in"));
    }

    #[test]
    fn the_same_evidence_twice_is_the_same_report() {
        let routes: Vec<RouteEvidence> = DESTINATIONS
            .iter()
            .map(|(_, path)| RouteEvidence {
                path: (*path).to_owned(),
                served: true,
            })
            .collect();
        let page = document_from(b"<a href=\"/tools\">tools</a>");
        let first = coverage(&routes, std::slice::from_ref(&page));
        let second = coverage(&routes, &[page]);
        assert_eq!(
            serde_json::to_string(&first).expect("serialisable"),
            serde_json::to_string(&second).expect("serialisable")
        );
    }
}
