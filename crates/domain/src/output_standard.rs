//! The programme output standard: one vocabulary and its legacy aliases.
//!
//! `seed/output-standard.json` is the source of truth; the Python seed builder, the generated
//! console copy and this module read the same file. A document category is canonicalised through
//! the alias map before it is stored, and anything neither canonical nor an alias is refused with
//! `422 unknown_category` by the action layer.

use std::fmt;

use serde_json::Value;

/// The embedded standard. `include_str!` fails the build if the file moves or disappears.
const STANDARD: &str = include_str!("../../../seed/output-standard.json");

/// A category that is neither canonical nor an alias in the standard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownCategory(pub String);

impl fmt::Display for UnknownCategory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} is not an output category in seed/output-standard.json",
            self.0
        )
    }
}

impl std::error::Error for UnknownCategory {}

fn standard() -> Value {
    serde_json::from_str(STANDARD).expect("seed/output-standard.json is valid JSON")
}

/// Canonicalise one category through the standard's aliases.
///
/// A canonical value passes through; an alias becomes its target; anything else is an error.
/// The caller trims the slot first, so an empty string is an error here rather than a silent null.
pub fn canonical_category(raw: &str) -> Result<String, UnknownCategory> {
    let raw = raw.trim();
    let document = standard();
    let categories = document["categories"]
        .as_array()
        .expect("the standard has a categories array");
    let aliases = document["aliases"]
        .as_object()
        .expect("the standard has an aliases object");
    let canonical = aliases.get(raw).and_then(Value::as_str).unwrap_or(raw);
    if categories
        .iter()
        .any(|value| value.as_str() == Some(canonical))
    {
        Ok(canonical.to_owned())
    } else {
        Err(UnknownCategory(raw.to_owned()))
    }
}

/// The canonical vocabulary, in the standard's order.
pub fn categories() -> Vec<String> {
    standard()["categories"]
        .as_array()
        .expect("the standard has a categories array")
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{UnknownCategory, canonical_category, categories};

    #[test]
    fn aliases_map_to_the_canonical_names() {
        assert_eq!(
            canonical_category("analysis").expect("alias"),
            "inception-analysis"
        );
        assert_eq!(
            canonical_category("architecture").expect("alias"),
            "architecture-design"
        );
        assert_eq!(canonical_category("brief").expect("alias"), "project-brief");
        assert_eq!(
            canonical_category("project-brief").expect("canonical"),
            "project-brief"
        );
    }

    #[test]
    fn an_unknown_category_is_refused() {
        assert_eq!(
            canonical_category("not-a-category"),
            Err(UnknownCategory("not-a-category".to_owned()))
        );
        assert!(canonical_category("").is_err());
    }

    #[test]
    fn every_canonical_category_round_trips() {
        let vocabulary = categories();
        assert!(!vocabulary.is_empty());
        for category in vocabulary {
            assert_eq!(
                canonical_category(&category).expect("canonical"),
                category,
                "the vocabulary must be closed"
            );
        }
    }

    /// The rigour vocabulary (#221) is part of the closed set: a class may require it and a
    /// document may carry it. Removing one is a standard change, never a silent rename.
    #[test]
    fn the_rigour_categories_are_in_the_vocabulary() {
        let vocabulary = categories();
        for category in [
            "benefit-profile",
            "benefits-map",
            "projects-dossier",
            "tranche-plan",
            "tranche-review",
            "benefits-strategy",
            "realization-plan",
            "stakeholder-profiles",
            "architecture-principles",
            "adm-profile",
            "artifact-standard",
            "architecture-artifact",
            "statement-of-architecture-work",
            "compliance-review",
            "waiver",
            "maturity-assessment",
            "skills-assessment",
        ] {
            assert!(
                vocabulary.iter().any(|value| value == category),
                "{category} is missing from seed/output-standard.json"
            );
            assert_eq!(
                canonical_category(category).expect("rigour category"),
                category,
                "a rigour category canonicalises to itself"
            );
        }
    }
}
