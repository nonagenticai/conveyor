//! `conveyor-archimate-3.2-v1` — the 30 element kinds and 11 relationship kinds this
//! application accepts from ArchiMate 3.2.
//!
//! The registry at `requirements_management/src/data/archimate/3.2/registry.json` holds
//! **61 concepts** across eight layers. This profile takes 30 and defers 31: the whole
//! physical layer (4), seven more business kinds, five more application kinds, nine more
//! technology kinds, three more motivation kinds, two more implementation kinds and one more
//! `other` kind. Every kind below was checked against the registry by name — none was
//! invented, and none is spelled differently.
//!
//! Deferred for a stated reason, not an oversight: collaborations, every event/interaction
//! kind, `product`, `contract`, `representation`, `meaning`, `value`, `implementation_event`,
//! `plateau` and junctions are either unused by fr's render map or model constructs the
//! exchange format cannot round-trip. Adding one later is a data change, not a code change.

use super::{ElementKind, Language, Layer, Profile, RelationshipKind, RelationshipRule};

/// The accepted element kinds, grouped by layer (S08 slice §5.4).
pub static ELEMENT_KINDS: &[ElementKind] = &[
    // Business (6 of 13).
    ElementKind {
        kind: "business_actor",
        layer: Some(Layer::Business),
    },
    ElementKind {
        kind: "business_role",
        layer: Some(Layer::Business),
    },
    ElementKind {
        kind: "business_process",
        layer: Some(Layer::Business),
    },
    ElementKind {
        kind: "business_function",
        layer: Some(Layer::Business),
    },
    ElementKind {
        kind: "business_service",
        layer: Some(Layer::Business),
    },
    ElementKind {
        kind: "business_object",
        layer: Some(Layer::Business),
    },
    // Application (4 of 9).
    ElementKind {
        kind: "application_component",
        layer: Some(Layer::Application),
    },
    ElementKind {
        kind: "application_service",
        layer: Some(Layer::Application),
    },
    ElementKind {
        kind: "application_interface",
        layer: Some(Layer::Application),
    },
    ElementKind {
        kind: "data_object",
        layer: Some(Layer::Application),
    },
    // Technology (4 of 13).
    ElementKind {
        kind: "node",
        layer: Some(Layer::Technology),
    },
    ElementKind {
        kind: "system_software",
        layer: Some(Layer::Technology),
    },
    ElementKind {
        kind: "technology_service",
        layer: Some(Layer::Technology),
    },
    ElementKind {
        kind: "artifact",
        layer: Some(Layer::Technology),
    },
    // Strategy (4 of 4).
    ElementKind {
        kind: "resource",
        layer: Some(Layer::Strategy),
    },
    ElementKind {
        kind: "capability",
        layer: Some(Layer::Strategy),
    },
    ElementKind {
        kind: "course_of_action",
        layer: Some(Layer::Strategy),
    },
    ElementKind {
        kind: "value_stream",
        layer: Some(Layer::Strategy),
    },
    // Motivation (7 of 10).
    ElementKind {
        kind: "stakeholder",
        layer: Some(Layer::Motivation),
    },
    ElementKind {
        kind: "driver",
        layer: Some(Layer::Motivation),
    },
    ElementKind {
        kind: "assessment",
        layer: Some(Layer::Motivation),
    },
    ElementKind {
        kind: "goal",
        layer: Some(Layer::Motivation),
    },
    ElementKind {
        kind: "outcome",
        layer: Some(Layer::Motivation),
    },
    ElementKind {
        kind: "requirement",
        layer: Some(Layer::Motivation),
    },
    ElementKind {
        kind: "principle",
        layer: Some(Layer::Motivation),
    },
    // Implementation and migration (3 of 5).
    ElementKind {
        kind: "work_package",
        layer: Some(Layer::Implementation),
    },
    ElementKind {
        kind: "deliverable",
        layer: Some(Layer::Implementation),
    },
    ElementKind {
        kind: "gap",
        layer: Some(Layer::Implementation),
    },
    // Composite (2 of the 3 `other` concepts).
    ElementKind {
        kind: "grouping",
        layer: Some(Layer::Composite),
    },
    ElementKind {
        kind: "location",
        layer: Some(Layer::Composite),
    },
];

/// The accepted relationship kinds (all 11 in the registry).
///
/// Four are structural and must join two elements of the same layer; the other seven are
/// unrestricted. See the module header: a rule that rejects a legal model is worse than one
/// that misses an illegal edge, and the 30 × 11 × 30 standard table is not transcribed here.
pub static RELATIONSHIP_KINDS: &[RelationshipKind] = &[
    RelationshipKind {
        kind: "composition",
        rule: RelationshipRule::SameLayer,
    },
    RelationshipKind {
        kind: "aggregation",
        rule: RelationshipRule::SameLayer,
    },
    RelationshipKind {
        kind: "assignment",
        rule: RelationshipRule::Any,
    },
    RelationshipKind {
        kind: "realization",
        rule: RelationshipRule::Any,
    },
    RelationshipKind {
        kind: "serving",
        rule: RelationshipRule::Any,
    },
    RelationshipKind {
        kind: "access",
        rule: RelationshipRule::Any,
    },
    RelationshipKind {
        kind: "influence",
        rule: RelationshipRule::Any,
    },
    RelationshipKind {
        kind: "triggering",
        rule: RelationshipRule::Any,
    },
    RelationshipKind {
        kind: "flow",
        rule: RelationshipRule::Any,
    },
    RelationshipKind {
        kind: "specialization",
        rule: RelationshipRule::SameLayer,
    },
    RelationshipKind {
        kind: "association",
        rule: RelationshipRule::SameLayer,
    },
];

/// The `model_view.kind` values this language accepts.
pub static VIEW_KINDS: &[&str] = &["diagram", "archimate_view"];

/// ArchiMate has no named diagram kinds beyond the two above.
pub static DIAGRAM_KINDS: &[&str] = &[];

/// The frozen profile. `language_version` is the ArchiMate version, not this table's.
pub static PROFILE: Profile = Profile {
    language: Language::Archimate,
    language_version: "3.2",
    ruleset: "conveyor-archimate-3.2-v1",
    ruleset_version: "1",
    element_kinds: ELEMENT_KINDS,
    relationship_kinds: RELATIONSHIP_KINDS,
    view_kinds: VIEW_KINDS,
    diagram_kinds: DIAGRAM_KINDS,
};
