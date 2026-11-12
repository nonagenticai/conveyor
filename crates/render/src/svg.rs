#![forbid(unsafe_code)]
//! The SVG document — the one artefact the PNG and the PDF are encoded from.
//!
//! Nothing here is a hash map, a counter or a clock: every id is derived from a UUID, every
//! collection is ordered, and numbers are formatted through [`ids::number`]. Two runs over the
//! same snapshot therefore produce the same bytes, which is what the golden fixtures assert.
//!
//! # Where the geometry comes from
//!
//! * A view that has objects is drawn from the **stored** `view_object.bounds` and the stored
//!   `view_connection.bendpoints`. The renderer never recomputes a position the app persisted —
//!   a diagram the user arranged must look like the diagram they arranged.
//! * A model rendered without a view (`view_id: None`), or a view with no objects, is drawn
//!   from [`conveyor_domain::model::layout`] — the same kind-banded placement
//!   `model_views.generate` stores, so the two paths cannot disagree about where an element
//!   belongs.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use conveyor_domain::model::{
    ModelSnapshot, SnapshotConnection, SnapshotObject, SnapshotView, layout,
};
use conveyor_domain::profile::{Layer, Profile, for_language};
use serde_json::Value;
use uuid::Uuid;

use crate::{CANVAS_HEIGHT, CANVAS_WIDTH, PAGE_MARGIN, RenderError, font, ids};

/// Padding inside an element box.
const BOX_PAD: f64 = 10.0;
/// Name text size.
const NAME_SIZE: f64 = font::BASE_SIZE;
/// Name line height.
const NAME_LINE: f64 = 14.0;
/// Kind caption size.
const KIND_SIZE: f64 = 9.0;
/// Box width when a `view_object` carries no bounds.
const DEFAULT_WIDTH: f64 = 180.0;
/// Box height when a `view_object` carries no bounds.
const DEFAULT_HEIGHT: f64 = 90.0;
/// How many wrapped name lines fit before the text is ellipsised.
const MAX_NAME_LINES: usize = 3;
/// Stroke width for boxes and connectors.
const STROKE: f64 = 1.5;

/// How a shape is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Form {
    /// A rounded rectangle with a label and a kind caption.
    Box,
    /// A UML sequence lifeline: a numbered header over a dashed vertical line.
    Lifeline,
}

/// One placed thing on the canvas.
#[derive(Clone, Debug)]
struct Shape {
    /// The element this draws, when it draws one — becomes the SVG id `e-<uuid>`.
    element_id: Option<Uuid>,
    /// The `view_object` (or the element, for a synthetic view) — the fallback SVG id.
    object_id: Uuid,
    label: String,
    kind: String,
    layer: Option<Layer>,
    form: Form,
    position: f64,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// One connector.
#[derive(Clone, Debug)]
struct Wire {
    id: Uuid,
    /// True when the id is a relationship's (`r-…`) rather than a connection's (`c-…`).
    synthetic: bool,
    kind: String,
    points: Vec<(f64, f64)>,
}

/// Render one view of `model` as an SVG document.
pub(crate) fn render(model: &ModelSnapshot, view_id: Option<Uuid>) -> Result<Vec<u8>, RenderError> {
    let profile =
        for_language(&model.model.language, &model.model.language_version).map_err(|_| {
            RenderError::UnknownProfile {
                language: model.model.language.clone(),
                version: model.model.language_version.clone(),
            }
        })?;
    let view = match view_id {
        Some(id) => Some(
            model
                .views
                .iter()
                .find(|view| view.id == id)
                .ok_or(RenderError::ViewNotFound(id))?,
        ),
        // No view named: the whole model, auto-laid-out. This is the only reading that lets a
        // freshly imported (view-less) model be drawn.
        None => None,
    };
    let (mut shapes, wires) = plan(model, view, profile)?;
    // Painting order is z-order: by `position`, then by id so two objects with equal position
    // cannot swap between runs.
    shapes.sort_by(|a, b| {
        a.position
            .total_cmp(&b.position)
            .then_with(|| a.object_id.cmp(&b.object_id))
    });
    Ok(emit(view.map(|view| view.id), &shapes, &wires)?.into_bytes())
}

fn plan(
    model: &ModelSnapshot,
    view: Option<&SnapshotView>,
    profile: &Profile,
) -> Result<(Vec<Shape>, Vec<Wire>), RenderError> {
    match view {
        Some(view) if !view.objects.is_empty() => {
            if is_sequence(view) {
                Ok(sequence_plan(model, view, profile))
            } else {
                Ok(diagram_plan(model, view, profile))
            }
        }
        _ => Ok(synthetic_plan(model, profile)),
    }
}

/// A UML sequence diagram is drawn as lifelines, not as boxes.
///
/// `model_view.meta.diagram_kind` names the family; the profile carries the five legal values.
fn is_sequence(view: &SnapshotView) -> bool {
    view.meta
        .get("diagram_kind")
        .and_then(Value::as_str)
        .is_some_and(|kind| kind == "sequence")
}

// ── real views ──────────────────────────────────────────────────────────────────────────────

fn diagram_plan(
    model: &ModelSnapshot,
    view: &SnapshotView,
    profile: &Profile,
) -> (Vec<Shape>, Vec<Wire>) {
    let mut shapes = Vec::with_capacity(view.objects.len());
    let mut rects: BTreeMap<Uuid, (f64, f64, f64, f64)> = BTreeMap::new();

    for object in &view.objects {
        let (x, y, w, h) = bounds_of(object);
        let element = object.element_id.and_then(|id| model.element(id));
        let kind = element
            .map(|element| element.kind.clone())
            .unwrap_or_else(|| object.kind.clone());
        let label = object
            .label
            .clone()
            .or_else(|| element.map(|element| element.name.clone()))
            .unwrap_or_else(|| object.kind.clone());
        shapes.push(Shape {
            element_id: object.element_id,
            object_id: object.id,
            label,
            layer: layer_of(profile, &kind),
            kind,
            form: Form::Box,
            position: object.position,
            x,
            y,
            w,
            h,
        });
        rects.insert(object.id, (x, y, w, h));
    }

    // A connector draws the relationship it names; the kind decides the marker and the dash.
    // Resolving it here (rather than defaulting to `association`) is what stops a composition
    // being drawn as a plain arrow — deterministic bytes are not the same as a right diagram.
    let kinds: BTreeMap<Uuid, &str> = model
        .relationships
        .iter()
        .map(|relationship| (relationship.id, relationship.kind.as_str()))
        .collect();
    let wires = view
        .connections
        .iter()
        .filter_map(|connection| connection_wire(connection, &rects, &kinds))
        .collect();
    (shapes, wires)
}

/// A connector runs centre-to-centre and is clipped to each box, so it never crosses a shape
/// it does not connect. Stored bendpoints, when present, are absolute canvas coordinates.
fn connection_wire(
    connection: &SnapshotConnection,
    rects: &BTreeMap<Uuid, (f64, f64, f64, f64)>,
    kinds: &BTreeMap<Uuid, &str>,
) -> Option<Wire> {
    let source = rects.get(&connection.source_object_id)?;
    let target = rects.get(&connection.target_object_id)?;
    let bends = bendpoints(&connection.bendpoints);
    let first = bends.first().copied().unwrap_or(centre(*target));
    let last = bends.last().copied().unwrap_or(centre(*source));
    let mut points = vec![clip(*source, first)];
    points.extend(bends);
    points.push(clip(*target, last));
    // A connector with no relationship (a hand-drawn line) is an association; one that names
    // a relationship takes that kind. An unresolvable id is also an association — the view is
    // drawn, and `models.validate` reports the dangling reference.
    let kind = connection
        .relationship_id
        .and_then(|id| kinds.get(&id).copied())
        .unwrap_or("association")
        .to_owned();
    Some(Wire {
        id: connection.id,
        synthetic: false,
        kind,
        points,
    })
}

// ── synthetic view ─────────────────────────────────────────────────────────────────────────

fn synthetic_plan(model: &ModelSnapshot, profile: &Profile) -> (Vec<Shape>, Vec<Wire>) {
    let mut shapes = Vec::with_capacity(model.elements.len());
    let mut rects: BTreeMap<Uuid, (f64, f64, f64, f64)> = BTreeMap::new();

    for (index, boxed) in layout(&model.elements).into_iter().enumerate() {
        let Some(element) = model.element(boxed.element_id) else {
            continue;
        };
        shapes.push(Shape {
            element_id: Some(element.id),
            object_id: element.id,
            label: element.name.clone(),
            kind: element.kind.clone(),
            layer: layer_of(profile, &element.kind),
            form: Form::Box,
            position: index as f64,
            x: boxed.x,
            y: boxed.y,
            w: boxed.w,
            h: boxed.h,
        });
        rects.insert(element.id, (boxed.x, boxed.y, boxed.w, boxed.h));
    }

    let wires = model
        .relationships
        .iter()
        .filter_map(|relationship| {
            let source = rects.get(&relationship.source_id)?;
            let target = rects.get(&relationship.target_id)?;
            Some(Wire {
                id: relationship.id,
                synthetic: true,
                kind: relationship.kind.clone(),
                points: vec![
                    clip(*source, centre(*target)),
                    clip(*target, centre(*source)),
                ],
            })
        })
        .collect();
    (shapes, wires)
}

// ── sequence diagrams ──────────────────────────────────────────────────────────────────────

/// Lifelines across the page, messages down it.
///
/// The slice's declared default (S08 §10 Q4): lifelines and ordered messages; combined
/// fragments, executions and gates are out. Message order is `(position, id)` of the *source*
/// object, then the connection id — deterministic, and the order the objects were arranged in.
fn sequence_plan(
    model: &ModelSnapshot,
    view: &SnapshotView,
    profile: &Profile,
) -> (Vec<Shape>, Vec<Wire>) {
    let mut columns: Vec<&SnapshotObject> = view.objects.iter().collect();
    columns.sort_by(|a, b| {
        a.position
            .total_cmp(&b.position)
            .then_with(|| a.id.cmp(&b.id))
    });

    let column_width = 200.0;
    let header_height = 48.0;
    let row_height = 46.0;
    let height = 120.0 + row_height * (view.connections.len().max(1) as f64);

    let mut shapes = Vec::with_capacity(columns.len());
    let mut centres: BTreeMap<Uuid, f64> = BTreeMap::new();
    for (index, object) in columns.iter().enumerate() {
        let centre_x = index as f64 * column_width + column_width / 2.0;
        centres.insert(object.id, centre_x);
        let element = object.element_id.and_then(|id| model.element(id));
        let kind = element
            .map(|element| element.kind.clone())
            .unwrap_or_else(|| object.kind.clone());
        let label = object
            .label
            .clone()
            .or_else(|| element.map(|element| element.name.clone()))
            .unwrap_or_else(|| object.kind.clone());
        shapes.push(Shape {
            element_id: object.element_id,
            object_id: object.id,
            label,
            layer: layer_of(profile, &kind),
            kind,
            form: Form::Lifeline,
            position: object.position,
            x: centre_x - 70.0,
            y: 0.0,
            w: 140.0,
            h: height.max(header_height + 40.0),
        });
    }

    let mut ordered: Vec<&SnapshotConnection> = view.connections.iter().collect();
    ordered.sort_by(|a, b| {
        let left = centre_of(&centres, a.source_object_id);
        let right = centre_of(&centres, b.source_object_id);
        left.total_cmp(&right)
            .then_with(|| {
                centre_of(&centres, a.target_object_id)
                    .total_cmp(&centre_of(&centres, b.target_object_id))
            })
            .then_with(|| a.id.cmp(&b.id))
    });

    let mut wires = Vec::with_capacity(ordered.len());
    for (row, connection) in ordered.iter().enumerate() {
        let (Some(&from), Some(&to)) = (
            centres.get(&connection.source_object_id),
            centres.get(&connection.target_object_id),
        ) else {
            continue;
        };
        let y = header_height + row_height * (row as f64 + 1.0);
        wires.push(Wire {
            id: connection.id,
            synthetic: false,
            kind: "message".to_owned(),
            points: vec![(from, y), (to, y)],
        });
    }
    (shapes, wires)
}

fn centre_of(centres: &BTreeMap<Uuid, f64>, id: Uuid) -> f64 {
    centres.get(&id).copied().unwrap_or(f64::MAX)
}

// ── geometry ───────────────────────────────────────────────────────────────────────────────

fn layer_of(profile: &Profile, kind: &str) -> Option<Layer> {
    profile.element(kind).and_then(|entry| entry.layer)
}

fn bounds_of(object: &SnapshotObject) -> (f64, f64, f64, f64) {
    let read = |key: &str, default: f64| {
        object
            .bounds
            .get(key)
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
            .unwrap_or(default)
    };
    (
        read("x", 0.0),
        read("y", 0.0),
        read("w", DEFAULT_WIDTH).max(1.0),
        read("h", DEFAULT_HEIGHT).max(1.0),
    )
}

fn bendpoints(value: &Value) -> Vec<(f64, f64)> {
    value
        .as_array()
        .map(|points| {
            points
                .iter()
                .filter_map(|point| Some((point.get("x")?.as_f64()?, point.get("y")?.as_f64()?)))
                .filter(|(x, y)| x.is_finite() && y.is_finite())
                .collect()
        })
        .unwrap_or_default()
}

fn centre(rect: (f64, f64, f64, f64)) -> (f64, f64) {
    (rect.0 + rect.2 / 2.0, rect.1 + rect.3 / 2.0)
}

/// Where the segment from `rect`'s centre towards `towards` leaves the rectangle.
fn clip(rect: (f64, f64, f64, f64), towards: (f64, f64)) -> (f64, f64) {
    let (cx, cy) = centre(rect);
    let dx = towards.0 - cx;
    let dy = towards.1 - cy;
    if dx == 0.0 && dy == 0.0 {
        return (cx, cy);
    }
    let half_w = rect.2 / 2.0;
    let half_h = rect.3 / 2.0;
    let scale_x = if dx == 0.0 {
        f64::INFINITY
    } else {
        half_w / dx.abs()
    };
    let scale_y = if dy == 0.0 {
        f64::INFINITY
    } else {
        half_h / dy.abs()
    };
    let scale = scale_x.min(scale_y);
    (cx + dx * scale, cy + dy * scale)
}

// ── emission ───────────────────────────────────────────────────────────────────────────────

/// A relationship kind's line style: structural relationships are solid, everything else is
/// dashed.
///
/// `generalization` is solid because UML draws it that way; `realization` is dashed, which is
/// right for UML and acceptable for ArchiMate (whose realization carries an arrow rather than
/// the hollow triangle). One rule per kind, not per language — a diagram cannot be drawn twice.
fn dashed(kind: &str) -> bool {
    !matches!(
        kind,
        "composition"
            | "aggregation"
            | "assignment"
            | "specialization"
            | "generalization"
            | "association"
    )
}

/// Which end carries a marker, and what shape it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Marker {
    Arrow,
    HollowTriangle,
    FilledDiamond,
    HollowDiamond,
}

fn markers(kind: &str) -> (Option<Marker>, Option<Marker>) {
    match kind {
        "composition" => (Some(Marker::FilledDiamond), None),
        "aggregation" => (Some(Marker::HollowDiamond), None),
        "generalization" | "realization" | "specialization" => (None, Some(Marker::HollowTriangle)),
        _ => (None, Some(Marker::Arrow)),
    }
}

fn layer_colours(layer: Option<Layer>) -> (&'static str, &'static str) {
    match layer {
        Some(Layer::Strategy) => ("#fdf1dc", "#b0822f"),
        Some(Layer::Business) => ("#fff2cc", "#c9a227"),
        Some(Layer::Application) => ("#dae8fc", "#5b8ac6"),
        Some(Layer::Technology) => ("#d5e8d4", "#6a9c5a"),
        Some(Layer::Motivation) => ("#e6dcf0", "#8b6aa8"),
        Some(Layer::Implementation) => ("#ffe6cc", "#c47f36"),
        Some(Layer::Composite) => ("#f1f5f9", "#94a3b8"),
        None => ("#ffffff", "#475569"),
    }
}

fn emit(view: Option<Uuid>, shapes: &[Shape], wires: &[Wire]) -> Result<String, RenderError> {
    // Every text run is checked against the embedded face before a byte is written. A
    // substituted glyph is a diagram that quietly lies about a name, so it is a refusal.
    for shape in shapes {
        font::ensure_covered(&shape.label)?;
        font::ensure_covered(&shape.kind)?;
    }
    let (min_x, min_y, max_x, max_y) = content_bounds(shapes, wires);
    let content_w = (max_x - min_x).max(1.0);
    let content_h = (max_y - min_y).max(1.0);
    let available_w = CANVAS_WIDTH - 2.0 * PAGE_MARGIN;
    let available_h = CANVAS_HEIGHT - 2.0 * PAGE_MARGIN;

    // Fit, never magnify: a two-element diagram at 1:1 reads better than the same diagram
    // blown up to fill A4, and the page stays a page.
    let scale = (available_w / content_w)
        .min(available_h / content_h)
        .min(1.0);
    let tx = (CANVAS_WIDTH - content_w * scale) / 2.0 - min_x * scale;
    let ty = (CANVAS_HEIGHT - content_h * scale) / 2.0 - min_y * scale;

    let mut document = String::with_capacity(8 * 1024 + shapes.len() * 512);
    let _ = writeln!(
        document,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" \
         viewBox=\"0 0 {w} {h}\" font-family=\"{family}\">",
        w = ids::number(CANVAS_WIDTH),
        h = ids::number(CANVAS_HEIGHT),
        family = font::FAMILY,
    );

    let _ = writeln!(
        document,
        "<rect x=\"0\" y=\"0\" width=\"{w}\" height=\"{h}\" fill=\"#ffffff\"/>\n\
         <g{anchor} transform=\"translate({tx}, {ty}) scale({scale})\">",
        w = ids::number(CANVAS_WIDTH),
        h = ids::number(CANVAS_HEIGHT),
        anchor = view
            .map(|id| format!(" id=\"{}\"", ids::view(id)))
            .unwrap_or_default(),
        tx = ids::number(tx),
        ty = ids::number(ty),
        scale = ids::number(scale),
    );

    for wire in wires {
        emit_wire(&mut document, wire);
    }
    for shape in shapes {
        emit_shape(&mut document, shape);
    }

    document.push_str("</g>\n</svg>\n");
    Ok(document)
}

/// The world-space length of a marker from tip to base: `markerWidth 9` in stroke-width
/// units (`9 × STROKE`). The viewBox is 10, so the marker's own scale is `9 × STROKE / 10`.
const MARKER_LENGTH: f64 = 9.0 * STROKE;
/// Half the marker's width: viewBox half-height 5 × the marker scale.
const MARKER_HALF: f64 = 4.5 * STROKE;
/// The outline width of a hollow marker: viewBox 1 × the marker scale.
const MARKER_STROKE: f64 = 0.9 * STROKE;

/// Draw one relationship marker as an explicit path at its wire's endpoint.
///
/// The old `<marker orient="auto-start-reverse">` made `usvg` compute the rotation with the
/// host's libm, and the PDF printed the unrounded matrix; the same diagram then had two byte
/// streams on two platforms. This geometry uses only `+ - * /` and `sqrt`, and every
/// coordinate goes through [`ids::number`], so the SVG, the PNG and the PDF are identical
/// everywhere. `auto-start-reverse` is the reversed direction at the start.
fn emit_marker(document: &mut String, wire: &Wire, marker: Marker, at_start: bool) {
    let Some(((tx, ty), (dx, dy))) = marker_frame(&wire.points, at_start) else {
        return;
    };
    // `d` points base → tip; `n` is its perpendicular, so the base corners are `base ± half·n`.
    let (nx, ny) = (-dy, dx);
    let (bx, by) = (tx - dx * MARKER_LENGTH, ty - dy * MARKER_LENGTH);
    let (mx, my) = (tx - dx * MARKER_HALF, ty - dy * MARKER_HALF);
    let path = match marker {
        Marker::Arrow | Marker::HollowTriangle => format!(
            "M {} {} L {} {} L {} {} z",
            ids::number(tx),
            ids::number(ty),
            ids::number(bx - nx * MARKER_HALF),
            ids::number(by - ny * MARKER_HALF),
            ids::number(bx + nx * MARKER_HALF),
            ids::number(by + ny * MARKER_HALF),
        ),
        Marker::FilledDiamond | Marker::HollowDiamond => format!(
            "M {} {} L {} {} L {} {} L {} {} z",
            ids::number(tx),
            ids::number(ty),
            ids::number(mx - nx * MARKER_HALF),
            ids::number(my - ny * MARKER_HALF),
            ids::number(bx),
            ids::number(by),
            ids::number(mx + nx * MARKER_HALF),
            ids::number(my + ny * MARKER_HALF),
        ),
    };
    let paint = match marker {
        Marker::Arrow | Marker::FilledDiamond => "fill=\"#334155\"".to_owned(),
        Marker::HollowTriangle | Marker::HollowDiamond => format!(
            "fill=\"#ffffff\" stroke=\"#334155\" stroke-width=\"{}\"",
            ids::number(MARKER_STROKE)
        ),
    };
    let _ = writeln!(document, "<path d=\"{path}\" {paint}/>");
}

/// The tip and outward unit direction for a marker at the start or the end of a wire.
///
/// Walks over zero-length segments; a wire with no extent carries no marker.
fn marker_frame(points: &[(f64, f64)], at_start: bool) -> Option<((f64, f64), (f64, f64))> {
    if points.len() < 2 {
        return None;
    }
    if at_start {
        let tip = points[0];
        for pair in points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if let Some(direction) = unit(a.0 - b.0, a.1 - b.1) {
                return Some((tip, direction));
            }
        }
    } else {
        let tip = *points.last()?;
        for pair in points.windows(2).rev() {
            let (a, b) = (pair[0], pair[1]);
            if let Some(direction) = unit(b.0 - a.0, b.1 - a.1) {
                return Some((tip, direction));
            }
        }
    }
    None
}

fn unit(dx: f64, dy: f64) -> Option<(f64, f64)> {
    let length = (dx * dx + dy * dy).sqrt();
    (length > 0.0).then(|| (dx / length, dy / length))
}

fn emit_wire(document: &mut String, wire: &Wire) {
    let mut path = String::with_capacity(wire.points.len() * 18);
    for (index, (x, y)) in wire.points.iter().enumerate() {
        let command = if index == 0 { 'M' } else { 'L' };
        let _ = write!(path, "{command} {} {}", ids::number(*x), ids::number(*y));
        if index + 1 != wire.points.len() {
            path.push(' ');
        }
    }
    let dash = if dashed(&wire.kind) {
        " stroke-dasharray=\"6 4\""
    } else {
        ""
    };
    let id = if wire.synthetic {
        ids::relationship(wire.id)
    } else {
        ids::connection(wire.id)
    };
    let _ = writeln!(
        document,
        "<path id=\"{id}\" d=\"{path}\" fill=\"none\" stroke=\"#334155\" \
         stroke-width=\"{stroke}\"{dash}/>",
        stroke = ids::number(STROKE),
    );

    // Markers paint after the path's own stroke, so they are written after it. The shapes
    // precede every element box, which keeps the old z-order.
    let (start, end) = markers(&wire.kind);
    if let Some(marker) = start {
        emit_marker(document, wire, marker, true);
    }
    if let Some(marker) = end {
        emit_marker(document, wire, marker, false);
    }
}

fn emit_shape(document: &mut String, shape: &Shape) {
    let id = match shape.element_id {
        Some(element) => ids::element(element),
        None => ids::object(shape.object_id),
    };
    let (fill, stroke) = layer_colours(shape.layer);

    document.push_str("<g id=\"");
    document.push_str(&id);
    document.push_str("\">\n");

    match shape.form {
        Form::Box => {
            let _ = writeln!(
                document,
                "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" rx=\"6\" \
                 fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"{sw}\"/>",
                x = ids::number(shape.x),
                y = ids::number(shape.y),
                w = ids::number(shape.w),
                h = ids::number(shape.h),
                sw = ids::number(STROKE),
            );
            let inner = shape.w - 2.0 * BOX_PAD;
            let lines = wrap(&shape.label, inner, NAME_SIZE);
            let block = lines.len() as f64 * NAME_LINE;
            let first = shape.y + (shape.h - block) / 2.0 + NAME_SIZE * 0.85;
            for (index, line) in lines.iter().enumerate() {
                let _ = writeln!(
                    document,
                    "<text x=\"{x}\" y=\"{y}\" text-anchor=\"middle\" font-size=\"{size}\" \
                     fill=\"#0f172a\">{text}</text>",
                    x = ids::number(shape.x + shape.w / 2.0),
                    y = ids::number(first + index as f64 * NAME_LINE),
                    size = ids::number(NAME_SIZE),
                    text = ids::escape(line),
                );
            }
            // The kind caption is what makes a diagram readable without a legend; it is
            // truncated rather than wrapped, so it never pushes the name around.
            let caption = truncate(&shape.kind, inner, KIND_SIZE);
            let _ = writeln!(
                document,
                "<text x=\"{x}\" y=\"{y}\" text-anchor=\"middle\" font-size=\"{size}\" \
                 fill=\"#64748b\">{text}</text>",
                x = ids::number(shape.x + shape.w / 2.0),
                y = ids::number(shape.y + shape.h - 6.0),
                size = ids::number(KIND_SIZE),
                text = ids::escape(&caption),
            );
        }
        Form::Lifeline => {
            let centre_x = shape.x + shape.w / 2.0;
            let _ = writeln!(
                document,
                "<rect x=\"{x}\" y=\"0\" width=\"{w}\" height=\"44\" rx=\"6\" \
                 fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"{sw}\"/>\n\
                 <line x1=\"{cx}\" y1=\"44\" x2=\"{cx}\" y2=\"{h}\" stroke=\"{stroke}\" \
                 stroke-width=\"1\" stroke-dasharray=\"5 5\"/>",
                x = ids::number(shape.x),
                w = ids::number(shape.w),
                cx = ids::number(centre_x),
                h = ids::number(shape.h),
                sw = ids::number(STROKE),
            );
            let lines = wrap(&shape.label, shape.w - 2.0 * BOX_PAD, NAME_SIZE);
            let first = 22.0 - (lines.len().saturating_sub(1) as f64) * NAME_LINE / 2.0;
            for (index, line) in lines.iter().take(MAX_NAME_LINES).enumerate() {
                let _ = writeln!(
                    document,
                    "<text x=\"{x}\" y=\"{y}\" text-anchor=\"middle\" font-size=\"{size}\" \
                     fill=\"#0f172a\">{text}</text>",
                    x = ids::number(centre_x),
                    y = ids::number(first + index as f64 * NAME_LINE),
                    size = ids::number(NAME_SIZE),
                    text = ids::escape(line),
                );
            }
        }
    }

    document.push_str("</g>\n");
}

fn content_bounds(shapes: &[Shape], wires: &[Wire]) -> (f64, f64, f64, f64) {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for shape in shapes {
        min_x = min_x.min(shape.x);
        min_y = min_y.min(shape.y);
        max_x = max_x.max(shape.x + shape.w);
        max_y = max_y.max(shape.y + shape.h);
    }
    for wire in wires {
        for (x, y) in &wire.points {
            min_x = min_x.min(*x);
            min_y = min_y.min(*y);
            max_x = max_x.max(*x);
            max_y = max_y.max(*y);
        }
    }
    if !min_x.is_finite() {
        return (0.0, 0.0, 1.0, 1.0);
    }
    (min_x, min_y, max_x, max_y)
}

// ── text ───────────────────────────────────────────────────────────────────────────────────

/// Greedy wrap to `width`, hard-breaking a word that cannot fit on its own line.
fn wrap(text: &str, width: f64, size: f64) -> Vec<String> {
    if width <= 0.0 {
        return Vec::new();
    }
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_owned()
        } else {
            format!("{current} {word}")
        };
        if font::advance(&candidate, size) <= width {
            current = candidate;
            continue;
        }
        if !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        if font::advance(word, size) <= width {
            current = word.to_owned();
            continue;
        }
        // A single word wider than the box: break it at the last character that fits.
        let mut chunk = String::new();
        for character in word.chars() {
            let candidate = format!("{chunk}{character}");
            if !chunk.is_empty() && font::advance(&candidate, size) > width {
                lines.push(std::mem::take(&mut chunk));
            }
            chunk.push(character);
        }
        current = chunk;
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.len() > MAX_NAME_LINES {
        lines.truncate(MAX_NAME_LINES);
        if let Some(last) = lines.last_mut() {
            let ellipsis = font::advance("…", size);
            *last = truncate(last, width - ellipsis, size);
            last.push('…');
        }
    }
    lines
}

/// The longest prefix of `text` that fits in `width`, at `size`.
fn truncate(text: &str, width: f64, size: f64) -> String {
    if font::advance(text, size) <= width {
        return text.to_owned();
    }
    let mut out = String::new();
    for character in text.chars() {
        let candidate = format!("{out}{character}");
        if !out.is_empty() && font::advance(&format!("{candidate}…"), size) > width {
            break;
        }
        out = candidate;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use conveyor_domain::model::{SnapshotModel, SnapshotObject};
    use serde_json::json;

    fn object(id: Uuid, position: f64, bounds: Value) -> SnapshotObject {
        SnapshotObject {
            id,
            element_id: None,
            parent_object_id: None,
            kind: "note".to_owned(),
            bounds,
            style: json!({}),
            label: Some("note".to_owned()),
            position,
        }
    }

    #[test]
    fn a_connector_is_clipped_to_both_boxes() {
        let left = (0.0, 0.0, 100.0, 50.0);
        let right = (200.0, 0.0, 100.0, 50.0);
        let start = clip(left, centre(right));
        let end = clip(right, centre(left));
        assert_eq!(start, (100.0, 25.0));
        assert_eq!(end, (200.0, 25.0));
    }

    #[test]
    fn stored_bendpoints_are_followed_in_order() {
        let connection = SnapshotConnection {
            id: Uuid::from_u128(0x77),
            relationship_id: None,
            source_object_id: Uuid::from_u128(1),
            target_object_id: Uuid::from_u128(2),
            bendpoints: json!([{"x": 150.0, "y": 300.0}, {"x": 400.0, "y": 300.0}]),
            style: json!({}),
        };
        let mut rects = BTreeMap::new();
        rects.insert(Uuid::from_u128(1), (0.0, 0.0, 100.0, 50.0));
        rects.insert(Uuid::from_u128(2), (500.0, 0.0, 100.0, 50.0));
        let wire = connection_wire(&connection, &rects, &BTreeMap::new())
            .expect("the connection resolves");
        // start-clip, bendpoint, bendpoint, end-clip
        assert_eq!(wire.points.len(), 4);
        assert_eq!(wire.points[1], (150.0, 300.0));
        assert_eq!(wire.points[2], (400.0, 300.0));
        // The route leaves the first box towards its first bendpoint, not towards the target.
        assert!(
            wire.points[0].1 > 25.0,
            "the start is pulled downwards: {:?}",
            wire.points[0]
        );
    }

    #[test]
    fn a_connector_draws_the_relationship_it_names() {
        let relationship = Uuid::from_u128(0x90);
        let connection = SnapshotConnection {
            id: Uuid::from_u128(0x91),
            relationship_id: Some(relationship),
            source_object_id: Uuid::from_u128(1),
            target_object_id: Uuid::from_u128(2),
            bendpoints: json!([]),
            style: json!({}),
        };
        let mut rects = BTreeMap::new();
        rects.insert(Uuid::from_u128(1), (0.0, 0.0, 100.0, 50.0));
        rects.insert(Uuid::from_u128(2), (200.0, 0.0, 100.0, 50.0));
        let mut kinds = BTreeMap::new();
        kinds.insert(relationship, "composition");
        let wire = connection_wire(&connection, &rects, &kinds).expect("resolves");
        assert_eq!(
            wire.kind, "composition",
            "the kind comes from the relationship"
        );
        assert_eq!(
            markers(&wire.kind),
            (Some(Marker::FilledDiamond), None),
            "a composition carries a diamond at its source and no arrowhead"
        );
    }

    #[test]
    fn a_degenerate_connector_stays_at_the_centre() {
        let rect = (10.0, 10.0, 20.0, 20.0);
        assert_eq!(clip(rect, centre(rect)), (20.0, 20.0));
    }

    #[test]
    fn numbers_are_trimmed_and_never_negative_zero() {
        assert_eq!(ids::number(0.0), "0");
        assert_eq!(ids::number(-0.0), "0");
        assert_eq!(ids::number(12.5), "12.5");
        assert_eq!(ids::number(1122.52), "1122.52");
        assert_eq!(ids::number(793.699_999), "793.7");
        assert_eq!(ids::number(f64::NAN), "0");
    }

    #[test]
    fn a_long_word_is_broken_rather_than_overflowing() {
        // Short enough that the hard break fits inside MAX_NAME_LINES, so nothing is
        // ellipsised and the concatenation must equal the input exactly.
        let long = "interoperability";
        let lines = wrap(long, 60.0, 12.0);
        assert!(lines.len() > 1, "the word must be split: {lines:?}");
        assert!(
            lines.len() <= MAX_NAME_LINES,
            "no ellipsis expected: {lines:?}"
        );
        for line in &lines {
            assert!(
                font::advance(line, 12.0) <= 60.0 + 0.01,
                "{line:?} overflows"
            );
        }
        assert_eq!(lines.concat(), long, "no character is lost");
    }

    #[test]
    fn a_three_line_name_is_ellipsised() {
        let text = "one two three four five six seven eight nine ten eleven twelve";
        let lines = wrap(text, 70.0, 12.0);
        assert_eq!(lines.len(), MAX_NAME_LINES);
        assert!(lines.last().is_some_and(|line| line.ends_with('…')));
    }

    #[test]
    fn painting_order_is_position_then_id() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let mut shapes = [
            Shape {
                element_id: None,
                object_id: b,
                label: "b".into(),
                kind: "note".into(),
                layer: None,
                form: Form::Box,
                position: 1.0,
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 10.0,
            },
            Shape {
                element_id: None,
                object_id: a,
                label: "a".into(),
                kind: "note".into(),
                layer: None,
                form: Form::Box,
                position: 0.0,
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 10.0,
            },
        ];
        shapes.sort_by(|left, right| {
            left.position
                .total_cmp(&right.position)
                .then_with(|| left.object_id.cmp(&right.object_id))
        });
        assert_eq!(shapes[0].object_id, a, "the earlier position paints first");
    }

    #[test]
    fn a_synthetic_view_places_every_element_and_wires_what_it_can() {
        let element = |id: Uuid, kind: &str| conveyor_domain::model::SnapshotElement {
            id,
            kind: kind.to_owned(),
            name: format!("{kind} {id}"),
            documentation: String::new(),
            parent_element_id: None,
            meta: json!({}),
        };
        let a = Uuid::from_u128(11);
        let b = Uuid::from_u128(12);
        let model = ModelSnapshot {
            model: SnapshotModel {
                id: Uuid::from_u128(10),
                language: "archimate".to_owned(),
                language_version: "3.2".to_owned(),
                title: "T".to_owned(),
                description: String::new(),
                meta: json!({}),
            },
            elements: vec![element(a, "application_component"), element(b, "node")],
            relationships: vec![conveyor_domain::model::SnapshotRelationship {
                id: Uuid::from_u128(13),
                kind: "serving".to_owned(),
                source_id: a,
                target_id: b,
                meta: json!({}),
            }],
            views: Vec::new(),
        };
        let profile = for_language("archimate", "3.2").expect("profile");
        let (shapes, wires) = synthetic_plan(&model, profile);
        assert_eq!(shapes.len(), 2);
        assert_eq!(wires.len(), 1);
        assert_eq!(wires[0].points.len(), 2);
        assert!(wires[0].synthetic, "a synthetic wire is named r-<uuid>");
    }

    #[test]
    fn an_object_without_bounds_gets_the_default_box() {
        let plain = object(Uuid::from_u128(1), 0.0, json!({}));
        assert_eq!(bounds_of(&plain), (0.0, 0.0, DEFAULT_WIDTH, DEFAULT_HEIGHT));
        let tiny = object(Uuid::from_u128(2), 0.0, json!({"w": 0.0, "h": -4.0}));
        let (_, _, w, h) = bounds_of(&tiny);
        assert!(w >= 1.0 && h >= 1.0, "a zero-size box cannot be rendered");
    }
}
