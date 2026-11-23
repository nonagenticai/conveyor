//! Plan — the project's plan spine (SPEC_V2 §10.5, destination **Plan**).
//!
//! Five reads and no write: `projects.list` fills the picker, and the four project-keyed
//! reads — `wbs.list` (the work-breakdown tree), `baselines.list` (scope, schedule and cost
//! snapshots), `registers.list` (risks, issues, assumptions and changes) and
//! `project_dependencies.list` (the typed edges to other projects) — are read for the
//! selected project and refetched when it changes or when Refresh is pressed.
//!
//! This screen is read-only. A WBS node's acceptance is evidence attached to the row, not a
//! checkbox this read can tick; node authoring, baseline-taking, register edits and dependency
//! edges are governed actions, not controls invented here.

use leptos::prelude::*;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{ApiError, InvokeOptions, Page};
use crate::components::{Empty, ErrorPanel, Planned, StatusPill, Tile};
use crate::state::AppCtx;

/// One project as `projects.list` returns it.
#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
struct Project {
    id: String,
    key: String,
    name: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    programme_id: Option<String>,
}

/// One WBS node as `wbs.list` returns it; only the plan-view fields are kept.
#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
struct WbsNode {
    id: String,
    project_id: String,
    #[serde(default)]
    parent_id: Option<String>,
    #[serde(default)]
    position: f64,
    kind: String,
    title: String,
    #[serde(default)]
    body: String,
    status: String,
    #[serde(default)]
    acceptance: Value,
}

/// One baseline as `baselines.list` returns it.
#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
struct Baseline {
    id: String,
    kind: String,
    #[serde(default)]
    taken_at: String,
    #[serde(default)]
    approved_by: Option<String>,
    #[serde(default)]
    version: i64,
}

/// One register row as `registers.list` returns it.
#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
struct Register {
    id: String,
    kind: String,
    title: String,
    status: String,
    #[serde(default)]
    severity: Option<String>,
}

/// One dependency edge as `project_dependencies.list` returns it.
#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
struct Dependency {
    id: String,
    from_project: String,
    to_project: String,
    kind: String,
    status: String,
}

fn parse<T: serde::de::DeserializeOwned>(value: Value, what: &str) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Network(format!("{what}: {error}")))
}

fn load_projects(ctx: AppCtx) -> LocalResource<Result<Vec<Project>, ApiError>> {
    LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        async move {
            let page: Page<Project> = parse(
                ctx.invoke(
                    "projects.list",
                    json!({"limit": 50}),
                    InvokeOptions::default(),
                )
                .await?
                .output,
                "projects",
            )?;
            Ok(page.items)
        }
    })
}

fn load_wbs(
    ctx: AppCtx,
    project: RwSignal<Option<String>>,
) -> LocalResource<Result<Vec<WbsNode>, ApiError>> {
    LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let project = project.get();
        async move {
            let Some(project_id) = project else {
                return Ok(Vec::new());
            };
            let page: Page<WbsNode> = parse(
                ctx.invoke(
                    "wbs.list",
                    json!({"project_id": project_id}),
                    InvokeOptions::default(),
                )
                .await?
                .output,
                "WBS",
            )?;
            Ok(page.items)
        }
    })
}

fn load_baselines(
    ctx: AppCtx,
    project: RwSignal<Option<String>>,
) -> LocalResource<Result<Vec<Baseline>, ApiError>> {
    LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let project = project.get();
        async move {
            let Some(project_id) = project else {
                return Ok(Vec::new());
            };
            let page: Page<Baseline> = parse(
                ctx.invoke(
                    "baselines.list",
                    json!({"project_id": project_id}),
                    InvokeOptions::default(),
                )
                .await?
                .output,
                "baselines",
            )?;
            Ok(page.items)
        }
    })
}

fn load_registers(
    ctx: AppCtx,
    project: RwSignal<Option<String>>,
) -> LocalResource<Result<Vec<Register>, ApiError>> {
    LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let project = project.get();
        async move {
            let Some(project_id) = project else {
                return Ok(Vec::new());
            };
            let page: Page<Register> = parse(
                ctx.invoke(
                    "registers.list",
                    json!({"project_id": project_id}),
                    InvokeOptions::default(),
                )
                .await?
                .output,
                "registers",
            )?;
            Ok(page.items)
        }
    })
}

fn load_dependencies(
    ctx: AppCtx,
    project: RwSignal<Option<String>>,
) -> LocalResource<Result<Vec<Dependency>, ApiError>> {
    LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let project = project.get();
        async move {
            let Some(project_id) = project else {
                return Ok(Vec::new());
            };
            let page: Page<Dependency> = parse(
                ctx.invoke(
                    "project_dependencies.list",
                    json!({"project_id": project_id}),
                    InvokeOptions::default(),
                )
                .await?
                .output,
                "dependencies",
            )?;
            Ok(page.items)
        }
    })
}

/// A tile's count: the read's length, or `—` until it has answered (and when it failed — the
/// panel below says why).
fn tile_count<T>(items: Option<Result<Vec<T>, ApiError>>) -> String {
    match items {
        Some(Ok(items)) => items.len().to_string(),
        _ => "—".to_owned(),
    }
}

/// The WBS rows whose status is `accepted`, or `—` until the read has answered.
fn accepted_count(items: Option<Result<Vec<WbsNode>, ApiError>>) -> String {
    match items {
        Some(Ok(items)) => items
            .iter()
            .filter(|node| node.status == "accepted")
            .count()
            .to_string(),
        _ => "—".to_owned(),
    }
}

/// The calendar date of an RFC 3339 stamp, or `—`.
fn day(value: &str) -> String {
    let date = value.split('T').next().unwrap_or(value);
    if date.is_empty() {
        "—".to_owned()
    } else {
        date.to_owned()
    }
}

/// A short identifier: eight characters, or `—` when absent.
fn short(value: &str) -> String {
    if value.is_empty() {
        "—".to_owned()
    } else {
        value.chars().take(8).collect()
    }
}

/// Whether the JSON acceptance document is set: `set` for any value, `—` for `null`.
fn acceptance(value: &Value) -> String {
    if value.is_null() {
        "—".to_owned()
    } else {
        "set".to_owned()
    }
}

/// An optional text field, or `—` when it is absent or blank.
fn or_dash(value: &Option<String>) -> String {
    value
        .as_deref()
        .filter(|text| !text.trim().is_empty())
        .unwrap_or("—")
        .to_owned()
}

#[component]
pub fn Plan() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let projects = load_projects(ctx);
    let selected = RwSignal::new(Option::<String>::None);
    let wbs = load_wbs(ctx, selected);
    let baselines = load_baselines(ctx, selected);
    let registers = load_registers(ctx, selected);
    let dependencies = load_dependencies(ctx, selected);

    // Default the picker to the first project the real read returns.
    Effect::new(move |_| {
        if selected.get_untracked().is_none()
            && let Some(Ok(items)) = projects.get()
            && let Some(first) = items.first()
        {
            selected.set(Some(first.id.clone()));
        }
    });

    view! {
        <section class="screen">
            {move || {
                let missing = [
                    "projects.list",
                    "wbs.list",
                    "baselines.list",
                    "registers.list",
                    "project_dependencies.list",
                ]
                    .into_iter()
                    .filter(|action| !ctx.has_action(action))
                    .collect::<Vec<_>>()
                    .join(" / ");
                if !missing.is_empty() {
                    return view! {
                        <Planned
                            title="Plan is planned".to_owned()
                            detail=format!("{missing} not in this role's manifest — the screen appears when the actions land, not before.")
                        />
                    }
                    .into_any();
                }
                view! {
                    <header class="screen-head">
                        <div>
                            <h1>"Plan"</h1>
                            <p class="muted">
                                "The project's WBS, baselines, registers and typed dependency edges are rows; acceptance is evidence, not a checkbox."
                            </p>
                        </div>
                        <div class="filterbar">
                            <label class="filter">
                                <span>"Project"</span>
                                <select
                                    aria-label="Project"
                                    on:change=move |event| {
                                        let value = event_target_value(&event);
                                        selected.set((!value.is_empty()).then_some(value));
                                    }
                                >
                                    {move || {
                                        let items = match projects.get() { Some(Ok(items)) => items, _ => Vec::new() };
                                        let options = items.into_iter().map(|row| {
                                            let value = row.id.clone();
                                            let selected_value = value.clone();
                                            view! {
                                                <option value=value selected=move || selected.get().as_deref() == Some(selected_value.as_str())>
                                                    {format!("{} · {}", row.key, row.name)}
                                                </option>
                                            }
                                        }).collect_view();
                                        view! { {options} }.into_any()
                                    }}
                                </select>
                            </label>
                            <button class="btn" on:click=move |_| {
                                wbs.refetch();
                                baselines.refetch();
                                registers.refetch();
                                dependencies.refetch();
                            }>"Refresh"</button>
                        </div>
                    </header>
                    {move || match projects.get() {
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=Callback::new(move |_| projects.refetch())/>
                        }.into_any(),
                        Some(Ok(items)) if items.is_empty() => view! {
                            <Empty message="No project in this tenant — a project is created by `projects.create`, not here.".to_owned()/>
                        }.into_any(),
                        _ => view! { <div></div> }.into_any(),
                    }}
                    {move || {
                        let wbs_value = tile_count(wbs.get());
                        let accepted_value = accepted_count(wbs.get());
                        let baseline_value = tile_count(baselines.get());
                        let register_value = tile_count(registers.get());
                        view! {
                            <div class="tiles">
                                <Tile label="WBS nodes".to_owned() value=wbs_value detail="nodes in this project's tree".to_owned()/>
                                <Tile label="Accepted nodes".to_owned() value=accepted_value detail="nodes whose status is accepted".to_owned()/>
                                <Tile label="Baselines".to_owned() value=baseline_value detail="scope, schedule and cost snapshots".to_owned()/>
                                <Tile label="Register rows".to_owned() value=register_value detail="risks, issues, assumptions and changes".to_owned()/>
                            </div>
                        }
                    }}
                    <h2>"WBS"</h2>
                    {move || match wbs.get() {
                        None => view! { <p class="muted">"Reading WBS…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=Callback::new(move |_| wbs.refetch())/>
                        }.into_any(),
                        Some(Ok(items)) if items.is_empty() => {
                            let message = if selected.get().is_some() {
                                "No WBS node for this project — nodes are authored with `wbs.create`."
                            } else {
                                "No project selected — choose one above to read its WBS."
                            };
                            view! { <Empty message=message.to_owned()/> }.into_any()
                        }
                        Some(Ok(items)) => view! {
                            <div class="tablewrap">
                                <table class="table">
                                    <thead><tr>
                                        <th>"Kind"</th><th>"Title"</th><th>"Status"</th><th>"Acceptance"</th>
                                    </tr></thead>
                                    <tbody>
                                        {items.iter().map(|node| view! {
                                            <tr>
                                                <td><StatusPill status=node.kind.clone()/></td>
                                                <td>{node.title.clone()}</td>
                                                <td><StatusPill status=node.status.clone()/></td>
                                                <td>{acceptance(&node.acceptance)}</td>
                                            </tr>
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        }.into_any(),
                    }}
                    <h2>"Baselines"</h2>
                    {move || match baselines.get() {
                        None => view! { <p class="muted">"Reading baselines…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=Callback::new(move |_| baselines.refetch())/>
                        }.into_any(),
                        Some(Ok(items)) if items.is_empty() => {
                            let message = if selected.get().is_some() {
                                "No baseline for this project — a baseline is taken by `baselines.take`."
                            } else {
                                "No project selected — choose one above to read its baselines."
                            };
                            view! { <Empty message=message.to_owned()/> }.into_any()
                        }
                        Some(Ok(items)) => view! {
                            <div class="tablewrap">
                                <table class="table">
                                    <thead><tr>
                                        <th>"Kind"</th><th>"Taken at"</th><th>"Approved"</th>
                                    </tr></thead>
                                    <tbody>
                                        {items.iter().map(|baseline| view! {
                                            <tr>
                                                <td><StatusPill status=baseline.kind.clone()/></td>
                                                <td>{day(&baseline.taken_at)}</td>
                                                <td>{if baseline.approved_by.as_deref().is_some_and(|id| !id.is_empty()) { "yes" } else { "—" }}</td>
                                            </tr>
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        }.into_any(),
                    }}
                    <h2>"Registers"</h2>
                    {move || match registers.get() {
                        None => view! { <p class="muted">"Reading registers…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=Callback::new(move |_| registers.refetch())/>
                        }.into_any(),
                        Some(Ok(items)) if items.is_empty() => {
                            let message = if selected.get().is_some() {
                                "No register row for this project — rows are created by `registers.create`."
                            } else {
                                "No project selected — choose one above to read its registers."
                            };
                            view! { <Empty message=message.to_owned()/> }.into_any()
                        }
                        Some(Ok(items)) => view! {
                            <div class="tablewrap">
                                <table class="table">
                                    <thead><tr>
                                        <th>"Kind"</th><th>"Title"</th><th>"Status"</th><th>"Severity"</th>
                                    </tr></thead>
                                    <tbody>
                                        {items.iter().map(|register| view! {
                                            <tr>
                                                <td><StatusPill status=register.kind.clone()/></td>
                                                <td>{register.title.clone()}</td>
                                                <td><StatusPill status=register.status.clone()/></td>
                                                <td>{or_dash(&register.severity)}</td>
                                            </tr>
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        }.into_any(),
                    }}
                    <h2>"Dependencies"</h2>
                    {move || match dependencies.get() {
                        None => view! { <p class="muted">"Reading dependencies…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=Callback::new(move |_| dependencies.refetch())/>
                        }.into_any(),
                        Some(Ok(items)) if items.is_empty() => {
                            let message = if selected.get().is_some() {
                                "No dependency edge for this project — edges are created by `project_dependencies.create`."
                            } else {
                                "No project selected — choose one above to read its dependencies."
                            };
                            view! { <Empty message=message.to_owned()/> }.into_any()
                        }
                        Some(Ok(items)) => view! {
                            <div class="tablewrap">
                                <table class="table">
                                    <thead><tr>
                                        <th>"Kind"</th><th>"Target"</th><th>"Status"</th>
                                    </tr></thead>
                                    <tbody>
                                        {items.iter().map(|edge| view! {
                                            <tr>
                                                <td><StatusPill status=edge.kind.clone()/></td>
                                                <td class="mono">{short(&edge.to_project)}</td>
                                                <td><StatusPill status=edge.status.clone()/></td>
                                            </tr>
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
