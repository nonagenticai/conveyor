#![forbid(unsafe_code)]
//! Programme certificates — the release bundle, `docs/SPEC_V2.md` §12.
//!
//! A release is not "every slice is done" said by a person; it is a set of leaf certificates, one
//! per built slice, and a bundle that names exactly the expected set. The bundle is what the
//! release gate carries as `lean_proof` evidence.
//!
//! What the Lean side proves (`checkProgramme` in `DeliveryModel.lean`):
//!
//! * the leaves' **slice names** are exactly the expected set — none missing, none extra;
//! * each leaf names a distinct **certificate digest**, so one certificate cannot stand in for two
//!   slices.
//!
//! What it does not prove: that a leaf's digest names a certificate that is `proven`. That is the
//! store's own check (a certificate's verdict is fixed at write time and its digest covers it), and
//! a reader who wants it re-runs `proofs.reverify` for each leaf. The bundle is a set statement, and
//! pretending otherwise would be the exact kind of over-claim this corpus exists to avoid.
//!
//! The expected set is an **input**, named by the caller and recorded in the evidence document. It
//! is never re-derived at prove time: a slice becoming built is a new certificate and a new bundle,
//! never an edit of an old one.

use serde_json::{Value, json};

use super::render;

/// `sha256(docs/SPEC_V2.md)` — the specification every programme certificate answers.
///
/// Pinned so a certificate says which spec it was proven against; the test
/// `the_pinned_spec_digest_matches_the_spec` is what keeps it true.
pub const SPEC_V2_SHA256: &str = "531edb3a0bfdee74c317746f9f4e3934f216ace6ebb4199d5387364ef833a265";

/// The slices the programme is delivered as, per `docs/SPEC_V2.md` §12.
///
/// Every slice in the table except `T18` (the real V2 UI), which is `proposed`: a proposed slice
/// has no route and no certificate, so including it would make the bundle untrue.
pub const V2_SLICES: [&str; 20] = [
    "T0", "T1", "T2", "T3", "T4", "T5", "T6", "T7", "T8", "T9", "T10", "T11", "T12", "T13", "T14",
    "T15", "T16", "T17", "T19", "T19F",
];

/// One leaf: a built slice and the digest of the certificate that proves it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Leaf {
    /// The slice's name, as §12 spells it (`T0`, `T19F`).
    pub slice: String,
    /// `certificate_sha256` of that slice's certificate.
    pub certificate_sha256: String,
}

impl Leaf {
    /// A leaf from anything string-shaped.
    pub fn new(slice: impl Into<String>, certificate_sha256: impl Into<String>) -> Self {
        Self {
            slice: slice.into(),
            certificate_sha256: certificate_sha256.into(),
        }
    }
}

/// The expected set: every slice the programme is delivered as.
pub fn expected() -> Vec<String> {
    V2_SLICES.iter().map(|slice| (*slice).to_owned()).collect()
}

/// The programme facts: the pinned spec, the expected set and the leaves.
///
/// `expected` is passed in rather than read from [`V2_SLICES`] so a caller that bundles a named
/// subset (one tranche, one release train) says so explicitly, and the facts record what was
/// actually proven instead of what the constant happens to say today.
pub fn facts(expected: &[String], leaves: &[Leaf]) -> Value {
    let leaves: Vec<Value> = leaves
        .iter()
        .map(|leaf| json!([leaf.slice, leaf.certificate_sha256]))
        .collect();
    json!({
        "kind": "programme",
        "spec_sha256": SPEC_V2_SHA256,
        "model_sha256": render::model_sha256(),
        "expected": expected,
        "leaves": leaves,
    })
}
