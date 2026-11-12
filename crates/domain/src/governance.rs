//! One lifecycle for requirements, principles and ADRs, plus revision retention.
//!
//! `draft → in_review → approved → superseded`; `in_review → draft` when changes are
//! requested; `archived` is an operator act from any non-superseded status. A supersession
//! is not a bare status flip: `approved` is required, both pointers are written, and the
//! pair is returned so the caller writes them in one statement. PostgreSQL CHECK constraints
//! are immediate, so `superseded ⇒ superseded_by_id IS NOT NULL` can only hold if both
//! pointers are set together — and the reciprocal edge (`supersedes_id`/_by agree) is this
//! function's job, not a deferred constraint (data-arch §3.4).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ids::Version;

/// The three governed-asset kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    /// A requirement.
    Requirement,
    /// A principle.
    Principle,
    /// An architecture decision record.
    Adr,
}

impl AssetKind {
    /// Every kind.
    pub const ALL: [AssetKind; 3] = [AssetKind::Requirement, AssetKind::Principle, AssetKind::Adr];

    /// The stored `governed_asset.kind`.
    pub fn as_str(self) -> &'static str {
        match self {
            AssetKind::Requirement => "requirement",
            AssetKind::Principle => "principle",
            AssetKind::Adr => "adr",
        }
    }
}

impl fmt::Display for AssetKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a string is not an asset kind.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown governed-asset kind {0:?}")]
pub struct AssetKindError(pub String);

impl FromStr for AssetKind {
    type Err = AssetKindError;
    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        AssetKind::ALL
            .into_iter()
            .find(|kind| kind.as_str() == raw)
            .ok_or_else(|| AssetKindError(raw.to_owned()))
    }
}

/// The five asset statuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetStatus {
    /// Being written.
    Draft,
    /// Awaiting review.
    InReview,
    /// Passed review; only an approved asset can be superseded.
    Approved,
    /// Replaced by `superseded_by_id`; terminal.
    Superseded,
    /// Retired by an operator; terminal.
    Archived,
}

impl AssetStatus {
    /// Every status.
    pub const ALL: [AssetStatus; 5] = [
        AssetStatus::Draft,
        AssetStatus::InReview,
        AssetStatus::Approved,
        AssetStatus::Superseded,
        AssetStatus::Archived,
    ];

    /// The stored `governed_asset.status`.
    pub fn as_str(self) -> &'static str {
        match self {
            AssetStatus::Draft => "draft",
            AssetStatus::InReview => "in_review",
            AssetStatus::Approved => "approved",
            AssetStatus::Superseded => "superseded",
            AssetStatus::Archived => "archived",
        }
    }
}

impl fmt::Display for AssetStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a string is not an asset status.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown governed-asset status {0:?}")]
pub struct AssetStatusError(pub String);

impl FromStr for AssetStatus {
    type Err = AssetStatusError;
    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        AssetStatus::ALL
            .into_iter()
            .find(|status| status.as_str() == raw)
            .ok_or_else(|| AssetStatusError(raw.to_owned()))
    }
}

/// One governed asset (a requirement, a principle or an ADR).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GovernedAsset {
    /// The row id.
    pub id: uuid::Uuid,
    /// Requirement, principle or ADR.
    pub kind: AssetKind,
    /// The stable key within the kind; unique per tenant.
    pub key: String,
    /// The title.
    pub title: String,
    /// The asset's status.
    pub status: AssetStatus,
    /// The asset this one replaces.
    pub supersedes_id: Option<uuid::Uuid>,
    /// The asset that replaced this one; non-null whenever the status is `superseded`.
    pub superseded_by_id: Option<uuid::Uuid>,
    /// The optimistic-concurrency version. `revision.seq` equals this value.
    pub version: Version,
}

/// Why a transition is refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GovernanceError {
    /// A superseded asset is terminal.
    #[error("the asset is superseded and accepts no transition")]
    AlreadySuperseded,
    /// An archived asset is terminal.
    #[error("the asset is archived and accepts no transition")]
    Archived,
    /// `superseded` is reached only through `supersede`, which sets both pointers.
    #[error("use `supersede`, which sets superseded_by_id and the reciprocal edge")]
    UseSupersede,
    /// Only an approved asset may be superseded.
    #[error("only an approved asset can be superseded (status is {0})")]
    NotApproved(AssetStatus),
    /// An asset cannot supersede itself.
    #[error("an asset cannot supersede itself")]
    SelfSupersede,
    /// The successor is not a live asset.
    #[error("the successor is {0}; only a live asset can supersede")]
    SuccessorNotLive(AssetStatus),
    /// The successor already supersedes another asset.
    #[error("the successor already supersedes {0}; the chain may not fork")]
    SuccessorAlreadySupersedes(uuid::Uuid),
    /// The pair is not in the documented transition table.
    #[error("an asset cannot transition from {from} to {to}")]
    Illegal {
        /// The status being left.
        from: AssetStatus,
        /// The status being entered.
        to: AssetStatus,
    },
}

impl GovernedAsset {
    /// Apply a status transition, or refuse it.
    ///
    /// `superseded` is deliberately unreachable here: it requires `superseded_by_id`, and a
    /// transition that cannot write it would violate the table's CHECK.
    pub fn transition(mut self, to: AssetStatus) -> Result<Self, GovernanceError> {
        match self.status {
            AssetStatus::Superseded => return Err(GovernanceError::AlreadySuperseded),
            AssetStatus::Archived => return Err(GovernanceError::Archived),
            _ => {}
        }
        if to == AssetStatus::Superseded {
            return Err(GovernanceError::UseSupersede);
        }
        let legal = matches!(
            (self.status, to),
            (AssetStatus::Draft, AssetStatus::InReview)
                | (AssetStatus::Draft, AssetStatus::Archived)
                | (AssetStatus::InReview, AssetStatus::Draft)
                | (AssetStatus::InReview, AssetStatus::Approved)
                | (AssetStatus::InReview, AssetStatus::Archived)
                | (AssetStatus::Approved, AssetStatus::Archived)
        );
        if !legal {
            return Err(GovernanceError::Illegal {
                from: self.status,
                to,
            });
        }
        self.status = to;
        self.version = self.version.next();
        Ok(self)
    }

    /// Replace this asset with `successor`, setting both edges of the chain.
    ///
    /// Returns `(self_as_superseded, successor_with_supersedes_id)`. The caller writes both
    /// rows in the same transaction; the database's CHECK only sees the pair, never a
    /// dangling pointer.
    pub fn supersede(
        mut self,
        mut successor: GovernedAsset,
    ) -> Result<(GovernedAsset, GovernedAsset), GovernanceError> {
        if self.status != AssetStatus::Approved {
            return Err(GovernanceError::NotApproved(self.status));
        }
        if successor.id == self.id {
            return Err(GovernanceError::SelfSupersede);
        }
        if matches!(
            successor.status,
            AssetStatus::Archived | AssetStatus::Superseded
        ) {
            return Err(GovernanceError::SuccessorNotLive(successor.status));
        }
        if let Some(existing) = successor.supersedes_id
            && existing != self.id
        {
            return Err(GovernanceError::SuccessorAlreadySupersedes(existing));
        }

        self.status = AssetStatus::Superseded;
        self.superseded_by_id = Some(successor.id);
        self.version = self.version.next();

        successor.supersedes_id = Some(self.id);
        successor.version = successor.version.next();
        Ok((self, successor))
    }
}

/// A revision row's status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionStatus {
    /// Editing.
    Draft,
    /// Published.
    Published,
    /// Replaced by a later revision.
    Superseded,
}

impl RevisionStatus {
    /// Every status.
    pub const ALL: [RevisionStatus; 3] = [
        RevisionStatus::Draft,
        RevisionStatus::Published,
        RevisionStatus::Superseded,
    ];

    /// The stored `revision.status`.
    pub fn as_str(self) -> &'static str {
        match self {
            RevisionStatus::Draft => "draft",
            RevisionStatus::Published => "published",
            RevisionStatus::Superseded => "superseded",
        }
    }
}

impl fmt::Display for RevisionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a string is not a revision status.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown revision status {0:?}")]
pub struct RevisionStatusError(pub String);

impl FromStr for RevisionStatus {
    type Err = RevisionStatusError;
    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        RevisionStatus::ALL
            .into_iter()
            .find(|status| status.as_str() == raw)
            .ok_or_else(|| RevisionStatusError(raw.to_owned()))
    }
}

/// One row of the ONE history table.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Revision {
    /// The row id.
    pub id: uuid::Uuid,
    /// The entity family (`requirement`, `model`, …). Retention is a function of this.
    pub entity_type: String,
    /// The entity's id.
    pub entity_id: uuid::Uuid,
    /// The dense sequence within the entity. For a governed asset it equals `version`.
    pub seq: i32,
    /// A display label for the revision.
    pub label: String,
    /// The content snapshot.
    pub content: Value,
    /// The content's hash, for deduplication and integrity.
    pub content_hash: String,
    /// The author, when one is recorded.
    pub author_id: Option<uuid::Uuid>,
    /// The revision's status.
    pub status: RevisionStatus,
}

/// Retention for revisions: a job-applied policy, never a trigger (S03 §8 trap 4).
///
/// Only `model` is bounded, to the last 50 revisions. Every other entity type — a
/// requirement, an unknown type, anything — keeps every revision, because a wrong cap deletes
/// history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevisionPolicy {
    /// How many model revisions to keep.
    pub model_retained: i32,
}

impl Default for RevisionPolicy {
    fn default() -> Self {
        Self {
            model_retained: Self::MODEL_RETAINED,
        }
    }
}

impl RevisionPolicy {
    /// The documented cap for model revisions.
    pub const MODEL_RETAINED: i32 = 50;

    /// True only for a model revision past the cap. Unknown entity types are kept.
    pub fn is_prunable(&self, entity_type: &str, seq: i32) -> bool {
        entity_type == "model" && seq > self.model_retained
    }

    /// The same question asked of a revision row.
    pub fn is_prunable_revision(&self, revision: &Revision) -> bool {
        self.is_prunable(&revision.entity_type, revision.seq)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn asset(status: AssetStatus) -> GovernedAsset {
        GovernedAsset {
            id: uuid::Uuid::now_v7(),
            kind: AssetKind::Requirement,
            key: "r-1".to_owned(),
            title: "R1".to_owned(),
            status,
            supersedes_id: None,
            superseded_by_id: None,
            version: Version::INITIAL,
        }
    }

    #[test]
    fn the_lifecycle_is_exactly_the_documented_one() {
        let cases = [
            (AssetStatus::Draft, AssetStatus::InReview, true),
            (AssetStatus::Draft, AssetStatus::Approved, false),
            (AssetStatus::Draft, AssetStatus::Archived, true),
            (AssetStatus::Draft, AssetStatus::Draft, false),
            (AssetStatus::InReview, AssetStatus::Draft, true),
            (AssetStatus::InReview, AssetStatus::Approved, true),
            (AssetStatus::InReview, AssetStatus::Archived, true),
            (AssetStatus::InReview, AssetStatus::Superseded, false),
            (AssetStatus::Approved, AssetStatus::Archived, true),
            (AssetStatus::Approved, AssetStatus::Draft, false),
            (AssetStatus::Approved, AssetStatus::Superseded, false),
        ];
        for (from, to, legal) in cases {
            let outcome = asset(from).transition(to);
            if legal {
                assert_eq!(outcome.expect("legal").status, to, "{from} -> {to}");
            } else {
                assert!(outcome.is_err(), "{from} -> {to} must be refused");
            }
        }
        for terminal in [AssetStatus::Superseded, AssetStatus::Archived] {
            for to in AssetStatus::ALL {
                assert!(
                    asset(terminal).transition(to).is_err(),
                    "{terminal} -> {to} must be refused"
                );
            }
        }
    }

    #[test]
    fn supersede_writes_both_edges_and_requires_approval() {
        let old = asset(AssetStatus::Approved);
        let new = asset(AssetStatus::Draft);
        let (old, new) = old
            .clone()
            .supersede(new.clone())
            .expect("an approved asset supersedes");
        assert_eq!(old.status, AssetStatus::Superseded);
        assert_eq!(old.superseded_by_id, Some(new.id));
        assert_eq!(new.supersedes_id, Some(old.id));

        // Still draft: not approvable-by-supersession.
        assert_eq!(
            asset(AssetStatus::Draft).supersede(new.clone()),
            Err(GovernanceError::NotApproved(AssetStatus::Draft))
        );
        // The superseded asset is terminal.
        assert_eq!(
            old.clone().supersede(asset(AssetStatus::Draft)),
            Err(GovernanceError::NotApproved(AssetStatus::Superseded))
        );
        // Approved, but the candidate successor is itself retired.
        let approved = asset(AssetStatus::Approved);
        assert_eq!(
            approved.clone().supersede(asset(AssetStatus::Archived)),
            Err(GovernanceError::SuccessorNotLive(AssetStatus::Archived))
        );
        // An asset cannot supersede itself.
        assert_eq!(
            approved.clone().supersede(approved.clone()),
            Err(GovernanceError::SelfSupersede)
        );
        // The successor may not already supersede a third asset.
        let mut already = asset(AssetStatus::Draft);
        already.supersedes_id = Some(uuid::Uuid::now_v7());
        assert!(matches!(
            approved.supersede(already),
            Err(GovernanceError::SuccessorAlreadySupersedes(_))
        ));
    }

    #[test]
    fn only_model_revisions_are_prunable_and_only_past_fifty() {
        let policy = RevisionPolicy::default();
        assert!(!policy.is_prunable("model", 50));
        assert!(policy.is_prunable("model", 51));
        assert!(!policy.is_prunable("requirement", 51));
        assert!(!policy.is_prunable("requirement", 100_000));
        assert!(!policy.is_prunable("", 999));
        assert_eq!(RevisionPolicy::MODEL_RETAINED, 50);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Gate property (d), retention arm: `model` is prunable exactly past 50; every other
        /// entity type is kept at any depth.
        #[test]
        fn retention_is_a_function_of_entity_type_and_depth(
            entity_type in "[a-z_]{0,20}",
            seq in 1i32..=10_000,
        ) {
            let policy = RevisionPolicy::default();
            let expected = entity_type == "model" && seq > 50;
            prop_assert_eq!(policy.is_prunable(&entity_type, seq), expected);
            prop_assert!(!policy.is_prunable("requirement", seq));
        }
    }
}
