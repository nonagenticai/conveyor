//! The captured media-discovery contract, and the check that this service still serves it.
//!
//! The capture was taken from the reference `media-discovery` service; see
//! `contract/media/README.md` and `contract/media/manifest.json`.
//!
//! The document is served verbatim from the capture, and what is checked is the part a rewrite can
//! actually drift on: the set of operations. Every operation in the lock must be routed, and no
//! route may exist that the lock does not name. Prose (summaries, descriptions, tags) is not
//! compared: it is copied from the capture, not regenerated from the code, so comparing it to
//! itself would prove nothing.

use std::collections::BTreeSet;

/// The captured OpenAPI 3.1 document (23 operations, 20 paths, 19 schemas).
pub const LOCK_JSON: &str = include_str!("../../contract/media/openapi.lock.json");

/// The HTTP methods a path item may carry, in OpenAPI's own spelling.
const METHODS: [&str; 7] = ["get", "put", "post", "delete", "options", "head", "patch"];

#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("the embedded contract is not valid JSON: {0}")]
    Unreadable(#[from] serde_json::Error),
    #[error("the served routes drifted from the capture: {0}")]
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

/// The declared route set as `"METHOD path"`, with axum's `{param}` spelling normalised to the
/// OpenAPI `{param}` spelling (the two agree today; this keeps the comparison from depending on it).
pub fn route_set(routes: &[(&str, &str)]) -> BTreeSet<String> {
    routes
        .iter()
        .map(|(method, path)| format!("{} {}", method.to_ascii_uppercase(), path.replace(':', "{")))
        .collect()
}

/// Compare the implementation with the capture, reporting both directions of the difference.
pub fn check(routes: &[(&str, &str)]) -> Result<(), LockError> {
    let locked = operations(&lock()?);
    let served = route_set(routes);

    let missing: Vec<_> = locked.difference(&served).cloned().collect();
    let extra: Vec<_> = served.difference(&locked).cloned().collect();
    if missing.is_empty() && extra.is_empty() {
        return Ok(());
    }

    let mut message = String::new();
    if !missing.is_empty() {
        message.push_str(&format!(
            "\n  in the capture, not served ({}):\n    {}",
            missing.len(),
            missing.join("\n    ")
        ));
    }
    if !extra.is_empty() {
        message.push_str(&format!(
            "\n  served, not in the capture ({}):\n    {}",
            extra.len(),
            extra.join("\n    ")
        ));
    }
    Err(LockError::Drifted(message))
}
