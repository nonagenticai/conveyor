//! Portfolio — the in-flight portfolio view (SPEC_V2 §10.5, destination **Portfolio**).
//!
//! One read, `portfolio.in_flight`, and no write: there is no portfolio row. The in-flight set
//! is computed from the tenant's configured statuses (`tenant.settings -> 'portfolio' ->
//! 'in_flight'`, default `active,paused`), so this screen shows what the read returns and
//! cannot drift from it.
//!
//! Two panels share the one envelope:
//! - **Programmes** — every in-flight programme with its status and the count of in-flight
//!   projects that name it. A programme with no projects still appears.
//! - **Projects** — key, name, status, and the programme name resolved by id (`—` when the
//!   project belongs to none).
//!
//! Four tiles count the two sets and the split between projects with and without a programme.
//! SPEC_V2 §10.5's programme shared config, dependency graph, backlog/WSJF, cadence, capacity
//! and metering are separate destinations, not controls this read-only screen invents.

use leptos::prelude::*;
use serde::Deserialize;
use serde_json::json;

use crate::api::{ApiError, InvokeOptions};
use crate::components::{Empty, ErrorPanel, Planned, StatusPill, Tile};
use crate::state::AppCtx;

/// One in-flight project (`InFlightOut` in the action's output).
#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
struct Project {
    id: String,
    key: String,
    name: String,
    status: String,
    #[serde(default)]
    programme_id: Option<String>,
    #[serde(default)]
    version: i64,
}

/// One in-flight programme (`InFlightProgrammeOut`).
#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
struct Programme {
    id: String,
    name: String,
    status: String,
    #[serde(default)]
    version: i64,
}

/// `portfolio.in_flight`'s output: the project page plus the in-flight programmes.
#[derive(Clone, Debug, Default, Deserialize)]
#[allow(dead_code)]
struct InFlight {
    #[serde(default)]
    items: Vec<Project>,
    #[serde(default)]
    programmes: Vec<Programme>,
    #[serde(default)]
    next_cursor: Option<String>,
}

fn load(ctx: AppCtx) -> LocalResource<Result<InFlight, ApiError>> {
    LocalResource::new(move || {
        // Read the tenant so a switch refetches the computed set.
        let _tenant = ctx.tenant.get();
        async move {
            let output = ctx
                .invoke(
                    "portfolio.in_flight",
                    json!({"limit": 100}),
                    InvokeOptions::default(),
                )
                .await?
                .output;
            serde_json::from_value(output)
                .map_err(|error| ApiError::Network(format!("portfolio in flight: {error}")))
        }
    })
}

#[component]
pub fn Portfolio() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let in_flight = load(ctx);

    view! {
        <section class="screen">
            {move || {
                if !ctx.has_action("portfolio.in_flight") {
                    return view! {
                        <Planned
                            title="Portfolio is planned".to_owned()
                            detail="portfolio.in_flight is not in this role's manifest; the screen appears when the action lands.".to_owned()
                        />
                    }
                    .into_any();
                }
                view! {
                    <header class="screen-head">
                        <div>
                            <h1>"Portfolio"</h1>
                            <p class="muted">
                                "The in-flight set is computed from project status, and a programme with no projects still appears."
                            </p>
                        </div>
                    </header>
                    {move || match in_flight.get() {
                        None => view! { <p class="muted">"Reading the in-flight set…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=Callback::new(move |_| in_flight.refetch())/>
                        }.into_any(),
                        Some(Ok(data)) => view! { <PortfolioBody data=data/> }.into_any(),
                    }}
                }
                .into_any()
            }}
        </section>
    }
}

#[component]
fn PortfolioBody(data: InFlight) -> impl IntoView {
    let projects = data.items;
    let programmes = data.programmes;
    let in_flight_projects = projects.len();
    let in_flight_programmes = programmes.len();
    let with_programme = projects
        .iter()
        .filter(|project| project.programme_id.is_some())
        .count();
    let without_programme = in_flight_projects - with_programme;
    let nothing = projects.is_empty() && programmes.is_empty();

    // Resolve a project's programme by id, and count each programme's in-flight projects.
    let names: std::collections::HashMap<String, String> = programmes
        .iter()
        .map(|programme| (programme.id.clone(), programme.name.clone()))
        .collect();
    let programme_rows: Vec<(Programme, usize)> = programmes
        .into_iter()
        .map(|programme| {
            let count = projects
                .iter()
                .filter(|project| project.programme_id.as_deref() == Some(programme.id.as_str()))
                .count();
            (programme, count)
        })
        .collect();

    view! {
        <div class="tiles">
            <Tile
                label="In-flight projects".to_owned()
                value=in_flight_projects.to_string()
                detail="projects in the in-flight status set".to_owned()
            />
            <Tile
                label="In-flight programmes".to_owned()
                value=in_flight_programmes.to_string()
                detail="programmes in the in-flight set".to_owned()
            />
            <Tile
                label="With a programme".to_owned()
                value=with_programme.to_string()
                detail="in-flight projects linked to a programme".to_owned()
            />
            <Tile
                label="Without".to_owned()
                value=without_programme.to_string()
                detail="in-flight projects with no programme".to_owned()
            />
        </div>
        {if nothing {
            view! {
                <Empty message="Nothing is in flight — no project or programme is in the in-flight status set.".to_owned()/>
            }
            .into_any()
        } else {
            view! {
                {(!programme_rows.is_empty()).then(|| view! {
                    <h2>"Programmes"</h2>
                    <div class="tablewrap">
                        <table class="table">
                            <thead><tr>
                                <th>"Name"</th><th>"Status"</th><th class="n">"Projects"</th>
                            </tr></thead>
                            <tbody>
                                {programme_rows.iter().map(|(programme, count)| view! {
                                    <tr>
                                        <td>{programme.name.clone()}</td>
                                        <td><StatusPill status=programme.status.clone()/></td>
                                        <td class="n">{count.to_string()}</td>
                                    </tr>
                                }).collect_view()}
                            </tbody>
                        </table>
                    </div>
                })}
                {(!projects.is_empty()).then(|| view! {
                    <h2>"Projects"</h2>
                    <div class="tablewrap">
                        <table class="table">
                            <thead><tr>
                                <th>"Key"</th><th>"Name"</th><th>"Status"</th><th>"Programme"</th>
                            </tr></thead>
                            <tbody>
                                {projects.iter().map(|project| {
                                    let programme = project
                                        .programme_id
                                        .as_ref()
                                        .and_then(|id| names.get(id))
                                        .cloned()
                                        .unwrap_or_else(|| "—".to_owned());
                                    view! {
                                        <tr>
                                            <td class="mono">{project.key.clone()}</td>
                                            <td>{project.name.clone()}</td>
                                            <td><StatusPill status=project.status.clone()/></td>
                                            <td>{programme}</td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                    </div>
                })}
            }
            .into_any()
        }}
    }
}
