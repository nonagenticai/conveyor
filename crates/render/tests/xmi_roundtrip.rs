//! XMI round-trip and hardening tests (S08 §5.7).
//!
//! The codec promises four things, and every test below names the one it proves:
//!
//! 1. **Completeness** — every in-profile element and relationship appears exactly once, with
//!    an XML id, and nothing is dropped in silence (the `unsupported` list, never a refusal).
//! 2. **Canonical round trip** — export → import yields the same canonical multiset of
//!    `(kind, name, documentation, parent-path)` and `(kind, source-path, target-path)`.
//!    Paths are compared **by name**: the store mints a fresh row id on insert
//!    (`crates/store/src/model.rs::create_element_tx`), so ids do not survive a real import
//!    and must not be part of the contract.
//! 3. **Canonical re-export** — export → import → (store: new ids) → export is canonically
//!    equal to the first export. Byte equality is deliberately NOT asserted: two imports of
//!    the same document receive different row ids, so the `e-<uuid>` values differ by design.
//! 4. **Determinism** — two exports of the same snapshot are byte-identical; the codec is a
//!    pure function of the snapshot.
//!
//! Hardening is asserted separately: a `DOCTYPE`, a duplicate id, an oversize document and
//! one nested past `MAX_DEPTH` are all refused, and every out-of-profile or unknown construct
//! is reported rather than dropped.

mod fixtures;

use std::collections::{BTreeMap, BTreeSet};

use conveyor_domain::model::{
    ModelSnapshot, SnapshotConnection, SnapshotElement, SnapshotModel, SnapshotObject,
    SnapshotRelationship, SnapshotView,
};
use conveyor_domain::profile::{Profile, archimate, uml};
use conveyor_render::xmi::{ARCHIMATE_NS, CONVEYOR_XMI_NS, MAX_DEPTH, MAX_DOCUMENT_BYTES, UML_NS};
use conveyor_render::{ImportedModel, export_xmi, import_xmi};
use serde_json::json;
use uuid::Uuid;

// ── The canonical projection ─────────────────────────────────────────────────────────────
//
// The multiset is compared as sorted tuples of strings, so no hash map is involved and a
// difference is readable in the failure message.

/// `id → (parent, name)` for a snapshot's elements.
fn snapshot_paths(model: &ModelSnapshot) -> BTreeMap<Uuid, (Option<Uuid>, String)> {
    model
        .elements
        .iter()
        .map(|element| {
            (
                element.id,
                (element.parent_element_id, element.name.clone()),
            )
        })
        .collect()
}

/// `id → (parent, name)` for an imported model's elements.
fn imported_paths(imported: &ImportedModel) -> BTreeMap<Uuid, (Option<Uuid>, String)> {
    imported
        .elements
        .iter()
        .map(|element| (element.id, (element.parent_id, element.name.clone())))
        .collect()
}

/// The ancestor names of `id`, root first. A cycle is cut rather than followed.
fn parent_path(paths: &BTreeMap<Uuid, (Option<Uuid>, String)>, id: Uuid) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = BTreeSet::new();
    let mut cursor = paths.get(&id).and_then(|(parent, _)| *parent);
    while let Some(parent) = cursor {
        if !seen.insert(parent) {
            break;
        }
        let Some((grandparent, name)) = paths.get(&parent) else {
            break;
        };
        names.push(name.clone());
        cursor = *grandparent;
    }
    names.reverse();
    names
}

/// The ancestor names followed by the element's own name.
fn element_path(paths: &BTreeMap<Uuid, (Option<Uuid>, String)>, id: Uuid) -> Vec<String> {
    let mut path = parent_path(paths, id);
    if let Some((_, name)) = paths.get(&id) {
        path.push(name.clone());
    }
    path
}

type ElementEntry = (String, String, String, Vec<String>);
type RelationshipEntry = (String, Vec<String>, Vec<String>);

fn snapshot_elements(model: &ModelSnapshot) -> Vec<ElementEntry> {
    let paths = snapshot_paths(model);
    let mut entries: Vec<ElementEntry> = model
        .elements
        .iter()
        .map(|element| {
            (
                element.kind.clone(),
                element.name.clone(),
                element.documentation.clone(),
                parent_path(&paths, element.id),
            )
        })
        .collect();
    entries.sort();
    entries
}

fn imported_elements(imported: &ImportedModel) -> Vec<ElementEntry> {
    let paths = imported_paths(imported);
    let mut entries: Vec<ElementEntry> = imported
        .elements
        .iter()
        .map(|element| {
            (
                element.kind.clone(),
                element.name.clone(),
                element.documentation.clone(),
                parent_path(&paths, element.id),
            )
        })
        .collect();
    entries.sort();
    entries
}

fn snapshot_relationships(model: &ModelSnapshot) -> Vec<RelationshipEntry> {
    let paths = snapshot_paths(model);
    let mut entries: Vec<RelationshipEntry> = model
        .relationships
        .iter()
        .map(|relationship| {
            (
                relationship.kind.clone(),
                element_path(&paths, relationship.source_id),
                element_path(&paths, relationship.target_id),
            )
        })
        .collect();
    entries.sort();
    entries
}

fn imported_relationships(imported: &ImportedModel) -> Vec<RelationshipEntry> {
    let paths = imported_paths(imported);
    let mut entries: Vec<RelationshipEntry> = imported
        .relationships
        .iter()
        .map(|relationship| {
            (
                relationship.kind.clone(),
                element_path(&paths, relationship.source_id),
                element_path(&paths, relationship.target_id),
            )
        })
        .collect();
    entries.sort();
    entries
}

// ── The store's step ─────────────────────────────────────────────────────────────────────

/// Rebuild a snapshot the way the store does: **every row gets a fresh id**, and every
/// reference is rewritten to it. `crates/store/src/model.rs::create_element_tx` takes a
/// `NewElement` with no id, so the database's `DEFAULT uuidv7()` is the only id an imported
/// element ever gets — a re-export therefore cannot be byte-identical to the first export.
fn stored(imported: &ImportedModel, model_id: Uuid) -> ModelSnapshot {
    let mut next = 0xF000_0000_0000_0000_0000_0000_0000_0000_u128;
    let mut fresh = || {
        next += 1;
        Uuid::from_u128(next)
    };
    let element_ids: BTreeMap<Uuid, Uuid> = imported
        .elements
        .iter()
        .map(|element| (element.id, fresh()))
        .collect();
    let relationship_ids: BTreeMap<Uuid, Uuid> = imported
        .relationships
        .iter()
        .map(|relationship| (relationship.id, fresh()))
        .collect();
    let view_ids: BTreeMap<Uuid, Uuid> = imported
        .views
        .iter()
        .map(|view| (view.id, fresh()))
        .collect();
    let object_ids: BTreeMap<Uuid, Uuid> = imported
        .views
        .iter()
        .flat_map(|view| view.objects.iter())
        .map(|object| (object.id, fresh()))
        .collect();
    let connection_ids: BTreeMap<Uuid, Uuid> = imported
        .views
        .iter()
        .flat_map(|view| view.connections.iter())
        .map(|connection| (connection.id, fresh()))
        .collect();

    ModelSnapshot {
        model: SnapshotModel {
            id: model_id,
            language: imported.language.clone(),
            language_version: imported.language_version.clone(),
            title: imported.title.clone(),
            description: imported.description.clone(),
            meta: json!({}),
        },
        elements: imported
            .elements
            .iter()
            .map(|element| SnapshotElement {
                id: element_ids[&element.id],
                kind: element.kind.clone(),
                name: element.name.clone(),
                documentation: element.documentation.clone(),
                parent_element_id: element
                    .parent_id
                    .and_then(|parent| element_ids.get(&parent).copied()),
                meta: json!({}),
            })
            .collect(),
        relationships: imported
            .relationships
            .iter()
            .map(|relationship| SnapshotRelationship {
                id: relationship_ids[&relationship.id],
                kind: relationship.kind.clone(),
                source_id: element_ids[&relationship.source_id],
                target_id: element_ids[&relationship.target_id],
                meta: json!({}),
            })
            .collect(),
        views: imported
            .views
            .iter()
            .map(|view| SnapshotView {
                id: view_ids[&view.id],
                kind: view.kind.clone(),
                name: view.name.clone(),
                meta: view
                    .diagram_kind
                    .as_ref()
                    .map(|kind| json!({ "diagram_kind": kind }))
                    .unwrap_or_else(|| json!({})),
                objects: view
                    .objects
                    .iter()
                    .map(|object| SnapshotObject {
                        id: object_ids[&object.id],
                        element_id: object
                            .element_id
                            .and_then(|element| element_ids.get(&element).copied()),
                        parent_object_id: object
                            .parent_id
                            .and_then(|parent| object_ids.get(&parent).copied()),
                        kind: object.kind.clone(),
                        bounds: object.bounds.clone(),
                        style: json!({}),
                        label: object.label.clone(),
                        position: object.position,
                    })
                    .collect(),
                connections: view
                    .connections
                    .iter()
                    .map(|connection| SnapshotConnection {
                        id: connection_ids[&connection.id],
                        relationship_id: connection
                            .relationship_id
                            .and_then(|relationship| relationship_ids.get(&relationship).copied()),
                        source_object_id: object_ids[&connection.source_id],
                        target_object_id: object_ids[&connection.target_id],
                        bendpoints: connection.bendpoints.clone(),
                        style: json!({}),
                    })
                    .collect(),
            })
            .collect(),
    }
}

// ── Guarantee 4: determinism ────────────────────────────────────────────────────────────

/// Guarantee 4 — two exports of one snapshot are byte-identical.
#[test]
fn export_is_deterministic() {
    for (name, model) in fixtures::all() {
        let first = export_xmi(&model).unwrap_or_else(|error| panic!("{name}: {error}"));
        let second = export_xmi(&model).expect("the second export");
        assert_eq!(first.xml, second.xml, "{name} is not deterministic");
        assert_eq!(
            first.unsupported, second.unsupported,
            "{name} reports a different unsupported list"
        );
    }
}

/// Guarantee 1 — the dialect, its namespace and its version follow `model.language`, and each
/// dialect's document has the shape the on-disk authorities write.
#[test]
fn the_dialect_follows_the_model_language() {
    let archimate = export_xmi(&fixtures::model_a()).expect("archimate export");
    assert_eq!(archimate.namespace, ARCHIMATE_NS);
    assert_eq!(archimate.version, "3.2");
    assert!(
        archimate
            .xml
            .contains("xmlns=\"http://www.opengroup.org/xsd/archimate/3.0/\""),
        "{}",
        archimate.xml
    );
    assert!(
        archimate.xml.contains("xsi:type=\"BusinessActor\""),
        "the default namespace is ArchiMate, so xsi:type is unprefixed"
    );
    assert!(
        archimate.xml.contains("<elements>") && archimate.xml.contains("<relationships>"),
        "{}",
        archimate.xml
    );

    let uml = export_xmi(&fixtures::model_c()).expect("uml export");
    assert_eq!(uml.namespace, UML_NS);
    assert_eq!(uml.version, "2.5.1");
    assert!(
        uml.xml
            .contains("xmlns:xmi=\"http://www.omg.org/spec/XMI/20131001\""),
        "{}",
        uml.xml
    );
    assert!(
        uml.xml
            .contains("xmlns:uml=\"http://www.omg.org/spec/UML/20161101\""),
        "{}",
        uml.xml
    );
    assert!(uml.xml.contains("xmi:type=\"uml:Class\""), "{}", uml.xml);
}

// ── Guarantee 2: the canonical round trip ───────────────────────────────────────────────

/// Guarantee 2 (ArchiMate) — export → import reproduces the fixture's element and
/// relationship multisets, and the stored view's objects and connectors survive.
#[test]
fn archimate_round_trip_is_canonically_equal() {
    let model = fixtures::model_a();
    let exported = export_xmi(&model).expect("export fixture A");
    assert!(
        exported.unsupported.is_empty(),
        "fixture A is entirely in profile: {:?}",
        exported.unsupported
    );

    let imported = import_xmi(&exported.xml).expect("import fixture A");
    assert!(
        imported.unsupported.is_empty(),
        "nothing in fixture A is out of profile: {:?}",
        imported.unsupported
    );
    assert_eq!(imported.language, "archimate");
    assert_eq!(imported.language_version, "3.2");
    assert_eq!(imported.title, model.model.title);
    assert_eq!(snapshot_elements(&model), imported_elements(&imported));
    assert_eq!(
        snapshot_relationships(&model),
        imported_relationships(&imported)
    );

    let view_id = fixtures::view_of(&model).expect("fixture A stores exactly one view");
    assert_eq!(imported.views.len(), 1, "the stored view survives");
    assert_eq!(imported.views[0].kind, "archimate_view");
    assert_eq!(
        imported.views[0].name,
        model
            .views
            .iter()
            .find(|view| view.id == view_id)
            .expect("the fixture's view")
            .name
    );
    assert_eq!(
        imported.views[0].objects.len(),
        model.views[0].objects.len()
    );
    assert_eq!(
        imported.views[0].connections.len(),
        model.views[0].connections.len()
    );
}

/// Guarantee 2 (UML) — the same multiset proof for the second language, whose relationship
/// grammar is entirely different (nested generalizations, associations with owned ends,
/// client/supplier dependencies).
#[test]
fn uml_round_trip_is_canonically_equal() {
    let model = fixtures::model_c();
    let exported = export_xmi(&model).expect("export fixture C");
    assert!(
        exported.unsupported.is_empty(),
        "fixture C is entirely in profile: {:?}",
        exported.unsupported
    );

    let imported = import_xmi(&exported.xml).expect("import fixture C");
    assert!(
        imported.unsupported.is_empty(),
        "nothing in fixture C is out of profile: {:?}",
        imported.unsupported
    );
    assert_eq!(imported.language, "uml");
    assert_eq!(imported.language_version, "2.5.1");
    assert_eq!(imported.title, model.model.title);
    assert_eq!(snapshot_elements(&model), imported_elements(&imported));
    assert_eq!(
        snapshot_relationships(&model),
        imported_relationships(&imported)
    );

    let view_id = fixtures::view_of(&model).expect("fixture C stores exactly one view");
    assert_eq!(imported.views.len(), 1, "the class diagram survives");
    assert_eq!(imported.views[0].kind, "uml_diagram");
    assert_eq!(imported.views[0].diagram_kind.as_deref(), Some("class"));
    assert_eq!(
        imported.views[0].name,
        model
            .views
            .iter()
            .find(|view| view.id == view_id)
            .expect("the fixture's view")
            .name
    );
    assert_eq!(
        imported.views[0].objects.len(),
        model.views[0].objects.len()
    );
    assert_eq!(
        imported.views[0].connections.len(),
        model.views[0].connections.len()
    );
}

/// Guarantee 3 — export → import → **store (new ids)** → export is canonically equal to the
/// first export. Byte equality is not asserted and must not be: the store mints the row ids.
#[test]
fn a_re_export_after_the_store_is_canonically_equal() {
    for (name, model) in fixtures::all() {
        let first = export_xmi(&model).unwrap_or_else(|error| panic!("{name}: {error}"));
        let imported = import_xmi(&first.xml).unwrap_or_else(|error| panic!("{name}: {error}"));
        let rebuilt = stored(&imported, model.model.id);
        let second = export_xmi(&rebuilt).unwrap_or_else(|error| panic!("{name}: {error}"));
        let reimported = import_xmi(&second.xml).unwrap_or_else(|error| panic!("{name}: {error}"));

        assert_eq!(
            snapshot_elements(&model),
            imported_elements(&reimported),
            "{name}: elements drifted through the store"
        );
        assert_eq!(
            snapshot_relationships(&model),
            imported_relationships(&reimported),
            "{name}: relationships drifted through the store"
        );

        // Prove the second export really is a different document, so the canonical comparison
        // above is doing the work and a byte comparison would have failed.
        let first_ids: BTreeSet<Uuid> =
            imported.elements.iter().map(|element| element.id).collect();
        let second_ids: BTreeSet<Uuid> = reimported
            .elements
            .iter()
            .map(|element| element.id)
            .collect();
        assert!(
            first_ids.is_disjoint(&second_ids),
            "{name}: the store must mint new element ids, or this test is not proving what it \
             claims"
        );
    }
}

// ── Guarantee 1/4: what cannot be carried is reported ───────────────────────────────────

/// Guarantee 2, the parent-path half — UML carries containment by nesting `packagedElement`,
/// so a nested element's parent path survives the round trip. The names and the description
/// carry XML metacharacters, which exercises the escape/unescape path in both character data
/// and attribute values.
#[test]
fn uml_nesting_and_special_characters_survive_the_round_trip() {
    let package = Uuid::from_u128(0x5101);
    let class = Uuid::from_u128(0x5102);
    let model = ModelSnapshot {
        model: SnapshotModel {
            id: Uuid::from_u128(0x5100),
            language: "uml".to_owned(),
            language_version: "2.5.1".to_owned(),
            title: "Nesting & <escaping>".to_owned(),
            description: "A & B \"quoted\"".to_owned(),
            meta: json!({}),
        },
        elements: vec![
            SnapshotElement {
                id: package,
                kind: "package".to_owned(),
                name: "root & <hub>".to_owned(),
                documentation: String::new(),
                parent_element_id: None,
                meta: json!({}),
            },
            SnapshotElement {
                id: class,
                kind: "class".to_owned(),
                name: "A \"quoted\" & <angled>".to_owned(),
                documentation: "Doc & <text>".to_owned(),
                parent_element_id: Some(package),
                meta: json!({}),
            },
        ],
        relationships: Vec::new(),
        views: Vec::new(),
    };

    let exported = export_xmi(&model).expect("export the nested model");
    assert!(
        exported.unsupported.is_empty(),
        "{:?}",
        exported.unsupported
    );
    assert!(
        exported.xml.contains("&amp;") && exported.xml.contains("&lt;hub&gt;"),
        "character data and attribute values are escaped: {}",
        exported.xml
    );

    let imported = import_xmi(&exported.xml).expect("import the nested model");
    assert!(
        imported.unsupported.is_empty(),
        "{:?}",
        imported.unsupported
    );
    assert_eq!(snapshot_elements(&model), imported_elements(&imported));
    assert_eq!(imported.title, model.model.title);
    assert_eq!(imported.description, model.model.description);
    let nested = imported
        .elements
        .iter()
        .find(|element| element.name.starts_with("A "))
        .expect("the nested class");
    assert_eq!(
        nested.parent_id,
        Some(package),
        "the class is nested in the package"
    );
}

/// Guarantee 4 — the ArchiMate exchange format has no containment slot for a concept, so a
/// `parent_element_id` is reported rather than silently dropped. (UML nests; ArchiMate's
/// `ElementType` is `ConceptType` plus nothing.)
#[test]
fn archimate_element_nesting_is_reported() {
    let parent = Uuid::from_u128(0x5201);
    let child = Uuid::from_u128(0x5202);
    let model = ModelSnapshot {
        model: SnapshotModel {
            id: Uuid::from_u128(0x5200),
            language: "archimate".to_owned(),
            language_version: "3.2".to_owned(),
            title: "Containment".to_owned(),
            description: String::new(),
            meta: json!({}),
        },
        elements: vec![
            SnapshotElement {
                id: parent,
                kind: "grouping".to_owned(),
                name: "Whole".to_owned(),
                documentation: String::new(),
                parent_element_id: None,
                meta: json!({}),
            },
            SnapshotElement {
                id: child,
                kind: "business_actor".to_owned(),
                name: "Part".to_owned(),
                documentation: String::new(),
                parent_element_id: Some(parent),
                meta: json!({}),
            },
        ],
        relationships: Vec::new(),
        views: Vec::new(),
    };

    let exported = export_xmi(&model).expect("export");
    let child_id = format!("e-{child}");
    let entry = exported
        .unsupported
        .iter()
        .find(|entry| entry.construct == "element" && entry.id == child_id)
        .unwrap_or_else(|| panic!("the parent link is reported: {:?}", exported.unsupported));
    assert!(
        entry.reason.contains("parent_element_id"),
        "{:?}",
        entry.reason
    );

    let imported = import_xmi(&exported.xml).expect("import");
    assert_eq!(
        imported.elements.len(),
        2,
        "both elements are still carried, flat"
    );
    assert!(
        imported
            .elements
            .iter()
            .all(|element| element.parent_id.is_none())
    );
}

/// Guarantee 1 — a containment cycle is broken and reported, rather than writing a document
/// with no root and silently dropping every element in the cycle.
#[test]
fn a_containment_cycle_is_broken_and_reported() {
    let first = Uuid::from_u128(0x6101);
    let second = Uuid::from_u128(0x6102);
    let element = |id: Uuid, name: &str, parent: Uuid| SnapshotElement {
        id,
        kind: "class".to_owned(),
        name: name.to_owned(),
        documentation: String::new(),
        parent_element_id: Some(parent),
        meta: json!({}),
    };
    let model = ModelSnapshot {
        model: SnapshotModel {
            id: Uuid::from_u128(0x6100),
            language: "uml".to_owned(),
            language_version: "2.5.1".to_owned(),
            title: "Cyclic containment".to_owned(),
            description: String::new(),
            meta: json!({}),
        },
        elements: vec![
            element(first, "First", second),
            element(second, "Second", first),
        ],
        relationships: Vec::new(),
        views: Vec::new(),
    };

    let exported = export_xmi(&model).expect("export");
    for id in [first, second] {
        let identifier = format!("e-{id}");
        assert!(
            exported
                .unsupported
                .iter()
                .any(|entry| entry.construct == "element"
                    && entry.id == identifier
                    && entry.reason.contains("cycle")),
            "the cycle is reported for {identifier}: {:?}",
            exported.unsupported
        );
        assert_eq!(
            exported.xml.matches(&identifier).count(),
            1,
            "the element is still written exactly once"
        );
    }

    let imported = import_xmi(&exported.xml).expect("import");
    assert_eq!(imported.elements.len(), 2);
    assert!(
        imported
            .elements
            .iter()
            .all(|element| element.parent_id.is_none()),
        "both ends of the cycle become roots"
    );
}

/// Guarantee 1 and the edge rule — an out-of-profile element is skipped and reported, and so
/// is every relationship whose endpoint was skipped. Nothing is lost in silence.
#[test]
fn an_out_of_profile_kind_is_reported_and_its_edges_are_reported_too() {
    let model = fixtures::model_a();
    let exported = export_xmi(&model).expect("export fixture A");
    let goal = Uuid::from_u128(0xA1);
    let goal_id = format!("e-{goal}");
    assert!(
        exported.xml.contains(&goal_id),
        "the goal element is in the document"
    );

    // `Goal` appears exactly once in fixture A's document, so this tampers with one element.
    let tampered = exported
        .xml
        .replace("xsi:type=\"Goal\"", "xsi:type=\"PhysicalEquipment\"");
    assert_ne!(tampered, exported.xml, "the tamper must hit the document");

    let imported = import_xmi(&tampered).expect("the rest of the document still imports");
    assert_eq!(
        imported.elements.len(),
        model.elements.len() - 1,
        "the out-of-profile element is skipped"
    );

    let element = imported
        .unsupported
        .iter()
        .find(|entry| entry.construct == "element" && entry.id == goal_id)
        .unwrap_or_else(|| {
            panic!(
                "the element is reported with its id: {:?}",
                imported.unsupported
            )
        });
    assert!(
        element.reason.contains("PhysicalEquipment"),
        "the reason names the type: {:?}",
        element.reason
    );

    // `influence` (requirement → goal) and `association` (stakeholder → goal) both end at the
    // skipped element. An edge must never vanish without a report.
    for relationship in [Uuid::from_u128(0xAF), Uuid::from_u128(0xB0)] {
        let id = format!("r-{relationship}");
        assert!(
            imported
                .unsupported
                .iter()
                .any(|entry| entry.construct == "relationship" && entry.id == id),
            "the edge {id} must be reported: {:?}",
            imported.unsupported
        );
    }
    assert_eq!(
        imported.relationships.len(),
        model.relationships.len() - 2,
        "the two edges into the skipped element are not carried"
    );
}

/// Guarantee 1 — an unknown view construct, node type and connection type are each reported,
/// and the rest of the view still imports.
#[test]
fn unknown_view_constructs_are_reported() {
    let document = r#"<?xml version="1.0" encoding="UTF-8"?>
<model xmlns="http://www.opengroup.org/xsd/archimate/3.0/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" identifier="id-00000000-0000-0000-0000-000000000200">
  <elements>
    <element identifier="e-00000000-0000-0000-0000-000000000201" xsi:type="BusinessActor"><name xml:lang="en">A</name></element>
  </elements>
  <views>
    <diagrams>
      <view identifier="v-00000000-0000-0000-0000-000000000202" xsi:type="Diagram">
        <node identifier="o-00000000-0000-0000-0000-000000000203" xsi:type="Element" elementRef="e-00000000-0000-0000-0000-000000000201" x="0" y="0" w="10" h="10"/>
        <node identifier="o-00000000-0000-0000-0000-000000000204" xsi:type="MagicNode"/>
        <connection identifier="c-00000000-0000-0000-0000-000000000205" xsi:type="Teleport" source="o-00000000-0000-0000-0000-000000000203" target="o-00000000-0000-0000-0000-000000000203"/>
      </view>
    </diagrams>
    <widgets/>
  </views>
</model>"#;

    let imported = import_xmi(document).expect("the known constructs import");
    assert_eq!(imported.views.len(), 1);
    assert_eq!(
        imported.views[0].objects.len(),
        1,
        "only the known node survives"
    );
    assert!(
        imported
            .unsupported
            .iter()
            .any(|entry| entry.construct == "object"
                && entry.id == "o-00000000-0000-0000-0000-000000000204"),
        "{:?}",
        imported.unsupported
    );
    assert!(
        imported
            .unsupported
            .iter()
            .any(|entry| entry.construct == "connection"
                && entry.id == "c-00000000-0000-0000-0000-000000000205"),
        "{:?}",
        imported.unsupported
    );
    assert!(
        imported
            .unsupported
            .iter()
            .any(|entry| entry.construct == "view" && entry.reason.contains("widgets")),
        "{:?}",
        imported.unsupported
    );
}

// ── The hardening rules ─────────────────────────────────────────────────────────────────

/// A `DOCTYPE` is refused outright — the codec never reads a DTD.
#[test]
fn a_doctype_is_refused() {
    let exported = export_xmi(&fixtures::model_a()).expect("export");
    let doctype = exported.xml.replacen(
        "?>",
        "?>\n<!DOCTYPE model [<!ENTITY xxe SYSTEM \"file:///etc/passwd\">]>",
        1,
    );
    assert_ne!(doctype, exported.xml, "the DOCTYPE must be in the document");
    let error = import_xmi(&doctype).expect_err("a DOCTYPE must be refused");
    assert!(error.to_string().contains("DOCTYPE"), "{error}");
}

/// A duplicate id is refused: an `xs:ID` is document-wide unique.
#[test]
fn a_duplicate_id_is_refused() {
    let document = r#"<?xml version="1.0" encoding="UTF-8"?>
<model xmlns="http://www.opengroup.org/xsd/archimate/3.0/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" identifier="id-00000000-0000-0000-0000-000000000300">
  <elements>
    <element identifier="e-00000000-0000-0000-0000-000000000301" xsi:type="BusinessActor"><name xml:lang="en">A</name></element>
    <element identifier="e-00000000-0000-0000-0000-000000000301" xsi:type="BusinessRole"><name xml:lang="en">B</name></element>
  </elements>
</model>"#;
    let error = import_xmi(document).expect_err("a duplicate id must be refused");
    assert!(error.to_string().contains("duplicate"), "{error}");
}

/// A document over `MAX_DOCUMENT_BYTES` is refused before anything is allocated for it.
#[test]
fn an_oversize_document_is_refused() {
    let document = "x".repeat(MAX_DOCUMENT_BYTES + 1);
    let error = import_xmi(&document).expect_err("over the cap");
    assert!(error.to_string().contains("cap"), "{error}");
}

/// A document nested past `MAX_DEPTH` is refused.
#[test]
fn a_document_nested_too_deeply_is_refused() {
    let depth = MAX_DEPTH + 8;
    let mut document = String::from(
        "<?xml version=\"1.0\"?><model xmlns=\"http://www.opengroup.org/xsd/archimate/3.0/\" \
         identifier=\"id-1\">",
    );
    for _ in 0..depth {
        document.push_str("<element>");
    }
    for _ in 0..depth {
        document.push_str("</element>");
    }
    document.push_str("</model>");
    let error = import_xmi(&document).expect_err("too deep");
    assert!(error.to_string().contains("deeper"), "{error}");
}

// ── Ids ─────────────────────────────────────────────────────────────────────────────────

/// Foreign ids are mapped through the codec namespace, and the mapping is a pure function:
/// every relationship endpoint and view connector resolves to the element the document meant,
/// and parsing the same document twice in one process gives the same ids. (The row id is
/// minted by the store on insert; that is what a real import changes, not the codec.)
#[test]
fn foreign_ids_resolve_to_the_elements_the_document_meant() {
    let document = r#"<?xml version="1.0" encoding="UTF-8"?>
<model xmlns="http://www.opengroup.org/xsd/archimate/3.0/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" identifier="id-model">
  <elements>
    <element identifier="id-1a2b3c" xsi:type="BusinessActor"><name xml:lang="en">Source</name></element>
    <element identifier="id-9z" xsi:type="BusinessRole"><name xml:lang="en">Target</name></element>
  </elements>
  <relationships>
    <relationship identifier="id-edge" source="id-1a2b3c" target="id-9z" xsi:type="ServingRelationship"/>
  </relationships>
  <views>
    <diagrams>
      <view identifier="id-view" xsi:type="Diagram">
        <node identifier="id-a" xsi:type="Element" elementRef="id-1a2b3c" x="0" y="0" w="10" h="10"/>
        <node identifier="id-b" xsi:type="Element" elementRef="id-9z" x="20" y="0" w="10" h="10"/>
        <connection identifier="id-c" xsi:type="Relationship" relationshipRef="id-edge" source="id-a" target="id-b"/>
      </view>
    </diagrams>
  </views>
</model>"#;

    let first = import_xmi(document).expect("import");
    let second = import_xmi(document).expect("import again");
    assert_eq!(
        first, second,
        "the id mapping is a pure function of the document"
    );

    let source = first
        .elements
        .iter()
        .find(|element| element.name == "Source")
        .expect("the source element");
    let target = first
        .elements
        .iter()
        .find(|element| element.name == "Target")
        .expect("the target element");
    assert_eq!(
        source.id,
        Uuid::new_v5(&CONVEYOR_XMI_NS, b"id-1a2b3c"),
        "a foreign id is hashed through the codec namespace"
    );
    assert_eq!(first.relationships.len(), 1);
    assert_eq!(
        first.relationships[0].source_id, source.id,
        "the endpoint resolves to the element the document meant"
    );
    assert_eq!(first.relationships[0].target_id, target.id);
    assert_eq!(first.relationships[0].kind, "serving");

    assert_eq!(first.views.len(), 1);
    let objects = &first.views[0].objects;
    assert_eq!(objects.len(), 2);
    assert_eq!(objects[0].element_id, Some(source.id));
    assert_eq!(objects[1].element_id, Some(target.id));
    assert_eq!(
        first.views[0].connections[0].relationship_id,
        Some(first.relationships[0].id),
        "a connector resolves to the relationship it draws"
    );
}

// ── Guarantee 1: the tables cover the profiles ──────────────────────────────────────────

fn profile_model(profile: &Profile) -> ModelSnapshot {
    let elements: Vec<SnapshotElement> = profile
        .element_kinds
        .iter()
        .enumerate()
        .map(|(index, entry)| SnapshotElement {
            id: Uuid::from_u128(0x1000 + index as u128),
            kind: entry.kind.to_owned(),
            name: entry.kind.to_owned(),
            documentation: String::new(),
            parent_element_id: None,
            meta: json!({}),
        })
        .collect();
    let source_id = elements[0].id;
    let target_id = elements[1].id;
    let relationships: Vec<SnapshotRelationship> = profile
        .relationship_kinds
        .iter()
        .enumerate()
        .map(|(index, entry)| SnapshotRelationship {
            id: Uuid::from_u128(0x2000 + index as u128),
            kind: entry.kind.to_owned(),
            source_id,
            target_id,
            meta: json!({}),
        })
        .collect();
    ModelSnapshot {
        model: SnapshotModel {
            id: Uuid::from_u128(0x99),
            language: profile.language.as_str().to_owned(),
            language_version: profile.language_version.to_owned(),
            title: "Profile coverage".to_owned(),
            description: String::new(),
            meta: json!({}),
        },
        elements,
        relationships,
        views: Vec::new(),
    }
}

/// Guarantee 1 and the anti-drift rule — every kind in **both** profiles has an entry in the
/// kind ↔ type table, and round-trips through a document. A kind added to a profile without a
/// mapping is reported as unsupported and dropped, so this fails rather than letting the
/// table and the profile diverge silently.
#[test]
fn every_profile_kind_has_an_xmi_type_and_round_trips() {
    for profile in [&archimate::PROFILE, &uml::PROFILE] {
        let language = profile.language.as_str();
        let model = profile_model(profile);
        let exported =
            export_xmi(&model).unwrap_or_else(|error| panic!("{language} export: {error}"));
        assert!(
            exported.unsupported.is_empty(),
            "{language}: every profile kind must have an XMI type: {:?}",
            exported.unsupported
        );
        let imported =
            import_xmi(&exported.xml).unwrap_or_else(|error| panic!("{language} import: {error}"));
        assert!(
            imported.unsupported.is_empty(),
            "{language}: nothing in a profile model is out of profile: {:?}",
            imported.unsupported
        );

        let expected: BTreeSet<&str> = profile
            .element_kinds
            .iter()
            .map(|entry| entry.kind)
            .collect();
        let actual: BTreeSet<&str> = imported
            .elements
            .iter()
            .map(|element| element.kind.as_str())
            .collect();
        assert_eq!(
            expected, actual,
            "{language}: every element kind must be written and read back"
        );

        let expected: BTreeSet<&str> = profile
            .relationship_kinds
            .iter()
            .map(|entry| entry.kind)
            .collect();
        let actual: BTreeSet<&str> = imported
            .relationships
            .iter()
            .map(|relationship| relationship.kind.as_str())
            .collect();
        assert_eq!(
            expected, actual,
            "{language}: every relationship kind must be written and read back"
        );
    }
}
