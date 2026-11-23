//! Operations — the estate's operational read (SPEC_V2 §10.5, destination **Operations**).
//!
//! Four reads and no write. `workers.list` returns the conveyor's own durable queue (one row
//! per kind and status) and the hosts it may dispatch to; `providers.list` returns the provider
//! surfaces this tenant has configured. The Cluster panel reads `ops.pods` and `ops.deployments`
//! in the conveyor's own namespace through the worker's service account, and an unavailable
//! cluster answers with the action's own refusal — rendered here verbatim.
//!
//! Incidents are recorded by the governed `ops.incident` action (`ops.triage`, `ops.mitigate`,
//! `ops.resolve` and `ops.postmortem` follow the same lifecycle), not on this screen; runbooks,
//! on-call and the andon cord are governed actions too. This screen is the read side only.

use leptos::prelude::*;
use serde::Deserialize;
use serde_json::json;

use crate::api::{ApiError, InvokeOptions};
use crate::components::{Empty, ErrorPanel, Planned, StatusPill, Tile};
use crate::state::AppCtx;

/// The namespace the cluster sub-reads name: the conveyor's own deployment namespace
/// (`deploy_namespace: conveyor`, S14). A namespace picker is not this read's to invent.
const CONVEYOR_NAMESPACE: &str = "conveyor";

/// One `(kind, status)` bucket of the conveyor's durable queue.
#[derive(Clone, Debug, Deserialize)]
struct QueueRow {
    kind: String,
    status: String,
    count: i64,
}

/// One host the conveyor's agent may reach; the screen shows it without its credentials.
#[derive(Clone, Debug, Deserialize)]
struct HostRow {
    name: String,
    role: String,
    mechanism: String,
    enabled: bool,
}

/// `workers.list` — the queue's depth and the registered hosts.
#[derive(Clone, Debug, Deserialize)]
struct WorkersOut {
    queue: Vec<QueueRow>,
    hosts: Vec<HostRow>,
}

/// One provider surface this tenant has configured.
#[derive(Clone, Debug, Deserialize)]
struct ProviderRow {
    key: String,
    #[serde(default)]
    destination: Option<String>,
}

/// `providers.list` — the tenant's provider inventory.
#[derive(Clone, Debug, Deserialize)]
struct ProvidersOut {
    providers: Vec<ProviderRow>,
}

/// `ops.pods` / `ops.deployments` — the read's declared shape; the objects themselves are
/// returned untouched and are not projected by this screen.
#[derive(Clone, Debug, Deserialize)]
struct ListOut {
    namespace: String,
    resource: String,
    count: usize,
}

fn parse<T: serde::de::DeserializeOwned>(
    value: serde_json::Value,
    what: &str,
) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Network(format!("{what}: {error}")))
}

fn load_workers(ctx: AppCtx) -> LocalResource<Result<WorkersOut, ApiError>> {
    LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        async move {
            parse(
                ctx.invoke("workers.list", json!({}), InvokeOptions::default())
                    .await?
                    .output,
                "workers",
            )
        }
    })
}

fn load_providers(ctx: AppCtx) -> LocalResource<Result<ProvidersOut, ApiError>> {
    LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        async move {
            parse(
                ctx.invoke("providers.list", json!({}), InvokeOptions::default())
                    .await?
                    .output,
                "providers",
            )
        }
    })
}

fn load_list(ctx: AppCtx, action: &'static str) -> LocalResource<Result<ListOut, ApiError>> {
    LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        async move {
            parse(
                ctx.invoke(
                    action,
                    json!({"namespace": CONVEYOR_NAMESPACE}),
                    InvokeOptions::default(),
                )
                .await?
                .output,
                action,
            )
        }
    })
}

/// The queued tile: the sum of the queue buckets that are waiting, or `—` until the read has
/// answered (and when it failed — the panels below say why).
fn queued_count(workers: Option<Result<WorkersOut, ApiError>>) -> String {
    match workers {
        Some(Ok(workers)) => workers
            .queue
            .iter()
            .filter(|row| matches!(row.status.as_str(), "pending" | "queued"))
            .map(|row| row.count)
            .sum::<i64>()
            .to_string(),
        _ => "—".to_owned(),
    }
}

/// The running tile: the sum of the queue buckets in `running`.
fn running_count(workers: Option<Result<WorkersOut, ApiError>>) -> String {
    match workers {
        Some(Ok(workers)) => workers
            .queue
            .iter()
            .filter(|row| row.status == "running")
            .map(|row| row.count)
            .sum::<i64>()
            .to_string(),
        _ => "—".to_owned(),
    }
}

/// The hosts tile: the registry's length, enabled or not.
fn host_count(workers: Option<Result<WorkersOut, ApiError>>) -> String {
    match workers {
        Some(Ok(workers)) => workers.hosts.len().to_string(),
        _ => "—".to_owned(),
    }
}

/// The providers tile: the configured surfaces' count.
fn provider_count(providers: Option<Result<ProvidersOut, ApiError>>) -> String {
    match providers {
        Some(Ok(providers)) => providers.providers.len().to_string(),
        _ => "—".to_owned(),
    }
}

/// A provider destination, or the platform default when the tenant has registered none.
fn destination(value: &Option<String>) -> String {
    value
        .as_deref()
        .filter(|text| !text.trim().is_empty())
        .unwrap_or("platform default")
        .to_owned()
}

/// One cluster sub-read: the action's own refusal when it fails, the read's declared shape when
/// it answers. The object bodies stay untouched — no field this screen was not told is guessed.
fn cluster_read(
    action: &'static str,
    label: &'static str,
    noun: &'static str,
    read: LocalResource<Result<ListOut, ApiError>>,
) -> AnyView {
    match read.get() {
        None => view! { <p class="muted">{format!("Reading {label}…")}</p> }.into_any(),
        Some(Err(error)) => view! {
            <div class="callout warn" role="status">
                <p>{format!("{action}: {error}")}</p>
            </div>
        }
        .into_any(),
        Some(Ok(out)) if out.count == 0 => view! {
            <Empty message=format!("No {noun} in the conveyor namespace.") />
        }
        .into_any(),
        Some(Ok(out)) => view! {
            <div class="tablewrap">
                <table class="table">
                    <thead><tr>
                        <th>"Namespace"</th><th>"Resource"</th><th class="n">"Count"</th>
                    </tr></thead>
                    <tbody><tr>
                        <td class="mono">{out.namespace}</td>
                        <td>{out.resource}</td>
                        <td class="n">{out.count.to_string()}</td>
                    </tr></tbody>
                </table>
            </div>
        }
        .into_any(),
    }
}

#[component]
pub fn Operations() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let workers = load_workers(ctx);
    let providers = load_providers(ctx);
    let pods = load_list(ctx, "ops.pods");
    let deployments = load_list(ctx, "ops.deployments");

    view! {
        <section class="screen">
            {move || {
                let missing = ["workers.list", "providers.list"]
                    .into_iter()
                    .filter(|action| !ctx.has_action(action))
                    .collect::<Vec<_>>()
                    .join(" / ");
                if !missing.is_empty() {
                    return view! {
                        <Planned
                            title="Operations is planned".to_owned()
                            detail=format!("{missing} not in this role's manifest — the screen appears when the actions land, not before.")
                        />
                    }
                    .into_any();
                }
                view! {
                    <header class="screen-head">
                        <div>
                            <h1>"Operations"</h1>
                            <p class="muted">
                                "The queue, its hosts and the configured providers are this tenant's own rows; a cluster read that is unavailable says why in its own words."
                            </p>
                        </div>
                    </header>
                    {move || {
                        let queued = queued_count(workers.get());
                        let running = running_count(workers.get());
                        let hosts = host_count(workers.get());
                        let providers_value = provider_count(providers.get());
                        view! {
                            <div class="tiles">
                                <Tile label="Queued".to_owned() value=queued detail="jobs waiting in this tenant's durable queue".to_owned()/>
                                <Tile label="Running".to_owned() value=running detail="jobs the queue is executing now".to_owned()/>
                                <Tile label="Hosts".to_owned() value=hosts detail="registered hosts, enabled or not".to_owned()/>
                                <Tile label="Providers".to_owned() value=providers_value detail="provider surfaces this tenant has configured".to_owned()/>
                            </div>
                        }
                    }}
                    <h2>"Queue"</h2>
                    {move || match workers.get() {
                        None => view! { <p class="muted">"Reading the queue…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=Callback::new(move |_| workers.refetch())/>
                        }.into_any(),
                        Some(Ok(workers)) if workers.queue.is_empty() => view! {
                            <Empty message="No job in this tenant's queue — work is enqueued by a run or a job, not here.".to_owned()/>
                        }.into_any(),
                        Some(Ok(workers)) => view! {
                            <div class="tablewrap">
                                <table class="table">
                                    <thead><tr>
                                        <th>"Kind"</th><th>"Status"</th><th class="n">"Count"</th>
                                    </tr></thead>
                                    <tbody>
                                        {workers.queue.into_iter().map(|row| view! {
                                            <tr>
                                                <td><StatusPill status=row.kind/></td>
                                                <td><StatusPill status=row.status/></td>
                                                <td class="n">{row.count.to_string()}</td>
                                            </tr>
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        }.into_any(),
                    }}
                    <h2>"Hosts"</h2>
                    {move || match workers.get() {
                        None => view! { <p class="muted">"Reading hosts…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=Callback::new(move |_| workers.refetch())/>
                        }.into_any(),
                        Some(Ok(workers)) if workers.hosts.is_empty() => view! {
                            <Empty message="No host is registered — the conveyor's host registry adds one, not this screen.".to_owned()/>
                        }.into_any(),
                        Some(Ok(workers)) => view! {
                            <div class="tablewrap">
                                <table class="table">
                                    <thead><tr>
                                        <th>"Name"</th><th>"Role"</th><th>"Mechanism"</th><th>"Enabled"</th>
                                    </tr></thead>
                                    <tbody>
                                        {workers.hosts.into_iter().map(|host| view! {
                                            <tr>
                                                <td class="mono">{host.name}</td>
                                                <td>{host.role}</td>
                                                <td>{host.mechanism}</td>
                                                <td><StatusPill status=if host.enabled { "ok".to_owned() } else { "bad".to_owned() }/></td>
                                            </tr>
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        }.into_any(),
                    }}
                    <h2>"Providers"</h2>
                    {move || match providers.get() {
                        None => view! { <p class="muted">"Reading providers…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=Callback::new(move |_| providers.refetch())/>
                        }.into_any(),
                        Some(Ok(providers)) if providers.providers.is_empty() => view! {
                            <Empty message="No provider is configured — the conveyor reports the tenant's inference destination; `inference_endpoint.set` registers it, not here.".to_owned()/>
                        }.into_any(),
                        Some(Ok(providers)) => view! {
                            <div class="tablewrap">
                                <table class="table">
                                    <thead><tr>
                                        <th>"Key"</th><th>"Destination"</th>
                                    </tr></thead>
                                    <tbody>
                                        {providers.providers.into_iter().map(|provider| view! {
                                            <tr>
                                                <td class="mono">{provider.key}</td>
                                                <td>{destination(&provider.destination)}</td>
                                            </tr>
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        }.into_any(),
                    }}
                    <h2>"Cluster"</h2>
                    <p class="muted">
                        "Pods and deployments in the conveyor's own namespace (`conveyor`), read with the worker's service account. Each read returns its objects exactly as the Kubernetes API did; this panel shows the read's declared shape, not their fields — and an unavailable cluster says why in its own words."
                    </p>
                    <h3>"Pods"</h3>
                    {move || cluster_read("ops.pods", "pods", "pod", pods)}
                    <h3>"Deployments"</h3>
                    {move || cluster_read("ops.deployments", "deployments", "deployment", deployments)}
                }
                .into_any()
            }}
        </section>
    }
}
