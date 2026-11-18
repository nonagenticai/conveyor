//! The TOGAF ADM vocabulary (SPEC_V2 §8.1, §10.3, D63; T9, issue #81).
//!
//! One definition of each vocabulary, so the migration's CHECK, the actions' validation and the
//! tests cannot drift. The values are the lowercase machine codes stored in Postgres; the prose
//! names live in the docs and the action descriptions.
//!
//! The phase order is the ADM's, not an arbitrary list: a phase advances to the next one only,
//! and `next_phase` is the single place that order is defined.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// The ADM's phase codes in order: Preliminary, then A–H.
pub const PHASES: [&str; 9] = [
    "preliminary", "a", "b", "c", "d", "e", "f", "g", "h",
];

/// The three artefact plateaus (§8.1).
pub const PLATEAUS: [&str; 3] = ["psa", "hld", "sd"];

/// The three artefact types plus the principles catalog, which is a catalog subtype (§8.1).
pub const ARTIFACT_KINDS: [&str; 4] = ["catalog", "matrix", "diagram", "principles_catalog"];

/// The artefact-to-artefact link kinds (§8.1).
pub const LINK_KINDS: [&str; 4] = ["derivation", "realization", "reuse", "dependency"];

/// The four contract kinds across the ADM (§8.1).
pub const CONTRACT_KINDS: [&str; 4] = [
    "statement_of_architecture_work",
    "delivery_contract",
    "statement_of_intent",
    "request_for_architecture",
];

/// The six compliance relationships a review may record (§8.1).
pub const COMPLIANCE_LEVELS: [&str; 6] = [
    "irrelevant",
    "consistent",
    "compliant",
    "conformant",
    "fully_conformant",
    "non_conformant",
];

/// The nine maturity dimensions (§8.1).
pub const MATURITY_DIMENSIONS: [&str; 9] = [
    "architecture_process",
    "architecture_development",
    "business_linkage",
    "senior_management",
    "operating_unit_participation",
    "architecture_communication",
    "it_security",
    "architecture_governance",
    "investment_strategy",
];

/// The seven Architecture Skills Framework categories (§8.1).
pub const SKILL_CATEGORIES: [&str; 7] = [
    "general",
    "business",
    "architecture",
    "programme_management",
    "it_knowledge",
    "technical",
    "legal",
];

/// The phase's position in the ADM, or `None` for an unknown code.
pub fn phase_index(code: &str) -> Option<usize> {
    PHASES.iter().position(|phase| *phase == code)
}

/// The phase that follows `code`, or `None` at H (there is no I).
pub fn next_phase(code: &str) -> Option<&'static str> {
    phase_index(code).and_then(|index| PHASES.get(index + 1).copied())
}

/// True when `code` is one of the nine phase codes.
pub fn is_phase(code: &str) -> bool {
    phase_index(code).is_some()
}

/// True when `value` is one of the three plateaus.
pub fn is_plateau(value: &str) -> bool {
    PLATEAUS.contains(&value)
}

/// True when `value` is one of the four artefact types.
pub fn is_artifact_kind(value: &str) -> bool {
    ARTIFACT_KINDS.contains(&value)
}

/// True when `value` is one of the four link kinds.
pub fn is_link_kind(value: &str) -> bool {
    LINK_KINDS.contains(&value)
}

/// True when `value` is one of the four contract kinds.
pub fn is_contract_kind(value: &str) -> bool {
    CONTRACT_KINDS.contains(&value)
}

/// A compliance relationship, parsed from its stored code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComplianceLevel {
    /// The architecture does not apply to this project.
    Irrelevant,
    /// Consistent with the architecture.
    Consistent,
    /// Compliant with the architecture.
    Compliant,
    /// Conformant with the architecture.
    Conformant,
    /// Fully conformant.
    FullyConformant,
    /// Not conformant.
    NonConformant,
}

impl ComplianceLevel {
    /// Every level.
    pub const ALL: [ComplianceLevel; 6] = [
        ComplianceLevel::Irrelevant,
        ComplianceLevel::Consistent,
        ComplianceLevel::Compliant,
        ComplianceLevel::Conformant,
        ComplianceLevel::FullyConformant,
        ComplianceLevel::NonConformant,
    ];

    /// The stored code.
    pub fn as_str(self) -> &'static str {
        match self {
            ComplianceLevel::Irrelevant => "irrelevant",
            ComplianceLevel::Consistent => "consistent",
            ComplianceLevel::Compliant => "compliant",
            ComplianceLevel::Conformant => "conformant",
            ComplianceLevel::FullyConformant => "fully_conformant",
            ComplianceLevel::NonConformant => "non_conformant",
        }
    }
}

impl FromStr for ComplianceLevel {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        ComplianceLevel::ALL
            .into_iter()
            .find(|level| level.as_str() == value)
            .ok_or(())
    }
}

/// Why an ADM vocabulary value was refused.
#[derive(Debug, thiserror::Error)]
pub enum ArchitectureError {
    /// A phase, plateau, kind or level the standard does not define.
    #[error("{what} {value:?} is not one of the standard's values")]
    UnknownVocabulary {
        /// The field name.
        what: &'static str,
        /// The refused value.
        value: String,
    },
    /// A phase cannot be skipped: the requested phase is not the next one.
    #[error("phase {requested:?} does not follow {current:?}")]
    PhaseOutOfOrder {
        /// The project's current phase.
        current: String,
        /// The requested phase.
        requested: String,
    },
    /// A dimension, category or score was outside the standard's shape.
    #[error("{0}")]
    Invalid(String),
}

/// Validate a phase code.
pub fn require_phase(value: &str) -> Result<(), ArchitectureError> {
    if is_phase(value) {
        Ok(())
    } else {
        Err(ArchitectureError::UnknownVocabulary {
            what: "phase_code",
            value: value.to_owned(),
        })
    }
}

/// Validate a plateau code.
pub fn require_plateau(value: &str) -> Result<(), ArchitectureError> {
    if is_plateau(value) {
        Ok(())
    } else {
        Err(ArchitectureError::UnknownVocabulary {
            what: "plateau",
            value: value.to_owned(),
        })
    }
}

/// Validate an artefact kind.
pub fn require_artifact_kind(value: &str) -> Result<(), ArchitectureError> {
    if is_artifact_kind(value) {
        Ok(())
    } else {
        Err(ArchitectureError::UnknownVocabulary {
            what: "artifact_kind",
            value: value.to_owned(),
        })
    }
}

/// Validate a link kind.
pub fn require_link_kind(value: &str) -> Result<(), ArchitectureError> {
    if is_link_kind(value) {
        Ok(())
    } else {
        Err(ArchitectureError::UnknownVocabulary {
            what: "kind",
            value: value.to_owned(),
        })
    }
}

/// Validate a contract kind.
pub fn require_contract_kind(value: &str) -> Result<(), ArchitectureError> {
    if is_contract_kind(value) {
        Ok(())
    } else {
        Err(ArchitectureError::UnknownVocabulary {
            what: "kind",
            value: value.to_owned(),
        })
    }
}

/// Validate a compliance level.
pub fn require_compliance_level(value: &str) -> Result<(), ArchitectureError> {
    if COMPLIANCE_LEVELS.contains(&value) {
        Ok(())
    } else {
        Err(ArchitectureError::UnknownVocabulary {
            what: "level",
            value: value.to_owned(),
        })
    }
}
