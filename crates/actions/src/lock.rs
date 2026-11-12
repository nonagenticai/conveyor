#![forbid(unsafe_code)]
//! `served-spec.lock.json`: one sha256 per `$ref`-resolved operation (S04 §5.4).
//!
//! Written by `--print-openapi --write-lock`, checked by `--print-openapi --check-lock`.
//! A lock that only records path strings would miss a renamed *field*; hashing the whole
//! resolved operation makes the difference a breaking contract change, and the failure
//! names the operation.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The lock's on-disk shape.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lock {
    /// The format version (bumped only by a contract change).
    pub version: u32,
    /// `"<METHOD> <path>"` → sha256 of the `$ref`-resolved operation.
    pub operations: BTreeMap<String, String>,
}

/// The HTTP methods a path item may hold.
const METHODS: [&str; 7] = ["get", "put", "post", "delete", "options", "head", "patch"];

/// The default lock file name, relative to the workspace root.
pub const DEFAULT_PATH: &str = "served-spec.lock.json";

/// Why the served document disagrees with the lock.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LockDiff {
    /// No lock file exists.
    #[error("{0} does not exist: run --print-openapi --write-lock and commit the result")]
    Missing(PathBuf),
    /// The lock file cannot be read or parsed.
    #[error("{0}: {1}")]
    Unreadable(PathBuf, String),
    /// Operations were added, removed or changed.
    #[error("the served OpenAPI drifted from the lock: {0}")]
    Drifted(String),
}

/// Build the lock from a serialized OpenAPI document.
pub fn build(document: &Value) -> Lock {
    Lock {
        version: 1,
        operations: operations(document),
    }
}

/// `"<METHOD> <path>"` → sha256 of the resolved operation, sorted by key.
pub fn operations(document: &Value) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(paths) = document.get("paths").and_then(Value::as_object) else {
        return out;
    };
    for (path, item) in paths {
        let Some(item) = item.as_object() else {
            continue;
        };
        for method in METHODS {
            let Some(operation) = item.get(method) else {
                continue;
            };
            let resolved = resolve(operation, document, 0);
            let bytes = serde_json::to_vec(&resolved).unwrap_or_default();
            out.insert(
                format!("{} {}", method.to_ascii_uppercase(), path),
                crate::spec::sha256_hex(&bytes),
            );
        }
    }
    out
}

/// Replace `$ref` with its target, recursively. Only same-document JSON pointers are
/// resolved; an external reference is left as written.
fn resolve(value: &Value, root: &Value, depth: usize) -> Value {
    if depth > 32 {
        return value.clone();
    }
    match value {
        Value::Object(map) => {
            if let Some(reference) = map.get("$ref").and_then(Value::as_str)
                && let Some(target) = pointer(root, reference)
            {
                return resolve(target, root, depth + 1);
            }
            Value::Object(
                map.iter()
                    .map(|(key, nested)| (key.clone(), resolve(nested, root, depth + 1)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| resolve(item, root, depth + 1))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn pointer<'a>(root: &'a Value, reference: &str) -> Option<&'a Value> {
    let path = reference.strip_prefix("#/")?;
    let mut current = root;
    for segment in path.split('/') {
        let segment = segment.replace("~1", "/").replace("~0", "~");
        current = current.get(&segment)?;
    }
    Some(current)
}

/// Compare a document against a lock, returning a diff that names every operation.
pub fn diff(document: &Value, lock: &Lock) -> Result<(), LockDiff> {
    let current = operations(document);
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();
    for (key, hash) in &current {
        match lock.operations.get(key) {
            None => added.push(key.clone()),
            Some(previous) if previous != hash => changed.push(key.clone()),
            Some(_) => {}
        }
    }
    for key in lock.operations.keys() {
        if !current.contains_key(key) {
            removed.push(key.clone());
        }
    }
    if added.is_empty() && removed.is_empty() && changed.is_empty() {
        return Ok(());
    }
    let mut parts = Vec::new();
    if !changed.is_empty() {
        parts.push(format!("changed: {}", changed.join(", ")));
    }
    if !added.is_empty() {
        parts.push(format!("added: {}", added.join(", ")));
    }
    if !removed.is_empty() {
        parts.push(format!("removed: {}", removed.join(", ")));
    }
    Err(LockDiff::Drifted(parts.join("; ")))
}

/// Read a lock file.
pub fn read(path: &Path) -> Result<Lock, LockDiff> {
    let bytes = std::fs::read(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            LockDiff::Missing(path.to_path_buf())
        } else {
            LockDiff::Unreadable(path.to_path_buf(), error.to_string())
        }
    })?;
    serde_json::from_slice(&bytes)
        .map_err(|error| LockDiff::Unreadable(path.to_path_buf(), error.to_string()))
}

/// Write a lock file (pretty, trailing newline — a stable diff).
pub fn write(path: &Path, lock: &Lock) -> std::io::Result<()> {
    let mut bytes = serde_json::to_vec_pretty(lock).map_err(std::io::Error::other)?;
    bytes.push(b'\n');
    std::fs::write(path, bytes)
}

/// Check a document against the lock at `path`.
pub fn check(document: &Value, path: &Path) -> Result<Lock, LockDiff> {
    let lock = read(path)?;
    diff(document, &lock)?;
    Ok(lock)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn document(description: &str) -> Value {
        json!({
            "openapi": "3.1.0",
            "info": {"title": "t", "version": "1"},
            "paths": {
                "/a": {"post": {"operationId": "a", "description": description,
                                "responses": {"200": {"description": "ok"}}}},
                "/b": {"get": {"operationId": "b", "responses": {"200": {"description": "ok"}}}}
            }
        })
    }

    #[test]
    fn a_field_rename_changes_the_operation_hash_and_names_it() {
        let lock = build(&document("one"));
        assert_eq!(diff(&document("one"), &lock), Ok(()));
        let error = diff(&document("two"), &lock).unwrap_err();
        assert!(
            error.to_string().contains("POST /a"),
            "the diff must name the operation: {error}"
        );
    }

    #[test]
    fn a_removed_operation_is_reported() {
        let lock = build(&document("one"));
        let mut less = document("one");
        less["paths"].as_object_mut().unwrap().remove("/b");
        let error = diff(&less, &lock).unwrap_err();
        assert!(error.to_string().contains("removed: GET /b"), "{error}");
    }

    #[test]
    fn refs_are_resolved_before_hashing() {
        let mut with_ref = json!({
            "openapi": "3.1.0",
            "info": {"title": "t", "version": "1"},
            "paths": {"/a": {"post": {"requestBody": {"$ref": "#/components/requestBodies/R"}}}},
            "components": {"requestBodies": {"R": {"description": "one"}}}
        });
        let lock = build(&with_ref);
        with_ref["components"]["requestBodies"]["R"]["description"] = json!("two");
        assert!(diff(&with_ref, &lock).is_err(), "the resolved body changed");
    }
}
