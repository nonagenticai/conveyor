//! `conveyor-uml-2.5.1-v1` — the 10 element kinds and 9 relationship kinds this application
//! accepts from UML 2.5.1.
//!
//! fr's `lib/modeling/uml.ts` lists **25** UML kinds. This profile takes 10; the other 15
//! (activity, state-machine and deployment constructs, most behaviour kinds) are deferred
//! together with the three diagram families that would need them. Adding a kind here is a
//! data change; adding a *family* means revisiting the exchange format as well.
//!
//! Diagram kinds are not `model_view.kind` — that column holds one of three values and is
//! CHECKed by the database. The specific UML diagram is `model_view.meta.diagram_kind`, and
//! when it is present it must be one of [`DIAGRAM_KINDS`].

use super::{ElementKind, Language, Profile, RelationshipKind, RelationshipRule};

/// The accepted element kinds (S08 slice §5.5).
pub static ELEMENT_KINDS: &[ElementKind] = &[
    ElementKind {
        kind: "class",
        layer: None,
    },
    ElementKind {
        kind: "interface",
        layer: None,
    },
    ElementKind {
        kind: "enumeration",
        layer: None,
    },
    ElementKind {
        kind: "datatype",
        layer: None,
    },
    ElementKind {
        kind: "package",
        layer: None,
    },
    ElementKind {
        kind: "component",
        layer: None,
    },
    ElementKind {
        kind: "object",
        layer: None,
    },
    ElementKind {
        kind: "actor",
        layer: None,
    },
    ElementKind {
        kind: "use_case",
        layer: None,
    },
    ElementKind {
        kind: "lifeline",
        layer: None,
    },
];

/// The accepted relationship kinds, with the endpoint restrictions UML actually states.
///
/// `include`/`extend` join use cases to use cases; `message` joins lifelines; `realization`
/// targets an interface or a component. Everything else is unrestricted — multiplicity
/// expressions, combined fragments and OCL are out of v1.
pub static RELATIONSHIP_KINDS: &[RelationshipKind] = &[
    RelationshipKind {
        kind: "association",
        rule: RelationshipRule::Any,
    },
    RelationshipKind {
        kind: "aggregation",
        rule: RelationshipRule::Any,
    },
    RelationshipKind {
        kind: "composition",
        rule: RelationshipRule::Any,
    },
    RelationshipKind {
        kind: "generalization",
        rule: RelationshipRule::Any,
    },
    RelationshipKind {
        kind: "realization",
        rule: RelationshipRule::Kinds {
            sources: None,
            targets: Some(&["interface", "component"]),
        },
    },
    RelationshipKind {
        kind: "dependency",
        rule: RelationshipRule::Any,
    },
    RelationshipKind {
        kind: "include",
        rule: RelationshipRule::Kinds {
            sources: Some(&["use_case"]),
            targets: Some(&["use_case"]),
        },
    },
    RelationshipKind {
        kind: "extend",
        rule: RelationshipRule::Kinds {
            sources: Some(&["use_case"]),
            targets: Some(&["use_case"]),
        },
    },
    RelationshipKind {
        kind: "message",
        rule: RelationshipRule::Kinds {
            sources: Some(&["lifeline"]),
            targets: Some(&["lifeline"]),
        },
    },
];

/// The `model_view.kind` values this language accepts.
pub static VIEW_KINDS: &[&str] = &["diagram", "uml_diagram"];

/// The five UML diagram families, carried in `model_view.meta.diagram_kind`.
pub static DIAGRAM_KINDS: &[&str] = &["class", "package", "component", "use_case", "sequence"];

/// The frozen profile. `language_version` is the UML version, not this table's.
pub static PROFILE: Profile = Profile {
    language: Language::Uml,
    language_version: "2.5.1",
    ruleset: "conveyor-uml-2.5.1-v1",
    ruleset_version: "1",
    element_kinds: ELEMENT_KINDS,
    relationship_kinds: RELATIONSHIP_KINDS,
    view_kinds: VIEW_KINDS,
    diagram_kinds: DIAGRAM_KINDS,
};
