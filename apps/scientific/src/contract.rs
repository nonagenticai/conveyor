//! The captured contract, and the check that the implementation still matches it.

use std::collections::BTreeSet;

/// The captured OpenAPI 3.1 document (17 paths, 18 operations after the drop in `contract/drop.jq`).
pub const LOCK_JSON: &str = include_str!("../contract/openapi.lock.json");

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

/// `"METHOD path"` for every operation in a document, sorted so every message is deterministic.
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
/// The comparison is on the **operation set**, in three directions: a locked operation that is
/// not registered, a registered operation that is not locked, and (through `served`) one that is
/// registered but the OpenAPI derive does not document. Response *shapes* are proven by the golden
/// replay in `tests/parity.rs`, which is a stronger statement than a schema diff and is the reason
/// this check deliberately stops at operations.
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
        let ops = operations(&document);
        // 17 paths, 18 operations: `/api/formal/proofs/{claim_id}` carries both GET and POST.
        assert_eq!(ops.len(), 18);
        assert!(ops.contains("POST /api/stats/meta-analysis"));
        // The graph surfaces and the dead-upstream operations are not carried.
        assert!(!ops.iter().any(|op| op.contains("/api/topology")));
        assert!(!ops.iter().any(|op| op.contains("/api/explain")));
        assert!(!ops.iter().any(|op| op.contains("/api/verify/cove")));
        assert!(!ops.iter().any(|op| op.contains("/api/verify/integrity")));
        assert!(!ops.iter().any(|op| op.contains("/api/pipeline/trigger")));
        assert!(!ops.iter().any(|op| op.contains("/api/pipeline/webhook")));
        assert!(!ops.iter().any(|op| op.contains("/api/pipeline/batch")));
        assert!(!ops.iter().any(|op| op.contains("/api/sandbox")));
    }

    #[test]
    fn an_extra_registered_route_is_named() {
        let captured = serde_json::json!({"paths": {"/a": {"get": {}}}});
        let served = serde_json::json!({"paths": {"/a": {"get": {}}}});
        let result = check_with(&served, &[("GET", "/a"), ("POST", "/b")], &captured);
        let message = result.expect_err("b is not in the contract").to_string();
        assert!(
            message.contains("not in the contract: POST /b"),
            "{message}"
        );
    }
}
