//! Programme — the programme's row-level plan view (SPEC_V2 §10.5, destination **Programme**).
//!
//! Five reads and no write: `programmes.list` fills the picker, and the four programme-keyed
//! reads — `tranches.list` (the plan view, retired rows hidden), `cadences.list`, `benefits.list`
//! and `lessons.list` — are read for the selected programme and refetched when it changes or when
//! Refresh is pressed.
//!
//! This screen is read-only. SPEC_V2 §10.5's mandate, brief, blueprint and business case, the
//! benefits map, the projects dossier, stakeholder profiles, end-of-tranche reviews, and
//! transition and realised benefits are governed actions (and, where they earn it, destinations
//! of their own) — not controls this read invents. Tranche authorisation and adoption are actions,
//! not this read.

use leptos::prelude::*;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{ApiError, InvokeOptions, Page};
use crate::components::{Empty, ErrorPanel, Planned, StatusPill, Tile};
use crate::state::AppCtx;

/// One programme as `programmes.list` returns it.
#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
struct ProgrammeRow {
    id: String,
    name: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    version: i64,
}

/// One tranche as `tranches.list` returns it; only the plan-view fields are kept.
#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
struct Tranche {
    id: String,
    programme_id: String,
    number: i32,
    state: String,
    #[serde(default)]
    review_at: Option<String>,
    #[serde(default)]
    authorised_at: Option<String>,
}

/// One cadence row as `cadences.list` returns it.
#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
struct Cadence {
    id: String,
    kind: String,
    period: String,
    state: String,
}

/// One benefit profile as `benefits.list` returns it.
#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
struct Benefit {
    id: String,
    reference: String,
    description: String,
    category: String,
    #[serde(default)]
    baseline: Option<Value>,
    #[serde(default)]
    target: Option<Value>,
    #[serde(default)]
    measurement_method: String,
}

/// One lesson as `lessons.list` returns it.
#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
struct Lesson {
    id: String,
    finding_key: String,
    statement: String,
    state: String,
    #[serde(default)]
    independent_count: i32,
}

fn parse<T: serde::de::DeserializeOwned>(value: Value, what: &str) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Network(format!("{what}: {error}")))
}

fn load_programmes(ctx: AppCtx) -> LocalResource<Result<Vec<ProgrammeRow>, ApiError>> {
    LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        async move {
            let page: Page<ProgrammeRow> = parse(
                ctx.invoke(
                    "programmes.list",
                    json!({"limit": 50}),
                    InvokeOptions::default(),
                )
                .await?
                .output,
                "programmes",
            )?;
            Ok(page.items)
        }
    })
}

fn load_tranches(
    ctx: AppCtx,
    programme: RwSignal<Option<String>>,
) -> LocalResource<Result<Vec<Tranche>, ApiError>> {
    LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let programme = programme.get();
        async move {
            let Some(programme_id) = programme else {
                return Ok(Vec::new());
            };
            let page: Page<Tranche> = parse(
                ctx.invoke(
                    "tranches.list",
                    json!({"programme_id": programme_id, "include_retired": false}),
                    InvokeOptions::default(),
                )
                .await?
                .output,
                "tranches",
            )?;
            Ok(page.items)
        }
    })
}

fn load_cadences(
    ctx: AppCtx,
    programme: RwSignal<Option<String>>,
) -> LocalResource<Result<Vec<Cadence>, ApiError>> {
    LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let programme = programme.get();
        async move {
            let Some(programme_id) = programme else {
                return Ok(Vec::new());
            };
            let page: Page<Cadence> = parse(
                ctx.invoke(
                    "cadences.list",
                    json!({"programme_id": programme_id}),
                    InvokeOptions::default(),
                )
                .await?
                .output,
                "cadences",
            )?;
            Ok(page.items)
        }
    })
}

fn load_benefits(
    ctx: AppCtx,
    programme: RwSignal<Option<String>>,
) -> LocalResource<Result<Vec<Benefit>, ApiError>> {
    LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let programme = programme.get();
        async move {
            let Some(programme_id) = programme else {
                return Ok(Vec::new());
            };
            let page: Page<Benefit> = parse(
                ctx.invoke(
                    "benefits.list",
                    json!({"programme_id": programme_id}),
                    InvokeOptions::default(),
                )
                .await?
                .output,
                "benefits",
            )?;
            Ok(page.items)
        }
    })
}

fn load_lessons(
    ctx: AppCtx,
    programme: RwSignal<Option<String>>,
) -> LocalResource<Result<Vec<Lesson>, ApiError>> {
    LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let programme = programme.get();
        async move {
            let Some(programme_id) = programme else {
                return Ok(Vec::new());
            };
            let page: Page<Lesson> = parse(
                ctx.invoke(
                    "lessons.list",
                    json!({"programme_id": programme_id}),
                    InvokeOptions::default(),
                )
                .await?
                .output,
                "lessons",
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

/// The calendar date of an RFC 3339 stamp, or `—`.
fn day(value: Option<&str>) -> String {
    value
        .and_then(|value| value.split('T').next())
        .filter(|value| !value.is_empty())
        .unwrap_or("—")
        .to_owned()
}

/// A JSON baseline, summarised: its string value when it is one, `set` for any structure, `—`
/// when absent. The full document is not this screen's to draw.
fn brief(value: &Option<Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(_) => "set".to_owned(),
        None => "—".to_owned(),
    }
}

#[component]
pub fn Programme() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let programmes = load_programmes(ctx);
    let selected = RwSignal::new(Option::<String>::None);
    let tranches = load_tranches(ctx, selected);
    let cadences = load_cadences(ctx, selected);
    let benefits = load_benefits(ctx, selected);
    let lessons = load_lessons(ctx, selected);

    // Default the picker to the first programme the real read returns.
    Effect::new(move |_| {
        if selected.get_untracked().is_none()
            && let Some(Ok(items)) = programmes.get()
            && let Some(first) = items.first()
        {
            selected.set(Some(first.id.clone()));
        }
    });

    view! {
        <section class="screen">
            {move || {
                let missing = ["programmes.list", "tranches.list", "benefits.list"]
                    .into_iter()
                    .filter(|action| !ctx.has_action(action))
                    .collect::<Vec<_>>()
                    .join(" / ");
                if !missing.is_empty() {
                    return view! {
                        <Planned
                            title="Programme is planned".to_owned()
                            detail=format!("{missing} not in this role's manifest — the screen appears when the actions land, not before.")
                        />
                    }
                    .into_any();
                }
                view! {
                    <header class="screen-head">
                        <div>
                            <h1>"Programme"</h1>
                            <p class="muted">
                                "Tranches, cadences, benefits and lessons are rows; adoption and approval are governed actions, not this read."
                            </p>
                        </div>
                        <div class="filterbar">
                            <label class="filter">
                                <span>"Programme"</span>
                                <select
                                    aria-label="Programme"
                                    on:change=move |event| {
                                        let value = event_target_value(&event);
                                        selected.set((!value.is_empty()).then_some(value));
                                    }
                                >
                                    {move || {
                                        let items = match programmes.get() { Some(Ok(items)) => items, _ => Vec::new() };
                                        let options = items.into_iter().map(|row| {
                                            let value = row.id.clone();
                                            let selected_value = value.clone();
                                            view! {
                                                <option value=value selected=move || selected.get().as_deref() == Some(selected_value.as_str())>
                                                    {format!("{} · {}", row.name, row.status)}
                                                </option>
                                            }
                                        }).collect_view();
                                        view! { {options} }.into_any()
                                    }}
                                </select>
                            </label>
                            <button class="btn" on:click=move |_| {
                                tranches.refetch();
                                cadences.refetch();
                                benefits.refetch();
                                lessons.refetch();
                            }>"Refresh"</button>
                        </div>
                    </header>
                    {move || match programmes.get() {
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=Callback::new(move |_| programmes.refetch())/>
                        }.into_any(),
                        Some(Ok(items)) if items.is_empty() => view! {
                            <Empty message="No programme in this tenant — a programme is created by `programmes.create`, not here.".to_owned()/>
                        }.into_any(),
                        _ => view! { <div></div> }.into_any(),
                    }}
                    {move || {
                        let tranche_count = tile_count(tranches.get());
                        let cadence_count = tile_count(cadences.get());
                        let benefit_count = tile_count(benefits.get());
                        let lesson_count = tile_count(lessons.get());
                        view! {
                            <div class="tiles">
                                <Tile label="Tranches".to_owned() value=tranche_count detail="in the plan view (retired hidden)".to_owned()/>
                                <Tile label="Cadences".to_owned() value=cadence_count detail="PI, sprint, demo, retro and refinement rows".to_owned()/>
                                <Tile label="Benefits".to_owned() value=benefit_count detail="live benefit profiles".to_owned()/>
                                <Tile label="Lessons".to_owned() value=lesson_count detail="recorded lessons".to_owned()/>
                            </div>
                        }
                    }}
                    <h2>"Tranches"</h2>
                    {move || match tranches.get() {
                        None => view! { <p class="muted">"Reading tranches…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=Callback::new(move |_| tranches.refetch())/>
                        }.into_any(),
                        Some(Ok(items)) if items.is_empty() => {
                            let message = if selected.get().is_some() {
                                "No tranche in this programme's plan view — retired tranches are hidden."
                            } else {
                                "No programme selected — choose one above to read its tranches."
                            };
                            view! { <Empty message=message.to_owned()/> }.into_any()
                        }
                        Some(Ok(items)) => view! {
                            <div class="tablewrap">
                                <table class="table">
                                    <thead><tr>
                                        <th class="n">"Number"</th><th>"State"</th><th>"Review at"</th><th>"Authorised"</th>
                                    </tr></thead>
                                    <tbody>
                                        {items.iter().map(|tranche| view! {
                                            <tr>
                                                <td class="n">{tranche.number.to_string()}</td>
                                                <td><StatusPill status=tranche.state.clone()/></td>
                                                <td>{day(tranche.review_at.as_deref())}</td>
                                                <td>{if tranche.authorised_at.is_some() { "yes" } else { "—" }}</td>
                                            </tr>
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        }.into_any(),
                    }}
                    <h2>"Cadences"</h2>
                    {move || match cadences.get() {
                        None => view! { <p class="muted">"Reading cadences…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=Callback::new(move |_| cadences.refetch())/>
                        }.into_any(),
                        Some(Ok(items)) if items.is_empty() => {
                            let message = if selected.get().is_some() {
                                "No cadence row for this programme — PI, demo, retro and sprint rows are created by `cadences.create`."
                            } else {
                                "No programme selected — choose one above to read its cadences."
                            };
                            view! { <Empty message=message.to_owned()/> }.into_any()
                        }
                        Some(Ok(items)) => view! {
                            <div class="tablewrap">
                                <table class="table">
                                    <thead><tr>
                                        <th>"Kind"</th><th>"Period"</th><th>"State"</th>
                                    </tr></thead>
                                    <tbody>
                                        {items.iter().map(|cadence| view! {
                                            <tr>
                                                <td><StatusPill status=cadence.kind.clone()/></td>
                                                <td>{cadence.period.clone()}</td>
                                                <td><StatusPill status=cadence.state.clone()/></td>
                                            </tr>
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        }.into_any(),
                    }}
                    <h2>"Benefits"</h2>
                    {move || match benefits.get() {
                        None => view! { <p class="muted">"Reading benefits…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=Callback::new(move |_| benefits.refetch())/>
                        }.into_any(),
                        Some(Ok(items)) if items.is_empty() => {
                            let message = if selected.get().is_some() {
                                "No live benefit profile for this programme — retired profiles are hidden; a profile is created by `benefits.create`."
                            } else {
                                "No programme selected — choose one above to read its benefits."
                            };
                            view! { <Empty message=message.to_owned()/> }.into_any()
                        }
                        Some(Ok(items)) => view! {
                            <div class="tablewrap">
                                <table class="table">
                                    <thead><tr>
                                        <th>"Reference"</th><th>"Description"</th><th>"Category"</th>
                                        <th>"Baseline"</th><th>"Method"</th>
                                    </tr></thead>
                                    <tbody>
                                        {items.iter().map(|benefit| view! {
                                            <tr>
                                                <td class="mono">{benefit.reference.clone()}</td>
                                                <td>{benefit.description.clone()}</td>
                                                <td>{benefit.category.clone()}</td>
                                                <td>{brief(&benefit.baseline)}</td>
                                                <td>{benefit.measurement_method.clone()}</td>
                                            </tr>
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        }.into_any(),
                    }}
                    <h2>"Lessons"</h2>
                    {move || match lessons.get() {
                        None => view! { <p class="muted">"Reading lessons…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=Callback::new(move |_| lessons.refetch())/>
                        }.into_any(),
                        Some(Ok(items)) if items.is_empty() => {
                            let message = if selected.get().is_some() {
                                "No lesson recorded for this programme — lessons come from a retro cadence via `lessons.propose`."
                            } else {
                                "No programme selected — choose one above to read its lessons."
                            };
                            view! { <Empty message=message.to_owned()/> }.into_any()
                        }
                        Some(Ok(items)) => view! {
                            <div class="tablewrap">
                                <table class="table">
                                    <thead><tr>
                                        <th>"Finding"</th><th>"Statement"</th><th>"State"</th>
                                        <th class="n">"Independent refs"</th>
                                    </tr></thead>
                                    <tbody>
                                        {items.iter().map(|lesson| view! {
                                            <tr>
                                                <td class="mono">{lesson.finding_key.clone()}</td>
                                                <td>{lesson.statement.clone()}</td>
                                                <td><StatusPill status=lesson.state.clone()/></td>
                                                <td class="n">{lesson.independent_count.to_string()}</td>
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
