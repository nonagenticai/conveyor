//! The captured content-service document, parsed, with its digest.
//!
//! The bytes are embedded at compile time, so a change to the committed file is a change to what
//! this module reports — `cargo` tracks the `include_str!` input. The document is the *reference*
//! service's output, captured by `contract/capture.sh` from the pinned image; it is never
//! regenerated from this workspace.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use serde_json::Value;
use sha2::{Digest, Sha256};

/// The committed capture, verbatim.
pub const DOCUMENT_JSON: &str =
    include_str!("../../../docs/evidence/produce-openapi-2026-11-16.json");

const HTTP_METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "options", "head", "trace"];

/// The parsed document. Parsing once is enough: the input is a constant.
pub fn document() -> &'static Value {
    static DOCUMENT: OnceLock<Value> = OnceLock::new();
    DOCUMENT.get_or_init(|| {
        serde_json::from_str(DOCUMENT_JSON).expect("the committed capture is valid JSON")
    })
}

/// SHA-256 of the committed bytes, lowercase hex.
///
/// This is the digest the provenance table records. When the committed document changes at all —
/// whitespace included — this value changes and the parity test fails.
pub fn sha256_hex() -> String {
    Sha256::digest(DOCUMENT_JSON.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Every operation the document defines, as `METHOD /path`.
///
/// The set is derived from the document, never listed by hand: a hand list can drift from the
/// capture, and drift is exactly what the parity test exists to catch.
pub fn operations() -> BTreeSet<String> {
    let mut operations = BTreeSet::new();
    let paths = document()
        .get("paths")
        .and_then(Value::as_object)
        .expect("the committed capture has a paths object");

    for (path, item) in paths {
        let Some(item) = item.as_object() else {
            continue;
        };
        for method in HTTP_METHODS {
            if item.contains_key(*method) {
                operations.insert(format!("{} {path}", method.to_ascii_uppercase()));
            }
        }
    }

    operations
}
