//! Evals — the instrument, its runs, the report, the sign-off, the climb, the health audit
//! and the interop tab.
//!
//! The screen shows what the actions actually answer. The index creates an instrument, the
//! detail edits or retires it and freezes its next version, and the Cases tab authors the cases
//! and the metrics that grade them. The audit tab starts `eval_audits.start` and reads the
//! findings the worker writes; interop exports a run as an S09 document and imports a JSON
//! case bundle.
//!
//! Report numbers come from `eval_runs.report` — the server derives the means, the intervals,
//! the noise floor, the headroom and the cost from the run's rows, so the screen never computes
//! a score from a page. `eval_results.list` only feeds the per-case table, which pages with its
//! own `next_cursor`: a page is rows, never a whole-run figure. A held-out `test` split needs
//! `evals:approve`; a refusal is shown as the server's message and leaves the tiles empty,
//! never zeroed.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use serde_json::{Value, json};
use wasm_bindgen::JsCast;

use crate::api::{ApiError, InvokeOptions, Page};
use crate::components::{Empty, ErrorPanel, Planned, StatusPill, Tile, age, fmt_int, short_id};
use crate::models::{
    ApprovalOut, ClimbRound, EvalCase, EvalClimb, EvalResult, EvalResultDetail, EvalRun,
    EvalSummary, EvalVersion,
};
use crate::state::AppCtx;

const TABS: [(&str, &str); 7] = [
    ("cases", "Cases"),
    ("runs", "Runs"),
    ("report", "Report"),
    ("sign-off", "Sign-off"),
    ("climb", "Climb"),
    ("audit", "Audit"),
    ("interop", "Interop"),
];

const GATES: [(&str, &str); 4] = [
    ("eval_inputs", "Inputs"),
    ("eval_grading", "Grading"),
    ("eval_cost", "Cost ceiling"),
    ("eval_harness", "Harness"),
];

// The closed vocabularies the actions validate. The screen offers exactly these; the server's
// `422` is still the authority and is shown verbatim when it disagrees.
const SUBJECT_KINDS: [&str; 5] = ["completions", "copilot", "process", "external", "agent_run"];
const TRACE_SOURCES: [&str; 3] = ["synthesized", "conversation_turn", "run_step"];
const CASE_SOURCES: [&str; 4] = ["production", "ticket", "handwritten", "synthesized"];
const GOLD_PROVENANCES: [&str; 3] = ["human_written", "human_verified", "model"];
const METRIC_ROLES: [&str; 2] = ["quality", "perf"];
const METRIC_KINDS: [&str; 3] = ["binary", "judge", "float"];
const METRIC_BETTER: [&str; 2] = ["higher", "lower"];
const GRADER_KINDS: [&str; 5] = ["programmatic", "pairwise", "rubric", "human", "none"];

fn parse<T: serde::de::DeserializeOwned>(value: Value, what: &str) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Network(format!("{what}: {error}")))
}

/// `eval_versions.get` — the version's fields at the top level plus the metrics it froze.
///
/// The cases are deliberately not embedded: `eval_cases.list` pages them. Metrics have no list
/// action, so this read is the authoring screen's only view of them.
#[derive(Clone, Debug, serde::Deserialize)]
struct VersionDetail {
    #[serde(flatten)]
    version: EvalVersion,
    #[serde(default)]
    metrics: Vec<MetricRow>,
}

/// One metric row from `eval_versions.get`.
#[derive(Clone, Debug, serde::Deserialize)]
struct MetricRow {
    id: String,
    #[serde(default)]
    key: String,
    #[serde(default)]
    role: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    better: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    grader_kind: String,
    #[serde(default)]
    grader: Value,
    #[serde(default)]
    rubric: Option<String>,
    #[serde(default)]
    ordinal: i32,
}

/// `eval_cases.list` — the case view plus the attachment document an update must send back.
///
/// `CaseView` carries `attachments` but the shared `EvalCase` model does not, and
/// `eval_cases.update` is a replacement: omitting them would blank the column.
#[derive(Clone, Debug, serde::Deserialize)]
struct CaseRow {
    #[serde(flatten)]
    case: EvalCase,
    #[serde(default)]
    attachments: Value,
}

impl std::ops::Deref for CaseRow {
    type Target = EvalCase;

    fn deref(&self) -> &Self::Target {
        &self.case
    }
}

/// How many result rows one `eval_results.list` request asks for. The table pages with the
/// action's own `next_cursor`; this is a page size, never a cap the screen treats as a total.
const RESULTS_PAGE: i64 = 50;

/// `eval_results.list` — one result row plus the person's score.
///
/// `human_score` is a decimal **string** in the action's output (`"0.75"`), while the shared
/// `EvalResult` model reads it as a float. Flattening keeps the shared model and parses the
/// score here, so a labelled row does not make every page that carries one unreadable.
#[derive(Clone, Debug, serde::Deserialize)]
struct ResultRowOut {
    #[serde(flatten)]
    result: EvalResult,
    #[serde(default, deserialize_with = "human_score_of")]
    human_score: Option<f64>,
}

impl std::ops::Deref for ResultRowOut {
    type Target = EvalResult;

    fn deref(&self) -> &Self::Target {
        &self.result
    }
}

/// A decimal string or a number → `f64`; anything else is no score.
fn human_score_of<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = <Option<Value> as serde::Deserialize>::deserialize(deserializer)?;
    Ok(raw.and_then(|value| match value {
        Value::String(text) => text.trim().parse::<f64>().ok(),
        Value::Number(number) => number.as_f64(),
        _ => None,
    }))
}

/// `eval_runs.report` — one run's server-derived numbers (S19, CONTRACT §20).
#[derive(Clone, Debug, serde::Deserialize)]
#[allow(dead_code)]
struct ReportOut {
    #[serde(default)]
    variant: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    reps: i32,
    #[serde(default)]
    cases: i64,
    #[serde(default)]
    graded: i64,
    #[serde(default)]
    truncated: i64,
    #[serde(default)]
    errors: ErrorCountsOut,
    #[serde(default)]
    metrics: Vec<MetricStatOut>,
    #[serde(default)]
    perf: Vec<MetricStatOut>,
    #[serde(default)]
    per_tag: Vec<GroupStatOut>,
    #[serde(default)]
    noise_floor: Option<f64>,
    #[serde(default)]
    headroom: Option<f64>,
    #[serde(default)]
    cost: CostOut,
}

/// One declared metric's numbers from `eval_runs.report`.
#[derive(Clone, Debug, serde::Deserialize)]
#[allow(dead_code)]
struct MetricStatOut {
    key: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    role: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    better: String,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    n: i64,
    #[serde(default)]
    missing: i64,
    #[serde(default)]
    mean: Option<f64>,
    #[serde(default)]
    ci_low: Option<f64>,
    #[serde(default)]
    ci_high: Option<f64>,
}

/// One group of `eval_runs.report`'s per-tag breakdown.
#[derive(Clone, Debug, serde::Deserialize)]
struct GroupStatOut {
    #[serde(default)]
    group: String,
    #[serde(default)]
    n: i64,
    #[serde(default)]
    metrics: Vec<MetricStatOut>,
}

/// The failed attempts, counted.
#[derive(Clone, Debug, Default, serde::Deserialize)]
struct ErrorCountsOut {
    #[serde(default)]
    total: i64,
    #[serde(default)]
    failed_slots: i64,
    #[serde(default)]
    by_class: std::collections::BTreeMap<String, i64>,
}

/// What the run spent, in the unit the version prices in.
#[derive(Clone, Debug, Default, serde::Deserialize)]
struct CostOut {
    #[serde(default)]
    unit: String,
    #[serde(default)]
    subject: f64,
    #[serde(default)]
    judge: f64,
    #[serde(default)]
    failed: f64,
    #[serde(default)]
    total: f64,
    #[serde(default)]
    unpriced: i64,
}

/// `eval_climbs.compare` — the paired comparison of two runs.
#[derive(Clone, Debug, serde::Deserialize)]
struct CompareOut {
    #[serde(default)]
    metric: String,
    #[serde(default)]
    both: usize,
    #[serde(default)]
    neither: usize,
    #[serde(default)]
    a_only: usize,
    #[serde(default)]
    b_only: usize,
    #[serde(default)]
    discordant: usize,
    #[serde(default)]
    missing_in_one: usize,
    #[serde(default)]
    p_value: f64,
    #[serde(default)]
    delta: f64,
    #[serde(default)]
    ci_low: f64,
    #[serde(default)]
    ci_high: f64,
    #[serde(default)]
    significant: bool,
}

// ── index ───────────────────────────────────────────────────────────────────────────────

fn load_index(ctx: AppCtx) -> LocalResource<Result<Vec<EvalSummary>, ApiError>> {
    LocalResource::new(move || async move {
        let _tenant = ctx.tenant.get();
        let page: Page<EvalSummary> = parse(
            ctx.client()
                .invoke("evals.list", json!({"limit": 50}), InvokeOptions::default())
                .await?
                .output,
            "evals",
        )?;
        Ok(page.items)
    })
}

#[component]
pub fn Evals() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let data = load_index(ctx);
    let retry = Callback::new(move |_| data.refetch());

    view! {
        <section class="screen measure">
            {move || {
                if !ctx.has_action("evals.list") {
                    return view! {
                        <Planned
                            title="Evals are planned".to_owned()
                            detail="evals.list is not in this role's manifest. The screen appears when the action lands, not before.".to_owned()
                        />
                    }.into_any();
                }
                view! {
                    <header class="screen-head">
                        <div>
                            <h1>"Evals"</h1>
                            <p class="muted">"one flow per eval · a version freezes cases, metrics, graders, prices and harness paths under one digest"</p>
                        </div>
                        <div class="filterbar">
                            <button class="btn" on:click=move |_| data.refetch()>"Refresh"</button>
                        </div>
                    </header>
                    {move || match data.get() {
                        None => view! { <p class="muted">"Reading the instruments…"</p> }.into_any(),
                        Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                        Some(Ok(rows)) => view! {
                            {if rows.is_empty() {
                                view! { <Empty message="No eval in this tenant yet.".to_owned()/> }.into_any()
                            } else {
                                view! {
                                    <table class="wtbl">
                                        <thead><tr>
                                            <th>"Eval"</th><th>"Subject"</th><th>"Entry action"</th>
                                            <th>"Status"</th><th>"Version"</th><th>"Changed"</th>
                                        </tr></thead>
                                        <tbody>
                                            {rows.clone().into_iter().map(|row| {
                                                let id = row.id.clone();
                                                let href = format!("/evals/{id}/report");
                                                view! {
                                                    <tr>
                                                        <td>
                                                            <A href=href><span class="row-title">{row.name.clone()}</span></A>
                                                            <span class="adv mono">" "{short_id(Some(&id))}</span>
                                                        </td>
                                                        <td class="mono">{row.subject_kind.clone()}</td>
                                                        <td class="mono">{row.entry_action.clone()}</td>
                                                        <td><StatusPill status=row.status.clone()/></td>
                                                        <td>
                                                            {row.current_version_id.clone().map(|version| view! {
                                                                <span class="mono">{short_id(Some(&version))}</span>
                                                            })}
                                                        </td>
                                                        <td>{row.updated_at.map(|at| age(Some(at))).unwrap_or_else(|| "—".to_owned())}</td>
                                                    </tr>
                                                }
                                            }).collect_view()}
                                        </tbody>
                                    </table>
                                }.into_any()
                            }}
                            {move || ctx.has_action("evals.create").then(|| view! {
                                <EvalCreateForm on_created=retry/>
                            })}
                        }.into_any(),
                    }}
                }
                .into_any()
            }}
        </section>
    }
}

/// `evals.create` — the instrument's subject, fixed at creation, plus its first content
/// fields. The optional fields live behind an expanded block so the required three stay the
/// only thing a first author must answer.
#[component]
fn EvalCreateForm(on_created: Callback<()>) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let name = RwSignal::new(String::new());
    let subject_kind = RwSignal::new("completions".to_owned());
    let entry_action = RwSignal::new("completions.create".to_owned());
    let purpose = RwSignal::new(String::new());
    let model_ref = RwSignal::new(String::new());
    let prompt_ref = RwSignal::new(String::new());
    let inputs_schema = RwSignal::new("{}".to_owned());
    let trace_source = RwSignal::new("synthesized".to_owned());
    let busy = RwSignal::new(false);
    let notice = RwSignal::new(Option::<String>::None);

    let on_submit = move |event: web_sys::SubmitEvent| {
        event.prevent_default();
        let name_value = name.get_untracked().trim().to_owned();
        if name_value.is_empty() {
            notice.set(Some("A name is required.".into()));
            return;
        }
        let entry_value = entry_action.get_untracked().trim().to_owned();
        if entry_value.is_empty() {
            notice.set(Some("An entry action is required.".into()));
            return;
        }
        let schema = match parse_json_field(&inputs_schema.get_untracked(), "inputs_schema") {
            Ok(Value::Null) => json!({}),
            Ok(value) => value,
            Err(error) => {
                notice.set(Some(error));
                return;
            }
        };
        let mut input = json!({
            "name": name_value,
            "subject_kind": subject_kind.get_untracked(),
            "entry_action": entry_value,
            "inputs_schema": schema,
            "trace_source": trace_source.get_untracked(),
        });
        let purpose_value = purpose.get_untracked().trim().to_owned();
        if !purpose_value.is_empty() {
            input["purpose"] = json!(purpose_value);
        }
        let model_value = model_ref.get_untracked().trim().to_owned();
        if !model_value.is_empty() {
            input["model_ref"] = json!(model_value);
        }
        let prompt_value = prompt_ref.get_untracked().trim().to_owned();
        if !prompt_value.is_empty() {
            input["prompt_ref"] = json!(prompt_value);
        }
        let ctx = ctx;
        busy.set(true);
        leptos::task::spawn_local(async move {
            match ctx
                .invoke("evals.create", input, InvokeOptions::default())
                .await
            {
                Ok(_) => {
                    name.set(String::new());
                    purpose.set(String::new());
                    model_ref.set(String::new());
                    prompt_ref.set(String::new());
                    notice.set(Some(
                        "Eval created. Freeze its first version to add cases and metrics.".into(),
                    ));
                    on_created.run(());
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
            busy.set(false);
        });
    };

    view! {
        <section class="panel" style="margin-top:12px">
            <div class="ph">
                <span>"New eval"</span>
                <span class="muted mono">"evals.create"</span>
            </div>
            <p class="muted" style="font-size:11px">
                "An eval measures one flow. The subject and the entry action are fixed at creation and a later edit cannot change them."
            </p>
            <form on:submit=on_submit>
                <div class="formrow">
                    <label class="filter">
                        <span>"Name"</span>
                        <input
                            aria-label="Eval name"
                            required
                            prop:value=move || name.get()
                            on:input=move |event| name.set(event_target_value(&event))
                            placeholder="What the instrument measures"
                        />
                    </label>
                    <label class="filter">
                        <span>"Subject"</span>
                        <select
                            aria-label="Subject kind"
                            on:change=move |event| subject_kind.set(event_target_value(&event))
                        >
                            {SUBJECT_KINDS.into_iter().map(|value| view! {
                                <option value=value selected=move || subject_kind.get() == value>{value}</option>
                            }).collect_view()}
                        </select>
                    </label>
                    <label class="filter">
                        <span>"Entry action"</span>
                        <input
                            aria-label="Entry action"
                            required
                            prop:value=move || entry_action.get()
                            on:input=move |event| entry_action.set(event_target_value(&event))
                            placeholder="completions.create"
                        />
                    </label>
                    <button class="btn primary" type="submit" disabled=move || busy.get()>
                        {move || if busy.get() { "Creating…" } else { "Create eval" }}
                    </button>
                </div>
                <details class="advanced">
                    <summary>"Optional fields"</summary>
                    <div class="formrow" style="margin-top:8px">
                        <label class="filter">
                            <span>"Purpose"</span>
                            <input
                                aria-label="Purpose"
                                prop:value=move || purpose.get()
                                on:input=move |event| purpose.set(event_target_value(&event))
                            />
                        </label>
                        <label class="filter">
                            <span>"Model ref"</span>
                            <input
                                aria-label="Model ref"
                                prop:value=move || model_ref.get()
                                on:input=move |event| model_ref.set(event_target_value(&event))
                            />
                        </label>
                        <label class="filter">
                            <span>"Prompt ref"</span>
                            <input
                                aria-label="Prompt ref"
                                prop:value=move || prompt_ref.get()
                                on:input=move |event| prompt_ref.set(event_target_value(&event))
                            />
                        </label>
                        <label class="filter">
                            <span>"Trace source"</span>
                            <select
                                aria-label="Trace source"
                                on:change=move |event| trace_source.set(event_target_value(&event))
                            >
                                {TRACE_SOURCES.into_iter().map(|value| view! {
                                    <option value=value selected=move || trace_source.get() == value>{value}</option>
                                }).collect_view()}
                            </select>
                        </label>
                    </div>
                    <label class="filter" style="margin-top:8px">
                        <span>"Inputs schema (JSON)"</span>
                        <textarea
                            aria-label="Inputs schema"
                            rows="3"
                            prop:value=move || inputs_schema.get()
                            on:input=move |event| inputs_schema.set(event_target_value(&event))
                        ></textarea>
                    </label>
                </details>
            </form>
            {move || notice.get().map(|message| view! {
                <p class="muted" style="font-size:11px">{message}</p>
            })}
        </section>
    }
}

// ── detail ──────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
struct EvalData {
    eval: EvalSummary,
    versions: Vec<EvalVersion>,
    version: Option<EvalVersion>,
    metrics: Vec<MetricRow>,
    cases: Vec<CaseRow>,
    runs: Vec<EvalRun>,
    gates: Vec<ApprovalOut>,
    climbs: Vec<EvalClimb>,
    report_run: Option<EvalRun>,
    report: Option<ReportOut>,
    results: Vec<ResultRowOut>,
    results_cursor: Option<String>,
}

fn load_eval<F>(
    ctx: AppCtx,
    eval_id: F,
    version_sig: RwSignal<Option<String>>,
    report_run: RwSignal<Option<String>>,
    split: RwSignal<String>,
) -> LocalResource<Result<EvalData, ApiError>>
where
    F: Fn() -> String + 'static,
{
    LocalResource::new(move || {
        // Every reactive input is read before the first await.
        let eval_id = eval_id();
        let chosen_version = version_sig.get();
        let chosen_run = report_run.get();
        let split_filter = split.get();
        let _tenant = ctx.tenant.get();
        let client = ctx.client();
        async move {
            if eval_id.is_empty() {
                return Err(ApiError::NotFound("no eval in the route".to_owned()));
            }
            let eval: EvalSummary = parse(
                client
                    .invoke(
                        "evals.get",
                        json!({"id": eval_id}),
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                "eval",
            )?;
            let versions: Page<EvalVersion> = parse(
                client
                    .invoke(
                        "eval_versions.list",
                        json!({"eval_id": eval.id}),
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                "eval versions",
            )?;
            let version_id = chosen_version
                .or_else(|| eval.current_version_id.clone())
                .or_else(|| versions.items.first().map(|row| row.id.clone()));
            // `eval_versions.get` is the only read that carries the frozen metrics. When the
            // manifest lacks it, the list row still names the version and the tab offers no
            // metric editor rather than inventing rows.
            let (version, metrics) = match &version_id {
                Some(id) if ctx.has_action("eval_versions.get") => {
                    let detail: VersionDetail = parse(
                        client
                            .invoke(
                                "eval_versions.get",
                                json!({"id": id}),
                                InvokeOptions::default(),
                            )
                            .await?
                            .output,
                        "eval version",
                    )?;
                    (Some(detail.version), detail.metrics)
                }
                Some(id) => (
                    versions.items.iter().find(|row| &row.id == id).cloned(),
                    Vec::new(),
                ),
                None => (None, Vec::new()),
            };
            let cases: Vec<CaseRow> = match &version_id {
                Some(id) => {
                    let page: Page<CaseRow> = parse(
                        client
                            .invoke(
                                "eval_cases.list",
                                json!({"version_id": id, "limit": 200}),
                                InvokeOptions::default(),
                            )
                            .await?
                            .output,
                        "eval cases",
                    )?;
                    page.items
                }
                None => Vec::new(),
            };
            let runs: Vec<EvalRun> = parse::<Page<EvalRun>>(
                client
                    .invoke(
                        "eval_runs.list",
                        json!({"eval_id": eval.id, "limit": 50}),
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                "eval runs",
            )?
            .items;
            let gates: Vec<ApprovalOut> = if ctx.has_action("approvals.list") {
                parse::<Page<ApprovalOut>>(
                    client
                        .invoke(
                            "approvals.list",
                            json!({"eval_id": eval.id, "limit": 20}),
                            InvokeOptions::default(),
                        )
                        .await?
                        .output,
                    "eval gates",
                )?
                .items
            } else {
                Vec::new()
            };
            let climbs: Vec<EvalClimb> = if ctx.has_action("eval_climbs.list") {
                parse::<Page<EvalClimb>>(
                    client
                        .invoke(
                            "eval_climbs.list",
                            json!({"eval_id": eval.id, "limit": 20}),
                            InvokeOptions::default(),
                        )
                        .await?
                        .output,
                    "eval climbs",
                )?
                .items
            } else {
                Vec::new()
            };
            let report = chosen_run.or_else(|| runs.first().map(|row| row.id.clone()));
            let report_run = runs
                .iter()
                .find(|row| Some(row.id.clone()) == report)
                .cloned();
            // The run's figures are the server's: `eval_runs.report` derives the means, the
            // intervals, the noise floor and the cost from the rows. A manifest without it
            // renders no tiles, never a browser-side approximation of the same numbers.
            let report_out: Option<ReportOut> = match &report_run {
                Some(run) if ctx.has_action("eval_runs.report") => Some(parse(
                    client
                        .invoke(
                            "eval_runs.report",
                            json!({"run_id": run.id}),
                            InvokeOptions::default(),
                        )
                        .await?
                        .output,
                    "eval report",
                )?),
                _ => None,
            };
            // One page for the per-case table. Its `next_cursor` is the Load more control's
            // state; the page is never promoted into a run-level number.
            let (results, results_cursor): (Vec<ResultRowOut>, Option<String>) = match &report_run {
                Some(run) if ctx.has_action("eval_results.list") => {
                    let mut input = json!({"run_id": run.id, "limit": RESULTS_PAGE});
                    if !split_filter.is_empty() {
                        input["split"] = json!(split_filter);
                    }
                    let outcome = client
                        .invoke("eval_results.list", input, InvokeOptions::default())
                        .await;
                    match outcome {
                        Ok(invocation) => {
                            let page: Page<ResultRowOut> =
                                parse(invocation.output, "eval results")?;
                            (page.items, page.next_cursor)
                        }
                        // A held-out split is a refusal the tab shows verbatim; it is not an
                        // empty result set and must not be rendered as one.
                        Err(error) => return Err(error),
                    }
                }
                _ => (Vec::new(), None),
            };
            Ok(EvalData {
                eval,
                versions: versions.items,
                version,
                metrics,
                cases,
                runs,
                gates,
                climbs,
                report_run,
                report: report_out,
                results,
                results_cursor,
            })
        }
    })
}

#[component]
pub fn EvalDetail() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let params = use_params_map();
    let eval_id = move || params.get().get("eval_id").unwrap_or_default();
    let tab = move || {
        params
            .get()
            .get("tab")
            .unwrap_or_else(|| "cases".to_owned())
    };
    let version_sig = RwSignal::new(Option::<String>::None);
    let report_run = RwSignal::new(Option::<String>::None);
    let split = RwSignal::new(String::new());
    let data = load_eval(ctx, eval_id, version_sig, report_run, split);
    let refresh = Callback::new(move |_| data.refetch());
    let notice = RwSignal::new(Option::<String>::None);

    view! {
        <section class="screen measure">
            {move || {
                if eval_id().is_empty() {
                    return view! { <p class="muted">"No eval in the route."</p> }.into_any();
                }
                view! {
                    {move || notice.get().map(|message| view! {
                        <div class="callout warn" role="status"><p>{message}</p></div>
                    })}
                    {move || match data.get() {
                        None => view! { <p class="muted">"Reading the instrument…"</p> }.into_any(),
                        Some(Err(error)) => view! { <ErrorPanel error=error on_retry=refresh/> }.into_any(),
                        Some(Ok(body)) => {
                            let current_tab = tab();
                            view! {
                                <EvalHeader
                                    data=body.clone()
                                    tab=current_tab.clone()
                                    refresh=refresh
                                    notice=notice
                                />
                                <EvalBody
                                    data=body
                                    tab=current_tab
                                    version_sig=version_sig
                                    report_run=report_run
                                    split=split
                                    refresh=refresh
                                    notice=notice
                                />
                            }.into_any()
                        },
                    }}
                }
                .into_any()
            }}
        </section>
    }
}

#[component]
fn EvalHeader(
    data: EvalData,
    tab: String,
    refresh: Callback<()>,
    notice: RwSignal<Option<String>>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let eval = data.eval.clone();
    let version = data.version.clone();
    let split_state = version
        .as_ref()
        .and_then(|row| row.split_sha256.clone())
        .map(|sha| format!("split locked {}", short_id(Some(&sha))))
        .unwrap_or_else(|| "split not drawn".to_owned());
    let digest = version
        .as_ref()
        .map(|row| short_id(Some(&row.digest)))
        .unwrap_or_else(|| "—".to_owned());
    let versions = data.versions.len();
    let cases = data.cases.len();
    let eval_id = eval.id.clone();

    let editing = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let retire_armed = RwSignal::new(false);
    let edit_name = RwSignal::new(eval.name.clone());
    let edit_purpose = RwSignal::new(eval.purpose.clone());
    let edit_model_ref = RwSignal::new(eval.model_ref.clone().unwrap_or_default());
    let edit_prompt_ref = RwSignal::new(eval.prompt_ref.clone().unwrap_or_default());
    let edit_schema = RwSignal::new(
        serde_json::to_string(&eval.inputs_schema).unwrap_or_else(|_| "{}".to_owned()),
    );
    let edit_trace = RwSignal::new(if eval.trace_source.is_empty() {
        "synthesized".to_owned()
    } else {
        eval.trace_source.clone()
    });

    let update_id = eval.id.clone();
    let on_save = Callback::new(move |event: web_sys::SubmitEvent| {
        event.prevent_default();
        let name_value = edit_name.get_untracked().trim().to_owned();
        if name_value.is_empty() {
            notice.set(Some("A name is required.".into()));
            return;
        }
        let schema = match parse_json_field(&edit_schema.get_untracked(), "inputs_schema") {
            Ok(Value::Null) => json!({}),
            Ok(value) => value,
            Err(error) => {
                notice.set(Some(error));
                return;
            }
        };
        // `model_ref`/`prompt_ref` send `""` to clear: the patch has no null spelling.
        let input = json!({
            "id": update_id.clone(),
            "name": name_value,
            "purpose": edit_purpose.get_untracked(),
            "model_ref": edit_model_ref.get_untracked(),
            "prompt_ref": edit_prompt_ref.get_untracked(),
            "inputs_schema": schema,
            "trace_source": edit_trace.get_untracked(),
        });
        let ctx = ctx;
        busy.set(true);
        leptos::task::spawn_local(async move {
            match ctx
                .invoke("evals.update", input, InvokeOptions::default())
                .await
            {
                Ok(_) => {
                    editing.set(false);
                    notice.set(Some("Eval updated.".into()));
                    refresh.run(());
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
            busy.set(false);
        });
    });

    let freeze_id = eval.id.clone();
    let on_freeze = Callback::new(move |_: ()| {
        let eval_id = freeze_id.clone();
        let ctx = ctx;
        busy.set(true);
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "eval_versions.create",
                    json!({"eval_id": eval_id}),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(invocation) => {
                    let number = invocation
                        .output
                        .get("version")
                        .and_then(Value::as_i64)
                        .unwrap_or(0);
                    notice.set(Some(format!(
                        "Version v{number} frozen from the current cases and metrics."
                    )));
                    refresh.run(());
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
            busy.set(false);
        });
    });

    // Retirement is a destructive transition, so it takes a second click: the first arms the
    // button, the second retires. A misclick therefore cannot retire an instrument.
    let retire_id = eval.id.clone();
    let on_retire = Callback::new(move |_: ()| {
        if !retire_armed.get_untracked() {
            retire_armed.set(true);
            return;
        }
        let eval_id = retire_id.clone();
        let ctx = ctx;
        busy.set(true);
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "evals.retire",
                    // `EvalIdIn` reads `id`; `eval_id` stays as the alias the slice names.
                    json!({"id": eval_id.clone(), "eval_id": eval_id}),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => {
                    notice.set(Some("Eval retired.".into()));
                    refresh.run(());
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
            busy.set(false);
        });
    });

    view! {
        <header class="screen-head">
            <div>
                <h1>{format!("{} · v{}", eval.name, version.as_ref().map(|row| row.version).unwrap_or(0))}</h1>
                <p class="muted">
                    {format!("Digest {digest} · {cases} cases · {versions} version(s) · {split_state}")}
                </p>
            </div>
            <div class="filterbar">
                <StatusPill status=eval.status.clone()/>
                {move || ctx.has_action("eval_versions.create").then(|| view! {
                    <button
                        class="btn small"
                        on:click=move |_| on_freeze.run(())
                        disabled=move || busy.get()
                    >
                        "Freeze the next version"
                    </button>
                })}
                {move || ctx.has_action("evals.update").then(|| view! {
                    <button class="btn small" on:click=move |_| editing.update(|open| *open = !*open)>
                        "Edit"
                    </button>
                })}
                {move || ctx.has_action("evals.retire").then(|| view! {
                    <button
                        class=move || if retire_armed.get() { "btn small danger" } else { "btn small" }
                        on:click=move |_| on_retire.run(())
                        disabled=move || busy.get()
                    >
                        {move || if retire_armed.get() { "Confirm retire" } else { "Retire" }}
                    </button>
                })}
                {move || retire_armed.get().then(|| view! {
                    <button class="btn small" on:click=move |_| retire_armed.set(false)>"Cancel"</button>
                })}
                <A href="/evals"><span class="btn small">"All evals"</span></A>
            </div>
        </header>
        <nav class="tabs" aria-label="Eval tabs">
            {TABS.into_iter().map(|(key, label)| {
                let href = format!("/evals/{eval_id}/{key}");
                let active = key == tab;
                view! {
                    <A href=href>
                        <span class={if active { "tab on" } else { "tab" }}>{label}</span>
                    </A>
                }
            }).collect_view()}
        </nav>
        {move || {
            let allowed = ctx.has_action("evals.update");
            (editing.get() && allowed).then(|| view! {
            <section class="panel" style="margin-top:10px">
                <div class="ph">
                    <span>"Edit eval"</span>
                    <span class="muted mono">"evals.update"</span>
                </div>
                <form on:submit=move |event| on_save.run(event)>
                    <div class="formrow">
                        <label class="filter">
                            <span>"Name"</span>
                            <input
                                aria-label="Edit name"
                                required
                                prop:value=move || edit_name.get()
                                on:input=move |event| edit_name.set(event_target_value(&event))
                            />
                        </label>
                        <label class="filter">
                            <span>"Purpose"</span>
                            <input
                                aria-label="Edit purpose"
                                prop:value=move || edit_purpose.get()
                                on:input=move |event| edit_purpose.set(event_target_value(&event))
                            />
                        </label>
                        <label class="filter">
                            <span>"Trace source"</span>
                            <select
                                aria-label="Edit trace source"
                                on:change=move |event| edit_trace.set(event_target_value(&event))
                            >
                                {TRACE_SOURCES.into_iter().map(|value| view! {
                                    <option value=value selected=move || edit_trace.get() == value>{value}</option>
                                }).collect_view()}
                            </select>
                        </label>
                    </div>
                    <div class="formrow" style="margin-top:8px">
                        <label class="filter">
                            <span>"Model ref"</span>
                            <input
                                aria-label="Edit model ref"
                                prop:value=move || edit_model_ref.get()
                                on:input=move |event| edit_model_ref.set(event_target_value(&event))
                            />
                        </label>
                        <label class="filter">
                            <span>"Prompt ref"</span>
                            <input
                                aria-label="Edit prompt ref"
                                prop:value=move || edit_prompt_ref.get()
                                on:input=move |event| edit_prompt_ref.set(event_target_value(&event))
                            />
                        </label>
                        <button class="btn primary" type="submit" disabled=move || busy.get()>
                            "Save eval"
                        </button>
                    </div>
                    <label class="filter" style="margin-top:8px">
                        <span>"Inputs schema (JSON)"</span>
                        <textarea
                            aria-label="Edit inputs schema"
                            rows="4"
                            prop:value=move || edit_schema.get()
                            on:input=move |event| edit_schema.set(event_target_value(&event))
                        ></textarea>
                    </label>
                </form>
            </section>
            })
        }}
    }
}

#[component]
fn EvalBody(
    data: EvalData,
    tab: String,
    version_sig: RwSignal<Option<String>>,
    report_run: RwSignal<Option<String>>,
    split: RwSignal<String>,
    refresh: Callback<()>,
    notice: RwSignal<Option<String>>,
) -> impl IntoView {
    match tab.as_str() {
        "runs" => view! { <RunsTab data=data refresh=refresh notice=notice/> }.into_any(),
        "report" => view! {
            <ReportTab data=data report_run=report_run split=split notice=notice/>
        }
        .into_any(),
        "sign-off" => view! { <SignoffTab data=data refresh=refresh notice=notice/> }.into_any(),
        "climb" => view! { <ClimbTab data=data refresh=refresh notice=notice/> }.into_any(),
        "audit" => view! {
            <AuditTab
                eval_id=data.eval.id.clone()
                version=data.version.clone()
                runs=data.runs.clone()
                refresh=refresh
                notice=notice
            />
        }
        .into_any(),
        "interop" => view! {
            <InteropTab version=data.version.clone() runs=data.runs.clone() notice=notice/>
        }
        .into_any(),
        _ => view! {
            <CasesTab data=data version_sig=version_sig refresh=refresh notice=notice/>
        }
        .into_any(),
    }
}

// ── cases ───────────────────────────────────────────────────────────────────────────────

#[component]
fn CasesTab(
    data: EvalData,
    version_sig: RwSignal<Option<String>>,
    refresh: Callback<()>,
    notice: RwSignal<Option<String>>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let cases = data.cases.clone();
    let metrics = data.metrics.clone();
    let versions = data.versions.clone();
    let locked = data
        .version
        .as_ref()
        .map(|row| row.split_sha256.is_some())
        .unwrap_or(false);
    let eval_id = data.eval.id.clone();
    let version_now = data
        .version
        .as_ref()
        .map(|row| row.id.clone())
        .unwrap_or_default();
    let has_version = !version_now.is_empty();
    let can_draw = move || ctx.has_action("evals.draw_split") && !locked;
    let draw_eval = eval_id.clone();
    let on_draw = move |_| {
        let eval_id = draw_eval.clone();
        let ctx = ctx;
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "evals.draw_split",
                    json!({"eval_id": eval_id, "train_ratio": 0.7}),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => refresh.run(()),
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    };

    // ── add a case ──
    let new_input = RwSignal::new(String::new());
    let new_tags = RwSignal::new(String::new());
    let new_source = RwSignal::new("handwritten".to_owned());
    let new_gold = RwSignal::new("human_written".to_owned());
    let new_gold_model = RwSignal::new(String::new());
    let new_expected = RwSignal::new(String::new());
    let new_meta = RwSignal::new(String::new());
    let case_busy = RwSignal::new(false);

    let create_version = version_now.clone();
    let add_case = Callback::new(move |_: ()| {
        if create_version.is_empty() {
            notice.set(Some("Freeze a version before adding a case.".into()));
            return;
        }
        let input_value = new_input.get_untracked().trim().to_owned();
        if input_value.is_empty() {
            notice.set(Some("A case input is required.".into()));
            return;
        }
        let expected = match parse_json_field(&new_expected.get_untracked(), "expected") {
            Ok(Value::Null) => None,
            Ok(value) => Some(value),
            Err(error) => {
                notice.set(Some(error));
                return;
            }
        };
        let meta = match parse_json_field(&new_meta.get_untracked(), "meta") {
            Ok(Value::Null) => json!({}),
            Ok(value) => value,
            Err(error) => {
                notice.set(Some(error));
                return;
            }
        };
        let gold_model_value = new_gold_model.get_untracked().trim().to_owned();
        let mut case = json!({
            "input": input_value,
            "tags": parse_tags(&new_tags.get_untracked()),
            "attachments": [],
            "source": new_source.get_untracked(),
            "gold_provenance": new_gold.get_untracked(),
            "meta": meta,
        });
        if let Some(expected) = expected {
            case["expected"] = expected;
        }
        if !gold_model_value.is_empty() {
            case["gold_model"] = json!(gold_model_value);
        }
        // `CaseCreateIn` names the case as one field but flattens it on the wire. Sending
        // both the nested document and the flat fields means either spelling deserializes.
        let mut input = json!({"version_id": create_version.clone(), "case": case.clone()});
        if let Some(object) = case.as_object() {
            for (key, value) in object {
                input[key.as_str()] = value.clone();
            }
        }
        let ctx = ctx;
        case_busy.set(true);
        leptos::task::spawn_local(async move {
            match ctx
                .invoke("eval_cases.create", input, InvokeOptions::default())
                .await
            {
                Ok(_) => {
                    new_input.set(String::new());
                    new_tags.set(String::new());
                    new_expected.set(String::new());
                    new_meta.set(String::new());
                    new_gold_model.set(String::new());
                    notice.set(Some("Case added.".into()));
                    refresh.run(());
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
            case_busy.set(false);
        });
    });

    // ── edit a case ──
    let editing_case = RwSignal::new(Option::<String>::None);
    let edit_input = RwSignal::new(String::new());
    let edit_tags = RwSignal::new(String::new());
    let edit_source = RwSignal::new("handwritten".to_owned());
    let edit_gold = RwSignal::new("human_written".to_owned());
    let edit_gold_model = RwSignal::new(String::new());
    let edit_expected = RwSignal::new(String::new());
    let edit_meta = RwSignal::new(String::new());
    let edit_attachments = RwSignal::new(Value::Null);

    let open_case_edit = move |row: CaseRow| {
        editing_case.set(Some(row.id.clone()));
        edit_input.set(row.input.clone());
        edit_tags.set(row.tags.join(", "));
        edit_source.set(if row.source.is_empty() {
            "handwritten".to_owned()
        } else {
            row.source.clone()
        });
        edit_gold.set(if row.gold_provenance.is_empty() {
            "human_written".to_owned()
        } else {
            row.gold_provenance.clone()
        });
        edit_gold_model.set(row.gold_model.clone().unwrap_or_default());
        edit_expected.set(String::new());
        edit_meta.set(serde_json::to_string(&row.meta).unwrap_or_else(|_| "{}".to_owned()));
        edit_attachments.set(row.attachments.clone());
    };

    let save_case = Callback::new(move |_: ()| {
        let Some(id) = editing_case.get_untracked() else {
            return;
        };
        let input_value = edit_input.get_untracked().trim().to_owned();
        if input_value.is_empty() {
            notice.set(Some("A case input is required.".into()));
            return;
        }
        let expected = match parse_json_field(&edit_expected.get_untracked(), "expected") {
            Ok(Value::Null) => None,
            Ok(value) => Some(value),
            Err(error) => {
                notice.set(Some(error));
                return;
            }
        };
        let meta = match parse_json_field(&edit_meta.get_untracked(), "meta") {
            Ok(Value::Null) => json!({}),
            Ok(value) => value,
            Err(error) => {
                notice.set(Some(error));
                return;
            }
        };
        let gold_model_value = edit_gold_model.get_untracked().trim().to_owned();
        // The update is a replacement, so every content field is sent, `attachments`
        // included; the two nullable fields are omitted only when they are being cleared.
        let mut input = json!({
            "id": id,
            "input": input_value,
            "tags": parse_tags(&edit_tags.get_untracked()),
            "attachments": edit_attachments.get_untracked(),
            "source": edit_source.get_untracked(),
            "gold_provenance": edit_gold.get_untracked(),
            "meta": meta,
        });
        if let Some(expected) = expected {
            input["expected"] = expected;
        }
        if !gold_model_value.is_empty() {
            input["gold_model"] = json!(gold_model_value);
        }
        let ctx = ctx;
        case_busy.set(true);
        leptos::task::spawn_local(async move {
            match ctx
                .invoke("eval_cases.update", input, InvokeOptions::default())
                .await
            {
                Ok(_) => {
                    editing_case.set(None);
                    notice.set(Some("Case updated.".into()));
                    refresh.run(());
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
            case_busy.set(false);
        });
    });

    let delete_version = version_now.clone();
    let delete_case = Callback::new(move |id: String| {
        let version_id = delete_version.clone();
        let ctx = ctx;
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "eval_cases.delete",
                    json!({"version_id": version_id, "id": id}),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => {
                    notice.set(Some("Case deleted.".into()));
                    refresh.run(());
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    });

    // ── the metric editor ──
    let metric_key = RwSignal::new(String::new());
    let metric_label = RwSignal::new(String::new());
    let metric_unit = RwSignal::new(String::new());
    let metric_role = RwSignal::new("quality".to_owned());
    let metric_kind = RwSignal::new("binary".to_owned());
    let metric_better = RwSignal::new("higher".to_owned());
    let metric_grader_kind = RwSignal::new("programmatic".to_owned());
    let metric_grader = RwSignal::new("{}".to_owned());
    let metric_rubric = RwSignal::new(String::new());
    let metric_editing = RwSignal::new(Option::<String>::None);
    let metric_ordinal = RwSignal::new(0_i32);
    let metric_busy = RwSignal::new(false);

    let reset_metric_form = Callback::new(move |_: ()| {
        metric_editing.set(None);
        metric_key.set(String::new());
        metric_label.set(String::new());
        metric_unit.set(String::new());
        metric_rubric.set(String::new());
        metric_grader.set("{}".to_owned());
        metric_ordinal.set(0);
        metric_role.set("quality".to_owned());
        metric_kind.set("binary".to_owned());
        metric_better.set("higher".to_owned());
        metric_grader_kind.set("programmatic".to_owned());
    });

    let open_metric_edit = move |row: MetricRow| {
        metric_editing.set(Some(row.id.clone()));
        metric_key.set(row.key.clone());
        metric_role.set(if row.role.is_empty() {
            "quality".to_owned()
        } else {
            row.role.clone()
        });
        metric_kind.set(if row.kind.is_empty() {
            "binary".to_owned()
        } else {
            row.kind.clone()
        });
        metric_better.set(if row.better.is_empty() {
            "higher".to_owned()
        } else {
            row.better.clone()
        });
        metric_label.set(row.label.clone());
        metric_unit.set(row.unit.clone().unwrap_or_default());
        metric_grader_kind.set(if row.grader_kind.is_empty() {
            "programmatic".to_owned()
        } else {
            row.grader_kind.clone()
        });
        metric_grader.set(serde_json::to_string(&row.grader).unwrap_or_else(|_| "{}".to_owned()));
        metric_rubric.set(row.rubric.clone().unwrap_or_default());
        metric_ordinal.set(row.ordinal);
    };

    let metric_version = version_now.clone();
    let save_metric = Callback::new(move |_: ()| {
        let key_value = metric_key.get_untracked().trim().to_owned();
        let label_value = metric_label.get_untracked().trim().to_owned();
        if key_value.is_empty() {
            notice.set(Some("A metric key is required.".into()));
            return;
        }
        if label_value.is_empty() {
            notice.set(Some("A metric label is required.".into()));
            return;
        }
        let grader = match parse_json_field(&metric_grader.get_untracked(), "grader") {
            Ok(Value::Null) => json!({}),
            Ok(value) => value,
            Err(error) => {
                notice.set(Some(error));
                return;
            }
        };
        let editing_id = metric_editing.get_untracked();
        let was_edit = editing_id.is_some();
        let mut input = json!({
            "key": key_value,
            "role": metric_role.get_untracked(),
            "kind": metric_kind.get_untracked(),
            "better": metric_better.get_untracked(),
            "label": label_value,
            "grader_kind": metric_grader_kind.get_untracked(),
            "grader": grader,
        });
        let unit_value = metric_unit.get_untracked().trim().to_owned();
        if !unit_value.is_empty() {
            input["unit"] = json!(unit_value);
        }
        let rubric_value = metric_rubric.get_untracked().trim().to_owned();
        if !rubric_value.is_empty() {
            input["rubric"] = json!(rubric_value);
        }
        let action = if let Some(id) = editing_id {
            input["id"] = json!(id);
            input["ordinal"] = json!(metric_ordinal.get_untracked());
            "eval_metrics.update"
        } else {
            if metric_version.is_empty() {
                notice.set(Some("Freeze a version before adding a metric.".into()));
                return;
            }
            input["version_id"] = json!(metric_version.clone());
            "eval_metrics.create"
        };
        let ctx = ctx;
        metric_busy.set(true);
        leptos::task::spawn_local(async move {
            match ctx.invoke(action, input, InvokeOptions::default()).await {
                Ok(_) => {
                    reset_metric_form.run(());
                    notice.set(Some(if was_edit {
                        "Metric updated.".to_owned()
                    } else {
                        "Metric added.".to_owned()
                    }));
                    refresh.run(());
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
            metric_busy.set(false);
        });
    });

    let delete_metric_version = version_now.clone();
    let delete_metric = Callback::new(move |id: String| {
        let version_id = delete_metric_version.clone();
        let ctx = ctx;
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "eval_metrics.delete",
                    json!({"version_id": version_id, "id": id}),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => {
                    notice.set(Some("Metric deleted.".into()));
                    refresh.run(());
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    });

    view! {
        <div class="panel" style="margin-top:12px">
            <div class="ph">
                <span>"Cases"</span>
                <span class="muted">
                    {format!("{} on this version · {}", cases.len(), if locked { "split locked" } else { "split open" })}
                </span>
            </div>
            {if versions.len() > 1 {
                view! {
                    <label class="filter" style="margin-bottom:8px">
                        <span>"Version"</span>
                        <select
                            aria-label="Version"
                            on:change=move |event| {
                                if let Some(select) = event.target().and_then(|target| target.dyn_into::<web_sys::HtmlSelectElement>().ok()) {
                                    version_sig.set(Some(select.value()));
                                }
                            }
                        >
                            {versions.clone().into_iter().map(|row| {
                                let value = row.id.clone();
                                let selected_value = value.clone();
                                let current = version_now.clone();
                                view! {
                                    <option value=value selected=move || current == selected_value>
                                        {format!("v{} · {}", row.version, short_id(Some(&row.digest)))}
                                    </option>
                                }
                            }).collect_view()}
                        </select>
                    </label>
                }.into_any()
            } else {
                ().into_any()
            }}
            {if cases.is_empty() {
                view! { <Empty message="This version has no cases.".to_owned()/> }.into_any()
            } else {
                view! {
                    <table class="wtbl">
                        <thead><tr>
                            <th class="n">"#"</th><th>"Case"</th><th>"Tags"</th>
                            <th>"Split"</th><th>"Source"</th><th>"Gold"</th><th></th>
                        </tr></thead>
                        <tbody>
                            {cases.clone().into_iter().map(|case| {
                                let edit_row = case.clone();
                                let delete_id = case.id.clone();
                                let open = open_case_edit;
                                view! {
                                    <tr>
                                        <td class="n">{case.ordinal + 1}</td>
                                        <td>{excerpt(&case.input, 90)}</td>
                                        <td>{case.tags.clone().into_iter().map(|tag| view! { <span class="lbl">{tag}</span> }).collect_view()}</td>
                                        <td>{split_pill(&case.split)}</td>
                                        <td class="mono">{case.source.clone()}</td>
                                        <td class="mono">{case.gold_provenance.clone()}</td>
                                        <td>
                                            {ctx.has_action("eval_cases.update").then(|| view! {
                                                <button class="btn small" on:click=move |_| open(edit_row.clone())>"Edit"</button>
                                            })}
                                            {ctx.has_action("eval_cases.delete").then(|| view! {
                                                <button class="btn small danger" on:click=move |_| delete_case.run(delete_id.clone())>"Delete"</button>
                                            })}
                                        </td>
                                    </tr>
                                }
                            }).collect_view()}
                        </tbody>
                    </table>
                }.into_any()
            }}
            {move || (has_version && ctx.has_action("eval_cases.create")).then(|| view! {
                <section class="panel" style="margin-top:10px">
                    <div class="ph">
                        <span>"Add a case"</span>
                        <span class="muted mono">"eval_cases.create"</span>
                    </div>
                    <div class="formrow">
                        <label class="filter">
                            <span>"Input"</span>
                            <textarea
                                aria-label="Case input"
                                rows="2"
                                prop:value=move || new_input.get()
                                on:input=move |event| new_input.set(event_target_value(&event))
                                placeholder="The input the subject is shown"
                            ></textarea>
                        </label>
                        <label class="filter">
                            <span>"Tags"</span>
                            <input
                                aria-label="Case tags"
                                prop:value=move || new_tags.get()
                                on:input=move |event| new_tags.set(event_target_value(&event))
                                placeholder="smoke, math"
                            />
                        </label>
                    </div>
                    <div class="formrow" style="margin-top:8px">
                        <label class="filter">
                            <span>"Source"</span>
                            <select
                                aria-label="Case source"
                                on:change=move |event| new_source.set(event_target_value(&event))
                            >
                                {CASE_SOURCES.into_iter().map(|value| view! {
                                    <option value=value selected=move || new_source.get() == value>{value}</option>
                                }).collect_view()}
                            </select>
                        </label>
                        <label class="filter">
                            <span>"Gold provenance"</span>
                            <select
                                aria-label="Gold provenance"
                                on:change=move |event| new_gold.set(event_target_value(&event))
                            >
                                {GOLD_PROVENANCES.into_iter().map(|value| view! {
                                    <option value=value selected=move || new_gold.get() == value>{value}</option>
                                }).collect_view()}
                            </select>
                        </label>
                        <label class="filter">
                            <span>"Gold model"</span>
                            <input
                                aria-label="Gold model"
                                prop:value=move || new_gold_model.get()
                                on:input=move |event| new_gold_model.set(event_target_value(&event))
                                placeholder="required only for model provenance"
                            />
                        </label>
                        <button
                            class="btn primary"
                            on:click=move |_| add_case.run(())
                            disabled=move || case_busy.get()
                        >
                            "Add case"
                        </button>
                    </div>
                    <details class="advanced">
                        <summary>"Answer key and metadata"</summary>
                        <label class="filter" style="margin-top:8px">
                            <span>"Expected (JSON, write-only)"</span>
                            <textarea
                                aria-label="Expected JSON"
                                rows="3"
                                prop:value=move || new_expected.get()
                                on:input=move |event| new_expected.set(event_target_value(&event))
                                placeholder="{\"answer\": \"42\"} — empty leaves no key"
                            ></textarea>
                        </label>
                        <label class="filter" style="margin-top:8px">
                            <span>"Meta (JSON)"</span>
                            <textarea
                                aria-label="Case meta"
                                rows="2"
                                prop:value=move || new_meta.get()
                                on:input=move |event| new_meta.set(event_target_value(&event))
                                placeholder="{}"
                            ></textarea>
                        </label>
                    </details>
                </section>
            })}
            {move || {
                let allowed = ctx.has_action("eval_cases.update");
                editing_case.get().filter(|_| allowed).map(|case_id| view! {
                <section class="panel" style="margin-top:10px">
                    <div class="ph">
                        <span>"Edit case"</span>
                        <span class="muted mono">{short_id(Some(&case_id))}</span>
                    </div>
                    <div class="formrow">
                        <label class="filter">
                            <span>"Input"</span>
                            <textarea
                                aria-label="Edit input"
                                rows="2"
                                prop:value=move || edit_input.get()
                                on:input=move |event| edit_input.set(event_target_value(&event))
                            ></textarea>
                        </label>
                        <label class="filter">
                            <span>"Tags"</span>
                            <input
                                aria-label="Edit tags"
                                prop:value=move || edit_tags.get()
                                on:input=move |event| edit_tags.set(event_target_value(&event))
                            />
                        </label>
                    </div>
                    <div class="formrow" style="margin-top:8px">
                        <label class="filter">
                            <span>"Source"</span>
                            <select
                                aria-label="Edit source"
                                on:change=move |event| edit_source.set(event_target_value(&event))
                            >
                                {CASE_SOURCES.into_iter().map(|value| view! {
                                    <option value=value selected=move || edit_source.get() == value>{value}</option>
                                }).collect_view()}
                            </select>
                        </label>
                        <label class="filter">
                            <span>"Gold provenance"</span>
                            <select
                                aria-label="Edit gold provenance"
                                on:change=move |event| edit_gold.set(event_target_value(&event))
                            >
                                {GOLD_PROVENANCES.into_iter().map(|value| view! {
                                    <option value=value selected=move || edit_gold.get() == value>{value}</option>
                                }).collect_view()}
                            </select>
                        </label>
                        <label class="filter">
                            <span>"Gold model"</span>
                            <input
                                aria-label="Edit gold model"
                                prop:value=move || edit_gold_model.get()
                                on:input=move |event| edit_gold_model.set(event_target_value(&event))
                            />
                        </label>
                    </div>
                    <details class="advanced">
                        <summary>"Answer key and metadata"</summary>
                        <label class="filter" style="margin-top:8px">
                            <span>"Expected (JSON, write-only)"</span>
                            <textarea
                                aria-label="Edit expected"
                                rows="3"
                                prop:value=move || edit_expected.get()
                                on:input=move |event| edit_expected.set(event_target_value(&event))
                                placeholder="empty clears the key"
                            ></textarea>
                        </label>
                        <label class="filter" style="margin-top:8px">
                            <span>"Meta (JSON)"</span>
                            <textarea
                                aria-label="Edit meta"
                                rows="2"
                                prop:value=move || edit_meta.get()
                                on:input=move |event| edit_meta.set(event_target_value(&event))
                            ></textarea>
                        </label>
                    </details>
                    <div style="display:flex;gap:6px;margin-top:8px">
                        <button
                            class="btn primary"
                            on:click=move |_| save_case.run(())
                            disabled=move || case_busy.get()
                        >
                            "Save case"
                        </button>
                        <button class="btn" on:click=move |_| editing_case.set(None)>"Cancel"</button>
                    </div>
                </section>
                })
            }}
            <div style="display:flex;gap:8px;margin-top:10px;align-items:center">
                {locked.then(|| view! {
                    <span class="muted" style="font-size:11px">
                        "The split is locked; a re-draw is refused (409 split_locked). Editing a case voids the sign-offs until they are re-signed."
                    </span>
                })}
                {can_draw().then(|| view! {
                    <button class="btn small" on:click=on_draw>"Draw split (0.7 train)"</button>
                })}
                <span class="adv muted mono">"eval_cases.list|create|update|delete · evals.draw_split"</span>
            </div>
        </div>
        <section class="panel" style="margin-top:12px">
            <div class="ph">
                <span>"Metrics"</span>
                <span class="muted">
                    {if has_version {
                        format!("{} on this version", metrics.len())
                    } else {
                        "no frozen version".to_owned()
                    }}
                </span>
            </div>
            <p class="muted" style="font-size:11px">
                "A version freezes its metrics. A quality metric is graded; a perf metric is measured and must use grader none. Every write reopens both sign-offs."
            </p>
            {if !has_version {
                view! { <Empty message="Freeze a version first; metrics are frozen with it.".to_owned()/> }.into_any()
            } else if metrics.is_empty() {
                view! { <Empty message="No metric on this version yet — a run needs at least one to grade.".to_owned()/> }.into_any()
            } else {
                view! {
                    <div class="tablewrap">
                        <table class="table">
                            <thead><tr>
                                <th>"Key"</th><th>"Role"</th><th>"Kind"</th><th>"Better"</th>
                                <th>"Label"</th><th>"Unit"</th><th>"Grader"</th>
                                <th class="n">"Ordinal"</th><th></th>
                            </tr></thead>
                            <tbody>
                                {metrics.clone().into_iter().map(|row| {
                                    let edit_row = row.clone();
                                    let delete_id = row.id.clone();
                                    let open = open_metric_edit;
                                    view! {
                                        <tr>
                                            <td class="mono">{row.key.clone()}</td>
                                            <td>{row.role.clone()}</td>
                                            <td>{row.kind.clone()}</td>
                                            <td>{row.better.clone()}</td>
                                            <td>{row.label.clone()}</td>
                                            <td class="mono">{row.unit.clone().unwrap_or_else(|| "—".to_owned())}</td>
                                            <td class="mono">
                                                {row.grader_kind.clone()}
                                                {row.rubric.clone().map(|rubric| view! {
                                                    <span class="adv muted">" · "{excerpt(&rubric, 40)}</span>
                                                })}
                                            </td>
                                            <td class="n">{row.ordinal}</td>
                                            <td>
                                                {ctx.has_action("eval_metrics.update").then(|| view! {
                                                    <button class="btn small" on:click=move |_| open(edit_row.clone())>"Edit"</button>
                                                })}
                                                {ctx.has_action("eval_metrics.delete").then(|| view! {
                                                    <button class="btn small danger" on:click=move |_| delete_metric.run(delete_id.clone())>"Delete"</button>
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
            {move || {
                let editing = metric_editing.get().is_some();
                let allowed = if editing {
                    ctx.has_action("eval_metrics.update")
                } else {
                    ctx.has_action("eval_metrics.create")
                };
                (has_version && allowed).then(|| view! {
                    <div style="margin-top:10px;border-top:1px solid var(--line);padding-top:10px">
                        <div class="ph">
                            <span>{if editing { "Edit metric" } else { "Add a metric" }}</span>
                            <span class="muted mono">"eval_metrics.create|update"</span>
                        </div>
                        <div class="formrow">
                            <label class="filter">
                                <span>"Key"</span>
                                <input
                                    aria-label="Metric key"
                                    prop:value=move || metric_key.get()
                                    on:input=move |event| metric_key.set(event_target_value(&event))
                                    placeholder="correct"
                                />
                            </label>
                            <label class="filter">
                                <span>"Label"</span>
                                <input
                                    aria-label="Metric label"
                                    maxlength="14"
                                    prop:value=move || metric_label.get()
                                    on:input=move |event| metric_label.set(event_target_value(&event))
                                />
                            </label>
                            <label class="filter">
                                <span>"Unit"</span>
                                <input
                                    aria-label="Metric unit"
                                    prop:value=move || metric_unit.get()
                                    on:input=move |event| metric_unit.set(event_target_value(&event))
                                />
                            </label>
                            <label class="filter">
                                <span>"Role"</span>
                                <select
                                    aria-label="Metric role"
                                    on:change=move |event| {
                                        let value = event_target_value(&event);
                                        if value == "perf" {
                                            metric_grader_kind.set("none".to_owned());
                                        } else if metric_grader_kind.get_untracked() == "none" {
                                            metric_grader_kind.set("programmatic".to_owned());
                                        }
                                        metric_role.set(value);
                                    }
                                >
                                    {METRIC_ROLES.into_iter().map(|value| view! {
                                        <option value=value selected=move || metric_role.get() == value>{value}</option>
                                    }).collect_view()}
                                </select>
                            </label>
                        </div>
                        <div class="formrow" style="margin-top:8px">
                            <label class="filter">
                                <span>"Kind"</span>
                                <select
                                    aria-label="Metric kind"
                                    on:change=move |event| metric_kind.set(event_target_value(&event))
                                >
                                    {METRIC_KINDS.into_iter().map(|value| view! {
                                        <option value=value selected=move || metric_kind.get() == value>{value}</option>
                                    }).collect_view()}
                                </select>
                            </label>
                            <label class="filter">
                                <span>"Better"</span>
                                <select
                                    aria-label="Metric better"
                                    on:change=move |event| metric_better.set(event_target_value(&event))
                                >
                                    {METRIC_BETTER.into_iter().map(|value| view! {
                                        <option value=value selected=move || metric_better.get() == value>{value}</option>
                                    }).collect_view()}
                                </select>
                            </label>
                            <label class="filter">
                                <span>"Grader kind"</span>
                                <select
                                    aria-label="Metric grader kind"
                                    on:change=move |event| metric_grader_kind.set(event_target_value(&event))
                                >
                                    {GRADER_KINDS.into_iter().map(|value| view! {
                                        <option value=value selected=move || metric_grader_kind.get() == value>{value}</option>
                                    }).collect_view()}
                                </select>
                            </label>
                            <button
                                class="btn primary"
                                on:click=move |_| save_metric.run(())
                                disabled=move || metric_busy.get()
                            >
                                {move || if metric_editing.get().is_some() { "Save metric" } else { "Add metric" }}
                            </button>
                            {move || metric_editing.get().is_some().then(|| view! {
                                <button class="btn" on:click=move |_| reset_metric_form.run(())>"Cancel"</button>
                            })}
                        </div>
                        <label class="filter" style="margin-top:8px">
                            <span>"Grader config (JSON)"</span>
                            <textarea
                                aria-label="Grader JSON"
                                rows="3"
                                prop:value=move || metric_grader.get()
                                on:input=move |event| metric_grader.set(event_target_value(&event))
                                placeholder="{\"expect\": \"exact\"}"
                            ></textarea>
                        </label>
                        <label class="filter" style="margin-top:8px">
                            <span>"Rubric"</span>
                            <textarea
                                aria-label="Metric rubric"
                                rows="2"
                                prop:value=move || metric_rubric.get()
                                on:input=move |event| metric_rubric.set(event_target_value(&event))
                                placeholder="required by rubric and pairwise graders"
                            ></textarea>
                        </label>
                    </div>
                })
            }}
        </section>
    }
}

// ── runs ────────────────────────────────────────────────────────────────────────────────

#[component]
fn RunsTab(
    data: EvalData,
    refresh: Callback<()>,
    notice: RwSignal<Option<String>>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let eval_id = data.eval.id.clone();
    let eval_id_for_rows = data.eval.id.clone();
    let runs = data.runs.clone();
    let reps = RwSignal::new("1".to_owned());
    let mode = RwSignal::new("full".to_owned());
    let can_start = move || ctx.has_action("eval_runs.start");
    let can_list = ctx.has_action("eval_results.list");
    let can_label = ctx.has_action("eval_results.label");
    let cases = data.cases.clone();
    let open_run = RwSignal::new(Option::<String>::None);
    let open_rows = RwSignal::new(Option::<Result<Vec<ResultRowOut>, ApiError>>::None);
    let open_cursor = RwSignal::new(Option::<String>::None);

    // One run's results, read by the action and paged by its own cursor. The rows carry the
    // person's score, and a successful label reloads this list rather than the whole screen —
    // refetching the page under the panel would close the panel the person is working in.
    let load_results = move |run_id: String| {
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "eval_results.list",
                    json!({"run_id": run_id, "limit": RESULTS_PAGE}),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(invocation) => {
                    match parse::<Page<ResultRowOut>>(invocation.output, "eval results") {
                        Ok(page) => {
                            open_cursor.set(page.next_cursor);
                            open_rows.set(Some(Ok(page.items)));
                        }
                        Err(error) => open_rows.set(Some(Err(error))),
                    }
                }
                Err(error) => {
                    notice.set(Some(error.to_string()));
                    open_rows.set(Some(Err(error)));
                }
            }
        });
    };

    let start_eval = eval_id.clone();
    let on_start = move |_| {
        let eval_id = start_eval.clone();
        let reps_value = reps.get_untracked().parse::<i32>().unwrap_or(1).max(1);
        let mode_value = mode.get_untracked();
        let ctx = ctx;
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "eval_runs.start",
                    json!({"eval_id": eval_id, "reps": reps_value, "mode": mode_value}),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(invocation) => {
                    let queued = invocation
                        .output
                        .get("queued_jobs")
                        .and_then(Value::as_i64)
                        .unwrap_or(0);
                    notice.set(Some(format!("Run started; {queued} slot(s) queued.")));
                    refresh.run(());
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    };

    view! {
        <div class="panel" style="margin-top:12px">
            <div class="ph"><span>"Runs"</span><span class="muted">{runs.len()}</span></div>
            {if runs.is_empty() {
                view! { <Empty message="No run yet. A run needs both sign-offs and a locked split (see the Sign-off tab).".to_owned()/> }.into_any()
            } else {
                view! {
                    <table class="wtbl">
                        <thead><tr>
                            <th>"Variant"</th><th>"Mode"</th><th>"Status"</th><th class="n">"Round"</th>
                            <th class="n">"Cases"</th><th class="n">"Reps"</th><th>"Started"</th><th></th>
                        </tr></thead>
                        <tbody>
                            {runs.clone().into_iter().map(|run| {
                                let id = run.id.clone();
                                let report_href = format!("/evals/{}/report", eval_id_for_rows);
                                let cancel_id = id.clone();
                                let cancel_ctx = ctx;
                                let cancel_refresh = refresh;
                                let cancel_notice = notice;
                                let regrade_id = id.clone();
                                let regrade_ctx = ctx;
                                let regrade_refresh = refresh;
                                let regrade_notice = notice;
                                let results_id = id.clone();
                                let results_click = results_id.clone();
                                let results_signal = open_run;
                                let results_load = load_results;
                                view! {
                                    <tr>
                                        <td class="mono">{run.variant.clone()}</td>
                                        <td class="mono">{run.mode.clone()}</td>
                                        <td><StatusPill status=run.status.clone()/></td>
                                        <td class="n">{run.round}</td>
                                        <td class="n">{run.case_count}</td>
                                        <td class="n">{run.reps}</td>
                                        <td>{run.started_at.map(|at| age(Some(at))).unwrap_or_else(|| "—".to_owned())}</td>
                                        <td>
                                            <A href=report_href><span class="btn small">"Report"</span></A>
                                            {can_list.then(|| view! {
                                                <button class="btn small" on:click=move |_| {
                                                    results_signal.set(Some(results_id.clone()));
                                                    results_load(results_click.clone());
                                                }>"Results"</button>
                                            })}
                                            {(run.status == "queued" || run.status == "running").then(|| view! {
                                                <button class="btn small danger" on:click=move |_| cancel_run(cancel_ctx, cancel_refresh, cancel_notice, cancel_id.clone())>"Cancel"</button>
                                            })}
                                            {(run.status == "complete" || run.status == "partial").then(|| view! {
                                                <button class="btn small" on:click=move |_| regrade(regrade_ctx, regrade_refresh, regrade_notice, regrade_id.clone())>"Regrade"</button>
                                            })}
                                        </td>
                                    </tr>
                                }
                            }).collect_view()}
                        </tbody>
                    </table>
                }.into_any()
            }}
            {move || open_run.get().map(|run_id| {
                let rows_state = match open_rows.get() {
                    None => view! { <p class="muted">"Reading the run's results…"</p> }.into_any(),
                    Some(Err(error)) => view! { <div class="callout bad"><p>{error.to_string()}</p></div> }.into_any(),
                    Some(Ok(rows)) if rows.is_empty() => view! {
                        <Empty message="This run has no result rows yet.".to_owned()/>
                    }.into_any(),
                    Some(Ok(rows)) => {
                        let keys = metric_keys(&rows);
                        let row_metrics = keys.clone();
                        let run_for_reload = run_id.clone();
                        view! {
                            <div class="tablewrap">
                                <table class="wtbl">
                                    <thead><tr>
                                        <th>"Case"</th><th>"Split"</th><th>"Status"</th>
                                        {keys.clone().into_iter().map(|key| view! { <th class="n">{key}</th> }).collect_view()}
                                        <th class="n">"Human"</th><th>"Label a result"</th>
                                    </tr></thead>
                                    <tbody>
                                        {rows.into_iter().map(|result| {
                                            let case = cases.iter().find(|row| row.id == result.case_id).cloned();
                                            let label = case
                                                .as_ref()
                                                .map(|row| format!("{} · {}", row.ordinal + 1, excerpt(&row.input, 40)))
                                                .unwrap_or_else(|| short_id(Some(&result.case_id)));
                                            let scores: Vec<Option<f64>> = row_metrics
                                                .iter()
                                                .map(|key| result.grade.get(key).and_then(Value::as_f64))
                                                .collect();
                                            let result_id = result.id.clone();
                                            let human = result.human_score;
                                            // A person scores a parked row; `eval_results.label`
                                            // refuses any other status. The control is shown
                                            // exactly where the action can accept it.
                                            let parked = result.status == "pending_label";
                                            let score = RwSignal::new(String::new());
                                            let note = RwSignal::new(String::new());
                                            let label_ctx = ctx;
                                            let label_notice = notice;
                                            let reload = load_results;
                                            let reload_run = run_for_reload.clone();
                                            view! {
                                                <tr>
                                                    <td>{label}</td>
                                                    <td>{split_pill(&result.split)}</td>
                                                    <td><StatusPill status=result.status.clone()/></td>
                                                    {scores.into_iter().map(|value| view! { <td class="n">{fmt_score(value)}</td> }).collect_view()}
                                                    <td class="n">{fmt_score(human)}</td>
                                                    <td>
                                                        {(can_label && parked).then(|| view! {
                                                            <span class="filterbar">
                                                                <input
                                                                    type="number" min="0" max="1" step="0.01"
                                                                    aria-label="Human score"
                                                                    placeholder="0–1"
                                                                    prop:value=move || score.get()
                                                                    on:input=move |event| score.set(event_target_value(&event))
                                                                />
                                                                <input
                                                                    type="text"
                                                                    aria-label="Note"
                                                                    placeholder="note (optional)"
                                                                    prop:value=move || note.get()
                                                                    on:input=move |event| note.set(event_target_value(&event))
                                                                />
                                                                <button class="btn small" on:click=move |_| {
                                                                    let Ok(value) = score.get_untracked().trim().parse::<f64>() else {
                                                                        label_notice.set(Some("Human score must be a number between 0 and 1.".to_owned()));
                                                                        return;
                                                                    };
                                                                    if !(0.0..=1.0).contains(&value) {
                                                                        label_notice.set(Some("Human score must be between 0 and 1.".to_owned()));
                                                                        return;
                                                                    }
                                                                    let note_value = note.get_untracked().trim().to_owned();
                                                                    let mut input = json!({"result_id": result_id.clone(), "human_score": value});
                                                                    if !note_value.is_empty() {
                                                                        input["note"] = json!(note_value);
                                                                    }
                                                                    let reload_run = reload_run.clone();
                                                                    leptos::task::spawn_local(async move {
                                                                        match label_ctx.invoke("eval_results.label", input, InvokeOptions::default()).await {
                                                                            Ok(_) => {
                                                                                label_notice.set(Some(format!("Label recorded: {value:.2}.")));
                                                                                reload(reload_run);
                                                                            }
                                                                            Err(error) => label_notice.set(Some(error.to_string())),
                                                                        }
                                                                    });
                                                                }>"Label"</button>
                                                            </span>
                                                        })}
                                                    </td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        }.into_any()
                    }
                };
                view! {
                    <section class="panel" style="margin-top:10px">
                        <div class="ph">
                            <span>"Results"</span>
                            <span class="muted">
                                {format!("{} · {}", short_id(Some(&run_id)), if open_cursor.get().is_some() { "first page" } else { "all rows" })}
                            </span>
                            <button class="btn small" on:click=move |_| { open_run.set(None); open_rows.set(None); }>"Close"</button>
                        </div>
                        {rows_state}
                    </section>
                }
            })}
            {can_start().then(|| view! {
                <div class="filterbar" style="margin-top:10px">
                    <label class="filter">
                        <span>"Reps"</span>
                        <input
                            type="number" min="1" max="10"
                            prop:value=move || reps.get()
                            on:input=move |event| {
                                if let Some(input) = event.target().and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok()) {
                                    reps.set(input.value());
                                }
                            }
                        />
                    </label>
                    <label class="filter">
                        <span>"Mode"</span>
                        <select on:change=move |event| {
                            if let Some(select) = event.target().and_then(|target| target.dyn_into::<web_sys::HtmlSelectElement>().ok()) {
                                mode.set(select.value());
                            }
                        }>
                            <option value="full" selected=move || mode.get() == "full">"full"</option>
                            <option value="pilot" selected=move || mode.get() == "pilot">"pilot"</option>
                        </select>
                    </label>
                    <button class="btn pri small" on:click=on_start>"Start run"</button>
                    <span class="muted" style="font-size:10.5px">"refused with 409 until every gate is satisfied"</span>
                </div>
            })}
            <p class="hint adv blk" style="margin-top:8px">"eval_runs.list · eval_runs.start · eval_runs.cancel · eval_runs.regrade"</p>
        </div>
    }
}

fn cancel_run(ctx: AppCtx, refresh: Callback<()>, notice: RwSignal<Option<String>>, id: String) {
    leptos::task::spawn_local(async move {
        match ctx
            .invoke(
                "eval_runs.cancel",
                json!({"id": id}),
                InvokeOptions::default(),
            )
            .await
        {
            Ok(_) => refresh.run(()),
            Err(error) => notice.set(Some(error.to_string())),
        }
    });
}

fn regrade(ctx: AppCtx, refresh: Callback<()>, notice: RwSignal<Option<String>>, id: String) {
    leptos::task::spawn_local(async move {
        match ctx
            .invoke(
                "eval_runs.regrade",
                json!({"source_run_id": id}),
                InvokeOptions::default(),
            )
            .await
        {
            Ok(invocation) => {
                let skipped = invocation
                    .output
                    .get("not_regradable")
                    .and_then(Value::as_array)
                    .map(|items| items.len())
                    .unwrap_or(0);
                notice.set(Some(if skipped == 0 {
                    "Regrade queued for every slot.".to_owned()
                } else {
                    format!("Regrade queued; {skipped} slot(s) cannot be re-graded.")
                }));
                refresh.run(());
            }
            Err(error) => notice.set(Some(error.to_string())),
        }
    });
}

// ── report ──────────────────────────────────────────────────────────────────────────────

#[component]
fn ReportTab(
    data: EvalData,
    report_run: RwSignal<Option<String>>,
    split: RwSignal<String>,
    notice: RwSignal<Option<String>>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let run = data.report_run.clone();
    let runs = data.runs.clone();
    let report = data.report.clone();
    let results = data.results.clone();
    let cases = data.cases.clone();
    let metrics = metric_keys(&results);
    let transcript = RwSignal::new(Option::<Result<EvalResultDetail, ApiError>>::None);
    let more = RwSignal::new(Vec::<ResultRowOut>::new());
    let cursor = RwSignal::new(data.results_cursor.clone());
    let can_page = ctx.has_action("eval_results.list");
    let run_id = run.as_ref().map(|row| row.id.clone()).unwrap_or_default();
    let run_id_for_transcript = run_id.clone();
    let run_label = run
        .as_ref()
        .map(|row| {
            format!(
                "{} · {} · {}",
                row.variant,
                row.status,
                short_id(Some(&row.id))
            )
        })
        .unwrap_or_else(|| "no run".to_owned());
    let run_options = runs.clone();

    let load_more = {
        let run_id = run_id.clone();
        move |_| {
            let Some(after) = cursor.get_untracked() else {
                return;
            };
            let run_id = run_id.clone();
            let split_value = split.get_untracked();
            leptos::task::spawn_local(async move {
                let mut input = json!({
                    "run_id": run_id,
                    "limit": RESULTS_PAGE,
                    "cursor": after,
                });
                if !split_value.is_empty() {
                    input["split"] = json!(split_value);
                }
                match ctx
                    .invoke("eval_results.list", input, InvokeOptions::default())
                    .await
                {
                    Ok(invocation) => {
                        match parse::<Page<ResultRowOut>>(invocation.output, "eval results") {
                            Ok(page) => {
                                more.update(|rows| rows.extend(page.items));
                                cursor.set(page.next_cursor);
                            }
                            Err(error) => notice.set(Some(error.to_string())),
                        }
                    }
                    Err(error) => notice.set(Some(error.to_string())),
                }
            });
        }
    };

    let on_run = move |event: web_sys::Event| {
        if let Some(select) = event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::HtmlSelectElement>().ok())
        {
            report_run.set(Some(select.value()));
        }
    };
    let on_split = move |event: web_sys::Event| {
        if let Some(select) = event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::HtmlSelectElement>().ok())
        {
            split.set(select.value());
        }
    };

    let open_transcript = {
        move |case_id: String, rep: i32| {
            if !ctx.has_action("eval_results.get") {
                return;
            }
            let run_id = run_id_for_transcript.clone();
            let notice = notice;
            let transcript = transcript;
            leptos::task::spawn_local(async move {
                let outcome = ctx
                    .invoke(
                        "eval_results.get",
                        json!({"run_id": run_id, "case_id": case_id, "rep": rep}),
                        InvokeOptions::default(),
                    )
                    .await
                    .and_then(|invocation| {
                        serde_json::from_value::<EvalResultDetail>(invocation.output)
                            .map_err(|error| ApiError::Network(format!("result detail: {error}")))
                    });
                if let Err(error) = &outcome {
                    notice.set(Some(error.to_string()));
                }
                transcript.set(Some(outcome));
            });
        }
    };

    view! {
        <div class="panel" style="margin-top:12px">
            <div class="ph">
                <span>"Report"</span>
                <span class="muted">{run_label.clone()}</span>
            </div>
            <div class="filterbar">
                <label class="filter">
                    <span>"Run"</span>
                    <select aria-label="Run" on:change=on_run>
                        {run_options.clone().into_iter().map(|row| {
                            let value = row.id.clone();
                            let selected_value = value.clone();
                            let current = run_id.clone();
                            view! {
                                <option value=value selected=move || current == selected_value>
                                    {format!("{} · {} · {}", row.variant, row.status, short_id(Some(&row.id)))}
                                </option>
                            }
                        }).collect_view()}
                    </select>
                </label>
                <label class="filter">
                    <span>"Split"</span>
                    <select aria-label="Split" on:change=on_split>
                        <option value="" selected=move || split.get().is_empty()>"all"</option>
                        <option value="train" selected=move || split.get() == "train">"train"</option>
                        <option value="test" selected=move || split.get() == "test">"test — needs evals:approve"</option>
                    </select>
                </label>
            </div>

            {if run.is_none() {
                view! { <Empty message="No run to report on yet.".to_owned()/> }.into_any()
            } else {
                match report.clone() {
                    None => view! {
                        <Empty message="This build does not expose eval_runs.report, so there are no run figures to show.".to_owned()/>
                    }.into_any(),
                    Some(report) => {
                        // The per-case table's columns are the grade keys the rows actually
                        // carry; the tiles above are the server's, keyed by its declared metrics.
                        let metric_keys: Vec<String> =
                            report.metrics.iter().map(|stat| stat.key.clone()).collect();
                        let perf_rows: Vec<(String, String, String, i64, String)> = report
                            .perf
                            .iter()
                            .map(|stat| {
                                let ci = match (stat.ci_low, stat.ci_high) {
                                    (Some(low), Some(high)) => format!("{low:.2}–{high:.2}"),
                                    _ => "—".to_owned(),
                                };
                                let unit = stat.unit.clone().unwrap_or_else(|| "—".to_owned());
                                (
                                    stat.label.clone(),
                                    fmt_score(stat.mean),
                                    ci,
                                    stat.n,
                                    unit,
                                )
                            })
                            .collect();
                        let tag_rows: Vec<(String, i64, Vec<String>)> = report
                            .per_tag
                            .iter()
                            .map(|group| {
                                let cells = report
                                    .metrics
                                    .iter()
                                    .map(|stat| {
                                        fmt_score(
                                            group
                                                .metrics
                                                .iter()
                                                .find(|row| row.key == stat.key)
                                                .and_then(|row| row.mean),
                                        )
                                    })
                                    .collect();
                                (group.group.clone(), group.n, cells)
                            })
                            .collect();
                        let errors = report.errors.clone();
                        view! {
                            <div class="tiles" style="margin-top:10px">
                                <Tile
                                    label="Results".to_owned()
                                    value=fmt_int(report.cases)
                                    detail=format!("case slots in the run · {} rep(s) · eval_runs.report", report.reps)
                                />
                                {report.metrics.clone().into_iter().map(|stat| {
                                    let detail = match (stat.ci_low, stat.ci_high) {
                                        (Some(low), Some(high)) => format!("95% CI {low:.2}–{high:.2} · {} case(s)", stat.n),
                                        _ => format!("mean of case means · {} case(s)", stat.n),
                                    };
                                    view! { <Tile label=format!("Mean {}", stat.key) value=fmt_score(stat.mean) detail=detail/> }
                                }).collect_view()}
                                <Tile label="Graded".to_owned() value=fmt_int(report.graded) detail="slots that produced a scorable result".to_owned()/>
                                <Tile label="Truncated".to_owned() value=fmt_int(report.truncated) detail="slots cut at max_tokens".to_owned()/>
                                {if report.cost.unit == "cents" {
                                    view! { <Tile
                                        label="Cost".to_owned()
                                        value=format!("${:.2}", report.cost.total / 100.0)
                                        detail=format!("subject ${:.2} · judge ${:.2} · failed ${:.2} · {} unpriced", report.cost.subject / 100.0, report.cost.judge / 100.0, report.cost.failed / 100.0, report.cost.unpriced)
                                    /> }.into_any()
                                } else {
                                    view! { <Tile
                                        label="Tokens".to_owned()
                                        value=fmt_int(report.cost.total.round() as i64)
                                        detail=format!("{} unpriced · the version sets no input/output prices", report.cost.unpriced)
                                    /> }.into_any()
                                }}
                            </div>
                            <p class="muted" style="margin-top:8px">
                                {match report.noise_floor {
                                    Some(value) => format!("Noise floor: {value:.3} — the smallest held-out movement this run can tell from zero (1/√(cases·reps))."),
                                    None => "Noise floor: — (nothing was scored).".to_owned(),
                                }}
                            </p>
                            <p class="muted">
                                {match report.headroom {
                                    Some(value) => format!("Headroom: {value:.2} to the headline metric's ceiling."),
                                    None => "Headroom: — (the headline metric has no bounded ceiling, or nothing was scored).".to_owned(),
                                }}
                            </p>
                            {(errors.total > 0).then(|| view! {
                                <p class="callout warn">
                                    {format!(
                                        "Failed attempts: {} over {} slot(s){}{}.",
                                        errors.total,
                                        errors.failed_slots,
                                        if errors.by_class.is_empty() { String::new() } else { " — ".to_owned() },
                                        errors.by_class.iter().map(|(class, count)| format!("{class}: {count}")).collect::<Vec<_>>().join(", "),
                                    )}
                                </p>
                            })}

                            {if !perf_rows.is_empty() {
                                view! {
                                    <div class="tablewrap">
                                        <table class="wtbl" style="margin-top:10px">
                                            <thead><tr><th>"Perf metric"</th><th class="n">"Mean"</th><th class="n">"95% CI"</th><th class="n">"Cases"</th><th>"Unit"</th></tr></thead>
                                            <tbody>
                                                {perf_rows.into_iter().map(|(label, mean, ci, n, unit)| view! {
                                                    <tr>
                                                        <td>{label}</td>
                                                        <td class="n">{mean}</td>
                                                        <td class="n">{ci}</td>
                                                        <td class="n">{n}</td>
                                                        <td>{unit}</td>
                                                    </tr>
                                                }).collect_view()}
                                            </tbody>
                                        </table>
                                    </div>
                                }.into_any()
                            } else {
                                view! { <span></span> }.into_any()
                            }}

                            {if !tag_rows.is_empty() {
                                view! {
                                    <div class="tablewrap">
                                        <table class="wtbl" style="margin-top:10px">
                                            <thead><tr>
                                                <th>"Tag"</th><th class="n">"Cases"</th>
                                                {metric_keys.into_iter().map(|key| view! { <th class="n">{key}</th> }).collect_view()}
                                            </tr></thead>
                                            <tbody>
                                                {tag_rows.into_iter().map(|(group, n, cells)| view! {
                                                    <tr>
                                                        <td class="mono">{group}</td>
                                                        <td class="n">{n}</td>
                                                        {cells.into_iter().map(|value| view! { <td class="n">{value}</td> }).collect_view()}
                                                    </tr>
                                                }).collect_view()}
                                            </tbody>
                                        </table>
                                    </div>
                                }.into_any()
                            } else {
                                view! { <span></span> }.into_any()
                            }}

                            {if results.is_empty() && more.get().is_empty() {
                                view! {
                                    <p class="callout" style="margin-top:10px">
                                        "No results for this run and split. A held-out split without "
                                        <span class="mono">"evals:approve"</span>
                                        " is a refusal, not an empty table — the message above names it."
                                    </p>
                                }.into_any()
                            } else {
                                let table_results = results.clone();
                                let count_results = results.clone();
                                view! {
                                    <div class="tablewrap">
                                        <table class="wtbl" aria-label="Run results" style="margin-top:10px">
                                            <thead><tr>
                                                <th>"Case"</th><th>"Tags"</th><th>"Split"</th>
                                                {metrics.clone().into_iter().map(|key| view! { <th class="n">{key}</th> }).collect_view()}
                                                <th class="n">"Latency"</th><th>"State"</th><th class="n">"Rep"</th>
                                            </tr></thead>
                                            <tbody>
                                                {move || {
                                                    let mut rows = table_results.clone();
                                                    rows.extend(more.get());
                                                    rows.into_iter().map(|result| {
                                                        let case = cases.iter().find(|row| row.id == result.case_id).cloned();
                                                        let label = case
                                                            .as_ref()
                                                            .map(|row| format!("{} · {}", row.ordinal + 1, excerpt(&row.input, 48)))
                                                            .unwrap_or_else(|| short_id(Some(&result.case_id)));
                                                        let tags = case.as_ref().map(|row| row.tags.clone()).unwrap_or_default();
                                                        let case_id = result.case_id.clone();
                                                        let rep = result.rep;
                                                        let open = open_transcript.clone();
                                                        let state = result_state(&result, &metrics);
                                                        let scores: Vec<Option<f64>> = metrics
                                                            .iter()
                                                            .map(|key| result.grade.get(key).and_then(Value::as_f64))
                                                            .collect();
                                                        view! {
                                                            <tr style="cursor:pointer" on:click=move |_| open(case_id.clone(), rep)>
                                                                <td>{label}</td>
                                                                <td>{tags.into_iter().map(|tag| view! { <span class="lbl">{tag}</span> }).collect_view()}</td>
                                                                <td>{split_pill(&result.split)}</td>
                                                                {scores.into_iter().map(|score| view! { <td class="n">{fmt_score(score)}</td> }).collect_view()}
                                                                <td class="n">{format!("{:.1} s", result.latency_ms as f64 / 1000.0)}</td>
                                                                <td>{state}</td>
                                                                <td class="n">{result.rep}</td>
                                                            </tr>
                                                        }
                                                    }).collect_view()
                                                }}
                                            </tbody>
                                        </table>
                                    </div>
                                    <div class="filterbar" style="margin-top:8px">
                                        <span class="muted">
                                            {move || if cursor.get().is_some() {
                                                format!("{} row(s) shown; more on the server", count_results.len() + more.get().len())
                                            } else {
                                                format!("{} row(s) shown; the run's table ends here", count_results.len() + more.get().len())
                                            }}
                                        </span>
                                        {can_page.then(|| view! {
                                            <button class="btn small" on:click=load_more.clone()>"Load more"</button>
                                        })}
                                    </div>
                                }.into_any()
                            }}
                        }.into_any()
                    }
                }
            }}
            {move || transcript.get().map(|outcome| match outcome {
                Err(error) => view! { <div class="callout bad"><p>{error.to_string()}</p></div> }.into_any(),
                Ok(detail) => view! {
                    <section class="panel" style="margin-top:12px">
                        <div class="ph">
                            <span>"Transcript"</span>
                            <span class="muted">{detail.trace_sha256.clone().map(|sha| short_id(Some(&sha))).unwrap_or_else(|| "no trace hash".to_owned())}</span>
                        </div>
                        <pre class="mono" style="white-space:pre-wrap;max-height:320px;overflow:auto">
                            {serde_json::to_string_pretty(&detail.turns.clone().unwrap_or(Value::Null)).unwrap_or_default()}
                        </pre>
                    </section>
                }.into_any(),
            })}
            <p class="hint adv blk" style="margin-top:8px">
                "eval_runs.report · eval_results.list|get · the server derives the means, the intervals, the noise floor, the headroom and the cost; the table pages with next_cursor and a page is never a run score"
            </p>
        </div>
    }
}

// ── sign-off ────────────────────────────────────────────────────────────────────────────

#[component]
fn SignoffTab(
    data: EvalData,
    refresh: Callback<()>,
    notice: RwSignal<Option<String>>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let eval_id = data.eval.id.clone();
    let gates = data.gates.clone();
    let ask = RwSignal::new(String::new());

    view! {
        <div class="panel" style="margin-top:12px">
            <div class="ph"><span>"Sign-off"</span><span class="muted">"approvals never expire"</span></div>
            <p class="muted" style="font-size:11px">
                "Two gates pause the instrument: inputs and grading. Editing any case changes the instrument digest and voids both — re-sign after an edit. Only a person may decide (a machine principal is refused 403 signoff_requires_human)."
            </p>
            <table class="wtbl">
                <thead><tr><th>"Gate"</th><th>"Status"</th><th class="n">"Decisions"</th><th>"Requested"</th><th></th></tr></thead>
                <tbody>
                    {GATES.into_iter().map(|(gate, label)| {
                        let existing = gates
                            .iter()
                            .rev()
                            .find(|row| row.gate == gate)
                            .cloned();
                        let gate_for_request = gate.to_owned();
                        let existing_id = existing.as_ref().map(|row| row.id.clone());
                        let existing_pending = existing
                            .as_ref()
                            .map(|row| row.status == "pending")
                            .unwrap_or(false);
                        let eval_for_request = eval_id.clone();
                        let ask_signal = ask;
                        let refresh_request = refresh;
                        let notice_request = notice;
                        let ctx_request = ctx;
                        view! {
                            <tr>
                                <td><b>{label}</b><span class="adv mono">" "{gate}</span></td>
                                <td>
                                    {match existing.clone() {
                                        Some(row) => view! { <StatusPill status=row.status.clone()/> }.into_any(),
                                        None => view! { <span class="pill muted">"not requested"</span> }.into_any(),
                                    }}
                                </td>
                                <td class="n">
                                    {existing.as_ref().map(|row| format!("{}/{}", row.approvals, row.required_decisions)).unwrap_or_else(|| "—".to_owned())}
                                </td>
                                <td>{existing.as_ref().and_then(|row| row.created_at).map(|at| age(Some(at))).unwrap_or_else(|| "—".to_owned())}</td>
                                <td>
                                    {match (existing_id.clone(), existing_pending) {
                                        (Some(id), true) => view! {
                                            <button class="btn small" on:click=move |_| {
                                                decide_gate(ctx_request, refresh_request, notice_request, id.clone(), "approve")
                                            }>"Approve"</button>
                                        }.into_any(),
                                        (Some(_), false) => view! { <span class="muted">"—"</span> }.into_any(),
                                        (None, _) => view! {
                                            <button class="btn small" on:click=move |_| {
                                                let ask_value = ask_signal.get_untracked().trim().to_owned();
                                                let ask_value = if ask_value.is_empty() {
                                                    format!("Sign off the {label} gate for this instrument.")
                                                } else {
                                                    ask_value
                                                };
                                                request_gate(
                                                    ctx_request,
                                                    refresh_request,
                                                    notice_request,
                                                    eval_for_request.clone(),
                                                    gate_for_request.clone(),
                                                    ask_value,
                                                );
                                            }>"Request"</button>
                                        }.into_any(),
                                    }}
                                </td>
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
            <label class="filter" style="margin-top:8px">
                <span>"What is being confirmed"</span>
                <input
                    type="text"
                    placeholder="a sentence for the approver"
                    prop:value=move || ask.get()
                    on:input=move |event| {
                        if let Some(input) = event.target().and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok()) {
                            ask.set(input.value());
                        }
                    }
                />
            </label>
            <p class="hint adv blk" style="margin-top:8px">"eval_signoffs.request|decide · approvals.list filtered by eval_id"</p>
        </div>
    }
}

fn request_gate(
    ctx: AppCtx,
    refresh: Callback<()>,
    notice: RwSignal<Option<String>>,
    eval_id: String,
    gate: String,
    ask: String,
) {
    leptos::task::spawn_local(async move {
        match ctx
            .invoke(
                "eval_signoffs.request",
                json!({"eval_id": eval_id, "gate": gate, "ask": ask}),
                InvokeOptions::default(),
            )
            .await
        {
            Ok(_) => refresh.run(()),
            Err(error) => notice.set(Some(error.to_string())),
        }
    });
}

fn decide_gate(
    ctx: AppCtx,
    refresh: Callback<()>,
    notice: RwSignal<Option<String>>,
    approval_id: String,
    decision: &'static str,
) {
    leptos::task::spawn_local(async move {
        match ctx
            .invoke(
                "eval_signoffs.decide",
                json!({"approval_id": approval_id, "decision": decision}),
                InvokeOptions::default(),
            )
            .await
        {
            Ok(_) => refresh.run(()),
            Err(error) => notice.set(Some(error.to_string())),
        }
    });
}

// ── climb ───────────────────────────────────────────────────────────────────────────────

#[component]
fn ClimbTab(
    data: EvalData,
    refresh: Callback<()>,
    notice: RwSignal<Option<String>>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let climbs = data.climbs.clone();
    let runs = data.runs.clone();
    let eval_id = data.eval.id.clone();
    let target = RwSignal::new(
        metric_keys(&data.results)
            .first()
            .cloned()
            .unwrap_or_else(|| "correct".to_owned()),
    );
    let delta = RwSignal::new("0.05".to_owned());
    let detail = RwSignal::new(Option::<Result<EvalClimb, ApiError>>::None);
    let can_start = move || ctx.has_action("eval_climbs.start");
    let can_round = ctx.has_action("eval_climbs.round");
    let can_compare = ctx.has_action("eval_climbs.compare");
    let round_climb = RwSignal::new(climbs.first().map(|row| row.id.clone()).unwrap_or_default());
    let round_no = RwSignal::new(String::new());
    let round_target = RwSignal::new(String::new());
    let change_md = RwSignal::new(String::new());
    let change_patch = RwSignal::new(String::new());
    let prediction = RwSignal::new(String::new());
    let buckets = RwSignal::new(String::new());
    let lever = RwSignal::new(String::new());
    let compare_a = RwSignal::new(runs.first().map(|row| row.id.clone()).unwrap_or_default());
    let compare_b = RwSignal::new(
        runs.get(1)
            .map(|row| row.id.clone())
            .unwrap_or_else(|| runs.first().map(|row| row.id.clone()).unwrap_or_default()),
    );
    let pass_threshold = RwSignal::new("0.5".to_owned());
    let comparison = RwSignal::new(Option::<Result<CompareOut, ApiError>>::None);

    let on_round = move |_| {
        let climb_id = round_climb.get_untracked();
        if climb_id.is_empty() {
            notice.set(Some("A climb is needed to record a round.".to_owned()));
            return;
        }
        let mut input = json!({"id": climb_id});
        let resume = round_no.get_untracked().trim().to_owned();
        if !resume.is_empty() {
            match resume.parse::<i32>() {
                Ok(round) => input["round"] = json!(round),
                Err(_) => {
                    notice.set(Some("Resume round must be a whole number.".to_owned()));
                    return;
                }
            }
        }
        for (key, raw) in [
            ("change_md", change_md.get_untracked()),
            ("change_patch", change_patch.get_untracked()),
            ("target", round_target.get_untracked()),
            ("lever", lever.get_untracked()),
        ] {
            let raw = raw.trim().to_owned();
            if !raw.is_empty() {
                input[key] = json!(raw);
            }
        }
        for (key, signal) in [("prediction", prediction), ("buckets", buckets)] {
            match parse_json_field(&signal.get_untracked(), key) {
                // Empty means omit, never `null`: the action's vocabulary has no null member.
                Ok(Value::Null) => {}
                Ok(value) => input[key] = value,
                Err(message) => {
                    notice.set(Some(message));
                    return;
                }
            }
        }
        leptos::task::spawn_local(async move {
            match ctx
                .invoke("eval_climbs.round", input, InvokeOptions::default())
                .await
            {
                Ok(invocation) => {
                    let round = invocation
                        .output
                        .get("round")
                        .and_then(Value::as_i64)
                        .unwrap_or(0);
                    let queued = invocation
                        .output
                        .get("queued_jobs")
                        .and_then(Value::as_i64)
                        .unwrap_or(0);
                    let awaiting = invocation
                        .output
                        .get("awaiting_approval")
                        .and_then(Value::as_str)
                        .is_some();
                    notice.set(Some(if awaiting {
                        format!("Round {round} recorded; its sign-off is pending.")
                    } else {
                        format!("Round {round} recorded; {queued} job(s) queued.")
                    }));
                    refresh.run(());
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    };

    let on_compare = move |_| {
        let a = compare_a.get_untracked();
        let b = compare_b.get_untracked();
        if a.is_empty() || b.is_empty() {
            notice.set(Some("Two runs are needed to compare.".to_owned()));
            return;
        }
        let mut input = json!({"a_run_id": a, "b_run_id": b});
        let threshold = pass_threshold.get_untracked().trim().to_owned();
        if !threshold.is_empty() {
            match threshold.parse::<f64>() {
                Ok(value) if (0.0..=1.0).contains(&value) => {
                    input["pass_threshold"] = json!(value);
                }
                _ => {
                    notice.set(Some("Pass threshold must be between 0 and 1.".to_owned()));
                    return;
                }
            }
        }
        leptos::task::spawn_local(async move {
            let outcome = ctx
                .invoke("eval_climbs.compare", input, InvokeOptions::default())
                .await
                .and_then(|invocation| {
                    serde_json::from_value::<CompareOut>(invocation.output)
                        .map_err(|error| ApiError::Network(format!("compare: {error}")))
                });
            if let Err(error) = &outcome {
                notice.set(Some(error.to_string()));
            }
            comparison.set(Some(outcome));
        });
    };

    let on_start = {
        let eval_id = eval_id.clone();
        move |_| {
            let eval_id = eval_id.clone();
            let goal_target = target.get_untracked();
            let delta_value = delta.get_untracked().parse::<f64>().unwrap_or(0.05);
            leptos::task::spawn_local(async move {
                match ctx
                    .invoke(
                        "eval_climbs.start",
                        json!({
                            "eval_id": eval_id,
                            "goal": {"target": goal_target, "direction": "higher", "hold": []},
                            "delta": delta_value,
                        }),
                        InvokeOptions::default(),
                    )
                    .await
                {
                    Ok(_) => refresh.run(()),
                    Err(error) => notice.set(Some(error.to_string())),
                }
            });
        }
    };

    view! {
        <div class="panel" style="margin-top:12px">
            <div class="ph"><span>"Climb"</span><span class="muted">{climbs.len()}</span></div>
            {if climbs.is_empty() {
                view! { <Empty message="No climb on this eval.".to_owned()/> }.into_any()
            } else {
                view! {
                    <table class="wtbl">
                        <thead><tr>
                            <th>"Climb"</th><th>"Target"</th><th>"Status"</th>
                            <th class="n">"Round"</th><th class="n">"Best"</th><th class="n">"Spend"</th><th></th>
                        </tr></thead>
                        <tbody>
                            {climbs.clone().into_iter().map(|climb| {
                                let id = climb.id.clone();
                                let click_id = id.clone();
                                let detail_ctx = ctx;
                                let detail_notice = notice;
                                let detail_signal = detail;
                                let stop_id = climb.id.clone();
                                let stop_ctx = ctx;
                                let stop_refresh = refresh;
                                let stop_notice = notice;
                                view! {
                                    <tr>
                                        <td class="mono">{short_id(Some(&id))}</td>
                                        <td class="mono">{climb.target.clone()}</td>
                                        <td><StatusPill status=climb.status.clone()/></td>
                                        <td class="n">{format!("{}/{}", climb.current_round, climb.max_rounds.unwrap_or(0))}</td>
                                        <td class="n">{climb.best_round.map(|round| round.to_string()).unwrap_or_else(|| "—".to_owned())}</td>
                                        <td class="n">{climb.spend_cents.map(|cents| format!("${:.2}", cents / 100.0)).unwrap_or_else(|| "—".to_owned())}</td>
                                        <td>
                                            <button class="btn small" on:click=move |_| {
                                                let id = click_id.clone();
                                                leptos::task::spawn_local(async move {
                                                    let outcome = detail_ctx
                                                        .invoke("eval_climbs.get", json!({"id": id}), InvokeOptions::default())
                                                        .await
                                                        .and_then(|invocation| {
                                                            serde_json::from_value::<EvalClimb>(invocation.output)
                                                                .map_err(|error| ApiError::Network(format!("climb: {error}")))
                                                        });
                                                    if let Err(error) = &outcome { detail_notice.set(Some(error.to_string())); }
                                                    detail_signal.set(Some(outcome));
                                                });
                                            }>"Rounds"</button>
                                            {(climb.status == "running" || climb.status == "planned").then(|| view! {
                                                <button class="btn small danger" on:click=move |_| {
                                                    let id = stop_id.clone();
                                                    leptos::task::spawn_local(async move {
                                                        match stop_ctx.invoke("eval_climbs.stop", json!({"id": id, "reason": "stopped from the console"}), InvokeOptions::default()).await {
                                                            Ok(_) => stop_refresh.run(()),
                                                            Err(error) => stop_notice.set(Some(error.to_string())),
                                                        }
                                                    });
                                                }>"Stop"</button>
                                            })}
                                        </td>
                                    </tr>
                                }
                            }).collect_view()}
                        </tbody>
                    </table>
                }.into_any()
            }}
            {can_start().then(|| view! {
                <div class="filterbar" style="margin-top:10px">
                    <label class="filter">
                        <span>"Target metric"</span>
                        <input
                            type="text"
                            prop:value=move || target.get()
                            on:input=move |event| {
                                if let Some(input) = event.target().and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok()) {
                                    target.set(input.value());
                                }
                            }
                        />
                    </label>
                    <label class="filter">
                        <span>"Noise band"</span>
                        <input
                            type="number" step="0.01" min="0.01"
                            prop:value=move || delta.get()
                            on:input=move |event| {
                                if let Some(input) = event.target().and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok()) {
                                    delta.set(input.value());
                                }
                            }
                        />
                    </label>
                    <button class="btn pri small" on:click=on_start>"Start climb"</button>
                </div>
            })}
            {move || detail.get().map(|outcome| match outcome {
                Err(error) => view! { <div class="callout bad"><p>{error.to_string()}</p></div> }.into_any(),
                Ok(climb) => view! {
                    <section class="panel" style="margin-top:10px">
                        <div class="ph"><span>"Rounds"</span><span class="muted">{format!("{} · {}", climb.status, climb.direction)}</span></div>
                        {if climb.rounds.is_empty() {
                            view! { <Empty message="This climb has no rounds yet.".to_owned()/> }.into_any()
                        } else {
                            view! {
                                <table class="wtbl">
                                    <thead><tr><th class="n">"Round"</th><th>"Variant"</th><th>"Status"</th><th class="n">"Train"</th><th class="n">"Test"</th><th class="n">"Delta"</th><th>"Decision"</th></tr></thead>
                                    <tbody>
                                        {climb.rounds.clone().into_iter().map(|round: ClimbRound| view! {
                                            <tr>
                                                <td class="n">{round.round}</td>
                                                <td class="mono">{round.variant.clone()}</td>
                                                <td><StatusPill status=round.status.clone()/></td>
                                                <td class="n">{round.train.map(|value| format!("{value:.2}")).unwrap_or_else(|| "—".to_owned())}</td>
                                                <td class="n">{round.test.map(|value| format!("{value:.2}")).unwrap_or_else(|| "—".to_owned())}</td>
                                                <td class="n">{round.delta.map(|value| format!("{value:+.2}")).unwrap_or_else(|| "—".to_owned())}</td>
                                                <td>{round.decision.clone().unwrap_or_else(|| "—".to_owned())}</td>
                                            </tr>
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            }.into_any()
                        }}
                        {climb.narrative_md.clone().map(|narrative| view! {
                            <pre class="mono" style="white-space:pre-wrap;margin-top:10px">{narrative}</pre>
                        })}
                    </section>
                }.into_any(),
            })}
            {can_round.then(|| view! {
                <section class="panel" style="margin-top:10px">
                    <div class="ph">
                        <span>"Record a round"</span>
                        <span class="muted">"eval_climbs.round"</span>
                    </div>
                    <div class="filterbar">
                        <label class="filter">
                            <span>"Climb"</span>
                            <select aria-label="Round climb" on:change=move |event| round_climb.set(event_target_value(&event))>
                                {climbs.clone().into_iter().map(|climb| {
                                    let value = climb.id.clone();
                                    let selected_value = value.clone();
                                    let current = round_climb;
                                    view! {
                                        <option value=value selected=move || current.get() == selected_value>
                                            {format!("{} · {} · round {}", short_id(Some(&climb.id)), climb.status, climb.current_round)}
                                        </option>
                                    }
                                }).collect_view()}
                            </select>
                        </label>
                        <label class="filter">
                            <span>"Resume round"</span>
                            <input type="number" min="1" aria-label="Round" placeholder="new"
                                prop:value=move || round_no.get()
                                on:input=move |event| round_no.set(event_target_value(&event))/>
                        </label>
                        <label class="filter">
                            <span>"Target"</span>
                            <input type="text" aria-label="Round target" placeholder="the surface being changed"
                                prop:value=move || round_target.get()
                                on:input=move |event| round_target.set(event_target_value(&event))/>
                        </label>
                        <label class="filter">
                            <span>"Lever"</span>
                            <input type="text" aria-label="Lever" placeholder="cost-walk lever"
                                prop:value=move || lever.get()
                                on:input=move |event| lever.set(event_target_value(&event))/>
                        </label>
                    </div>
                    <label class="filter" style="margin-top:8px">
                        <span>"What changed"</span>
                        <textarea aria-label="What changed" rows="2"
                            prop:value=move || change_md.get()
                            on:input=move |event| change_md.set(event_target_value(&event))></textarea>
                    </label>
                    <label class="filter" style="margin-top:8px">
                        <span>"Patch"</span>
                        <textarea aria-label="Change patch" rows="2"
                            prop:value=move || change_patch.get()
                            on:input=move |event| change_patch.set(event_target_value(&event))></textarea>
                    </label>
                    <div class="filterbar" style="margin-top:8px">
                        <label class="filter">
                            <span>"Prediction (JSON)"</span>
                            <textarea aria-label="Prediction" rows="2" placeholder="empty = omit"
                                prop:value=move || prediction.get()
                                on:input=move |event| prediction.set(event_target_value(&event))></textarea>
                        </label>
                        <label class="filter">
                            <span>"Buckets (JSON)"</span>
                            <textarea aria-label="Buckets" rows="2" placeholder="empty = omit"
                                prop:value=move || buckets.get()
                                on:input=move |event| buckets.set(event_target_value(&event))></textarea>
                        </label>
                    </div>
                    <button class="btn pri small" on:click=on_round>"Record round"</button>
                </section>
            })}
            {can_compare.then(|| view! {
                <section class="panel" style="margin-top:10px">
                    <div class="ph">
                        <span>"Compare two runs"</span>
                        <span class="muted">"eval_climbs.compare"</span>
                    </div>
                    <div class="filterbar">
                        <label class="filter">
                            <span>"Earlier run"</span>
                            <select aria-label="Earlier run" on:change=move |event| compare_a.set(event_target_value(&event))>
                                {runs.clone().into_iter().map(|run| {
                                    let value = run.id.clone();
                                    let selected_value = value.clone();
                                    let current = compare_a;
                                    view! {
                                        <option value=value selected=move || current.get() == selected_value>
                                            {format!("{} · {} · {}", run.variant, run.status, short_id(Some(&run.id)))}
                                        </option>
                                    }
                                }).collect_view()}
                            </select>
                        </label>
                        <label class="filter">
                            <span>"Later run"</span>
                            <select aria-label="Later run" on:change=move |event| compare_b.set(event_target_value(&event))>
                                {runs.clone().into_iter().map(|run| {
                                    let value = run.id.clone();
                                    let selected_value = value.clone();
                                    let current = compare_b;
                                    view! {
                                        <option value=value selected=move || current.get() == selected_value>
                                            {format!("{} · {} · {}", run.variant, run.status, short_id(Some(&run.id)))}
                                        </option>
                                    }
                                }).collect_view()}
                            </select>
                        </label>
                        <label class="filter">
                            <span>"Pass threshold"</span>
                            <input type="number" min="0" max="1" step="0.01" aria-label="Pass threshold"
                                prop:value=move || pass_threshold.get()
                                on:input=move |event| pass_threshold.set(event_target_value(&event))/>
                        </label>
                        <button class="btn pri small" on:click=on_compare>"Compare"</button>
                    </div>
                    {move || comparison.get().map(|outcome| match outcome {
                        Err(error) => view! { <div class="callout bad"><p>{error.to_string()}</p></div> }.into_any(),
                        Ok(compare) => view! {
                            <div class="tiles" style="margin-top:10px">
                                <Tile label="Both resolved".to_owned() value=compare.both.to_string() detail="cases both runs resolved".to_owned()/>
                                <Tile label="Neither resolved".to_owned() value=compare.neither.to_string() detail="cases neither run resolved".to_owned()/>
                                <Tile label="Only earlier".to_owned() value=compare.a_only.to_string() detail="resolved by a, not b".to_owned()/>
                                <Tile label="Only later".to_owned() value=compare.b_only.to_string() detail="resolved by b, not a".to_owned()/>
                                <Tile label="Discordant".to_owned() value=compare.discordant.to_string() detail="a_only + b_only, the exact test's pairs".to_owned()/>
                                <Tile label="Missing in one".to_owned() value=compare.missing_in_one.to_string() detail="excluded from the pairing".to_owned()/>
                                <Tile label="p".to_owned() value=format!("{:.3}", compare.p_value) detail="exact two-sided McNemar".to_owned()/>
                                <Tile label="Delta (b − a)".to_owned() value=format!("{:+.3}", compare.delta) detail=format!("95% CI {:.3}–{:.3}", compare.ci_low, compare.ci_high)/>
                            </div>
                            <p class="muted">
                                {format!(
                                    "Metric {} · {}",
                                    compare.metric,
                                    if compare.significant {
                                        "significant at p < 0.05 in b's favour"
                                    } else {
                                        "not significant at p < 0.05 in b's favour"
                                    },
                                )}
                            </p>
                        }.into_any(),
                    })}
                </section>
            })}
            <p class="hint adv blk" style="margin-top:8px">
                "eval_climbs.list|get|start|stop|round|compare — a round records one hypothesis and its run; a movement smaller than the noise band is never a result"
            </p>
        </div>
    }
}

// ── helpers ─────────────────────────────────────────────────────────────────────────────

/// The value of an input, textarea or select, whichever the event's target is.
fn event_target_value(event: &web_sys::Event) -> String {
    event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|input| input.value())
        .or_else(|| {
            event
                .target()
                .and_then(|target| target.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
                .map(|area| area.value())
        })
        .or_else(|| {
            event
                .target()
                .and_then(|target| target.dyn_into::<web_sys::HtmlSelectElement>().ok())
                .map(|select| select.value())
        })
        .unwrap_or_default()
}

/// A comma-separated tag field, trimmed and without empties.
fn parse_tags(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Parse a JSON text field. Empty is `Null`, so the caller chooses between omit and `{}`.
fn parse_json_field(raw: &str, field: &str) -> Result<Value, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(raw).map_err(|error| format!("{field}: {error}"))
}

fn excerpt(text: &str, limit: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= limit {
        flat
    } else {
        format!("{}…", flat.chars().take(limit).collect::<String>())
    }
}

fn split_pill(split: &str) -> AnyView {
    let (class, label) = match split {
        "train" => ("run", "train".to_owned()),
        "test" => ("wait", "test".to_owned()),
        _ => ("muted", "none".to_owned()),
    };
    view! { <span class=format!("pill {class}")>{label}</span> }.into_any()
}

fn fmt_score(score: Option<f64>) -> String {
    score
        .map(|value| format!("{value:.2}"))
        .unwrap_or_else(|| "—".to_owned())
}

/// The metric keys present in a run's grades. The registry has no `eval_metrics.list` action,
/// so the keys the grader actually wrote are the only honest column set. Generic over the row
/// wrapper because `eval_results.list` rows carry the human score beside the shared model.
fn metric_keys<T: std::ops::Deref<Target = EvalResult>>(results: &[T]) -> Vec<String> {
    let mut keys: Vec<String> = results
        .iter()
        .filter_map(|row| row.grade.as_object())
        .flat_map(|object| object.keys().cloned())
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

/// The table's state pill, derived from the recorded grade and never from a stored verdict.
fn result_state(result: &EvalResult, metrics: &[String]) -> AnyView {
    if result.status != "ok" {
        return view! { <span class="pill wait">{result.status.clone()}</span> }.into_any();
    }
    let values: Vec<f64> = metrics
        .iter()
        .filter_map(|key| result.grade.get(key).and_then(Value::as_f64))
        .collect();
    if values.is_empty() {
        return view! { <span class="pill muted">"ungraded"</span> }.into_any();
    }
    if values.iter().all(|value| *value >= 0.999) {
        view! { <span class="pill ok">"go"</span> }.into_any()
    } else if values.iter().all(|value| *value <= 0.001) {
        view! { <span class="pill bad">"miss"</span> }.into_any()
    } else {
        view! { <span class="pill wait">"partial"</span> }.into_any()
    }
}

// ── audit ────────────────────────────────────────────────────────────────────────────────
//
// The audit proposes and a person disposes: `eval_audits.start` writes a queued row and its
// job writes the findings; the tab reads them back and records accept/fix/dismiss on each.
// The controls are the oracle and null scores and the noise-floor-versus-headroom line — the
// three facts that say whether the number can be trusted.

#[derive(Clone, Debug, serde::Deserialize)]
struct EvalAudit {
    id: String,
    status: String,
    #[serde(default)]
    oracle_score: Option<f64>,
    #[serde(default)]
    null_score: Option<f64>,
    #[serde(default)]
    noise_floor: Option<f64>,
    #[serde(default)]
    headroom: Option<f64>,
    #[serde(default)]
    summary: Value,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    created_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Clone, Debug, serde::Deserialize)]
struct EvalFinding {
    id: String,
    section: String,
    item: String,
    severity: String,
    #[serde(default)]
    evidence: String,
    #[serde(default)]
    suggestion: String,
    status: String,
}

#[derive(Clone, Debug, serde::Deserialize)]
struct AuditGet {
    audit: EvalAudit,
    #[serde(default)]
    findings: Vec<EvalFinding>,
}

fn score(value: Option<f64>) -> String {
    value
        .map(|value| format!("{:.1}%", value * 100.0))
        .unwrap_or_else(|| "—".to_owned())
}

#[component]
fn AuditTab(
    eval_id: String,
    version: Option<EvalVersion>,
    runs: Vec<EvalRun>,
    refresh: Callback<()>,
    notice: RwSignal<Option<String>>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let selected = RwSignal::new(Option::<String>::None);
    let load_eval_id = eval_id.clone();
    let data = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let eval_id = load_eval_id.clone();
        let chosen = selected.get();
        let client = ctx.client();
        async move {
            let page: Page<EvalAudit> = parse(
                client
                    .invoke(
                        "eval_audits.list",
                        json!({"eval_id": eval_id, "limit": 20}),
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                "eval audits",
            )?;
            let id = chosen.or_else(|| page.items.first().map(|audit| audit.id.clone()));
            let detail = match id {
                Some(id) => Some(parse::<AuditGet>(
                    client
                        .invoke(
                            "eval_audits.get",
                            json!({"id": id}),
                            InvokeOptions::default(),
                        )
                        .await?
                        .output,
                    "eval audit",
                )?),
                None => None,
            };
            Ok((page.items, detail))
        }
    });
    let retry = Callback::new(move |_| data.refetch());

    let start_eval = eval_id.clone();
    let start_version = version.as_ref().map(|version| version.id.clone());
    let start_run = runs
        .iter()
        .find(|run| run.status == "succeeded")
        .map(|run| run.id.clone());
    let start = Callback::new(move |_: ()| {
        let Some(version_id) = start_version.clone() else {
            notice.set(Some(
                "This eval has no frozen version, so there is nothing to audit.".into(),
            ));
            return;
        };
        let mut input = json!({"eval_id": start_eval.clone(), "version_id": version_id});
        if let Some(run) = start_run.clone() {
            input["target_run_id"] = json!(run);
        }
        let ctx = ctx;
        leptos::task::spawn_local(async move {
            match ctx
                .invoke("eval_audits.start", input, InvokeOptions::default())
                .await
            {
                Ok(_) => {
                    notice.set(Some("Audit queued; the worker writes its findings.".into()));
                    data.refetch();
                    refresh.run(());
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    });

    let decide = Callback::new(move |(finding_id, decision): (String, &'static str)| {
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "eval_findings.decide",
                    json!({"finding_id": finding_id, "decision": decision}),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => data.refetch(),
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    });

    view! {
        <div class="filterbar" style="margin-bottom:10px">
            <button class="btn primary" on:click=move |_| start.run(())>"Run audit"</button>
            <button class="btn" on:click=move |_| data.refetch()>"Refresh"</button>
            <span class="muted">{move || version.as_ref().map(|version| format!("instrument v{}", version.version)).unwrap_or_else(|| "no frozen version".to_owned())}</span>
        </div>
        {move || match data.get() {
            None => view! { <p class="muted">"Reading the audits…"</p> }.into_any(),
            Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
            Some(Ok((audits, detail))) => view! {
                <div class="split">
                    <div>
                        {if audits.is_empty() {
                            view! { <Empty message="No audit has run for this instrument yet. Run one; the job writes the findings.".to_owned()/> }.into_any()
                        } else {
                            view! {
                                <div class="tablewrap">
                                    <table class="wtbl">
                                        <thead><tr><th>"Audit"</th><th>"Status"</th><th>"Oracle"</th><th>"Null"</th><th>"When"</th></tr></thead>
                                        <tbody>
                                            {audits.into_iter().map(|audit| {
                                                let id = audit.id.clone();
                                                let pick = id.clone();
                                                view! {
                                                    <tr>
                                                        <td class="mono">
                                                            <button class="link" on:click=move |_| selected.set(Some(pick.clone()))>{short_id(Some(&id))}</button>
                                                        </td>
                                                        <td><StatusPill status=audit.status.clone()/></td>
                                                        <td>{score(audit.oracle_score)}</td>
                                                        <td>{score(audit.null_score)}</td>
                                                        <td>{age(audit.created_at)}</td>
                                                    </tr>
                                                }
                                            }).collect_view()}
                                        </tbody>
                                    </table>
                                </div>
                            }.into_any()
                        }}
                    </div>
                    <div>
                        {detail.map(|detail| view! {
                            <section class="panel">
                                <div class="ph">
                                    <span>"Controls"</span>
                                    <StatusPill status=detail.audit.status.clone()/>
                                </div>
                                <div class="kv">
                                    <dt>"Oracle"</dt><dd>{score(detail.audit.oracle_score)}</dd>
                                    <dt>"Null"</dt><dd>{score(detail.audit.null_score)}</dd>
                                    <dt>"Noise floor"</dt><dd>{score(detail.audit.noise_floor)}</dd>
                                    <dt>"Headroom"</dt><dd>{score(detail.audit.headroom)}</dd>
                                </div>
                                {detail.audit.error.clone().map(|error| view! {
                                    <div class="callout warn" role="alert" style="margin-top:8px"><p>{error}</p></div>
                                })}
                                <details style="margin-top:8px">
                                    <summary class="muted">"Summary"</summary>
                                    <pre class="code">{serde_json::to_string_pretty(&detail.audit.summary).unwrap_or_default()}</pre>
                                </details>
                            </section>
                            <section class="panel" style="margin-top:10px">
                                <div class="ph"><span>"Findings"</span><span class="muted">{detail.findings.len()}</span></div>
                                {if detail.findings.is_empty() {
                                    view! { <Empty message="No finding: the checklist is clean, or the audit has not finished.".to_owned()/> }.into_any()
                                } else {
                                    view! {
                                        {detail.findings.into_iter().map(|finding| {
                                            let tone = match finding.severity.as_str() {
                                                "misleading" => "finding high",
                                                "noise" => "finding med",
                                                _ => "finding",
                                            };
                                            let decide_id = finding.id.clone();
                                            let open = finding.status == "open";
                                            view! {
                                                <div class=tone>
                                                    <div class="fh">
                                                        <span class="pill">{format!("§{} {}", finding.section, finding.item)}</span>
                                                        <span class="pill muted">{finding.severity.clone()}</span>
                                                        <span class="pill muted" style="margin-left:auto">{finding.status.clone()}</span>
                                                    </div>
                                                    <p>{finding.evidence.clone()}</p>
                                                    {(!finding.suggestion.is_empty()).then(|| view! {
                                                        <p class="add">{finding.suggestion.clone()}</p>
                                                    })}
                                                    {open.then(|| {
                                                        let accept = decide_id.clone();
                                                        let fixed = decide_id.clone();
                                                        let dismiss = decide_id.clone();
                                                        view! {
                                                            <div style="display:flex;gap:6px;margin-top:6px">
                                                                <button class="btn small pri" on:click=move |_| decide.run((accept.clone(), "accepted"))>"Accept"</button>
                                                                <button class="btn small" on:click=move |_| decide.run((fixed.clone(), "fixed"))>"Mark fixed"</button>
                                                                <button class="btn small" on:click=move |_| decide.run((dismiss.clone(), "dismissed"))>"Dismiss"</button>
                                                            </div>
                                                        }
                                                    })}
                                                </div>
                                            }
                                        }).collect_view()}
                                    }.into_any()
                                }}
                            </section>
                        })}
                    </div>
                </div>
            }.into_any(),
        }}
    }
}

// ── interop ──────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, serde::Deserialize)]
struct ExportOut {
    document_id: String,
    #[serde(default)]
    content_hash: String,
    #[serde(default)]
    jsonl_bytes: i64,
    #[serde(default)]
    gzip_bytes: i64,
    #[serde(default)]
    results: i32,
    #[serde(default)]
    traces: i32,
}

#[derive(Clone, Debug, serde::Deserialize)]
struct CaseImportOut {
    #[serde(default)]
    imported: Vec<Value>,
}

#[component]
fn InteropTab(
    version: Option<EvalVersion>,
    runs: Vec<EvalRun>,
    notice: RwSignal<Option<String>>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let run = RwSignal::new(runs.first().map(|run| run.id.clone()).unwrap_or_default());
    let exported = RwSignal::new(Option::<ExportOut>::None);
    let bundle = RwSignal::new(String::new());
    let imported = RwSignal::new(Option::<usize>::None);

    let run_ids: Vec<(String, String)> = runs
        .iter()
        .map(|run| {
            (
                run.id.clone(),
                format!(
                    "{} · {} · {}",
                    run.variant,
                    run.status,
                    short_id(Some(&run.id))
                ),
            )
        })
        .collect();
    let has_runs = !run_ids.is_empty();
    let version_id = version.as_ref().map(|version| version.id.clone());
    let version_label = version
        .as_ref()
        .map(|version| format!("version {}", short_id(Some(&version.id))))
        .unwrap_or_else(|| "no frozen version".to_owned());

    let export = Callback::new(move |_: ()| {
        let id = run.get_untracked();
        if id.is_empty() {
            notice.set(Some("There is no run to export.".into()));
            return;
        }
        let ctx = ctx;
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "eval_runs.export",
                    json!({"id": id}),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(invocation) => match parse::<ExportOut>(invocation.output, "export") {
                    Ok(out) => {
                        exported.set(Some(out));
                        imported.set(None);
                    }
                    Err(error) => notice.set(Some(error.to_string())),
                },
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    });

    let import = Callback::new(move |_: ()| {
        let Some(version_id) = version_id.clone() else {
            notice.set(Some(
                "This eval has no frozen version to import into.".into(),
            ));
            return;
        };
        let raw = bundle.get_untracked();
        let cases: Value = match serde_json::from_str(&raw) {
            Ok(value @ Value::Array(_)) => value,
            Ok(_) => {
                notice.set(Some("The bundle must be a JSON array of cases.".into()));
                return;
            }
            Err(error) => {
                notice.set(Some(format!("The bundle is not JSON: {error}")));
                return;
            }
        };
        let ctx = ctx;
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "eval_cases.import",
                    json!({"version_id": version_id, "cases": cases}),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(invocation) => match parse::<CaseImportOut>(invocation.output, "import") {
                    Ok(out) => {
                        imported.set(Some(out.imported.len()));
                        notice.set(Some(format!("Imported {} cases.", out.imported.len())));
                    }
                    Err(error) => notice.set(Some(error.to_string())),
                },
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    });

    view! {
        <div class="split">
            <div>
                <section class="panel">
                    <div class="ph"><span>"Export a run"</span></div>
                    <p class="muted">
                        "Results and traces as one gzip JSONL document in this tenant's object store; the answer key is never written."
                    </p>
                    {if has_runs {
                        view! {
                            <div class="formrow">
                                <label class="filter">
                                    <span>"Run"</span>
                                    <select on:change=move |event| run.set(event_target_value(&event))>
                                        {run_ids.into_iter().map(|(id, label)| view! {
                                            <option value=id.clone() selected=move || run.get() == id>{label}</option>
                                        }).collect_view()}
                                    </select>
                                </label>
                                <button class="btn primary" on:click=move |_| export.run(())>"Export"</button>
                            </div>
                        }.into_any()
                    } else {
                        view! { <Empty message="This eval has no run to export yet.".to_owned()/> }.into_any()
                    }}
                    {move || exported.get().map(|out| view! {
                        <div class="callout" role="status">
                            <p>
                                {format!("Document {} · {} results · {} traces · {} bytes gzip / {} bytes JSONL", short_id(Some(&out.document_id)), out.results, out.traces, out.gzip_bytes, out.jsonl_bytes)}
                            </p>
                            <p class="mono muted">{short_id(Some(&out.content_hash))}</p>
                            <a class="btn small" href=format!("/api/documents/{}/content", out.document_id)>"Open the bundle"</a>
                        </div>
                    })}
                </section>
                <section class="panel" style="margin-top:10px">
                    <div class="ph"><span>"Import cases"</span></div>
                    <p class="muted">
                        "A JSON array of cases copied into the frozen version, all or nothing. Only a version with no split drawn can take them."
                    </p>
                    <label class="filter">
                        <span>"Cases (JSON)"</span>
                        <textarea
                            rows="8"
                            prop:value=move || bundle.get()
                            on:input=move |event| bundle.set(event_target_value(&event))
                            placeholder="[{\"input\": \"…\", \"tags\": [\"smoke\"]}]"
                        ></textarea>
                    </label>
                    <button class="btn primary" on:click=move |_| import.run(()) style="margin-top:8px">"Import"</button>
                    {move || imported.get().map(|count| view! {
                        <p class="muted">{format!("{count} cases imported into this version.")}</p>
                    })}
                </section>
            </div>
            <div>
                <section class="panel">
                    <div class="ph"><span>"The method's files"</span></div>
                    <p class="muted">
                        "The export is the method's `results.jsonl` + `traces/` bundle, stored as an ordinary S09 document so it can be re-read or audited like any other object. Import is the inverse: cases, never an answer key."
                    </p>
                    <p class="muted mono">{version_label.clone()}</p>
                </section>
            </div>
        </div>
    }
}
