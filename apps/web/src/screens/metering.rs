//! Metering — every level recomputes from the ledger (SPEC_V2 §10.5.2; T18-4, #565).
//!
//! One read, `metrics.usage`, serves all eight levels. The five roll-ups are the first five
//! tabs; the drill-down is the run/step/call tail. The action's `id` scopes WITHIN a level
//! (its own column), so only the run→step and run→call drill can be offered honestly: both
//! filter `run_id`. Every tile and every row is a sum of the rows this page just read — there
//! is no stored aggregate and no client-side estimate.

use chrono::{Duration, Utc};
use leptos::prelude::*;
use serde_json::{Value, json};

use crate::api::{ApiError, InvokeOptions};
use crate::components::{Empty, ErrorPanel, Planned, Tile, fmt_int, short_id};
use crate::state::AppCtx;

/// The action's eight levels, in the order the roll-up→drill-down reads.
const LEVELS: [(&str, &str); 8] = [
    ("tenant", "Tenant"),
    ("department", "Department"),
    ("programme", "Programme"),
    ("project", "Project"),
    ("job", "Job"),
    ("run", "Run"),
    ("step", "Step"),
    ("call", "Call"),
];

const WINDOW_DAYS: i64 = 7;

#[derive(Clone, Debug, Default)]
struct Data {
    level: String,
    rows: Vec<Value>,
    since: String,
    until: String,
}

/// `metrics.usage` returns `{level, rows}` — not the `{items}` page the shared reader expects.
fn level_rows(output: &Value) -> Vec<Value> {
    output
        .get("rows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

async fn load(ctx: AppCtx, level: String, scope: Option<String>) -> Result<Data, ApiError> {
    let until = Utc::now();
    let since = until - Duration::days(WINDOW_DAYS);
    let mut input = json!({
        "level": level,
        "since": since.to_rfc3339(),
        "until": until.to_rfc3339(),
    });
    // The action takes the parent's id only where its filter is the parent's column. The run
    // id is the one honest parent: `level_filter` scopes step/call by `run_id`. A non-UUID
    // key (`unattributed`) is not a row the ledger can scope to.
    if let Some(id) = scope.as_deref().filter(|id| id.len() == 36) {
        input["id"] = Value::String(id.to_owned());
    }
    let invocation = ctx
        .invoke("metrics.usage", input, InvokeOptions::default())
        .await?;
    Ok(Data {
        level,
        rows: level_rows(&invocation.output),
        since: since.format("%Y-%m-%d %H:%M UTC").to_string(),
        until: until.format("%Y-%m-%d %H:%M UTC").to_string(),
    })
}

fn number(row: &Value, key: &str) -> f64 {
    row.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn integer(row: &Value, key: &str) -> i64 {
    row.get(key).and_then(Value::as_i64).unwrap_or(0)
}

fn money(value: f64) -> String {
    format!("${value:.4}")
}

/// A row's key: `tenant`/`unattributed` are named, every id is shortened.
fn key_label(key: &str) -> String {
    match key {
        "tenant" | "unattributed" => key.to_owned(),
        other => short_id(Some(other)),
    }
}

fn is_id(key: &str) -> bool {
    key.len() == 36 && key.contains('-')
}

fn sums(data: &Data) -> (i64, i64, i64, f64, f64) {
    let calls: i64 = data.rows.iter().map(|row| integer(row, "calls")).sum();
    let tool_calls: i64 = data.rows.iter().map(|row| integer(row, "tool_calls")).sum();
    let tokens: i64 = data.rows.iter().map(|row| integer(row, "tokens")).sum();
    let cost: f64 = data.rows.iter().map(|row| number(row, "cost_usd")).sum();
    let tool_cost: f64 = data
        .rows
        .iter()
        .map(|row| number(row, "tool_cost_usd"))
        .sum();
    (calls, tool_calls, tokens, cost, tool_cost)
}

#[component]
pub fn Metering() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let level = RwSignal::new("tenant".to_owned());
    let scope = RwSignal::new(Option::<String>::None);
    let data = LocalResource::new(move || {
        // Read the tenant so a switch refetches under the new role.
        let _tenant = ctx.tenant.get();
        let level = level.get();
        let scope = scope.get();
        async move { load(ctx, level, scope).await }
    });
    let refresh = Callback::new(move |_| data.refetch());

    view! {
        <section class="screen" aria-labelledby="metering-title">
            {move || {
                if !ctx.has_action("metrics.usage") {
                    return view! {
                        <Planned
                            title="Metering is planned".to_owned()
                            detail="metrics.usage is not in this role's manifest; the screen appears when it lands.".to_owned()
                        />
                    }
                    .into_any();
                }
                view! {
                    <header class="screen-head">
                        <div>
                            <h1 id="metering-title">"Metering"</h1>
                            <p class="muted">
                                "the five roll-ups and the run/step/call drill-down, recomputed from the inference and tool ledgers over the last seven days. Every number below is the sum of the rows this page read — there is no stored roll-up to drift."
                            </p>
                        </div>
                        <div class="filterbar">
                            <button class="btn" on:click=move |_| refresh.run(())>"Refresh"</button>
                        </div>
                    </header>
                    <div class="filterbar" role="tablist" data-testid="metering-levels">
                        {LEVELS.into_iter().map(|(entry, label)| view! {
                            <button
                                class="btn small"
                                class:primary=move || level.get() == entry && scope.get().is_none()
                                role="tab"
                                aria-selected=move || (level.get() == entry && scope.get().is_none()).to_string()
                                on:click=move |_| {
                                    scope.set(None);
                                    level.set(entry.to_owned());
                                }
                            >
                                {label}
                            </button>
                        }).collect_view()}
                    </div>
                    {move || scope.get().map(|id| view! {
                        <div class="callout" data-testid="metering-scope">
                            <strong>"Scoped to run "</strong>
                            <span class="mono">{short_id(Some(&id))}</span>
                            <span class="muted">{format!(" · {} rows", level.get())}</span>
                            <button class="btn small" on:click=move |_| {
                                scope.set(None);
                                level.set("run".to_owned());
                            }>"Back to all runs"</button>
                        </div>
                    })}
                    {move || match data.get() {
                        None => view! { <p class="muted">"Reading the ledgers…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=refresh/>
                        }.into_any(),
                        Some(Ok(loaded)) => {
                            let (calls, tool_calls, tokens, cost, tool_cost) = sums(&loaded);
                            let rows = loaded.rows.clone();
                            let shown_level = loaded.level.clone();
                            let window = format!("{} → {}", loaded.since, loaded.until);
                            view! {
                                <div class="tiles">
                                    <Tile label="Calls".to_owned() value=fmt_int(calls) detail="inference calls in the window".to_owned()/>
                                    <Tile label="Tool calls".to_owned() value=fmt_int(tool_calls) detail="from the tool ledger".to_owned()/>
                                    <Tile label="Tokens".to_owned() value=fmt_int(tokens) detail="prompt + completion".to_owned()/>
                                    <Tile label="Cost".to_owned() value=money(cost + tool_cost) detail=format!("inference {} · tools {}", money(cost), money(tool_cost))/>
                                </div>
                                <p class="muted" data-testid="metering-window">{window}</p>
                                {if rows.is_empty() {
                                    view! {
                                        <Empty message="No usage at this level in the window; metrics.usage recomputes from the inference and tool ledgers and an empty level writes nothing.".to_owned()/>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="tablewrap">
                                            <table class="table" data-testid="metering-table">
                                                <thead><tr>
                                                    <th>"Level"</th><th>"Key"</th><th class="n">"Calls"</th>
                                                    <th class="n">"Tool calls"</th><th class="n">"Tokens"</th>
                                                    <th class="n">"Cost"</th><th class="n">"Runs"</th>
                                                    <th class="n">"Priced"</th><th class="n">"Unpriced"</th>
                                                    <th class="n">"Participant h"</th><th></th>
                                                </tr></thead>
                                                <tbody>
                                                    {rows.into_iter().map(|row| {
                                                        let key = row.get("key").and_then(Value::as_str).unwrap_or("—").to_owned();
                                                        let drill = is_id(&key);
                                                        let row_key = key.clone();
                                                        let to_steps = key.clone();
                                                        let to_calls = key.clone();
                                                        view! {
                                                            // The full key is the row's identity: eight characters are not
                                                            // unique (UUIDv7's timestamp prefix repeats within a millisecond).
                                                            <tr data-run=row_key>
                                                                <td>{shown_level.clone()}</td>
                                                                <td class="mono">{key_label(&key)}</td>
                                                                <td class="n">{fmt_int(integer(&row, "calls"))}</td>
                                                                <td class="n">{fmt_int(integer(&row, "tool_calls"))}</td>
                                                                <td class="n">{fmt_int(integer(&row, "tokens"))}</td>
                                                                <td class="n">{money(number(&row, "cost_usd") + number(&row, "tool_cost_usd"))}</td>
                                                                <td class="n">{fmt_int(integer(&row, "runs"))}</td>
                                                                <td class="n">{fmt_int(integer(&row, "priced_calls"))}</td>
                                                                <td class="n">{fmt_int(integer(&row, "unpriced_calls"))}</td>
                                                                <td class="n">{format!("{:.2}", number(&row, "participant_hours"))}</td>
                                                                <td>
                                                                    {(drill).then(|| view! {
                                                                        <span class="filterbar">
                                                                            <button class="btn small" on:click=move |_| {
                                                                                scope.set(Some(to_steps.clone()));
                                                                                level.set("step".to_owned());
                                                                            }>"Drill to steps"</button>
                                                                            <button class="btn small" on:click=move |_| {
                                                                                scope.set(Some(to_calls.clone()));
                                                                                level.set("call".to_owned());
                                                                            }>"Drill to calls"</button>
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
                                }}
                                <p class="muted">
                                    "The roll-ups name their own key: a department by name (the ledger's attribution), a programme/project/job/run/step/call by id, and `unattributed` where the row carries no attribution. The drill from a run scopes steps and calls by `run_id` — the one parent the action's filter names."
                                </p>
                            }.into_any()
                        }
                    }}
                }
                .into_any()
            }}
        </section>
    }
}
