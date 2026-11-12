//! Admin — the registry, jobs, the audit log, the licence and the unconfigured externals
//! (S12 screen 9).
//!
//! The screen is a read surface with one write (`jobs.retry`). It shows what the service
//! actually reports: a tab whose upstream is not configured renders the reason, never zeros,
//! and the action detail is `actions.describe`'s own schema rather than a second description.

use chrono::{DateTime, Utc};
use leptos::prelude::*;
use serde::Deserialize;
use serde_json::{Value, json};
use wasm_bindgen::JsCast;

use crate::api::{ApiError, InvokeOptions, Page};
use crate::components::{Empty, ErrorPanel, Planned, StatusPill, age, short_id};
use crate::state::AppCtx;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Actions,
    Jobs,
    Audit,
    Licence,
    Workers,
    Billing,
}

impl Tab {
    const ALL: [Tab; 6] = [
        Tab::Actions,
        Tab::Jobs,
        Tab::Audit,
        Tab::Licence,
        Tab::Workers,
        Tab::Billing,
    ];

    fn label(self) -> &'static str {
        match self {
            Tab::Actions => "Actions",
            Tab::Jobs => "Jobs",
            Tab::Audit => "Audit",
            Tab::Licence => "Licence",
            Tab::Workers => "Workers",
            Tab::Billing => "Billing",
        }
    }

    fn action(self) -> &'static str {
        match self {
            Tab::Actions => "actions.list",
            Tab::Jobs => "jobs.list",
            Tab::Audit => "audit.list",
            Tab::Licence => "licences.get",
            Tab::Workers => "workers.list",
            Tab::Billing => "billing.summary",
        }
    }
}

const JOB_STATUSES: [&str; 5] = ["pending", "running", "succeeded", "failed", "dead"];

#[derive(Clone, Debug, Deserialize)]
struct ActionManifest {
    #[serde(default)]
    actions: Vec<ActionEntry>,
}

#[derive(Clone, Debug, Deserialize)]
struct ActionEntry {
    name: String,
    version: u32,
    kind: String,
    #[serde(default)]
    scopes: Vec<String>,
    idempotent: bool,
    timeout_ms: u32,
}

#[derive(Clone, Debug, Deserialize)]
struct ActionSpec {
    name: String,
    version: u32,
    kind: String,
    #[serde(default)]
    scopes: Vec<String>,
    effect: String,
    confirm: String,
    tenancy: String,
    spec_hash: String,
    input: Value,
    output: Value,
}

#[derive(Clone, Debug, Deserialize)]
struct JobView {
    id: String,
    kind: String,
    status: String,
    attempts: i32,
    max_attempts: i32,
    #[serde(default)]
    last_error: Option<String>,
    run_after: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize)]
struct AuditView {
    id: String,
    #[serde(default)]
    actor_id: Option<String>,
    actor_kind: String,
    action: String,
    status: String,
    #[serde(default)]
    resource_type: Option<String>,
    #[serde(default)]
    resource_id: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize)]
struct LicenceView {
    app_key: String,
    status: String,
    #[serde(default)]
    expires_at: Option<DateTime<Utc>>,
    checked_at: DateTime<Utc>,
}

#[derive(Clone)]
enum TabData {
    Actions(Vec<ActionEntry>),
    Jobs(Vec<JobView>),
    Audit(Vec<AuditView>),
    Licence(LicenceView),
    External(Value),
}

fn parse<T: serde::de::DeserializeOwned>(value: Value, what: &str) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Network(format!("{what}: {error}")))
}

fn load(
    ctx: AppCtx,
    tab: RwSignal<Tab>,
    job_status: RwSignal<Option<String>>,
) -> LocalResource<Result<TabData, ApiError>> {
    LocalResource::new(move || {
        // Every reactive input is read before the first await.
        let _tenant = ctx.tenant.get();
        let selected = tab.get();
        let job_filter = job_status.get();
        let client = ctx.client();
        async move {
            match selected {
                Tab::Actions => {
                    let manifest: ActionManifest = parse(
                        client
                            .invoke("actions.list", json!({}), InvokeOptions::default())
                            .await?
                            .output,
                        "actions",
                    )?;
                    Ok(TabData::Actions(manifest.actions))
                }
                Tab::Jobs => {
                    let mut input = json!({"limit": 50});
                    if let Some(status) = job_filter {
                        input["status"] = json!(status);
                    }
                    let page: Page<JobView> = parse(
                        client
                            .invoke("jobs.list", input, InvokeOptions::default())
                            .await?
                            .output,
                        "jobs",
                    )?;
                    Ok(TabData::Jobs(page.items))
                }
                Tab::Audit => {
                    let page: Page<AuditView> = parse(
                        client
                            .invoke("audit.list", json!({"limit": 50}), InvokeOptions::default())
                            .await?
                            .output,
                        "audit",
                    )?;
                    Ok(TabData::Audit(page.items))
                }
                Tab::Licence => {
                    let licence: LicenceView = parse(
                        client
                            .invoke("licences.get", json!({}), InvokeOptions::default())
                            .await?
                            .output,
                        "licence",
                    )?;
                    Ok(TabData::Licence(licence))
                }
                Tab::Workers => Ok(TabData::External(
                    client
                        .invoke("workers.list", json!({}), InvokeOptions::default())
                        .await?
                        .output,
                )),
                Tab::Billing => Ok(TabData::External(
                    client
                        .invoke("billing.summary", json!({}), InvokeOptions::default())
                        .await?
                        .output,
                )),
            }
        }
    })
}

#[component]
pub fn AdminScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let tab = RwSignal::new(Tab::Actions);
    let job_status = RwSignal::new(Option::<String>::None);
    let data = load(ctx, tab, job_status);
    let retry = Callback::new(move |_| data.refetch());
    let detail = RwSignal::new(Option::<Result<ActionSpec, String>>::None);
    let notice = RwSignal::new(Option::<String>::None);

    let on_retry_job = Callback::new(move |job_id: String| {
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "jobs.retry",
                    json!({"id": job_id}),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => data.refetch(),
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    });

    let describe = Callback::new(move |name: String| {
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "actions.describe",
                    json!({"name": name}),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(invocation) => match parse::<ActionSpec>(invocation.output, "action") {
                    Ok(spec) => detail.set(Some(Ok(spec))),
                    Err(error) => detail.set(Some(Err(error.to_string()))),
                },
                Err(error) => detail.set(Some(Err(error.to_string()))),
            }
        });
    });

    view! {
        <section class="screen">
            {move || {
                if !ctx.has_action("actions.list") {
                    return view! {
                        <Planned
                            title="Admin is planned".to_owned()
                            detail="actions.list is not in this role's manifest; the operator surface is admin-only.".to_owned()
                        />
                    }
                    .into_any();
                }
                view! {
                    <header class="screen-head">
                        <div>
                            <h1>"Admin"</h1>
                            <p class="muted">"The registry, the worker queue, the audit trail and the entitling licence — read from the service, not remembered."</p>
                        </div>
                        <button class="btn" on:click=move |_| data.refetch()>"Refresh"</button>
                    </header>
                    <nav class="tabs" aria-label="Admin tabs">
                        {Tab::ALL.into_iter().map(|entry| {
                            view! {
                                <button
                                    class=move || if tab.get() == entry { "tab on" } else { "tab" }
                                    on:click=move |_| {
                                        tab.set(entry);
                                        detail.set(None);
                                    }
                                    disabled=move || !ctx.has_action(entry.action())
                                >{entry.label()}</button>
                            }
                        }).collect_view()}
                    </nav>
                    {move || notice.get().map(|message| view! {
                        <div class="callout warn" role="status"><p>{message}</p></div>
                    })}
                    {move || match data.get() {
                        None => view! { <p class="muted">"Reading the admin surface…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=retry/>
                        }.into_any(),
                        Some(Ok(body)) => view! {
                            <AdminBody
                                body=body
                                tab=tab
                                job_status=job_status
                                detail=detail
                                describe=describe
                                on_retry_job=on_retry_job
                            />
                        }.into_any(),
                    }}
                }
                .into_any()
            }}
        </section>
    }
}

#[component]
fn AdminBody(
    body: TabData,
    tab: RwSignal<Tab>,
    job_status: RwSignal<Option<String>>,
    detail: RwSignal<Option<Result<ActionSpec, String>>>,
    describe: Callback<String>,
    on_retry_job: Callback<String>,
) -> impl IntoView {
    match body {
        TabData::Actions(actions) => view! {
            <p class="muted">{format!("{} actions this role may invoke.", actions.len())}</p>
            <div style="overflow-x:auto">
                <table class="table">
                    <thead><tr><th>"Action"</th><th>"Kind"</th><th>"Version"</th><th>"Scopes"</th><th>"Idempotent"</th><th>"Timeout"</th></tr></thead>
                    <tbody>
                        {actions.into_iter().map(|action| {
                            let name = action.name.clone();
                            let scopes = action.scopes.join(", ");
                            view! {
                                <tr>
                                    <td class="mono">
                                        <button class="link" on:click=move |_| describe.run(name.clone())>{action.name.clone()}</button>
                                    </td>
                                    <td>{action.kind.clone()}</td>
                                    <td class="num">{action.version}</td>
                                    <td class="muted">{scopes}</td>
                                    <td>{if action.idempotent { "yes" } else { "no" }}</td>
                                    <td class="num">{format!("{} ms", action.timeout_ms)}</td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
            {move || detail.get().map(|result| match result {
                Err(error) => view! {
                    <div class="panel problem" role="alert"><p>{error}</p></div>
                }.into_any(),
                Ok(spec) => view! {
                    <section class="panel" style="margin-top:12px">
                        <div class="ph">
                            <span class="mono">{spec.name.clone()}</span>
                            <StatusPill status=spec.effect.clone()/>
                        </div>
                        <p class="muted">
                            {format!("v{} · {} · tenancy {} · confirm {} · hash {}", spec.version, spec.kind, spec.tenancy, spec.confirm, short_id(Some(&spec.spec_hash)))}
                        </p>
                        <p class="muted">{format!("scopes: {}", spec.scopes.join(", "))}</p>
                        <h3>"Input schema"</h3>
                        <pre class="code">{pretty(&spec.input)}</pre>
                        <h3>"Output schema"</h3>
                        <pre class="code">{pretty(&spec.output)}</pre>
                    </section>
                }.into_any(),
            })}
        }.into_any(),
        TabData::Jobs(jobs) => view! {
            <div class="filterbar">
                <label class="filter">
                    <span>"Status"</span>
                    <select
                        on:change=move |event| {
                            let value = event_target_value(&event);
                            job_status.set(if value.is_empty() { None } else { Some(value) });
                        }
                    >
                        <option value="">"Every status"</option>
                        {JOB_STATUSES.into_iter().map(|status| view! {
                            <option value=status>{status}</option>
                        }).collect_view()}
                    </select>
                </label>
            </div>
            {if jobs.is_empty() {
                view! { <Empty message="No job matches this filter.".to_owned()/> }.into_any()
            } else {
                view! {
                    <div style="overflow-x:auto">
                        <table class="table">
                            <thead><tr><th>"Job"</th><th>"Kind"</th><th>"Status"</th><th>"Attempts"</th><th>"Run after"</th><th>"Updated"</th><th>"Error"</th><th></th></tr></thead>
                            <tbody>
                                {jobs.into_iter().map(|job| {
                                    let retry_id = job.id.clone();
                                    let failed = job.status == "failed" || job.status == "dead";
                                    view! {
                                        <tr>
                                            <td class="mono">{short_id(Some(&job.id))}</td>
                                            <td>{job.kind.clone()}</td>
                                            <td><StatusPill status=job.status.clone()/></td>
                                            <td class="num">{format!("{}/{}", job.attempts, job.max_attempts)}</td>
                                            <td>{age(Some(job.run_after))}</td>
                                            <td>{age(Some(job.updated_at))}</td>
                                            <td class="muted">{job.last_error.clone().unwrap_or_else(|| "—".to_owned())}</td>
                                            <td>
                                                {failed.then(|| view! {
                                                    <button class="btn small" on:click=move |_| on_retry_job.run(retry_id.clone())>"Retry"</button>
                                                })}
                                            </td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                    </div>
                }.into_any()
            }}
        }.into_any(),
        TabData::Audit(events) => view! {
            {if events.is_empty() {
                view! { <Empty message="No audit event in this tenant yet.".to_owned()/> }.into_any()
            } else {
                view! {
                    <div style="overflow-x:auto">
                        <table class="table">
                            <thead><tr><th>"When"</th><th>"Action"</th><th>"Status"</th><th>"Actor"</th><th>"Resource"</th><th>"Event"</th></tr></thead>
                            <tbody>
                                {events.into_iter().map(|event| view! {
                                    <tr>
                                        <td>{age(Some(event.created_at))}</td>
                                        <td class="mono">{event.action.clone()}</td>
                                        <td><StatusPill status=event.status.clone()/></td>
                                        <td>{format!("{} {}", event.actor_kind, short_id(event.actor_id.as_deref()))}</td>
                                        <td class="mono">{format!("{} {}", event.resource_type.unwrap_or_default(), short_id(event.resource_id.as_deref()))}</td>
                                        <td class="mono">{short_id(Some(&event.id))}</td>
                                    </tr>
                                }).collect_view()}
                            </tbody>
                        </table>
                    </div>
                }.into_any()
            }}
        }.into_any(),
        TabData::Licence(licence) => view! {
            <section class="panel">
                <div class="ph"><span>"Licence"</span><StatusPill status=licence.status.clone()/></div>
                <table class="table">
                    <tbody>
                        <tr><th>"App"</th><td class="mono">{licence.app_key.clone()}</td></tr>
                        <tr><th>"Status"</th><td>{licence.status.clone()}</td></tr>
                        <tr><th>"Expires"</th><td>{licence.expires_at.map(|at| at.to_rfc3339()).unwrap_or_else(|| "no expiry".to_owned())}</td></tr>
                        <tr><th>"Checked"</th><td>{format!("{} ({})", licence.checked_at.to_rfc3339(), age(Some(licence.checked_at)))}</td></tr>
                    </tbody>
                </table>
                <p class="muted">"The synced entitlement is authoritative; this screen never writes it."</p>
            </section>
        }.into_any(),
        TabData::External(value) => view! {
            <section class="panel">
                <div class="ph"><span>{tab.get().label()}</span></div>
                <p class="muted">
                    "This tab reads an external service through the action engine. An unconfigured service is an error, not an empty list."
                </p>
                <pre class="code">{pretty(&value)}</pre>
            </section>
        }.into_any(),
    }
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn event_target_value(event: &web_sys::Event) -> String {
    event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlSelectElement>().ok())
        .map(|select| select.value())
        .unwrap_or_default()
}
