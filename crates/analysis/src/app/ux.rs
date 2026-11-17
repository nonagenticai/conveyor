//! The deterministic guard over LLM-authored UX claims (the source's `ux_claims.py`).
//!
//! Placeholder until the port lands; [`guard`] returns no finding it cannot verify.

use crate::model::{Document, Finding, Severity};

/// One UX claim as the reviewer returned it.
#[derive(Clone, Debug, Default)]
pub struct UxClaim {
    pub kind: String,
    pub severity: Severity,
    pub title: String,
    pub description: String,
    pub recommendation: String,
    pub claim_type: String,
    pub scope: String,
    pub evidence: Option<String>,
    pub selector: Option<String>,
}

/// Apply the source's guards to the reviewer's claims.
pub fn guard(_document: &Document, _claims: &[UxClaim]) -> Vec<Finding> {
    Vec::new()
}
