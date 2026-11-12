#![forbid(unsafe_code)]
//! XMI — the ArchiMate 3.2 exchange format and UML 2.5.1, as a scoped subset.
//!
//! Both directions are implemented and tested against `crates/render/tests/xmi_roundtrip.rs`
//! (16 tests). What the codec deliberately does **not** carry is listed there and in S08
//! §5.7: ArchiMate has no concept-containment slot (a parent link is reported, both elements
//! are written), ArchiMate carries one diagram type, UML diagrams travel in an
//! `xmi:Extension extender="conveyor"`, and `aggregation`/`composition` are one
//! `uml:Association` distinguished by `aggregation="shared"|"composite"`.
//!
//! # What this codec is
//!
//! A transport adapter between a [`ModelSnapshot`] and XML. It is not a MOF/EMOF implementation:
//! no profile definitions, no stereotypes beyond `meta.stereotype`, no `Difference`/`Copy`
//! envelopes, no diagram interchange. What it guarantees instead (S08 §5.7):
//!
//! 1. every in-profile element and relationship appears **exactly once**, with an XML id;
//! 2. re-importing an export into a fresh model yields an identical canonical multiset of
//!    `(kind, name, documentation, parent-path)` and `(kind, source-path, target-path)`;
//! 3. re-exporting that import is canonically equal;
//! 4. every construct the codec cannot carry is **reported** — never dropped in silence and
//!    never a reason to refuse the document.
//!
//! # Ids
//!
//! ⛔ An `xs:ID`/NCName may not begin with a digit, and a UUID usually does. Every id is
//! therefore prefixed: `e-` element, `r-` relationship, `v-` view, `o-` view object, `c-` view
//! connection. Import strips the prefix; anything else (an Archi export writes `id-1a2b3c`) is
//! mapped through a UUID v5 namespace so re-importing the same file twice produces the same
//! ids. RM's own exporters do the same thing (`_` in `tools/uml/xmi.py`, `id-` in
//! `tools/architecture/exchange.py`).
//!
//! # Parsing is hardened
//!
//! `quick-xml` is a pull parser: it never resolves an external entity. A `DOCTYPE` is refused
//! outright, input size and nesting depth are capped, and a duplicate id is an error — an XMI
//! file is untrusted input.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use conveyor_domain::model::{
    ModelSnapshot, SnapshotConnection, SnapshotElement, SnapshotObject, SnapshotRelationship,
    SnapshotView,
};
use conveyor_domain::profile::{Language, Profile, for_language};
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::RenderError;
use crate::ids;

/// The XMI namespace shared by both dialects.
pub const XMI_NS: &str = "http://www.omg.org/spec/XMI/20131001";
/// The ArchiMate 3.x exchange namespace.
pub const ARCHIMATE_NS: &str = "http://www.opengroup.org/xsd/archimate/3.0/";
/// The UML 2.5.1 namespace.
pub const UML_NS: &str = "http://www.omg.org/spec/UML/20161101";
/// The namespace foreign ids are mapped through.
pub const CONVEYOR_XMI_NS: Uuid = Uuid::from_u128(0x9a2f_0c41_7b3d_4e58_9c11_6f2a_8d47_0b93);

/// The largest XMI document accepted, in bytes.
pub const MAX_DOCUMENT_BYTES: usize = 1024 * 1024;
/// The deepest nesting accepted.
pub const MAX_DEPTH: usize = 256;

/// The XML Schema instance namespace, which carries `xsi:type` in the exchange format.
const XSI_NS: &str = "http://www.w3.org/2001/XMLSchema-instance";
/// The one language every text run is written in.
const LANGUAGE: &str = "en";
/// The `meta.diagram_kind` key a UML view is rebuilt from.
const DIAGRAM_KIND: &str = "diagram_kind";
/// The `xmi:Extension` extender name the UML views travel under.
const EXTENDER: &str = "conveyor";

// ── The two kind ↔ type tables ────────────────────────────────────────────────────────────
//
// These are the whole mapping. A profile kind with no entry here is reported as unsupported on
// export and never written; the `xmi_roundtrip` test walks both profiles so a kind added to a
// profile without a mapping here fails CI instead of disappearing from a document.

/// ArchiMate element kind → the exchange format's `xsi:type`.
const ARCHIMATE_ELEMENT_TYPES: &[(&str, &str)] = &[
    ("business_actor", "BusinessActor"),
    ("business_role", "BusinessRole"),
    ("business_process", "BusinessProcess"),
    ("business_function", "BusinessFunction"),
    ("business_service", "BusinessService"),
    ("business_object", "BusinessObject"),
    ("application_component", "ApplicationComponent"),
    ("application_service", "ApplicationService"),
    ("application_interface", "ApplicationInterface"),
    ("data_object", "DataObject"),
    ("node", "Node"),
    ("system_software", "SystemSoftware"),
    ("technology_service", "TechnologyService"),
    ("artifact", "Artifact"),
    ("resource", "Resource"),
    ("capability", "Capability"),
    ("course_of_action", "CourseOfAction"),
    ("value_stream", "ValueStream"),
    ("stakeholder", "Stakeholder"),
    ("driver", "Driver"),
    ("assessment", "Assessment"),
    ("goal", "Goal"),
    ("outcome", "Outcome"),
    ("requirement", "Requirement"),
    ("principle", "Principle"),
    ("work_package", "WorkPackage"),
    ("deliverable", "Deliverable"),
    ("gap", "Gap"),
    ("grouping", "Grouping"),
    ("location", "Location"),
];

/// ArchiMate relationship kind → the exchange format's `xsi:type`.
const ARCHIMATE_RELATIONSHIP_TYPES: &[(&str, &str)] = &[
    ("composition", "CompositionRelationship"),
    ("aggregation", "AggregationRelationship"),
    ("assignment", "AssignmentRelationship"),
    ("realization", "RealizationRelationship"),
    ("serving", "ServingRelationship"),
    ("access", "AccessRelationship"),
    ("influence", "InfluenceRelationship"),
    ("triggering", "TriggeringRelationship"),
    ("flow", "FlowRelationship"),
    ("specialization", "SpecializationRelationship"),
    ("association", "AssociationRelationship"),
];

/// UML element kind → the `xmi:type` local name (written as `uml:<name>`).
const UML_ELEMENT_TYPES: &[(&str, &str)] = &[
    ("class", "Class"),
    ("interface", "Interface"),
    ("enumeration", "Enumeration"),
    ("datatype", "DataType"),
    ("package", "Package"),
    ("component", "Component"),
    ("object", "Object"),
    ("actor", "Actor"),
    ("use_case", "UseCase"),
    ("lifeline", "Lifeline"),
];

/// UML relationship kind → the `xmi:type` local name.
///
/// `aggregation` and `composition` are both an `uml:Association`; the kind is carried by the
/// `aggregation` attribute of the part end (`shared` / `composite`), which is what the importer
/// reads back.
const UML_RELATIONSHIP_TYPES: &[(&str, &str)] = &[
    ("association", "Association"),
    ("aggregation", "Association"),
    ("composition", "Association"),
    ("generalization", "Generalization"),
    ("realization", "Realization"),
    ("dependency", "Dependency"),
    ("include", "Include"),
    ("extend", "Extend"),
    ("message", "Message"),
];

/// One construct the codec could not carry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Unsupported {
    /// `element`, `relationship`, `view`, `object` or `connection`.
    pub construct: &'static str,
    /// The document's id for it, when it has one.
    pub id: String,
    /// Why it was not carried.
    pub reason: String,
}

/// An exported document and everything that could not be said in it.
#[derive(Clone, Debug)]
pub struct ExportedXmi {
    /// The XML document, UTF-8.
    pub xml: String,
    /// The dialect's namespace URI.
    pub namespace: &'static str,
    /// `3.2` or `2.5.1`.
    pub version: String,
    /// Constructs omitted, with a reason each.
    pub unsupported: Vec<Unsupported>,
}

/// One element read from a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedElement {
    /// The mapped id.
    pub id: Uuid,
    /// The profile kind.
    pub kind: String,
    /// The display name.
    pub name: String,
    /// The documentation, `''` when absent.
    pub documentation: String,
    /// The containment parent, when the document nested it.
    pub parent_id: Option<Uuid>,
}

/// One relationship read from a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedRelationship {
    /// The mapped id.
    pub id: Uuid,
    /// The profile relationship kind.
    pub kind: String,
    /// Source element id.
    pub source_id: Uuid,
    /// Target element id.
    pub target_id: Uuid,
}

/// One view object read from a document.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportedObject {
    /// The mapped id.
    pub id: Uuid,
    /// The element it draws, when it draws one.
    pub element_id: Option<Uuid>,
    /// The containing object, when nested.
    pub parent_id: Option<Uuid>,
    /// `element` when it draws an element.
    pub kind: String,
    /// `{x,y,w,h}` in canvas units.
    pub bounds: serde_json::Value,
    /// An override label.
    pub label: Option<String>,
    /// The paint order.
    pub position: f64,
}

/// One view connection read from a document.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportedConnection {
    /// The mapped id.
    pub id: Uuid,
    /// The relationship it draws, when it draws one.
    pub relationship_id: Option<Uuid>,
    /// The object it starts at.
    pub source_id: Uuid,
    /// The object it ends at.
    pub target_id: Uuid,
    /// `[{x,y}, …]` route points.
    pub bendpoints: serde_json::Value,
}

/// One view read from a document.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportedView {
    /// The mapped id.
    pub id: Uuid,
    /// `diagram`, `archimate_view` or `uml_diagram`.
    pub kind: String,
    /// The display name.
    pub name: String,
    /// The UML diagram family, when the document names one.
    pub diagram_kind: Option<String>,
    /// The objects on the view.
    pub objects: Vec<ImportedObject>,
    /// The connectors between them.
    pub connections: Vec<ImportedConnection>,
}

/// A whole model read from a document.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportedModel {
    /// `archimate` or `uml`.
    pub language: String,
    /// `3.2` or `2.5.1`.
    pub language_version: String,
    /// The model title.
    pub title: String,
    /// The description, `''` when absent.
    pub description: String,
    /// Every element.
    pub elements: Vec<ImportedElement>,
    /// Every relationship.
    pub relationships: Vec<ImportedRelationship>,
    /// Every view.
    pub views: Vec<ImportedView>,
    /// Constructs the codec did not carry.
    pub unsupported: Vec<Unsupported>,
}

// ── Export ────────────────────────────────────────────────────────────────────────────────

/// Serialise a model as XMI.
///
/// Returns the document **and** the list of constructs it could not carry — a caller must
/// surface the list; it is never the codec's job to decide that silence is acceptable.
pub fn export_xmi(model: &ModelSnapshot) -> Result<ExportedXmi, RenderError> {
    let profile =
        for_language(&model.model.language, &model.model.language_version).map_err(|_| {
            RenderError::UnknownProfile {
                language: model.model.language.clone(),
                version: model.model.language_version.clone(),
            }
        })?;
    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);
    let mut unsupported = Vec::new();
    write_declaration(&mut writer)?;
    match profile.language {
        Language::Archimate => write_archimate(model, profile, &mut writer, &mut unsupported)?,
        Language::Uml => write_uml(model, profile, &mut writer, &mut unsupported)?,
    }
    let mut xml = into_string(writer)?;
    // One trailing newline, always: a document that ends without one is a diff waiting to
    // happen, and its presence is a pure function of the writer either way.
    xml.push('\n');
    Ok(ExportedXmi {
        xml,
        namespace: match profile.language {
            Language::Archimate => ARCHIMATE_NS,
            Language::Uml => UML_NS,
        },
        version: profile.language_version.to_owned(),
        unsupported,
    })
}

/// Write the XML declaration.
fn write_declaration(writer: &mut Writer<Vec<u8>>) -> Result<(), RenderError> {
    writer
        .write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
        .map_err(io_error)
}

/// Map a writer's in-memory `Vec<u8>` to the document string.
fn into_string(writer: Writer<Vec<u8>>) -> Result<String, RenderError> {
    String::from_utf8(writer.into_inner())
        .map_err(|error| RenderError::Xmi(format!("the document is not UTF-8: {error}")))
}

/// A write to a `Vec` cannot fail; the signature is `io::Result` only because the writer is
/// generic. Mapping it keeps the codec total rather than panicking on a theoretical error.
fn io_error(error: std::io::Error) -> RenderError {
    RenderError::Xmi(error.to_string())
}

/// Build a start tag.
///
/// `Attribute::from((&str, &str))` escapes the value itself (measured: `"Bells & whistles"`
/// becomes `Bells &amp; whistles`), so escaping here as well would double it. What this adds
/// is the one thing quick-xml does not do: dropping the control characters XML 1.0 cannot
/// carry at all, so a name that arrived from an import cannot make the document unparseable.
fn element(name: &str, attributes: &[(&str, String)]) -> BytesStart<'static> {
    let mut element = BytesStart::new(name.to_owned());
    for (key, value) in attributes {
        element.push_attribute((*key, sanitize(value).as_str()));
    }
    element
}

/// Drop the C0/C1 control characters XML 1.0 forbids, leaving tab/newline/CR alone.
fn sanitize(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\t' | '\n' | '\r'))
        .collect()
}

fn start(
    writer: &mut Writer<Vec<u8>>,
    name: &str,
    attributes: &[(&str, String)],
) -> Result<(), RenderError> {
    writer
        .write_event(Event::Start(element(name, attributes)))
        .map_err(io_error)
}

fn empty(
    writer: &mut Writer<Vec<u8>>,
    name: &str,
    attributes: &[(&str, String)],
) -> Result<(), RenderError> {
    writer
        .write_event(Event::Empty(element(name, attributes)))
        .map_err(io_error)
}

fn end(writer: &mut Writer<Vec<u8>>, name: &str) -> Result<(), RenderError> {
    writer
        .write_event(Event::End(BytesEnd::new(name)))
        .map_err(io_error)
}

/// A text-only element, escaped through [`ids::escape`] so a name from an import cannot break
/// out of the document.
fn text_element(
    writer: &mut Writer<Vec<u8>>,
    name: &str,
    attributes: &[(&str, String)],
    value: &str,
) -> Result<(), RenderError> {
    start(writer, name, attributes)?;
    writer
        .write_event(Event::Text(BytesText::from_escaped(ids::escape(value))))
        .map_err(io_error)?;
    end(writer, name)
}

/// The document identifier of the model — `id-<uuid>`, the spelling both exporters use.
fn document_identifier(id: Uuid) -> String {
    format!("id-{id}")
}

/// `via` and `reverse` are the two directions of a kind ↔ type table.
fn via(table: &'static [(&'static str, &'static str)], kind: &str) -> Option<&'static str> {
    table
        .iter()
        .find(|(name, _)| *name == kind)
        .map(|(_, value)| *value)
}

fn reverse(table: &'static [(&'static str, &'static str)], xmi_type: &str) -> Option<&'static str> {
    table
        .iter()
        .find(|(_, value)| *value == xmi_type)
        .map(|(name, _)| *name)
}

/// The type name without a namespace prefix: `uml:Class` → `Class`, `Class` → `Class`.
fn bare_type(value: &str) -> &str {
    match value.rsplit_once(':') {
        Some((_, local)) => local,
        None => value,
    }
}

fn by_position(left: &SnapshotObject, right: &SnapshotObject) -> Ordering {
    left.position
        .partial_cmp(&right.position)
        .unwrap_or(Ordering::Equal)
        .then_with(|| left.id.cmp(&right.id))
}

fn bounds_of(bounds: &Value) -> (f64, f64, f64, f64) {
    let read = |key: &str| bounds.get(key).and_then(Value::as_f64).unwrap_or(0.0);
    (read("x"), read("y"), read("w"), read("h"))
}

fn points_of(bendpoints: &Value) -> Vec<(f64, f64)> {
    let point = |value: &Value| -> Option<(f64, f64)> {
        Some((value.get("x")?.as_f64()?, value.get("y")?.as_f64()?))
    };
    bendpoints
        .as_array()
        .map(|values| values.iter().filter_map(point).collect())
        .unwrap_or_default()
}

/// The ArchiMate node type for a `view_object.kind`, when the format can carry the kind.
fn archimate_node_type(kind: &str) -> Option<&'static str> {
    match kind {
        "element" => Some("Element"),
        "note" => Some("Label"),
        "group" => Some("Container"),
        _ => None,
    }
}

/// The `view_object.kind` for an ArchiMate node type.
fn archimate_object_kind(node_type: &str) -> Option<&'static str> {
    match node_type {
        "" | "Element" => Some("element"),
        "Label" => Some("note"),
        "Container" => Some("group"),
        _ => None,
    }
}

/// The nearest exported ancestor of `id`, with a note when the chain had to be repaired.
///
/// `all` is every object of the view (so a dropped ancestor can be walked through) and
/// `kept` is the exported set. The chain is walked **whole** before an ancestor is chosen: a
/// cycle between two kept objects would otherwise return a parent for each of them, leave no
/// root, and drop both out of the document without a report.
fn archimate_effective_parent(
    id: Uuid,
    all: &BTreeMap<Uuid, &SnapshotObject>,
    kept: &BTreeMap<Uuid, &SnapshotObject>,
) -> (Option<Uuid>, Option<String>) {
    let chain = |from: Uuid| all.get(&from).and_then(|object| object.parent_object_id);
    let mut seen = BTreeSet::new();
    seen.insert(id);
    let mut cursor = chain(id);
    while let Some(parent) = cursor {
        if !seen.insert(parent) {
            return (
                None,
                Some("nesting cycle broken; the object is placed at the top level".to_owned()),
            );
        }
        cursor = chain(parent);
    }
    let mut note = None;
    let mut cursor = chain(id);
    while let Some(parent) = cursor {
        if kept.contains_key(&parent) {
            return (Some(parent), note);
        }
        note = Some(
            "the containing object was not exported; re-parented to the nearest exported ancestor"
                .to_owned(),
        );
        cursor = chain(parent);
    }
    (None, note)
}

/// The nearest exported ancestor of an element, with the same repair contract as
/// [`archimate_effective_parent`].
fn element_effective_parent(
    id: Uuid,
    all: &BTreeMap<Uuid, &SnapshotElement>,
    kept: &BTreeMap<Uuid, &'static str>,
) -> (Option<Uuid>, Option<String>) {
    let chain = |from: Uuid| all.get(&from).and_then(|element| element.parent_element_id);
    let mut seen = BTreeSet::new();
    seen.insert(id);
    let mut cursor = chain(id);
    while let Some(parent) = cursor {
        if !seen.insert(parent) {
            return (
                None,
                Some("containment cycle broken; the element is placed at the top level".to_owned()),
            );
        }
        cursor = chain(parent);
    }
    let mut note = None;
    let mut cursor = chain(id);
    while let Some(parent) = cursor {
        if kept.contains_key(&parent) {
            return (Some(parent), note);
        }
        note = Some(
            "the containing element was not exported; re-parented to the nearest exported ancestor"
                .to_owned(),
        );
        cursor = chain(parent);
    }
    (None, note)
}

// ── ArchiMate export ─────────────────────────────────────────────────────────────────────

fn write_archimate(
    model: &ModelSnapshot,
    profile: &'static Profile,
    writer: &mut Writer<Vec<u8>>,
    unsupported: &mut Vec<Unsupported>,
) -> Result<(), RenderError> {
    let mut elements: Vec<&SnapshotElement> = model.elements.iter().collect();
    elements.sort_by_key(|element| element.id);

    let mut element_types: BTreeMap<Uuid, &'static str> = BTreeMap::new();
    for element in &elements {
        if element.parent_element_id.is_some() {
            unsupported.push(Unsupported {
                construct: "element",
                id: ids::element(element.id),
                reason: "parent_element_id has no representation in the ArchiMate exchange format"
                    .to_owned(),
            });
        }
        match via(ARCHIMATE_ELEMENT_TYPES, &element.kind) {
            Some(xmi_type) => {
                element_types.insert(element.id, xmi_type);
            }
            None => unsupported.push(Unsupported {
                construct: "element",
                id: ids::element(element.id),
                reason: format!(
                    "element kind {:?} is not in the ArchiMate profile",
                    element.kind
                ),
            }),
        }
    }

    let mut relationships: Vec<&SnapshotRelationship> = model.relationships.iter().collect();
    relationships.sort_by_key(|relationship| relationship.id);
    let mut exported_relationships: Vec<(&SnapshotRelationship, &'static str)> = Vec::new();
    for relationship in relationships {
        let Some(xmi_type) = via(ARCHIMATE_RELATIONSHIP_TYPES, &relationship.kind) else {
            unsupported.push(Unsupported {
                construct: "relationship",
                id: ids::relationship(relationship.id),
                reason: format!(
                    "relationship kind {:?} is not in the ArchiMate profile",
                    relationship.kind
                ),
            });
            continue;
        };
        if !element_types.contains_key(&relationship.source_id) {
            unsupported.push(Unsupported {
                construct: "relationship",
                id: ids::relationship(relationship.id),
                reason: format!(
                    "the source element {} was not exported",
                    ids::element(relationship.source_id)
                ),
            });
            continue;
        }
        if !element_types.contains_key(&relationship.target_id) {
            unsupported.push(Unsupported {
                construct: "relationship",
                id: ids::relationship(relationship.id),
                reason: format!(
                    "the target element {} was not exported",
                    ids::element(relationship.target_id)
                ),
            });
            continue;
        }
        exported_relationships.push((relationship, xmi_type));
    }

    start(
        writer,
        "model",
        &[
            ("xmlns", ARCHIMATE_NS.to_owned()),
            ("xmlns:xsi", XSI_NS.to_owned()),
            ("identifier", document_identifier(model.model.id)),
        ],
    )?;
    text_element(
        writer,
        "name",
        &[("xml:lang", LANGUAGE.to_owned())],
        &model.model.title,
    )?;
    if !model.model.description.is_empty() {
        text_element(
            writer,
            "documentation",
            &[("xml:lang", LANGUAGE.to_owned())],
            &model.model.description,
        )?;
    }

    if !element_types.is_empty() {
        start(writer, "elements", &[])?;
        for element in &elements {
            let Some(xmi_type) = element_types.get(&element.id) else {
                continue;
            };
            start(
                writer,
                "element",
                &[
                    ("identifier", ids::element(element.id)),
                    ("xsi:type", (*xmi_type).to_owned()),
                ],
            )?;
            if !element.name.is_empty() {
                text_element(
                    writer,
                    "name",
                    &[("xml:lang", LANGUAGE.to_owned())],
                    &element.name,
                )?;
            }
            if !element.documentation.is_empty() {
                text_element(
                    writer,
                    "documentation",
                    &[("xml:lang", LANGUAGE.to_owned())],
                    &element.documentation,
                )?;
            }
            end(writer, "element")?;
        }
        end(writer, "elements")?;
    }

    if !exported_relationships.is_empty() {
        start(writer, "relationships", &[])?;
        for (relationship, xmi_type) in &exported_relationships {
            empty(
                writer,
                "relationship",
                &[
                    ("identifier", ids::relationship(relationship.id)),
                    ("source", ids::element(relationship.source_id)),
                    ("target", ids::element(relationship.target_id)),
                    ("xsi:type", (*xmi_type).to_owned()),
                ],
            )?;
        }
        end(writer, "relationships")?;
    }

    let mut views: Vec<&SnapshotView> = model.views.iter().collect();
    views.sort_by_key(|view| view.id);
    let relationship_ids: BTreeSet<Uuid> = exported_relationships
        .iter()
        .map(|(relationship, _)| relationship.id)
        .collect();
    let mut written_views = Vec::new();
    for view in views {
        if !profile.has_view_kind(&view.kind) {
            unsupported.push(Unsupported {
                construct: "view",
                id: ids::view(view.id),
                reason: format!("view kind {:?} is not in the ArchiMate profile", view.kind),
            });
            continue;
        }
        if view.kind != "archimate_view" {
            unsupported.push(Unsupported {
                construct: "view",
                id: ids::view(view.id),
                reason: format!(
                    "view kind {:?} has no exchange-format slot; it imports as archimate_view",
                    view.kind
                ),
            });
        }
        written_views.push(view);
    }
    if !written_views.is_empty() {
        start(writer, "views", &[])?;
        start(writer, "diagrams", &[])?;
        for view in written_views {
            write_archimate_view(writer, view, &element_types, &relationship_ids, unsupported)?;
        }
        end(writer, "diagrams")?;
        end(writer, "views")?;
    }

    end(writer, "model")
}

fn write_archimate_view(
    writer: &mut Writer<Vec<u8>>,
    view: &SnapshotView,
    element_types: &BTreeMap<Uuid, &'static str>,
    relationship_ids: &BTreeSet<Uuid>,
    unsupported: &mut Vec<Unsupported>,
) -> Result<(), RenderError> {
    start(
        writer,
        "view",
        &[
            ("identifier", ids::view(view.id)),
            ("xsi:type", "Diagram".to_owned()),
        ],
    )?;
    if !view.name.is_empty() {
        text_element(
            writer,
            "name",
            &[("xml:lang", LANGUAGE.to_owned())],
            &view.name,
        )?;
    }

    let all: BTreeMap<Uuid, &SnapshotObject> = view
        .objects
        .iter()
        .map(|object| (object.id, object))
        .collect();
    let mut kept: BTreeMap<Uuid, &SnapshotObject> = BTreeMap::new();
    for object in &view.objects {
        match archimate_node_type(&object.kind) {
            Some(_) => {
                kept.insert(object.id, object);
            }
            None => unsupported.push(Unsupported {
                construct: "object",
                id: ids::object(object.id),
                reason: format!(
                    "view object kind {:?} is not an ArchiMate node type",
                    object.kind
                ),
            }),
        }
    }

    let mut parents: BTreeMap<Uuid, Option<Uuid>> = BTreeMap::new();
    for (id, object) in &kept {
        let (parent, note) = archimate_effective_parent(*id, &all, &kept);
        if let Some(note) = note {
            unsupported.push(Unsupported {
                construct: "object",
                id: ids::object(*id),
                reason: note,
            });
        }
        parents.insert(*id, parent);
        if let Some(element_id) = object
            .element_id
            .filter(|element_id| !element_types.contains_key(element_id))
        {
            unsupported.push(Unsupported {
                construct: "object",
                id: ids::object(*id),
                reason: format!(
                    "the element {} was not exported; the node carries no elementRef",
                    ids::element(element_id)
                ),
            });
        }
    }

    let mut children: BTreeMap<Option<Uuid>, Vec<&SnapshotObject>> = BTreeMap::new();
    for (id, object) in &kept {
        children.entry(parents[id]).or_default().push(*object);
    }
    for group in children.values_mut() {
        group.sort_by(|left, right| by_position(left, right));
    }
    emit_archimate_nodes(writer, None, &children, element_types)?;

    let node_ids: BTreeSet<Uuid> = kept.keys().copied().collect();
    let mut connections: Vec<&SnapshotConnection> = view.connections.iter().collect();
    connections.sort_by_key(|connection| connection.id);
    for connection in connections {
        if !node_ids.contains(&connection.source_object_id)
            || !node_ids.contains(&connection.target_object_id)
        {
            unsupported.push(Unsupported {
                construct: "connection",
                id: ids::connection(connection.id),
                reason: "an endpoint view object was not exported".to_owned(),
            });
            continue;
        }
        let mut attributes = vec![
            ("identifier", ids::connection(connection.id)),
            ("source", ids::object(connection.source_object_id)),
            ("target", ids::object(connection.target_object_id)),
        ];
        match connection.relationship_id {
            Some(relationship_id) if relationship_ids.contains(&relationship_id) => {
                attributes.push(("xsi:type", "Relationship".to_owned()));
                attributes.push(("relationshipRef", ids::relationship(relationship_id)));
            }
            Some(relationship_id) => {
                unsupported.push(Unsupported {
                    construct: "connection",
                    id: ids::connection(connection.id),
                    reason: format!(
                        "the relationship {} was not exported; the connector is written as a line",
                        ids::relationship(relationship_id)
                    ),
                });
                attributes.push(("xsi:type", "Line".to_owned()));
            }
            None => attributes.push(("xsi:type", "Line".to_owned())),
        }
        let bendpoints = points_of(&connection.bendpoints);
        if bendpoints.is_empty() {
            empty(writer, "connection", &attributes)?;
        } else {
            start(writer, "connection", &attributes)?;
            for (x, y) in bendpoints {
                empty(
                    writer,
                    "bendpoint",
                    &[("x", ids::number(x)), ("y", ids::number(y))],
                )?;
            }
            end(writer, "connection")?;
        }
    }

    end(writer, "view")
}

fn emit_archimate_nodes(
    writer: &mut Writer<Vec<u8>>,
    parent: Option<Uuid>,
    children: &BTreeMap<Option<Uuid>, Vec<&SnapshotObject>>,
    element_types: &BTreeMap<Uuid, &'static str>,
) -> Result<(), RenderError> {
    let Some(group) = children.get(&parent) else {
        return Ok(());
    };
    for object in group {
        let node_type = archimate_node_type(&object.kind).unwrap_or("Element");
        let (x, y, w, h) = bounds_of(&object.bounds);
        let mut attributes = vec![
            ("identifier", ids::object(object.id)),
            ("xsi:type", node_type.to_owned()),
        ];
        if let Some(element_id) = object
            .element_id
            .filter(|element_id| element_types.contains_key(element_id))
        {
            attributes.push(("elementRef", ids::element(element_id)));
        }
        attributes.push(("x", ids::number(x)));
        attributes.push(("y", ids::number(y)));
        attributes.push(("w", ids::number(w)));
        attributes.push(("h", ids::number(h)));
        let has_children = children
            .get(&Some(object.id))
            .is_some_and(|group| !group.is_empty());
        let label = object.label.as_deref().filter(|label| !label.is_empty());
        if has_children || label.is_some() {
            start(writer, "node", &attributes)?;
            if let Some(label) = label {
                text_element(writer, "label", &[("xml:lang", LANGUAGE.to_owned())], label)?;
            }
            emit_archimate_nodes(writer, Some(object.id), children, element_types)?;
            end(writer, "node")?;
        } else {
            empty(writer, "node", &attributes)?;
        }
    }
    Ok(())
}

// ── UML export ───────────────────────────────────────────────────────────────────────────

fn write_uml(
    model: &ModelSnapshot,
    profile: &'static Profile,
    writer: &mut Writer<Vec<u8>>,
    unsupported: &mut Vec<Unsupported>,
) -> Result<(), RenderError> {
    let all: BTreeMap<Uuid, &SnapshotElement> = model
        .elements
        .iter()
        .map(|element| (element.id, element))
        .collect();
    let mut elements: Vec<&SnapshotElement> = model.elements.iter().collect();
    elements.sort_by_key(|element| element.id);

    let mut element_types: BTreeMap<Uuid, &'static str> = BTreeMap::new();
    for element in &elements {
        match via(UML_ELEMENT_TYPES, &element.kind) {
            Some(xmi_type) => {
                element_types.insert(element.id, xmi_type);
            }
            None => unsupported.push(Unsupported {
                construct: "element",
                id: ids::element(element.id),
                reason: format!("element kind {:?} is not in the UML profile", element.kind),
            }),
        }
    }

    let mut parents: BTreeMap<Uuid, Option<Uuid>> = BTreeMap::new();
    for id in element_types.keys() {
        let (parent, note) = element_effective_parent(*id, &all, &element_types);
        if let Some(note) = note {
            unsupported.push(Unsupported {
                construct: "element",
                id: ids::element(*id),
                reason: note,
            });
        }
        parents.insert(*id, parent);
    }
    let mut children: BTreeMap<Option<Uuid>, Vec<&SnapshotElement>> = BTreeMap::new();
    for (id, element) in &all {
        if element_types.contains_key(id) {
            children.entry(parents[id]).or_default().push(*element);
        }
    }
    for group in children.values_mut() {
        group.sort_by_key(|element| element.id);
    }

    let mut relationships: Vec<&SnapshotRelationship> = model.relationships.iter().collect();
    relationships.sort_by_key(|relationship| relationship.id);
    let mut nested: BTreeMap<Uuid, Vec<&SnapshotRelationship>> = BTreeMap::new();
    let mut top: Vec<(&SnapshotRelationship, &'static str)> = Vec::new();
    for relationship in relationships {
        let Some(xmi_type) = via(UML_RELATIONSHIP_TYPES, &relationship.kind) else {
            unsupported.push(Unsupported {
                construct: "relationship",
                id: ids::relationship(relationship.id),
                reason: format!(
                    "relationship kind {:?} is not in the UML profile",
                    relationship.kind
                ),
            });
            continue;
        };
        if !element_types.contains_key(&relationship.source_id) {
            unsupported.push(Unsupported {
                construct: "relationship",
                id: ids::relationship(relationship.id),
                reason: format!(
                    "the source element {} was not exported",
                    ids::element(relationship.source_id)
                ),
            });
            continue;
        }
        if !element_types.contains_key(&relationship.target_id) {
            unsupported.push(Unsupported {
                construct: "relationship",
                id: ids::relationship(relationship.id),
                reason: format!(
                    "the target element {} was not exported",
                    ids::element(relationship.target_id)
                ),
            });
            continue;
        }
        if relationship.kind == "generalization" {
            nested
                .entry(relationship.source_id)
                .or_default()
                .push(relationship);
        } else {
            top.push((relationship, xmi_type));
        }
    }

    start(
        writer,
        "xmi:XMI",
        &[
            ("xmlns:xmi", XMI_NS.to_owned()),
            ("xmlns:uml", UML_NS.to_owned()),
        ],
    )?;
    let identifier = document_identifier(model.model.id);
    start(
        writer,
        "uml:Model",
        &[
            ("xmi:id", identifier.clone()),
            ("name", model.model.title.clone()),
        ],
    )?;
    if !model.model.description.is_empty() {
        start(
            writer,
            "ownedComment",
            &[
                ("xmi:type", "uml:Comment".to_owned()),
                ("xmi:id", format!("{identifier}-doc")),
            ],
        )?;
        text_element(writer, "body", &[], &model.model.description)?;
        end(writer, "ownedComment")?;
    }

    emit_uml_elements(writer, None, &children, &element_types, &nested)?;

    for (relationship, xmi_type) in &top {
        let relationship_id = ids::relationship(relationship.id);
        if matches!(
            relationship.kind.as_str(),
            "association" | "aggregation" | "composition"
        ) {
            let source_end = format!("{relationship_id}-end-0");
            let target_end = format!("{relationship_id}-end-1");
            start(
                writer,
                "packagedElement",
                &[
                    ("xmi:type", "uml:Association".to_owned()),
                    ("xmi:id", relationship_id.clone()),
                    ("memberEnd", format!("{source_end} {target_end}")),
                ],
            )?;
            empty(
                writer,
                "ownedEnd",
                &[
                    ("xmi:type", "uml:Property".to_owned()),
                    ("xmi:id", source_end),
                    ("type", ids::element(relationship.source_id)),
                    ("association", relationship_id.clone()),
                ],
            )?;
            let mut target_attributes = vec![
                ("xmi:type", "uml:Property".to_owned()),
                ("xmi:id", target_end),
                ("type", ids::element(relationship.target_id)),
                ("association", relationship_id.clone()),
            ];
            match relationship.kind.as_str() {
                "composition" => {
                    target_attributes.push(("aggregation", "composite".to_owned()));
                }
                "aggregation" => {
                    target_attributes.push(("aggregation", "shared".to_owned()));
                }
                _ => {}
            }
            empty(writer, "ownedEnd", &target_attributes)?;
            end(writer, "packagedElement")?;
        } else {
            empty(
                writer,
                "packagedElement",
                &[
                    ("xmi:type", format!("uml:{xmi_type}")),
                    ("xmi:id", relationship_id),
                    ("client", ids::element(relationship.source_id)),
                    ("supplier", ids::element(relationship.target_id)),
                ],
            )?;
        }
    }

    end(writer, "uml:Model")?;

    let mut views: Vec<&SnapshotView> = model.views.iter().collect();
    views.sort_by_key(|view| view.id);
    let mut written_views = Vec::new();
    for view in views {
        if !profile.has_view_kind(&view.kind) {
            unsupported.push(Unsupported {
                construct: "view",
                id: ids::view(view.id),
                reason: format!("view kind {:?} is not in the UML profile", view.kind),
            });
            continue;
        }
        written_views.push(view);
    }
    if !written_views.is_empty() {
        start(
            writer,
            "xmi:Extension",
            &[("extender", EXTENDER.to_owned())],
        )?;
        for view in written_views {
            write_uml_view(writer, view, &element_types)?;
        }
        end(writer, "xmi:Extension")?;
    }

    end(writer, "xmi:XMI")
}

fn emit_uml_elements(
    writer: &mut Writer<Vec<u8>>,
    parent: Option<Uuid>,
    children: &BTreeMap<Option<Uuid>, Vec<&SnapshotElement>>,
    element_types: &BTreeMap<Uuid, &'static str>,
    nested: &BTreeMap<Uuid, Vec<&SnapshotRelationship>>,
) -> Result<(), RenderError> {
    let Some(group) = children.get(&parent) else {
        return Ok(());
    };
    for element in group {
        let Some(xmi_type) = element_types.get(&element.id) else {
            continue;
        };
        let identifier = ids::element(element.id);
        start(
            writer,
            "packagedElement",
            &[
                ("xmi:type", format!("uml:{xmi_type}")),
                ("xmi:id", identifier.clone()),
                ("name", element.name.clone()),
            ],
        )?;
        if !element.documentation.is_empty() {
            start(
                writer,
                "ownedComment",
                &[
                    ("xmi:type", "uml:Comment".to_owned()),
                    ("xmi:id", format!("{identifier}-doc")),
                ],
            )?;
            text_element(writer, "body", &[], &element.documentation)?;
            end(writer, "ownedComment")?;
        }
        emit_uml_elements(writer, Some(element.id), children, element_types, nested)?;
        for relationship in nested.get(&element.id).map(Vec::as_slice).unwrap_or(&[]) {
            empty(
                writer,
                "generalization",
                &[
                    ("xmi:type", "uml:Generalization".to_owned()),
                    ("xmi:id", ids::relationship(relationship.id)),
                    ("general", ids::element(relationship.target_id)),
                ],
            )?;
        }
        end(writer, "packagedElement")?;
    }
    Ok(())
}

fn write_uml_view(
    writer: &mut Writer<Vec<u8>>,
    view: &SnapshotView,
    element_types: &BTreeMap<Uuid, &'static str>,
) -> Result<(), RenderError> {
    let mut attributes = vec![
        ("xmi:id", ids::view(view.id)),
        ("kind", view.kind.clone()),
        ("name", view.name.clone()),
    ];
    if let Some(diagram_kind) = view.meta.get(DIAGRAM_KIND).and_then(Value::as_str) {
        attributes.push(("diagramKind", diagram_kind.to_owned()));
    }
    start(writer, "view", &attributes)?;

    let mut objects: Vec<&SnapshotObject> = view.objects.iter().collect();
    objects.sort_by(|left, right| by_position(left, right));
    for object in objects {
        let (x, y, w, h) = bounds_of(&object.bounds);
        let mut attributes = vec![
            ("xmi:id", ids::object(object.id)),
            ("kind", object.kind.clone()),
            ("x", ids::number(x)),
            ("y", ids::number(y)),
            ("w", ids::number(w)),
            ("h", ids::number(h)),
            ("position", ids::number(object.position)),
        ];
        if let Some(element_id) = object
            .element_id
            .filter(|element_id| element_types.contains_key(element_id))
        {
            attributes.push(("element", ids::element(element_id)));
        }
        if let Some(label) = object.label.as_deref().filter(|label| !label.is_empty()) {
            attributes.push(("label", label.to_owned()));
        }
        empty(writer, "object", &attributes)?;
    }

    let mut connections: Vec<&SnapshotConnection> = view.connections.iter().collect();
    connections.sort_by_key(|connection| connection.id);
    let object_ids: BTreeSet<Uuid> = view.objects.iter().map(|object| object.id).collect();
    for connection in connections {
        if !object_ids.contains(&connection.source_object_id)
            || !object_ids.contains(&connection.target_object_id)
        {
            continue;
        }
        let mut attributes = vec![
            ("xmi:id", ids::connection(connection.id)),
            ("source", ids::object(connection.source_object_id)),
            ("target", ids::object(connection.target_object_id)),
        ];
        if let Some(relationship_id) = connection.relationship_id {
            attributes.push(("relationship", ids::relationship(relationship_id)));
        }
        let bendpoints = points_of(&connection.bendpoints);
        if bendpoints.is_empty() {
            empty(writer, "connection", &attributes)?;
        } else {
            start(writer, "connection", &attributes)?;
            for (x, y) in bendpoints {
                empty(
                    writer,
                    "bendpoint",
                    &[("x", ids::number(x)), ("y", ids::number(y))],
                )?;
            }
            end(writer, "connection")?;
        }
    }

    end(writer, "view")
}

// ── Import: the hardened tree ────────────────────────────────────────────────────────────

/// One element of the parsed document. The tree is bounded by [`MAX_DOCUMENT_BYTES`] (the
/// input cap) and [`MAX_DEPTH`] (the nesting cap), so it can never be larger than the parsed
/// source; nothing is ever allocated from a declared length.
struct Node {
    /// The element's local name, namespace prefix stripped.
    name: String,
    /// Attributes as `(local name, value)` pairs, in document order.
    attributes: Vec<(String, String)>,
    /// Child elements, in document order.
    children: Vec<Node>,
    /// Character data directly inside this element.
    content: String,
}

impl Node {
    fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|child| child.name == name)
    }

    fn attribute(&self, name: &str) -> Option<&str> {
        // The qualified spelling wins, so `xmi:type="uml:Property"` on an association end is
        // never mistaken for its `type="e-…"` endpoint reference. The local-name fallback
        // keeps a document that drops the prefix (`type="BusinessActor"`) readable.
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .or_else(|| {
                self.attributes
                    .iter()
                    .find(|(key, _)| local_name(key) == name)
            })
            .map(|(_, value)| value.as_str())
    }

    /// The element's text content, trimmed. Indentation from a pretty-printed document is
    /// therefore never mistaken for a name or a description.
    fn text(&self) -> String {
        self.content.trim().to_owned()
    }
}

/// Resolve one element reference to its character.
fn resolve_reference(
    reference: &quick_xml::events::BytesRef<'_>,
) -> Result<Option<char>, RenderError> {
    if reference.is_char_ref() {
        return reference
            .resolve_char_ref()
            .map_err(|error| RenderError::Xmi(format!("invalid character reference: {error}")));
    }
    let name: &str = reference.as_ref();
    match name {
        "amp" => Ok(Some('&')),
        "lt" => Ok(Some('<')),
        "gt" => Ok(Some('>')),
        "quot" => Ok(Some('"')),
        "apos" => Ok(Some('\'')),
        other => Err(RenderError::Xmi(format!(
            "the entity reference &{other}; is not defined; a DOCTYPE is never read"
        ))),
    }
}

/// Attach a finished node to its parent, or to the root slot.
fn attach(stack: &mut [Node], root: &mut Option<Node>, node: Node) -> Result<(), RenderError> {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
        Ok(())
    } else if root.is_none() {
        *root = Some(node);
        Ok(())
    } else {
        Err(RenderError::Xmi(
            "the document has more than one root element".to_owned(),
        ))
    }
}

/// The local name of a tag or attribute name: `uml:Model` → `Model`, `name` → `name`.
fn local_name(raw: &str) -> &str {
    match raw.rsplit_once(':') {
        Some((_, local)) => local,
        None => raw,
    }
}

/// Build one node from a start tag, collecting its attributes by local name.
fn node_from(element: &BytesStart<'_>) -> Result<Node, RenderError> {
    let name = local_name(element.name().as_ref()).to_owned();
    let mut attributes = Vec::new();
    for attribute in element.attributes() {
        let attribute = attribute
            .map_err(|error| RenderError::Xmi(format!("an attribute is not valid: {error}")))?;
        let key = attribute.key.as_ref().to_owned();
        let value = attribute
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(|error| {
                RenderError::Xmi(format!("an attribute value is not valid: {error}"))
            })?;
        attributes.push((key, value.into_owned()));
    }
    Ok(Node {
        name,
        attributes,
        children: Vec::new(),
        content: String::new(),
    })
}

/// Parse the document into a tree, enforcing every hardening rule on the way.
fn parse_document(document: &str) -> Result<Node, RenderError> {
    if document.len() > MAX_DOCUMENT_BYTES {
        return Err(RenderError::Xmi(format!(
            "the XMI document is {} bytes, over the {MAX_DOCUMENT_BYTES}-byte cap",
            document.len()
        )));
    }
    let mut reader = Reader::from_str(document);
    let mut stack: Vec<Node> = Vec::new();
    let mut root: Option<Node> = None;
    loop {
        let event = reader
            .read_event()
            .map_err(|error| RenderError::Xmi(format!("not well-formed XML: {error}")))?;
        match event {
            Event::Start(element) => {
                if stack.len() + 1 > MAX_DEPTH {
                    return Err(RenderError::Xmi(format!(
                        "the document nests deeper than {MAX_DEPTH} elements"
                    )));
                }
                stack.push(node_from(&element)?);
            }
            Event::Empty(element) => {
                if stack.len() + 1 > MAX_DEPTH {
                    return Err(RenderError::Xmi(format!(
                        "the document nests deeper than {MAX_DEPTH} elements"
                    )));
                }
                let node = node_from(&element)?;
                attach(&mut stack, &mut root, node)?;
            }
            Event::End(_) => {
                let node = stack.pop().ok_or_else(|| {
                    RenderError::Xmi("the document has an unbalanced end tag".to_owned())
                })?;
                attach(&mut stack, &mut root, node)?;
            }
            Event::Text(text) => {
                if let Some(node) = stack.last_mut() {
                    node.content.push_str(&text.xml10_content());
                }
            }
            Event::CData(data) => {
                if let Some(node) = stack.last_mut() {
                    node.content.push_str(&data.into_inner());
                }
            }
            Event::GeneralRef(reference) => {
                if let (Some(node), Some(resolved)) =
                    (stack.last_mut(), resolve_reference(&reference)?)
                {
                    node.content.push(resolved);
                }
            }
            Event::DocType(_) => {
                return Err(RenderError::Xmi(
                    "a DOCTYPE declaration is not accepted in an XMI document".to_owned(),
                ));
            }
            Event::Decl(_) | Event::Comment(_) | Event::PI(_) => {}
            Event::Eof => break,
        }
    }
    if !stack.is_empty() {
        return Err(RenderError::Xmi(
            "the document ends with elements still open".to_owned(),
        ));
    }
    root.ok_or_else(|| RenderError::Xmi("the document has no root element".to_owned()))
}

/// Refuse an id that was already declared. An `xs:ID` is document-wide unique; so is this.
fn declare_id(seen: &mut BTreeSet<String>, raw: &str) -> Result<(), RenderError> {
    if !seen.insert(raw.to_owned()) {
        return Err(RenderError::Xmi(format!("duplicate id {raw:?}")));
    }
    Ok(())
}

/// The `identifier`/`xmi:id` of a construct, declared against the duplicate set.
fn node_id(node: &Node, seen: &mut BTreeSet<String>) -> Result<Option<String>, RenderError> {
    let Some(raw) = node
        .attribute("identifier")
        .or_else(|| node.attribute("id"))
    else {
        return Ok(None);
    };
    declare_id(seen, raw)?;
    Ok(Some(raw.to_owned()))
}

/// Map a document id to a model id.
///
/// One of our prefixed UUIDs is stripped to its UUID; anything else is hashed through the
/// codec's namespace, so the mapping is a pure function of the document's bytes — the same
/// file always produces the same ids in this process. (The database mints the row id on
/// insert; that is a property of the store, not of the codec.)
fn map_id(raw: &str) -> Uuid {
    for prefix in ["e-", "r-", "v-", "o-", "c-"] {
        if let Some(rest) = raw.strip_prefix(prefix)
            && let Ok(uuid) = Uuid::parse_str(rest)
        {
            return uuid;
        }
    }
    Uuid::new_v5(&CONVEYOR_XMI_NS, raw.as_bytes())
}

fn number_attribute(node: &Node, name: &str) -> f64 {
    node.attribute(name)
        .and_then(|value| value.trim().parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// The documentation of a UML element: its first `ownedComment`'s `body`.
fn uml_documentation(node: &Node) -> String {
    node.children
        .iter()
        .find(|child| child.name == "ownedComment")
        .and_then(|comment| comment.child("body"))
        .map(Node::text)
        .unwrap_or_default()
}

/// Parse an XMI document.
///
/// `language` and `version`, when given, must agree with what the document declares.
pub fn import_xmi(document: &str) -> Result<ImportedModel, RenderError> {
    let root = parse_document(document)?;
    match root.name.as_str() {
        "model" => read_archimate(&root),
        "XMI" | "Model" => read_uml(&root),
        other => Err(RenderError::Xmi(format!(
            "the document root <{other}> is neither an ArchiMate exchange model nor an XMI document"
        ))),
    }
}

// ── Import: ArchiMate ───────────────────────────────────────────────────────────────────

fn read_archimate(root: &Node) -> Result<ImportedModel, RenderError> {
    let mut seen = BTreeSet::new();
    let mut unsupported = Vec::new();
    if let Some(raw) = root.attribute("identifier") {
        declare_id(&mut seen, raw)?;
    }
    let title = root.child("name").map(Node::text).unwrap_or_default();
    let description = root
        .child("documentation")
        .map(Node::text)
        .unwrap_or_default();

    let mut elements = Vec::new();
    let mut declared = BTreeSet::new();
    let mut kept = BTreeSet::new();
    for container in root
        .children
        .iter()
        .filter(|child| child.name == "elements")
    {
        for node in container
            .children
            .iter()
            .filter(|child| child.name == "element")
        {
            let Some(raw) = node_id(node, &mut seen)? else {
                unsupported.push(Unsupported {
                    construct: "element",
                    id: String::new(),
                    reason: "the element has no identifier".to_owned(),
                });
                continue;
            };
            declared.insert(raw.clone());
            let xmi_type = node.attribute("type").unwrap_or("");
            let Some(kind) = reverse(ARCHIMATE_ELEMENT_TYPES, bare_type(xmi_type)) else {
                unsupported.push(Unsupported {
                    construct: "element",
                    id: raw,
                    reason: format!("element type {xmi_type:?} is not in the ArchiMate profile"),
                });
                continue;
            };
            let id = map_id(&raw);
            kept.insert(id);
            elements.push(ImportedElement {
                id,
                kind: kind.to_owned(),
                name: node.child("name").map(Node::text).unwrap_or_default(),
                documentation: node
                    .child("documentation")
                    .map(Node::text)
                    .unwrap_or_default(),
                parent_id: None,
            });
        }
    }

    let mut relationships = Vec::new();
    for container in root
        .children
        .iter()
        .filter(|child| child.name == "relationships")
    {
        for node in container
            .children
            .iter()
            .filter(|child| child.name == "relationship")
        {
            let Some(raw) = node_id(node, &mut seen)? else {
                unsupported.push(Unsupported {
                    construct: "relationship",
                    id: String::new(),
                    reason: "the relationship has no identifier".to_owned(),
                });
                continue;
            };
            let xmi_type = node.attribute("type").unwrap_or("");
            let Some(kind) = reverse(ARCHIMATE_RELATIONSHIP_TYPES, bare_type(xmi_type)) else {
                unsupported.push(Unsupported {
                    construct: "relationship",
                    id: raw,
                    reason: format!(
                        "relationship type {xmi_type:?} is not in the ArchiMate profile"
                    ),
                });
                continue;
            };
            let (Some(source), Some(target)) = (node.attribute("source"), node.attribute("target"))
            else {
                unsupported.push(Unsupported {
                    construct: "relationship",
                    id: raw,
                    reason: "the relationship has no source or target".to_owned(),
                });
                continue;
            };
            let mut endpoints_ok = true;
            for (role, endpoint) in [("source", source), ("target", target)] {
                if !declared.contains(endpoint) {
                    unsupported.push(Unsupported {
                        construct: "relationship",
                        id: raw.clone(),
                        reason: format!("{role} {endpoint:?} is not an element of this document"),
                    });
                    endpoints_ok = false;
                } else if !kept.contains(&map_id(endpoint)) {
                    unsupported.push(Unsupported {
                        construct: "relationship",
                        id: raw.clone(),
                        reason: format!("{role} {endpoint:?} was not carried"),
                    });
                    endpoints_ok = false;
                }
            }
            if !endpoints_ok {
                continue;
            }
            relationships.push(ImportedRelationship {
                id: map_id(&raw),
                kind: kind.to_owned(),
                source_id: map_id(source),
                target_id: map_id(target),
            });
        }
    }

    let relationship_ids: BTreeSet<Uuid> = relationships
        .iter()
        .map(|relationship| relationship.id)
        .collect();
    let mut views = Vec::new();
    for container in root.children.iter().filter(|child| child.name == "views") {
        for child in &container.children {
            match child.name.as_str() {
                "diagrams" => {
                    for node in child.children.iter().filter(|node| node.name == "view") {
                        if let Some(view) = read_archimate_view(
                            node,
                            &kept,
                            &relationship_ids,
                            &mut seen,
                            &mut unsupported,
                        )? {
                            views.push(view);
                        }
                    }
                }
                "view" => {
                    if let Some(view) = read_archimate_view(
                        child,
                        &kept,
                        &relationship_ids,
                        &mut seen,
                        &mut unsupported,
                    )? {
                        views.push(view);
                    }
                }
                other => unsupported.push(Unsupported {
                    construct: "view",
                    id: child.attribute("identifier").unwrap_or("").to_owned(),
                    reason: format!("view construct <{other}> is not carried"),
                }),
            }
        }
    }

    Ok(ImportedModel {
        language: "archimate".to_owned(),
        language_version: "3.2".to_owned(),
        title,
        description,
        elements,
        relationships,
        views,
        unsupported,
    })
}

/// The state of one ArchiMate view while it is read.
struct ArchimateViewReader<'a> {
    elements: &'a BTreeSet<Uuid>,
    seen: &'a mut BTreeSet<String>,
    unsupported: &'a mut Vec<Unsupported>,
    objects: Vec<ImportedObject>,
    node_ids: BTreeMap<String, Uuid>,
    position: f64,
}

impl ArchimateViewReader<'_> {
    fn nodes(&mut self, nodes: &[Node], parent: Option<Uuid>) -> Result<(), RenderError> {
        for node in nodes.iter().filter(|node| node.name == "node") {
            let Some(raw) = node_id(node, self.seen)? else {
                self.unsupported.push(Unsupported {
                    construct: "object",
                    id: String::new(),
                    reason: "the view object has no identifier".to_owned(),
                });
                self.nodes(&node.children, parent)?;
                continue;
            };
            let xmi_type = node.attribute("type").unwrap_or("");
            let Some(kind) = archimate_object_kind(bare_type(xmi_type)) else {
                self.unsupported.push(Unsupported {
                    construct: "object",
                    id: raw,
                    reason: format!(
                        "node type {xmi_type:?} is not an ArchiMate node; its children are re-parented to the enclosing node"
                    ),
                });
                self.nodes(&node.children, parent)?;
                continue;
            };
            let id = map_id(&raw);
            let element_id = match node.attribute("elementRef") {
                Some(reference) => {
                    let mapped = map_id(reference);
                    if self.elements.contains(&mapped) {
                        Some(mapped)
                    } else {
                        self.unsupported.push(Unsupported {
                            construct: "object",
                            id: raw.clone(),
                            reason: format!(
                                "element {reference:?} was not carried; the node draws no element"
                            ),
                        });
                        None
                    }
                }
                None => None,
            };
            let position = self.position;
            self.position += 1.0;
            self.node_ids.insert(raw, id);
            self.objects.push(ImportedObject {
                id,
                element_id,
                parent_id: parent,
                kind: kind.to_owned(),
                bounds: json!({
                    "x": number_attribute(node, "x"),
                    "y": number_attribute(node, "y"),
                    "w": number_attribute(node, "w"),
                    "h": number_attribute(node, "h"),
                }),
                label: node
                    .child("label")
                    .map(Node::text)
                    .filter(|label| !label.is_empty()),
                position,
            });
            self.nodes(&node.children, Some(id))?;
        }
        Ok(())
    }

    fn connections(
        &mut self,
        view: &Node,
        relationships: &BTreeSet<Uuid>,
    ) -> Result<Vec<ImportedConnection>, RenderError> {
        let mut connections = Vec::new();
        for node in view
            .children
            .iter()
            .filter(|child| child.name == "connection")
        {
            let Some(raw) = node_id(node, self.seen)? else {
                self.unsupported.push(Unsupported {
                    construct: "connection",
                    id: String::new(),
                    reason: "the connection has no identifier".to_owned(),
                });
                continue;
            };
            let xmi_type = node.attribute("type").unwrap_or("");
            if !matches!(bare_type(xmi_type), "" | "Relationship" | "Line") {
                self.unsupported.push(Unsupported {
                    construct: "connection",
                    id: raw,
                    reason: format!("connection type {xmi_type:?} is not carried"),
                });
                continue;
            }
            let (Some(source), Some(target)) = (node.attribute("source"), node.attribute("target"))
            else {
                self.unsupported.push(Unsupported {
                    construct: "connection",
                    id: raw,
                    reason: "the connection has no source or target".to_owned(),
                });
                continue;
            };
            let (Some(source_id), Some(target_id)) = (
                self.node_ids.get(source).copied(),
                self.node_ids.get(target).copied(),
            ) else {
                self.unsupported.push(Unsupported {
                    construct: "connection",
                    id: raw,
                    reason: "an endpoint view object was not carried".to_owned(),
                });
                continue;
            };
            let relationship_id = match node.attribute("relationshipRef") {
                Some(reference) => {
                    let mapped = map_id(reference);
                    if relationships.contains(&mapped) {
                        Some(mapped)
                    } else {
                        self.unsupported.push(Unsupported {
                            construct: "connection",
                            id: raw.clone(),
                            reason: format!(
                                "relationship {reference:?} was not carried; the connection draws a line"
                            ),
                        });
                        None
                    }
                }
                None => None,
            };
            let bendpoints: Vec<Value> = node
                .children
                .iter()
                .filter(|child| child.name == "bendpoint")
                .map(|point| {
                    json!({
                        "x": number_attribute(point, "x"),
                        "y": number_attribute(point, "y"),
                    })
                })
                .collect();
            connections.push(ImportedConnection {
                id: map_id(&raw),
                relationship_id,
                source_id,
                target_id,
                bendpoints: Value::Array(bendpoints),
            });
        }
        Ok(connections)
    }
}

fn read_archimate_view(
    node: &Node,
    elements: &BTreeSet<Uuid>,
    relationships: &BTreeSet<Uuid>,
    seen: &mut BTreeSet<String>,
    unsupported: &mut Vec<Unsupported>,
) -> Result<Option<ImportedView>, RenderError> {
    let Some(raw) = node_id(node, seen)? else {
        unsupported.push(Unsupported {
            construct: "view",
            id: String::new(),
            reason: "the view has no identifier".to_owned(),
        });
        return Ok(None);
    };
    let xmi_type = node.attribute("type").unwrap_or("");
    if !matches!(bare_type(xmi_type), "" | "Diagram") {
        unsupported.push(Unsupported {
            construct: "view",
            id: raw,
            reason: format!("view type {xmi_type:?} is not an ArchiMate diagram"),
        });
        return Ok(None);
    }
    let name = node.child("name").map(Node::text).unwrap_or_default();
    let mut reader = ArchimateViewReader {
        elements,
        seen,
        unsupported,
        objects: Vec::new(),
        node_ids: BTreeMap::new(),
        position: 0.0,
    };
    reader.nodes(&node.children, None)?;
    let connections = reader.connections(node, relationships)?;
    let objects = reader.objects;
    Ok(Some(ImportedView {
        id: map_id(&raw),
        kind: "archimate_view".to_owned(),
        name,
        diagram_kind: None,
        objects,
        connections,
    }))
}

// ── Import: UML ─────────────────────────────────────────────────────────────────────────

/// The element pass of a UML document. Relationships are collected as node references and
/// resolved afterwards, because a generalization or an association may name an element that
/// appears later in the document.
struct UmlCollect<'a> {
    seen: BTreeSet<String>,
    unsupported: Vec<Unsupported>,
    elements: Vec<ImportedElement>,
    declared: BTreeSet<String>,
    by_raw: BTreeMap<String, Uuid>,
    pending: Vec<(&'a Node, Option<Uuid>)>,
}

impl<'a> UmlCollect<'a> {
    /// `parent` is the containment parent for an element; `owner` is the classifier a nested
    /// relationship (a generalization) belongs to. They differ when the enclosing packaged
    /// element was not carried: its children are still walked so they are not lost, but a
    /// generalization inside it must not be silently re-attributed to the outer element.
    fn walk(
        &mut self,
        nodes: &'a [Node],
        parent: Option<Uuid>,
        owner: Option<Uuid>,
    ) -> Result<(), RenderError> {
        for node in nodes {
            match node.name.as_str() {
                "packagedElement" => self.packaged_element(node, parent, owner)?,
                "generalization" => {
                    if node.attribute("id").is_none() && node.attribute("identifier").is_none() {
                        self.unsupported.push(Unsupported {
                            construct: "relationship",
                            id: String::new(),
                            reason: "the generalization has no identifier".to_owned(),
                        });
                        continue;
                    }
                    // The id is declared when the relationship is read, so a document that
                    // repeats it is refused exactly once.
                    self.pending.push((node, owner));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn packaged_element(
        &mut self,
        node: &'a Node,
        parent: Option<Uuid>,
        owner: Option<Uuid>,
    ) -> Result<(), RenderError> {
        let xmi_type = node.attribute("type").unwrap_or("");
        let local = bare_type(xmi_type);
        if UML_RELATIONSHIP_TYPES
            .iter()
            .any(|(_, value)| *value == local)
        {
            // A relationship's id is declared when the relationship is read, so a document
            // that repeats it is refused exactly once (and not once per pass).
            if node.attribute("id").is_none() && node.attribute("identifier").is_none() {
                self.unsupported.push(Unsupported {
                    construct: "relationship",
                    id: String::new(),
                    reason: "the relationship has no identifier".to_owned(),
                });
                return Ok(());
            }
            self.pending.push((node, owner));
            return Ok(());
        }
        let Some(raw) = node_id(node, &mut self.seen)? else {
            self.unsupported.push(Unsupported {
                construct: "element",
                id: String::new(),
                reason: "the packaged element has no identifier".to_owned(),
            });
            self.walk(&node.children, parent, None)?;
            return Ok(());
        };
        if let Some(kind) = reverse(UML_ELEMENT_TYPES, local) {
            let id = map_id(&raw);
            self.declared.insert(raw.clone());
            self.by_raw.insert(raw, id);
            self.elements.push(ImportedElement {
                id,
                kind: kind.to_owned(),
                name: node.attribute("name").unwrap_or("").to_owned(),
                documentation: uml_documentation(node),
                parent_id: parent,
            });
            self.walk(&node.children, Some(id), Some(id))?;
        } else {
            self.declared.insert(raw.clone());
            self.unsupported.push(Unsupported {
                construct: "element",
                id: raw,
                reason: format!("xmi:type {xmi_type:?} is not in the UML profile"),
            });
            // The children keep the outer containment parent (so a nested element is not
            // lost) but lose their owner, so a relationship inside them is reported rather
            // than mis-attributed.
            self.walk(&node.children, parent, None)?;
        }
        Ok(())
    }

    /// The mapped id of an element endpoint, reporting the edge when it cannot be resolved.
    fn endpoint(&mut self, endpoint: &str, relationship: &str) -> Option<Uuid> {
        if let Some(id) = self.by_raw.get(endpoint) {
            return Some(*id);
        }
        let reason = if self.declared.contains(endpoint) {
            format!("the endpoint {endpoint:?} was not carried")
        } else {
            format!("the endpoint {endpoint:?} is not an element of this document")
        };
        self.unsupported.push(Unsupported {
            construct: "relationship",
            id: relationship.to_owned(),
            reason,
        });
        None
    }

    fn relationships(&mut self) -> Result<Vec<ImportedRelationship>, RenderError> {
        let pending = std::mem::take(&mut self.pending);
        let mut relationships = Vec::new();
        for (node, enclosing) in pending {
            let Some(raw) = node_id(node, &mut self.seen)? else {
                self.unsupported.push(Unsupported {
                    construct: "relationship",
                    id: String::new(),
                    reason: "the relationship has no identifier".to_owned(),
                });
                continue;
            };
            if node.name == "generalization" {
                let Some(source_id) = enclosing else {
                    self.unsupported.push(Unsupported {
                        construct: "relationship",
                        id: raw,
                        reason: "the generalization is not nested in a classifier".to_owned(),
                    });
                    continue;
                };
                let Some(target) = node.attribute("general") else {
                    self.unsupported.push(Unsupported {
                        construct: "relationship",
                        id: raw,
                        reason: "the generalization has no general".to_owned(),
                    });
                    continue;
                };
                let Some(target_id) = self.endpoint(target, &raw) else {
                    continue;
                };
                relationships.push(ImportedRelationship {
                    id: map_id(&raw),
                    kind: "generalization".to_owned(),
                    source_id,
                    target_id,
                });
                continue;
            }

            let xmi_type = node.attribute("type").unwrap_or("");
            let local = bare_type(xmi_type);
            if local == "Association" {
                let ends: Vec<&Node> = node
                    .children
                    .iter()
                    .filter(|child| child.name == "ownedEnd")
                    .collect();
                let (Some(source), Some(target)) = (
                    ends.first().and_then(|end| end.attribute("type")),
                    ends.get(1).and_then(|end| end.attribute("type")),
                ) else {
                    self.unsupported.push(Unsupported {
                        construct: "relationship",
                        id: raw,
                        reason: "the association has fewer than two ownedEnds".to_owned(),
                    });
                    continue;
                };
                let aggregation = ends.iter().find_map(|end| end.attribute("aggregation"));
                let kind = match aggregation {
                    Some("composite") => "composition",
                    Some("shared") => "aggregation",
                    _ => "association",
                };
                let (Some(source_id), Some(target_id)) =
                    (self.endpoint(source, &raw), self.endpoint(target, &raw))
                else {
                    continue;
                };
                relationships.push(ImportedRelationship {
                    id: map_id(&raw),
                    kind: kind.to_owned(),
                    source_id,
                    target_id,
                });
                continue;
            }

            let Some(kind) = reverse(UML_RELATIONSHIP_TYPES, local) else {
                self.unsupported.push(Unsupported {
                    construct: "relationship",
                    id: raw,
                    reason: format!("xmi:type {xmi_type:?} is not in the UML profile"),
                });
                continue;
            };
            let (Some(source), Some(target)) =
                (node.attribute("client"), node.attribute("supplier"))
            else {
                self.unsupported.push(Unsupported {
                    construct: "relationship",
                    id: raw,
                    reason: "the relationship has no client or supplier".to_owned(),
                });
                continue;
            };
            let (Some(source_id), Some(target_id)) =
                (self.endpoint(source, &raw), self.endpoint(target, &raw))
            else {
                continue;
            };
            relationships.push(ImportedRelationship {
                id: map_id(&raw),
                kind: kind.to_owned(),
                source_id,
                target_id,
            });
        }
        Ok(relationships)
    }
}

fn read_uml(root: &Node) -> Result<ImportedModel, RenderError> {
    let model = if root.name == "XMI" {
        root.child("Model")
            .ok_or_else(|| RenderError::Xmi("the XMI document holds no uml:Model".to_owned()))?
    } else {
        root
    };
    let mut collector = UmlCollect {
        seen: BTreeSet::new(),
        unsupported: Vec::new(),
        elements: Vec::new(),
        declared: BTreeSet::new(),
        by_raw: BTreeMap::new(),
        pending: Vec::new(),
    };
    if let Some(raw) = model.attribute("id") {
        declare_id(&mut collector.seen, raw)?;
    }
    let title = model.attribute("name").unwrap_or("").to_owned();
    let description = model
        .children
        .iter()
        .find(|child| child.name == "ownedComment")
        .and_then(|comment| comment.child("body"))
        .map(Node::text)
        .unwrap_or_default();
    collector.walk(&model.children, None, None)?;
    let relationships = collector.relationships()?;

    let relationship_ids: BTreeSet<Uuid> = relationships
        .iter()
        .map(|relationship| relationship.id)
        .collect();
    let mut views = Vec::new();
    for extension in root
        .children
        .iter()
        .filter(|child| child.name == "Extension")
    {
        for node in extension
            .children
            .iter()
            .filter(|child| child.name == "view")
        {
            if let Some(view) = read_uml_view(
                node,
                &collector.by_raw,
                &relationship_ids,
                &mut collector.seen,
                &mut collector.unsupported,
            )? {
                views.push(view);
            }
        }
    }

    Ok(ImportedModel {
        language: "uml".to_owned(),
        language_version: "2.5.1".to_owned(),
        title,
        description,
        elements: collector.elements,
        relationships,
        views,
        unsupported: collector.unsupported,
    })
}

fn read_uml_view(
    node: &Node,
    elements: &BTreeMap<String, Uuid>,
    relationships: &BTreeSet<Uuid>,
    seen: &mut BTreeSet<String>,
    unsupported: &mut Vec<Unsupported>,
) -> Result<Option<ImportedView>, RenderError> {
    let Some(raw) = node_id(node, seen)? else {
        unsupported.push(Unsupported {
            construct: "view",
            id: String::new(),
            reason: "the view has no identifier".to_owned(),
        });
        return Ok(None);
    };
    let kept_elements: BTreeSet<Uuid> = elements.values().copied().collect();
    let mut objects = Vec::new();
    let mut object_ids: BTreeMap<String, Uuid> = BTreeMap::new();
    let mut index = 0.0_f64;
    for child in node.children.iter().filter(|child| child.name == "object") {
        let Some(object_raw) = node_id(child, seen)? else {
            unsupported.push(Unsupported {
                construct: "object",
                id: String::new(),
                reason: "the view object has no identifier".to_owned(),
            });
            continue;
        };
        let id = map_id(&object_raw);
        let element_id = match child.attribute("element") {
            Some(reference) => {
                let mapped = map_id(reference);
                if kept_elements.contains(&mapped) {
                    Some(mapped)
                } else {
                    unsupported.push(Unsupported {
                        construct: "object",
                        id: object_raw.clone(),
                        reason: format!(
                            "element {reference:?} was not carried; the object draws no element"
                        ),
                    });
                    None
                }
            }
            None => None,
        };
        let position = child
            .attribute("position")
            .and_then(|value| value.trim().parse::<f64>().ok())
            .unwrap_or(index);
        objects.push(ImportedObject {
            id,
            element_id,
            parent_id: None,
            kind: child.attribute("kind").unwrap_or("element").to_owned(),
            bounds: json!({
                "x": number_attribute(child, "x"),
                "y": number_attribute(child, "y"),
                "w": number_attribute(child, "w"),
                "h": number_attribute(child, "h"),
            }),
            label: child
                .attribute("label")
                .map(str::to_owned)
                .filter(|label| !label.is_empty()),
            position,
        });
        object_ids.insert(object_raw, id);
        index += 1.0;
    }

    let mut connections = Vec::new();
    for child in node
        .children
        .iter()
        .filter(|child| child.name == "connection")
    {
        let Some(connection_raw) = node_id(child, seen)? else {
            unsupported.push(Unsupported {
                construct: "connection",
                id: String::new(),
                reason: "the connection has no identifier".to_owned(),
            });
            continue;
        };
        let (Some(source), Some(target)) = (child.attribute("source"), child.attribute("target"))
        else {
            unsupported.push(Unsupported {
                construct: "connection",
                id: connection_raw,
                reason: "the connection has no source or target".to_owned(),
            });
            continue;
        };
        let (Some(source_id), Some(target_id)) = (
            object_ids.get(source).copied(),
            object_ids.get(target).copied(),
        ) else {
            unsupported.push(Unsupported {
                construct: "connection",
                id: connection_raw,
                reason: "an endpoint view object was not carried".to_owned(),
            });
            continue;
        };
        let relationship_id = match child.attribute("relationship") {
            Some(reference) => {
                let mapped = map_id(reference);
                if relationships.contains(&mapped) {
                    Some(mapped)
                } else {
                    unsupported.push(Unsupported {
                        construct: "connection",
                        id: connection_raw.clone(),
                        reason: format!(
                            "relationship {reference:?} was not carried; the connection draws a line"
                        ),
                    });
                    None
                }
            }
            None => None,
        };
        let bendpoints: Vec<Value> = child
            .children
            .iter()
            .filter(|child| child.name == "bendpoint")
            .map(|point| {
                json!({
                    "x": number_attribute(point, "x"),
                    "y": number_attribute(point, "y"),
                })
            })
            .collect();
        connections.push(ImportedConnection {
            id: map_id(&connection_raw),
            relationship_id,
            source_id,
            target_id,
            bendpoints: Value::Array(bendpoints),
        });
    }

    Ok(Some(ImportedView {
        id: map_id(&raw),
        kind: node.attribute("kind").unwrap_or("uml_diagram").to_owned(),
        name: node.attribute("name").unwrap_or("").to_owned(),
        diagram_kind: node
            .attribute("diagramKind")
            .map(str::to_owned)
            .filter(|kind| !kind.is_empty()),
        objects,
        connections,
    }))
}
