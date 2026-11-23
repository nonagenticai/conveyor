//! Organisation — the tenant's industries, departments, catalogue positions, policy sets and
//! decision ladder.
//!
//! T18-2 reads the owning slices' existing actions; it adds no action and no row. The write
//! verbs (create/update/release/retire) stay with their slices; this screen renders what the
//! principal's manifest lets it read, and says so when a family is absent.

use leptos::prelude::*;
use serde_json::{Value, json};

use crate::api::{ApiError, InvokeOptions};
use crate::components::{Empty, ErrorPanel, StatusPill, Tile, fmt_int};
use crate::screens::v2_data::{count, int, read_rows, text};
use crate::state::AppCtx;

/// The Organisation read: one section per action family.
#[derive(Clone, Debug, Default)]
struct OrgData {
    industries: Vec<Value>,
    departments: Vec<Value>,
    jobs: Vec<Value>,
    approval_policies: Vec<Value>,
    decision_policies: Vec<Value>,
    refinements: Vec<Value>,
    laya_configs: Vec<Value>,
    ladder: Vec<Value>,
}

async fn load(ctx: AppCtx) -> Result<OrgData, ApiError> {
    let client = ctx.client();
    let mut data = OrgData::default();
    if ctx.has_action("industries.list") {
        data.industries = read_rows(&client, "industries.list", json!({})).await?;
    }
    if ctx.has_action("departments.list") {
        data.departments = read_rows(&client, "departments.list", json!({})).await?;
    }
    if ctx.has_action("job_catalogue.list") {
        data.jobs = read_rows(&client, "job_catalogue.list", json!({})).await?;
    }
    if ctx.has_action("approval_policies.list") {
        data.approval_policies = read_rows(&client, "approval_policies.list", json!({})).await?;
    }
    if ctx.has_action("decision_policies.list") {
        data.decision_policies = read_rows(
            &client,
            "decision_policies.list",
            json!({ "page": { "limit": 50 } }),
        )
        .await?;
    }
    if ctx.has_action("refinements.list") {
        data.refinements = read_rows(
            &client,
            "refinements.list",
            json!({ "page": { "limit": 50 } }),
        )
        .await?;
    }
    if ctx.has_action("laya_configs.list") {
        data.laya_configs = read_rows(&client, "laya_configs.list", json!({})).await?;
    }
    if ctx.has_action("ladder.list") {
        data.ladder = read_rows(&client, "ladder.list", json!({ "page": { "limit": 50 } })).await?;
    }
    Ok(data)
}

#[component]
pub fn OrganisationScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let data = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        async move { load(ctx).await }
    });
    let retry = Callback::new(move |_| data.refetch());

    view! {
        <section class="screen" aria-labelledby="organisation-title">
            <header class="screen-head">
                <div>
                    <h1 id="organisation-title">"Organisation"</h1>
                    <p class="muted">"The tenant's positions, policy sets and decision ladder, from its own rows."</p>
                </div>
                <button class="btn" on:click=move |_| data.refetch()>"Refresh"</button>
            </header>
            {move || match data.get() {
                None => view! { <p class="muted">"Loading…"</p> }.into_any(),
                Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                Some(Ok(data)) => view! { <OrganisationBody data=data refresh=retry/> }.into_any(),
            }}
        </section>
    }
}

#[component]
fn OrganisationBody(data: OrgData, refresh: Callback<()>) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let industries = data.industries;
    let departments = data.departments;
    let jobs = data.jobs;
    let approval_policies = data.approval_policies;
    let decision_policies = data.decision_policies;
    let refinements = data.refinements;
    let laya_configs = data.laya_configs;
    let ladder = data.ladder;
    let department_name = {
        let departments = departments.clone();
        move |id: &str| -> String {
            departments
                .iter()
                .find(|row| text(row, "id") == id)
                .map(|row| text(row, "name"))
                .unwrap_or_else(|| "—".to_owned())
        }
    };
    let notice = RwSignal::new(Option::<String>::None);
    let dept_name = RwSignal::new(String::new());
    let dept_industry = RwSignal::new(String::new());
    let job_key = RwSignal::new(String::new());
    let job_title = RwSignal::new(String::new());
    let job_family = RwSignal::new("engineering".to_owned());
    let job_industry = RwSignal::new(String::new());
    let job_department = RwSignal::new(String::new());
    let policy_action = RwSignal::new(String::new());
    let policy_approval = RwSignal::new("required".to_owned());
    let policy_self = RwSignal::new("forbidden".to_owned());

    let create_department = move |_| {
        let ctx = ctx;
        let name = dept_name.get();
        let industry = dept_industry.get();
        let refresh = refresh;
        leptos::task::spawn_local(async move {
            let mut input = json!({ "name": name, "ordinal": 1, "is_primary": true });
            if !industry.is_empty() {
                input["industry_id"] = json!(industry);
            }
            match ctx
                .invoke("departments.create", input, InvokeOptions::default())
                .await
            {
                Ok(_) => {
                    notice.set(Some("Department created.".to_owned()));
                    refresh.run(());
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    };
    let create_position = move |_| {
        let ctx = ctx;
        let key = job_key.get();
        let title = job_title.get();
        let family = job_family.get();
        let industry = job_industry.get();
        let department = job_department.get();
        let refresh = refresh;
        leptos::task::spawn_local(async move {
            let input = json!({
                "job_key": key,
                "industry_id": industry,
                "family": family,
                "title": title,
                "definition": { "charter": title, "limits": { "max_iterations": 4 } },
                "departments": [{ "department_id": department, "ordinal": 1, "is_primary": true }],
            });
            match ctx
                .invoke("job_catalogue.create", input, InvokeOptions::default())
                .await
            {
                Ok(_) => {
                    notice.set(Some("Position created.".to_owned()));
                    refresh.run(());
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    };
    let set_policy = move |_| {
        let ctx = ctx;
        let action = policy_action.get();
        let approval = policy_approval.get();
        let self_approval = policy_self.get();
        let refresh = refresh;
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "approval_policies.set",
                    json!({ "action": action, "approval": approval, "self_approval": self_approval }),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => {
                    notice.set(Some("Approval policy set.".to_owned()));
                    refresh.run(());
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    };

    view! {
        <div class="grid-3">
            <Tile label="Industries".to_owned() value=fmt_int(industries.len() as i64) detail="tenant rows".to_owned()/>
            <Tile label="Departments".to_owned() value=fmt_int(departments.len() as i64) detail="tenant rows".to_owned()/>
            <Tile label="Positions".to_owned() value=fmt_int(jobs.len() as i64) detail="catalogue rows".to_owned()/>
        </div>

        <h2 id="departments">"Departments"</h2>
        {if departments.is_empty() {
            view! { <Empty message="No departments in this tenant yet.".to_owned()/> }.into_any()
        } else {
            let department_name = department_name.clone();
            view! {
                <table class="table" data-testid="organisation-departments">
                    <thead><tr><th>"Name"</th><th>"Parent"</th><th>"Industry"</th><th>"Primary"</th><th>"Version"</th></tr></thead>
                    <tbody>
                        {departments.iter().map(|row| {
                            let parent = text(row, "parent_id");
                            let parent = if parent == "—" { parent } else { department_name(&parent) };
                            view! {
                                <tr>
                                    <td>{text(row, "name")}</td>
                                    <td>{parent}</td>
                                    <td class="mono">{text(row, "industry_id")}</td>
                                    <td>{if row.get("is_primary").and_then(Value::as_bool).unwrap_or(false) { "yes" } else { "no" }}</td>
                                    <td class="mono">{int(row, "version").to_string()}</td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            }.into_any()
        }}

        <h2 id="industries">"Industries"</h2>
        {if industries.is_empty() {
            view! { <Empty message="No industries in this tenant yet.".to_owned()/> }.into_any()
        } else {
            view! {
                <table class="table" data-testid="organisation-industries">
                    <thead><tr><th>"Key"</th><th>"Name"</th><th>"Status"</th></tr></thead>
                    <tbody>
                        {industries.iter().map(|row| view! {
                            <tr>
                                <td class="mono">{text(row, "key")}</td>
                                <td>{text(row, "name")}</td>
                                <td><StatusPill status=text(row, "status")/></td>
                            </tr>
                        }).collect_view()}
                    </tbody>
                </table>
            }.into_any()
        }}

        <h2 id="positions">"Catalogue positions"</h2>
        {if jobs.is_empty() {
            view! { <Empty message="No catalogue positions in this tenant yet.".to_owned()/> }.into_any()
        } else {
            view! {
                <table class="table" data-testid="organisation-jobs">
                    <thead><tr><th>"Key"</th><th>"Title"</th><th>"Family"</th><th>"Status"</th><th>"Departments"</th></tr></thead>
                    <tbody>
                        {jobs.iter().map(|row| view! {
                            <tr>
                                <td class="mono">{text(row, "key")}</td>
                                <td>{text(row, "title")}</td>
                                <td>{text(row, "family")}</td>
                                <td><StatusPill status=text(row, "status")/></td>
                                <td>{fmt_int(count(row, "departments") as i64)}</td>
                            </tr>
                        }).collect_view()}
                    </tbody>
                </table>
            }.into_any()
        }}

        <h2 id="approval-policies">"Approval policies"</h2>
        {if approval_policies.is_empty() {
            view! { <Empty message="No approval policies in this tenant yet.".to_owned()/> }.into_any()
        } else {
            view! {
                <table class="table" data-testid="organisation-approval-policies">
                    <thead><tr><th>"Action"</th><th>"Approval"</th><th>"Self approval"</th></tr></thead>
                    <tbody>
                        {approval_policies.iter().map(|row| view! {
                            <tr>
                                <td class="mono">{text(row, "action")}</td>
                                <td><StatusPill status=text(row, "approval")/></td>
                                <td>{text(row, "self_approval")}</td>
                            </tr>
                        }).collect_view()}
                    </tbody>
                </table>
            }.into_any()
        }}

        <h2 id="decision-policies">"Decision policies and refinements"</h2>
        {if decision_policies.is_empty() {
            view! { <Empty message="No decision policies in this tenant yet.".to_owned()/> }.into_any()
        } else {
            view! {
                <table class="table" data-testid="organisation-decision-policies">
                    <thead><tr><th>"Kind"</th><th>"Gate"</th><th>"Threshold"</th><th>"Fail policy"</th><th>"Action"</th><th>"Version"</th></tr></thead>
                    <tbody>
                        {decision_policies.iter().map(|row| view! {
                            <tr>
                                <td>{text(row, "kind")}</td>
                                <td class="mono">{text(row, "gate")}</td>
                                <td class="mono">{row.get("threshold").map(|value| value.to_string()).unwrap_or_else(|| "—".to_owned())}</td>
                                <td>{text(row, "fail_policy")}</td>
                                <td>{text(row, "action")}</td>
                                <td class="mono">{int(row, "version").to_string()}</td>
                            </tr>
                        }).collect_view()}
                    </tbody>
                </table>
            }.into_any()
        }}
        {if refinements.is_empty() {
            view! { <Empty message="No refinements in this tenant yet.".to_owned()/> }.into_any()
        } else {
            view! {
                <table class="table" data-testid="organisation-refinements">
                    <thead><tr><th>"Policy"</th><th>"Threshold override"</th><th>"Fail action"</th><th>"Laya config"</th><th>"Version"</th></tr></thead>
                    <tbody>
                        {refinements.iter().map(|row| view! {
                            <tr>
                                <td class="mono">{text(row, "policy_id")}</td>
                                <td class="mono">{row.get("threshold_override").map(|value| value.to_string()).unwrap_or_else(|| "—".to_owned())}</td>
                                <td>{text(row, "fail_action")}</td>
                                <td class="mono">{text(row, "laya_config_id")}</td>
                                <td class="mono">{int(row, "version").to_string()}</td>
                            </tr>
                        }).collect_view()}
                    </tbody>
                </table>
            }.into_any()
        }}
        <p class="muted">
            {format!("{} laya configuration row(s) readable for this principal.", laya_configs.len())}
        </p>

        <h2 id="ladder">"Decision ladder"</h2>
        {if ladder.is_empty() {
            view! { <Empty message="No ladder decisions in this tenant yet.".to_owned()/> }.into_any()
        } else {
            view! {
                <table class="table" data-testid="organisation-ladder">
                    <thead><tr><th>"Kind"</th><th>"Rung"</th><th>"Question"</th><th>"Answer"</th></tr></thead>
                    <tbody>
                        {ladder.iter().map(|row| view! {
                            <tr>
                                <td>{text(row, "kind")}</td>
                                <td><StatusPill status=text(row, "rung")/></td>
                                <td>{text(row, "question")}</td>
                                <td>{text(row, "answer")}</td>
                            </tr>
                        }).collect_view()}
                    </tbody>
                </table>
            }.into_any()
        }}

        <h2 id="create">"Create"</h2>
        <div class="panel">
            <h3>"Department"</h3>
            <label class="field">
                <span>"Name"</span>
                <input class="input" placeholder="Delivery"
                    on:input=move |event| dept_name.set(event_target_value(&event))/></label>
            <label class="field">
                <span>"Industry"</span>
                <select class="input" on:change=move |event| dept_industry.set(event_target_value(&event))>
                    <option value="">"No industry"</option>
                    {industries.iter().map(|row| {
                        let id = text(row, "id");
                        let label = text(row, "name");
                        view! { <option value=id>{label}</option> }
                    }).collect_view()}
                </select></label>
            <button class="btn" disabled=move || dept_name.get().trim().is_empty()
                on:click=create_department>"Create department"</button>

            <h3>"Catalogue position"</h3>
            <label class="field">
                <span>"Job key"</span>
                <input class="input mono" placeholder="backend_engineer"
                    on:input=move |event| job_key.set(event_target_value(&event))/></label>
            <label class="field">
                <span>"Title"</span>
                <input class="input" placeholder="Backend engineer"
                    on:input=move |event| job_title.set(event_target_value(&event))/></label>
            <label class="field">
                <span>"Family"</span>
                <input class="input mono" placeholder="engineering"
                    on:input=move |event| job_family.set(event_target_value(&event))/></label>
            <label class="field">
                <span>"Industry"</span>
                <select class="input" on:change=move |event| job_industry.set(event_target_value(&event))>
                    <option value="">"Choose an industry"</option>
                    {industries.iter().map(|row| {
                        let id = text(row, "id");
                        let label = text(row, "name");
                        view! { <option value=id>{label}</option> }
                    }).collect_view()}
                </select></label>
            <label class="field">
                <span>"Department"</span>
                <select class="input" on:change=move |event| job_department.set(event_target_value(&event))>
                    <option value="">"Choose a department"</option>
                    {departments.iter().map(|row| {
                        let id = text(row, "id");
                        let label = text(row, "name");
                        view! { <option value=id>{label}</option> }
                    }).collect_view()}
                </select></label>
            <button class="btn"
                disabled=move || job_key.get().trim().is_empty()
                    || job_title.get().trim().is_empty()
                    || job_industry.get().is_empty()
                    || job_department.get().is_empty()
                on:click=create_position>"Create position"</button>

            <h3>"Approval policy (the authority matrix)"</h3>
            <label class="field">
                <span>"Action"</span>
                <input class="input mono" placeholder="cards.archive"
                    on:input=move |event| policy_action.set(event_target_value(&event))/></label>
            <label class="field">
                <span>"Approval"</span>
                <select class="input" on:change=move |event| policy_approval.set(event_target_value(&event))>
                    <option value="required">"required"</option>
                    <option value="waived">"waived"</option>
                </select></label>
            <label class="field">
                <span>"Self approval"</span>
                <select class="input" on:change=move |event| policy_self.set(event_target_value(&event))>
                    <option value="forbidden">"forbidden"</option>
                    <option value="allowed">"allowed"</option>
                </select></label>
            <button class="btn" disabled=move || policy_action.get().trim().is_empty()
                on:click=set_policy>"Set policy"</button>

            {move || notice.get().map(|text| view! {
                <p class="notice" role="alert">{text}</p>
            })}
        </div>
    }
}
