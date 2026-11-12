//! The three fixture models every render test shares.
//!
//! ⛔ **Every id here is a fixed constant.** `Uuid::now_v7()` would renumber every SVG id per
//! run and churn every golden byte, which is exactly the defect the determinism contract
//! exists to prevent.
//!
//! The three cover the three drawing paths:
//!
//! * **A** — an ArchiMate model with a **stored view**: objects with authored bounds and
//!   connections with authored bendpoints. This is the path that must draw what the user
//!   arranged.
//! * **B** — an ArchiMate technology model with **no views**: the synthetic path, placed by
//!   [`conveyor_domain::model::layout`].
//! * **C** — a UML class model with a class view, so the second language and its relationship
//!   grammar are drawn, not just the first.

use conveyor_domain::model::{
    ModelSnapshot, SnapshotConnection, SnapshotElement, SnapshotModel, SnapshotObject,
    SnapshotRelationship, SnapshotView,
};
use serde_json::json;
use uuid::Uuid;

/// A fixed uuid.
const fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn element(id: Uuid, kind: &str, name: &str, documentation: &str) -> SnapshotElement {
    SnapshotElement {
        id,
        kind: kind.to_owned(),
        name: name.to_owned(),
        documentation: documentation.to_owned(),
        parent_element_id: None,
        meta: json!({}),
    }
}

fn relationship(id: Uuid, kind: &str, source: Uuid, target: Uuid) -> SnapshotRelationship {
    SnapshotRelationship {
        id,
        kind: kind.to_owned(),
        source_id: source,
        target_id: target,
        meta: json!({}),
    }
}

fn object(
    id: Uuid,
    element: Option<Uuid>,
    label: Option<&str>,
    x: f64,
    y: f64,
    position: f64,
) -> SnapshotObject {
    SnapshotObject {
        id,
        element_id: element,
        parent_object_id: None,
        kind: "element".to_owned(),
        bounds: json!({"x": x, "y": y, "w": 180.0, "h": 90.0}),
        style: json!({}),
        label: label.map(str::to_owned),
        position,
    }
}

fn model(id: Uuid, language: &str, version: &str, title: &str) -> SnapshotModel {
    SnapshotModel {
        id,
        language: language.to_owned(),
        language_version: version.to_owned(),
        title: title.to_owned(),
        description: String::new(),
        meta: json!({}),
    }
}

/// ArchiMate 3.2, ten elements, seven relationships, one stored view.
pub fn model_a() -> ModelSnapshot {
    let goal = id(0xA1);
    let requirement = id(0xA2);
    let stakeholder = id(0xA3);
    let capability = id(0xA4);
    let course = id(0xA5);
    let component = id(0xA6);
    let service = id(0xA7);
    let data = id(0xA8);
    let process = id(0xA9);
    let actor = id(0xAA);

    let elements = vec![
        element(
            goal,
            "goal",
            "Reduce onboarding to one day",
            "The outcome this change is for.",
        ),
        element(
            requirement,
            "requirement",
            "Self-service workspace provisioning",
            "",
        ),
        element(stakeholder, "stakeholder", "Platform Engineering", ""),
        element(capability, "capability", "Workspace lifecycle", ""),
        element(course, "course_of_action", "Automate provisioning", ""),
        element(component, "application_component", "Conveyor", ""),
        element(service, "application_service", "Workspace API", ""),
        element(data, "data_object", "Workspace record", ""),
        element(process, "business_process", "Onboarding", ""),
        element(actor, "business_actor", "New engineer", ""),
    ];

    let relationships = vec![
        relationship(id(0xAB), "realization", service, capability),
        relationship(id(0xAC), "serving", component, service),
        relationship(id(0xAD), "access", component, data),
        relationship(id(0xAE), "composition", capability, course),
        relationship(id(0xAF), "influence", requirement, goal),
        relationship(id(0xB0), "association", stakeholder, goal),
        relationship(id(0xB1), "triggering", process, component),
    ];

    // A stored arrangement: three rows of boxes. The two connected pairs sit next to each
    // other, so the connectors are short and read as the relationships they are — a golden
    // diagram with a line across the page would be a bad example, not a harder test.
    let objects = vec![
        object(id(0xB2), Some(stakeholder), None, 0.0, 0.0, 0.0),
        object(id(0xB3), Some(goal), None, 240.0, 0.0, 1.0),
        object(id(0xB4), Some(requirement), None, 480.0, 0.0, 2.0),
        object(id(0xB5), Some(capability), None, 0.0, 150.0, 3.0),
        object(id(0xB6), Some(course), None, 240.0, 150.0, 4.0),
        object(id(0xB7), Some(component), None, 480.0, 150.0, 5.0),
        object(id(0xB8), Some(process), None, 0.0, 300.0, 6.0),
        object(id(0xB9), Some(data), None, 240.0, 300.0, 7.0),
        object(id(0xBA), Some(service), None, 480.0, 300.0, 8.0),
        object(id(0xBB), Some(actor), None, 240.0, 450.0, 9.0),
    ];

    let connections = vec![
        // `Conveyor serves Workspace API` — the serving pair is stacked in the same column.
        SnapshotConnection {
            id: id(0xBC),
            relationship_id: Some(id(0xAC)),
            source_object_id: id(0xB7),
            target_object_id: id(0xBA),
            bendpoints: json!([]),
            style: json!({}),
        },
        // `Workspace lifecycle is composed of Automate provisioning` — side by side.
        SnapshotConnection {
            id: id(0xBD),
            relationship_id: Some(id(0xAE)),
            source_object_id: id(0xB5),
            target_object_id: id(0xB6),
            bendpoints: json!([]),
            style: json!({}),
        },
    ];

    ModelSnapshot {
        model: model(id(0xA0), "archimate", "3.2", "Onboarding architecture"),
        elements,
        relationships,
        views: vec![SnapshotView {
            id: id(0xBE),
            kind: "archimate_view".to_owned(),
            name: "Onboarding".to_owned(),
            meta: json!({}),
            objects,
            connections,
        }],
    }
}

/// ArchiMate 3.2 technology layer with **no view** — the synthetic path.
pub fn model_b() -> ModelSnapshot {
    let worker = id(0xC1);
    let host = id(0xC2);
    let container = id(0xC3);
    let registry = id(0xC4);
    let image = id(0xC5);
    let ci = id(0xC6);

    ModelSnapshot {
        model: model(id(0xC0), "archimate", "3.2", "Delivery platform"),
        elements: vec![
            element(host, "node", "strato-worker3", ""),
            element(worker, "node", "hp2", ""),
            element(container, "system_software", "containerd", ""),
            element(registry, "technology_service", "Registry API", ""),
            element(image, "artifact", "conveyor-server image", ""),
            element(ci, "technology_service", "CI runners", ""),
        ],
        relationships: vec![
            relationship(id(0xC7), "composition", host, container),
            relationship(id(0xC8), "composition", host, image),
            relationship(id(0xC9), "serving", registry, host),
            relationship(id(0xCA), "flow", image, registry),
            relationship(id(0xCB), "serving", ci, worker),
        ],
        views: Vec::new(),
    }
}

/// UML 2.5.1 class model with a stored class view.
pub fn model_c() -> ModelSnapshot {
    let entity = id(0xD1);
    let model_ = id(0xD2);
    let revision = id(0xD3);
    let store = id(0xD4);
    let status = id(0xD5);
    let renderer = id(0xD6);
    let package = id(0xD7);

    let elements = vec![
        element(entity, "class", "Entity", "Anything with an identity."),
        element(model_, "class", "Model", ""),
        element(revision, "class", "Revision", ""),
        element(store, "interface", "Store", ""),
        element(status, "enumeration", "Status", ""),
        element(renderer, "component", "Renderer", ""),
        element(package, "package", "governance", ""),
    ];

    let relationships = vec![
        relationship(id(0xD8), "generalization", model_, entity),
        relationship(id(0xD9), "generalization", revision, entity),
        relationship(id(0xDA), "association", model_, revision),
        relationship(id(0xDB), "realization", model_, store),
        relationship(id(0xDC), "dependency", renderer, model_),
    ];

    let objects = vec![
        object(id(0xDD), Some(entity), None, 0.0, 0.0, 0.0),
        object(id(0xDE), Some(model_), None, 240.0, 150.0, 1.0),
        object(id(0xDF), Some(revision), None, 480.0, 150.0, 2.0),
        object(id(0xE0), Some(store), None, 0.0, 300.0, 3.0),
        object(id(0xE1), Some(status), None, 240.0, 300.0, 4.0),
        object(id(0xE2), Some(renderer), None, 480.0, 0.0, 5.0),
    ];

    let connections = vec![
        SnapshotConnection {
            id: id(0xE3),
            relationship_id: Some(id(0xD8)),
            source_object_id: id(0xDE),
            target_object_id: id(0xDD),
            bendpoints: json!([]),
            style: json!({}),
        },
        SnapshotConnection {
            id: id(0xE4),
            relationship_id: Some(id(0xDC)),
            source_object_id: id(0xE2),
            target_object_id: id(0xDE),
            bendpoints: json!([{"x": 570.0, "y": 60.0}]),
            style: json!({}),
        },
    ];

    ModelSnapshot {
        model: model(id(0xD0), "uml", "2.5.1", "Governance model"),
        elements,
        relationships,
        views: vec![SnapshotView {
            id: id(0xE5),
            kind: "uml_diagram".to_owned(),
            name: "Classes".to_owned(),
            meta: json!({"diagram_kind": "class"}),
            objects,
            connections,
        }],
    }
}

/// The view a fixture should be rendered through, when it has one.
pub fn view_of(model: &ModelSnapshot) -> Option<Uuid> {
    model.views.first().map(|view| view.id)
}

/// The three fixtures, named as their golden directory.
pub fn all() -> [(&'static str, ModelSnapshot); 3] {
    [("a", model_a()), ("b", model_b()), ("c", model_c())]
}
