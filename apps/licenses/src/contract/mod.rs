//! The captured contract, and the check that the implementation still matches it.
//!
//! `LOCK_JSON` is the document the **live** service served (`contract/openapi.lock.json`, captured
//! 2026-11-14 from image `sha256:13093d39…`). It is embedded so a binary always carries the
//! contract it checks, and it is **never rewritten from this implementation** — the direction of
//! the check is implementation → capture, which is the only direction that can fail for the reason
//! the check exists.

use std::collections::BTreeSet;

pub mod normalize;

/// The captured OpenAPI 3.1 document (43 paths, 60 operations, 57 schemas).
pub const LOCK_JSON: &str = include_str!("../../contract/openapi.lock.json");

/// The HTTP methods a path item may carry, in OpenAPI's own spelling.
const METHODS: [&str; 7] = ["get", "put", "post", "delete", "options", "head", "patch"];

#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("the embedded contract is not valid JSON: {0}")]
    Unreadable(#[from] serde_json::Error),
    #[error("the served contract drifted from the capture: {0}")]
    Drifted(String),
}

/// Parse the captured document.
pub fn lock() -> Result<serde_json::Value, LockError> {
    Ok(serde_json::from_str(LOCK_JSON)?)
}

/// `"METHOD path"` for every operation in a document.
///
/// Sorted, so the comparison and every message are deterministic.
pub fn operations(document: &serde_json::Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Some(paths) = document.get("paths").and_then(serde_json::Value::as_object) else {
        return out;
    };
    for (path, item) in paths {
        let Some(item) = item.as_object() else {
            continue;
        };
        for method in METHODS {
            if item.contains_key(method) {
                out.insert(format!("{} {path}", method.to_ascii_uppercase()));
            }
        }
    }
    out
}

/// The declared route set as `"METHOD path"`.
pub fn route_set(routes: &[(&str, &str)]) -> BTreeSet<String> {
    routes
        .iter()
        .map(|(method, path)| format!("{} {path}", method.to_ascii_uppercase()))
        .collect()
}

/// Compare the implementation with the capture.
///
/// Three sets must agree: the routes `ROUTES` declares, the operations the served document
/// describes, and the operations the live service served. Any difference is named; a check that
/// only said "drifted" would send the reader back to the JSON by hand.
pub fn check(served: &serde_json::Value, routes: &[(&str, &str)]) -> Result<(), LockError> {
    check_with(served, routes, &lock()?)
}

/// `check` with the capture supplied, so the comparison itself is testable.
pub fn check_with(
    served: &serde_json::Value,
    routes: &[(&str, &str)],
    captured: &serde_json::Value,
) -> Result<(), LockError> {
    let locked = operations(captured);
    let declared = route_set(routes);
    let published = operations(served);

    let mut problems = Vec::new();
    for missing in locked.difference(&declared) {
        problems.push(format!("not registered: {missing}"));
    }
    for extra in declared.difference(&locked) {
        problems.push(format!("not in the contract: {extra}"));
    }
    for missing in declared.difference(&published) {
        problems.push(format!("registered but not documented: {missing}"));
    }
    for extra in published.difference(&declared) {
        problems.push(format!("documented but not registered: {extra}"));
    }

    // The document itself, under the projection documented in `normalize`.
    let expected = normalize::normalize(captured);
    let actual = normalize::normalize(served);
    if expected != actual {
        let diffs = normalize::diff(&expected, &actual);
        problems.push(format!(
            "document drift ({} difference(s)): {}",
            diffs.len(),
            diffs.join(" | ")
        ));
    }

    if problems.is_empty() {
        Ok(())
    } else {
        Err(LockError::Drifted(problems.join("; ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_capture_parses_and_names_its_operations() {
        let document = lock().expect("the embedded capture parses");
        assert_eq!(document["openapi"], "3.1.0");
        assert_eq!(document["paths"].as_object().expect("paths").len(), 43);
        assert_eq!(
            document["components"]["schemas"]
                .as_object()
                .expect("schemas")
                .len(),
            57
        );
        let operations = operations(&document);
        assert_eq!(operations.len(), 60);
        assert!(operations.contains("GET /licenses/active"));
        assert!(operations.contains("POST /license/introspect"));
        assert!(operations.contains("POST /internal/tenants/{tenant_id}/bundle-token"));
    }

    fn served() -> serde_json::Value {
        serde_json::to_value(crate::router::openapi()).expect("serialises")
    }

    /// Slice 1 of the port is the contract, the schema and the health surface. The check must
    /// name every operation still missing, and must not report the routes that are served — a
    /// positive assertion on the gap is what replaces a silently skipped test.
    #[test]
    fn the_lock_check_names_the_unported_operations() {
        let error = check(&served(), crate::router::ROUTES)
            .expect_err("the port is incomplete by construction in slice 1");
        let message = error.to_string();
        assert!(
            message.contains("not registered: GET /licenses/active"),
            "{message}"
        );
        assert!(
            message.contains("not registered: POST /license/introspect"),
            "{message}"
        );
        assert!(
            !message.contains("not registered: GET /health"),
            "{message}"
        );
    }

    #[test]
    fn a_dropped_schema_property_fails() {
        let mut drifted = served();
        drifted["components"]["schemas"]["HealthResponse"]["properties"]
            .as_object_mut()
            .expect("properties")
            .remove("database");
        let expected = normalize::normalize(&lock().expect("capture"))["components"].clone();
        let actual = normalize::normalize(&drifted)["components"].clone();
        let diffs = normalize::diff(&expected, &actual);
        assert!(
            diffs
                .iter()
                .any(|row| row.contains("HealthResponse/properties/database")),
            "{diffs:?}"
        );
    }

    #[test]
    fn a_changed_property_type_fails() {
        let mut drifted = served();
        drifted["components"]["schemas"]["HealthResponse"]["properties"]["status"]["type"] =
            serde_json::json!("integer");
        let expected = normalize::normalize(&lock().expect("capture"))["components"].clone();
        let actual = normalize::normalize(&drifted)["components"].clone();
        let diffs = normalize::diff(&expected, &actual);
        assert!(
            diffs
                .iter()
                .any(|row| row.contains("HealthResponse/properties/status/type")),
            "{diffs:?}"
        );
    }
}
