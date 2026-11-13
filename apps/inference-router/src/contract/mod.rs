//! The captured contract, and the check that the implementation still matches it.
//!
//! `LOCK_JSON` is the document the **live** service served (`contract/openapi.lock.json`,
//! captured 2026-11-13 from image `sha256:723671a9…`). It is embedded so a binary always carries
//! the contract it checks, and it is **never rewritten from this implementation** — the direction
//! of the check is implementation → capture, which is the only direction that can fail for the
//! reason the check exists.

use std::collections::BTreeSet;

/// The captured OpenAPI 3.1 document (26 paths, 35 schemas).
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
        assert_eq!(document["paths"].as_object().expect("paths").len(), 26);
        assert_eq!(
            document["components"]["schemas"]
                .as_object()
                .expect("schemas")
                .len(),
            35
        );
        let operations = operations(&document);
        // 26 paths, 27 operations: one path carries GET and PUT.
        assert_eq!(operations.len(), 27);
        assert!(operations.contains("POST /api/chat/completions"));
        assert!(operations.contains("PATCH /api/replay/invocation-log/{log_id}"));
    }

    fn tiny_document() -> serde_json::Value {
        serde_json::json!({
            "openapi": "3.1.0",
            "paths": {
                "/health": {"get": {}},
                "/thing": {"get": {}, "post": {}}
            }
        })
    }

    #[test]
    fn a_matching_route_set_passes() {
        let document = tiny_document();
        let routes = [("GET", "/health"), ("GET", "/thing"), ("POST", "/thing")];
        check_with(&document, &routes, &document).expect("the sets agree");
    }

    #[test]
    fn a_missing_registration_is_named() {
        let document = tiny_document();
        let routes = [("GET", "/health"), ("GET", "/thing")];
        let error = check_with(&document, &routes, &document).expect_err("POST /thing is missing");
        assert!(
            error.to_string().contains("not registered: POST /thing"),
            "{error}"
        );
    }

    #[test]
    fn every_direction_of_drift_is_named() {
        // Declared but absent from the capture: a route the service invented.
        let captured = serde_json::json!({"paths": {"/health": {"get": {}}}});
        let served = serde_json::json!({
            "paths": {"/health": {"get": {}}, "/gone": {"delete": {}}}
        });
        let routes = [("GET", "/health"), ("DELETE", "/gone")];
        let error =
            check_with(&served, &routes, &captured).expect_err("DELETE /gone is not captured");
        assert!(
            error
                .to_string()
                .contains("not in the contract: DELETE /gone"),
            "{error}"
        );

        // Registered but missing from the document: a handler without an annotation.
        let document = tiny_document();
        let published = serde_json::json!({"paths": {"/health": {"get": {}}}});
        let routes = [("GET", "/health"), ("GET", "/thing"), ("POST", "/thing")];
        let error =
            check_with(&published, &routes, &document).expect_err("POST /thing is undocumented");
        assert!(
            error
                .to_string()
                .contains("registered but not documented: POST /thing"),
            "{error}"
        );

        // Documented but not registered: an annotation without a handler.
        let error = check_with(&document, &[("GET", "/health")], &document)
            .expect_err("POST /thing is registered nowhere");
        assert!(
            error
                .to_string()
                .contains("documented but not registered: POST /thing"),
            "{error}"
        );
    }

    #[test]
    fn the_current_scaffold_truthfully_reports_the_routes_it_lacks() {
        let served = serde_json::to_value(crate::router::openapi()).expect("serialises");
        let error = check(&served, crate::router::ROUTES)
            .expect_err("the contract is not complete until the migration slices land");
        let message = error.to_string();
        assert!(
            message.contains("not registered: POST /api/chat/completions"),
            "{message}"
        );
        assert!(
            message.contains("not registered: GET /api/models"),
            "{message}"
        );
    }
}
