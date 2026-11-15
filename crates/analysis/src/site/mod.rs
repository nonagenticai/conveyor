//! `analyse.site` — the deterministic SEO scan over a supplied [`Document`].
//!
//! Same input, same report: the scan parses the bytes the caller captured, runs a fixed list of
//! checks in a fixed order, deduplicates, and scores. Nothing here reads a clock, the network or
//! the environment, so two runs on two machines agree byte for byte.

pub mod seo;

use crate::model::{Document, Report};

/// Analyse one page.
///
/// The source's `/api/seo/audit` did exactly these three steps on its default path — scan,
/// `deduplicate_findings`, `compute_seo_audit_score` — and this is that path without the fetch,
/// the live probes and the optional LLM annotations (all of which are the caller's, not the
/// library's).
pub fn analyse(document: &Document) -> Report {
    let findings = crate::rubric::dedupe(seo::scan(document));
    let scorecard = seo::scorecard(&findings);
    Report {
        url: document.url.clone(),
        findings,
        scorecard,
    }
}
