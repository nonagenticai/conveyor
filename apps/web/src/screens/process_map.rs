//! Process map — the user-facing DAG (S12 screen 22, SPEC D30).
//!
//! Two reads and nothing else: `process_maps.list` and `process_maps.get`. The map is a
//! **versioned document** `{schema, nodes, edges}` bound to a project type. It is declarative —
//! it never reorders a step and never starts a run — so this screen draws it and offers no
//! control that would imply execution.
//!
//! ⚠️ **The live position is not drawn, and the screen says why.** The amended slice wants each
//! node's running/done/blocked state "derived from the runs", but no action joins a project's
//! runs to map nodes (`runs.list` filters by status and card only, and `RunOut` carries no
//! process key). Rendering a guessed position would be the one thing this screen must not do, so
//! the panel names the missing read instead. When S13 exposes it, the overlay belongs here.
//!
//! ⚠️ **Read-only.** `process_maps.*` registers `get` and `list` only; there is no create or
//! update action, so the "author" half of the row is not buildable from the registry yet. The
//! screen does not fake an editor.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use serde::Deserialize;
use serde_json::json;

use crate::api::{ApiError, InvokeOptions, Page};
use crate::components::{Empty, ErrorPanel, Planned, StatusPill, age, short_id};
use crate::state::AppCtx;

/// One map row, exactly as `MapOut` writes it.
#[derive(Clone, Debug, Deserialize)]
struct MapOut {
    id: String,
    key: String,
    title: String,
    status: String,
    version: i64,
    project_type_id: String,
    #[serde(default)]
    definition: serde_json::Value,
    #[serde(default)]
    updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// The document's shape (`conveyor_domain::process_map::MapDoc`), read here as data.
#[derive(Clone, Debug, Default, Deserialize)]
struct MapDoc {
    #[serde(default)]
    schema: String,
    #[serde(default)]
    nodes: Vec<MapNode>,
    #[serde(default)]
    edges: Vec<MapEdge>,
}

#[derive(Clone, Debug, Deserialize)]
struct MapNode {
    id: String,
    #[serde(default)]
    kind: String,
    #[serde(rename = "ref", default)]
    reference: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    position: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize)]
struct MapEdge {
    from: String,
    to: String,
    #[serde(default)]
    label: String,
}

fn parse<T: serde::de::DeserializeOwned>(
    value: serde_json::Value,
    what: &str,
) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Network(format!("{what}: {error}")))
}

fn load_maps(ctx: AppCtx) -> LocalResource<Result<Vec<MapOut>, ApiError>> {
    LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        async move {
            let page: Page<MapOut> = parse(
                ctx.invoke(
                    "process_maps.list",
                    json!({"limit": 50}),
                    InvokeOptions::default(),
                )
                .await?
                .output,
                "process maps",
            )?;
            Ok(page.items)
        }
    })
}

/// The map list: one row per versioned document.
#[component]
pub fn ProcessMapList() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let data = load_maps(ctx);
    let retry = Callback::new(move |_| data.refetch());

    view! {
        <section class="screen">
            {move || {
                if !ctx.has_action("process_maps.list") {
                    return view! {
                        <Planned
                            title="Process map is planned".to_owned()
                            detail="process_maps.list is not in this role's manifest. The map appears when the action lands, not before.".to_owned()
                        />
                    }
                    .into_any();
                }
                view! {
                    <header class="screen-head">
                        <div>
                            <h1>"Process map"</h1>
                            <p class="muted">
                                "The user-facing DAG, one versioned document per project type. It is declarative: it shows how the phases, capabilities and processes relate, and it never executes a step."
                            </p>
                        </div>
                    </header>
                    <p class="hint">"process_maps.list — one row per map version"</p>
                    {move || match data.get() {
                        None => view! { <p class="muted">"Loading the maps…"</p> }.into_any(),
                        Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                        Some(Ok(maps)) if maps.is_empty() => view! {
                            <Empty message="No process map in this tenant. The shipped catalogue installs one per project type; a tenant with no catalogue yet shows this state.".to_owned()/>
                        }.into_any(),
                        Some(Ok(maps)) => view! {
                            <div class="panel">
                                <table class="table">
                                    <thead>
                                        <tr>
                                            <th>"Map"</th>
                                            <th>"Key"</th>
                                            <th>"Project type"</th>
                                            <th>"Version"</th>
                                            <th>"Status"</th>
                                            <th>"Updated"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {maps.into_iter().map(|row| {
                                            let href = format!("/build/process-map/{}", row.id);
                                            view! {
                                                <tr>
                                                    <td><A href=href>{row.title.clone()}</A></td>
                                                    <td class="mono">{row.key.clone()}</td>
                                                    <td class="mono" title=row.project_type_id.clone()>{short_id(Some(&row.project_type_id))}</td>
                                                    <td class="mono">{row.version.to_string()}</td>
                                                    <td><StatusPill status=row.status.clone()/></td>
                                                    <td>{age(row.updated_at)}</td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        }.into_any(),
                    }}
                }
                .into_any()
            }}
        </section>
    }
}

const NODE_W: f64 = 176.0;
const NODE_H: f64 = 54.0;
const COL_GAP: f64 = 68.0;
const ROW_GAP: f64 = 26.0;
const MARGIN: f64 = 32.0;

/// A node with a place on the canvas.
#[derive(Clone)]
struct Placed {
    node: MapNode,
    x: f64,
    y: f64,
}

/// Place every node: explicit `{x, y}` when the document carries one, otherwise a layered
/// left-to-right layout over the edges (longest-path layers, stable within a layer).
fn place(doc: &MapDoc) -> Vec<Placed> {
    let explicit = doc.nodes.iter().all(|node| {
        node.position
            .as_ref()
            .and_then(|position| position.get("x"))
            .and_then(serde_json::Value::as_f64)
            .is_some()
            && node
                .position
                .as_ref()
                .and_then(|position| position.get("y"))
                .and_then(serde_json::Value::as_f64)
                .is_some()
    });
    if explicit {
        return doc
            .nodes
            .iter()
            .map(|node| {
                let position = node.position.clone().unwrap_or_default();
                Placed {
                    x: position
                        .get("x")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(MARGIN),
                    y: position
                        .get("y")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(MARGIN),
                    node: node.clone(),
                }
            })
            .collect();
    }
    // Longest-path layering. Relaxation is bounded by the node count, so a cycle cannot hang.
    let mut layer: Vec<i64> = vec![0; doc.nodes.len()];
    for _ in 0..doc.nodes.len() {
        let mut changed = false;
        for edge in &doc.edges {
            let Some(from) = doc.nodes.iter().position(|node| node.id == edge.from) else {
                continue;
            };
            let Some(to) = doc.nodes.iter().position(|node| node.id == edge.to) else {
                continue;
            };
            if layer[to] < layer[from] + 1 {
                layer[to] = layer[from] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut per_layer: std::collections::BTreeMap<i64, usize> = std::collections::BTreeMap::new();
    doc.nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let lane = layer[index];
            let row = per_layer.entry(lane).or_insert(0);
            let placed = Placed {
                node: node.clone(),
                x: MARGIN + lane as f64 * (NODE_W + COL_GAP),
                y: MARGIN + *row as f64 * (NODE_H + ROW_GAP),
            };
            *row += 1;
            placed
        })
        .collect()
}

/// The canvas size the placed nodes need.
fn canvas(placed: &[Placed]) -> (f64, f64) {
    let width = placed
        .iter()
        .map(|node| node.x + NODE_W)
        .fold(0.0_f64, f64::max)
        + MARGIN;
    let height = placed
        .iter()
        .map(|node| node.y + NODE_H)
        .fold(0.0_f64, f64::max)
        + MARGIN;
    (width.max(320.0), height.max(160.0))
}

fn kind_class(kind: &str) -> &'static str {
    match kind {
        "process" => "pm-process",
        "capability" => "pm-capability",
        "phase" => "pm-phase",
        "milestone" => "pm-milestone",
        _ => "pm-other",
    }
}

/// The drawn map. Pure view over placed nodes and the document's edges.
#[component]
fn MapCanvas(doc: MapDoc) -> impl IntoView {
    let placed = place(&doc);
    let (width, height) = canvas(&placed);

    let wires = {
        let placed = &placed;
        doc.edges
            .iter()
            .filter_map(|edge| {
                let from = placed.iter().find(|node| node.node.id == edge.from)?;
                let to = placed.iter().find(|node| node.node.id == edge.to)?;
                let (x1, y1) = if to.x >= from.x {
                    (from.x + NODE_W, from.y + NODE_H / 2.0)
                } else {
                    (from.x, from.y + NODE_H / 2.0)
                };
                let (x2, y2) = if to.x >= from.x {
                    (to.x, to.y + NODE_H / 2.0)
                } else {
                    (to.x + NODE_W, to.y + NODE_H / 2.0)
                };
                // A small arrowhead at the target end, so the direction is readable without a marker.
                let dx = x2 - x1;
                let dy = y2 - y1;
                let len = (dx * dx + dy * dy).sqrt().max(0.001);
                let (ux, uy) = (dx / len, dy / len);
                let (nx, ny) = (-uy, ux);
                let tip = (x2, y2);
                let back = (x2 - ux * 9.0, y2 - uy * 9.0);
                let left = (back.0 + nx * 4.5, back.1 + ny * 4.5);
                let right = (back.0 - nx * 4.5, back.1 - ny * 4.5);
                Some((x1, y1, back.0, back.1, tip, left, right, edge.label.clone()))
            })
            .collect::<Vec<_>>()
    };

    let nodes = placed
        .clone()
        .into_iter()
        .map(|node| {
            let class = kind_class(&node.node.kind).to_owned();
            let label = if node.node.label.trim().is_empty() {
                node.node.reference.clone()
            } else {
                node.node.label.clone()
            };
            let subtitle = if node.node.reference.is_empty() {
                node.node.kind.clone()
            } else {
                format!("{} · {}", node.node.kind, node.node.reference)
            };
            view! {
                <g>
                    <rect
                        class=format!("pm-node {class}")
                        x=node.x
                        y=node.y
                        width=NODE_W
                        height=NODE_H
                        rx="8"
                    />
                    <text class="pm-label" x=node.x + 12.0 y=node.y + 23.0>{label}</text>
                    <text class="pm-sub" x=node.x + 12.0 y=node.y + 41.0>{subtitle}</text>
                </g>
            }
        })
        .collect_view();

    let _ = &placed;
    view! {
        <svg
            class="process-map"
            role="img"
            aria-label="Process map"
            viewBox=format!("0 0 {width} {height}")
            preserveAspectRatio="xMidYMid meet"
        >
            {wires.into_iter().map(|(x1, y1, x2, y2, tip, left, right, label)| {
                let mid = ((x1 + x2) / 2.0, (y1 + y2) / 2.0 - 6.0);
                view! {
                    <g>
                        <line class="pm-wire" x1=x1 y1=y1 x2=x2 y2=y2/>
                        <polygon
                            class="pm-arrow"
                            points=format!("{},{} {},{} {},{}", tip.0, tip.1, left.0, left.1, right.0, right.1)
                        />
                        {(!label.is_empty()).then(|| view! {
                            <text class="pm-edge" x=mid.0 y=mid.1>{label}</text>
                        })}
                    </g>
                }
            }).collect_view()}
            {nodes}
        </svg>
    }
}

/// One map: the drawn document, its edges and the honesty notes.
#[component]
pub fn ProcessMapDetail() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let params = use_params_map();
    let id = move || params.read().get("map_id").unwrap_or_default();
    let data = {
        LocalResource::new(move || {
            let _tenant = ctx.tenant.get();
            let id = id();
            async move {
                if id.is_empty() {
                    return Err(ApiError::Network("no map id in the route".to_owned()));
                }
                parse::<MapOut>(
                    ctx.invoke(
                        "process_maps.get",
                        json!({"map_id": id}),
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                    "process map",
                )
            }
        })
    };
    let retry = Callback::new(move |_| data.refetch());

    view! {
        <section class="screen">
            <p class="hint"><A href="/build/process-map">"← All process maps"</A></p>
            {move || {
                if !ctx.has_action("process_maps.get") {
                    return view! {
                        <Planned
                            title="Process map is planned".to_owned()
                            detail="process_maps.get is not in this role's manifest.".to_owned()
                        />
                    }
                    .into_any();
                }
                match data.get() {
                    None => view! { <p class="muted">"Loading the map…"</p> }.into_any(),
                    Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                    Some(Ok(map)) => {
                        let doc: MapDoc =
                            parse::<MapDoc>(map.definition.clone(), "map document").unwrap_or_default();
                        let nodes = doc.nodes.len();
                        let edges = doc.edges.len();
                        view! {
                            <header class="screen-head">
                                <div>
                                    <h1>{map.title.clone()}</h1>
                                    <p class="muted">
                                        {format!("{} · project type {} · version {} · schema {}", map.key, short_id(Some(&map.project_type_id)), map.version, if doc.schema.is_empty() { "—" } else { &doc.schema })}
                                    </p>
                                </div>
                                <div class="screen-actions">
                                    <StatusPill status=map.status.clone()/>
                                </div>
                            </header>
                            <p class="hint">
                                {format!("process_maps.get — decoded {nodes} nodes and {edges} edges; the map is declarative and never starts a run")}
                            </p>
                            <div class="panel">
                                <MapCanvas doc=doc.clone()/>
                            </div>
                            <div class="panel" role="status">
                                <h2>"Live position"</h2>
                                <p class="muted">
                                    "Not drawn: no action joins a project's runs to this map's nodes yet. runs.list filters by status and card only, and a run carries no process key in its queue projection. The map is shown exactly as declared; an overlay here would be a guess, so it is named and left out."
                                </p>
                            </div>
                            <div class="panel">
                                <h2>"Nodes"</h2>
                                <table class="table">
                                    <thead>
                                        <tr><th>"Kind"</th><th>"Ref"</th><th>"Label"</th></tr>
                                    </thead>
                                    <tbody>
                                        {doc.nodes.iter().map(|node| view! {
                                            <tr>
                                                <td class="mono">{node.kind.clone()}</td>
                                                <td class="mono">{node.reference.clone()}</td>
                                                <td>{node.label.clone()}</td>
                                            </tr>
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                            <div class="panel">
                                <h2>"Edges"</h2>
                                {if doc.edges.is_empty() {
                                    view! { <Empty message="This map declares no edges.".to_owned()/> }.into_any()
                                } else {
                                    view! {
                                        <table class="table">
                                            <thead><tr><th>"From"</th><th>"To"</th><th>"Label"</th></tr></thead>
                                            <tbody>
                                                {doc.edges.iter().map(|edge| view! {
                                                    <tr>
                                                        <td class="mono">{edge.from.clone()}</td>
                                                        <td class="mono">{edge.to.clone()}</td>
                                                        <td>{edge.label.clone()}</td>
                                                    </tr>
                                                }).collect_view()}
                                            </tbody>
                                        </table>
                                    }.into_any()
                                }}
                            </div>
                        }.into_any()
                    }
                }
            }}
        </section>
    }
}
