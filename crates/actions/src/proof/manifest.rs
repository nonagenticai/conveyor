#![forbid(unsafe_code)]
//! Package certificates — a delivered work product proves facts about its own manifest.
//!
//! A package is the unit a delivery job hands over: migrations, routes, and the checks that were
//! run on it. "It is complete" is a claim; a certificate makes it a statement Lean can refute.
//!
//! Three relations, each decided by `checkPackage` in `DeliveryModel.lean`:
//!
//! | Fact | Relation |
//! |---|---|
//! | migrations | the embedded set equals the ledger's applied set |
//! | routes | every route the package serves is in the served OpenAPI document |
//! | checks | every check the caller names is present **and** passed |
//!
//! Rust only canonicalises. It does not decide any of the three: if it did, the certificate would
//! be a screenshot of Rust's opinion rather than a statement the prover checked.

use serde_json::{Value, json};

use super::render;

/// The manifest a package certificate is about.
///
/// Every field is a fact the caller gathered; the certificate's value is that all of them are in
/// the digest, so none can be changed afterwards without the certificate refusing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackageManifest {
    /// Migration versions the package ships, as the embedded migrator names them (`0076`).
    pub migrations: Vec<String>,
    /// Migration versions the database actually applied.
    pub ledger: Vec<String>,
    /// Routes the package serves (`POST /api/actions/proofs.record`).
    pub routes: Vec<String>,
    /// Every route the served document defines, from `served-spec.lock.json`.
    pub served: Vec<String>,
    /// The checks the package claims, and whether each passed.
    pub checks: Vec<(String, bool)>,
}

impl PackageManifest {
    /// The canonical facts: what `render` turns into the certificate's Lean source.
    pub fn facts(&self) -> Value {
        json!({
            "kind": "package",
            "spec_sha256": super::programme::SPEC_V2_SHA256,
            "model_sha256": render::model_sha256(),
            "migrations": self.migrations,
            "ledger": self.ledger,
            "routes": self.routes,
            "served": self.served,
            "checks": self.checks,
        })
    }
}

/// The migration versions embedded in this binary, as `0076`-style strings.
///
/// The other half of the pair is the database's `_sqlx_migrations`; a package whose binary carries
/// a migration the database never applied is not a complete package, and the certificate says so.
pub fn embedded_migrations() -> Vec<String> {
    conveyor_store::migrate::MIGRATOR
        .migrations
        .iter()
        .map(|migration| format!("{:04}", migration.version))
        .collect()
}

/// The operations a served OpenAPI document defines, as `METHOD /path` strings.
///
/// This reuses the lock reader rather than re-deriving the document's shape: `lock::operations` is
/// what `--check-lock` compares against, so a certificate and CI cannot disagree.
pub fn served_operations(document: &Value) -> Vec<String> {
    crate::lock::operations(document).into_keys().collect()
}
