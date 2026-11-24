//! Plan — the WBS, the baselines, the registers and the change gate.
//!
//! The change control is the owning slice's `changes.request`; a change without a rationale is
//! refused by the server with `422 change_rationale_required`, and the screen shows that refusal
//! verbatim rather than deciding client-side.

use leptos::prelude::*;
use serde_json::{Value, json};

use crate::api::{ApiError, InvokeOptions};
use crate::components::{Empty, ErrorPanel, StatusPill, Tile, fmt_int};
use crate::screens::v2_data::{int, read_rows, rows, text};
use crate::state::AppCtx;

#[derive(Clone, Debug, Default)]
struct PlanData {
    projects: Vec<Value>,
}

async fn load(ctx: AppCtx) -> Result<PlanData, ApiError> {
    let client = ctx.client();
    let value = client.get("/api/projects?limit=200").await?;
    Ok(PlanData {
        projects: rows(&value),
    })
}

#[derive(Clone, Debug, Default)]
struct PlanDetail {
    wbs: Vec<Value>,
    baselines: Vec<Value>,
    registers: Vec<Value>,
}

async fn load_detail(ctx: AppCtx, project_id: String) -> Result<PlanDetail, ApiError> {
    let client = ctx.client();
    let mut detail = PlanDetail::default();
    if project_id.is_empty() {
        return Ok(detail);
    }
    if ctx.has_action("wbs.list") {
        detail.wbs = read_rows(&client, "wbs.list", json!({ "project_id": project_id })).await?;
    }
    if ctx.has_action("baselines.list") {
        detail.baselines = read_rows(
            &client,
            "baselines.list",
            json!({ "project_id": project_id }),
        )
        .await?;
    }
    if ctx.has_action("registers.list") {
        detail.registers = read_rows(
            &client,
            "registers.list",
            json!({ "project_id": project_id }),
        )
        .await?;
    }
    Ok(detail)
}

#[component]
pub fn PlanScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let data = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        async move { load(ctx).await }
    });
    let selected = RwSignal::new(Option::<String>::None);
    let detail = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let project = selected.get();
        async move {
            match project {
                None => Ok(PlanDetail::default()),
                Some(project) => load_detail(ctx, project).await,
            }
        }
    });
    let retry = Callback::new(move |_| data.refetch());
    let detail_retry = Callback::new(move |_| detail.refetch());

    view! {
        <section class="screen" aria-labelledby="plan-title">
            <header class="screen-head">
                <div>
                    <h1 id="plan-title">"Plan"</h1>
                    <p class="muted">"The work breakdown, the baselines, the registers and the change gate, from the tenant's rows."</p>
                </div>
                <button class="btn" on:click=move |_| data.refetch()>"Refresh"</button>
            </header>
            {move || match data.get() {
                None => view! { <p class="muted">"Loading…"</p> }.into_any(),
                Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                Some(Ok(data)) => view! { <PlanBody data=data selected=selected/> }.into_any(),
            }}
            {move || match detail.get() {
                None => view! { <p class="muted">"Choose a project to load its plan."</p> }.into_any(),
                Some(Err(error)) => view! { <ErrorPanel error=error on_retry=detail_retry/> }.into_any(),
                Some(Ok(detail)) => view! {
                    <PlanDetailBody detail=detail project_id=selected.get().unwrap_or_default() refresh=detail_retry/>
                }.into_any(),
            }}
        </section>
    }
}

#[component]
fn PlanBody(data: PlanData, selected: RwSignal<Option<String>>) -> impl IntoView {
    let projects = data.projects;
    view! {
        <div class="grid-3">
            <Tile label="Projects".to_owned() value=fmt_int(projects.len() as i64) detail="tenant rows".to_owned()/>
        </div>
        <h2 id="projects">"Projects"</h2>
        {if projects.is_empty() {
            view! { <Empty message="No project in this tenant yet.".to_owned()/> }.into_any()
        } else {
            view! {
                <table class="table" data-testid="plan-projects">
                    <thead><tr><th>"Key"</th><th>"Project"</th><th>"Status"</th><th></th></tr></thead>
                    <tbody>
                        {projects.into_iter().map(|row| {
                            let id = text(&row, "id");
                            let open = id.clone();
                            view! {
                                <tr>
                                    <td class="mono">{text(&row, "key")}</td>
                                    <td>{text(&row, "name")}</td>
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
fn PlanDetailBody(detail: PlanDetail, project_id: String, refresh: Callback<()>) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let wbs = detail.wbs;
    let baselines = detail.baselines;
    let registers = detail.registers;
    let baseline_options = baselines.clone();
    let baseline_id = RwSignal::new(String::new());
    let title = RwSignal::new(String::new());
    let rationale = RwSignal::new(String::new());
    let impact = RwSignal::new(String::new());
    let binding_id = RwSignal::new(String::new());
    let new_wbs_kind = RwSignal::new("deliverable".to_owned());
    let new_wbs_title = RwSignal::new(String::new());
    let new_wbs_card = RwSignal::new(String::new());
    let new_register_kind = RwSignal::new("risk".to_owned());
    let new_register_title = RwSignal::new(String::new());
    let notice = RwSignal::new(Option::<String>::None);

    let submit = {
        let project_id = project_id.clone();
        move |_| {
            let ctx = ctx;
            let project_id = project_id.clone();
            let baseline = baseline_id.get();
            let title = title.get();
            let body = rationale.get();
            let binding = binding_id.get();
            let impact = impact.get();
            leptos::task::spawn_local(async move {
                let Ok(impact) = impact.parse::<i64>() else {
                    notice.set(Some("cost impact must be a whole number".to_owned()));
                    return;
                };
                let input = json!({
                    "job_binding_id": binding,
                    "project_id": project_id,
                    "baseline_id": baseline,
                    "title": title,
                    "body": body,
                    // SPEC_V2 §5.5: the change carries a scope/schedule/cost/risk block. This
                    // form's single number is the cost delta, so it is sent under that key.
                    "impact": {"cost": impact},
                });
                match ctx
                    .invoke("changes.request", input, InvokeOptions::default())
                    .await
                {
                    Ok(invocation) => {
                        let change = text(&invocation.output, "change_id");
                        let gate = text(&invocation.output, "approval_id");
                        notice.set(Some(format!("Change {change} requested (gate {gate}).")));
                    }
                    Err(error) => notice.set(Some(error.to_string())),
                }
            });
        }
    };

    let create_wbs = {
        let project_id = project_id.clone();
        move |_| {
            let ctx = ctx;
            let project_id = project_id.clone();
            let kind = new_wbs_kind.get();
            let title = new_wbs_title.get();
            let card = new_wbs_card.get();
            let refresh = refresh;
            leptos::task::spawn_local(async move {
                // v1-p5-5: a work package names the card that executes it; the other kinds have
                // no executor, so the card field is not sent for them.
                let mut input = json!({ "project_id": project_id, "kind": kind, "title": title });
                if kind == "work_package" {
                    input["card_id"] = json!(card.trim());
                }
                match ctx
                    .invoke("wbs.create", input, InvokeOptions::default())
                    .await
                {
                    Ok(_) => {
                        notice.set(Some("WBS node created.".to_owned()));
                        refresh.run(());
                    }
                    Err(error) => notice.set(Some(error.to_string())),
                }
            });
        }
    };
    let create_register = {
        let project_id = project_id.clone();
        move |_| {
            let ctx = ctx;
            let project_id = project_id.clone();
            let kind = new_register_kind.get();
            let title = new_register_title.get();
            let refresh = refresh;
            leptos::task::spawn_local(async move {
                match ctx
                    .invoke(
                        "registers.create",
                        json!({ "project_id": project_id, "kind": kind, "title": title }),
                        InvokeOptions::default(),
                    )
                    .await
                {
                    Ok(_) => {
                        notice.set(Some("Register item created.".to_owned()));
                        refresh.run(());
                    }
                    Err(error) => notice.set(Some(error.to_string())),
                }
            });
        }
    };
    let take_baseline = {
        let project_id = project_id.clone();
        move |_| {
            let ctx = ctx;
            let project_id = project_id.clone();
            let refresh = refresh;
            leptos::task::spawn_local(async move {
                match ctx
                    .invoke(
                        "baselines.take",
                        json!({ "project_id": project_id }),
                        InvokeOptions::default(),
                    )
                    .await
                {
                    Ok(_) => {
                        notice.set(Some("Baseline taken.".to_owned()));
                        refresh.run(());
                    }
                    Err(error) => notice.set(Some(error.to_string())),
                }
            });
        }
    };

    view! {
        {if wbs.is_empty() {
            view! { <Empty message="This project has no WBS yet.".to_owned()/> }.into_any()
        } else {
            view! {
                <table class="table" data-testid="plan-wbs">
                    <thead><tr><th>"Kind"</th><th>"Title"</th><th>"Parent"</th><th>"Card"</th><th>"Status"</th></tr></thead>
                    <tbody>
                        {wbs.into_iter().map(|row| view! {
                            <tr>
                                <td>{text(&row, "kind")}</td>
                                <td>{text(&row, "title")}</td>
                                <td class="mono">{text(&row, "parent_id")}</td>
                                <td class="mono">{text(&row, "card_id")}</td>
                                <td><StatusPill status=text(&row, "status")/></td>
                            </tr>
                        }).collect_view()}
                    </tbody>
                </table>
            }.into_any()
        }}

        <div class="panel">
            <h3>"Add a WBS node"</h3>
            <label class="field">
                <span>"Kind"</span>
                <select class="input" on:change=move |event| new_wbs_kind.set(event_target_value(&event))>
                    <option value="deliverable">"deliverable"</option>
                    <option value="work_package">"work package"</option>
                    <option value="phase">"phase"</option>
                    <option value="milestone">"milestone"</option>
                </select></label>
            <label class="field">
                <span>"Title"</span>
                <input class="input" placeholder="what the node delivers"
                    on:input=move |event| new_wbs_title.set(event_target_value(&event))/></label>
            <label class="field">
                <span>"Card"</span>
                <input class="input mono" placeholder="the card a work package executes (its id)"
                    on:input=move |event| new_wbs_card.set(event_target_value(&event))/></label>
            <button class="btn" disabled=move || {
                    new_wbs_title.get().trim().is_empty()
                        || (new_wbs_kind.get() == "work_package"
                            && new_wbs_card.get().trim().is_empty())
                }
                on:click=create_wbs>"Add WBS node"</button>
        </div>

        <h2 id="baselines">"Baselines"</h2>
        <button class="btn small" on:click=take_baseline>"Take baseline"</button>
        {if baselines.is_empty() {
            view! { <Empty message="This project has no baseline yet.".to_owned()/> }.into_any()
        } else {
            view! {
                <table class="table" data-testid="plan-baselines">
                    <thead><tr><th>"Kind"</th><th>"Taken"</th><th>"Approved by"</th><th>"Taken by"</th><th>"Version"</th></tr></thead>
                    <tbody>
                        {baselines.into_iter().map(|row| view! {
                            <tr>
                                <td><StatusPill status=text(&row, "kind")/></td>
                                <td class="mono">{text(&row, "taken_at")}</td>
                                <td class="mono">{text(&row, "approved_by")}</td>
                                <td class="mono">{text(&row, "taken_by")}</td>
                                <td class="mono">{int(&row, "version").to_string()}</td>
                            </tr>
                        }).collect_view()}
                    </tbody>
                </table>
            }.into_any()
        }}

        <h2 id="registers">"Registers"</h2>
        {if registers.is_empty() {
            view! { <Empty message="This project has no register item yet.".to_owned()/> }.into_any()
        } else {
            view! {
                <table class="table" data-testid="plan-registers">
                    <thead><tr><th>"Kind"</th><th>"Code"</th><th>"Title"</th><th>"Status"</th><th>"Severity"</th></tr></thead>
                    <tbody>
                        {registers.into_iter().map(|row| view! {
                            <tr>
                                <td>{text(&row, "kind")}</td>
                                <td class="mono">{text(&row, "code")}</td>
                                <td>{text(&row, "title")}</td>
                                <td><StatusPill status=text(&row, "status")/></td>
                                <td>{text(&row, "severity")}</td>
                            </tr>
                        }).collect_view()}
                    </tbody>
                </table>
            }.into_any()
        }}

        <div class="panel">
            <h3>"Add a register item"</h3>
            <label class="field">
                <span>"Kind"</span>
                <select class="input" on:change=move |event| new_register_kind.set(event_target_value(&event))>
                    <option value="risk">"risk"</option>
                    <option value="issue">"issue"</option>
                    <option value="assumption">"assumption"</option>
                    <option value="dependency">"dependency"</option>
                </select></label>
            <label class="field">
                <span>"Title"</span>
                <input class="input" placeholder="what the register records"
                    on:input=move |event| new_register_title.set(event_target_value(&event))/></label>
            <button class="btn" disabled=move || new_register_title.get().trim().is_empty()
                on:click=create_register>"Add register item"</button>
        </div>

        <h2 id="change">"Request a change"</h2>
        <div class="panel">
            <label class="field">
                <span>"Baseline"</span>
                <select class="input" on:change=move |event| baseline_id.set(event_target_value(&event))>
                    <option value="">"Choose the approved baseline"</option>
                    {baseline_options.into_iter().map(|row| {
                        let id = text(&row, "id");
                        let label = format!("{} · {}", text(&row, "kind"), id);
                        view! { <option value=id>{label}</option> }
                    }).collect_view()}
                </select>
            </label>
            <label class="field">
                <span>"PM job binding"</span>
                <input class="input mono" placeholder="the PM's active binding id"
                    on:input=move |event| binding_id.set(event_target_value(&event))/></label>
            <label class="field">
                <span>"Title"</span>
                <input class="input" placeholder="what changes"
                    on:input=move |event| title.set(event_target_value(&event))/></label>
            <label class="field">
                <span>"Rationale"</span>
                <textarea class="input" placeholder="why this change is needed"
                    on:input=move |event| rationale.set(event_target_value(&event))></textarea></label>
            <label class="field">
                <span>"Cost impact"</span>
                <input class="input mono" placeholder="1"
                    on:input=move |event| impact.set(event_target_value(&event))/></label>
            <button class="btn" on:click=submit>"Request change"</button>
            {move || notice.get().map(|text| view! {
                <p class="notice" role="alert">{text}</p>
            })}
        </div>
    }
}
