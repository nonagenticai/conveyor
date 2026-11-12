//! Usage & cache — S16's inference ledger, measured.
//!
//! Definitions are the design's, not a convenient server field: the headline ratio is
//! `hits ÷ (hits + misses)`; a write is a first fill, not a miss; `unknown` is its own bucket.
//! Cost is never a stored column — every figure is read from `inference_call` at read time.
//!
//! The panel is honest about absence: without the actions in the manifest it says so, and a
//! tenant with no calls gets an empty state, never a drawn chart. The UI invents no fixture.

use chrono::{DateTime, Duration, Utc};
use leptos::prelude::*;
use serde_json::{Value, json};
use wasm_bindgen::JsCast;

use crate::api::{ApiError, Client, InvokeOptions, Page};
use crate::components::{
    Empty, ErrorPanel, Planned, Tile, age, fmt_compact, fmt_int, fmt_pct, hit_rate, short_id,
};
use crate::models::{InferenceCall, UsageReport, UsageRow};
use crate::screens::billing::BillingPanel;
use crate::state::AppCtx;

const WINDOWS: [(i64, &str); 3] = [
    (7, "last 7 days"),
    (30, "last 30 days"),
    (90, "last 90 days"),
];
const CALLER_KINDS: [(&str, &str); 6] = [
    ("", "all callers"),
    ("copilot", "copilot"),
    ("process_step", "process step"),
    ("workflow_step", "workflow step"),
    ("agent_run", "agent run"),
    ("other", "other"),
];
const CACHE_KINDS: [(&str, &str); 5] = [
    ("", "all classes"),
    ("hit", "hit"),
    ("write", "write"),
    ("miss", "miss"),
    ("unknown", "unknown"),
];

#[derive(Clone, Debug)]
struct UsageData {
    days: UsageReport,
    models: UsageReport,
    callers: UsageReport,
    prompts: UsageReport,
    calls: Vec<InferenceCall>,
    has_more: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct Totals {
    calls: i64,
    prompt_tokens: i64,
    completion_tokens: i64,
    hits: i64,
    writes: i64,
    misses: i64,
    unknown: i64,
}

impl Totals {
    fn of(rows: &[UsageRow]) -> Self {
        rows.iter().fold(Self::default(), |mut sum, row| {
            sum.calls += row.calls;
            sum.prompt_tokens += row.prompt_tokens;
            sum.completion_tokens += row.completion_tokens;
            sum.hits += row.hits;
            sum.writes += row.writes;
            sum.misses += row.misses;
            sum.unknown += row.unknown;
            sum
        })
    }
}

async fn report(
    client: &Client,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
    group: &str,
) -> Result<UsageReport, ApiError> {
    let output = client
        .invoke(
            "inference_usage.report",
            json!({ "since": since, "until": until, "group_by": group }),
            InvokeOptions::default(),
        )
        .await?
        .output;
    serde_json::from_value(output)
        .map_err(|error| ApiError::Network(format!("usage report ({group}): {error}")))
}

fn load(
    ctx: AppCtx,
    window: RwSignal<i64>,
    limit: RwSignal<u32>,
    caller: RwSignal<String>,
    model: RwSignal<String>,
    cache: RwSignal<String>,
) -> LocalResource<Result<UsageData, ApiError>> {
    LocalResource::new(move || async move {
        // Every reactive input is read BEFORE the first await: a signal read after a yield is
        // not tracked, and the resource would silently stop refetching on a filter change.
        let _tenant = ctx.tenant.get();
        let days = window.get();
        let page_limit = limit.get();
        let caller = caller.get();
        let model = model.get();
        let cache = cache.get();
        let until = Utc::now();
        let since = until - Duration::days(days);
        let client = ctx.client();

        let day_rows = report(&client, since, until, "day").await?;
        let models = report(&client, since, until, "model").await?;
        let callers = report(&client, since, until, "caller_kind").await?;
        let prompts = report(&client, since, until, "prompt_sha").await?;

        let mut input = serde_json::Map::new();
        if !caller.is_empty() {
            input.insert("caller_kind".into(), json!(caller));
        }
        if !model.is_empty() {
            input.insert("model".into(), json!(model));
        }
        if !cache.is_empty() {
            input.insert("cache_status".into(), json!(cache));
        }
        // `PageIn` is `#[serde(flatten)]` into the action input: `limit` is top-level.
        input.insert("limit".into(), json!(page_limit));
        let output = client
            .invoke(
                "inference_calls.list",
                Value::Object(input),
                InvokeOptions::default(),
            )
            .await?
            .output;
        let page: Page<InferenceCall> = serde_json::from_value(output)
            .map_err(|error| ApiError::Network(format!("inference calls: {error}")))?;
        Ok(UsageData {
            days: day_rows,
            models,
            callers,
            prompts,
            has_more: page.next_cursor.is_some(),
            calls: page.items,
        })
    })
}

#[component]
pub fn Usage() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let window = RwSignal::new(7i64);
    let limit = RwSignal::new(50u32);
    let caller = RwSignal::new(String::new());
    let model = RwSignal::new(String::new());
    let cache = RwSignal::new(String::new());
    let data = load(ctx, window, limit, caller, model, cache);
    let retry = Callback::new(move |_| data.refetch());

    let window_label = move || {
        WINDOWS
            .iter()
            .find(|(days, _)| *days == window.get())
            .map(|(_, label)| *label)
            .unwrap_or("last 7 days")
    };

    view! {
        <section class="screen measure">
            {move || {
                if !(ctx.has_action("inference_usage.report")
                    && ctx.has_action("inference_calls.list"))
                {
                    return view! {
                        <Planned
                            title="Usage & cache is planned".to_owned()
                            detail="inference_usage.report / inference_calls.list are not in this role's manifest. The panel appears when the action lands, not before.".to_owned()
                        />
                    }
                    .into_any();
                }
                let on_window = move |event: web_sys::Event| {
                    if let Some(select) = event
                        .target()
                        .and_then(|target| target.dyn_into::<web_sys::HtmlSelectElement>().ok())
                        && let Ok(value) = select.value().parse::<i64>()
                    {
                        window.set(value);
                        limit.set(50);
                    }
                };
                view! {
                    <header class="screen-head">
                        <div>
                            <h1>"Usage & cache"</h1>
                            <p class="muted">
                                {move || window_label()} " · inference_call · cost is derived, never stored"
                            </p>
                        </div>
                        <div class="filterbar">
                            <label class="filter">
                                <span>"Window"</span>
                                <select on:change=on_window aria-label="Usage window">
                                    {WINDOWS.iter().map(|(days, label)| {
                                        let days = *days;
                                        view! {
                                            <option value=days.to_string() selected=move || window.get() == days>
                                                {*label}
                                            </option>
                                        }
                                    }).collect_view()}
                                </select>
                            </label>
                            <button class="btn" on:click=move |_| data.refetch()>"Refresh"</button>
                        </div>
                    </header>
                    {move || match data.get() {
                        None => view! { <p class="muted">"Loading the ledger…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=retry/>
                        }.into_any(),
                        Some(Ok(data)) => view! {
                            <UsageBody
                                data=data
                                limit=limit
                                caller=caller
                                model=model
                                cache=cache
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
fn UsageBody(
    data: UsageData,
    limit: RwSignal<u32>,
    caller: RwSignal<String>,
    model: RwSignal<String>,
    cache: RwSignal<String>,
) -> impl IntoView {
    let totals = Totals::of(&data.days.rows);
    let model_rows = data.models.rows.clone();
    let caller_rows = data.callers.rows.clone();
    let model_count = model_rows.len();
    let caller_count = caller_rows.len();
    let mut prompt_rows = data.prompts.rows.clone();
    // Cache-hostile first: a prompt written and never read is the failure this screen exists
    // to show; then the lowest hit rate.
    prompt_rows.sort_by(|a, b| {
        let a_bad = a.writes > 0 && a.hits == 0;
        let b_bad = b.writes > 0 && b.hits == 0;
        b_bad.cmp(&a_bad).then_with(|| {
            let left = hit_rate(a.hits, a.misses).unwrap_or(-1.0);
            let right = hit_rate(b.hits, b.misses).unwrap_or(-1.0);
            left.partial_cmp(&right)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });

    let never_read: Vec<&UsageRow> = prompt_rows
        .iter()
        .filter(|row| row.writes > 0 && row.hits == 0)
        .collect();
    let never_writes: i64 = never_read.iter().map(|row| row.writes).sum();

    let mut chart_rows = data.days.rows.clone();
    chart_rows.sort_by(|a, b| a.bucket.cmp(&b.bucket));
    let chart_rows: Vec<UsageRow> = chart_rows
        .into_iter()
        .rev()
        .take(14)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let max_calls = chart_rows
        .iter()
        .map(|row| row.calls)
        .max()
        .unwrap_or(0)
        .max(1) as f64;

    let model_options = model_rows.clone();
    let calls = data.calls.clone();

    let on_caller = move |event: web_sys::Event| {
        if let Some(select) = event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::HtmlSelectElement>().ok())
        {
            caller.set(select.value());
            limit.set(50);
        }
    };
    let on_model = move |event: web_sys::Event| {
        if let Some(select) = event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::HtmlSelectElement>().ok())
        {
            model.set(select.value());
            limit.set(50);
        }
    };
    let on_cache = move |event: web_sys::Event| {
        if let Some(select) = event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::HtmlSelectElement>().ok())
        {
            cache.set(select.value());
            limit.set(50);
        }
    };

    let summary = move || {
        format!(
            "{} calls · {model_count} models · {caller_count} callers",
            fmt_int(totals.calls)
        )
    };
    let filters_active =
        move || !caller.get().is_empty() || !model.get().is_empty() || !cache.get().is_empty();

    view! {
        {if totals.calls == 0 {
            view! {
                <div class="panel" role="status">
                    <h2>"No inference calls in this window"</h2>
                    <p class="muted">
                        "The ledger fills as the app calls models. Widen the window or clear the filters."
                    </p>
                </div>
            }.into_any()
        } else {
            view! {
                <p class="muted summary">{summary}</p>
                <div class="tiles">
                    <Tile
                        label="Cache hit ratio".to_owned()
                        value=fmt_pct(hit_rate(totals.hits, totals.misses))
                        detail=format!("{} hits · {} misses", fmt_int(totals.hits), fmt_int(totals.misses))
                    />
                    <Tile
                        label="Calls".to_owned()
                        value=fmt_int(totals.calls)
                        detail=format!("{} writes · {} unknown", fmt_int(totals.writes), fmt_int(totals.unknown))
                    />
                    <Tile
                        label="Tokens".to_owned()
                        value=fmt_compact(totals.prompt_tokens + totals.completion_tokens)
                        detail=format!("{} prompt · {} out", fmt_compact(totals.prompt_tokens), fmt_compact(totals.completion_tokens))
                    />
                    <Tile
                        label="Writes never read".to_owned()
                        value=fmt_compact(never_writes)
                        detail=format!("{} prompt hashes", never_read.len())
                    />
                </div>

                <section class="panel" aria-labelledby="usage-chart-title">
                    <div class="panel-head">
                        <h2 id="usage-chart-title">"Cache classes by day"</h2>
                        <span class="muted">"calls · last 14 day buckets"</span>
                    </div>
                    <div class="chart">
                        <div class="plot" role="img" aria-label="Stacked bars of cache hits, writes, misses and unknowns by day">
                            {chart_rows.iter().map(|row| {
                                let height = |count: i64| format!("{:.2}%", count as f64 / max_calls * 100.0);
                                let title = format!(
                                    "{} · {} calls · {} hit, {} write, {} miss, {} unknown",
                                    row.bucket, row.calls, row.hits, row.writes, row.misses, row.unknown
                                );
                                view! {
                                    <div class="col" title=title>
                                        {(row.unknown > 0).then(|| view! { <div class="seg unk" style=height(row.unknown)></div> })}
                                        {(row.misses > 0).then(|| view! { <div class="seg miss" style=height(row.misses)></div> })}
                                        {(row.writes > 0).then(|| view! { <div class="seg wr" style=height(row.writes)></div> })}
                                        {(row.hits > 0).then(|| view! { <div class="seg hit" style=height(row.hits)></div> })}
                                    </div>
                                }
                            }).collect_view()}
                        </div>
                        <div class="axis">
                            {chart_rows.iter().map(|row| view! { <span>{day_label(&row.bucket)}</span> }).collect_view()}
                        </div>
                        <div class="legend">
                            <span><i class="sw hit"></i>"hit"</span>
                            <span><i class="sw wr"></i>"write"</span>
                            <span><i class="sw miss"></i>"miss"</span>
                            <span><i class="sw unk"></i>"unknown"</span>
                            <span class="muted">"hit ratio = hits ÷ (hits + misses); writes are first fills"</span>
                        </div>
                    </div>
                </section>

                <div class="grid-2">
                    <section class="panel" aria-labelledby="usage-models-title">
                        <div class="panel-head">
                            <h2 id="usage-models-title">"By model"</h2>
                            <span class="muted">{format!("{} models", model_rows.len())}</span>
                        </div>
                        <table class="wtbl">
                            <thead><tr>
                                <th>"Model"</th><th class="n">"Calls"</th><th class="n">"Hit"</th>
                                <th class="n">"Writes"</th><th class="n">"Misses"</th><th class="n">"Tokens"</th>
                                <th class="n adv">"Server hits/calls"</th>
                            </tr></thead>
                            <tbody>
                                {model_rows.iter().map(|row| view! {
                                    <tr>
                                        <td class="mono">{row.bucket.clone()}</td>
                                        <td class="n">{fmt_int(row.calls)}</td>
                                        <td class="n">{fmt_pct(hit_rate(row.hits, row.misses))}</td>
                                        <td class="n">{fmt_int(row.writes)}</td>
                                        <td class="n">{fmt_int(row.misses)}</td>
                                        <td class="n">{fmt_compact(row.prompt_tokens + row.completion_tokens)}</td>
                                        <td class="n adv">{format!("{}%", (row.hit_ratio * 100.0).round() as i64)}</td>
                                    </tr>
                                }).collect_view()}
                            </tbody>
                        </table>
                    </section>

                    <section class="panel" aria-labelledby="usage-callers-title">
                        <div class="panel-head">
                            <h2 id="usage-callers-title">"By caller"</h2>
                            <span class="muted">{format!("{} callers", caller_rows.len())}</span>
                        </div>
                        <table class="wtbl">
                            <thead><tr>
                                <th>"Caller"</th><th class="n">"Calls"</th><th class="n">"Hit"</th>
                                <th class="n">"Writes"</th><th class="n">"Misses"</th><th class="n">"Tokens"</th>
                            </tr></thead>
                            <tbody>
                                {caller_rows.iter().map(|row| view! {
                                    <tr>
                                        <td class="mono">{row.bucket.clone()}</td>
                                        <td class="n">{fmt_int(row.calls)}</td>
                                        <td class="n">{fmt_pct(hit_rate(row.hits, row.misses))}</td>
                                        <td class="n">{fmt_int(row.writes)}</td>
                                        <td class="n">{fmt_int(row.misses)}</td>
                                        <td class="n">{fmt_compact(row.prompt_tokens + row.completion_tokens)}</td>
                                    </tr>
                                }).collect_view()}
                            </tbody>
                        </table>
                    </section>
                </div>

                <section class="panel" aria-labelledby="usage-prompts-title">
                    <div class="panel-head">
                        <h2 id="usage-prompts-title">"By prompt hash — cache-hostile first"</h2>
                        <span class="muted">{format!("{} prompt hashes", prompt_rows.len())}</span>
                    </div>
                    <table class="wtbl">
                        <thead><tr>
                            <th>"Prompt"</th><th class="n">"Calls"</th><th class="n">"Writes"</th>
                            <th class="n">"Reads"</th><th class="n">"Hit"</th><th>"State"</th>
                        </tr></thead>
                        <tbody>
                            {prompt_rows.iter().map(|row| {
                                let ratio = hit_rate(row.hits, row.misses);
                                let state = if row.writes > 0 && row.hits == 0 {
                                    view! { <span class="pill bad">"never read"</span> }.into_any()
                                } else if row.hits > 0 {
                                    view! { <span class="pill ok">"cached"</span> }.into_any()
                                } else if row.misses > 0 {
                                    view! { <span class="pill wait">"all miss"</span> }.into_any()
                                } else {
                                    view! { <span class="pill muted">"unknown"</span> }.into_any()
                                };
                                let full = row.bucket.clone();
                                let shown = if row.bucket.is_empty() { "(no hash)".to_owned() } else { short_id(Some(&row.bucket)) };
                                view! {
                                    <tr>
                                        <td>
                                            <span class="mono">{shown}</span>
                                            <span class="adv mono">" "{full.clone()}</span>
                                        </td>
                                        <td class="n">{fmt_int(row.calls)}</td>
                                        <td class="n">{fmt_int(row.writes)}</td>
                                        <td class="n">{fmt_int(row.hits)}</td>
                                        <td class="n">{fmt_pct(ratio)}</td>
                                        <td>{state}</td>
                                    </tr>
                                }
                            }).collect_view()}
                        </tbody>
                    </table>
                </section>

                <section class="panel" aria-labelledby="usage-calls-title">
                    <div class="panel-head">
                        <h2 id="usage-calls-title">"Calls"</h2>
                        <span class="muted">
                            {move || if filters_active() { "filtered" } else { "newest first" }}
                        </span>
                    </div>
                    <div class="filterbar">
                        <label class="filter">
                            <span>"Caller"</span>
                            <select on:change=on_caller aria-label="Caller filter">
                                {CALLER_KINDS.iter().map(|(value, label)| {
                                    let value = *value;
                                    view! {
                                        <option value=value selected=move || caller.get() == value>{*label}</option>
                                    }
                                }).collect_view()}
                            </select>
                        </label>
                        <label class="filter">
                            <span>"Model"</span>
                            <select on:change=on_model aria-label="Model filter">
                                <option value="" selected=move || model.get().is_empty()>"all models"</option>
                                {model_options.iter().map(|row| {
                                    let value = row.bucket.clone();
                                    let selected = value.clone();
                                    view! {
                                        <option value=value selected=move || model.get() == selected>{row.bucket.clone()}</option>
                                    }
                                }).collect_view()}
                            </select>
                        </label>
                        <label class="filter">
                            <span>"Cache class"</span>
                            <select on:change=on_cache aria-label="Cache class filter">
                                {CACHE_KINDS.iter().map(|(value, label)| {
                                    let value = *value;
                                    view! {
                                        <option value=value selected=move || cache.get() == value>{*label}</option>
                                    }
                                }).collect_view()}
                            </select>
                        </label>
                    </div>
                    {if calls.is_empty() {
                        view! { <Empty message="No calls match these filters.".to_owned()/> }.into_any()
                    } else {
                        view! {
                            <table class="wtbl calls">
                                <thead><tr>
                                    <th>"When"</th><th>"Caller"</th><th>"Model"</th><th>"Prompt"</th>
                                    <th>"Cache"</th><th class="n">"Tokens"</th><th class="n">"Latency"</th><th>"Outcome"</th>
                                    <th class="adv">"Ids"</th>
                                </tr></thead>
                                <tbody>
                                    {calls.iter().map(|call| view! {
                                        <tr>
                                            <td title=call.created_at.to_rfc3339()>{age(Some(call.created_at))}</td>
                                            <td class="mono">{call.caller_kind.clone()}</td>
                                            <td class="mono">{call.model.clone()}</td>
                                            <td>
                                                <span class="mono">
                                                    {call.prompt_ref.clone().unwrap_or_else(|| "—".to_owned())}
                                                    {call.prompt_version.clone().map(|version| format!("@{version}"))}
                                                </span>
                                                <span class="adv mono">" " {short_id(call.prompt_sha256.as_deref())}</span>
                                            </td>
                                            <td>{cache_pill(&call.cache_status)}</td>
                                            <td class="n">{format!("{} / {}", fmt_compact(i64::from(call.prompt_tokens)), fmt_compact(i64::from(call.completion_tokens)))}</td>
                                            <td class="n">{format!("{} ms", call.latency_ms)}</td>
                                            <td>{outcome_pill(&call.outcome)}</td>
                                            <td class="adv mono">
                                                {short_id(Some(&call.id))}
                                                {call.trace_id.clone().map(|trace| format!(" · {trace}"))}
                                                {call.run_id.clone().map(|run| format!(" · run {run}"))}
                                                {call.conversation_turn_id.clone().map(|turn| format!(" · turn {turn}"))}
                                            </td>
                                        </tr>
                                    }).collect_view()}
                                </tbody>
                            </table>
                            <div class="pager">
                                {move || {
                                    let shown = limit.get().min(200);
                                    if data.has_more && limit.get() < 200 {
                                        view! {
                                            <button class="btn small" on:click=move |_| limit.update(|value| *value = (*value + 50).min(200))>
                                                "Show more"
                                            </button>
                                        }.into_any()
                                    } else if data.has_more {
                                        view! { <span class="muted">"Newest 200 shown; narrow the window for older calls."</span> }.into_any()
                                    } else {
                                        view! { <span class="muted">{format!("Newest {shown}.")}</span> }.into_any()
                                    }
                                }}
                            </div>
                        }.into_any()
                    }}
                    <p class="hint adv blk">
                        "inference_usage.report group_by=day|model|caller_kind|prompt_sha · "
                        "inference_calls.list · export has no action in this manifest"
                    </p>
                </section>
                // The ledger above is the conveyor's own trace; billing is the money, read
                // from billing_service. Two sources, never added together.
                {move || expect_context::<AppCtx>().tenant.get().map(|tenant_id| view! {
                    <BillingPanel tenant_id=tenant_id/>
                })}
            }.into_any()
        }}
    }
}

fn cache_pill(status: &str) -> impl IntoView {
    let (class, label) = match status {
        "hit" => ("ok", "hit"),
        "write" => ("run", "write"),
        "miss" => ("bad", "miss"),
        _ => ("muted", "unknown"),
    };
    view! { <span class=format!("pill {class}")>{label}</span> }
}

fn outcome_pill(outcome: &str) -> impl IntoView {
    let (class, label) = match outcome {
        "ok" => ("ok", "ok"),
        "refused" => ("wait", "refused"),
        "timeout" => ("wait", "timeout"),
        _ => ("bad", "error"),
    };
    view! { <span class=format!("pill {class}")>{label}</span> }
}

fn day_label(bucket: &str) -> String {
    bucket.get(5..10).unwrap_or(bucket).to_owned()
}
