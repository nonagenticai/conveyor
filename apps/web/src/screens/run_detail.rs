//! Run detail — a run is a state machine, so it is drawn as one.
//!
//! Everything on this screen is one `runs.get`: the run, its `run_step` trace, its gates, the
//! process it executes and the inputs it was created with. The card's name comes from the
//! existing `GET /api/cards/{id}` route, because the card's text fields live in the board
//! projection, not on the run row.
//!
//! Failure is named, never summarised: a failed step shows the action's own error text, and a
//! run's `error_class` / `error_message` are shown verbatim. No artifacts panel is drawn —
//! control runs have none, and a fake row is worse than an absent one.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use serde_json::{Value, json};

use crate::api::{ApiError, InvokeOptions};
use crate::components::{Empty, ErrorPanel, StatusPill, age, short_id};
use crate::models::{CardView, RunGet};
use crate::state::AppCtx;

#[derive(Clone, Debug)]
struct Detail {
    run: RunGet,
    card_name: Option<String>,
}

fn load<F>(ctx: AppCtx, run_id: F) -> LocalResource<Result<Detail, ApiError>>
where
    F: Fn() -> String + 'static,
{
    LocalResource::new(move || {
        // Every reactive input is read before the first await.
        let id = run_id();
        let _tenant = ctx.tenant.get();
        let client = ctx.client();
        async move {
            if id.is_empty() {
                return Err(ApiError::NotFound("no run in the route".to_owned()));
            }
            let run: RunGet = serde_json::from_value(
                client
                    .invoke("runs.get", json!({"id": id}), InvokeOptions::default())
                    .await?
                    .output,
            )
            .map_err(|error| ApiError::Network(format!("run: {error}")))?;
            let card_name = match run.run.card_id.clone() {
                Some(card_id) => match client.get(&format!("/api/cards/{card_id}")).await {
                    Ok(value) => serde_json::from_value::<CardView>(value)
                        .ok()
                        .map(|view| view.card.name),
                    // A card that no longer resolves must not fail the whole run view.
                    Err(_) => None,
                },
                None => None,
            };
            Ok(Detail { run, card_name })
        }
    })
}

#[component]
pub fn RunDetail() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let params = use_params_map();
    let run_id = move || params.get().get("run_id").unwrap_or_default();
    let data = load(ctx, run_id);
    let refresh = Callback::new(move |_| data.refetch());
    let notice = RwSignal::new(Option::<String>::None);

    view! {
        <section class="screen">
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
                            <RunBody data=body refresh=refresh notice=notice/>
                        }.into_any(),
                    }}
                }
                .into_any()
            }}
        </section>
    }
}

#[component]
fn RunBody(data: Detail, refresh: Callback<()>, notice: RwSignal<Option<String>>) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let run = data.run.run.clone();
    let steps = data.run.steps.clone();
    let approvals = data.run.approvals.clone();
    let process = data.run.process.clone();
    let inputs = data.run.inputs.clone();
    let run_tree = data.run.tree.clone();
    let card_name = data.card_name.clone();
    let card_id = run.card_id.clone();

    // One block per position, coloured by its latest attempt: a retry appends rows, and the
    // strip must show where the run is, not every attempt it survived.
    let mut positions: Vec<(i32, String, String)> = Vec::new();
    for step in &steps {
        match positions
            .iter_mut()
            .find(|(position, _, _)| *position == step.position)
        {
            Some(entry) => {
                entry.1 = step.action.clone();
                entry.2 = step.status.clone();
            }
            None => positions.push((step.position, step.action.clone(), step.status.clone())),
        }
    }
    let total = positions.len();
    let current = positions
        .iter()
        .position(|(_, _, status)| status == "running")
        .map(|index| index + 1)
        .unwrap_or_else(|| {
            positions
                .iter()
                .filter(|(_, _, status)| status == "succeeded" || status == "skipped")
                .count()
                .min(total)
        });
    let live =
        run.status == "queued" || run.status == "running" || run.status == "waiting_approval";
    let can_cancel = move || ctx.has_action("runs.cancel") && live;
    let cancel_id = run.id.clone();
    let on_cancel = move |_| {
        let id = cancel_id.clone();
        let ctx = ctx;
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "runs.cancel",
                    json!({"id": id, "reason": "cancelled from the run screen"}),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => refresh.run(()),
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    };

    let elapsed = match (run.started_at, run.finished_at) {
        (Some(start), Some(end)) => format!("{} total", minutes_between(start, end)),
        (Some(start), None) => format!("{} elapsed", minutes_since(start)),
        _ => "not started".to_owned(),
    };
    let header_title = format!(
        "Run {} · {}",
        short_id(Some(&run.id)),
        card_name
            .clone()
            .unwrap_or_else(|| "project run".to_owned())
    );

    view! {
        <header class="screen-head">
            <div>
                <h1>{header_title}</h1>
                <p class="muted">
                    {format!(
                        "{} · attempt {} · {} · {}",
                        run.project_id.clone().unwrap_or_else(|| "no project".to_owned()),
                        run.attempt,
                        process
                            .as_ref()
                            .map(|row| format!("{}@{}", row.title, row.version))
                            .unwrap_or_else(|| "no process".to_owned()),
                        elapsed
                    )}
                </p>
            </div>
            <div class="filterbar">
                <StatusPill status=run.status.clone()/>
                {(!positions.is_empty()).then(|| view! {
                    <span class="pill run">{format!("step {current} of {total}")}</span>
                })}
                {card_id.clone().map(|id| view! {
                    <A href=format!("/card/{id}")><span class="btn small">"Card"</span></A>
                })}
                {can_cancel().then(|| view! {
                    <button class="btn small danger" on:click=on_cancel>"Cancel"</button>
                })}
                <button class="btn small" on:click=move |_| refresh.run(())>"Refresh"</button>
            </div>
        </header>

        {run.error_class.clone().map(|class| view! {
            <div class="callout bad">
                <div class="hd">{class}</div>
                <p>{run.error_message.clone().unwrap_or_else(|| "no message recorded".to_owned())}</p>
            </div>
        })}

        <section class="panel" style="margin-bottom:12px">
            <div class="ph">
                <span>"Run tree"</span>
                <span class="muted">{format!("depth {}", run_tree.depth)}</span>
            </div>
            {run_tree.parent_run_id.clone().map(|parent| view! {
                <p class="muted">
                    "Child of "
                    <A href=format!("/runs/{parent}")>{format!("run {}", short_id(Some(&parent)))}</A>
                    {run_tree.step_id.clone().map(|step| format!(" · step {}", short_id(Some(&step)))).unwrap_or_default()}
                </p>
            })}
            {if run_tree.children.is_empty() {
                view! { <Empty message="No child runs (a call step starts one).".to_owned()/> }.into_any()
            } else {
                view! {
                    <div class="rows">
                        {run_tree.children.into_iter().map(|child| {
                            view! {
                                <article class="row-card">
                                    <div class="row-main">
                                        <div class="row-title mono">
                                            <A href=format!("/runs/{}", child.id)>{short_id(Some(&child.id))}</A>
                                        </div>
                                        <div class="row-sub">
                                            <span class="muted">
                                                {child.step_id.clone().map(|step| format!("step {}", short_id(Some(&step)))).unwrap_or_else(|| "no step".to_owned())}
                                            </span>
                                            <span class="muted">{format!("depth {}", child.depth)}</span>
                                        </div>
                                    </div>
                                    <div class="row-side"><StatusPill status=child.status/></div>
                                </article>
                            }
                        }).collect_view()}
                    </div>
                }.into_any()
            }}
        </section>

        <section class="panel">
            <div class="ph">
                <span>"Steps"</span>
                <span class="muted">
                    {process
                        .as_ref()
                        .map(|row| format!("process {}@{}", row.title, row.version))
                        .unwrap_or_else(|| "no process — a direct run".to_owned())}
                </span>
            </div>
            {if positions.is_empty() {
                view! { <Empty message="This run has no process steps; it is a direct run.".to_owned()/> }.into_any()
            } else {
                view! {
                    <div class="steps-strip">
                        {positions.clone().into_iter().map(|(position, action, status)| {
                            let tone = match status.as_str() {
                                "succeeded" => "ok",
                                "running" => "run",
                                "failed" => "bad",
                                _ => "wait",
                            };
                            let title = format!("{action} · {status}");
                            let label = action.clone();
                            view! {
                                <div class=format!("step-block {tone}") title=title>
                                    <span class="n">{position}</span>
                                    <span class="t">{label}</span>
                                </div>
                            }
                        }).collect_view()}
                    </div>
                }.into_any()
            }}
        </section>

        <div class="split" style="margin-top:12px">
            <section class="panel">
                <div class="ph">
                    <span>"Jobs"</span>
                    <span class="muted">{format!("{} rows · one per attempt", steps.len())}</span>
                </div>
                {if steps.is_empty() {
                    view! { <Empty message="No step has run yet.".to_owned()/> }.into_any()
                } else {
                    view! {
                        <table class="wtbl">
                            <thead><tr>
                                <th class="n">"Step"</th><th>"Action"</th><th class="n">"Attempt"</th>
                                <th>"State"</th><th class="n">"Duration"</th>
                            </tr></thead>
                            <tbody>
                                {steps.clone().into_iter().map(|step| {
                                    let duration = match (step.started_at, step.finished_at) {
                                        (Some(start), Some(end)) => minutes_between(start, end),
                                        (Some(start), None) => format!("{} running", minutes_since(start)),
                                        _ => "—".to_owned(),
                                    };
                                    view! {
                                        <tr>
                                            <td class="n">{step.position}</td>
                                            <td class="mono">{step.action.clone()}</td>
                                            <td class="n">{step.attempt}</td>
                                            <td><StatusPill status=step.status.clone()/></td>
                                            <td class="n">{duration}</td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                        {steps.clone().into_iter().filter(|step| step.error.is_some()).map(|step| view! {
                            <div class="callout bad" style="margin-top:8px">
                                <div class="hd">{format!("step {} · attempt {}", step.position, step.attempt)}</div>
                                <p class="mono">{step.error.clone().unwrap_or_default()}</p>
                            </div>
                        }).collect_view()}
                    }.into_any()
                }}
                <p class="hint adv blk" style="margin-top:8px">
                    "runs.get · run_step is the trace, one row per attempt (a retry appends)"
                </p>
            </section>

            <div>
                <section class="panel">
                    <div class="ph"><span>"Approvals"</span><span class="muted">{approvals.len()}</span></div>
                    {if approvals.is_empty() {
                        view! { <Empty message="No gate on this run.".to_owned()/> }.into_any()
                    } else {
                        view! {
                            <table class="wtbl">
                                <thead><tr><th>"Gate"</th><th>"Status"</th><th class="n">"Quorum"</th><th>"When"</th></tr></thead>
                                <tbody>
                                    {approvals.clone().into_iter().map(|gate| view! {
                                        <tr>
                                            <td>
                                                <span class="mono">{gate.gate.clone()}</span>
                                                {card_id.clone().map(|id| view! {
                                                    <A href=format!("/card/{id}")><span class="muted">" · decide in the cockpit"</span></A>
                                                })}
                                            </td>
                                            <td><StatusPill status=gate.status.clone()/></td>
                                            <td class="n">{format!("{}/{}", gate.approvals, gate.required_decisions)}</td>
                                            <td>{gate.created_at.map(|at| age(Some(at))).unwrap_or_else(|| "—".to_owned())}</td>
                                        </tr>
                                    }).collect_view()}
                                </tbody>
                            </table>
                        }.into_any()
                    }}
                </section>

                <section class="panel" style="margin-top:12px">
                    <div class="ph"><span>"Inputs"</span></div>
                    {inputs_list(inputs.clone())}
                    <div class="kv" style="margin-top:10px">
                        <dt>"Project"</dt><dd class="mono">{run.project_id.clone().unwrap_or_default()}</dd>
                        <dt>"Card"</dt>
                        <dd>
                            {match card_id.clone() {
                                Some(id) => view! {
                                    <A href=format!("/card/{id}")><span class="mono">{short_id(Some(&id))}</span></A>
                                }.into_any(),
                                None => view! { <span class="muted">"none"</span> }.into_any(),
                            }}
                        </dd>
                        <dt class="adv">"Run id"</dt><dd class="adv mono">{run.id.clone()}</dd>
                        <dt class="adv">"Version"</dt><dd class="adv mono">{run.version}</dd>
                    </div>
                </section>
            </div>
        </div>
    }
}

fn inputs_list(inputs: Value) -> AnyView {
    let entries: Vec<(String, String)> = inputs
        .as_object()
        .map(|object| {
            object
                .iter()
                .map(|(key, value)| (key.clone(), compact(value)))
                .collect()
        })
        .unwrap_or_default();
    let empty = entries.is_empty();
    view! {
        {if empty {
            view! { <p class="muted" style="font-size:11px">"No inputs were supplied."</p> }.into_any()
        } else {
            view! {
                <div class="kv">
                    {entries.into_iter().map(|(key, value)| view! {
                        <dt class="mono">{key}</dt><dd class="mono">{value}</dd>
                    }).collect_view()}
                </div>
            }.into_any()
        }}
    }
    .into_any()
}

fn compact(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn minutes_between(
    start: chrono::DateTime<chrono::Utc>,
    end: chrono::DateTime<chrono::Utc>,
) -> String {
    let seconds = (end - start).num_seconds().max(0);
    if seconds < 60 {
        format!("{seconds} s")
    } else if seconds < 3_600 {
        format!("{} m {:02} s", seconds / 60, seconds % 60)
    } else {
        format!("{} h {:02} m", seconds / 3_600, (seconds % 3_600) / 60)
    }
}

fn minutes_since(start: chrono::DateTime<chrono::Utc>) -> String {
    minutes_between(start, chrono::Utc::now())
}
