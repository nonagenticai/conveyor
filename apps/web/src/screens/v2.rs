//! The V2 destinations: the shell's route targets for SPEC_V2 §10.5.
//!
//! T18-1 lands the route registry, the Portfolio and Programme reads, and the L1–L4 delivery
//! view. Every other V2 destination is a [`DestinationGate`]: an honest "not yet live" page
//! that names its owning slice. No fixture data is rendered — the reads are the app's existing
//! literal GETs (`/api/projects`, `/api/projects/{id}`, `/api/boards/{id}`,
//! `/api/boards/{id}/cards`) and the existing actions (`programmes.list`,
//! `inference_usage.report`).

use std::collections::HashMap;

use chrono::{Duration, Utc};
use leptos::prelude::*;
use leptos_router::components::A;
use serde_json::json;

use crate::api::{ApiError, InvokeOptions, Page};
use crate::components::{Empty, ErrorPanel, StatusPill, Tile, age, fmt_int};
use crate::models::{BoardView, CardOut, ProgrammeOut, ProjectOut, ProjectView, UsageReport};
use crate::state::AppCtx;

/// One project's 30-day inference spend, as the ledger grouped it.
#[derive(Clone, Debug, Default)]
struct Spend {
    cost_usd: String,
    priced: i64,
    unpriced: i64,
}

/// The Portfolio read: the tenant's projects, programmes and per-project spend.
#[derive(Clone, Debug, Default)]
struct PortfolioData {
    projects: Vec<ProjectOut>,
    programmes: Vec<ProgrammeOut>,
    spend: HashMap<String, Spend>,
}

/// The selected project's board and cards: L3 and L4 of the delivery view.
#[derive(Clone, Debug)]
struct DeliveryDetail {
    project: ProjectOut,
    board: Option<BoardView>,
    cards: Vec<CardOut>,
}

/// The Programme read: the programmes and the projects that roll up to them.
#[derive(Clone, Debug, Default)]
struct ProgrammeData {
    programmes: Vec<ProgrammeOut>,
    projects: Vec<ProjectOut>,
}

/// One opaque page from a literal GET or an action output.
fn page<T: serde::de::DeserializeOwned>(
    value: serde_json::Value,
    what: &str,
) -> Result<Page<T>, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Network(format!("{what}: {error}")))
}

async fn load_portfolio(ctx: AppCtx) -> Result<PortfolioData, ApiError> {
    let client = ctx.client();
    let value = client.get("/api/projects?limit=200").await?;
    let projects = page::<ProjectOut>(value, "projects")?.items;

    let mut programmes = Vec::new();
    if ctx.has_action("programmes.list")
        && let Ok(invocation) = client
            .invoke(
                "programmes.list",
                json!({ "page": { "limit": 50 } }),
                InvokeOptions::default(),
            )
            .await
        && let Ok(parsed) = page::<ProgrammeOut>(invocation.output, "programmes")
    {
        programmes = parsed.items;
    }

    let mut spend = HashMap::new();
    if ctx.has_action("inference_usage.report") {
        let until = Utc::now();
        let since = until - Duration::days(30);
        let input = json!({
            "since": since.to_rfc3339(),
            "until": until.to_rfc3339(),
            "group_by": "project",
        });
        if let Ok(invocation) = client
            .invoke("inference_usage.report", input, InvokeOptions::default())
            .await
            && let Ok(report) = serde_json::from_value::<UsageReport>(invocation.output)
        {
            for row in report.rows {
                spend.insert(
                    row.bucket,
                    Spend {
                        cost_usd: row.cost_usd,
                        priced: row.priced_calls,
                        unpriced: row.unpriced_calls,
                    },
                );
            }
        }
    }

    Ok(PortfolioData {
        projects,
        programmes,
        spend,
    })
}

async fn load_delivery(ctx: AppCtx, id: &str) -> Result<DeliveryDetail, ApiError> {
    let client = ctx.client();
    let value = client.get(&format!("/api/projects/{id}")).await?;
    let view: ProjectView = serde_json::from_value(value)
        .map_err(|error| ApiError::Network(format!("project: {error}")))?;
    let board = view.boards.first().cloned();
    let mut board_view = None;
    let mut cards = Vec::new();
    if let Some(board) = &board {
        let value = client.get(&format!("/api/boards/{}", board.id)).await?;
        board_view = Some(
            serde_json::from_value::<BoardView>(value)
                .map_err(|error| ApiError::Network(format!("board: {error}")))?,
        );
        let value = client
            .get(&format!("/api/boards/{}/cards?limit=200", board.id))
            .await?;
        cards = page::<CardOut>(value, "cards")?.items;
    }
    Ok(DeliveryDetail {
        project: view.project,
        board: board_view,
        cards,
    })
}

async fn load_programmes(ctx: AppCtx) -> Result<ProgrammeData, ApiError> {
    if !ctx.has_action("programmes.list") {
        return Ok(ProgrammeData::default());
    }
    let client = ctx.client();
    let invocation = client
        .invoke(
            "programmes.list",
            json!({ "page": { "limit": 50 } }),
            InvokeOptions::default(),
        )
        .await?;
    let programmes = page::<ProgrammeOut>(invocation.output, "programmes")?.items;
    let value = client.get("/api/projects?limit=200").await?;
    let projects = page::<ProjectOut>(value, "projects")?.items;
    Ok(ProgrammeData {
        programmes,
        projects,
    })
}

fn meta_text(project: &ProjectOut, key: &str) -> Option<String> {
    project
        .meta
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::to_owned)
}

/// The planned window from the project's own meta; both dates or nothing.
fn plan_label(project: &ProjectOut) -> String {
    match (
        meta_text(project, "plan_start"),
        meta_text(project, "plan_end"),
    ) {
        (Some(start), Some(end)) => format!("{start} → {end}"),
        _ => "no dated activity".to_owned(),
    }
}

/// A spend cell. A row with no priced call is `—*` with its count, never `$0`.
fn spend_label(spend: Option<&Spend>) -> (String, String) {
    match spend {
        None => ("—*".to_owned(), "no priced call".to_owned()),
        // The report may carry a zero row for a project with no calls; it is not an unpriced
        // count, it is the same honest "no priced call" as an absent row.
        Some(spend) if spend.priced == 0 && spend.unpriced == 0 => {
            ("—*".to_owned(), "no priced call".to_owned())
        }
        Some(spend) if spend.priced == 0 => {
            ("—*".to_owned(), format!("{} unpriced", spend.unpriced))
        }
        Some(spend) if spend.unpriced > 0 => (
            "—*".to_owned(),
            format!("{} priced · {} unpriced", spend.priced, spend.unpriced),
        ),
        Some(spend) => (
            format!("${}", spend.cost_usd),
            format!("{} priced", spend.priced),
        ),
    }
}

#[component]
fn NotYetLive(slice: &'static str, detail: &'static str) -> impl IntoView {
    view! {
        <div class="panel planned-panel">
            <h2>"Not yet live"</h2>
            <p class="muted">
                {format!("{detail} (owner: {slice}).")}
            </p>
        </div>
    }
}

/// A V2 destination whose slice has not landed. The route is reserved; a deep link is honest.
#[component]
pub fn DestinationGate(title: &'static str, slice: &'static str) -> impl IntoView {
    view! {
        <section class="screen">
            <header class="screen-head">
                <div><h1>{title}</h1></div>
            </header>
            <NotYetLive
                slice=slice
                detail="This destination is not wired in the app yet; its screen and actions are absent"
            />
        </section>
    }
}

#[component]
pub fn OrganisationGate() -> impl IntoView {
    view! { <DestinationGate title="Organisation" slice="T0 · T2"/> }
}

#[component]
pub fn ProceduresGate() -> impl IntoView {
    view! { <DestinationGate title="Procedures" slice="T16"/> }
}

#[component]
pub fn AuthoringGate() -> impl IntoView {
    view! { <DestinationGate title="Authoring" slice="T17"/> }
}

#[component]
pub fn PlanGate() -> impl IntoView {
    view! { <DestinationGate title="Plan" slice="T1"/> }
}

#[component]
pub fn ImproveGate() -> impl IntoView {
    view! { <DestinationGate title="Improve" slice="T4 · T5"/> }
}

#[component]
pub fn OperationsGate() -> impl IntoView {
    view! { <DestinationGate title="Operations" slice="T7"/> }
}

#[component]
pub fn ToolsGate() -> impl IntoView {
    view! { <DestinationGate title="Tools" slice="T13 · T14 · T15"/> }
}

#[component]
pub fn AdmGate() -> impl IntoView {
    view! { <DestinationGate title="Architecture capability" slice="T9"/> }
}

#[component]
pub fn KnowledgeGate() -> impl IntoView {
    view! { <DestinationGate title="Knowledge & memory" slice="T8 · T15"/> }
}

#[component]
pub fn PortfolioScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let data = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        async move { load_portfolio(ctx).await }
    });
    let selected = RwSignal::new(None::<String>);
    let detail = LocalResource::new(move || {
        let id = selected.get();
        async move {
            match id {
                None => Ok(None),
                Some(id) => load_delivery(ctx, &id).await.map(Some),
            }
        }
    });
    let retry = Callback::new(move |_| data.refetch());

    view! {
        <section class="screen" aria-labelledby="portfolio-title">
            <header class="screen-head">
                <div>
                    <h1 id="portfolio-title">"Portfolio"</h1>
                    <p class="muted">"The in-flight set and the L1–L4 delivery view, from the tenant's own rows."</p>
                </div>
                <button class="btn" on:click=move |_| data.refetch()>"Refresh"</button>
            </header>
            {move || match data.get() {
                None => view! { <p class="muted">"Loading…"</p> }.into_any(),
                Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                Some(Ok(data)) => view! {
                    <PortfolioBody data=data selected=selected detail=detail/>
                }.into_any(),
            }}
        </section>
    }
}

#[component]
fn PortfolioBody(
    data: PortfolioData,
    selected: RwSignal<Option<String>>,
    detail: LocalResource<Result<Option<DeliveryDetail>, ApiError>>,
) -> impl IntoView {
    let mut projects = data.projects;
    projects.sort_by(|left, right| left.key.cmp(&right.key));
    let programmes = data.programmes;
    let spend = data.spend;
    let in_flight = projects
        .iter()
        .filter(|project| matches!(project.status.as_str(), "active" | "draft" | "paused"))
        .count();
    let programme_name = |project: &ProjectOut| -> String {
        project
            .programme_id
            .as_ref()
            .and_then(|id| programmes.iter().find(|programme| &programme.id == id))
            .map(|programme| programme.name.clone())
            .unwrap_or_else(|| "—".to_owned())
    };
    let programme_count = |id: &str| {
        projects
            .iter()
            .filter(|project| project.programme_id.as_deref() == Some(id))
            .count()
    };
    let project_rows = projects.clone();
    let programme_rows = programmes.clone();
    let detail_retry = Callback::new(move |_| detail.refetch());

    view! {
        <div class="grid-3">
            <Tile label="Projects".to_owned() value=fmt_int(projects.len() as i64) detail="tenant rows".to_owned()/>
            <Tile label="In flight".to_owned() value=fmt_int(in_flight as i64) detail="active · draft · paused".to_owned()/>
            <Tile label="Programmes".to_owned() value=fmt_int(programmes.len() as i64) detail="tenant rows".to_owned()/>
        </div>
        <h2 id="l1">"L1 · Portfolio"</h2>
        {if project_rows.is_empty() {
            view! { <Empty message="No projects in this tenant yet.".to_owned()/> }.into_any()
        } else {
            view! {
                <table class="table">
                    <thead>
                        <tr>
                            <th>"Key"</th><th>"Project"</th><th>"Status"</th><th>"Programme"</th>
                            <th>"Plan"</th><th>"Spend (30d)"</th><th></th>
                        </tr>
                    </thead>
                    <tbody>
                        {project_rows.into_iter().map(|project| {
                            let id = project.id.clone();
                            let plan = plan_label(&project);
                            let (amount, note) = spend_label(spend.get(&project.id));
                            let programme = programme_name(&project);
                            view! {
                                <tr>
                                    <td class="mono">{project.key.clone()}</td>
                                    <td>{project.name.clone()}</td>
                                    <td><StatusPill status=project.status.clone()/></td>
                                    <td>{programme}</td>
                                    <td>{plan}</td>
                                    <td>
                                        <span class="mono">{amount}</span>
                                        <span class="muted">" · "</span>
                                        <span class="muted">{note}</span>
                                    </td>
                                    <td>
                                        <button class="btn small" on:click=move |_| selected.set(Some(id.clone()))>
                                            "Open"
                                        </button>
                                    </td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            }.into_any()
        }}
        <h2 id="l2">"L2 · Programme"</h2>
        {if programme_rows.is_empty() {
            view! {
                <NotYetLive
                    slice="T10"
                    detail="No programme is readable for this principal; the programme definition read is absent"
                />
            }.into_any()
        } else {
            view! {
                <table class="table">
                    <thead><tr><th>"Programme"</th><th>"Status"</th><th>"Projects"</th><th>"Owner"</th></tr></thead>
                    <tbody>
                        {programme_rows.into_iter().map(|programme| {
                            let count = programme_count(&programme.id);
                            let owner = programme.owner_user_id.clone().unwrap_or_else(|| "—".to_owned());
                            view! {
                                <tr>
                                    <td>{programme.name.clone()}</td>
                                    <td><StatusPill status=programme.status.clone()/></td>
                                    <td>{fmt_int(count as i64)}</td>
                                    <td class="mono">{owner}</td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            }.into_any()
        }}
        {move || match detail.get() {
            None => view! {
                <p class="muted">"Choose a project's Open to load its board and work packages."</p>
            }.into_any(),
            Some(Err(error)) => view! {
                <ErrorPanel error=error on_retry=detail_retry/>
            }.into_any(),
            Some(Ok(None)) => view! {
                <p class="muted">"Choose a project's Open to load its board and work packages."</p>
            }.into_any(),
            Some(Ok(Some(detail))) => view! {
                <DeliveryLevels detail=detail/>
            }.into_any(),
        }}
    }
}

#[component]
fn DeliveryLevels(detail: DeliveryDetail) -> impl IntoView {
    let board_name = detail
        .board
        .as_ref()
        .map(|board| board.board.name.clone())
        .unwrap_or_else(|| "no board".to_owned());
    let no_board = detail.board.is_none();
    let lists = detail
        .board
        .as_ref()
        .map(|board| board.lists.clone())
        .unwrap_or_default();
    let cards = detail.cards.clone();
    let lane_names: HashMap<String, String> = lists
        .iter()
        .map(|lane| (lane.list.id.clone(), lane.list.name.clone()))
        .collect();
    let card_lane = |list_id: &str| -> String {
        lane_names
            .get(list_id)
            .cloned()
            .unwrap_or_else(|| "—".to_owned())
    };
    let project = detail.project.clone();
    let card_rows = cards.clone();

    view! {
        <h2 id="l3">"L3 · Project"</h2>
        <section class="panel" aria-labelledby="l3-board">
            <h3 id="l3-board">{format!("{} · {}", project.key, project.name)}</h3>
            <p class="muted">{format!("Board: {board_name}")}</p>
            {if no_board {
                view! { <Empty message="This project has no board.".to_owned()/> }.into_any()
            } else {
                view! {
                    <div class="grid-2">
                        {lists.into_iter().map(|lane| {
                            let lane_id = lane.list.id.clone();
                            let lane_cards = cards.iter().filter(|card| card.list_id == lane_id).cloned().collect::<Vec<_>>();
                            view! {
                                <section class="panel">
                                    <h4>{lane.list.name.clone()}</h4>
                                    <p class="muted">{format!("{} cards", lane_cards.len())}</p>
                                    {lane_cards.into_iter().map(|card| view! {
                                        <div class="row-card">
                                            <div class="row-title">{card.name.clone()}</div>
                                        </div>
                                    }).collect_view()}
                                </section>
                            }
                        }).collect_view()}
                    </div>
                }.into_any()
            }}
        </section>
        <h2 id="l4">"L4 · Work package"</h2>
        <section class="panel" aria-labelledby="l4-cards">
            <h3 id="l4-cards">"Work packages"</h3>
            {if card_rows.is_empty() {
                view! { <Empty message="No work packages on this board yet.".to_owned()/> }.into_any()
            } else {
                view! {
                    <table class="table">
                        <thead><tr><th>"Work package"</th><th>"Lane"</th><th>"Updated"</th><th></th></tr></thead>
                        <tbody>
                            {card_rows.into_iter().map(|card| {
                                let lane = card_lane(&card.list_id);
                                let updated = age(card.updated_at);
                                let href = format!("/card/{}", card.id);
                                view! {
                                    <tr>
                                        <td>{card.name.clone()}</td>
                                        <td>{lane}</td>
                                        <td>{updated}</td>
                                        <td><A href=href>"Open execution"</A></td>
                                    </tr>
                                }
                            }).collect_view()}
                        </tbody>
                    </table>
                }.into_any()
            }}
        </section>
    }
}

#[component]
pub fn ProgrammeScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let has = move || ctx.has_action("programmes.list");
    let data = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        async move { load_programmes(ctx).await }
    });
    let retry = Callback::new(move |_| data.refetch());

    view! {
        <section class="screen" aria-labelledby="programme-title">
            <header class="screen-head">
                <div>
                    <h1 id="programme-title">"Programme"</h1>
                    <p class="muted">"The programme definition read, from the tenant's rows."</p>
                </div>
                <button class="btn" on:click=move |_| data.refetch()>"Refresh"</button>
            </header>
            {move || if !has() {
                view! {
                    <NotYetLive
                        slice="T10"
                        detail="The programme definition read is not in this principal's manifest"
                    />
                }.into_any()
            } else {
                match data.get() {
                    None => view! { <p class="muted">"Loading…"</p> }.into_any(),
                    Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                    Some(Ok(data)) => view! { <ProgrammeBody data=data/> }.into_any(),
                }
            }}
        </section>
    }
}

#[component]
fn ProgrammeBody(data: ProgrammeData) -> impl IntoView {
    let mut programmes = data.programmes;
    programmes.sort_by(|left, right| left.name.cmp(&right.name));
    let projects = data.projects;
    let count = |id: &str| {
        projects
            .iter()
            .filter(|project| project.programme_id.as_deref() == Some(id))
            .count()
    };
    view! {
        {if programmes.is_empty() {
            view! { <Empty message="No programmes in this tenant yet.".to_owned()/> }.into_any()
        } else {
            view! {
                <table class="table">
                    <thead><tr><th>"Programme"</th><th>"Status"</th><th>"Projects"</th><th>"Owner"</th><th>"Version"</th></tr></thead>
                    <tbody>
                        {programmes.into_iter().map(|programme| {
                            let projects = count(&programme.id);
                            let owner = programme.owner_user_id.clone().unwrap_or_else(|| "—".to_owned());
                            view! {
                                <tr>
                                    <td>{programme.name.clone()}</td>
                                    <td><StatusPill status=programme.status.clone()/></td>
                                    <td>{fmt_int(projects as i64)}</td>
                                    <td class="mono">{owner}</td>
                                    <td class="mono">{programme.version.to_string()}</td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            }.into_any()
        }}
        <NotYetLive
            slice="T10"
            detail="The benefit profiles, the benefits map and the tranche reviews are not rendered here yet"
        />
    }
}
