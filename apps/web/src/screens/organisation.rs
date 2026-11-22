//! Organisation — the tenant's catalogue of industries, departments, jobs and bindings
//! (SPEC_V2 §10.5, destination **Organisation**).
//!
//! Four reads and no write:
//! - `industries.list` — the top of the spine;
//! - `departments.list` — every department with its industry placement;
//! - `job_catalogue.list` — every position with its family and status;
//! - `job_bindings.list` — the releases pinned to executors. Its input takes a job id, so it
//!   is read once per catalogue position and each row is labelled with that position's key.
//!
//! The catalogue is a tenant's curated positions and placements: a created position is a draft
//! (`job_catalogue.create` writes `status = draft`), and a release is compiled and hashed by
//! `job_releases.publish`, never authored. This screen therefore has no create, update, publish
//! or retire control — those are not invented here. `DepartmentOut` carries no key and no
//! status (its identity is the id/name; its placement is the ordinal and the primary flag), so
//! the department table shows those fields instead of inventing either.

use leptos::prelude::*;
use serde::Deserialize;
use serde_json::json;

use crate::api::{ApiError, InvokeOptions};
use crate::components::{Empty, ErrorPanel, Planned, StatusPill, Tile, short_id};
use crate::state::AppCtx;

/// One industry (`IndustryOut`).
#[derive(Clone, Debug, Deserialize)]
struct Industry {
    key: String,
    name: String,
    status: String,
}

/// One catalogue position (`JobOut`).
#[derive(Clone, Debug, Deserialize)]
struct Job {
    id: String,
    key: String,
    title: String,
    #[serde(default)]
    family: String,
    #[serde(default)]
    status: String,
}

/// The rendered half of one binding (`BindingOut`). The job id is kept so a row can still be
/// labelled when the API returns a binding outside the job that was asked for.
#[derive(Clone, Debug, Deserialize)]
struct Binding {
    #[serde(default)]
    job_id: String,
    #[serde(default)]
    release_hash: String,
    #[serde(default)]
    status: String,
}

/// One row of the bindings table: the binding plus the catalogue key of the job read.
#[derive(Clone)]
struct BindingRow {
    job: String,
    release_hash: String,
    status: String,
}

/// One department (`DepartmentOut`).
#[derive(Clone, Debug, Deserialize)]
struct Department {
    id: String,
    name: String,
    #[serde(default)]
    ordinal: Option<i32>,
    #[serde(default)]
    is_primary: bool,
}

fn parse<T: serde::de::DeserializeOwned>(
    value: serde_json::Value,
    what: &str,
) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Network(format!("{what}: {error}")))
}

fn load_industries(ctx: AppCtx) -> LocalResource<Result<Vec<Industry>, ApiError>> {
    LocalResource::new(move || {
        // The tenant is a reactive input: a switch refetches every read on this screen.
        let _tenant = ctx.tenant.get();
        async move {
            parse(
                ctx.invoke(
                    "industries.list",
                    json!({"limit": 100}),
                    InvokeOptions::default(),
                )
                .await?
                .output,
                "industries",
            )
        }
    })
}

fn load_departments(ctx: AppCtx) -> LocalResource<Result<Vec<Department>, ApiError>> {
    LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        async move {
            parse(
                ctx.invoke("departments.list", json!({}), InvokeOptions::default())
                    .await?
                    .output,
                "departments",
            )
        }
    })
}

fn load_jobs(ctx: AppCtx) -> LocalResource<Result<Vec<Job>, ApiError>> {
    LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        async move {
            parse(
                ctx.invoke(
                    "job_catalogue.list",
                    json!({"limit": 100}),
                    InvokeOptions::default(),
                )
                .await?
                .output,
                "job catalogue",
            )
        }
    })
}

/// Bindings are per job (`job_bindings.list` names its job in the input), so the read waits
/// for the catalogue: `Ok(None)` is "the catalogue has not loaded yet", not "no binding".
fn load_bindings(
    ctx: AppCtx,
    jobs: LocalResource<Result<Vec<Job>, ApiError>>,
) -> LocalResource<Result<Option<Vec<BindingRow>>, ApiError>> {
    LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let jobs = jobs.get();
        async move {
            let jobs = match jobs {
                None => return Ok(None),
                Some(Err(error)) => return Err(error),
                Some(Ok(jobs)) => jobs,
            };
            let mut rows = Vec::new();
            for job in jobs {
                let bindings: Vec<Binding> = parse(
                    ctx.invoke(
                        "job_bindings.list",
                        json!({"job_id": job.id.as_str()}),
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                    "job bindings",
                )?;
                for binding in bindings {
                    let label = if binding.job_id == job.id {
                        job.key.clone()
                    } else {
                        short_id(Some(&binding.job_id))
                    };
                    rows.push(BindingRow {
                        job: label,
                        release_hash: binding.release_hash,
                        status: binding.status,
                    });
                }
            }
            Ok(Some(rows))
        }
    })
}

/// A count, or `—` while the read has not answered.
fn count<T>(state: Option<Result<Vec<T>, ApiError>>) -> String {
    match state {
        Some(Ok(items)) => items.len().to_string(),
        _ => "—".to_owned(),
    }
}

fn count_jobs(state: Option<Result<Vec<Job>, ApiError>>) -> (String, String) {
    match state {
        Some(Ok(jobs)) => {
            let active = jobs.iter().filter(|job| job.status == "active").count();
            (jobs.len().to_string(), active.to_string())
        }
        _ => ("—".to_owned(), "—".to_owned()),
    }
}

fn count_bindings(state: Option<Result<Option<Vec<BindingRow>>, ApiError>>) -> String {
    match state {
        Some(Ok(Some(rows))) => rows.len().to_string(),
        _ => "—".to_owned(),
    }
}

fn or_dash(value: &str) -> String {
    if value.is_empty() {
        "—".to_owned()
    } else {
        value.to_owned()
    }
}

#[component]
pub fn Organisation() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let industries = load_industries(ctx);
    let departments = load_departments(ctx);
    let jobs = load_jobs(ctx);
    let bindings = load_bindings(ctx, jobs);

    view! {
        <section class="screen">
            {move || {
                let missing: Vec<&str> = ["industries.list", "job_catalogue.list"]
                    .into_iter()
                    .filter(|name| !ctx.has_action(name))
                    .collect();
                if !missing.is_empty() {
                    let detail = if missing.len() == 1 {
                        format!(
                            "{} is not in this role's manifest; the screen appears when it is present.",
                            missing[0],
                        )
                    } else {
                        format!(
                            "{} are not in this role's manifest; the screen appears when they are present.",
                            missing.join(" / "),
                        )
                    };
                    return view! {
                        <Planned title="Organisation is planned".to_owned() detail=detail/>
                    }
                    .into_any();
                }
                view! {
                    <header class="screen-head">
                        <div>
                            <h1>"Organisation"</h1>
                            <p class="muted">
                                "The catalogue is a tenant's curated positions and placements; a created position is a draft and a release is compiled, never authored."
                            </p>
                        </div>
                    </header>
                    {move || {
                        let (jobs_value, active_value) = count_jobs(jobs.get());
                        view! {
                            <div class="tiles">
                                <Tile
                                    label="Industries".to_owned()
                                    value=count(industries.get())
                                    detail="industries in the tenant".to_owned()
                                />
                                <Tile
                                    label="Jobs".to_owned()
                                    value=jobs_value
                                    detail="positions in the catalogue".to_owned()
                                />
                                <Tile
                                    label="Active jobs".to_owned()
                                    value=active_value
                                    detail="positions with status active".to_owned()
                                />
                                <Tile
                                    label="Bindings".to_owned()
                                    value=count_bindings(bindings.get())
                                    detail="releases pinned to an executor".to_owned()
                                />
                            </div>
                        }
                    }}
                    <h2>"Industries"</h2>
                    {move || match industries.get() {
                        None => view! { <p class="muted">"Reading industries…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel
                                error=error
                                on_retry=Callback::new(move |_| industries.refetch())
                            />
                        }
                        .into_any(),
                        Some(Ok(items)) => {
                            let empty = items.is_empty();
                            view! {
                                {empty.then(|| view! {
                                    <Empty message="No industry in this tenant; `migrate --seed` seeds the catalogue.".to_owned()/>
                                })}
                                <div class="tablewrap">
                                    <table class="table">
                                        <thead>
                                            <tr>
                                                <th>"Key"</th>
                                                <th>"Name"</th>
                                                <th>"Status"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {items
                                                .into_iter()
                                                .map(|industry| view! {
                                                    <tr>
                                                        <td class="mono">{industry.key}</td>
                                                        <td>{industry.name}</td>
                                                        <td><StatusPill status=industry.status/></td>
                                                    </tr>
                                                })
                                                .collect_view()}
                                        </tbody>
                                    </table>
                                </div>
                            }
                            .into_any()
                        }
                    }}
                    <h2>"Jobs"</h2>
                    {move || match jobs.get() {
                        None => view! { <p class="muted">"Reading the job catalogue…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=Callback::new(move |_| jobs.refetch())/>
                        }
                        .into_any(),
                        Some(Ok(items)) => {
                            let empty = items.is_empty();
                            view! {
                                {empty.then(|| view! {
                                    <Empty message="No position in the catalogue; a created position starts as a draft.".to_owned()/>
                                })}
                                <div class="tablewrap">
                                    <table class="table">
                                        <thead>
                                            <tr>
                                                <th>"Job key"</th>
                                                <th>"Title"</th>
                                                <th>"Family"</th>
                                                <th>"Status"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {items
                                                .into_iter()
                                                .map(|job| view! {
                                                    <tr>
                                                        <td class="mono">{job.key}</td>
                                                        <td>{job.title}</td>
                                                        <td>{or_dash(&job.family)}</td>
                                                        <td><StatusPill status=job.status/></td>
                                                    </tr>
                                                })
                                                .collect_view()}
                                        </tbody>
                                    </table>
                                </div>
                            }
                            .into_any()
                        }
                    }}
                    <h2>"Bindings"</h2>
                    {move || match bindings.get() {
                        None => view! { <p class="muted">"Reading the bindings…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=Callback::new(move |_| bindings.refetch())/>
                        }
                        .into_any(),
                        Some(Ok(None)) => view! {
                            <p class="muted">"Reading the job catalogue before the bindings…"</p>
                        }
                        .into_any(),
                        Some(Ok(Some(items))) => {
                            let empty = items.is_empty();
                            view! {
                                {empty.then(|| view! {
                                    <Empty message="No binding in this tenant; a binding pins a release to a real executor and the seed creates none.".to_owned()/>
                                })}
                                <div class="tablewrap">
                                    <table class="table">
                                        <thead>
                                            <tr>
                                                <th>"Job"</th>
                                                <th>"Release"</th>
                                                <th>"Status"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {items
                                                .into_iter()
                                                .map(|binding| view! {
                                                    <tr>
                                                        <td class="mono">{binding.job}</td>
                                                        <td class="mono">{short_id(Some(&binding.release_hash))}</td>
                                                        <td><StatusPill status=binding.status/></td>
                                                    </tr>
                                                })
                                                .collect_view()}
                                        </tbody>
                                    </table>
                                </div>
                            }
                            .into_any()
                        }
                    }}
                    <h2>"Departments"</h2>
                    {move || match departments.get() {
                        None => view! { <p class="muted">"Reading departments…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=Callback::new(move |_| departments.refetch())/>
                        }
                        .into_any(),
                        Some(Ok(items)) => {
                            let empty = items.is_empty();
                            view! {
                                {empty.then(|| view! {
                                    <Empty message="No department in this tenant; `migrate --seed` seeds the five software departments.".to_owned()/>
                                })}
                                <div class="tablewrap">
                                    <table class="table">
                                        <thead>
                                            <tr>
                                                <th>"Id"</th>
                                                <th>"Name"</th>
                                                <th>"Placement"</th>
                                                <th>"Primary"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {items
                                                .into_iter()
                                                .map(|department| {
                                                    let placement = department
                                                        .ordinal
                                                        .map(|ordinal| format!("#{ordinal}"))
                                                        .unwrap_or_else(|| "—".to_owned());
                                                    let primary = if department.is_primary { "yes" } else { "—" };
                                                    view! {
                                                        <tr>
                                                            <td class="mono">{short_id(Some(&department.id))}</td>
                                                            <td>{department.name}</td>
                                                            <td class="mono">{placement}</td>
                                                            <td>{primary}</td>
                                                        </tr>
                                                    }
                                                })
                                                .collect_view()}
                                        </tbody>
                                    </table>
                                </div>
                            }
                            .into_any()
                        }
                    }}
                }
                .into_any()
            }}
        </section>
    }
}
