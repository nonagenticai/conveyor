//! `analyse.site` — the deterministic SEO scan over a supplied [`Document`].
//!
//! Same input and same instant, same report: the scan parses the bytes the caller captured, runs
//! a fixed list of checks in a fixed order, deduplicates, and scores. Nothing here reads a clock,
//! the network or the environment — the one check that measures content age takes the instant as
//! an input ([`AnalyseOptions`]), so two runs on two machines agree byte for byte when they are
//! given the same one.

pub mod seo;

use chrono::{DateTime, Utc};

use crate::model::{Document, Report};

/// What the caller supplies besides the document.
///
/// `as_of` exists because `seo-geo-stale-content` asks "how old is this page's content date?", a
/// question with no answer independent of when it is asked. Making it an input keeps the scan a
/// pure function and lets a test pin the answer.
#[derive(Clone, Copy, Debug)]
pub struct AnalyseOptions {
    pub as_of: DateTime<Utc>,
}

impl AnalyseOptions {
    pub fn at(as_of: DateTime<Utc>) -> Self {
        Self { as_of }
    }
}

/// Analyse one page.
///
/// The source's `/api/seo/audit` did exactly these three steps on its default path — scan,
/// `deduplicate_findings`, `compute_seo_audit_score` — and this is that path without the fetch,
/// the live probes and the optional LLM annotations (all of which are the caller's, not the
/// library's).
pub fn analyse(document: &Document, options: &AnalyseOptions) -> Report {
    let findings = crate::rubric::dedupe(seo::scan_audit(document, &options.as_of));
    let scorecard = seo::scorecard(&findings);
    Report {
        url: document.url.clone(),
        findings,
        scorecard,
    }
}
