//! Improvement — the kata loop's read half (S12 screen 14, S18).
//!
//! Three reads and two writes, all through the registry:
//! `benchmark_suites.list` and `capability_agents.list` for the pickers, `benchmark_runs.trend`
//! for the points, `benchmark_runs.start` for a candidate and `benchmark_runs.promote` for the
//! baseline.
//!
//! The S18 §9.4 refusals are rendered, not hidden: a control, a non-conveyor baseline or a run
//! whose oracle/prompt differs is **shown with its reason** and never plotted against the
//! capability baseline. A point with no comparison is a point, not a zero.
//!
//! Two honest refusals belong on this screen because they are the states a person will meet:
//! starting a `conveyor` run needs a configured sandbox (`SANDBOX_NAMESPACE`; `503` in dev) and a
//! model provider behind `agent.run`. The form surfaces the API's own answer verbatim.

use leptos::prelude::*;
use serde::Deserialize;
use serde_json::json;

use crate::api::{ApiError, InvokeOptions, Page};
use crate::components::{Empty, ErrorPanel, Planned, StatusPill, Tile, fmt_pct, short_id};
use crate::state::AppCtx;

#[derive(Clone, Debug, Deserialize)]
struct Suite {
    id: String,
    slug: String,
    #[serde(default)]
    instance_count: i64,
}

#[derive(Clone, Debug, Deserialize)]
struct Agent {
    id: String,
    key: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    capability_id: String,
}

#[derive(Clone, Debug, Deserialize)]
struct Compare {
    comparable: bool,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    a_only: Option<usize>,
    #[serde(default)]
    b_only: Option<usize>,
    #[serde(default)]
    p_value: Option<f64>,
}

#[derive(Clone, Debug, Deserialize)]
struct Point {
    run_id: String,
    arm: String,
    #[serde(default)]
    subject_kind: String,
    #[serde(default)]
    subject_version: Option<i64>,
    #[serde(default)]
    label: String,
    #[serde(default)]
    n_graded: i32,
    #[serde(default)]
    n_resolved: i32,
    #[serde(default)]
    rate: Option<f64>,
    #[serde(default)]
    control_ok: Option<bool>,
    #[serde(default)]
    direction: Option<String>,
    #[serde(default)]
    vs_baseline: Option<Compare>,
}

#[derive(Clone, Debug, Deserialize)]
struct Trend {
    #[serde(default)]
    baseline_run_id: Option<String>,
    #[serde(default)]
    points: Vec<Point>,
}

fn parse<T: serde::de::DeserializeOwned>(
    value: serde_json::Value,
    what: &str,
) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Network(format!("{what}: {error}")))
}

fn load_suites(ctx: AppCtx) -> LocalResource<Result<Vec<Suite>, ApiError>> {
    LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        async move {
            let page: Page<Suite> = parse(
                ctx.invoke(
                    "benchmark_suites.list",
                    json!({"limit": 50}),
                    InvokeOptions::default(),
                )
                .await?
                .output,
                "benchmark suites",
            )?;
            Ok(page.items)
        }
    })
}

fn load_agents(ctx: AppCtx) -> LocalResource<Result<Vec<Agent>, ApiError>> {
    LocalResource::new(move || {
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
            Ok(page.items)
        }
    })
}

fn load_trend(
    ctx: AppCtx,
    suite: RwSignal<Option<String>>,
) -> LocalResource<Result<Trend, ApiError>> {
    LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let suite = suite.get();
        async move {
            let Some(suite) = suite else {
                return Ok(Trend {
                    baseline_run_id: None,
                    points: Vec::new(),
                });
            };
            parse(
                ctx.invoke(
                    "benchmark_runs.trend",
                    json!({"suite_id": suite, "limit": 100}),
                    InvokeOptions::default(),
                )
                .await?
                .output,
                "benchmark trend",
            )
        }
    })
}

fn direction_label(direction: Option<&str>) -> (&'static str, &'static str) {
    match direction {
        Some("candidate_better") => ("candidate better", "ok"),
        Some("baseline_better") => ("baseline better", "bad"),
        Some("unchanged") => ("unchanged", "muted"),
        _ => ("—", "muted"),
    }
}

#[component]
pub fn Improvement() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let suites = load_suites(ctx);
    let agents = load_agents(ctx);
    let suite = RwSignal::new(Option::<String>::None);
    let trend = load_trend(ctx, suite);
    let notice = RwSignal::new(Option::<String>::None);
    let busy = RwSignal::new(false);
    let agent_id = RwSignal::new(String::new());
    let baseline = RwSignal::new(String::new());
    let label = RwSignal::new(String::new());

    // Default the suite picker to the first one the real read returns.
    Effect::new(move |_| {
        if suite.get_untracked().is_none()
            && let Some(Ok(items)) = suites.get()
            && let Some(first) = items.first()
        {
            suite.set(Some(first.id.clone()));
        }
    });

    let start_candidate = Callback::new(move |event: web_sys::SubmitEvent| {
        // A form with no prevent_default navigates: the action still runs, then the page reloads
        // and the refusal it produced is thrown away. Measured in the browser, 2026-11-10.
        event.prevent_default();
        let Some(suite_id) = suite.get_untracked() else {
            notice.set(Some("Choose a suite first.".into()));
            return;
        };
        let agent = agent_id.get_untracked();
        if agent.is_empty() {
            notice.set(Some(
                "Choose the capability agent whose version is being measured.".into(),
            ));
            return;
        }
        let label = {
            let typed = label.get_untracked().trim().to_owned();
            if typed.is_empty() {
                format!("conveyor-{}", chrono::Utc::now().format("%Y%m%dT%H%M%S"))
            } else {
                typed
            }
        };
        let mut input = json!({
            "suite_id": suite_id,
            "arm": "conveyor",
            "subject_kind": "capability_agent",
            "capability_agent_id": agent,
            "label": label,
        });
        let baseline = baseline.get_untracked();
        if !baseline.is_empty() {
            input["baseline_run_id"] = json!(baseline);
        }
        notice.set(None);
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
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .to_owned();
                    notice.set(Some(format!(
                        "Candidate started ({}). It is measured by the driver and compared by the improvement job; nothing is promoted automatically.",
                        short_id(Some(&id))
                    )));
                    trend.refetch();
                }
                // 503 (no sandbox), 409 arm_not_ready and a 422 all arrive as problem documents;
                // the screen repeats the server's own sentence.
                Err(error) => notice.set(Some(error.to_string())),
            }
            busy.set(false);
        });
    });

    let promote = Callback::new(move |run_id: String| {
        busy.set(true);
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "benchmark_runs.promote",
                    json!({"id": run_id}),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => {
                    notice.set(Some(
                        "Promoted: every later point is compared against this run.".into(),
                    ));
                    trend.refetch();
                    suites.refetch();
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
            busy.set(false);
        });
    });

    view! {
        <section class="screen measure">
            {move || {
                if !(ctx.has_action("benchmark_runs.trend") && ctx.has_action("benchmark_suites.list")) {
                    return view! {
                        <Planned
                            title="Improvement is planned".to_owned()
                            detail="benchmark_runs.trend / benchmark_suites.list are not in this role's manifest. The screen appears when the actions land, not before.".to_owned()
                        />
                    }
                    .into_any();
                }
                view! {
                    <header class="screen-head">
                        <div>
                            <h1>"Improvement"</h1>
                            <p class="muted">
                                "Rate per measured version against the promoted baseline. A control is shown with its reason and never plotted; a regression is a significant delta, not a smaller number."
                            </p>
                        </div>
                        <div class="filterbar">
                            <label class="filter">
                                <span>"Suite"</span>
                                <select
                                    aria-label="Suite"
                                    on:change=move |event| {
                                        let value = event_target_value(&event);
                                        suite.set((!value.is_empty()).then_some(value));
                                    }
                                >
                                    {move || {
                                        let items = match suites.get() { Some(Ok(items)) => items, _ => Vec::new() };
                                        let options = items.into_iter().map(|row| {
                                            let value = row.id.clone();
                                            let selected_value = value.clone();
                                            view! {
                                                <option value=value selected=move || suite.get().as_deref() == Some(selected_value.as_str())>
                                                    {format!("{} · {} cases", row.slug, row.instance_count)}
                                                </option>
                                            }
                                        }).collect_view();
                                        view! { {options} }.into_any()
                                    }}
                                </select>
                            </label>
                            <button class="btn" on:click=move |_| trend.refetch()>"Refresh"</button>
                        </div>
                    </header>
                    {move || notice.get().map(|message| view! {
                        <div class="callout warn" role="status"><p>{message}</p></div>
                    })}
                    {move || match suites.get() {
                        Some(Err(error)) => view! { <ErrorPanel error=error on_retry=Callback::new(move |_| suites.refetch())/> }.into_any(),
                        Some(Ok(items)) if items.is_empty() => view! {
                            <Empty message="No benchmark suite in this tenant. Seeding a suite is `migrate --seed`.".to_owned()/>
                        }.into_any(),
                        _ => view! { <div></div> }.into_any(),
                    }}
                    {move || match trend.get() {
                        None => view! { <p class="muted">"Reading the trend…"</p> }.into_any(),
                        Some(Err(error)) => view! { <ErrorPanel error=error on_retry=Callback::new(move |_| trend.refetch())/> }.into_any(),
                        Some(Ok(view_data)) => {
                            let total = view_data.points.len();
                            let plotted = view_data
                                .points
                                .iter()
                                .filter(|point| point.vs_baseline.as_ref().map(|compare| compare.comparable).unwrap_or(false))
                                .count();
                            let regressions = view_data
                                .points
                                .iter()
                                .filter(|point| point.direction.as_deref() == Some("baseline_better"))
                                .count();
                            view! {
                                <div class="tiles">
                                    <Tile label="Baseline".to_owned() value=view_data.baseline_run_id.as_deref().map(|id| short_id(Some(id))).unwrap_or_else(|| "none".to_owned()) detail="the suite's promoted run".to_owned()/>
                                    <Tile label="Points".to_owned() value=total.to_string() detail="succeeded runs, oldest first".to_owned()/>
                                    <Tile label="Comparable".to_owned() value=format!("{plotted}/{total}") detail="plotted against the baseline".to_owned()/>
                                    <Tile label="Regressions".to_owned() value=regressions.to_string() detail="significant, baseline better".to_owned()/>
                                </div>
                                {if view_data.points.is_empty() {
                                    view! { <Empty message="This suite has no succeeded run yet. Start a control (gold/empty) on the Benchmarks screen, or a candidate below.".to_owned()/> }.into_any()
                                } else {
                                    view! {
                                        <div class="tablewrap">
                                            <table class="table">
                                                <thead><tr>
                                                    <th>"Run"</th><th>"Arm"</th><th>"Subject"</th>
                                                    <th class="n">"Resolved"</th><th class="n">"Rate"</th>
                                                    <th>"Control"</th><th>"vs baseline"</th><th>"Δ"</th><th></th>
                                                </tr></thead>
                                                <tbody>
                                                    {view_data.points.iter().map(|point| {
                                                        let run = short_id(Some(&point.run_id));
                                                        let (direction, tone) = direction_label(point.direction.as_deref());
                                                        let is_baseline = view_data.baseline_run_id.as_deref() == Some(point.run_id.as_str());
                                                        let promote_id = point.run_id.clone();
                                                        let can_promote = ctx.has_action("benchmark_runs.promote")
                                                            && point.arm == "conveyor"
                                                            && !is_baseline;
                                                        view! {
                                                            <tr>
                                                                <td class="mono">{run}</td>
                                                                <td><StatusPill status=point.arm.clone()/></td>
                                                                <td class="mono">
                                                                    {match (point.subject_kind.as_str(), point.subject_version) {
                                                                        ("suite", _) => "suite".to_owned(),
                                                                        (kind, Some(version)) => format!("{kind} v{version}"),
                                                                        (kind, None) => kind.to_owned(),
                                                                    }}
                                                                </td>
                                                                <td class="n">{format!("{}/{}", point.n_resolved, point.n_graded)}</td>
                                                                <td class="n">{fmt_pct(point.rate)}</td>
                                                                <td class="mono">{match point.control_ok {
                                                                    Some(true) => "ok",
                                                                    Some(false) => "missed",
                                                                    None => "—",
                                                                }}</td>
                                                                <td class="mono">
                                                                    {match &point.vs_baseline {
                                                                        Some(compare) if compare.comparable => format!(
                                                                            "a-only {} · b-only {} · p {}",
                                                                            compare.a_only.unwrap_or(0),
                                                                            compare.b_only.unwrap_or(0),
                                                                            compare.p_value.map(|p| format!("{p:.3}")).unwrap_or_else(|| "—".to_owned()),
                                                                        ),
                                                                        Some(compare) => format!("not plotted — {}", compare.reason.clone().unwrap_or_default()),
                                                                        None if is_baseline => "this is the baseline".to_owned(),
                                                                        None => "no baseline".to_owned(),
                                                                    }}
                                                                </td>
                                                                <td><span class=format!("pill {tone}")>{direction}</span></td>
                                                                <td>
                                                                    {can_promote.then(|| view! {
                                                                        <button class="btn small" type="button" disabled=move || busy.get()
                                                                            on:click=move |_| promote.run(promote_id.clone())>
                                                                            "Promote"
                                                                        </button>
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
                                {move || ctx.has_action("benchmark_runs.start").then(|| view! {
                                    <form class="panel" on:submit=move |event| start_candidate.run(event)>
                                        <h2>"Start a candidate"</h2>
                                        <p class="muted">
                                            "One conveyor run of this suite, pinned to the capability agent's version at start. With a baseline it is enqueued for the improvement job, which compares and records keep / revert / inconclusive; it never promotes."
                                        </p>
                                        <div class="formrow">
                                            <label class="filter">
                                                <span>"Capability agent"</span>
                                                <select aria-label="Capability agent" on:change=move |event| agent_id.set(event_target_value(&event))>
                                                    <option value="">"choose…"</option>
                                                    {move || {
                                                        let items = match agents.get() { Some(Ok(items)) => items, _ => Vec::new() };
                                                        let options = items.into_iter().filter(|agent| agent.status == "active").map(|agent| {
                                                            let value = agent.id.clone();
                                                            view! { <option value=value>{format!("{} · {}", agent.key, short_id(Some(&agent.capability_id)))}</option> }
                                                        }).collect_view();
                                                        view! { {options} }.into_any()
                                                    }}
                                                </select>
                                            </label>
                                            <label class="filter">
                                                <span>"Baseline"</span>
                                                <select aria-label="Baseline" on:change=move |event| baseline.set(event_target_value(&event))>
                                                    <option value="">"none (record only)"</option>
                                                    {move || {
                                                        let items = match trend.get() { Some(Ok(view_data)) => view_data.points, _ => Vec::new() };
                                                        let options = items.into_iter().filter(|point| point.arm == "conveyor").map(|point| {
                                                            let value = point.run_id.clone();
                                                            let label = format!("{} · {}", short_id(Some(&point.run_id)), point.label);
                                                            view! { <option value=value>{label}</option> }
                                                        }).collect_view();
                                                        view! { {options} }.into_any()
                                                    }}
                                                </select>
                                            </label>
                                            <label class="filter">
                                                <span>"Label"</span>
                                                <input type="text" aria-label="Label" prop:value=move || label.get()
                                                    on:input=move |event| label.set(event_target_value(&event))
                                                    placeholder="conveyor-<stamp> (default)"/>
                                            </label>
                                            <button class="btn primary" type="submit" disabled=move || busy.get()>
                                                {move || if busy.get() { "Starting…" } else { "Start a candidate" }}
                                            </button>
                                        </div>
                                        <p class="hint">
                                            "benchmark_runs.start — a missing sandbox is 503 and an unready capability agent is 409; both are shown above, never simulated."
                                        </p>
                                    </form>
                                })}
                            }.into_any()
                        }
                    }}
                }
                .into_any()
            }}
        </section>
    }
}
