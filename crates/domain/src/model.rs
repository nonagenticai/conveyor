//! The model snapshot and the deterministic diagram layout (S08 Part B decisions 3 and 7).
//!
//! A [`ModelSnapshot`] is the one document a model's whole graph turns into. It is what
//! `models.publish` freezes into `revision.content`, what `models.import_xmi` builds, and
//! what [`crate::validate`] checks — one shape, so a published model, a validated model and
//! an imported model cannot disagree about what "the model" is.
//!
//! It is capped at **256 KiB** by the data architecture; a caller that would exceed it must
//! refuse the publish rather than truncate. [`ModelSnapshot::content_value`] is the document
//! whose SHA-256 is `revision.content_hash`, and [`ModelSnapshot::normalize`] sorts every
//! collection by id so the hash cannot depend on the order a query returned rows in.
//!
//! [`layout`] is the one placement algorithm. It lives here, not in `crates/render`, because
//! `model_views.generate` needs it in a slice where the renderer does not exist yet, and the
//! renderer must draw exactly the bounds the action stored — two implementations would drift.
//! The renderer's only other use is the case where **nothing is stored**: a model rendered
//! without a view, or a view with no objects, is placed by this same function rather than by a
//! second algorithm, so an auto-laid-out diagram and a generated one put an element in the
//! same place.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// One model's complete graph, as of the moment it was read.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelSnapshot {
    /// The model header.
    pub model: SnapshotModel,
    /// Every element, unordered until [`ModelSnapshot::normalize`] runs.
    pub elements: Vec<SnapshotElement>,
    /// Every relationship.
    pub relationships: Vec<SnapshotRelationship>,
    /// Every view, each with its own objects and connections.
    pub views: Vec<SnapshotView>,
}

/// The `model` row as it appears in a snapshot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapshotModel {
    /// `model.id`.
    pub id: Uuid,
    /// `archimate` or `uml`.
    pub language: String,
    /// `3.2` or `2.5.1`.
    pub language_version: String,
    /// The display title.
    pub title: String,
    /// The description; `''` when unset.
    pub description: String,
    /// Free-form metadata, read whole.
    pub meta: Value,
}

/// One `model_element` row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapshotElement {
    /// `model_element.id`.
    pub id: Uuid,
    /// The profile kind, e.g. `application_component`.
    pub kind: String,
    /// The display name.
    pub name: String,
    /// The long description; `''` when unset.
    pub documentation: String,
    /// The containment parent, when nested.
    pub parent_element_id: Option<Uuid>,
    /// Free-form metadata, read whole. `governed_asset_id` links to S03's asset.
    pub meta: Value,
}

/// One `model_relationship` row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapshotRelationship {
    /// `model_relationship.id`.
    pub id: Uuid,
    /// The profile relationship kind, e.g. `serving`.
    pub kind: String,
    /// The source element id. No foreign key — a dangling end is a validation finding.
    pub source_id: Uuid,
    /// The target element id.
    pub target_id: Uuid,
    /// Free-form metadata, read whole.
    pub meta: Value,
}

/// One `model_view` row with its contents.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapshotView {
    /// `model_view.id`.
    pub id: Uuid,
    /// `diagram`, `archimate_view` or `uml_diagram`.
    pub kind: String,
    /// The display name.
    pub name: String,
    /// Free-form metadata; `diagram_kind` names the UML diagram family when present.
    pub meta: Value,
    /// The objects on the view.
    pub objects: Vec<SnapshotObject>,
    /// The connectors between them.
    pub connections: Vec<SnapshotConnection>,
}

/// One `view_object` row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapshotObject {
    /// `view_object.id`.
    pub id: Uuid,
    /// The element this object draws, when it draws one.
    pub element_id: Option<Uuid>,
    /// The containing object, when nested on the canvas.
    pub parent_object_id: Option<Uuid>,
    /// The object kind; `element` when it draws an element.
    pub kind: String,
    /// `{x, y, w, h}` in canvas units. Read whole; never filtered on.
    pub bounds: Value,
    /// Free-form styling, read whole.
    pub style: Value,
    /// An override label; the element's name when absent.
    pub label: Option<String>,
    /// The z-order / left-to-right order.
    pub position: f64,
}

/// One `view_connection` row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapshotConnection {
    /// `view_connection.id`.
    pub id: Uuid,
    /// The model relationship this connector draws, when it draws one.
    pub relationship_id: Option<Uuid>,
    /// The object the connector starts at.
    pub source_object_id: Uuid,
    /// The object it ends at.
    pub target_object_id: Uuid,
    /// The route between them; read whole.
    pub bendpoints: Value,
    /// Free-form styling, read whole.
    pub style: Value,
}

impl ModelSnapshot {
    /// Sort every collection by id, so two reads of the same graph serialise identically.
    ///
    /// Without this the hash would depend on the order Postgres returned rows, and
    /// `model:<id>:<content_hash>` would dedupe two identical graphs apart.
    pub fn normalize(&mut self) {
        self.elements.sort_by_key(|element| element.id);
        self.relationships
            .sort_by_key(|relationship| relationship.id);
        for view in &mut self.views {
            view.objects.sort_by_key(|object| object.id);
            view.connections.sort_by_key(|connection| connection.id);
        }
        self.views.sort_by_key(|view| view.id);
    }

    /// The document whose canonical SHA-256 is `revision.content_hash`.
    pub fn content_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    /// `revision.content_hash` for this snapshot.
    pub fn content_hash(&self) -> String {
        crate::compare::content_hash(&self.content_value())
    }

    /// The serialised size in bytes — what the 256 KiB revision cap is measured against.
    pub fn content_len(&self) -> usize {
        crate::compare::canonical_json(&self.content_value()).len()
    }

    /// The element with `id`, when the snapshot holds one.
    pub fn element(&self, id: Uuid) -> Option<&SnapshotElement> {
        self.elements.iter().find(|element| element.id == id)
    }

    /// The relationship with `id`, when the snapshot holds one.
    pub fn relationship(&self, id: Uuid) -> Option<&SnapshotRelationship> {
        self.relationships.iter().find(|r| r.id == id)
    }
}

/// The revision cap: `revision.content` may not exceed this (data-arch §5).
pub const REVISION_CONTENT_MAX_BYTES: usize = 256 * 1024;

/// A placed element. Canvas units; the origin is the top-left of the diagram.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutBox {
    /// The element placed.
    pub element_id: Uuid,
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Box width.
    pub w: f64,
    /// Box height.
    pub h: f64,
}

/// Box width in canvas units.
pub const BOX_WIDTH: f64 = 180.0;
/// Box height in canvas units.
pub const BOX_HEIGHT: f64 = 90.0;
/// Horizontal gap between boxes in a row.
pub const COLUMN_GAP: f64 = 40.0;
/// Vertical gap between rows inside one kind band.
pub const ROW_GAP: f64 = 32.0;
/// Extra vertical gap between two kind bands.
pub const BAND_GAP: f64 = 64.0;
/// Boxes per row.
pub const COLUMNS: usize = 6;

/// Place every element in kind-banded rows ordered by `(kind, id)`.
///
/// Deterministic by construction: the bands are a `BTreeMap` (kind order), and within a band
/// the elements are sorted by id — so the same element set always produces the same boxes,
/// whatever order it arrived in. This is the geometry the renderer draws; it is never
/// recomputed there.
pub fn layout(elements: &[SnapshotElement]) -> Vec<LayoutBox> {
    let mut bands: BTreeMap<&str, Vec<&SnapshotElement>> = BTreeMap::new();
    for element in elements {
        bands
            .entry(element.kind.as_str())
            .or_default()
            .push(element);
    }

    let mut boxes = Vec::with_capacity(elements.len());
    let mut y = 0.0;
    for band in bands.values_mut() {
        band.sort_by_key(|element| element.id);
        for (index, element) in band.iter().enumerate() {
            let column = index % COLUMNS;
            let row = index / COLUMNS;
            boxes.push(LayoutBox {
                element_id: element.id,
                x: column as f64 * (BOX_WIDTH + COLUMN_GAP),
                y: y + row as f64 * (BOX_HEIGHT + ROW_GAP),
                w: BOX_WIDTH,
                h: BOX_HEIGHT,
            });
        }
        let rows = band.len().div_ceil(COLUMNS);
        y += rows as f64 * (BOX_HEIGHT + ROW_GAP) + BAND_GAP;
    }
    boxes
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn element(kind: &str, id: Uuid) -> SnapshotElement {
        SnapshotElement {
            id,
            kind: kind.to_owned(),
            name: format!("{kind}-{id}"),
            documentation: String::new(),
            parent_element_id: None,
            meta: json!({}),
        }
    }

    fn snapshot(elements: Vec<SnapshotElement>) -> ModelSnapshot {
        ModelSnapshot {
            model: SnapshotModel {
                id: Uuid::now_v7(),
                language: "archimate".to_owned(),
                language_version: "3.2".to_owned(),
                title: "T".to_owned(),
                description: String::new(),
                meta: json!({}),
            },
            elements,
            relationships: Vec::new(),
            views: Vec::new(),
        }
    }

    #[test]
    fn normalize_makes_the_hash_independent_of_insertion_order() {
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();
        let c = Uuid::now_v7();

        let mut first = snapshot(vec![
            element("node", a),
            element("goal", b),
            element("node", c),
        ]);
        let mut second = snapshot(vec![
            element("node", c),
            element("node", a),
            element("goal", b),
        ]);
        // The header is part of the document; only the collection order may differ.
        second.model.id = first.model.id;
        first.normalize();
        second.normalize();
        assert_eq!(first.content_hash(), second.content_hash());
    }

    #[test]
    fn a_changed_name_changes_the_hash() {
        let id = Uuid::now_v7();
        let mut before = snapshot(vec![element("node", id)]);
        let mut after = snapshot(vec![element("node", id)]);
        after.elements[0].name.push('X');
        before.normalize();
        after.normalize();
        assert_ne!(before.content_hash(), after.content_hash());
    }

    #[test]
    fn layout_is_stable_and_banded() {
        let elements = vec![
            element("node", Uuid::now_v7()),
            element("goal", Uuid::now_v7()),
            element("node", Uuid::now_v7()),
        ];
        let first = layout(&elements);
        let mut shuffled = elements.clone();
        shuffled.reverse();
        let second = layout(&shuffled);
        assert_eq!(first, second, "layout must not depend on input order");
        assert_eq!(first.len(), 3);

        // `goal` sorts before `node` in the BTreeMap, so the goal band is at y = 0 and the
        // node band starts below it.
        let goal_id = elements
            .iter()
            .find(|element| element.kind == "goal")
            .expect("a goal is placed")
            .id;
        let goal_y = first
            .iter()
            .find(|b| b.element_id == goal_id)
            .map(|b| b.y)
            .expect("the goal has a box");
        assert_eq!(goal_y, 0.0);
        let node_y = BOX_HEIGHT + ROW_GAP + BAND_GAP;
        for b in first.iter().filter(|b| b.element_id != goal_id) {
            assert_eq!(b.y, node_y);
        }
    }

    #[test]
    fn a_band_wraps_after_six_columns() {
        let ids: Vec<Uuid> = (0..7).map(|_| Uuid::now_v7()).collect();
        let elements: Vec<SnapshotElement> = ids.iter().map(|id| element("node", *id)).collect();
        let boxes = layout(&elements);
        assert_eq!(boxes.len(), 7);
        let ys: std::collections::BTreeSet<String> =
            boxes.iter().map(|b| format!("{:.1}", b.y)).collect();
        assert_eq!(ys.len(), 2, "seven boxes occupy two rows");
        let xs: std::collections::BTreeSet<String> =
            boxes.iter().map(|b| format!("{:.1}", b.x)).collect();
        assert_eq!(xs.len(), COLUMNS, "the first row fills every column");
    }

    #[test]
    fn the_revision_cap_is_two_hundred_and_fifty_six_kibibytes() {
        assert_eq!(REVISION_CONTENT_MAX_BYTES, 262_144);
    }
}
