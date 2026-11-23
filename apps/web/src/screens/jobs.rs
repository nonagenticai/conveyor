//! Jobs — the catalogue positions, their published releases and their bindings.
//!
//! A binding pins a release hash; the server refuses a retired binding or a stale pin with
//! `409 job_binding_stale`. The start control sends the caller's expected hash, so the screen
//! never guesses client-side: the server's own refusal is what is shown.

use leptos::prelude::*;
use serde_json::{Value, json};

use crate::api::{ApiError, InvokeOptions};
use crate::components::{Empty, ErrorPanel, StatusPill, Tile, fmt_int};
use crate::screens::v2_data::{int, read_rows, text};
use crate::state::AppCtx;

#[derive(Clone, Debug, Default)]
struct JobsData {
    jobs: Vec<Value>,
}

async fn load(ctx: AppCtx) -> Result<JobsData, ApiError> {
    let client = ctx.client();
    let jobs = if ctx.has_action("job_catalogue.list") {
        read_rows(&client, "job_catalogue.list", json!({})).await?
    } else {
        Vec::new()
    };
    Ok(JobsData { jobs })
}

#[derive(Clone, Debug, Default)]
struct JobDetail {
    releases: Vec<Value>,
    bindings: Vec<Value>,
    agents: Vec<Value>,
}

async fn load_detail(ctx: AppCtx, job_id: String) -> Result<JobDetail, ApiError> {
    let client = ctx.client();
    let mut detail = JobDetail::default();
    if job_id.is_empty() {
        return Ok(detail);
    }
    if ctx.has_action("job_releases.list") {
        detail.releases =
            read_rows(&client, "job_releases.list", json!({ "job_id": job_id })).await?;
    }
    if ctx.has_action("job_bindings.list") {
        detail.bindings =
            read_rows(&client, "job_bindings.list", json!({ "job_id": job_id })).await?;
    }
    if ctx.has_action("capability_agents.list") {
        detail.agents = read_rows(
            &client,
            "capability_agents.list",
            json!({ "page": { "limit": 50 } }),
        )
        .await?;
    }
    Ok(detail)
}

fn short(value: String) -> String {
    if value == "—" || value.len() <= 14 {
        value
    } else {
        format!("{}…", &value[..14])
    }
}

#[component]
pub fn JobsScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let data = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        async move { load(ctx).await }
    });
    let selected = RwSignal::new(Option::<String>::None);
    let detail = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let job = selected.get();
        async move {
            match job {
                None => Ok(JobDetail::default()),
                Some(job) => load_detail(ctx, job).await,
            }
        }
    });
    let retry = Callback::new(move |_| data.refetch());
    let detail_retry = Callback::new(move |_| detail.refetch());

    view! {
        <section class="screen" aria-labelledby="jobs-title">
            <header class="screen-head">
                <div>
                    <h1 id="jobs-title">"Jobs"</h1>
                    <p class="muted">"The catalogue, the published releases, and the bindings that pin them."</p>
                </div>
                <button class="btn" on:click=move |_| data.refetch()>"Refresh"</button>
            </header>
            {move || match data.get() {
                None => view! { <p class="muted">"Loading…"</p> }.into_any(),
                Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                Some(Ok(data)) => view! { <JobsBody data=data selected=selected/> }.into_any(),
            }}
            {move || match detail.get() {
                None => view! { <p class="muted">"Loading the selected position…"</p> }.into_any(),
                Some(Err(error)) => view! { <ErrorPanel error=error on_retry=detail_retry/> }.into_any(),
                Some(Ok(detail)) => view! {
                    <JobDetailBody detail=detail job_id=selected.get().unwrap_or_default() refresh=detail_retry/>
                }.into_any(),
            }}
        </section>
    }
}

#[component]
fn JobsBody(data: JobsData, selected: RwSignal<Option<String>>) -> impl IntoView {
    let jobs = data.jobs;
    view! {
        <div class="grid-3">
            <Tile label="Positions".to_owned() value=fmt_int(jobs.len() as i64) detail="catalogue rows".to_owned()/>
        </div>
        <h2 id="catalogue">"Catalogue"</h2>
        {if jobs.is_empty() {
            view! { <Empty message="No catalogue positions in this tenant yet.".to_owned()/> }.into_any()
        } else {
            view! {
                <table class="table" data-testid="jobs-catalogue">
                    <thead><tr><th>"Key"</th><th>"Title"</th><th>"Family"</th><th>"Status"</th><th></th></tr></thead>
                    <tbody>
                        {jobs.into_iter().map(|row| {
                            let id = text(&row, "id");
                            let open = id.clone();
                            view! {
                                <tr>
                                    <td class="mono">{text(&row, "key")}</td>
                                    <td>{text(&row, "title")}</td>
                                    <td>{text(&row, "family")}</td>
                                    <td><StatusPill status=text(&row, "status")/></td>
                                    <td>
                                        <button class="btn small" on:click=move |_| selected.set(Some(open.clone()))>
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
    }
}

#[component]
fn JobDetailBody(detail: JobDetail, job_id: String, refresh: Callback<()>) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let releases = detail.releases;
    let bindings = detail.bindings;
    let agents = detail.agents;
    let release_options = releases.clone();
    let card_id = RwSignal::new(String::new());
    let expected_hash = RwSignal::new(String::new());
    let release_id = RwSignal::new(String::new());
    let agent_id = RwSignal::new(String::new());
    let notice = RwSignal::new(Option::<String>::None);

    let publish = {
        let job_id = job_id.clone();
        move |_| {
            let ctx = ctx;
            let job_id = job_id.clone();
            let refresh = refresh;
            leptos::task::spawn_local(async move {
                match ctx
                    .invoke(
                        "job_releases.publish",
                        json!({ "job_id": job_id }),
                        InvokeOptions::default(),
                    )
                    .await
                {
                    Ok(_) => {
                        notice.set(Some("Release published.".to_owned()));
                        refresh.run(());
                    }
                    Err(error) => notice.set(Some(error.to_string())),
                }
            });
        }
    };
    let bind = {
        let job_id = job_id.clone();
        move |_| {
            let ctx = ctx;
            let job_id = job_id.clone();
            let release = release_id.get();
            let agent = agent_id.get();
            let refresh = refresh;
            leptos::task::spawn_local(async move {
                match ctx
                    .invoke(
                        "job_bindings.create",
                        json!({
                            "job_id": job_id,
                            "release_id": release,
                            "executor_kind": "internal_agent",
                            "capability_agent_id": agent,
                        }),
                        InvokeOptions::default(),
                    )
                    .await
                {
                    Ok(_) => {
                        notice.set(Some("Binding created.".to_owned()));
                        refresh.run(());
                    }
                    Err(error) => notice.set(Some(error.to_string())),
                }
            });
        }
    };

    let start = move |binding_id: String| {
        let ctx = ctx;
        let card = card_id.get();
        let hash = expected_hash.get();
        leptos::task::spawn_local(async move {
            let mut input = json!({ "card_id": card, "job_binding_id": binding_id });
            if !hash.is_empty() {
                input["release_hash"] = json!(hash);
            }
            match ctx
                .invoke("runs.create", input, InvokeOptions::default())
                .await
            {
                Ok(invocation) => {
                    notice.set(Some(format!(
                        "Run {} started.",
                        text(&invocation.output, "id")
                    )));
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    };

    view! {
        <h2 id="releases">"Releases"</h2>
        <button class="btn small" disabled=move || job_id.is_empty() on:click=publish>
            "Publish release"
        </button>
        {if releases.is_empty() {
            view! { <Empty message="No release has been published for this position.".to_owned()/> }.into_any()
        } else {
            view! {
                <table class="table" data-testid="jobs-releases">
                    <thead><tr><th>"Hash"</th><th>"Status"</th><th>"Version"</th></tr></thead>
                    <tbody>
                        {releases.into_iter().map(|row| view! {
                            <tr>
                                <td class="mono" title=text(&row, "release_hash")>{short(text(&row, "release_hash"))}</td>
                                <td><StatusPill status=text(&row, "status")/></td>
                                <td class="mono">{int(&row, "version").to_string()}</td>
                            </tr>
                        }).collect_view()}
                    </tbody>
                </table>
            }.into_any()
        }}

        <h2 id="bindings">"Bindings"</h2>
        {if bindings.is_empty() {
            view! { <Empty message="No binding pins this position yet.".to_owned()/> }.into_any()
        } else {
            view! {
                <table class="table" data-testid="jobs-bindings">
                    <thead><tr><th>"Pinned release"</th><th>"Executor"</th><th>"Status"</th><th>"Start a run"</th></tr></thead>
                    <tbody>
                        {bindings.into_iter().map(|row| {
                            let binding_id = text(&row, "id");
                            let start_id = binding_id.clone();
                            view! {
                                <tr>
                                    <td class="mono" title=text(&row, "release_hash")>{short(text(&row, "release_hash"))}</td>
                                    <td>{text(&row, "executor_kind")}</td>
                                    <td><StatusPill status=text(&row, "status")/></td>
                                    <td>
                                        <button class="btn small" on:click=move |_| start(start_id.clone())>
                                            "Start run"
                                        </button>
                                    </td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
                <div class="panel">
                    <h3>"Start a run against a binding"</h3>
                    <p class="muted">"The card must be assigned to this binding. Leave the expected hash empty to accept the binding's own pin; name a different hash to prove the server's 409."</p>
                    <label class="field">
                        <span>"Card id"</span>
                        <input class="input mono" placeholder="00000000-0000-0000-0000-000000000000"
                            on:input=move |event| card_id.set(event_target_value(&event))/></label>
                    <label class="field">
                        <span>"Expected release hash (optional)"</span>
                        <input class="input mono" placeholder="the hash the caller expects"
                            on:input=move |event| expected_hash.set(event_target_value(&event))/></label>
                    <p class="muted">"Choose Start run on the binding above after filling the card id."</p>
                </div>
            }.into_any()
        }}

        <h2 id="create-binding">"Create a binding"</h2>
        <div class="panel">
            <label class="field">
                <span>"Release"</span>
                <select class="input" on:change=move |event| release_id.set(event_target_value(&event))>
                    <option value="">"Choose a published release"</option>
                    {release_options.iter().map(|row| {
                        let id = text(row, "id");
                        let label = format!("{} · {}", short(text(row, "release_hash")), text(row, "status"));
                        view! { <option value=id>{label}</option> }
                    }).collect_view()}
                </select></label>
            <label class="field">
                <span>"Capability agent"</span>
                <select class="input" on:change=move |event| agent_id.set(event_target_value(&event))>
                    <option value="">"Choose a capability agent"</option>
                    {agents.iter().map(|row| {
                        let id = text(row, "id");
                        let label = format!("{} · {}", text(row, "key"), text(row, "status"));
                        view! { <option value=id>{label}</option> }
                    }).collect_view()}
                </select></label>
            <p class="muted">"The binding pins the release and executes the agent; the run path is the one below."</p>
            <button class="btn"
                disabled=move || release_id.get().is_empty() || agent_id.get().is_empty()
                on:click=bind>"Create binding"</button>
        </div>
        {move || notice.get().map(|text| view! {
            <p class="notice" role="alert">{text}</p>
        })}
    }
}
