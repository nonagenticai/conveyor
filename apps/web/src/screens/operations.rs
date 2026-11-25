//! Operations — incidents, the cluster and host reads, gated writes and the andon cord
//! (SPEC_V2 §10.5.4; T18-4, #565).
//!
//! Every read is an existing `ops.*` read and every write an existing `ops.*`/`andon.*` action.
//! A destructive write goes through the console's one gate path (`approvals::run_destructive`),
//! so an unapproved flip opens a gate and changes nothing — it never bypasses the engine.
//!
//! Measured gap, named rather than faked: there is no incident-list, runbook-list or on-call
//! read in the registry (T7 exposes the incident writes and the cluster reads only), so those
//! panels say so and the incident forms take the ids they operate on.

use leptos::prelude::*;
use serde_json::{Value, json};
use wasm_bindgen::JsCast;

use crate::api::{ApiError, InvokeOptions};
use crate::approvals::{Destructive, run_destructive};
use crate::components::{Empty, Planned, StatusPill};
use crate::screens::v2_data::{int, rows, text};
use crate::state::AppCtx;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Incidents,
    Cluster,
    Flags,
    Andon,
}

const TABS: [(Tab, &str); 4] = [
    (Tab::Incidents, "Incidents"),
    (Tab::Cluster, "Cluster & host"),
    (Tab::Flags, "Flag changes"),
    (Tab::Andon, "Andon"),
];

#[derive(Clone, Copy)]
struct Form {
    // Cluster & host
    namespace: RwSignal<String>,
    host: RwSignal<String>,
    unit: RwSignal<String>,
    // Incidents
    incident_id: RwSignal<String>,
    board_id: RwSignal<String>,
    list_id: RwSignal<String>,
    service: RwSignal<String>,
    severity: RwSignal<String>,
    title: RwSignal<String>,
    action: RwSignal<String>,
    action_input: RwSignal<String>,
    postmortem_title: RwSignal<String>,
    postmortem_body: RwSignal<String>,
    // Flags
    deployment: RwSignal<String>,
    flag: RwSignal<String>,
    value: RwSignal<String>,
    // Andon
    reason: RwSignal<String>,
}

impl Form {
    fn new() -> Self {
        Self {
            namespace: RwSignal::new("default".to_owned()),
            host: RwSignal::new(String::new()),
            unit: RwSignal::new(String::new()),
            incident_id: RwSignal::new(String::new()),
            board_id: RwSignal::new(String::new()),
            list_id: RwSignal::new(String::new()),
            service: RwSignal::new(String::new()),
            severity: RwSignal::new("sev3".to_owned()),
            title: RwSignal::new(String::new()),
            action: RwSignal::new("restart".to_owned()),
            action_input: RwSignal::new("{}".to_owned()),
            postmortem_title: RwSignal::new(String::new()),
            postmortem_body: RwSignal::new(String::new()),
            deployment: RwSignal::new(String::new()),
            flag: RwSignal::new(String::new()),
            value: RwSignal::new("true".to_owned()),
            reason: RwSignal::new(String::new()),
        }
    }
}

fn input_value(event: &web_sys::Event) -> String {
    event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|input| input.value())
        .unwrap_or_default()
}

fn area_value(event: &web_sys::Event) -> String {
    event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
        .map(|area| area.value())
        .unwrap_or_default()
}

fn error_text(error: &ApiError) -> String {
    error.to_string()
}

/// Invoke one write, surface its outcome and refetch what the panel shows.
fn run_write(
    ctx: AppCtx,
    action: &'static str,
    input: Value,
    notice: RwSignal<Option<String>>,
    done_message: &'static str,
) {
    leptos::task::spawn_local(async move {
        match ctx.invoke(action, input, InvokeOptions::default()).await {
            Ok(invocation) => {
                let created = text(&invocation.output, "id");
                let suffix = if created == "—" {
                    String::new()
                } else {
                    format!(" ({created})")
                };
                notice.set(Some(format!("{done_message}{suffix}")));
            }
            Err(error) => notice.set(Some(format!("{action} refused: {}", error_text(&error)))),
        }
    });
}

/// A JSON document input: parse or say why not.
fn parse_json(field: &str, text: &str, notice: RwSignal<Option<String>>) -> Option<Value> {
    match serde_json::from_str::<Value>(text) {
        Ok(value) if value.is_object() => Some(value),
        Ok(_) => {
            notice.set(Some(format!("{field} must be a JSON object")));
            None
        }
        Err(error) => {
            notice.set(Some(format!("{field} is not JSON: {error}")));
            None
        }
    }
}

#[component]
pub fn Operations() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let tab = RwSignal::new(Tab::Incidents);
    let notice = RwSignal::new(Option::<String>::None);
    let cluster = RwSignal::new(Option::<Result<(String, Vec<Value>), String>>::None);
    let host_status = RwSignal::new(Option::<Value>::None);
    let host_logs = RwSignal::new(Option::<String>::None);
    let andon = RwSignal::new(Option::<Value>::None);
    let form = Form::new();

    view! {
        <section class="screen" aria-labelledby="operations-title">
            {move || {
                if !ctx.has_action("ops.incident") {
                    return view! {
                        <Planned
                            title="Operations is planned".to_owned()
                            detail="ops.incident is not in this role's manifest; the screen appears when it lands.".to_owned()
                        />
                    }
                    .into_any();
                }
                view! {
                    <header class="screen-head">
                        <div>
                            <h1 id="operations-title">"Operations"</h1>
                            <p class="muted">
                                "incidents, the cluster and host reads, gated flag changes and the andon cord. Every write is an engine action; a destructive one opens its approval gate and changes nothing until that gate is approved."
                            </p>
                        </div>
                    </header>
                    {move || notice.get().map(|message| view! {
                        <p class="notice" role="alert" data-testid="operations-notice">{message}</p>
                    })}
                    <div class="filterbar" role="tablist">
                        {TABS.into_iter().map(|(entry, label)| view! {
                            <button
                                class="btn small"
                                class:primary=move || tab.get() == entry
                                role="tab"
                                aria-selected=move || (tab.get() == entry).to_string()
                                on:click=move |_| tab.set(entry)
                            >
                                {label}
                            </button>
                        }).collect_view()}
                    </div>
                    {move || match tab.get() {
                        Tab::Incidents => incidents_panel(form, ctx, notice),
                        Tab::Cluster => cluster_panel(form, ctx, notice, cluster, host_status, host_logs),
                        Tab::Flags => flags_panel(form, ctx, notice),
                        Tab::Andon => andon_panel(form, ctx, notice, andon),
                    }}
                }
                .into_any()
            }}
        </section>
    }
}

fn incidents_panel(form: Form, ctx: AppCtx, notice: RwSignal<Option<String>>) -> AnyView {
    let open = move |_event: web_sys::MouseEvent| {
        let board = form.board_id.get().trim().to_owned();
        let list = form.list_id.get().trim().to_owned();
        let service = form.service.get().trim().to_owned();
        let severity = form.severity.get();
        let title = form.title.get().trim().to_owned();
        if board.is_empty() || list.is_empty() || service.is_empty() {
            notice.set(Some(
                "An incident needs the board, its lane and the service".to_owned(),
            ));
            return;
        }
        let payload = json!({
            "board_id": board,
            "list_id": list,
            "service": service,
            "severity": severity,
            "title": (!title.is_empty()).then_some(title),
        });
        run_write(ctx, "ops.incident", payload, notice, "Incident opened");
    };

    let incident = move || form.incident_id.get().trim().to_owned();

    let triage = move |_event: web_sys::MouseEvent| {
        let id = incident();
        if id.is_empty() {
            notice.set(Some("Name the incident first".to_owned()));
            return;
        }
        run_write(
            ctx,
            "ops.triage",
            json!({
                "incident_id": id,
                "severity": form.severity.get(),
                "runbook_procedure_id": form.action.get(),
            }),
            notice,
            "Incident triaged",
        );
    };

    let mitigate = move |_event: web_sys::MouseEvent| {
        let id = incident();
        if id.is_empty() {
            notice.set(Some("Name the incident first".to_owned()));
            return;
        }
        let Some(input) = parse_json("The mitigation input", &form.action_input.get(), notice)
        else {
            return;
        };
        run_write(
            ctx,
            "ops.mitigate",
            json!({
                "incident_id": id,
                "action": form.action.get(),
                "input": input,
            }),
            notice,
            "Mitigation recorded",
        );
    };

    let resolve = move |_event: web_sys::MouseEvent| {
        let id = incident();
        if id.is_empty() {
            notice.set(Some("Name the incident first".to_owned()));
            return;
        }
        run_write(
            ctx,
            "ops.resolve",
            json!({ "incident_id": id }),
            notice,
            "Incident resolved",
        );
    };

    let postmortem = move |_event: web_sys::MouseEvent| {
        let id = incident();
        if id.is_empty() {
            notice.set(Some("Name the incident first".to_owned()));
            return;
        }
        let title = form.postmortem_title.get().trim().to_owned();
        let body = form.postmortem_body.get().trim().to_owned();
        if title.is_empty() || body.is_empty() {
            notice.set(Some(
                "A postmortem needs its title and its blameless body".to_owned(),
            ));
            return;
        }
        run_write(
            ctx,
            "ops.postmortem",
            json!({
                "incident_id": id,
                "evidence": json!({}),
                "postmortem": { "title": title, "body": body, "systemic": false },
            }),
            notice,
            "Postmortem recorded",
        );
    };

    view! {
        <div class="grid-2" style="margin-top:12px">
            <section class="panel" data-testid="operations-incidents">
                <div class="ph"><span>"Incident"</span><span class="muted">"one card, its triage, mitigation and postmortem"</span></div>
                <label class="field"><span>"Incident id"</span>
                    <input class="inp" prop:value=move || form.incident_id.get()
                        on:input=move |event| form.incident_id.set(input_value(&event))/></label>
                <div class="grid-2">
                    <label class="field"><span>"Board id"</span>
                        <input class="inp" prop:value=move || form.board_id.get()
                            on:input=move |event| form.board_id.set(input_value(&event))/></label>
                    <label class="field"><span>"Lane (list) id"</span>
                        <input class="inp" prop:value=move || form.list_id.get()
                            on:input=move |event| form.list_id.set(input_value(&event))/></label>
                </div>
                <div class="grid-2">
                    <label class="field"><span>"Service"</span>
                        <input class="inp" prop:value=move || form.service.get()
                            on:input=move |event| form.service.set(input_value(&event))/></label>
                    <label class="field"><span>"Severity"</span>
                        <select aria-label="Severity" prop:value=move || form.severity.get()
                            on:change=move |event| {
                                let value = event.target().and_then(|t| t.dyn_into::<web_sys::HtmlSelectElement>().ok()).map(|s| s.value()).unwrap_or_default();
                                form.severity.set(value);
                            }>
                            <option value="sev1">"sev1"</option>
                            <option value="sev2">"sev2"</option>
                            <option value="sev3">"sev3"</option>
                            <option value="sev4">"sev4"</option>
                        </select></label>
                </div>
                <label class="field"><span>"Title"</span>
                    <input class="inp" prop:value=move || form.title.get()
                        on:input=move |event| form.title.set(input_value(&event))/></label>
                <div class="filterbar">
                    <button class="btn primary" on:click=open>"Open the incident"</button>
                </div>
                <p class="muted">
                    "T7 exposes the incident writes; there is no incident-list, runbook-list or on-call read in the registry, so this panel takes the ids it operates on rather than inventing a list."
                </p>
            </section>
            <section class="panel">
                <div class="ph"><span>"Act on the incident"</span><span class="muted">"T7's five writes"</span></div>
                <label class="field"><span>"Runbook / procedure id (triage) or action (mitigate)"</span>
                    <input class="inp" prop:value=move || form.action.get()
                        on:input=move |event| form.action.set(input_value(&event))/></label>
                <label class="field"><span>"Mitigation input (a JSON object)"</span>
                    <textarea class="inp" rows="3" prop:value=move || form.action_input.get()
                        on:input=move |event| form.action_input.set(area_value(&event))></textarea></label>
                <div class="filterbar">
                    <button class="btn small" on:click=triage>"Triage"</button>
                    <button class="btn small" on:click=mitigate>"Mitigate"</button>
                    <button class="btn small" on:click=resolve>"Resolve"</button>
                </div>
                <label class="field"><span>"Postmortem title"</span>
                    <input class="inp" prop:value=move || form.postmortem_title.get()
                        on:input=move |event| form.postmortem_title.set(input_value(&event))/></label>
                <label class="field"><span>"Postmortem body"</span>
                    <textarea class="inp" rows="4" prop:value=move || form.postmortem_body.get()
                        on:input=move |event| form.postmortem_body.set(area_value(&event))></textarea></label>
                <button class="btn small" on:click=postmortem>"Record the postmortem"</button>
            </section>
        </div>
    }
    .into_any()
}

fn cluster_panel(
    form: Form,
    ctx: AppCtx,
    notice: RwSignal<Option<String>>,
    cluster: RwSignal<Option<Result<(String, Vec<Value>), String>>>,
    host_status: RwSignal<Option<Value>>,
    host_logs: RwSignal<Option<String>>,
) -> AnyView {
    let read_pods = move |_event: web_sys::MouseEvent| {
        let namespace = form.namespace.get().trim().to_owned();
        cluster.set(None);
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "ops.pods",
                    json!({ "namespace": namespace }),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(invocation) => {
                    cluster.set(Some(Ok(("pods".to_owned(), rows(&invocation.output)))))
                }
                Err(error) => cluster.set(Some(Err(error_text(&error)))),
            }
        });
    };
    let read_deployments = move |_event: web_sys::MouseEvent| {
        let namespace = form.namespace.get().trim().to_owned();
        cluster.set(None);
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "ops.deployments",
                    json!({ "namespace": namespace }),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(invocation) => cluster.set(Some(Ok((
                    "deployments".to_owned(),
                    rows(&invocation.output),
                )))),
                Err(error) => cluster.set(Some(Err(error_text(&error)))),
            }
        });
    };
    let read_host = move |_event: web_sys::MouseEvent| {
        let host = form.host.get().trim().to_owned();
        let unit = form.unit.get().trim().to_owned();
        host_status.set(None);
        host_logs.set(None);
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "ops.host_status",
                    json!({ "host": host, "unit": unit }),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(invocation) => host_status.set(Some(invocation.output)),
                Err(error) => notice.set(Some(format!(
                    "ops.host_status refused: {}",
                    error_text(&error)
                ))),
            }
        });
    };
    let tail_logs = move |_event: web_sys::MouseEvent| {
        let host = form.host.get().trim().to_owned();
        let unit = form.unit.get().trim().to_owned();
        host_logs.set(None);
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "ops.host_logs",
                    json!({ "host": host, "unit": unit }),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(invocation) => host_logs.set(Some(text(&invocation.output, "log"))),
                Err(error) => notice.set(Some(format!(
                    "ops.host_logs refused: {}",
                    error_text(&error)
                ))),
            }
        });
    };

    view! {
        <div class="grid-2" style="margin-top:12px">
            <section class="panel" data-testid="operations-cluster">
                <div class="ph"><span>"Cluster reads"</span><span class="muted">"the estate integration's own view"</span></div>
                <label class="field"><span>"Namespace"</span>
                    <input class="inp" prop:value=move || form.namespace.get()
                        on:input=move |event| form.namespace.set(input_value(&event))/></label>
                <div class="filterbar">
                    <button class="btn small" on:click=read_pods>"Read pods"</button>
                    <button class="btn small" on:click=read_deployments>"Read deployments"</button>
                </div>
                {move || match cluster.get() {
                    None => view! { <p class="muted">"No cluster read yet in this session."</p> }.into_any(),
                    Some(Err(error)) => view! {
                        <div class="panel problem" role="alert"><p>{error}</p></div>
                    }.into_any(),
                    Some(Ok((what, rows))) => view! {
                        <div class="tablewrap">
                            <table class="table">
                                <thead><tr><th>{what}</th><th>"Namespace"</th></tr></thead>
                                <tbody>
                                    {rows.into_iter().map(|row| {
                                        let name = row
                                            .get("metadata")
                                            .and_then(|meta| meta.get("name"))
                                            .and_then(Value::as_str)
                                            .unwrap_or("—")
                                            .to_owned();
                                        let namespace = row
                                            .get("metadata")
                                            .and_then(|meta| meta.get("namespace"))
                                            .and_then(Value::as_str)
                                            .unwrap_or("—")
                                            .to_owned();
                                        view! { <tr><td class="mono">{name}</td><td class="mono">{namespace}</td></tr> }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    }.into_any(),
                }}
                <p class="muted">
                    "The reads name a closed operation — never a URL — and answer with whatever the estate's Kubernetes integration returns. When it is not configured the action's own refusal is shown unchanged."
                </p>
            </section>
            <section class="panel">
                <div class="ph"><span>"Host unit"</span><span class="muted">"status and a bounded log tail"</span></div>
                <div class="grid-2">
                    <label class="field"><span>"Host"</span>
                        <input class="inp" prop:value=move || form.host.get()
                            on:input=move |event| form.host.set(input_value(&event))/></label>
                    <label class="field"><span>"Unit"</span>
                        <input class="inp" prop:value=move || form.unit.get()
                            on:input=move |event| form.unit.set(input_value(&event))/></label>
                </div>
                <div class="filterbar">
                    <button class="btn small" on:click=read_host>"Read the status"</button>
                    <button class="btn small" on:click=tail_logs>"Tail the logs"</button>
                </div>
                {move || host_status.get().map(|status| view! {
                    <dl class="kv" data-testid="host-status">
                        <dt>"Host"</dt><dd class="mono">{text(&status, "host")}</dd>
                        <dt>"Unit"</dt><dd class="mono">{text(&status, "unit")}</dd>
                        <dt>"Mechanism"</dt><dd>{text(&status, "mechanism")}</dd>
                        <dt>"State"</dt><dd><StatusPill status=text(&status, "state")/></dd>
                        <dt>"Detail"</dt><dd class="muted">{text(&status, "detail")}</dd>
                    </dl>
                })}
                {move || host_logs.get().map(|log| view! {
                    <pre class="log" data-testid="host-logs">{log}</pre>
                })}
            </section>
        </div>
    }
    .into_any()
}

fn flags_panel(form: Form, ctx: AppCtx, notice: RwSignal<Option<String>>) -> AnyView {
    let flip = move |_event: web_sys::MouseEvent| {
        let namespace = form.namespace.get().trim().to_owned();
        let deployment = form.deployment.get().trim().to_owned();
        let flag = form.flag.get().trim().to_owned();
        let value = form.value.get().trim().to_owned();
        if namespace.is_empty() || deployment.is_empty() || flag.is_empty() {
            notice.set(Some(
                "A flag change needs its namespace, deployment and flag".to_owned(),
            ));
            return;
        }
        notice.set(Some("Attempting the flip…".to_owned()));
        let input = json!({
            "namespace": namespace,
            "deployment": deployment,
            "flag": flag,
            "value": value,
            "dry_run": false,
        });
        run_destructive(
            ctx,
            "ops.flag_flip",
            input,
            InvokeOptions::default(),
            move |outcome| match outcome {
                Destructive::Done => {
                    notice.set(Some("The flag was flipped and read back.".to_owned()))
                }
                Destructive::Waiting { approval_id } => notice.set(Some(format!(
                    "Blocked: a gate ({}) is waiting for someone else. Nothing was written.",
                    crate::components::short_id(Some(&approval_id))
                ))),
                Destructive::Failed(error) => notice.set(Some(format!(
                    "Refused: {}. Nothing was written.",
                    error_text(&error)
                ))),
            },
            || {},
        );
    };

    view! {
        <div class="grid-2" style="margin-top:12px">
            <section class="panel" data-testid="operations-flags">
                <div class="ph"><span>"Set one environment flag"</span><span class="muted">"destructive · gated"</span></div>
                <div class="grid-2">
                    <label class="field"><span>"Namespace"</span>
                        <input class="inp" prop:value=move || form.namespace.get()
                            on:input=move |event| form.namespace.set(input_value(&event))/></label>
                    <label class="field"><span>"Deployment"</span>
                        <input class="inp" prop:value=move || form.deployment.get()
                            on:input=move |event| form.deployment.set(input_value(&event))/></label>
                </div>
                <div class="grid-2">
                    <label class="field"><span>"Flag (environment variable)"</span>
                        <input class="inp" placeholder="FEATURE_X" prop:value=move || form.flag.get()
                            on:input=move |event| form.flag.set(input_value(&event))/></label>
                    <label class="field"><span>"Value"</span>
                        <input class="inp" prop:value=move || form.value.get()
                            on:input=move |event| form.value.set(input_value(&event))/></label>
                </div>
                <button class="btn primary" on:click=flip>"Flip the flag"</button>
                <p class="muted">
                    "ops.flag_flip has effect `destructive`: the console asks the tenant's policy, requests a gate that names this deployment, this flag and this value, and invokes nothing. A person who is not the requester decides the gate on the approvals screen; only then does the retry carry it."
                </p>
            </section>
            <section class="panel">
                <div class="ph"><span>"What the gate protects"</span></div>
                <ul class="steps">
                    <li><span class="muted">"The approval target is the deployment, the flag and its value — a gate for `FEATURE_X=true` never authorises `FEATURE_X=false`."</span></li>
                    <li><span class="muted">"The request is the whole containers list, read first, so no other container is dropped."</span></li>
                    <li><span class="muted">"A refusal is the engine's own sentence; the screen never pre-empts or hides it."</span></li>
                </ul>
                <div class="ph"><span>"Not in this read"</span></div>
                <p class="muted">
                    "There is no flag-inventory or deployment-config read; the form names the workload. The read would belong to T7."
                </p>
            </section>
        </div>
    }
    .into_any()
}

fn andon_panel(
    form: Form,
    ctx: AppCtx,
    notice: RwSignal<Option<String>>,
    andon: RwSignal<Option<Value>>,
) -> AnyView {
    let engage = move |_event: web_sys::MouseEvent| {
        let reason = form.reason.get().trim().to_owned();
        if reason.is_empty() {
            notice.set(Some("The cord needs a reason".to_owned()));
            return;
        }
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "andon.engage",
                    json!({ "reason": reason }),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(invocation) => {
                    andon.set(Some(invocation.output));
                    notice.set(Some("The cord is engaged.".to_owned()));
                }
                Err(error) => notice.set(Some(format!(
                    "andon.engage refused: {}",
                    error_text(&error)
                ))),
            }
        });
    };
    let release = move |_event: web_sys::MouseEvent| {
        leptos::task::spawn_local(async move {
            match ctx
                .invoke("andon.release", json!({}), InvokeOptions::default())
                .await
            {
                Ok(invocation) => {
                    andon.set(Some(invocation.output));
                    notice.set(Some("The cord is released.".to_owned()));
                }
                Err(error) => notice.set(Some(format!(
                    "andon.release refused: {}",
                    error_text(&error)
                ))),
            }
        });
    };

    view! {
        <div class="grid-2" style="margin-top:12px">
            <section class="panel" data-testid="operations-andon">
                <div class="ph"><span>"Andon cord"</span><span class="muted">"tenant-wide stop"</span></div>
                <label class="field"><span>"Reason"</span>
                    <textarea class="inp" rows="3" prop:value=move || form.reason.get()
                        on:input=move |event| form.reason.set(area_value(&event))></textarea></label>
                <div class="filterbar">
                    <button class="btn primary" on:click=engage>"Pull the cord"</button>
                    <button class="btn" on:click=release>"Release the cord"</button>
                </div>
                {move || andon.get().map(|state| {
                    let engaged = state.get("engaged").and_then(Value::as_bool).unwrap_or(false);
                    view! {
                        <dl class="kv" data-testid="andon-state">
                            <dt>"Engaged"</dt><dd><StatusPill status=(if engaged { "engaged" } else { "released" }).to_owned()/></dd>
                            <dt>"Reason"</dt><dd>{text(&state, "reason")}</dd>
                            <dt>"Since"</dt><dd class="muted">{text(&state, "engaged_at")}</dd>
                            <dt>"Version"</dt><dd>{int(&state, "version").to_string()}</dd>
                        </dl>
                    }
                })}
                {move || andon.get().is_none().then(|| view! {
                    <Empty message="No cord state read in this session; pull or release it, and the action's own row is shown.".to_owned()/>
                })}
            </section>
            <section class="panel">
                <div class="ph"><span>"What the cord does"</span></div>
                <p class="muted">
                    "An engaged cord refuses runs.create with 409 andon-engaged and stops the worker claiming anything except the two exempt kinds. It is a per-tenant singleton: a second pull is a 409, never a re-stamp."
                </p>
            </section>
        </div>
    }
    .into_any()
}
