//! The curated, versioned element and relationship profiles (S08-D2).
//!
//! A **profile** is what *this application accepts*, not what the standard permits. The
//! ArchiMate 3.2 registry (`requirements_management/src/data/archimate/3.2/registry.json`,
//! 61 concepts) supplies labels and icons; it is never the validator (slice trap 8). The
//! profile is a strict subset chosen by three consumers, and every kind below was verified
//! present in that registry with the same spelling.
//!
//! Membership was decided by consumers, in this order: live RM data (measured **zero** rows
//! in every `architecture_*`/`uml_*` table), fr's render map (`lib/modeling/archimate.ts`
//! carries the same 11 relationship kinds; `lib/modeling/uml.ts` carries 25 UML kinds, of
//! which 10 are here), and the standard.
//!
//! # Legality is layer-level for ArchiMate, kind-level for UML
//!
//! The full ArchiMate 3.2 relationship table is 30 × 11 × 30 cells. Only the parts that can
//! be stated without inventing data are enforced: composition, aggregation, association and
//! specialization must join two elements of the **same layer**; every other ArchiMate
//! relationship is unrestricted. UML relationships that genuinely restrict their endpoints
//! (`include`, `extend`, `message`, `realization`) name the kinds; the rest are unrestricted.
//! A rule here that rejects a legal model is worse than one that misses an illegal edge.

pub mod archimate;
pub mod uml;

/// The modelling language a [`Profile`] governs. `model.language` holds the same value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    /// ArchiMate 3.2.
    Archimate,
    /// UML 2.5.1.
    Uml,
}

impl Language {
    /// The value stored in `model.language`.
    pub fn as_str(self) -> &'static str {
        match self {
            Language::Archimate => "archimate",
            Language::Uml => "uml",
        }
    }

    /// Parse `model.language`. Unknown values are `None`, never a default.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "archimate" => Some(Language::Archimate),
            "uml" => Some(Language::Uml),
            _ => None,
        }
    }
}

/// The ArchiMate layer an element kind belongs to. UML element kinds carry no layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    /// Strategy.
    Strategy,
    /// Business.
    Business,
    /// Application.
    Application,
    /// Technology.
    Technology,
    /// Motivation.
    Motivation,
    /// Implementation and migration.
    Implementation,
    /// Composite (grouping, location) — the kinds that may contain anything.
    Composite,
}

impl Layer {
    /// The layer name used in findings.
    pub fn as_str(self) -> &'static str {
        match self {
            Layer::Strategy => "strategy",
            Layer::Business => "business",
            Layer::Application => "application",
            Layer::Technology => "technology",
            Layer::Motivation => "motivation",
            Layer::Implementation => "implementation_migration",
            Layer::Composite => "composite",
        }
    }
}

/// One accepted element kind, with its ArchiMate layer when it has one.
#[derive(Clone, Copy, Debug)]
pub struct ElementKind {
    /// The stored `model_element.kind`.
    pub kind: &'static str,
    /// The ArchiMate layer; `None` for every UML kind.
    pub layer: Option<Layer>,
}

/// What a relationship kind permits at each end.
#[derive(Clone, Copy, Debug)]
pub enum RelationshipRule {
    /// Any in-profile element may be either end.
    Any,
    /// Both ends must be in the same ArchiMate layer.
    SameLayer,
    /// Each end, when named, must be one of the listed element kinds.
    Kinds {
        /// Legal source kinds; `None` means any in-profile kind.
        sources: Option<&'static [&'static str]>,
        /// Legal target kinds; `None` means any in-profile kind.
        targets: Option<&'static [&'static str]>,
    },
}

/// One accepted relationship kind and its endpoint rule.
#[derive(Clone, Copy, Debug)]
pub struct RelationshipKind {
    /// The stored `model_relationship.kind`.
    pub kind: &'static str,
    /// The rule applied by [`super::validate`].
    pub rule: RelationshipRule,
}

/// A curated, versioned profile: the element kinds, the relationship kinds and the view
/// kinds one language version accepts.
#[derive(Debug)]
pub struct Profile {
    /// The language this profile governs.
    pub language: Language,
    /// The language version, e.g. `3.2` — `model.language_version`.
    pub language_version: &'static str,
    /// The ruleset id stored on every `model_validation_run`, e.g.
    /// `conveyor-archimate-3.2-v1`.
    pub ruleset: &'static str,
    /// The ruleset's own version. Bumped when a rule changes meaning, never when a kind is
    /// merely added (a run records what it was judged by).
    pub ruleset_version: &'static str,
    /// Every accepted element kind.
    pub element_kinds: &'static [ElementKind],
    /// Every accepted relationship kind.
    pub relationship_kinds: &'static [RelationshipKind],
    /// Every accepted `model_view.kind` (the column the database CHECKs).
    pub view_kinds: &'static [&'static str],
    /// Every accepted `model_view.meta.diagram_kind`, when that key is present.
    ///
    /// Empty for ArchiMate, which has no named diagram families beyond `view_kinds`.
    pub diagram_kinds: &'static [&'static str],
}

impl Profile {
    /// The element kind's entry, or `None` when the kind is out of profile.
    pub fn element(&self, kind: &str) -> Option<&'static ElementKind> {
        self.element_kinds.iter().find(|entry| entry.kind == kind)
    }

    /// Whether `kind` is an accepted element kind.
    pub fn has_element_kind(&self, kind: &str) -> bool {
        self.element(kind).is_some()
    }

    /// The relationship kind's entry, or `None` when the kind is out of profile.
    pub fn relationship(&self, kind: &str) -> Option<&'static RelationshipKind> {
        self.relationship_kinds
            .iter()
            .find(|entry| entry.kind == kind)
    }

    /// Whether `kind` is an accepted relationship kind.
    pub fn has_relationship_kind(&self, kind: &str) -> bool {
        self.relationship(kind).is_some()
    }

    /// Whether `kind` is an accepted `model_view.kind`.
    pub fn has_view_kind(&self, kind: &str) -> bool {
        self.view_kinds.contains(&kind)
    }

    /// Whether `diagram_kind` is accepted when `model_view.meta.diagram_kind` names one.
    pub fn has_diagram_kind(&self, diagram_kind: &str) -> bool {
        self.diagram_kinds.contains(&diagram_kind)
    }
}

/// Why a `(language, language_version)` pair has no profile.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProfileError {
    /// `model.language` is not a language this build knows.
    #[error("unknown modelling language {0:?}")]
    UnknownLanguage(String),
    /// The language is known but this version of it is not curated here.
    #[error("no profile for {language} {version}")]
    UnknownVersion {
        /// The language that was asked for.
        language: &'static str,
        /// The version that was asked for.
        version: String,
    },
}

/// The profile for a `(language, language_version)` pair.
///
/// Unknown values are an error, never a default: a model created against a profile that
/// does not exist would validate against nothing.
pub fn for_language(language: &str, version: &str) -> Result<&'static Profile, ProfileError> {
    let parsed = Language::parse(language)
        .ok_or_else(|| ProfileError::UnknownLanguage(language.to_owned()))?;
    let matches = |profile: &'static Profile| -> Result<&'static Profile, ProfileError> {
        if profile.language_version == version {
            Ok(profile)
        } else {
            Err(ProfileError::UnknownVersion {
                language: profile.language.as_str(),
                version: version.to_owned(),
            })
        }
    };
    match parsed {
        Language::Archimate => matches(&archimate::PROFILE),
        Language::Uml => matches(&uml::PROFILE),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(profile: &Profile) -> Vec<&'static str> {
        profile.element_kinds.iter().map(|e| e.kind).collect()
    }

    #[test]
    fn archimate_profile_has_30_element_kinds_and_11_relationships() {
        let profile = for_language("archimate", "3.2").expect("archimate 3.2 is curated");
        assert_eq!(profile.element_kinds.len(), 30);
        assert_eq!(profile.relationship_kinds.len(), 11);
        assert_eq!(profile.ruleset, "conveyor-archimate-3.2-v1");
    }

    #[test]
    fn uml_profile_has_10_element_kinds_and_9_relationships() {
        let profile = for_language("uml", "2.5.1").expect("uml 2.5.1 is curated");
        assert_eq!(profile.element_kinds.len(), 10);
        assert_eq!(profile.relationship_kinds.len(), 9);
        assert_eq!(profile.ruleset, "conveyor-uml-2.5.1-v1");
        assert_eq!(profile.view_kinds.len(), 2);
        assert_eq!(profile.diagram_kinds.len(), 5);
    }

    #[test]
    fn no_kind_is_listed_twice() {
        for language in [Language::Archimate, Language::Uml] {
            let profile = match language {
                Language::Archimate => &archimate::PROFILE,
                Language::Uml => &uml::PROFILE,
            };
            let mut seen = std::collections::BTreeSet::new();
            for kind in kinds(profile) {
                assert!(seen.insert(kind), "{kind} is listed twice");
            }
            let mut seen = std::collections::BTreeSet::new();
            for relationship in profile.relationship_kinds {
                assert!(
                    seen.insert(relationship.kind),
                    "{} is listed twice",
                    relationship.kind
                );
            }
        }
    }

    #[test]
    fn every_archimate_element_kind_carries_a_layer() {
        for element in archimate::PROFILE.element_kinds {
            assert!(element.layer.is_some(), "{} has no layer", element.kind);
        }
    }

    #[test]
    fn every_uml_element_kind_carries_no_layer() {
        for element in uml::PROFILE.element_kinds {
            assert!(element.layer.is_none(), "{} has a layer", element.kind);
        }
    }

    #[test]
    fn out_of_profile_kinds_are_absent() {
        let profile = &archimate::PROFILE;
        // Measured present in the registry (61 concepts) and deliberately out of profile.
        for kind in [
            "business_interaction",
            "application_process",
            "technology_function",
            "communication_network",
            "equipment",
            "facility",
            "product",
            "contract",
            "implementation_event",
            "plateau",
            "value",
            "meaning",
            "representation",
        ] {
            assert!(
                !profile.has_element_kind(kind),
                "{kind} must stay out of profile"
            );
        }
        for kind in ["publish", "using", "occurrence"] {
            assert!(!profile.has_relationship_kind(kind));
        }
    }

    #[test]
    fn an_unknown_language_or_version_is_an_error_not_a_default() {
        assert!(matches!(
            for_language("sysml", "1.0"),
            Err(ProfileError::UnknownLanguage(ref language)) if language == "sysml"
        ));
        assert!(matches!(
            for_language("archimate", "4.0"),
            Err(ProfileError::UnknownVersion { .. })
        ));
        assert!(matches!(
            for_language("uml", "2.0"),
            Err(ProfileError::UnknownVersion { .. })
        ));
    }

    #[test]
    fn same_layer_relationships_are_the_four_structural_ones() {
        let profile = &archimate::PROFILE;
        let mut same_layer = Vec::new();
        for relationship in profile.relationship_kinds {
            if matches!(relationship.rule, RelationshipRule::SameLayer) {
                same_layer.push(relationship.kind);
            }
        }
        assert_eq!(
            same_layer,
            vec![
                "composition",
                "aggregation",
                "specialization",
                "association"
            ]
        );
    }
}
