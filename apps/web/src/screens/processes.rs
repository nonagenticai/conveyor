//! Processes — author, publish and run an executable process (S12 screen 7).
//!
//! A process is a version family (S11 §5.3): `draft → published → retired`, one published version
//! per key, and an edit is a whole-list replace under `If-Match`. The screen keeps the three facts
//! the data model makes true and hides none of them:
//!
//! - **A draft is editable; a published version is frozen.** The steps table saves the whole list
//!   with `processes.update` and the `lock_version` as `If-Match` — never the family `version`,
//!   which an edit does not move.
//! - **Publish is where the rules run.** Its `422` refusal is shown as the API returned it, in its
//!   own warning panel, before the successful publish. The UI does not pre-judge the checks; it
//!   repeats the answer.
//! - **A run names a card.** `process_runs.start` takes a card id and a published version; the
//!   screen shows the resulting run as a link, never a second run surface.
//!
//! Every control is drawn only when its action is in the role's manifest, and the gate vocabulary
//! is not invented here: the gate kind is free text because it is a tenant `catalogue_entry` code.

use chrono::{DateTime, Utc};
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{ApiError, InvokeOptions, Page};
use crate::components::{Empty, ErrorPanel, Planned, StatusPill, age, short_id};
use crate::state::AppCtx;

/// The list's status filter. The vocabulary is the domain's, not a UI invention.
const STATUSES: [(&str, &str); 4] = [
    ("", "Every status"),
    ("draft", "Draft"),
    ("published", "Published"),
    ("retired", "Retired"),
];

/// One step as every read returns it. `outputs` is carried through an edit untouched even though
/// this screen does not author it: `processes.update` replaces the whole list, so dropping a
/// member the user did not see would be a silent deletion.
#[derive(Clone, Debug, Deserialize)]
struct ProcessStep {
    #[serde(default)]
    position: i32,
    /// `agent | computed | gate | call` (SPEC D22/§21).
    #[serde(default)]
    kind: String,
    /// `process_agent | tool_agent | system | human`.
    #[serde(default)]
    actor: String,
    #[serde(default)]
    action: String,
    #[serde(default)]
    input_template: Value,
    #[serde(default)]
    gate_kind: Option<String>,
    /// A `call` step's child process key.
    #[serde(default)]
    child_process_key: Option<String>,
    /// The SOP the step is judged against.
    #[serde(default)]
    procedure_id: Option<String>,
    /// Carried through an edit untouched: the form does not author these.
    #[serde(default)]
    capability_id: Option<String>,
    #[serde(default)]
    process_agent_id: Option<String>,
    #[serde(default = "default_on_result")]
    on_result: String,
    #[serde(default = "default_attempts")]
    max_attempts: i32,
    #[serde(default)]
    outputs: Vec<String>,
}

fn default_on_result() -> String {
    "fail".to_owned()
}

fn default_attempts() -> i32 {
    1
}

/// A process version, as `processes.list` and `processes.get` return it.
#[derive(Clone, Debug, Deserialize)]
struct Process {
    pub id: String,
    #[serde(default)]
    pub key: String,
    pub title: String,
    #[serde(default)]
    pub status: String,
    pub version: i64,
    #[serde(default = "default_lock")]
    pub lock_version: i64,
    #[serde(default)]
    pub card_type_code: Option<String>,
    /// The user-facing deliverables a person sees when the process completes (D29).
    #[serde(default)]
    pub outputs: Value,
    #[serde(default)]
    pub steps: Vec<ProcessStep>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

fn default_lock() -> i64 {
    1
}

/// The editable form of one step. The table owns these values; a save turns them into `StepIn`
/// documents and sends the whole list, because that is the action's contract.
#[derive(Clone, Debug)]
struct StepDraft {
    kind: String,
    actor: String,
    action: String,
    template: String,
    gate_kind: String,
    child_process_key: String,
    procedure_id: String,
    capability_id: Option<String>,
    process_agent_id: Option<String>,
    on_result: Option<String>,
    max_attempts: Option<i32>,
    outputs: Vec<String>,
}

impl StepDraft {
    fn from_step(step: &ProcessStep) -> Self {
        Self {
            kind: step.kind.clone(),
            actor: step.actor.clone(),
            action: step.action.clone(),
            template: if step.input_template.is_null() {
                "{}".to_owned()
            } else {
                step.input_template.to_string()
            },
            gate_kind: step.gate_kind.clone().unwrap_or_default(),
            child_process_key: step.child_process_key.clone().unwrap_or_default(),
            procedure_id: step.procedure_id.clone().unwrap_or_default(),
            capability_id: step.capability_id.clone(),
            process_agent_id: step.process_agent_id.clone(),
            on_result: Some(step.on_result.clone()),
            max_attempts: Some(step.max_attempts),
            outputs: step.outputs.clone(),
        }
    }

    fn blank() -> Self {
        Self {
            kind: String::new(),
            actor: String::new(),
            action: String::new(),
            template: "{}".to_owned(),
            gate_kind: String::new(),
            child_process_key: String::new(),
            procedure_id: String::new(),
            capability_id: None,
            process_agent_id: None,
            on_result: None,
            max_attempts: None,
            outputs: Vec::new(),
        }
    }

    /// One `StepIn`, or the message that stops the save. The API's own validation is still the
    /// authority; this only catches what a browser can catch first (blank action, broken JSON).
    fn to_input(&self, index: usize) -> Result<Value, String> {
        let kind = self.kind.trim();
        let action = self.action.trim();
        if kind.is_empty() && action.is_empty() {
            return Err(format!(
                "Step {}: choose a kind or name an action.",
                index + 1
            ));
        }
        if kind == "computed" && action.is_empty() {
            return Err(format!(
                "Step {}: a computed step needs an action.",
                index + 1
            ));
        }
        let template: Value = serde_json::from_str(self.template.trim()).map_err(|error| {
            format!(
                "Step {}: the input template is not JSON: {error}",
                index + 1
            )
        })?;
        let mut input = serde_json::Map::new();
        if !kind.is_empty() {
            input.insert("kind".into(), json!(kind));
        }
        let actor = self.actor.trim();
        if !actor.is_empty() {
            input.insert("actor".into(), json!(actor));
        }
        input.insert("action".into(), json!(action));
        input.insert("input_template".into(), template);
        let gate = self.gate_kind.trim();
        if !gate.is_empty() {
            input.insert("gate_kind".into(), json!(gate));
        }
        let child = self.child_process_key.trim();
        if !child.is_empty() {
            input.insert("child_process_key".into(), json!(child));
        }
        let sop = self.procedure_id.trim();
        if !sop.is_empty() {
            input.insert("procedure_id".into(), json!(sop));
        }
        if let Some(capability) = &self.capability_id {
            input.insert("capability_id".into(), json!(capability));
        }
        if let Some(agent) = &self.process_agent_id {
            input.insert("process_agent_id".into(), json!(agent));
        }
        if let Some(on_result) = &self.on_result {
            input.insert("on_result".into(), json!(on_result));
        }
        if let Some(max_attempts) = self.max_attempts {
            input.insert("max_attempts".into(), json!(max_attempts));
        }
        input.insert("outputs".into(), json!(self.outputs));
        Ok(Value::Object(input))
    }
}

fn parse<T: serde::de::DeserializeOwned>(value: Value, what: &str) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Network(format!("{what}: {error}")))
}

/// One SOP as `procedures.get` returns it. The word *procedure* now means the SOP attached to a
/// step (CONTRACT §21); the sequence is the process.
#[derive(Clone, Debug, Deserialize)]
struct Sop {
    id: String,
    #[serde(default)]
    key: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    version: i64,
    #[serde(default)]
    definition_of_ready: Value,
    #[serde(default)]
    definition_of_done: Value,
    #[serde(default)]
    acceptance_criteria: Value,
    #[serde(default)]
    inspection: Value,
}

/// A JSON field as a person reads it: text stays text, a document is pretty-printed, absent is `—`.
fn json_block(value: &Value) -> String {
    match value {
        Value::Null => "—".to_owned(),
        Value::String(text) if text.trim().is_empty() => "—".to_owned(),
        Value::String(text) => text.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_else(|_| other.to_string()),
    }
}

/// The SOP a step is judged against: ready, done, acceptance and the inspection (the judge).
#[component]
fn SopPanel(procedure_id: String) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let data = {
        let id = procedure_id.clone();
        LocalResource::new(move || {
            let _tenant = ctx.tenant.get();
            let id = id.clone();
            async move {
                if id.is_empty() {
                    return Err(ApiError::Network("no SOP id".to_owned()));
                }
                parse::<Sop>(
                    ctx.invoke(
                        "procedures.get",
                        json!({"procedure_id": id}),
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                    "SOP",
                )
            }
        })
    };
    view! {
        <div class="panel" style="margin-top:10px">
            {move || match data.get() {
                None => view! { <p class="muted">"Reading the SOP…"</p> }.into_any(),
                Some(Err(error)) => view! {
                    <p class="muted">{format!("SOP {} did not load: {error}", short_id(Some(&procedure_id)))}</p>
                }.into_any(),
                Some(Ok(sop)) => view! {
                    <div class="ph">
                        <span>{if sop.title.is_empty() { sop.key.clone() } else { sop.title.clone() }}</span>
                        <span class="mono">{format!("SOP · v{} · {}", sop.version, short_id(Some(&sop.id)))}</span>
                    </div>
                    <div class="kv">
                        <dt>"Ready"</dt><dd class="mono">{json_block(&sop.definition_of_ready)}</dd>
                        <dt>"Done"</dt><dd class="mono">{json_block(&sop.definition_of_done)}</dd>
                        <dt>"Acceptance"</dt><dd class="mono">{json_block(&sop.acceptance_criteria)}</dd>
                        <dt>"Inspection"</dt><dd class="mono">{json_block(&sop.inspection)}</dd>
                    </div>
                }.into_any(),
            }}
        </div>
    }
}

fn load_list(
    ctx: AppCtx,
    status: RwSignal<Option<String>>,
) -> LocalResource<Result<Vec<Process>, ApiError>> {
    LocalResource::new(move || {
        // Every reactive input is read before the first await.
        let _tenant = ctx.tenant.get();
        let filter = status.get();
        let client = ctx.client();
        async move {
            let mut input = json!({"limit": 100});
            if let Some(filter) = filter {
                input["status"] = json!(filter);
            }
            let page: Page<Process> = parse(
                client
                    .invoke("processes.list", input, InvokeOptions::default())
                    .await?
                    .output,
                "processes",
            )?;
            Ok(page.items)
        }
    })
}

#[component]
pub fn ProcessesScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let status = RwSignal::new(Option::<String>::None);
    let data = load_list(ctx, status);
    let retry = Callback::new(move |_| data.refetch());

    let key = RwSignal::new(String::new());
    let title = RwSignal::new(String::new());
    let card_type = RwSignal::new(String::new());
    let notice = RwSignal::new(Option::<String>::None);
    let busy = RwSignal::new(false);

    let on_create = Callback::new(move |event: web_sys::SubmitEvent| {
        event.prevent_default();
        let key_value = key.get_untracked().trim().to_owned();
        let title_value = title.get_untracked().trim().to_owned();
        let card_type_value = card_type.get_untracked().trim().to_owned();
        if key_value.is_empty() || title_value.is_empty() {
            notice.set(Some("A key and a title are required.".into()));
            return;
        }
        let mut input = json!({"key": key_value, "title": title_value});
        if !card_type_value.is_empty() {
            input["card_type_code"] = json!(card_type_value);
        }
        busy.set(true);
        leptos::task::spawn_local(async move {
            match ctx
                .invoke("processes.create", input, InvokeOptions::default())
                .await
            {
                Ok(invocation) => match parse::<Process>(invocation.output, "process") {
                    Ok(created) => {
                        key.set(String::new());
                        title.set(String::new());
                        card_type.set(String::new());
                        notice.set(Some(format!(
                            "Created draft {}/v{}.",
                            created.key, created.version
                        )));
                        data.refetch();
                    }
                    Err(error) => notice.set(Some(error.to_string())),
                },
                Err(error) => notice.set(Some(error.to_string())),
            }
            busy.set(false);
        });
    });

    view! {
        <section class="screen">
            {move || {
                if !ctx.has_action("processes.list") {
                    return view! {
                        <Planned
                            title="Procedures is planned".to_owned()
                            detail="processes.list is not in this role's manifest. The register appears when the action lands, not before.".to_owned()
                        />
                    }
                    .into_any();
                }
                view! {
                    <header class="screen-head" style="flex-wrap:wrap">
                        <div>
                            <h1>"Processes"</h1>
                            <p class="muted">
                                "Ordered registry actions on one card. A draft is editable; publishing freezes the version."
                            </p>
                        </div>
                        <div class="filterbar">
                            <label class="filter">
                                <span>"Status"</span>
                                <select
                                    aria-label="Filter by status"
                                    on:change=move |event| {
                                        let value = event_target_value(&event);
                                        status.set(if value.is_empty() { None } else { Some(value) });
                                    }
                                >
                                    {STATUSES.iter().map(|(value, label)| {
                                        let option_value = *value;
                                        view! {
                                            <option value=option_value selected=move || {
                                                match status.get() {
                                                    None => option_value.is_empty(),
                                                    Some(current) => option_value == current.as_str(),
                                                }
                                            }>{*label}</option>
                                        }
                                    }).collect_view()}
                                </select>
                            </label>
                            <button class="btn" on:click=move |_| data.refetch()>"Refresh"</button>
                        </div>
                    </header>
                    {move || notice.get().map(|message| view! {
                        <div class="callout warn" role="status"><p>{message}</p></div>
                    })}
                    {move || match data.get() {
                        None => view! { <p class="muted">"Reading the process register…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=retry/>
                        }.into_any(),
                        Some(Ok(items)) => {
                            if items.is_empty() {
                                view! {
                                    <Empty message="No process matches this filter. Create one below.".to_owned()/>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="tablewrap">
                                        <table class="wtbl processes">
                                            <thead><tr>
                                                <th>"Key"</th><th>"Title"</th><th>"Status"</th>
                                                <th class="n">"Version"</th><th>"Card type"</th><th>"Updated"</th>
                                            </tr></thead>
                                            <tbody>
                                                {items.into_iter().map(|procedure| {
                                                    let id = procedure.id.clone();
                                                    view! {
                                                        <tr>
                                                            <td class="mono">{procedure.key.clone()}</td>
                                                            <td>
                                                                <A href=format!("/build/processes/{id}")>
                                                                    {procedure.title.clone()}
                                                                </A>
                                                            </td>
                                                            <td><StatusPill status=procedure.status.clone()/></td>
                                                            <td class="n">{procedure.version}</td>
                                                            <td class="mono">
                                                                {procedure.card_type_code.clone().unwrap_or_else(|| "—".to_owned())}
                                                            </td>
                                                            <td>{age(procedure.updated_at)}</td>
                                                        </tr>
                                                    }
                                                }).collect_view()}
                                            </tbody>
                                        </table>
                                    </div>
                                }.into_any()
                            }
                        }
                    }}
                    {move || ctx.has_action("processes.create").then(|| view! {
                        <form class="panel" on:submit=move |event| on_create.run(event)>
                            <h2>"New process"</h2>
                            <p class="muted">
                                "It starts as a draft. A key that already exists forks its next version."
                            </p>
                            <div class="formrow">
                                <label class="filter">
                                    <span>"Key"</span>
                                    <input
                                        prop:value=move || key.get()
                                        on:input=move |event| key.set(event_target_value(&event))
                                        placeholder="dev-phase"
                                    />
                                </label>
                                <label class="filter">
                                    <span>"Title"</span>
                                    <input
                                        prop:value=move || title.get()
                                        on:input=move |event| title.set(event_target_value(&event))
                                        placeholder="Development phase"
                                    />
                                </label>
                                <label class="filter">
                                    <span>"Card type"</span>
                                    <input
                                        prop:value=move || card_type.get()
                                        on:input=move |event| card_type.set(event_target_value(&event))
                                        placeholder="optional code, e.g. bug"
                                    />
                                </label>
                                <button class="btn primary" type="submit" disabled=move || busy.get()>
                                    {move || if busy.get() { "Creating…" } else { "Create draft" }}
                                </button>
                            </div>
                            <p class="hint adv blk">
                                "processes.create · the card type is a code from this tenant's card_type table"
                            </p>
                        </form>
                    })}
                }
                .into_any()
            }}
        </section>
    }
}

fn load_detail<F>(ctx: AppCtx, process_id: F) -> LocalResource<Result<Process, ApiError>>
where
    F: Fn() -> String + 'static,
{
    LocalResource::new(move || {
        // Every reactive input is read before the first await.
        let id = process_id();
        let _tenant = ctx.tenant.get();
        let client = ctx.client();
        async move {
            if id.is_empty() {
                return Err(ApiError::NotFound("no process in the route".to_owned()));
            }
            let procedure: Process = parse(
                client
                    .invoke(
                        "processes.get",
                        json!({"process_id": id}),
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                "process",
            )?;
            Ok(procedure)
        }
    })
}

#[component]
pub fn ProcessDetail() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let params = use_params_map();
    let process_id = move || params.get().get("process_id").unwrap_or_default();
    let data = load_detail(ctx, process_id);
    let refresh = Callback::new(move |_| data.refetch());

    // These four outlive a refetch: the resource re-renders the body, but a refusal, a published
    // version and a started run are answers the operator asked for and must stay on screen.
    let notice = RwSignal::new(Option::<String>::None);
    let refusal = RwSignal::new(Option::<String>::None);
    let published = RwSignal::new(Option::<String>::None);
    let run_id = RwSignal::new(Option::<String>::None);

    view! {
        <section class="screen">
            {move || {
                if process_id().is_empty() {
                    return view! { <p class="muted">"No process in the route."</p> }.into_any();
                }
                view! {
                    <A href="/build/processes">"← All processes"</A>
                    {move || notice.get().map(|message| view! {
                        <div class="callout warn" role="status"><p>{message}</p></div>
                    })}
                    {move || refusal.get().map(|message| view! {
                        <div class="callout bad" role="alert">
                            <div class="hd">"Publish refused"</div>
                            <p class="mono">{message}</p>
                        </div>
                    })}
                    {move || published.get().map(|message| view! {
                        <div class="callout" role="status"><p>{message}</p></div>
                    })}
                    {move || run_id.get().map(|id| view! {
                        <div class="callout" role="status">
                            <p>
                                "Run started. "
                                <A href=format!("/runs/{id}")>{format!("Open run {}", short_id(Some(&id)))}</A>
                            </p>
                        </div>
                    })}
                    {move || match data.get() {
                        None => view! { <p class="muted">"Reading the process…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=refresh/>
                        }.into_any(),
                        Some(Ok(procedure)) => view! {
                            <ProcessBody
                                procedure=procedure
                                refresh=refresh
                                notice=notice
                                refusal=refusal
                                published=published
                                run_id=run_id
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
fn ProcessBody(
    procedure: Process,
    refresh: Callback<()>,
    notice: RwSignal<Option<String>>,
    refusal: RwSignal<Option<String>>,
    published: RwSignal<Option<String>>,
    run_id: RwSignal<Option<String>>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();

    let process_id = procedure.id.clone();
    let key = procedure.key.clone();
    let title = procedure.title.clone();
    let card_type = procedure.card_type_code.clone();
    let version = procedure.version;
    let outputs = procedure.outputs.clone();
    // Each distinct SOP a step is judged against, in a stable order.
    let sops: Vec<String> = procedure
        .steps
        .iter()
        .filter_map(|step| step.procedure_id.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    // `status` and `lock` are seeded from the read and then advanced by each action's own answer,
    // so a screen that does not refetch in time still shows what the server just said.
    let status = RwSignal::new(procedure.status.clone());
    let lock = RwSignal::new(procedure.lock_version);

    // One signal per row: typing updates only that row, so the table never re-renders under the
    // cursor. `next_id` keeps a stable key when a row is removed.
    let next_id = RwSignal::new(procedure.steps.len());
    let rows = RwSignal::new(
        procedure
            .steps
            .iter()
            .enumerate()
            .map(|(index, step)| (index, RwSignal::new(StepDraft::from_step(step))))
            .collect::<Vec<(usize, RwSignal<StepDraft>)>>(),
    );
    let busy = RwSignal::new(false);
    let card_id = RwSignal::new(String::new());

    let save_steps = Callback::new({
        let process_id = process_id.clone();
        move |_: ()| {
            let drafts: Vec<StepDraft> = rows
                .get_untracked()
                .iter()
                .map(|(_, row)| row.get_untracked())
                .collect();
            if drafts.is_empty() {
                notice.set(Some("Add at least one step before saving.".into()));
                return;
            }
            let mut steps = Vec::with_capacity(drafts.len());
            for (index, draft) in drafts.iter().enumerate() {
                match draft.to_input(index) {
                    Ok(value) => steps.push(value),
                    Err(message) => {
                        notice.set(Some(message));
                        return;
                    }
                }
            }
            let id = process_id.clone();
            let expected = lock.get_untracked();
            busy.set(true);
            leptos::task::spawn_local(async move {
                let result = ctx
                    .invoke(
                        "processes.update",
                        json!({"process_id": id, "steps": steps}),
                        InvokeOptions {
                            if_match: Some(expected),
                            ..Default::default()
                        },
                    )
                    .await;
                match result {
                    Ok(invocation) => match parse::<Process>(invocation.output, "process") {
                        Ok(saved) => {
                            let rebuilt = saved
                                .steps
                                .iter()
                                .map(|step| {
                                    let row_id = next_id.get_untracked();
                                    next_id.set(row_id + 1);
                                    (row_id, RwSignal::new(StepDraft::from_step(step)))
                                })
                                .collect::<Vec<(usize, RwSignal<StepDraft>)>>();
                            rows.set(rebuilt);
                            lock.set(saved.lock_version);
                            status.set(saved.status.clone());
                            refusal.set(None);
                            notice.set(Some(format!(
                                "Saved {} steps at lock v{}.",
                                saved.steps.len(),
                                saved.lock_version
                            )));
                        }
                        Err(error) => notice.set(Some(error.to_string())),
                    },
                    Err(ApiError::Precondition(detail)) => {
                        notice.set(Some(format!(
                            "The procedure moved before this save: {detail} — reloaded."
                        )));
                        refresh.run(());
                    }
                    Err(error) => notice.set(Some(error.to_string())),
                }
                busy.set(false);
            });
        }
    });

    let publish = Callback::new({
        let process_id = process_id.clone();
        move |_: ()| {
            let id = process_id.clone();
            refusal.set(None);
            published.set(None);
            busy.set(true);
            leptos::task::spawn_local(async move {
                match ctx
                    .invoke(
                        "processes.publish",
                        json!({"process_id": id, "acknowledge_findings": false}),
                        InvokeOptions::default(),
                    )
                    .await
                {
                    Ok(invocation) => {
                        let version = invocation
                            .output
                            .get("version")
                            .and_then(Value::as_i64)
                            .unwrap_or_default();
                        let revision = invocation
                            .output
                            .get("revision_id")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        let hash = invocation
                            .output
                            .get("content_hash")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        published.set(Some(format!(
                            "Published version {version} · revision {} · hash {}",
                            short_id(Some(revision)),
                            short_id(Some(hash))
                        )));
                        status.set("published".to_owned());
                    }
                    // The API's refusal, verbatim. It is a `422` problem document; the screen names the
                    // status and repeats the server's detail, including any field it pointed at.
                    Err(ApiError::Invalid { detail, errors }) => {
                        let mut message = format!("422 · {detail}");
                        for error in &errors {
                            message.push_str(&format!(" · {}: {}", error.field, error.detail));
                        }
                        refusal.set(Some(message));
                    }
                    Err(error) => refusal.set(Some(error.to_string())),
                }
                busy.set(false);
            });
        }
    });

    let retire = Callback::new({
        let process_id = process_id.clone();
        move |_: ()| {
            let id = process_id.clone();
            busy.set(true);
            leptos::task::spawn_local(async move {
                match ctx
                    .invoke(
                        "processes.retire",
                        json!({"process_id": id}),
                        InvokeOptions::default(),
                    )
                    .await
                {
                    Ok(invocation) => match parse::<Process>(invocation.output, "process") {
                        Ok(retired) => {
                            status.set(retired.status.clone());
                            notice.set(Some(format!("Retired version {}.", retired.version)));
                        }
                        Err(error) => notice.set(Some(error.to_string())),
                    },
                    Err(error) => notice.set(Some(error.to_string())),
                }
                busy.set(false);
            });
        }
    });

    let start_run = Callback::new({
        let process_id = process_id.clone();
        move |event: web_sys::SubmitEvent| {
            event.prevent_default();
            let card = card_id.get_untracked().trim().to_owned();
            if card.is_empty() {
                notice.set(Some("A card id is required to start a run.".into()));
                return;
            }
            let id = process_id.clone();
            run_id.set(None);
            busy.set(true);
            leptos::task::spawn_local(async move {
                match ctx
                    .invoke(
                        "process_runs.start",
                        json!({"card_id": card, "process_id": id, "inputs": {}}),
                        InvokeOptions::default(),
                    )
                    .await
                {
                    Ok(invocation) => match invocation.output.get("id").and_then(Value::as_str) {
                        Some(id) => {
                            run_id.set(Some(id.to_owned()));
                            notice.set(Some("Run started.".into()));
                        }
                        None => notice.set(Some("process_runs.start returned no run id".into())),
                    },
                    Err(error) => notice.set(Some(error.to_string())),
                }
                busy.set(false);
            });
        }
    });

    let add_step = Callback::new(move |_: ()| {
        let row_id = next_id.get_untracked();
        next_id.set(row_id + 1);
        rows.update(|list| list.push((row_id, RwSignal::new(StepDraft::blank()))));
        refusal.set(None);
    });

    let remove_row = Callback::new(move |row_id: usize| {
        rows.update(|list| list.retain(|(id, _)| *id != row_id));
    });

    let row_count = move || rows.get().len();

    view! {
        <header class="screen-head" style="margin-top:8px;flex-wrap:wrap">
            <div>
                <h1>{title.clone()}</h1>
                <p class="muted">
                    <span class="mono">{key.clone()}</span>
                    {" · "}
                    {move || view! { <StatusPill status=status.get()/> }}
                    {" · "}
                    {format!("version {version}")}
                    {card_type.clone().map(|code| format!(" · card type {code}")).unwrap_or_default()}
                </p>
            </div>
            <div class="filterbar">
                <span class="adv mono">{move || format!("lock v{}", lock.get())}</span>
            </div>
        </header>

        {(!outputs.is_null() && outputs.as_array().is_some_and(|rows| !rows.is_empty())).then(|| view! {
            <section class="panel" style="margin-bottom:10px">
                <div class="ph"><span>"User-facing outputs"</span></div>
                <ul>
                    {outputs.as_array().cloned().unwrap_or_default().into_iter().map(|row| {
                        let name = row.get("name").and_then(Value::as_str)
                            .or_else(|| row.get("path").and_then(Value::as_str))
                            .unwrap_or("output")
                            .to_owned();
                        let description = row.get("description").and_then(Value::as_str).unwrap_or("").to_owned();
                        view! { <li><span class="mono">{name}</span>{(!description.is_empty()).then(|| format!(" — {description}"))}</li> }
                    }).collect_view()}
                </ul>
            </section>
        })}

        <div class="split">
            <section class="panel">
                <div class="ph">
                    <span>"Steps"</span>
                    <span class="muted">{move || format!("{} ordered", row_count())}</span>
                </div>
                {move || {
                    if ctx.has_action("processes.update") {
                        view! {
                            {move || (row_count() == 0).then(|| view! {
                                <Empty message="No steps yet. Add one below.".to_owned()/>
                            })}
                            {move || (row_count() != 0).then(|| view! {
                                <div class="tablewrap">
                                    <table class="wtbl steps">
                                        <thead><tr>
                                            <th class="n">"#"</th><th>"Kind"</th><th>"Actor"</th>
                                            <th>"Action"</th><th>"SOP"</th>
                                            <th>"Input template (JSON)"</th><th>"Gate kind"</th><th></th>
                                        </tr></thead>
                                        <tbody>
                                            {move || rows.get().into_iter().enumerate().map(|(index, (row_id, row))| {
                                                let kind = move || row.get().kind;
                                                let actor = move || row.get().actor;
                                                let action = move || row.get().action;
                                                let sop = move || row.get().procedure_id;
                                                let template = move || row.get().template;
                                                let gate = move || row.get().gate_kind;
                                                view! {
                                                    <tr>
                                                        <td class="n">{format!("{}", index + 1)}</td>
                                                        <td>
                                                            <input
                                                                aria-label=format!("Step {} kind", index + 1)
                                                                prop:value=kind
                                                                on:input=move |event| {
                                                                    let value = event_target_value(&event);
                                                                    row.update(|draft| draft.kind = value);
                                                                }
                                                                placeholder="agent | computed | gate | call"
                                                            />
                                                        </td>
                                                        <td>
                                                            <input
                                                                aria-label=format!("Step {} actor", index + 1)
                                                                prop:value=actor
                                                                on:input=move |event| {
                                                                    let value = event_target_value(&event);
                                                                    row.update(|draft| draft.actor = value);
                                                                }
                                                                placeholder="process_agent | tool_agent | system | human"
                                                            />
                                                        </td>
                                                        <td>
                                                            <input
                                                                aria-label=format!("Step {} action", index + 1)
                                                                prop:value=action
                                                                on:input=move |event| {
                                                                    let value = event_target_value(&event);
                                                                    row.update(|draft| draft.action = value);
                                                                }
                                                                placeholder="cards.comment"
                                                            />
                                                        </td>
                                                        <td>
                                                            <input
                                                                aria-label=format!("Step {} SOP", index + 1)
                                                                prop:value=sop
                                                                on:input=move |event| {
                                                                    let value = event_target_value(&event);
                                                                    row.update(|draft| draft.procedure_id = value);
                                                                }
                                                                placeholder="SOP id"
                                                            />
                                                        </td>
                                                        <td>
                                                            <textarea
                                                                rows="2"
                                                                aria-label=format!("Step {} input template", index + 1)
                                                                prop:value=template
                                                                on:input=move |event| {
                                                                    let value = event_target_value(&event);
                                                                    row.update(|draft| draft.template = value);
                                                                }
                                                            ></textarea>
                                                        </td>
                                                        <td>
                                                            <input
                                                                aria-label=format!("Step {} gate kind", index + 1)
                                                                prop:value=gate
                                                                on:input=move |event| {
                                                                    let value = event_target_value(&event);
                                                                    row.update(|draft| draft.gate_kind = value);
                                                                }
                                                                placeholder="optional, e.g. human_approval"
                                                            />
                                                        </td>
                                                        <td>
                                                            <button
                                                                class="btn small"
                                                                type="button"
                                                                on:click=move |_| remove_row.run(row_id)
                                                            >"Remove"</button>
                                                        </td>
                                                    </tr>
                                                }
                                            }).collect_view()}
                                        </tbody>
                                    </table>
                                </div>
                            })}
                            <div style="display:flex;gap:6px;margin-top:8px;flex-wrap:wrap">
                                <button class="btn small" type="button" on:click=move |_| add_step.run(())>"Add step"</button>
                                <button
                                    class="btn pri small"
                                    type="button"
                                    on:click=move |_| save_steps.run(())
                                    disabled=move || busy.get()
                                >"Save steps"</button>
                            </div>
                            <p class="muted" style="margin-top:8px">
                                "The whole list is replaced on save, under If-Match. The gate kind is a catalogue_entry code; a blank one means the step runs immediately."
                            </p>
                        }.into_any()
                    } else {
                        view! {
                            {if procedure.steps.is_empty() {
                                view! { <Empty message="This version has no steps.".to_owned()/> }.into_any()
                            } else {
                                view! {
                                    <div class="tablewrap">
                                        <table class="wtbl">
                                            <thead><tr>
                                                <th class="n">"#"</th><th>"Kind"</th><th>"Actor"</th>
                                                <th>"Action"</th><th>"SOP"</th><th>"Gate kind"</th>
                                            </tr></thead>
                                            <tbody>
                                                {procedure.steps.iter().map(|step| view! {
                                                    <tr>
                                                        <td class="n">{step.position}</td>
                                                        <td class="mono">{if step.kind.is_empty() { "—".to_owned() } else { step.kind.clone() }}</td>
                                                        <td class="mono">{if step.actor.is_empty() { "—".to_owned() } else { step.actor.clone() }}</td>
                                                        <td class="mono">{step.action.clone()}</td>
                                                        <td class="mono">{step.procedure_id.clone().map(|id| short_id(Some(&id))).unwrap_or_else(|| "—".to_owned())}</td>
                                                        <td class="mono">{step.gate_kind.clone().unwrap_or_else(|| "—".to_owned())}</td>
                                                    </tr>
                                                }).collect_view()}
                                            </tbody>
                                        </table>
                                    </div>
                                }.into_any()
                            }}
                        }.into_any()
                    }
                }}
                {sops.into_iter().map(|id| view! { <SopPanel procedure_id=id/> }).collect_view()}
            </section>

            <div>
                <section class="panel">
                    <div class="ph"><span>"Actions"</span></div>
                    <div style="display:flex;flex-wrap:wrap;gap:6px">
                        {move || (status.get() == "draft" && ctx.has_action("processes.publish")).then(|| view! {
                            <button class="btn pri small" on:click=move |_| publish.run(()) disabled=move || busy.get()>
                                "Publish"
                            </button>
                        })}
                        {move || (status.get() == "published" && ctx.has_action("processes.retire")).then(|| view! {
                            <button class="btn small danger" on:click=move |_| retire.run(()) disabled=move || busy.get()>
                                "Retire"
                            </button>
                        })}
                        <button class="btn small" on:click=move |_| refresh.run(())>"Refresh"</button>
                    </div>
                    <p class="muted" style="margin-top:8px">
                        {move || match status.get().as_str() {
                            "draft" => "Publishing freezes this version and audits it first.",
                            "published" => "This version is frozen; an edit creates the next version.",
                            "retired" => "Retired; runs already underway are unaffected.",
                            _ => "",
                        }}
                    </p>
                </section>

                <section class="panel" style="margin-top:12px">
                    <div class="ph"><span>"Run on a card"</span></div>
                    {move || {
                        if status.get() != "published" {
                            return view! {
                                <p class="muted">"Publish this version before starting a run."</p>
                            }.into_any();
                        }
                        if !ctx.has_action("process_runs.start") {
                            return view! {
                                <p class="muted">"process_runs.start is not in this role's manifest."</p>
                            }.into_any();
                        }
                        view! {
                            <form on:submit=move |event| start_run.run(event)>
                                <label class="filter">
                                    <span>"Card id"</span>
                                    <input
                                        aria-label="Card id"
                                        prop:value=move || card_id.get()
                                        on:input=move |event| card_id.set(event_target_value(&event))
                                        placeholder="00000000-0000-0000-0000-000000000000"
                                    />
                                </label>
                                <button class="btn primary small" type="submit" disabled=move || busy.get() style="margin-top:8px">
                                    "Start run"
                                </button>
                                <p class="muted" style="margin-top:8px">
                                    "The card must be of this procedure's card type. One live run per card."
                                </p>
                            </form>
                        }.into_any()
                    }}
                </section>

                <section class="panel" style="margin-top:12px">
                    <div class="ph"><span>"Version"</span></div>
                    <div class="kv">
                        <dt>"Key"</dt><dd class="mono">{key.clone()}</dd>
                        <dt>"Family version"</dt><dd class="mono">{version}</dd>
                        <dt>"Lock"</dt><dd class="mono adv">{move || format!("v{}", lock.get())}</dd>
                        <dt>"Id"</dt><dd class="mono adv">{short_id(Some(&process_id))}</dd>
                    </div>
                    <p class="hint adv blk" style="margin-top:8px">
                        "processes.get · processes.update (If-Match on lock_version) · processes.publish · processes.retire · process_runs.start"
                    </p>
                </section>
            </div>
        </div>
    }
}
