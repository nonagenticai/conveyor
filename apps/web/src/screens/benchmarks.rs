//! Benchmarks — two arms, a paired matrix, and an exact test that refuses when the pairing
//! is not valid. The refusal is part of the screen, not an error state.
//!
//! What is measurable today: the two **controls** (`gold`, `empty`) and S18's **conveyor** arm,
//! which measures one pinned capability agent. `baseline` is still refused by the action with
//! `409 arm_not_ready` (its producer has no build step), so the form keeps it disabled with that
//! reason and no fixture invents it. A comparison between two controls is labelled a suite
//! check, never a subject comparison.
//!
//! `benchmark_runs.promote` writes the suite's `baseline_run_id`, which S18 reads as the
//! subject baseline. Promoting a control would corrupt that meaning, so it is shown disabled
//! with the reason rather than offered.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::JsCast;

use crate::api::{ApiError, InvokeOptions, Page};
use crate::components::{Empty, ErrorPanel, Planned, StatusPill, Tile, age, fmt_int, short_id};
use crate::models::{BenchmarkCase, BenchmarkCompare, BenchmarkRun, BenchmarkSuite};
use crate::state::AppCtx;

fn parse<T: serde::de::DeserializeOwned>(
    value: serde_json::Value,
    what: &str,
) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Network(format!("{what}: {error}")))
}

#[derive(Clone, Debug, Deserialize)]
struct Agent {
    id: String,
    key: String,
    #[serde(default)]
    status: String,
}

#[derive(Clone, Debug)]
struct IndexData {
    suites: Vec<BenchmarkSuite>,
    runs: Vec<BenchmarkRun>,
    shown_suite: Option<String>,
}

fn load_index(
    ctx: AppCtx,
    suite: RwSignal<Option<String>>,
) -> LocalResource<Result<IndexData, ApiError>> {
    LocalResource::new(move || async move {
        let _tenant = ctx.tenant.get();
        let chosen = suite.get();
        let client = ctx.client();
        let suites: Page<BenchmarkSuite> = parse(
            client
                .invoke(
                    "benchmark_suites.list",
                    json!({"limit": 25}),
                    InvokeOptions::default(),
                )
                .await?
                .output,
            "benchmark suites",
        )?;
        let shown = chosen.or_else(|| suites.items.first().map(|row| row.id.clone()));
        let input = match &shown {
            Some(id) => json!({"suite_id": id, "limit": 50}),
            None => json!({"limit": 50}),
        };
        let runs: Page<BenchmarkRun> = parse(
            client
                .invoke("benchmark_runs.list", input, InvokeOptions::default())
                .await?
                .output,
            "benchmark runs",
        )?;
        Ok(IndexData {
            suites: suites.items,
            runs: runs.items,
            shown_suite: shown,
        })
    })
}

#[component]
pub fn Benchmarks() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let suite = RwSignal::new(Option::<String>::None);
    let data = load_index(ctx, suite);
    let retry = Callback::new(move |_| data.refetch());
    let notice = RwSignal::new(Option::<String>::None);

    view! {
        <section class="screen measure">
            {move || {
                if !(ctx.has_action("benchmark_suites.list")
                    && ctx.has_action("benchmark_runs.list"))
                {
                    return view! {
                        <Planned
                            title="Benchmarks are planned".to_owned()
                            detail="benchmark_suites.list / benchmark_runs.list are not in this role's manifest. The screen appears when the actions land, not before.".to_owned()
                        />
                    }
                    .into_any();
                }
                view! {
                    <header class="screen-head">
                        <div>
                            <h1>"Benchmarks"</h1>
                            <p class="muted">"gold and empty controls prove the suite · conveyor measures a capability agent · baseline is still refused by the action"</p>
                        </div>
                        <div class="filterbar">
                            <button class="btn" on:click=move |_| data.refetch()>"Refresh"</button>
                        </div>
                    </header>
                    {move || notice.get().map(|message| view! {
                        <div class="callout warn" role="status"><p>{message}</p></div>
                    })}
                    {move || match data.get() {
                        None => view! { <p class="muted">"Reading the suites…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=retry/>
                        }.into_any(),
                        Some(Ok(body)) => view! {
                            <IndexBody data=body suite=suite refresh=retry notice=notice/>
                        }.into_any(),
                    }}
                }
                .into_any()
            }}
        </section>
    }
}

#[component]
fn IndexBody(
    data: IndexData,
    suite: RwSignal<Option<String>>,
    refresh: Callback<()>,
    notice: RwSignal<Option<String>>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let suites = data.suites.clone();
    let runs = data.runs.clone();
    let shown_suite = data.shown_suite.clone();
    let shown_suite_name = suites
        .iter()
        .find(|row| Some(row.id.clone()) == shown_suite)
        .map(|row| row.slug.clone())
        .unwrap_or_else(|| "no suite".to_owned());
    let suite_for_form = shown_suite.clone();
    let suite_options = suites.clone();
    let can_start = move || ctx.has_action("benchmark_runs.start");

    // Promote writes the suite's baseline, which S18 reads as the subject baseline. Only a
    // succeeded conveyor run is offered: promoting a control would make every trend point
    // incomparable, and the store does not refuse it.
    let promote = Callback::new(move |id: String| {
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "benchmark_runs.promote",
                    json!({"id": id}),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => {
                    notice.set(Some(
                        "Promoted as the suite baseline; every later candidate is compared against it.".into(),
                    ));
                    refresh.run(());
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    });

    view! {
        <div class="callout">
            <p>
                "A control run is the suite proving itself: the gold arm must resolve every instance, the empty arm none. If either misses, the suite is broken — not the subject. "
                <b>"The conveyor arm measures one pinned capability agent; the baseline arm is still refused by the action (409 arm_not_ready)."</b>
            </p>
        </div>
        <div class="split">
            <div>
                <section class="panel">
                    <div class="panel-head">
                        <h2>"Suites"</h2>
                        <span class="muted">{format!("{} seeded", suites.len())}</span>
                    </div>
                    {if suites.is_empty() {
                        view! {
                            <Empty message="No benchmark suite in this tenant. The embedded verified50 artifact seeds with `migrate --dev-seed`.".to_owned()/>
                        }.into_any()
                    } else {
                        view! {
                            <table class="wtbl">
                                <thead><tr>
                                    <th>"Suite"</th><th>"Dataset"</th><th class="n">"Instances"</th>
                                    <th>"Seed"</th><th>"Baseline"</th>
                                </tr></thead>
                                <tbody>
                                    {suites.clone().into_iter().map(|row| {
                                        let id = row.id.clone();
                                        let click = id.clone();
                                        let is_shown = Some(row.id.clone()) == shown_suite;
                                        view! {
                                            <tr
                                                class={if is_shown { "sel" } else { "" }}
                                                style="cursor:pointer"
                                                on:click=move |_| suite.set(Some(click.clone()))
                                            >
                                                <td>
                                                    <span class="mono">{row.slug.clone()}</span>
                                                    {is_shown.then(|| view! { <span class="pill ok">"shown"</span> })}
                                                    <span class="adv mono">" "{short_id(Some(&row.id))}</span>
                                                </td>
                                                <td class="mono">
                                                    {row.dataset.clone()}
                                                    <span class="muted">{format!(" @{}", short_id(Some(&row.dataset_revision)))}</span>
                                                </td>
                                                <td class="n">{fmt_int(row.instance_count)}</td>
                                                <td class="mono" title=row.seed_sha256.clone()>{short_id(Some(&row.seed_sha256))}</td>
                                                <td>
                                                    {row.baseline_run_id.clone().map(|id| view! {
                                                        <A href=format!("/evals/benchmarks/run/{id}")><span class="mono">{short_id(Some(&id))}</span></A>
                                                    })}
                                                    {row.baseline_run_id.is_none().then(|| view! {
                                                        <span class="muted">"none — S18 needs one"</span>
                                                    })}
                                                </td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        }.into_any()
                    }}
                </section>

                <section class="panel" style="margin-top:12px">
                    <div class="panel-head">
                        <h2>"Runs"</h2>
                        <span class="muted">{format!("{} · {}", shown_suite_name, runs.len())}</span>
                    </div>
                    {if runs.is_empty() {
                        view! {
                            <Empty message="No run for this suite yet. Start a gold control to prove it.".to_owned()/>
                        }.into_any()
                    } else {
                        view! {
                            <table class="wtbl">
                                <thead><tr>
                                    <th>"Run"</th><th>"Arm"</th><th>"Status"</th>
                                    <th class="n">"Resolved"</th><th>"Control"</th><th>"When"</th><th></th>
                                </tr></thead>
                                <tbody>
                                    {runs.clone().into_iter().map(|row| view! {
                                        <tr>
                                            <td>
                                                <A href=format!("/evals/benchmarks/run/{}", row.id)>
                                                    <span class="mono">{row.label.clone()}</span>
                                                </A>
                                                <span class="adv mono">" "{short_id(Some(&row.id))}</span>
                                            </td>
                                            <td class="mono">{row.arm.clone()}</td>
                                            <td><StatusPill status=row.status.clone()/></td>
                                            <td class="n">{format!("{} / {}", fmt_int(i64::from(row.n_resolved)), fmt_int(i64::from(row.n_graded)))}</td>
                                            <td>{control_pill(row.control_ok)}</td>
                                            <td>{row.created_at.map(|at| age(Some(at))).unwrap_or_else(|| "—".to_owned())}</td>
                                            <td>
                                                {(row.arm == "conveyor"
                                                    && row.status == "succeeded"
                                                    && ctx.has_action("benchmark_runs.promote")).then(|| {
                                                    let id = row.id.clone();
                                                    view! {
                                                        <button class="btn small" type="button" on:click=move |_| promote.run(id.clone())>
                                                            "Promote"
                                                        </button>
                                                    }
                                                })}
                                            </td>
                                        </tr>
                                    }).collect_view()}
                                </tbody>
                            </table>
                        }.into_any()
                    }}
                </section>
            </div>
            <div>
                {can_start().then(|| view! {
                    <StartControlForm
                        suite=suite_for_form
                        suites=suite_options
                        refresh=refresh
                        notice=notice
                    />
                })}
                <section class="panel" style="margin-top:12px">
                    <div class="ph"><span>"Not offered"</span></div>
                    <p class="muted" style="font-size:11px">
                        <b>"Promote"</b> " writes the suite's baseline, which S18 reads as the subject baseline. It is offered on a succeeded conveyor run only — promoting a control would make every trend point incomparable."
                    </p>
                    <p class="muted" style="font-size:11px;margin-top:6px">
                        <b>"baseline / conveyor"</b> " — refused by " <span class="mono">"benchmark_runs.start"</span> " with 409 " <span class="mono">"arm_not_ready"</span>.
                    </p>
                </section>
            </div>
        </div>
    }
}

#[component]
fn StartControlForm(
    suite: Option<String>,
    suites: Vec<BenchmarkSuite>,
    refresh: Callback<()>,
    notice: RwSignal<Option<String>>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let chosen_suite = RwSignal::new(suite.unwrap_or_default());
    let arm = RwSignal::new("gold".to_owned());
    let agent_id = RwSignal::new(String::new());
    let agents = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        async move {
            let page: Page<Agent> = parse(
                ctx.invoke(
                    "capability_agents.list",
                    json!({"limit": 200}),
                    InvokeOptions::default(),
                )
                .await?
                .output,
                "capability agents",
            )?;
            Ok::<_, ApiError>(page.items)
        }
    });
    let label = RwSignal::new(String::new());
    let failing_test = RwSignal::new(false);
    let busy = RwSignal::new(false);

    let on_submit = move |event: web_sys::SubmitEvent| {
        event.prevent_default();
        let suite_id = chosen_suite.get_untracked();
        if suite_id.is_empty() {
            notice.set(Some("Choose a suite first.".into()));
            return;
        }
        let arm_value = arm.get_untracked();
        let label_value = label.get_untracked().trim().to_owned();
        let label_value = if label_value.is_empty() {
            arm_value.clone()
        } else {
            label_value
        };
        let mut input = json!({
            "suite_id": suite_id,
            "arm": arm_value,
            "label": label_value,
            "with_failing_test": failing_test.get_untracked(),
        });
        if arm_value == "conveyor" {
            let agent = agent_id.get_untracked();
            if agent.is_empty() {
                notice.set(Some(
                    "Choose the capability agent a conveyor run measures.".into(),
                ));
                return;
            }
            input["subject_kind"] = json!("capability_agent");
            input["capability_agent_id"] = json!(agent);
        }
        let ctx = ctx;
        busy.set(true);
        leptos::task::spawn_local(async move {
            match ctx
                .invoke("benchmark_runs.start", input, InvokeOptions::default())
                .await
            {
                Ok(invocation) => {
                    let id = invocation
                        .output
                        .get("id")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                        .unwrap_or_else(|| "?".to_owned());
                    notice.set(Some(format!(
                        "Control run queued ({}) — the driver grades it when the sandbox is reachable.",
                        short_id(Some(&id))
                    )));
                    refresh.run(());
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
            busy.set(false);
        });
    };

    view! {
        <section class="panel">
            <div class="ph"><span>"Start a control run"</span></div>
            <form on:submit=on_submit>
                <label class="filter">
                    <span>"Suite"</span>
                    <select
                        aria-label="Suite"
                        on:change=move |event| {
                            if let Some(select) = event.target().and_then(|target| target.dyn_into::<web_sys::HtmlSelectElement>().ok()) {
                                chosen_suite.set(select.value());
                            }
                        }
                    >
                        {suites.clone().into_iter().map(|row| {
                            let value = row.id.clone();
                            let selected_value = value.clone();
                            view! {
                                <option value=value selected=move || chosen_suite.get() == selected_value>
                                    {row.slug.clone()}
                                </option>
                            }
                        }).collect_view()}
                    </select>
                </label>
                <label class="filter" style="margin-top:6px">
                    <span>"Arm"</span>
                    <select
                        aria-label="Arm"
                        on:change=move |event| {
                            if let Some(select) = event.target().and_then(|target| target.dyn_into::<web_sys::HtmlSelectElement>().ok()) {
                                arm.set(select.value());
                            }
                        }
                    >
                        <option value="gold" selected=move || arm.get() == "gold">"gold — must resolve all"</option>
                        <option value="empty" selected=move || arm.get() == "empty">"empty — must resolve none"</option>
                        <option value="conveyor" selected=move || arm.get() == "conveyor">"conveyor — measures a capability agent"</option>
                        <option value="baseline" disabled=true>"baseline — refused by the action (409 arm_not_ready)"</option>
                    </select>
                </label>
                <label class="filter" style="margin-top:6px">
                    <span>"Capability agent"</span>
                    <select
                        aria-label="Capability agent"
                        on:change=move |event| agent_id.set(event_target_value(&event))
                    >
                        <option value="">"choose…"</option>
                        {move || {
                            let items = match agents.get() { Some(Ok(items)) => items, _ => Vec::new() };
                            let options = items.into_iter().filter(|agent| agent.status == "active").map(|agent| {
                                let value = agent.id.clone();
                                view! { <option value=value>{agent.key.clone()}</option> }
                            }).collect_view();
                            view! { {options} }.into_any()
                        }}
                    </select>
                </label>
                <label class="filter" style="margin-top:6px">
                    <span>"Label"</span>
                    <input
                        type="text"
                        placeholder="gold"
                        prop:value=move || label.get()
                        on:input=move |event| {
                            if let Some(input) = event.target().and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok()) {
                                label.set(input.value());
                            }
                        }
                    />
                </label>
                <label class="filter" style="margin-top:6px" title="Runs the oracle with the failing test visible; the Arm variant derives from the label suffix">
                    <input
                        type="checkbox"
                        prop:checked=move || failing_test.get()
                        on:change=move |event| {
                            failing_test.set(
                                event.target()
                                    .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
                                    .map(|input| input.checked())
                                    .unwrap_or(false),
                            );
                        }
                    />
                    <span>"+ failing test"</span>
                </label>
                <div style="display:flex;gap:6px;margin-top:8px">
                    <button class="btn pri small" type="submit" disabled=move || busy.get()>
                        {move || if busy.get() { "Starting…" } else { "Start control" }}
                    </button>
                </div>
                <p class="muted" style="font-size:10.5px;margin-top:6px">
                    "A run needs the k8s sandbox; without it the run stays queued and says so."
                </p>
            </form>
        </section>
    }
}

fn control_pill(control_ok: Option<bool>) -> impl IntoView {
    match control_ok {
        Some(true) => view! { <span class="pill ok">"ok"</span> }.into_any(),
        Some(false) => view! { <span class="pill bad">"missed"</span> }.into_any(),
        None => view! { <span class="pill muted">"—"</span> }.into_any(),
    }
}

// ── run detail ─────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
struct DetailData {
    run: BenchmarkRun,
    suite: Option<BenchmarkSuite>,
    others: Vec<BenchmarkRun>,
    compare_to: Option<String>,
    compare: Option<Result<BenchmarkCompare, ApiError>>,
    cases_a: Vec<BenchmarkCase>,
    cases_b: Vec<BenchmarkCase>,
}

fn load_detail<F>(
    ctx: AppCtx,
    run_id: F,
    compare_to: RwSignal<Option<String>>,
) -> LocalResource<Result<DetailData, ApiError>>
where
    F: Fn() -> String + 'static,
{
    LocalResource::new(move || {
        // Every reactive input is read before the first await.
        let run_id = run_id();
        let chosen = compare_to.get();
        let _tenant = ctx.tenant.get();
        let can_compare = ctx.has_action("benchmark_runs.compare");
        async move {
            if run_id.is_empty() {
                return Err(ApiError::NotFound("no run in the route".to_owned()));
            }
            let client = ctx.client();
            let run: BenchmarkRun = parse(
                client
                    .invoke(
                        "benchmark_runs.get",
                        json!({"id": run_id}),
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                "benchmark run",
            )?;
            let suites: Page<BenchmarkSuite> = parse(
                client
                    .invoke(
                        "benchmark_suites.list",
                        json!({"limit": 25}),
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                "benchmark suites",
            )?;
            let suite = suites
                .items
                .iter()
                .find(|row| row.id == run.suite_id)
                .cloned();
            let runs: Page<BenchmarkRun> = parse(
                client
                    .invoke(
                        "benchmark_runs.list",
                        json!({"suite_id": run.suite_id, "limit": 50}),
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                "benchmark runs",
            )?;
            let others: Vec<BenchmarkRun> = runs
                .items
                .into_iter()
                .filter(|row| row.id != run.id)
                .collect();
            let compare_to = chosen.or_else(|| others.first().map(|row| row.id.clone()));
            let compare = match &compare_to {
                Some(other) => Some(if !can_compare {
                    Ok(BenchmarkCompare::refused(
                        "benchmark_runs.compare is not in this role's manifest",
                    ))
                } else {
                    let value = client
                        .invoke(
                            "benchmark_runs.compare",
                            json!({"a_run_id": run.id, "b_run_id": other}),
                            InvokeOptions::default(),
                        )
                        .await
                        .map(|invocation| invocation.output);
                    match value {
                        Ok(output) => parse::<BenchmarkCompare>(output, "benchmark compare"),
                        Err(error) => Err(error),
                    }
                }),
                None => None,
            };
            let cases_a = fetch_cases(&client, &run.id).await?;
            let cases_b = match &compare_to {
                Some(other) => fetch_cases(&client, other).await?,
                None => Vec::new(),
            };
            Ok(DetailData {
                run,
                suite,
                others,
                compare_to,
                compare,
                cases_a,
                cases_b,
            })
        }
    })
}

impl BenchmarkCompare {
    fn refused(reason: &str) -> Self {
        Self {
            comparable: false,
            reason: Some(reason.to_owned()),
            both: None,
            neither: None,
            a_only: None,
            b_only: None,
            discordant: None,
            p_value: None,
            missing_in_one: None,
        }
    }
}

async fn fetch_cases(
    client: &crate::api::Client,
    run_id: &str,
) -> Result<Vec<BenchmarkCase>, ApiError> {
    let page: Page<BenchmarkCase> = parse(
        client
            .invoke(
                "benchmark_cases.list",
                json!({"run_id": run_id, "limit": 200}),
                InvokeOptions::default(),
            )
            .await?
            .output,
        "benchmark cases",
    )?;
    Ok(page.items)
}

#[component]
pub fn BenchmarkRunDetail() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let params = use_params_map();
    let run_id = move || params.get().get("run_id").unwrap_or_default();
    let compare_to = RwSignal::new(Option::<String>::None);
    let data = load_detail(ctx, run_id, compare_to);
    let refresh = Callback::new(move |_| data.refetch());
    let notice = RwSignal::new(Option::<String>::None);

    view! {
        <section class="screen measure">
            {move || {
                if run_id().is_empty() {
                    return view! { <p class="muted">"No run in the route."</p> }.into_any();
                }
                view! {
                    {move || notice.get().map(|message| view! {
                        <div class="callout warn" role="status"><p>{message}</p></div>
                    })}
                    {move || match data.get() {
                        None => view! { <p class="muted">"Reading the run…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=refresh/>
                        }.into_any(),
                        Some(Ok(body)) => view! {
                            <RunDetailBody data=body compare_to=compare_to refresh=refresh notice=notice/>
                        }.into_any(),
                    }}
                }
                .into_any()
            }}
        </section>
    }
}

#[component]
fn RunDetailBody(
    data: DetailData,
    compare_to: RwSignal<Option<String>>,
    refresh: Callback<()>,
    notice: RwSignal<Option<String>>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let run = data.run.clone();
    let suite_label = data
        .suite
        .as_ref()
        .map(|row| row.slug.clone())
        .unwrap_or_else(|| "suite".to_owned());
    let run_id = run.id.clone();
    let cancel_status = run.status.clone();
    let can_cancel = move || {
        ctx.has_action("benchmark_runs.cancel")
            && (cancel_status == "queued" || cancel_status == "running")
    };
    let cancel_id = run.id.clone();
    let on_cancel = move |_| {
        let id = cancel_id.clone();
        let ctx = ctx;
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "benchmark_runs.cancel",
                    json!({"id": id}),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => refresh.run(()),
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    };

    let compare = data.compare.clone();
    let cases_a = data.cases_a.clone();
    let cases_b = data.cases_b.clone();
    let others = data.others.clone();
    let compare_to_value = data.compare_to.clone();
    let other_run = data
        .others
        .iter()
        .find(|row| Some(row.id.clone()) == compare_to_value)
        .cloned();
    let both_controls =
        run.control_ok.is_some() && other_run.as_ref().and_then(|row| row.control_ok).is_some();

    let on_compare = move |event: web_sys::Event| {
        if let Some(select) = event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::HtmlSelectElement>().ok())
        {
            compare_to.set(Some(select.value()));
        }
    };

    view! {
        <header class="screen-head">
            <div>
                <h1>{format!("{} · {}", suite_label, run.label)}</h1>
                <p class="muted">
                    {data.suite.as_ref().map(|suite| format!(
                        "{} @{} · seed {} · {} instances · {}/{}",
                        suite.dataset,
                        short_id(Some(&suite.dataset_revision)),
                        short_id(Some(&suite.seed_sha256)),
                        suite.instance_count,
                        run.arm,
                        run.subject_kind,
                    )).unwrap_or_else(|| run.label.clone())}
                </p>
            </div>
            <div class="filterbar">
                <StatusPill status=run.status.clone()/>
                <A href="/evals/benchmarks"><span class="btn small">"All runs"</span></A>
                {can_cancel().then(|| view! {
                    <button class="btn small danger" on:click=on_cancel>"Cancel"</button>
                })}
                <button class="btn small" on:click=move |_| refresh.run(())>"Refresh"</button>
            </div>
        </header>

        {run.error.clone().map(|error| view! {
            <div class="callout bad"><p>{error}</p></div>
        })}

        <div class="tiles">
            <Tile
                label="This run".to_owned()
                value=format!("{} / {}", fmt_int(i64::from(run.n_resolved)), fmt_int(i64::from(run.n_graded)))
                detail=format!("{} no-attempt · {}", fmt_int(i64::from(run.n_no_attempt)), run.arm)
            />
            {other_run.as_ref().map(|other| {
                let value = format!("{} / {}", fmt_int(i64::from(other.n_resolved)), fmt_int(i64::from(other.n_graded)));
                let detail = format!("{} · {}", other.label, other.arm);
                view! { <Tile label="Compared run".to_owned() value=value detail=detail/> }
            })}
            {compare.as_ref().and_then(|outcome| outcome.as_ref().ok()).filter(|outcome| outcome.comparable).map(|outcome| {
                let delta = outcome.a_only.unwrap_or(0) - outcome.b_only.unwrap_or(0);
                let value = if delta >= 0 { format!("+{delta}") } else { delta.to_string() };
                view! { <Tile label="Delta".to_owned() value=value detail=format!("discordant {}", outcome.discordant.unwrap_or(0))/> }
            })}
            {compare.as_ref().and_then(|outcome| outcome.as_ref().ok()).filter(|outcome| outcome.comparable).map(|outcome| {
                let p = outcome.p_value.unwrap_or(1.0);
                view! { <Tile label="McNemar exact".to_owned() value=format!("p={p:.2}") detail=if p < 0.05 { "significant".to_owned() } else { "not significant".to_owned() }/> }
            })}
        </div>

        <div class="split" style="margin-top:12px">
            <div>
                {match &compare {
                    Some(Err(error)) => view! { <div class="callout bad"><p>{error.to_string()}</p></div> }.into_any(),
                    Some(Ok(outcome)) if !outcome.comparable => view! {
                        <div class="callout warn" role="status">
                            <div class="hd">"Comparison refused"</div>
                            <p>{outcome.reason.clone().unwrap_or_else(|| "the runs are not comparable".to_owned())}</p>
                        </div>
                    }.into_any(),
                    Some(Ok(outcome)) => view! {
                        <section class="panel">
                            <div class="panel-head">
                                <h2>"Paired outcomes"</h2>
                                <span class="muted">
                                    {format!("n={}", outcome.both.unwrap_or(0) + outcome.a_only.unwrap_or(0) + outcome.b_only.unwrap_or(0) + outcome.neither.unwrap_or(0))}
                                    {outcome.missing_in_one.map(|missing| format!(" · {missing} unpaired"))}
                                </span>
                            </div>
                            <table class="wtbl">
                                <thead><tr><th></th><th class="n">{format!("{} ✓", other_label(&other_run))}</th><th class="n">{format!("{} ✗", other_label(&other_run))}</th></tr></thead>
                                <tbody>
                                    <tr><td><b>{format!("{} ✓", run.label)}</b></td><td class="n">{outcome.both.unwrap_or(0)}</td><td class="n">{outcome.a_only.unwrap_or(0)}</td></tr>
                                    <tr><td><b>{format!("{} ✗", run.label)}</b></td><td class="n">{outcome.b_only.unwrap_or(0)}</td><td class="n">{outcome.neither.unwrap_or(0)}</td></tr>
                                </tbody>
                            </table>
                            {both_controls.then(|| view! {
                                <p class="hint" style="margin-top:8px">
                                    "Both runs are controls: this is a suite check, not a subject comparison. A promotion decision needs a subject arm (S18)."
                                </p>
                            })}
                        </section>
                    }.into_any(),
                    None => view! {
                        <div class="callout">
                            <p>"No second run on this suite exists to compare against yet."</p>
                        </div>
                    }.into_any(),
                }}
            </div>
            <div>
                <section class="panel">
                    <div class="ph"><span>"Compare with"</span></div>
                    {if others.is_empty() {
                        view! { <Empty message="Only this run exists on the suite.".to_owned()/> }.into_any()
                    } else {
                        view! {
                            <select aria-label="Compare with" on:change=on_compare>
                                {others.clone().into_iter().map(|row| {
                                    let value = row.id.clone();
                                    let selected_value = value.clone();
                                    view! {
                                        <option value=value selected=move || compare_to.get().as_deref() == Some(selected_value.as_str())>
                                            {format!("{} · {}", row.label, short_id(Some(&row.id)))}
                                        </option>
                                    }
                                }).collect_view()}
                            </select>
                        }.into_any()
                    }}
                    <div class="kv" style="margin-top:10px">
                        <dt>"Status"</dt><dd>{run.status.clone()}</dd>
                        <dt>"Model"</dt><dd>{run.model.clone().unwrap_or_else(|| "—".to_owned())}</dd>
                        <dt>"Graded"</dt><dd>{fmt_int(i64::from(run.n_graded))}</dd>
                        <dt>"No attempt"</dt><dd>{fmt_int(i64::from(run.n_no_attempt))}</dd>
                        <dt>"Counts"</dt><dd class="mono">{run.counts.to_string()}</dd>
                        <dt class="adv">"Run id"</dt><dd class="adv mono">{run_id.clone()}</dd>
                        <dt class="adv">"Label"</dt><dd class="adv mono">{run.label.clone()}</dd>
                    </div>
                </section>
                <section class="panel" style="margin-top:12px">
                    <div class="ph"><span>"Controls"</span></div>
                    {control_pill(run.control_ok)}
                    <p class="muted" style="font-size:10.5px;margin-top:6px">
                        "A control that missed means the suite is broken, not the subject."
                    </p>
                </section>
            </div>
        </div>

        <section class="panel" style="margin-top:12px">
            <div class="panel-head">
                <h2>"Cases"</h2>
                <span class="muted">{format!("{} this run · {} compared", cases_a.len(), cases_b.len())}</span>
            </div>
            {if cases_a.is_empty() {
                view! { <Empty message="This run has no case rows yet.".to_owned()/> }.into_any()
            } else {
                view! {
                    <table class="wtbl">
                        <thead><tr>
                            <th>"Instance"</th><th>"Repo"</th>
                            <th class="n">{run.label.clone()}</th>
                            <th class="n">{other_label(&other_run)}</th>
                            <th class="n">"Time"</th>
                        </tr></thead>
                        <tbody>
                            {cases_a.clone().into_iter().map(|case| {
                                let other = cases_b
                                    .iter()
                                    .find(|row| row.instance_id == case.instance_id)
                                    .map(|row| row.verdict.clone());
                                view! {
                                    <tr>
                                        <td class="mono">{case.instance_id.clone()}</td>
                                        <td class="mono">{repo_of(&case.instance_id)}</td>
                                        <td class="n">{verdict_pill(&case.verdict)}</td>
                                        <td class="n">{verdict_cell(other.as_deref())}</td>
                                        <td class="n">{case.seconds.map(|seconds| format!("{seconds:.0} s")).unwrap_or_else(|| "—".to_owned())}</td>
                                    </tr>
                                }
                            }).collect_view()}
                        </tbody>
                    </table>
                }.into_any()
            }}
            <p class="hint adv blk" style="margin-top:8px">
                "benchmark_runs.get · benchmark_runs.compare · benchmark_cases.list · benchmark_runs.cancel"
            </p>
        </section>
    }
}

fn other_label(other: &Option<BenchmarkRun>) -> String {
    other
        .as_ref()
        .map(|row| row.label.clone())
        .unwrap_or_else(|| "—".to_owned())
}

fn repo_of(instance_id: &str) -> String {
    instance_id
        .split("__")
        .next()
        .unwrap_or(instance_id)
        .to_owned()
}

/// The compared run's cell: its verdict, or an em dash when the instance is unpaired.
fn verdict_cell(verdict: Option<&str>) -> AnyView {
    match verdict {
        Some(verdict) => verdict_pill(verdict),
        None => view! { <span class="muted">"—"</span> }.into_any(),
    }
}

fn verdict_pill(verdict: &str) -> AnyView {
    let (class, label) = match verdict {
        "resolved" => ("ok", "resolved".to_owned()),
        "unresolved" => ("bad", "unresolved".to_owned()),
        "apply_failed" => ("bad", "apply failed".to_owned()),
        "eval_timeout" => ("bad", "eval timeout".to_owned()),
        "infra_error" => ("bad", "infra error".to_owned()),
        "pending" => ("wait", "pending".to_owned()),
        other => ("muted", other.to_owned()),
    };
    view! { <span class=format!("pill {class}")>{label}</span> }.into_any()
}
