//! The one vocabulary table's kinds, its validated [`Code`], and the project-type matrix.
//!
//! `catalogue_entry.kind` has no CHECK in the migration by design (S03 §5): the vocabulary
//! is fixed here, and a new kind is a code change. The matrix lives on the
//! `kind='project_type'` row as `meta.matrix`, keyed by card-type code — there is no second
//! matrix row and no separate `project_type` column set (S3-D1).

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::board::{CardType, ListKind};
use crate::ids::Version;

/// A catalogue code: lowercase, stable, never free text.
///
/// The validation exists so a code that reached a `WHERE code = $1` is one spelling. A
/// display name is `label`; it is never used as a key.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Code(String);

/// Why a [`Code`] could not be built.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CodeError {
    /// The empty string is not a code.
    #[error("a code must not be empty")]
    Empty,
    /// Codes are short keys, not prose.
    #[error("a code is at most 64 characters, got {0}")]
    TooLong(usize),
    /// Only `[a-z0-9_-]`, starting with a letter or digit.
    #[error("invalid character {0:?} in a code (lowercase letters, digits, '_' and '-' only)")]
    InvalidChar(char),
}

impl Code {
    /// The maximum length. Long enough for `architecture_standard`, short enough to index.
    pub const MAX_LEN: usize = 64;

    /// Validate and wrap a code.
    pub fn parse(raw: &str) -> Result<Self, CodeError> {
        if raw.is_empty() {
            return Err(CodeError::Empty);
        }
        if raw.chars().count() > Self::MAX_LEN {
            return Err(CodeError::TooLong(raw.chars().count()));
        }
        for (index, ch) in raw.chars().enumerate() {
            let ok = if index == 0 {
                ch.is_ascii_lowercase() || ch.is_ascii_digit()
            } else {
                ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-'
            };
            if !ok {
                return Err(CodeError::InvalidChar(ch));
            }
        }
        Ok(Self(raw.to_owned()))
    }

    /// The stored value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Code {
    type Err = CodeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Serialize for Code {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Code {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// The eleven catalogue namespaces S03 fixes (S03 §5). The 280 generated flows are rows in
/// these namespaces, not 280 tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogueKind {
    /// A project type. Its row carries the card-type × list matrix in `meta.matrix`.
    ProjectType,
    /// A delivery phase code (`phase_code` on project and governed_asset).
    Phase,
    /// A reusable recipe.
    Recipe,
    /// A WIP-limit template; the live limit is `list.wip_limit`.
    WipLimit,
    /// A gate name, referenced by `approval.gate`.
    Gate,
    /// A capability exposed by the action registry.
    Capability,
    /// A deliverable kind.
    DeliverableKind,
    /// A milestone kind.
    MilestoneKind,
    /// A priority level.
    PriorityLevel,
    /// A board template.
    BoardTemplate,
    /// An architecture standard.
    ArchitectureStandard,
}

impl CatalogueKind {
    /// Every kind, in the order the plan lists them.
    pub const ALL: [CatalogueKind; 11] = [
        CatalogueKind::ProjectType,
        CatalogueKind::Phase,
        CatalogueKind::Recipe,
        CatalogueKind::WipLimit,
        CatalogueKind::Gate,
        CatalogueKind::Capability,
        CatalogueKind::DeliverableKind,
        CatalogueKind::MilestoneKind,
        CatalogueKind::PriorityLevel,
        CatalogueKind::BoardTemplate,
        CatalogueKind::ArchitectureStandard,
    ];

    /// The stored `catalogue_entry.kind`.
    pub fn as_str(self) -> &'static str {
        match self {
            CatalogueKind::ProjectType => "project_type",
            CatalogueKind::Phase => "phase",
            CatalogueKind::Recipe => "recipe",
            CatalogueKind::WipLimit => "wip_limit",
            CatalogueKind::Gate => "gate",
            CatalogueKind::Capability => "capability",
            CatalogueKind::DeliverableKind => "deliverable_kind",
            CatalogueKind::MilestoneKind => "milestone_kind",
            CatalogueKind::PriorityLevel => "priority_level",
            CatalogueKind::BoardTemplate => "board_template",
            CatalogueKind::ArchitectureStandard => "architecture_standard",
        }
    }
}

impl fmt::Display for CatalogueKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a string is not a catalogue kind.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown catalogue kind {0:?}: S03 fixes eleven kinds")]
pub struct CatalogueKindError(pub String);

impl FromStr for CatalogueKind {
    type Err = CatalogueKindError;
    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        CatalogueKind::ALL
            .into_iter()
            .find(|kind| kind.as_str() == raw)
            .ok_or_else(|| CatalogueKindError(raw.to_owned()))
    }
}

/// One `catalogue_entry` row as the domain sees it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CatalogueEntry {
    /// The row id.
    pub id: uuid::Uuid,
    /// Which vocabulary this row belongs to.
    pub kind: CatalogueKind,
    /// The stable code (`project.project_type_code` points at this for `project_type`).
    pub code: Code,
    /// The display label. Never a key.
    pub label: String,
    /// The kind-specific payload; `meta.matrix` for a project type.
    pub meta: Value,
    /// Display order within the kind.
    pub sort: i32,
    /// Retired rows stay readable but are not offered.
    pub is_active: bool,
    /// The optimistic-concurrency version.
    pub version: Version,
}

/// A project type's card-type × list matrix, resolved for one card type.
///
/// `meta.matrix` is keyed by card-type code (`feature`, `bug`) and then by the list kind the
/// card is leaving. A card type or a leaving kind with no entry is a denial, never a default
/// allow (S3-D3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardTypeRules {
    card_type_id: uuid::Uuid,
    card_type_code: Code,
    matrix: BTreeMap<ListKind, Vec<ListKind>>,
}

impl CardTypeRules {
    /// Resolve the matrix for `card_type` from a `kind='project_type'` entry.
    ///
    /// The worked row (S03 §5): `code='delivery'`, `meta.matrix.feature.active =
    /// ["waiting","done","archive"]`.
    pub fn from_entry(
        entry: &CatalogueEntry,
        card_type: &CardType,
    ) -> Result<Self, CatalogueError> {
        if entry.kind != CatalogueKind::ProjectType {
            return Err(CatalogueError::WrongKind {
                expected: CatalogueKind::ProjectType,
                found: entry.kind,
            });
        }
        let matrix = entry
            .meta
            .get("matrix")
            .and_then(Value::as_object)
            .ok_or(CatalogueError::MissingMatrix)?;
        let rows = matrix
            .get(card_type.code.as_str())
            .and_then(Value::as_object)
            .ok_or_else(|| CatalogueError::MissingCardType(card_type.code.clone()))?;

        let mut parsed = BTreeMap::new();
        for (from, destinations) in rows {
            let from = ListKind::from_str(from).map_err(CatalogueError::InvalidListKind)?;
            let destinations = destinations
                .as_array()
                .ok_or(CatalogueError::InvalidMatrixShape)?;
            let mut allowed = Vec::with_capacity(destinations.len());
            for destination in destinations {
                let raw = destination
                    .as_str()
                    .ok_or(CatalogueError::InvalidMatrixShape)?;
                allowed.push(ListKind::from_str(raw).map_err(CatalogueError::InvalidListKind)?);
            }
            parsed.insert(from, allowed);
        }

        Ok(Self {
            card_type_id: card_type.id,
            card_type_code: card_type.code.clone(),
            matrix: parsed,
        })
    }

    /// The card type these rules were resolved for.
    pub fn card_type_id(&self) -> uuid::Uuid {
        self.card_type_id
    }

    /// The card-type code these rules were resolved for.
    pub fn card_type_code(&self) -> &Code {
        &self.card_type_code
    }

    /// The destinations allowed when leaving `from`, or `None` when the row is absent.
    ///
    /// An empty slice is a row that allows nothing — still a denial for every destination.
    pub fn allowed_from(&self, from: ListKind) -> Option<&[ListKind]> {
        self.matrix.get(&from).map(Vec::as_slice)
    }
}

/// Why a matrix could not be resolved.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CatalogueError {
    /// Only a `project_type` row carries a matrix.
    #[error("expected a {expected} row, found {found}")]
    WrongKind {
        /// The kind the caller needed.
        expected: CatalogueKind,
        /// The kind the row actually has.
        found: CatalogueKind,
    },
    /// `meta.matrix` is absent.
    #[error("catalogue entry meta has no `matrix` object")]
    MissingMatrix,
    /// The matrix has no row for this card type.
    #[error("the matrix has no entry for card type {0}")]
    MissingCardType(Code),
    /// A list-kind key or destination is not one of the five kinds.
    #[error("invalid list kind in the matrix: {0}")]
    InvalidListKind(#[source] crate::board::ListKindError),
    /// A destinations value is not an array of strings.
    #[error("a matrix entry must be an array of list-kind strings")]
    InvalidMatrixShape,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry(kind: CatalogueKind, code: &str, meta: Value) -> CatalogueEntry {
        CatalogueEntry {
            id: uuid::Uuid::now_v7(),
            kind,
            code: Code::parse(code).expect("code"),
            label: code.to_owned(),
            meta,
            sort: 0,
            is_active: true,
            version: Version::INITIAL,
        }
    }

    fn card_type(code: &str) -> CardType {
        CardType {
            id: uuid::Uuid::now_v7(),
            code: Code::parse(code).expect("code"),
            label: code.to_owned(),
            definition: json!({}),
            is_active: true,
            version: Version::INITIAL,
        }
    }

    #[test]
    fn the_worked_matrix_resolves_and_denies_absent_entries() {
        let worked = json!({
            "matrix": {
                "feature": {
                    "intake": ["active", "archive"],
                    "active": ["waiting", "done", "archive"],
                    "waiting": ["active", "archive"],
                    "done": ["archive"],
                    "archive": ["intake"]
                },
                "bug": {
                    "intake": ["active"],
                    "active": ["done", "archive"],
                    "done": ["archive"]
                }
            }
        });
        let rules = CardTypeRules::from_entry(
            &entry(CatalogueKind::ProjectType, "delivery", worked),
            &card_type("feature"),
        )
        .expect("the worked row resolves");
        assert_eq!(
            rules.allowed_from(ListKind::Active),
            Some(&[ListKind::Waiting, ListKind::Done, ListKind::Archive][..])
        );
        assert_eq!(
            rules.allowed_from(ListKind::Intake),
            Some(&[ListKind::Active, ListKind::Archive][..])
        );
        assert_eq!(
            rules.allowed_from(ListKind::Archive),
            Some(&[ListKind::Intake][..])
        );

        // The bug row has no `waiting` entry: absence is a denial, not a default allow.
        let bug =
            CardTypeRules::from_entry(&entry(CatalogueKind::ProjectType, "delivery", json!({
                "matrix": {"bug": {"intake": ["active"], "active": ["done"], "done": ["archive"]}}
            })), &card_type("bug"))
            .expect("bug resolves");
        assert_eq!(bug.allowed_from(ListKind::Waiting), None);
    }

    #[test]
    fn a_wrong_kind_or_missing_matrix_is_an_error() {
        let wrong = entry(CatalogueKind::Gate, "g", json!({"matrix": {}}));
        assert!(matches!(
            CardTypeRules::from_entry(&wrong, &card_type("feature")),
            Err(CatalogueError::WrongKind { .. })
        ));

        let no_meta = entry(CatalogueKind::ProjectType, "delivery", json!({}));
        assert_eq!(
            CardTypeRules::from_entry(&no_meta, &card_type("feature")),
            Err(CatalogueError::MissingMatrix)
        );

        let no_type = entry(
            CatalogueKind::ProjectType,
            "delivery",
            json!({"matrix": {}}),
        );
        assert_eq!(
            CardTypeRules::from_entry(&no_type, &card_type("feature")),
            Err(CatalogueError::MissingCardType(
                Code::parse("feature").expect("code")
            ))
        );
    }

    #[test]
    fn an_unknown_list_kind_in_the_matrix_is_rejected() {
        let entry = entry(
            CatalogueKind::ProjectType,
            "delivery",
            json!({"matrix": {"feature": {"blocked": ["active"]}}}),
        );
        assert!(matches!(
            CardTypeRules::from_entry(&entry, &card_type("feature")),
            Err(CatalogueError::InvalidListKind(_))
        ));
    }

    #[test]
    fn codes_reject_uppercase_spaces_and_long_values() {
        assert!(matches!(Code::parse(""), Err(CodeError::Empty)));
        assert!(matches!(
            Code::parse("Delivery"),
            Err(CodeError::InvalidChar('D'))
        ));
        assert!(matches!(
            Code::parse("de livery"),
            Err(CodeError::InvalidChar(' '))
        ));
        assert!(matches!(
            Code::parse(&"a".repeat(65)),
            Err(CodeError::TooLong(65))
        ));
        assert_eq!(
            Code::parse("delivery_2").expect("valid").as_str(),
            "delivery_2"
        );
    }

    #[test]
    fn every_kind_round_trips_through_its_stored_string() {
        for kind in CatalogueKind::ALL {
            assert_eq!(
                CatalogueKind::from_str(kind.as_str()).expect("round trip"),
                kind
            );
        }
        assert_eq!(CatalogueKind::ALL.len(), 11);
        assert!(CatalogueKind::from_str("matrix").is_err());
    }
}
