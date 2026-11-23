//! Procedures — the T16 intake, the PDDL steps, the knowledge-pack pairing and both publication
//! gates, from the tenant's own rows.
//!
//! The reviewed reference is the design pack's `proceduresLive` (`docs/design/v2.js`): a draft
//! judge (procedure) and a draft executable (process) are produced by an intake; nothing publishes
//! itself. The screen reads `procedures.list`, `processes.list`, `capabilities.list` and
//! `approvals.list`, and writes only through the owning slices' actions (`procedures.ingest_*`,
//! `validate`, `review_source`, `publish`, `processes.publish`, `approvals.request|decide`). The
//! PKO editor the pack once proposed was ruled out by the owner on 2026-11-19 (#413/PR #419);
//! SPEC_V2 §5.6's knowledge-pack pairing is what the screen shows instead.

use std::collections::HashMap;

use leptos::prelude::*;
use leptos_router::components::A;
use serde_json::{Value, json};
use wasm_bindgen::JsCast;

use crate::api::{ApiError, InvokeOptions};
use crate::components::{Empty, ErrorPanel, StatusPill, Tile, fmt_int, short_id};
use crate::screens::v2_data::{count, int, read_rows, text};
use crate::state::AppCtx;

/// The tenant's procedure workspace. `gates` carries both pending and approved publication gates;
/// a rejected gate is visible only in the approvals screen's history.
#[derive(Clone, Debug, Default)]
struct ProceduresData {
    procedures: Vec<Value>,
    processes: Vec<Value>,
    capabilities: Vec<Value>,
    gates: Vec<Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Workspace,
    Sop,
    Docs,
    Steps,
    Pairing,
    Map,
    Publish,
}

const TABS: [(Tab, &str); 7] = [
    (Tab::Workspace, "Workspace"),
    (Tab::Sop, "SOP intake"),
    (Tab::Docs, "RDF/OWL & process docs"),
    (Tab::Steps, "PDDL steps"),
    (Tab::Pairing, "Knowledge pairing"),
    (Tab::Map, "Process map"),
    (Tab::Publish, "Publish"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IntakeKind {
    Sop,
    Bpmn,
    Rdf,
}

impl IntakeKind {
    fn label(self) -> &'static str {
        match self {
            Self::Sop => "SOP",
            Self::Bpmn => "BPMN",
            Self::Rdf => "RDF/OWL",
        }
    }

    fn action(self) -> &'static str {
        match self {
            Self::Sop => "procedures.ingest_sop",
            Self::Bpmn => "procedures.ingest_process",
            Self::Rdf => "procedures.ingest_rdf",
        }
    }

    fn source_kind(self) -> &'static str {
        match self {
            Self::Sop => "text",
            Self::Bpmn => "bpmn",
            Self::Rdf => "rdf",
        }
    }
}

/// One shared form. The intake tabs reuse the same signals, so a document pasted on one tab is
/// still there when the operator switches to the other.
#[derive(Clone, Copy)]
struct IntakeForm {
    title: RwSignal<String>,
    process_key: RwSignal<String>,
    source_key: RwSignal<String>,
    url: RwSignal<String>,
    text: RwSignal<String>,
    store_id: RwSignal<String>,
}

impl IntakeForm {
    fn new() -> Self {
        Self {
            title: RwSignal::new(String::new()),
            process_key: RwSignal::new(String::new()),
            source_key: RwSignal::new(String::new()),
            url: RwSignal::new(String::new()),
            text: RwSignal::new(String::new()),
            store_id: RwSignal::new(String::new()),
        }
    }
}

async fn load(ctx: AppCtx) -> Result<ProceduresData, ApiError> {
    let client = ctx.client();
    let mut data = ProceduresData::default();
    // The page fields are flattened into each list input (`PageIn` in
    // `crates/actions/src/views.rs`); the nested `{"page": …}` shape some older screens use is
    // ignored and silently defaults to 50 rows.
    if ctx.has_action("procedures.list") {
        data.procedures = read_rows(&client, "procedures.list", json!({ "limit": 200 })).await?;
    }
    if ctx.has_action("processes.list") {
        data.processes = read_rows(&client, "processes.list", json!({ "limit": 200 })).await?;
    }
    if ctx.has_action("capabilities.list") {
        data.capabilities =
            read_rows(&client, "capabilities.list", json!({ "limit": 200 })).await?;
    }
    if ctx.has_action("approvals.list") {
        let pending = read_rows(
            &client,
            "approvals.list",
            json!({ "status": "pending", "limit": 200 }),
        )
        .await?;
        let approved = read_rows(
            &client,
            "approvals.list",
            json!({ "status": "approved", "limit": 200 }),
        )
        .await?;
        data.gates = pending.into_iter().chain(approved).collect();
    }
    Ok(data)
}

async fn load_process(ctx: AppCtx, id: &str) -> Result<Value, ApiError> {
    let invocation = ctx
        .client()
        .invoke(
            "processes.get",
            json!({ "process_id": id }),
            InvokeOptions::default(),
        )
        .await?;
    Ok(invocation.output)
}

/// The API's refusal, with the field code the issue's scenarios name. `ApiError`'s `Display`
/// carries the detail but not the code; a refusal is only useful to an operator when both show.
fn error_text(error: &ApiError) -> String {
    match error {
        ApiError::Invalid { detail, errors } if !errors.is_empty() => {
            let fields: Vec<String> = errors
                .iter()
                .map(|error| format!("{}: {} [{}]", error.field, error.detail, error.code))
                .collect();
            format!("{detail} · {}", fields.join(" · "))
        }
        other => other.to_string(),
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

fn pretty(value: Option<&Value>) -> String {
    value
        .map(|value| serde_json::to_string_pretty(value).unwrap_or_default())
        .unwrap_or_else(|| "—".to_owned())
}

fn compact(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn find_row<'a>(rows: &'a [Value], id: &str) -> Option<&'a Value> {
    rows.iter().find(|row| text(row, "id") == id)
}

/// The most advanced gate for one target: an approved gate is what a publish may name.
fn gate_status(gates: &[Value], gate: &str, target: &str) -> &'static str {
    let matching = |status: &str| {
        gates.iter().any(|row| {
            text(row, "gate") == gate
                && text(row, "target") == target
                && text(row, "status") == status
        })
    };
    if matching("approved") {
        "approved"
    } else if matching("pending") {
        "pending · a second person decides"
    } else if matching("rejected") {
        "rejected"
    } else {
        "none · request it"
    }
}

fn approved_gate(gates: &[Value], gate: &str, target: &str) -> Option<String> {
    gates
        .iter()
        .find(|row| {
            text(row, "gate") == gate
                && text(row, "target") == target
                && text(row, "status") == "approved"
        })
        .map(|row| text(row, "id"))
}

#[component]
pub fn ProceduresScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let data = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        async move { load(ctx).await }
    });
    let refresh = Callback::new(move |_| data.refetch());
    let tab = RwSignal::new(Tab::Workspace);
    let selected_process = RwSignal::new(Option::<String>::None);
    let selected_procedure = RwSignal::new(Option::<String>::None);
    let process_detail = RwSignal::new(Option::<Result<Value, ApiError>>::None);
    let detail_loading = RwSignal::new(false);
    let notice = RwSignal::new(Option::<String>::None);
    let last_ingest = RwSignal::new(Option::<(String, Value)>::None);
    let validate_result = RwSignal::new(Option::<Result<Value, String>>::None);
    let review_note = RwSignal::new(String::new());
    let form = IntakeForm::new();

    view! {
        <section class="screen" aria-labelledby="procedures-title">
            <header class="screen-head">
                <div>
                    <h1 id="procedures-title">"Procedures"</h1>
                    <p class="muted">
                        "The SOP/BPMN/RDF intake, the PDDL steps, the knowledge-pack pairing and the human publication gates."
                    </p>
                </div>
                <div class="filterbar">
                    <button class="btn" on:click=move |_| refresh.run(())>"Refresh"</button>
                </div>
            </header>
            {move || notice.get().map(|message| view! {
                <p class="notice" role="alert">{message}</p>
            })}
            {move || match data.get() {
                None => view! { <p class="muted">"Reading the tenant's procedures…"</p> }.into_any(),
                Some(Err(error)) => view! {
                    <ErrorPanel error=error on_retry=refresh/>
                }.into_any(),
                Some(Ok(data)) => view! {
                    <ProceduresBody
                        data=data
                        tab=tab
                        selected_process=selected_process
                        selected_procedure=selected_procedure
                        process_detail=process_detail
                        detail_loading=detail_loading
                        notice=notice
                        last_ingest=last_ingest
                        validate_result=validate_result
                        review_note=review_note
                        form=form
                        refresh=refresh
                    />
                }.into_any(),
            }}
        </section>
    }
}

#[component]
fn ProceduresBody(
    data: ProceduresData,
    tab: RwSignal<Tab>,
    selected_process: RwSignal<Option<String>>,
    selected_procedure: RwSignal<Option<String>>,
    process_detail: RwSignal<Option<Result<Value, ApiError>>>,
    detail_loading: RwSignal<bool>,
    notice: RwSignal<Option<String>>,
    last_ingest: RwSignal<Option<(String, Value)>>,
    validate_result: RwSignal<Option<Result<Value, String>>>,
    review_note: RwSignal<String>,
    form: IntakeForm,
    refresh: Callback<()>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let set_notice = move |message: String| notice.set(Some(message));
    let notice_reset = move || notice.set(None);

    let load_detail = move |id: String| {
        detail_loading.set(true);
        process_detail.set(None);
        leptos::task::spawn_local(async move {
            let result = load_process(ctx, &id).await;
            process_detail.set(Some(result));
            detail_loading.set(false);
        });
    };

    let submit = move |kind: IntakeKind| {
        notice_reset();
        let title = form.title.get().trim().to_owned();
        let process_key = form.process_key.get().trim().to_owned();
        let source_key = form.source_key.get().trim().to_owned();
        let store_id = form.store_id.get().trim().to_owned();
        let text = form.text.get();
        let url = form.url.get().trim().to_owned();
        if title.is_empty() || source_key.is_empty() {
            set_notice("A title and a source key are required.".to_owned());
            return;
        }
        if kind != IntakeKind::Rdf && process_key.is_empty() {
            set_notice("An SOP or BPMN conversion needs a process key.".to_owned());
            return;
        }
        if kind == IntakeKind::Rdf && store_id.is_empty() {
            set_notice("An RDF/OWL import needs the tenant's strata store id.".to_owned());
            return;
        }
        let source = if !text.trim().is_empty() {
            json!({ "kind": kind.source_kind(), "text": text })
        } else if !url.is_empty() {
            json!({ "kind": "url", "url": url })
        } else {
            set_notice("Paste the document or give a source URL.".to_owned());
            return;
        };
        let mut input = match kind {
            IntakeKind::Sop | IntakeKind::Bpmn => json!({
                "title": title,
                "process_key": process_key,
                "source_key": source_key,
            }),
            IntakeKind::Rdf => json!({
                "title": title,
                "source_key": source_key,
                "strata_store_id": store_id,
            }),
        };
        input["source"] = source;
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(kind.action(), input, InvokeOptions::default())
                .await
            {
                Ok(invocation) => {
                    let output = invocation.output;
                    last_ingest.set(Some((kind.label().to_owned(), output.clone())));
                    if let Some(id) = output.get("process_id").and_then(Value::as_str) {
                        selected_process.set(Some(id.to_owned()));
                        load_detail(id.to_owned());
                    }
                    if let Some(id) = output.get("procedure_id").and_then(Value::as_str) {
                        selected_procedure.set(Some(id.to_owned()));
                    }
                    set_notice(format!(
                        "{} draft created; nothing is published and no tool was granted.",
                        kind.label()
                    ));
                    refresh.run(());
                }
                Err(error) => set_notice(format!("Ingest refused: {}", error_text(&error))),
            }
        });
    };

    let validate = move |id: String| {
        validate_result.set(None);
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "procedures.validate",
                    json!({ "process_id": id }),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(invocation) => validate_result.set(Some(Ok(invocation.output))),
                Err(error) => validate_result.set(Some(Err(error_text(&error)))),
            }
        });
    };

    let review = move |asset: String, decision: &'static str| {
        let note = review_note.get();
        let selected = selected_process.get_untracked();
        let load_detail = load_detail;
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "procedures.review_source",
                    json!({ "asset_id": asset, "decision": decision, "note": note }),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => {
                    set_notice(
                        (if decision == "approve" {
                            "Source approved; the drafts stay drafts until their gates publish."
                        } else {
                            "Source rejected; the derived drafts stay private."
                        })
                        .to_owned(),
                    );
                    if let Some(id) = selected {
                        load_detail(id);
                    }
                    refresh.run(());
                }
                Err(error) => set_notice(format!("Source review refused: {}", error_text(&error))),
            }
        });
    };

    let request_gate = move |gate: &'static str, target: Option<String>| {
        let Some(target) = target else {
            set_notice("Select a draft first.".to_owned());
            return;
        };
        leptos::task::spawn_local(async move {
            let description = if gate == "procedures.publish" {
                "Release the converted SOP judge"
            } else {
                "Release the executable procedure"
            };
            match ctx
                .invoke(
                    "approvals.request",
                    json!({ "gate": gate, "kind": "publication", "target": target, "description": description }),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => {
                    set_notice("Gate requested; a second person decides it.".to_owned());
                    refresh.run(());
                }
                Err(error) => set_notice(format!("Gate request refused: {}", error_text(&error))),
            }
        });
    };

    let decide = move |id: String, decision: &'static str| {
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "approvals.decide",
                    json!({ "approval_id": id, "decision": decision }),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => {
                    set_notice(
                        (if decision == "approve" {
                            "Gate approved."
                        } else {
                            "Gate rejected."
                        })
                        .to_owned(),
                    );
                    refresh.run(());
                }
                Err(error) => set_notice(format!("Gate decision refused: {}", error_text(&error))),
            }
        });
    };

    let publish = move |action: &'static str, target: String, approval: Option<String>| {
        let selected = selected_process.get_untracked();
        let load_detail = load_detail;
        leptos::task::spawn_local(async move {
            let options = InvokeOptions {
                approval_id: approval,
                ..Default::default()
            };
            let input = if action == "procedures.publish" {
                json!({ "procedure_id": target })
            } else {
                json!({ "process_id": target })
            };
            match ctx.invoke(action, input, options).await {
                Ok(_) => {
                    set_notice(
                        "Published; the released version records its source and any gate."
                            .to_owned(),
                    );
                    refresh.run(());
                    if let Some(id) = selected {
                        load_detail(id);
                    }
                }
                Err(error) => set_notice(format!("Publish refused: {}", error_text(&error))),
            }
        });
    };

    view! {
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
            Tab::Workspace => {
                let procedures = data.procedures.clone();
                let processes = data.processes.clone();
                let capabilities = data.capabilities.clone();
                let drafts = procedures.iter().filter(|row| text(row, "status") == "draft").count();
                let published = procedures.iter().filter(|row| text(row, "status") == "published").count();
                let by_key: HashMap<String, String> = procedures
                    .iter()
                    .map(|row| (text(row, "key"), text(row, "id")))
                    .collect();
                view! {
                    <div class="grid-3">
                        <Tile label="Draft procedures".to_owned() value=fmt_int(drafts as i64) detail="awaiting review and the gate".to_owned()/>
                        <Tile label="Published procedures".to_owned() value=fmt_int(published as i64) detail="immutable versions".to_owned()/>
                        <Tile label="Processes".to_owned() value=fmt_int(processes.len() as i64) detail="derived and hand-authored".to_owned()/>
                    </div>
                    <div class="split">
                        <section class="panel">
                            <div class="ph">
                                <span>"Procedures"</span>
                                <span class="muted">{format!("{} rows", procedures.len())}</span>
                            </div>
                            {if procedures.is_empty() {
                                view! { <Empty message="No procedure in this tenant yet; an intake or a hand-authored draft creates one.".to_owned()/> }.into_any()
                            } else {
                                view! {
                                    <table class="table" data-testid="procedures-table">
                                        <thead><tr>
                                            <th>"Key"</th><th>"Title"</th><th>"Status"</th><th>"Version"</th>
                                            <th>"Source"</th><th>"Knowledge pack"</th><th></th>
                                        </tr></thead>
                                        <tbody>
                                            {procedures.iter().map(|row| {
                                                let id = text(row, "id");
                                                let select_id = id.clone();
                                                view! {
                                                    <tr>
                                                        <td class="mono">{text(row, "key")}</td>
                                                        <td>{text(row, "title")}</td>
                                                        <td><StatusPill status=text(row, "status")/></td>
                                                        <td class="mono">{int(row, "version").to_string()}</td>
                                                        <td class="mono">{short_id(row.get("source_asset_id").and_then(Value::as_str))}</td>
                                                        <td class="mono">{short_id(row.get("knowledge_pack_ref").and_then(Value::as_str))}</td>
                                                        <td>
                                                            <button class="btn small" on:click=move |_| {
                                                                selected_procedure.set(Some(select_id.clone()));
                                                                tab.set(Tab::Publish);
                                                            }>"Select"</button>
                                                        </td>
                                                    </tr>
                                                }
                                            }).collect_view()}
                                        </tbody>
                                    </table>
                                }.into_any()
                            }}
                            <p class="hint">
                                "A converted SOP produces a draft judge (procedure) and a draft process; nothing publishes itself."
                            </p>
                        </section>
                        <section class="panel">
                            <div class="ph">
                                <span>"Processes"</span>
                                <span class="muted">{format!("{} rows", processes.len())}</span>
                            </div>
                            {if processes.is_empty() {
                                view! { <Empty message="No process in this tenant yet; an intake or a draft creates one.".to_owned()/> }.into_any()
                            } else {
                                view! {
                                    <table class="table" data-testid="procedures-processes">
                                        <thead><tr>
                                            <th>"Key"</th><th>"Status"</th><th>"Version"</th>
                                            <th>"Steps"</th><th>"Converter"</th><th>"Knowledge pack"</th><th></th>
                                        </tr></thead>
                                        <tbody>
                                            {processes.iter().map(|row| {
                                                let id = text(row, "id");
                                                let key = text(row, "key");
                                                let procedure = by_key.get(&key).cloned();
                                                let select_id = id.clone();
                                                view! {
                                                    <tr>
                                                        <td class="mono">{text(row, "key")}</td>
                                                        <td><StatusPill status=text(row, "status")/></td>
                                                        <td class="mono">{int(row, "version").to_string()}</td>
                                                        <td class="mono">{count(row, "steps").to_string()}</td>
                                                        <td class="mono">{short_id(row.get("converter_version").and_then(Value::as_str))}</td>
                                                        <td class="mono">{short_id(row.get("knowledge_pack_ref").and_then(Value::as_str))}</td>
                                                        <td>
                                                            <button class="btn small" on:click=move |_| {
                                                                selected_process.set(Some(select_id.clone()));
                                                                if let Some(procedure) = procedure.clone() {
                                                                    selected_procedure.set(Some(procedure));
                                                                }
                                                                load_detail(select_id.clone());
                                                                tab.set(Tab::Steps);
                                                            }>"Select"</button>
                                                        </td>
                                                    </tr>
                                                }
                                            }).collect_view()}
                                        </tbody>
                                    </table>
                                }.into_any()
                            }}
                            <p class="hint">
                                "Select a process to read its steps, its source and its gates."
                            </p>
                        </section>
                    </div>
                    <section class="panel">
                        <div class="ph">
                            <span>"Capabilities"</span>
                            <span class="muted">{format!("{} rows", capabilities.len())}</span>
                        </div>
                        {if capabilities.is_empty() {
                            view! { <Empty message="No capability in this tenant yet; the T16 ingest drafts one with its procedure.".to_owned()/> }.into_any()
                        } else {
                            view! {
                                <table class="table" data-testid="procedures-capabilities">
                                    <thead><tr><th>"Code"</th><th>"Label"</th><th>"Status"</th><th>"Version"</th></tr></thead>
                                    <tbody>
                                        {capabilities.iter().map(|row| view! {
                                            <tr>
                                                <td class="mono">{text(row, "code")}</td>
                                                <td>{text(row, "label")}</td>
                                                <td><StatusPill status=text(row, "status")/></td>
                                                <td class="mono">{int(row, "version").to_string()}</td>
                                            </tr>
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            }.into_any()
                        }}
                        <p class="hint">
                            "Capabilities are created by the T16 ingest; capabilities.design and capabilities.publish are not registered in this tenant's manifest (owner: T16)."
                        </p>
                    </section>
                }.into_any()
            }
            Tab::Sop => intake_panel(IntakeKind::Sop, form, last_ingest, submit),
            Tab::Docs => {
                view! {
                    <div class="split">
                        {intake_panel(IntakeKind::Bpmn, form, last_ingest, submit)}
                        {intake_panel(IntakeKind::Rdf, form, last_ingest, submit)}
                    </div>
                    <section class="panel">
                        <div class="ph"><span>"Ingestion paths"</span></div>
                        <table class="table">
                            <thead><tr><th>"Input"</th><th>"Conversion"</th><th>"Produces"</th><th>"Gate"</th></tr></thead>
                            <tbody>
                                <tr><td>"SOP (PDF, text, URL)"</td><td>"prose → PDDL steps"</td><td>"draft judge + draft process + capability/agent/job drafts"</td><td>"source review · human publication gate"</td></tr>
                                <tr><td>"BPMN 2.0 XML"</td><td>"activities → steps; gateways → gate steps"</td><td>"PDDL procedure draft + draft process"</td><td>"source review · human publication gate"</td></tr>
                                <tr><td>"RDF / OWL / Turtle"</td><td>"submitted to strata_ingest"</td><td>"a strata knowledge pack the drafts name"</td><td>"the import job + review"</td></tr>
                            </tbody>
                        </table>
                        <p class="hint">
                            "The conveyor runs no RDF store and no reasoner: an ontology becomes a strata knowledge pack, and a procedure pins the pack by reference."
                        </p>
                    </section>
                }.into_any()
            }
            Tab::Steps => steps_panel(selected_process, process_detail, detail_loading, validate_result, validate, load_detail),
            Tab::Pairing => pairing_panel(data.procedures.clone(), selected_process, selected_procedure, process_detail, detail_loading, review_note, review, load_detail),
            Tab::Map => map_panel(selected_process, process_detail),
            Tab::Publish => publish_panel(data.clone(), selected_process, selected_procedure, process_detail, ctx, request_gate, decide, publish),
        }}
    }
}

/// The intake form plus its own result panel. The result only renders on the tab that produced it.
fn intake_panel<F>(
    kind: IntakeKind,
    form: IntakeForm,
    last: RwSignal<Option<(String, Value)>>,
    submit: F,
) -> AnyView
where
    F: Fn(IntakeKind) + Copy + 'static,
{
    let needs_process = kind != IntakeKind::Rdf;
    view! {
        <section class="panel">
            <div class="ph">
                <span>{format!("{} intake", kind.label())}</span>
                <span class="muted">"one document at a time"</span>
            </div>
            <label class="field"><span>"Title"</span>
                <input class="inp" prop:value=move || form.title.get()
                    on:input=move |event| form.title.set(input_value(&event))/></label>
            {needs_process.then(|| view! {
                <label class="field"><span>"Process key"</span>
                    <input class="inp" prop:value=move || form.process_key.get()
                        on:input=move |event| form.process_key.set(input_value(&event))/></label>
            })}
            <label class="field"><span>"Source key"</span>
                <input class="inp" prop:value=move || form.source_key.get()
                    on:input=move |event| form.source_key.set(input_value(&event))/></label>
            {(!needs_process).then(|| view! {
                <label class="field"><span>"Strata store id"</span>
                    <input class="inp" prop:value=move || form.store_id.get()
                        on:input=move |event| form.store_id.set(input_value(&event))/></label>
            })}
            <label class="field"><span>"Source URL"</span>
                <input class="inp" placeholder="https://…" prop:value=move || form.url.get()
                    on:input=move |event| form.url.set(input_value(&event))/></label>
            <label class="field"><span>"Or paste the document"</span>
                <textarea class="inp" rows="4" placeholder="Paste the SOP text, the BPMN XML or the Turtle source"
                    prop:value=move || form.text.get()
                    on:input=move |event| form.text.set(area_value(&event))></textarea></label>
            <button class="btn primary" on:click=move |_| submit(kind)>
                {format!("Ingest the {}", kind.label())}
            </button>
            {ingest_result(last, kind.label())}
        </section>
    }
    .into_any()
}

fn ingest_result(last: RwSignal<Option<(String, Value)>>, label: &'static str) -> AnyView {
    match last.get() {
        Some((produced, output)) if produced == label => {
            let rows: Vec<(String, String)> = output
                .as_object()
                .map(|object| {
                    object
                        .iter()
                        .map(|(key, value)| (key.clone(), compact(value)))
                        .collect()
                })
                .unwrap_or_default();
            view! {
                <div class="callout" style="margin-top:10px">
                    <div class="hd">{format!("{label} result · draft")}</div>
                    <div class="kv">
                        {rows.into_iter().map(|(key, value)| view! {
                            <dt class="mono">{key}</dt><dd class="mono">{value}</dd>
                        }).collect_view()}
                    </div>
                    <p class="hint">
                        "Every derived artefact is a draft: nothing is published and no tool grant was requested."
                    </p>
                </div>
            }
            .into_any()
        }
        _ => view! {
            <p class="hint">
                "No result yet. An ingest answers the source asset, the draft judge and the draft process."
            </p>
        }
        .into_any(),
    }
}

fn steps_panel(
    selected_process: RwSignal<Option<String>>,
    process_detail: RwSignal<Option<Result<Value, ApiError>>>,
    detail_loading: RwSignal<bool>,
    validate_result: RwSignal<Option<Result<Value, String>>>,
    validate: impl Fn(String) + Copy + 'static,
    load_detail: impl Fn(String) + Copy + 'static,
) -> AnyView {
    let Some(process_id) = selected_process.get() else {
        return view! {
            <p class="muted">"Select a draft process in Workspace first."</p>
        }
        .into_any();
    };
    match process_detail.get() {
        None if detail_loading.get() => {
            view! { <p class="muted">"Reading the process…"</p> }.into_any()
        }
        None => {
            let retry = Callback::new(move |_| load_detail(process_id.clone()));
            view! {
                <section class="panel">
                    <p class="muted">"No step has been read yet."</p>
                    <button class="btn" on:click=move |_| retry.run(())>"Read the process"</button>
                </section>
            }
            .into_any()
        }
        Some(Err(error)) => {
            let retry = Callback::new(move |_| load_detail(process_id.clone()));
            view! { <ErrorPanel error=error on_retry=retry/> }.into_any()
        }
        Some(Ok(process)) => {
            let steps = process
                .get("steps")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let id = text(&process, "id");
            let title = format!("PDDL steps · {}", text(&process, "key"));
            let version = format!(
                "{} · v{}",
                text(&process, "status"),
                int(&process, "version")
            );
            let validate_id = id.clone();
            view! {
                <section class="panel">
                    <div class="ph"><span>{title}</span><span class="muted">{version}</span></div>
                    <p class="muted">{text(&process, "title")}</p>
                    {if steps.is_empty() {
                        view! { <Empty message="This draft carries no steps.".to_owned()/> }.into_any()
                    } else {
                        view! {
                            <table class="table" data-testid="procedures-steps">
                                <thead><tr>
                                    <th>"#"</th><th>"Kind"</th><th>"Actor"</th><th>"Action"</th>
                                    <th>"Action text"</th><th>"Preconditions"</th><th>"Effects"</th><th>"Judge"</th>
                                </tr></thead>
                                <tbody>
                                    {steps.iter().map(|step| view! {
                                        <tr>
                                            <td class="mono">{int(step, "position").to_string()}</td>
                                            <td>{text(step, "kind")}</td>
                                            <td>{text(step, "actor")}</td>
                                            <td class="mono">{text(step, "action")}</td>
                                            <td class="mono">{text(step, "action_text")}</td>
                                            <td class="mono">{pretty(step.get("pddl_preconditions"))}</td>
                                            <td class="mono">{pretty(step.get("pddl_effects"))}</td>
                                            <td class="mono">{short_id(step.get("procedure_id").and_then(Value::as_str))}</td>
                                        </tr>
                                    }).collect_view()}
                                </tbody>
                            </table>
                        }.into_any()
                    }}
                    <div class="filterbar">
                        <button class="btn" on:click=move |_| validate(validate_id.clone())>"Validate the draft"</button>
                        <A href=format!("/build/processes/{}", id)><span class="btn small">"Open the process editor"</span></A>
                    </div>
                    {validate_panel(validate_result)}
                    <p class="hint">
                        "A step is an action (PDDL), a tool call or a gate. Validation checks the literals, each step's judge and that no tool grant was requested."
                    </p>
                </section>
            }
            .into_any()
        }
    }
}

fn validate_panel(result: RwSignal<Option<Result<Value, String>>>) -> AnyView {
    match result.get() {
        None => view! {
            <p class="hint">"No validation yet. The server parses the draft's PDDL; it does not pre-judge it here."</p>
        }
        .into_any(),
        Some(Err(message)) => view! {
            <div class="callout bad" role="alert">
                <div class="hd">"Validation refused"</div>
                <p class="mono">{message}</p>
            </div>
        }
        .into_any(),
        Some(Ok(output)) => {
            let valid = output
                .get("valid")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let checks: Vec<String> = output
                .get("checks")
                .and_then(Value::as_array)
                .map(|checks| {
                    checks
                        .iter()
                        .map(|check| check.as_str().unwrap_or_default().to_owned())
                        .collect()
                })
                .unwrap_or_default();
            view! {
                <div class=if valid { "callout" } else { "callout bad" } role="alert">
                    <div class="hd">
                        {if valid { "Every check passes; the human gate still decides." } else { "The draft is not valid." }}
                    </div>
                    {if checks.is_empty() {
                        view! { <p class="muted">"No checks were returned."</p> }.into_any()
                    } else {
                        view! {
                            <ul>
                                {checks.into_iter().map(|check| view! { <li class="mono">{check}</li> }).collect_view()}
                            </ul>
                        }.into_any()
                    }}
                </div>
            }
            .into_any()
        }
    }
}

fn pairing_panel(
    procedures: Vec<Value>,
    selected_process: RwSignal<Option<String>>,
    selected_procedure: RwSignal<Option<String>>,
    process_detail: RwSignal<Option<Result<Value, ApiError>>>,
    detail_loading: RwSignal<bool>,
    review_note: RwSignal<String>,
    review: impl Fn(String, &'static str) + Copy + 'static,
    load_detail: impl Fn(String) + Copy + 'static,
) -> AnyView {
    let judge = selected_procedure
        .get()
        .and_then(|id| find_row(&procedures, &id).cloned());
    match process_detail.get() {
        None if detail_loading.get() => {
            view! { <p class="muted">"Reading the process…"</p> }.into_any()
        }
        None => view! {
            <p class="muted">"Select a draft process in Workspace first."</p>
        }
        .into_any(),
        Some(Err(error)) => {
            let retry_id = selected_process.get();
            let retry = Callback::new(move |_| {
                if let Some(id) = retry_id.clone() {
                    load_detail(id);
                }
            });
            view! { <ErrorPanel error=error on_retry=retry/> }.into_any()
        }
        Some(Ok(process)) => {
            let asset = process
                .get("source_asset_id")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let has_asset = asset.is_some();
            let asset_label = asset.clone().unwrap_or_else(|| "—".to_owned());
            let review_asset = asset.clone();
            view! {
                <section class="panel">
                    <div class="ph"><span>"Procedure ↔ knowledge pairing"</span></div>
                    <div class="kv">
                        <dt>"Source asset"</dt><dd class="mono">{asset_label}</dd>
                        <dt>"Knowledge pack"</dt><dd class="mono">{text(&process, "knowledge_pack_ref")}</dd>
                        <dt>"Converter"</dt><dd class="mono">{text(&process, "converter_version")}</dd>
                        <dt>"Recorded approval"</dt><dd class="mono">{text(&process, "approval_id")}</dd>
                        <dt>"Judge procedure"</dt><dd class="mono">{judge.as_ref().map(|row| text(row, "id")).unwrap_or_else(|| "—".to_owned())}</dd>
                    </div>
                    {has_asset.then(|| view! {
                        <label class="field"><span>"Review note"</span>
                            <input class="inp" placeholder="Why the source is accepted or refused"
                                prop:value=move || review_note.get()
                                on:input=move |event| review_note.set(input_value(&event))/></label>
                        <div class="filterbar">
                            <button class="btn small" on:click=move |_| {
                                if let Some(asset) = review_asset.clone() { review(asset, "approve"); }
                            }>"Approve the source"</button>
                            <button class="btn small danger" on:click=move |_| {
                                if let Some(asset) = review_asset.clone() { review(asset, "reject"); }
                            }>"Reject the source"</button>
                        </div>
                    })}
                    <p class="hint">
                        "An RDF/OWL document is imported into strata_ingest and its job id is the knowledge pack a converted draft names. A source decision unblocks the derived drafts; it never publishes them."
                    </p>
                </section>
            }
            .into_any()
        }
    }
}

fn map_panel(
    selected_process: RwSignal<Option<String>>,
    process_detail: RwSignal<Option<Result<Value, ApiError>>>,
) -> AnyView {
    let _ = selected_process;
    view! {
        <section class="panel">
            <div class="ph"><span>"Process map"</span></div>
            <p class="muted">
                "Intent (card) → Activities (steps) → Gateways (gate steps) → Evidence (run + artefact)."
            </p>
            {match process_detail.get() {
                Some(Ok(process)) => view! {
                    <div class="kv">
                        <dt>"Selected process"</dt><dd class="mono">{text(&process, "key")}</dd>
                        <dt>"Steps"</dt><dd class="mono">{count(&process, "steps").to_string()}</dd>
                        <dt>"Status"</dt><dd><StatusPill status=text(&process, "status")/></dd>
                    </div>
                }.into_any(),
                _ => view! { <p class="muted">"Select a process in Workspace."</p> }.into_any(),
            }}
            <A href="/build/process-map"><span class="btn small">"Open the process map"</span></A>
            <p class="hint">
                "A BPMN diagram becomes a procedure draft: activities → steps, gateways → gate steps. The diagram is a source document, not the executable artefact."
            </p>
        </section>
    }
    .into_any()
}

fn publish_panel(
    data: ProceduresData,
    selected_process: RwSignal<Option<String>>,
    selected_procedure: RwSignal<Option<String>>,
    process_detail: RwSignal<Option<Result<Value, ApiError>>>,
    ctx: AppCtx,
    request_gate: impl Fn(&'static str, Option<String>) + Copy + 'static,
    decide: impl Fn(String, &'static str) + Copy + 'static,
    publish: impl Fn(&'static str, String, Option<String>) + Copy + 'static,
) -> AnyView {
    let procedure_id = selected_procedure.get();
    let process_id = selected_process.get();
    let procedure = procedure_id
        .as_deref()
        .and_then(|id| find_row(&data.procedures, id))
        .cloned();
    let process = process_detail.get().and_then(|result| result.ok());
    let gates = data.gates.clone();
    let judge_gate = procedure_id
        .as_deref()
        .map(|target| gate_status(&gates, "procedures.publish", target))
        .unwrap_or("no draft");
    let process_gate = process_id
        .as_deref()
        .map(|target| gate_status(&gates, "processes.publish", target))
        .unwrap_or("no draft");
    let me = ctx.me.get().map(|me| me.principal.id);
    let targets: Vec<String> = [procedure_id.clone(), process_id.clone()]
        .into_iter()
        .flatten()
        .collect();
    let relevant: Vec<Value> = gates
        .iter()
        .filter(|gate| targets.contains(&text(gate, "target")))
        .cloned()
        .collect();
    let judge_publish_id = procedure_id.clone();
    let process_publish_id = process_id.clone();
    let judge_gates = gates.clone();
    let process_gates = gates.clone();
    let judge_target = procedure_id.clone();
    let process_target = process_id.clone();
    let decide_me = me.clone();

    view! {
        <div class="split">
            <section class="panel">
                <div class="ph"><span>"Publish readiness"</span></div>
                {match procedure {
                    Some(row) => view! {
                        <div class="kv">
                            <dt>"Judge procedure"</dt><dd class="mono">{format!("{} · {}", text(&row, "key"), text(&row, "status"))}</dd>
                            <dt>"Source asset"</dt><dd class="mono">{short_id(row.get("source_asset_id").and_then(Value::as_str))}</dd>
                            <dt>"Recorded approval"</dt><dd class="mono">{short_id(row.get("approval_id").and_then(Value::as_str))}</dd>
                            <dt>"Judge gate"</dt><dd><StatusPill status=judge_gate.to_owned()/></dd>
                        </div>
                    }.into_any(),
                    None => view! { <p class="muted">"Select a procedure in Workspace."</p> }.into_any(),
                }}
                {match process {
                    Some(row) => view! {
                        <div class="kv">
                            <dt>"Executable"</dt><dd class="mono">{format!("{} · {}", text(&row, "key"), text(&row, "status"))}</dd>
                            <dt>"Executable gate"</dt><dd><StatusPill status=process_gate.to_owned()/></dd>
                        </div>
                    }.into_any(),
                    None => view! { <p class="muted">"Select a process in Workspace to publish the executable."</p> }.into_any(),
                }}
                <div class="filterbar">
                    {(ctx.has_action("approvals.request") && judge_target.is_some()).then(|| view! {
                        <button class="btn small" on:click=move |_| request_gate("procedures.publish", judge_target.clone())>"Request the judge gate"</button>
                    })}
                    {(ctx.has_action("approvals.request") && process_target.is_some()).then(|| view! {
                        <button class="btn small" on:click=move |_| request_gate("processes.publish", process_target.clone())>"Request the executable gate"</button>
                    })}
                    {judge_publish_id.clone().filter(|_| ctx.has_action("procedures.publish")).map(|id| view! {
                        <button class="btn primary" on:click=move |_| {
                            let approval = approved_gate(&judge_gates, "procedures.publish", &id);
                            publish("procedures.publish", id.clone(), approval);
                        }>"Publish the judge"</button>
                    })}
                    {process_publish_id.clone().filter(|_| ctx.has_action("processes.publish")).map(|id| view! {
                        <button class="btn" on:click=move |_| {
                            let approval = approved_gate(&process_gates, "processes.publish", &id);
                            publish("processes.publish", id.clone(), approval);
                        }>"Publish the process"</button>
                    })}
                </div>
                <p class="hint">
                    "The requester cannot decide their own gate: a second person decides a listed gate, and publishing then names that approved gate. The approval id and the source are recorded on the released version."
                </p>
            </section>
            <section class="panel">
                <div class="ph">
                    <span>"Publication gates"</span>
                    <span class="muted">{format!("{} listed", relevant.len())}</span>
                </div>
                {if relevant.is_empty() {
                    view! { <Empty message="No pending or approved gate names this process or judge.".to_owned()/> }.into_any()
                } else {
                    view! {
                        <table class="table" data-testid="procedures-gates">
                            <thead><tr><th>"Gate"</th><th>"Status"</th><th>"Target"</th><th>"Approvals"</th><th>"Decision"</th></tr></thead>
                            <tbody>
                                {relevant.iter().map(|gate| {
                                    let id = text(gate, "id");
                                    let requested_by = gate.get("requested_by").and_then(Value::as_str).map(str::to_owned);
                                    let pending = text(gate, "status") == "pending";
                                    let can_decide = pending
                                        && ctx.has_action("approvals.decide")
                                        && requested_by.as_deref() != decide_me.as_deref();
                                    view! {
                                        <tr>
                                            <td class="mono">{text(gate, "gate")}</td>
                                            <td><StatusPill status=text(gate, "status")/></td>
                                            <td class="mono">{short_id(gate.get("target").and_then(Value::as_str))}</td>
                                            <td class="mono">{format!("{} / {}", int(gate, "approvals"), int(gate, "required_decisions"))}</td>
                                            <td>
                                                {if can_decide {
                                                    let approve_id = id.clone();
                                                    let reject_id = id.clone();
                                                    view! {
                                                        <button class="btn small" on:click=move |_| decide(approve_id.clone(), "approve")>"Approve"</button>
                                                        <button class="btn small danger" on:click=move |_| decide(reject_id.clone(), "reject")>"Reject"</button>
                                                    }.into_any()
                                                } else {
                                                    view! { <span class="muted">{if pending { "another person decides" } else { "—" }}</span> }.into_any()
                                                }}
                                            </td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                    }.into_any()
                }}
                <p class="hint">
                    "A gate with no parent is a publication gate: its target is the draft it releases. A tenant policy that waives the gate publishes without one."
                </p>
            </section>
        </div>
    }
    .into_any()
}
