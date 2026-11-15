//! The captured media-group contract, and the check that a binary still serves it.
//!
//! Eight reference services are absorbed here (ADR 0006 §Decision, the `apps/media` row). Each one
//! has its own lock, taken from its own running image; a binary serves the union of the locks of
//! the services assigned to it. What is checked is the part a rewrite can actually drift on: **the
//! set of operations**. Prose (summaries, descriptions, tags) is copied from the capture, so
//! comparing it to itself would prove nothing.
//!
//! The direction is load-bearing — a lock regenerated from the rewrite cannot fail for the reason
//! the check exists.

use std::collections::BTreeSet;

use thiserror::Error;

/// One captured reference: the service's own OpenAPI document, embedded verbatim.
#[derive(Debug, Clone, Copy)]
pub struct Lock {
    /// The reference's name, as the contract directory spells it.
    pub service: &'static str,
    /// The captured OpenAPI 3.1 document.
    pub json: &'static str,
}

/// The HTTP methods a path item may carry, in OpenAPI's own spelling.
const METHODS: [&str; 7] = ["get", "put", "post", "delete", "options", "head", "patch"];

#[derive(Debug, Error)]
pub enum LockError {
    #[error("the embedded {service} contract is not valid JSON: {source}")]
    Unreadable {
        service: &'static str,
        #[source]
        source: serde_json::Error,
    },
    /// A binary with no embedded capture cannot claim a port. Without this arm an empty lock list
    /// would compare two empty sets and report green — the one result that must never be free.
    #[error("no captured contract is embedded: this binary cannot claim a port yet")]
    Uncaptured,
    #[error(
        "the served routes drifted from the capture: {unserved} captured operation(s) have no \
         route, {unlocked} route(s) are not in any capture\n  unserved: {unserved_list}\n  \
         unlocked: {unlocked_list}"
    )]
    Drifted {
        unserved: usize,
        unlocked: usize,
        unserved_list: String,
        unlocked_list: String,
    },
}

/// What a green lock reports: how many services and operations it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    pub services: usize,
    pub operations: usize,
}

/// `"METHOD path"` for every operation in a document, sorted so every message is deterministic.
pub fn operations(service: &'static str, document: &serde_json::Value) -> BTreeSet<String> {
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
                out.insert(format!(
                    "{} {path}",
                    method.to_ascii_uppercase()
                ));
            }
        }
    }
    let _ = service;
    out
}

/// Parse a lock and return its operations.
pub fn locked(lock: &Lock) -> Result<BTreeSet<String>, LockError> {
    let document: serde_json::Value =
        serde_json::from_str(lock.json).map_err(|source| LockError::Unreadable {
            service: lock.service,
            source,
        })?;
    Ok(operations(lock.service, &document))
}

/// Every operation named by any of `locks`.
pub fn locked_union(locks: &[Lock]) -> Result<BTreeSet<String>, LockError> {
    let mut out = BTreeSet::new();
    for lock in locks {
        out.extend(locked(lock)?);
    }
    Ok(out)
}

/// Check the served routes against the locks: every captured operation must be routed, and no
/// route may exist that no capture names.
pub fn check(locks: &[Lock], served: &[(&str, &str)]) -> Result<Summary, LockError> {
    if locks.is_empty() {
        return Err(LockError::Uncaptured);
    }
    let want = locked_union(locks)?;
    let have: BTreeSet<String> = served
        .iter()
        .map(|(method, path)| format!("{method} {path}"))
        .collect();

    let unserved: Vec<&String> = want.difference(&have).collect();
    let unlocked: Vec<&String> = have.difference(&want).collect();
    if !unserved.is_empty() || !unlocked.is_empty() {
        return Err(LockError::Drifted {
            unserved: unserved.len(),
            unlocked: unlocked.len(),
            unserved_list: unserved
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            unlocked_list: unlocked
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        });
    }
    Ok(Summary {
        services: locks.len(),
        operations: want.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"{"paths":{"/a":{"get":{},"post":{}},"/b/{id}":{"delete":{}}}}"#;

    #[test]
    fn operations_are_method_and_path() {
        let ops = operations("x", &serde_json::from_str(DOC).unwrap());
        assert_eq!(
            ops.into_iter().collect::<Vec<_>>(),
            vec!["DELETE /b/{id}", "GET /a", "POST /a"]
        );
    }

    #[test]
    fn a_missing_route_is_drift() {
        let locks = [Lock {
            service: "x",
            json: DOC,
        }];
        let err = check(&locks, &[("GET", "/a"), ("POST", "/a")]).unwrap_err();
        assert!(err.to_string().contains("1 captured operation"), "{err}");
    }

    #[test]
    fn an_extra_route_is_drift() {
        let locks = [Lock {
            service: "x",
            json: DOC,
        }];
        let err = check(
            &locks,
            &[
                ("GET", "/a"),
                ("POST", "/a"),
                ("DELETE", "/b/{id}"),
                ("GET", "/z"),
            ],
        )
        .unwrap_err();
        assert!(err.to_string().contains("1 route(s)"), "{err}");
    }

    #[test]
    fn a_complete_port_is_green() {
        let locks = [Lock {
            service: "x",
            json: DOC,
        }];
        let summary = check(
            &locks,
            &[("GET", "/a"), ("POST", "/a"), ("DELETE", "/b/{id}")],
        )
        .unwrap();
        assert_eq!(summary, Summary {
            services: 1,
            operations: 3
        });
    }
}
