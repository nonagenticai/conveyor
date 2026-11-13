//! The organisation spine's vocabulary (SPEC_V2 §4.1–§4.4; T0, issue #34 P2).
//!
//! Two things live here and nowhere else:
//!
//! - [`release_hash`] — the immutable release's identity. It is the crate's ONE canonical-form
//!   hash ([`crate::compare::content_hash`], the RFC 8785 subset), because a second hashing
//!   function would be a second definition of "the same release". The same job and pins
//!   compiled twice therefore hash identically and their snapshots are byte-identical (P2.1).
//! - [`BindingStatus`] — the binding's state machine (§4.4). A retired binding is refused for
//!   new runs (`409 job_binding_stale`) and existing runs keep their pin.

use std::str::FromStr;

use serde_json::Value;

/// SHA-256 of the canonical release snapshot, lowercase hex.
///
/// The canonical form is [`crate::compare::canonical_json`]: explicit UTF-16 key sort, no
/// whitespace. The store writes the returned string to `job_release.release_hash` and copies it
/// onto every binding and every run that pins the release.
pub fn release_hash(snapshot: &Value) -> String {
    crate::compare::content_hash(snapshot)
}

/// A binding's state (SPEC_V2 §4.4: `draft | active | retired`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BindingStatus {
    /// Authored but not yet usable by a run.
    Draft,
    /// The live pin: a run may name it.
    Active,
    /// Withdrawn: new runs naming it are refused; history keeps its pin.
    Retired,
}

impl BindingStatus {
    /// Every status, in lifecycle order.
    pub const ALL: [BindingStatus; 3] = [
        BindingStatus::Draft,
        BindingStatus::Active,
        BindingStatus::Retired,
    ];

    /// The database spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            BindingStatus::Draft => "draft",
            BindingStatus::Active => "active",
            BindingStatus::Retired => "retired",
        }
    }

    /// Whether a NEW run may name a binding in this state.
    ///
    /// A draft binding has no live pin yet; a retired one is exactly what
    /// `409 job_binding_stale` exists to refuse.
    pub fn is_usable_for_new_runs(self) -> bool {
        matches!(self, BindingStatus::Active)
    }
}

impl FromStr for BindingStatus {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "draft" => Ok(BindingStatus::Draft),
            "active" => Ok(BindingStatus::Active),
            "retired" => Ok(BindingStatus::Retired),
            other => Err(format!("{other:?} is not a binding status")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_release_hash_is_the_crate_canonical_hash() {
        let snapshot = json!({"job": "backend_engineer", "pins": {"model": "m", "process": "p"}});
        assert_eq!(
            release_hash(&snapshot),
            crate::compare::content_hash(&snapshot)
        );
        assert_eq!(release_hash(&snapshot), release_hash(&snapshot));
        assert_eq!(release_hash(&snapshot).len(), 64);
    }

    #[test]
    fn reordered_snapshots_hash_identically() {
        let left = json!({"b": 1, "a": {"y": true, "x": [1, 2]}});
        let right = json!({"a": {"x": [1, 2], "y": true}, "b": 1});
        assert_eq!(release_hash(&left), release_hash(&right));
    }

    #[test]
    fn a_different_pin_is_a_different_release() {
        let left = json!({"model": "m-1"});
        let right = json!({"model": "m-2"});
        assert_ne!(release_hash(&left), release_hash(&right));
    }

    #[test]
    fn only_an_active_binding_takes_new_runs() {
        assert!(BindingStatus::Active.is_usable_for_new_runs());
        assert!(!BindingStatus::Draft.is_usable_for_new_runs());
        assert!(!BindingStatus::Retired.is_usable_for_new_runs());
        for status in BindingStatus::ALL {
            assert_eq!(
                BindingStatus::from_str(status.as_str()).expect("round-trips"),
                status
            );
        }
        assert!(BindingStatus::from_str("live").is_err());
    }
}
