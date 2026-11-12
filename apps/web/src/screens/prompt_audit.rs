//! Prompt & process audit — the audit reports and never edits.
//!
//! Three rules the screen keeps visible rather than hiding:
//!
//! - **A finding is an input, not a report.** Accepting one records a decision on the finding;
//!   applying a rewrite stays an ordinary process edit a person makes elsewhere.
//! - **A high-confidence finding blocks publish until acknowledged.** The footer derives the
//!   same predicate `processes.publish` applies (`pending` + `high`), so the two never disagree.
//! - **The `actions` surface is declared, not auditable.** `ActionSpec` carries no authored
//!   description today (S16 CCR-7), so the checkbox is shown disabled with the reason instead of
//!   quietly yielding no findings.
//!
//! The cadence is the tenant's `settings.prompt_audit` document, written with `If-Match` so a
//! concurrent settings change is a 412 the screen reports, never a silent overwrite.

use chrono::{Duration, Utc};
use leptos::prelude::*;
use serde_json::{Value, json};
use wasm_bindgen::JsCast;

use crate::api::{ApiError, InvokeOptions, Page};
use crate::components::{Empty, ErrorPanel, Planned, age, short_id};
use crate::models::{
    ProcessSummary, PromptAudit, PromptAuditDetail, PromptCheck, PromptFinding, TenantView,
    UsageReport,
};
use crate::state::AppCtx;

/// The three surfaces, with the spelling the action accepts.
const SURFACES: [(&str, &str); 3] = [
    ("processes", "Process steps"),
    ("actions", "Action descriptions"),
    ("copilot", "Copilot system prompt"),
];

#[derive(Clone, Debug)]
struct AuditData {
    audits: Vec<PromptAudit>,
    next_cursor: Option<String>,
    detail: Option<PromptAuditDetail>,
    tenant: Option<TenantView>,
    processes: Vec<ProcessSummary>,
    models: Vec<String>,
}

fn parse<T: serde::de::DeserializeOwned>(value: Value, what: &str) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Network(format!("{what}: {error}")))
}

fn load(
    ctx: AppCtx,
    selected: RwSignal<Option<String>>,
) -> LocalResource<Result<AuditData, ApiError>> {
    LocalResource::new(move || async move {
        // Every reactive input is read before the first await.
        let _tenant = ctx.tenant.get();
        let chosen = selected.get();
        let client = ctx.client();

        let audits: Page<PromptAudit> = parse(
            client
                .invoke(
                    "prompt_audits.list",
                    json!({"limit": 25}),
                    InvokeOptions::default(),
                )
                .await?
                .output,
            "prompt audits",
        )?;
        // The newest audit with findings drives the detail until the operator picks another;
        // a freshly queued audit is worth showing, but not worth hiding the report behind.
        let id = chosen.or_else(|| {
            audits
                .items
                .iter()
                .find(|audit| audit.finding_count > 0)
                .or_else(|| audits.items.first())
                .map(|audit| audit.id.clone())
        });
        let detail = match id {
            Some(id) => Some(parse::<PromptAuditDetail>(
                client
                    .invoke(
                        "prompt_audits.get",
                        json!({"id": id}),
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                "prompt audit",
            )?),
            None => None,
        };

        let tenant = if ctx.has_action("tenants.get") {
            Some(parse::<TenantView>(
                client
                    .invoke("tenants.get", json!({}), InvokeOptions::default())
                    .await?
                    .output,
                "tenant",
            )?)
        } else {
            None
        };
        let processes = if ctx.has_action("processes.list") {
            let page: Page<ProcessSummary> = parse(
                client
                    .invoke(
                        "processes.list",
                        json!({"limit": 50}),
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                "processes",
            )?;
            page.items
        } else {
            Vec::new()
        };
        // The audit's target model is whatever the router serves; the ledger knows the names
        // this tenant has actually used, so the picker cannot invent one.
        let models = if ctx.has_action("inference_usage.report") {
            let until = Utc::now();
            let since = until - Duration::days(30);
            let report: UsageReport = parse(
                client
                    .invoke(
                        "inference_usage.report",
                        json!({"since": since, "until": until, "group_by": "model"}),
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                "usage by model",
            )?;
            report.rows.into_iter().map(|row| row.bucket).collect()
        } else {
            Vec::new()
        };

        Ok(AuditData {
            next_cursor: audits.next_cursor,
            audits: audits.items,
            detail,
            tenant,
            processes,
            models,
        })
    })
}

#[component]
pub fn PromptAuditScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let selected = RwSignal::new(Option::<String>::None);
    let data = load(ctx, selected);
    let retry = Callback::new(move |_| data.refetch());

    view! {
        <section class="screen measure">
            {move || {
                if !ctx.has_action("prompt_audits.list") {
                    return view! {
                        <Planned
                            title="Prompt & process audit is planned".to_owned()
                            detail="prompt_audits.list is not in this role's manifest. The screen appears when the action lands, not before.".to_owned()
                        />
                    }
                    .into_any();
                }
                view! {
                    <header class="screen-head">
                        <div>
                            <h1>"Prompt & process audit"</h1>
                            <p class="muted">{move || subtitle(&data)}</p>
                        </div>
                        <div class="filterbar">
                            <button class="btn" on:click=move |_| data.refetch()>"Refresh"</button>
                        </div>
                    </header>
                    {move || match data.get() {
                        None => view! { <p class="muted">"Reading the audits…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=retry/>
                        }.into_any(),
                        Some(Ok(body)) => view! {
                            <AuditBody data=body selected=selected refresh=retry/>
                        }.into_any(),
                    }}
                }
                .into_any()
            }}
        </section>
    }
}

fn subtitle(data: &LocalResource<Result<AuditData, ApiError>>) -> String {
    let Some(Ok(data)) = data.get() else {
        return "three triggers, one screen".to_owned();
    };
    match &data.detail {
        Some(detail) => {
            let surfaces = detail
                .audit
                .scope
                .get("surfaces")
                .and_then(Value::as_array)
                .map(|items| items.len())
                .unwrap_or(0);
            format!(
                "Audit {} · {} · {} surfaces · {} findings",
                short_id(Some(&detail.audit.id)),
                detail
                    .audit
                    .created_at
                    .map(|at| age(Some(at)))
                    .unwrap_or_else(|| "—".to_owned()),
                surfaces,
                detail.findings.len()
            )
        }
        None => "No audits yet — run one, or wait for the cadence".to_owned(),
    }
}

#[component]
fn AuditBody(
    data: AuditData,
    selected: RwSignal<Option<String>>,
    refresh: Callback<()>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let notice = RwSignal::new(Option::<String>::None);
    let run_open = RwSignal::new(false);

    let shown_id = data.detail.as_ref().map(|detail| detail.audit.id.clone());
    let findings = data
        .detail
        .as_ref()
        .map(|detail| sorted_findings(&detail.findings))
        .unwrap_or_default();
    let pending_high = findings
        .iter()
        .filter(|finding| finding.status == "open" && finding.confidence == "high")
        .count();
    let accepted = findings
        .iter()
        .filter(|finding| finding.status == "accepted")
        .count();
    let total_findings = data
        .detail
        .as_ref()
        .map(|detail| detail.audit.finding_count.max(detail.findings.len() as i64))
        .unwrap_or(0);
    let audits = data.audits.clone();
    let processes = data.processes.clone();
    let models = data.models.clone();
    let cadence_tenant = data.tenant.clone();
    let has_more = data.next_cursor.is_some();
    let has_processes = !processes.is_empty();
    let check_processes = processes.clone();

    view! {
        {move || notice.get().map(|message| view! {
            <div class="callout warn" role="status"><p>{message}</p></div>
        })}
        <div class="split">
            <div>
                {if findings.is_empty() {
                    view! {
                        <div class="panel" role="status">
                            <h2>"Nothing to report"</h2>
                            <p class="muted">
                                "This audit produced no findings. Running an audit inventories the selected surfaces and reports what is stale, duplicated or unusable."
                            </p>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        {findings.iter().map(|finding| {
                            let tone = match finding.confidence.as_str() {
                                "high" => "finding high",
                                "medium" => "finding med",
                                _ => "finding",
                            };
                            let id = finding.id.clone();
                            let accept_id = id.clone();
                            let reject_id = id.clone();
                            view! {
                                <div class=tone>
                                    <div class="fh">
                                        <span class=format!("pill {}", confidence_tone(&finding.confidence))>
                                            {finding.confidence.clone()}
                                        </span>
                                        <span class="pill">{finding.pattern.clone()}</span>
                                        <span class="ft">{finding.location.clone()}</span>
                                        <span class="muted" style="margin-left:auto">{finding.action.clone()}</span>
                                    </div>
                                    <p>{finding.why_obsolete.clone()}</p>
                                    {(!finding.evidence.is_empty() || finding.replacement.is_some()).then(|| {
                                        let replacement = finding.replacement.clone();
                                        let evidence = finding.evidence.clone();
                                        view! {
                                            <div class="diff">
                                                {(!evidence.is_empty()).then(|| view! {
                                                    <div class="del">{format!("- {evidence}")}</div>
                                                })}
                                                {replacement.clone().filter(|text| !text.is_empty()).map(|text| view! {
                                                    <div class="add">{format!("+ {text}")}</div>
                                                })}
                                            </div>
                                        }
                                    })}
                                    <div style="display:flex;gap:6px;margin-top:7px;align-items:center">
                                        {if finding.status == "open" {
                                            view! {
                                                <button
                                                    class="btn pri small"
                                                    on:click=move |_| decide(ctx, refresh, notice, accept_id.clone(), "accept")
                                                >"Accept rewrite"</button>
                                                <button
                                                    class="btn small"
                                                    on:click=move |_| decide(ctx, refresh, notice, reject_id.clone(), "reject")
                                                >"Reject"</button>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <span class="pill muted">{finding.status.clone()}</span>
                                                {finding.decided_at.map(|at| view! {
                                                    <span class="muted">{format!("decided {}", age(Some(at)))}</span>
                                                })}
                                            }.into_any()
                                        }}
                                        <span class="adv muted mono" style="margin-left:auto">
                                            {format!("finding {}", short_id(Some(&finding.id)))}
                                        </span>
                                    </div>
                                </div>
                            }
                        }).collect_view()}
                        <div style="display:flex;gap:10px;flex-wrap:wrap;font-size:11px;color:var(--ink-3)">
                            {total_findings.saturating_sub(findings.len() as i64).gt(&0).then(|| view! {
                                <span>{format!("{} more findings", total_findings - findings.len() as i64)}</span>
                            })}
                            <span>{"·"}</span>
                            <span>{format!("{accepted} accepted this audit")}</span>
                            <span>{"·"}</span>
                            <span>
                                "open high findings: "
                                <b style={if pending_high > 0 { "color:var(--wait)" } else { "color:var(--ok)" }}>
                                    {pending_high}
                                </b>
                            </span>
                            <span class="muted">
                                "(publish re-audits the draft itself — 422 process_audit_findings)"
                            </span>
                        </div>
                    }.into_any()
                }}
            </div>
            <div>
                <section class="panel">
                    <div class="ph">
                        <span>"Run an audit"</span>
                        <button class="btn small pri" on:click=move |_| run_open.update(|open| *open = !*open)>
                            {move || if run_open.get() { "Close" } else { "Run audit" }}
                        </button>
                    </div>
                    {move || run_open.get().then(|| view! {
                        <RunForm
                            processes=processes.clone()
                            models=models.clone()
                            refresh=refresh
                            selected=selected
                            notice=notice
                        />
                    })}
                </section>

                <section class="panel" style="margin-top:12px">
                    <div class="ph">
                        <span>"Audits"</span>
                        <span class="muted">{format!("{}", audits.len())}</span>
                    </div>
                    {if audits.is_empty() {
                        view! { <Empty message="No audit has run in this tenant yet.".to_owned()/> }.into_any()
                    } else {
                        view! {
                            <table class="wtbl">
                                <thead><tr>
                                    <th>"Audit"</th><th>"Status"</th><th class="n">"Findings"</th><th>"When"</th>
                                </tr></thead>
                                <tbody>
                                    {audits.iter().map(|audit| {
                                        let id = audit.id.clone();
                                        let click = id.clone();
                                        let is_shown = Some(audit.id.clone()) == shown_id;
                                        view! {
                                            <tr
                                                class={if is_shown { "sel" } else { "" }}
                                                style="cursor:pointer"
                                                on:click=move |_| selected.set(Some(click.clone()))
                                            >
                                                <td class="mono">
                                                    {short_id(Some(&id))}
                                                    {is_shown.then(|| view! { <span class="pill ok">"viewing"</span> })}
                                                </td>
                                                <td>{audit.status.clone()}</td>
                                                <td class="n">{audit.finding_count}</td>
                                                <td title=audit.created_at.map(|at| at.to_rfc3339()).unwrap_or_default()>
                                                    {audit.created_at.map(|at| age(Some(at))).unwrap_or_else(|| "—".to_owned())}
                                                </td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                            {has_more.then(|| view! {
                                <p class="muted">"Newest 25 shown."</p>
                            })}
                        }.into_any()
                    }}
                </section>

                {move || cadence_tenant.clone().map(|tenant| view! {
                    <CadencePanel tenant=tenant refresh=refresh notice=notice/>
                })}

                {has_processes.then(move || view! {
                    <section class="panel" style="margin-top:12px">
                        <div class="ph">
                            <span>"Pre-publish check"</span>
                            <span class="muted">"nothing is saved"</span>
                        </div>
                        <p class="muted" style="font-size:11px">
                            "A synchronous check against one process; a high finding would refuse the publish."
                        </p>
                        <CheckForm processes=check_processes notice=notice/>
                    </section>
                })}

                <p class="hint adv blk" style="margin-top:12px">
                    "prompt_audits.start · prompt_audits.get · prompt_audits.check · prompt_findings.decide · tenants.update_settings (If-Match)"
                </p>
            </div>
        </div>
    }
}

fn decide(
    ctx: AppCtx,
    refresh: Callback<()>,
    notice: RwSignal<Option<String>>,
    finding_id: String,
    decision: &'static str,
) {
    leptos::task::spawn_local(async move {
        match ctx
            .invoke(
                "prompt_findings.decide",
                json!({"finding_id": finding_id, "decision": decision}),
                InvokeOptions::default(),
            )
            .await
        {
            Ok(_) => refresh.run(()),
            Err(error) => notice.set(Some(error.to_string())),
        }
    });
}

fn sorted_findings(findings: &[PromptFinding]) -> Vec<PromptFinding> {
    let rank = |confidence: &str| match confidence {
        "high" => 0,
        "medium" => 1,
        _ => 2,
    };
    let mut sorted = findings.to_vec();
    // Pending first within a confidence, decided last of all: the thing still asking a question
    // is the thing the operator came for.
    sorted.sort_by_key(|finding| {
        (
            if finding.status == "open" { 0 } else { 1 },
            rank(&finding.confidence),
            finding.created_at,
        )
    });
    sorted
}

fn confidence_tone(confidence: &str) -> &'static str {
    match confidence {
        "high" => "bad",
        "medium" => "wait",
        _ => "muted",
    }
}

#[component]
fn RunForm(
    processes: Vec<ProcessSummary>,
    models: Vec<String>,
    refresh: Callback<()>,
    selected: RwSignal<Option<String>>,
    notice: RwSignal<Option<String>>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let surface_processes = RwSignal::new(true);
    let surface_copilot = RwSignal::new(true);
    let process = RwSignal::new(String::new());
    let default_model = models.first().cloned().unwrap_or_default();
    let model = RwSignal::new(default_model);
    let custom_model = RwSignal::new(String::new());
    let busy = RwSignal::new(false);

    let on_submit = move |event: web_sys::SubmitEvent| {
        event.prevent_default();
        let mut surfaces = Vec::new();
        if surface_processes.get_untracked() {
            surfaces.push("processes");
        }
        if surface_copilot.get_untracked() {
            surfaces.push("copilot");
        }
        if surfaces.is_empty() {
            notice.set(Some("Choose at least one surface to audit.".into()));
            return;
        }
        let mut process_ids = Vec::new();
        let chosen = process.get_untracked();
        if !chosen.is_empty() {
            process_ids.push(chosen);
        }
        let custom = custom_model.get_untracked().trim().to_owned();
        let target = if custom.is_empty() {
            model.get_untracked()
        } else {
            custom
        };
        if target.trim().is_empty() {
            notice.set(Some(
                "The audit calls a model: pick one from the ledger's models or type one in Advanced."
                    .into(),
            ));
            return;
        }
        let ctx = ctx;
        busy.set(true);
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "prompt_audits.start",
                    json!({
                        "scope": {"surfaces": surfaces, "process_ids": process_ids},
                        "target_model": target,
                    }),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(invocation) => {
                    if let Some(id) = invocation.output.get("id").and_then(Value::as_str) {
                        selected.set(Some(id.to_owned()));
                    }
                    refresh.run(());
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
            busy.set(false);
        });
    };

    view! {
        <form on:submit=on_submit>
            <div style="display:flex;flex-direction:column;gap:5px;font-size:12px">
                <label class="filter">
                    <input
                        type="checkbox"
                        prop:checked=move || surface_processes.get()
                        on:change=move |event| {
                            surface_processes.set(checked(event));
                        }
                    />
                    <span>"Process steps"</span>
                </label>
                <label class="filter" title="Declared, not auditable: ActionSpec carries no authored description today (S16 CCR-7)">
                    <input type="checkbox" disabled=true/>
                    <span>"Action descriptions — declared, not auditable"</span>
                </label>
                <label class="filter">
                    <input
                        type="checkbox"
                        prop:checked=move || surface_copilot.get()
                        on:change=move |event| {
                            surface_copilot.set(checked(event));
                        }
                    />
                    <span>"Copilot system prompt"</span>
                </label>
            </div>
            <label class="filter" style="margin-top:8px">
                <span>"Only this process"</span>
                <select
                    aria-label="Process scope"
                    on:change=move |event| {
                        if let Some(select) = event.target().and_then(|target| target.dyn_into::<web_sys::HtmlSelectElement>().ok()) {
                            process.set(select.value());
                        }
                    }
                >
                    <option value="" selected=move || process.get().is_empty()>"all processes"</option>
                    {processes.iter().map(|row| {
                        let value = row.id.clone();
                        let selected_value = value.clone();
                        view! {
                            <option value=value selected=move || process.get() == selected_value>
                                {format!("{} · {}", row.key, row.title)}
                            </option>
                        }
                    }).collect_view()}
                </select>
            </label>
            <label class="filter" style="margin-top:6px">
                <span>"Model"</span>
                <select
                    aria-label="Audit target model"
                    on:change=move |event| {
                        if let Some(select) = event.target().and_then(|target| target.dyn_into::<web_sys::HtmlSelectElement>().ok()) {
                            model.set(select.value());
                        }
                    }
                >
                    {models.iter().map(|name| {
                        let value = name.clone();
                        let selected_value = value.clone();
                        view! {
                            <option value=value selected=move || model.get() == selected_value>{name.clone()}</option>
                        }
                    }).collect_view()}
                    {(models.is_empty()).then(|| view! {
                        <option value="" selected=true>"no model in the ledger — type one below"</option>
                    })}
                </select>
            </label>
            <label class="filter adv" style="margin-top:6px">
                <span>"Override (free text)"</span>
                <input
                    type="text"
                    placeholder="e.g. sonnet-5-5"
                    prop:value=move || custom_model.get()
                    on:input=move |event| {
                        if let Some(input) = event.target().and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok()) {
                            custom_model.set(input.value());
                        }
                    }
                />
            </label>
            <div style="display:flex;gap:6px;margin-top:8px;align-items:center">
                <button class="btn pri small" type="submit" disabled=move || busy.get()>
                    {move || if busy.get() { "Starting…" } else { "Start audit" }}
                </button>
                <span class="muted" style="font-size:10.5px">
                    "The audit queues one model call; findings appear when the worker finishes."
                </span>
            </div>
        </form>
    }
}

#[component]
fn CadencePanel(
    tenant: TenantView,
    refresh: Callback<()>,
    notice: RwSignal<Option<String>>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let policy = tenant
        .settings
        .get("prompt_audit")
        .cloned()
        .unwrap_or(Value::Null);
    let enabled = RwSignal::new(
        policy
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    );
    let after = RwSignal::new(
        policy
            .get("after_executions")
            .and_then(Value::as_u64)
            .map(|value| value.to_string())
            .unwrap_or_default(),
    );
    let model = RwSignal::new(
        policy
            .get("target_model")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    );
    let surfaces = RwSignal::new(
        policy
            .get("surfaces")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| vec!["processes".to_owned(), "copilot".to_owned()]),
    );
    let busy = RwSignal::new(false);
    let version = tenant.version;
    let base_settings = tenant.settings.clone();

    let toggle_surface = move |name: &'static str| {
        let mut current = surfaces.get_untracked();
        if let Some(index) = current.iter().position(|value| value == name) {
            current.remove(index);
        } else {
            current.push(name.to_owned());
        }
        surfaces.set(current);
    };

    let on_save = move |event: web_sys::SubmitEvent| {
        event.prevent_default();
        let mut document = base_settings.clone();
        let mut next = serde_json::Map::new();
        next.insert("enabled".into(), json!(enabled.get_untracked()));
        let after_raw = after.get_untracked().trim().to_owned();
        if !after_raw.is_empty() {
            match after_raw.parse::<u64>() {
                Ok(value) if value > 0 => {
                    next.insert("after_executions".into(), json!(value));
                }
                _ => {
                    notice.set(Some("After-N must be a positive whole number.".into()));
                    return;
                }
            }
        }
        next.insert("surfaces".into(), json!(surfaces.get_untracked()));
        let target = model.get_untracked().trim().to_owned();
        if !target.is_empty() {
            next.insert("target_model".into(), json!(target));
        }
        if let Some(object) = document.as_object_mut() {
            object.insert("prompt_audit".into(), Value::Object(next));
        }
        let ctx = ctx;
        busy.set(true);
        leptos::task::spawn_local(async move {
            let result = ctx
                .invoke(
                    "tenants.update_settings",
                    json!({"settings": document}),
                    InvokeOptions {
                        if_match: Some(version),
                        ..Default::default()
                    },
                )
                .await;
            match result {
                Ok(_) => refresh.run(()),
                Err(ApiError::Precondition(detail)) => {
                    notice.set(Some(format!("The tenant moved: {detail} — reloaded.")));
                    refresh.run(());
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
            busy.set(false);
        });
    };

    view! {
        <section class="panel" style="margin-top:12px">
            <div class="ph">
                <span>"Cadence"</span>
                <span class="muted">{format!("tenant v{version}")}</span>
            </div>
            <form on:submit=on_save>
                <label class="filter">
                    <input
                        type="checkbox"
                        prop:checked=move || enabled.get()
                        on:change=move |event| enabled.set(checked(event))
                    />
                    <span>"Audit automatically after N executions"</span>
                </label>
                <label class="filter" style="margin-top:6px">
                    <span>"After"</span>
                    <input
                        type="number"
                        min="1"
                        placeholder="250"
                        prop:value=move || after.get()
                        on:input=move |event| {
                            if let Some(input) = event.target().and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok()) {
                                after.set(input.value());
                            }
                        }
                    />
                    <span class="muted">"captured calls since the last audit"</span>
                </label>
                <div style="display:flex;flex-direction:column;gap:4px;margin-top:6px;font-size:12px">
                    {SURFACES.iter().map(|(name, label)| {
                        let name = *name;
                        let disabled = name == "actions";
                        view! {
                            <label class="filter" title=if disabled { "Declared, not auditable (S16 CCR-7)" } else { "" }>
                                <input
                                    type="checkbox"
                                    disabled=disabled
                                    prop:checked=move || surfaces.get().iter().any(|value| value == name)
                                    on:change=move |_| if !disabled { toggle_surface(name) }
                                />
                                <span>{*label}</span>
                            </label>
                        }
                    }).collect_view()}
                </div>
                <label class="filter" style="margin-top:6px">
                    <span>"Model"</span>
                    <input
                        type="text"
                        placeholder="sonnet-5-5"
                        prop:value=move || model.get()
                        on:input=move |event| {
                            if let Some(input) = event.target().and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok()) {
                                model.set(input.value());
                            }
                        }
                    />
                </label>
                <div style="display:flex;gap:6px;margin-top:8px">
                    <button class="btn small" type="submit" disabled=move || busy.get()>"Save cadence"</button>
                </div>
            </form>
        </section>
    }
}

#[component]
fn CheckForm(processes: Vec<ProcessSummary>, notice: RwSignal<Option<String>>) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let process = RwSignal::new(
        processes
            .first()
            .map(|row| row.id.clone())
            .unwrap_or_default(),
    );
    let result = RwSignal::new(Option::<Result<PromptCheck, ApiError>>::None);
    let busy = RwSignal::new(false);

    let on_run = move |_| {
        let id = process.get_untracked();
        if id.is_empty() {
            notice.set(Some("Choose a process to check.".into()));
            return;
        }
        let ctx = ctx;
        busy.set(true);
        leptos::task::spawn_local(async move {
            let outcome = ctx
                .invoke(
                    "prompt_audits.check",
                    json!({"process_id": id}),
                    InvokeOptions::default(),
                )
                .await
                .and_then(|invocation| {
                    serde_json::from_value::<PromptCheck>(invocation.output)
                        .map_err(|error| ApiError::Network(format!("check: {error}")))
                });
            result.set(Some(outcome));
            busy.set(false);
        });
    };

    view! {
        <label class="filter" style="margin-top:6px">
            <span>"Process"</span>
            <select
                aria-label="Process to check"
                on:change=move |event| {
                    if let Some(select) = event.target().and_then(|target| target.dyn_into::<web_sys::HtmlSelectElement>().ok()) {
                        process.set(select.value());
                    }
                }
            >
                {processes.iter().map(|row| {
                    let value = row.id.clone();
                    let selected_value = value.clone();
                    view! {
                        <option value=value selected=move || process.get() == selected_value>
                            {format!("{} · {}", row.key, row.title)}
                        </option>
                    }
                }).collect_view()}
            </select>
        </label>
        <button class="btn small" style="margin-top:6px" on:click=on_run disabled=move || busy.get()>
            "Run check (nothing saved)"
        </button>
        {move || result.get().map(|outcome| match outcome {
            Err(error) => view! { <div class="callout bad"><p>{error.to_string()}</p></div> }.into_any(),
            Ok(check) => view! {
                <div style="margin-top:8px">
                    <p class="muted" style="font-size:11px">
                        {if check.blocking {
                            "Publish would be refused: at least one high finding is present.".to_owned()
                        } else {
                            "Publish would not be blocked by this check.".to_owned()
                        }}
                    </p>
                    {check.findings.iter().map(|finding| view! {
                        <div class="finding">
                            <div class="fh">
                                <span class=format!("pill {}", confidence_tone(&finding.confidence))>{finding.confidence.clone()}</span>
                                <span class="pill">{finding.pattern.clone()}</span>
                                <span class="ft">{finding.location.clone()}</span>
                            </div>
                            <p>{finding.why_obsolete.clone()}</p>
                        </div>
                    }).collect_view()}
                </div>
            }.into_any(),
        })}
    }
}

fn checked(event: web_sys::Event) -> bool {
    event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|input| input.checked())
        .unwrap_or(false)
}
