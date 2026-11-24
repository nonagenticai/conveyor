//! The execution room — the tenant's runs and the run inspector's provenance.
//!
//! `runs.list` is the queue; `runs.get` is the trace (steps, gates, process, inputs and the tree),
//! and the run detail screen renders the same read. The tool ledger and the executor/pin are not
//! exposed by either read yet, so this screen names those gaps instead of inventing columns
//! (owner: T13 for `tool_calls.list`; T0/T4/S21 for the executor kind and binding pin).

use leptos::prelude::*;
use leptos_router::components::A;
use serde_json::json;

use crate::api::{ApiError, InvokeOptions, Page};
use crate::components::{Empty, ErrorPanel, StatusPill, Tile, age, fmt_int, short_id};
use crate::models::{RunGet, RunOut};
use crate::state::AppCtx;

async fn load(ctx: AppCtx) -> Result<Vec<RunOut>, ApiError> {
    let value = ctx
        .invoke(
            "runs.list",
            json!({ "limit": 50 }),
            InvokeOptions::default(),
        )
        .await?
        .output;
    let page: Page<RunOut> = serde_json::from_value(value)
        .map_err(|error| ApiError::Network(format!("runs: {error}")))?;
    Ok(page.items)
}

async fn load_run(ctx: AppCtx, id: &str) -> Result<RunGet, ApiError> {
    let value = ctx
        .client()
        .invoke("runs.get", json!({ "id": id }), InvokeOptions::default())
        .await?
        .output;
    serde_json::from_value(value).map_err(|error| ApiError::Network(format!("run: {error}")))
}

fn pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

#[component]
pub fn ExecutionScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let data = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        async move { load(ctx).await }
    });
    let refresh = Callback::new(move |_| data.refetch());
    let selected = RwSignal::new(Option::<String>::None);
    let detail = RwSignal::new(Option::<Result<RunGet, ApiError>>::None);
    let detail_loading = RwSignal::new(false);
    let load_detail = move |id: String| {
        detail_loading.set(true);
        detail.set(None);
        leptos::task::spawn_local(async move {
            let result = load_run(ctx, &id).await;
            detail.set(Some(result));
            detail_loading.set(false);
        });
    };
    let retry_detail = Callback::new(move |_| {
        if let Some(id) = selected.get_untracked() {
            load_detail(id);
        }
    });

    view! {
        <section class="screen" aria-labelledby="execution-title">
            <header class="screen-head">
                <div>
                    <h1 id="execution-title">"Execution room"</h1>
                    <p class="muted">
                        "The tenant's runs, newest first; a run's steps, gates and inputs are its provenance."
                    </p>
                </div>
                <button class="btn" on:click=move |_| refresh.run(())>"Refresh"</button>
            </header>
            {move || match data.get() {
                None => view! { <p class="muted">"Reading the tenant's runs…"</p> }.into_any(),
                Some(Err(error)) => view! {
                    <ErrorPanel error=error on_retry=refresh/>
                }.into_any(),
                Some(Ok(runs)) => {
                    let live = runs
                        .iter()
                        .filter(|run| matches!(run.status.as_str(), "queued" | "running" | "waiting_approval"))
                        .count();
                    view! {
                        <div class="grid-3">
                            <Tile label="Runs".to_owned() value=fmt_int(runs.len() as i64) detail="newest 50".to_owned()/>
                            <Tile label="Live".to_owned() value=fmt_int(live as i64) detail="queued · running · waiting".to_owned()/>
                        </div>
                        {if runs.is_empty() {
                            view! { <Empty message="No run in this tenant yet; a board card starts one.".to_owned()/> }.into_any()
                        } else {
                            view! {
                                <table class="table" data-testid="execution-runs">
                                    <thead><tr>
                                        <th>"Run"</th><th>"Card"</th><th>"Status"</th><th>"Attempt"</th>
                                        <th>"Started"</th><th>"Finished"</th><th></th>
                                    </tr></thead>
                                    <tbody>
                                        {runs.iter().map(|run| {
                                            let id = run.id.clone();
                                            let select_id = id.clone();
                                            let card = run
                                                .card_id
                                                .as_deref()
                                                .map(|id| short_id(Some(id)))
                                                .unwrap_or_else(|| "—".to_owned());
                                            view! {
                                                <tr>
                                                    <td class="mono">
                                                        <button class="btn small" on:click=move |_| {
                                                            selected.set(Some(select_id.clone()));
                                                            load_detail(select_id.clone());
                                                        }>{short_id(Some(&id))}</button>
                                                    </td>
                                                    <td class="mono">{card}</td>
                                                    <td><StatusPill status=run.status.clone()/></td>
                                                    <td class="mono">{run.attempt.to_string()}</td>
                                                    <td>{age(run.started_at)}</td>
                                                    <td>{age(run.finished_at)}</td>
                                                    <td><A href=format!("/runs/{}", run.id)><span class="muted">"Open"</span></A></td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            }.into_any()
                        }}
                        {move || match detail.get() {
                            None if detail_loading.get() => view! {
                                <p class="muted">"Reading the run…"</p>
                            }.into_any(),
                            None => view! {
                                <p class="muted">"Select a run to read its steps, gates and inputs."</p>
                            }.into_any(),
                            Some(Err(error)) => view! {
                                <ErrorPanel error=error on_retry=retry_detail/>
                            }.into_any(),
                            Some(Ok(run)) => run_detail(run),
                        }}
                        <section class="panel planned-panel">
                            <h2>"Not in this read"</h2>
                            <p class="muted">
                                "The executor kind and the job binding / release hash pin are not exposed by runs.list or runs.get yet (owner: T0 · T4 · S21). The tools panel (tool version, credential alias, returned evidence) needs tool_calls.list (owner: T13, #557). The step's own input, output and judge are the execution trace this read carries."
                            </p>
                        </section>
                    }.into_any()
                }
            }}
        </section>
    }
}

fn run_detail(run: RunGet) -> AnyView {
    let row = run.run.clone();
    let steps = run.steps;
    let approvals = run.approvals;
    let process = run.process;
    let inputs = run.inputs;
    let tree = run.tree;
    view! {
        <section class="panel" data-testid="execution-detail">
            <div class="ph">
                <span>{format!("Run {}", short_id(Some(&row.id)))}</span>
                <span class="muted">
                    {process
                        .as_ref()
                        .map(|process| format!("{}@{}", process.title, process.version))
                        .unwrap_or_else(|| "no process — a direct run".to_owned())}
                </span>
            </div>
            <div class="kv">
                <dt>"Status"</dt><dd><StatusPill status=row.status.clone()/></dd>
                <dt>"Attempt"</dt><dd class="mono">{row.attempt.to_string()}</dd>
                <dt>"Started"</dt><dd>{age(row.started_at)}</dd>
                <dt>"Finished"</dt><dd>{age(row.finished_at)}</dd>
                <dt>"Card"</dt>
                <dd>
                    {match row.card_id.clone() {
                        Some(id) => view! {
                            <A href=format!("/card/{id}")><span class="mono">{short_id(Some(&id))}</span></A>
                        }.into_any(),
                        None => view! { <span class="muted">"none"</span> }.into_any(),
                    }}
                </dd>
                <dt class="adv">"Run id"</dt><dd class="adv mono">{row.id.clone()}</dd>
            </div>
            {row.error_class.clone().map(|class| view! {
                <div class="callout bad">
                    <div class="hd">{class}</div>
                    <p>{row.error_message.clone().unwrap_or_else(|| "no message recorded".to_owned())}</p>
                </div>
            })}
            <h3>"Steps"</h3>
            {if steps.is_empty() {
                view! { <Empty message="No step has run yet.".to_owned()/> }.into_any()
            } else {
                view! {
                    <table class="table" data-testid="execution-steps">
                        <thead><tr><th>"#"</th><th>"Action"</th><th>"Attempt"</th><th>"Status"</th><th>"Started"</th><th>"Finished"</th></tr></thead>
                        <tbody>
                            {steps.iter().map(|step| view! {
                                <tr>
                                    <td class="mono">{step.position.to_string()}</td>
                                    <td class="mono">{step.action.clone()}</td>
                                    <td class="mono">{step.attempt.to_string()}</td>
                                    <td><StatusPill status=step.status.clone()/></td>
                                    <td>{age(step.started_at)}</td>
                                    <td>{age(step.finished_at)}</td>
                                </tr>
                            }).collect_view()}
                        </tbody>
                    </table>
                }.into_any()
            }}
            {steps.iter().filter(|step| step.error.is_some()).map(|step| view! {
                <div class="callout bad">
                    <div class="hd">{format!("step {} · attempt {}", step.position, step.attempt)}</div>
                    <p class="mono">{step.error.clone().unwrap_or_default()}</p>
                </div>
            }).collect_view()}
            <h3>"Gates"</h3>
            {if approvals.is_empty() {
                view! { <Empty message="No gate on this run.".to_owned()/> }.into_any()
            } else {
                view! {
                    <table class="table" data-testid="execution-gates">
                        <thead><tr><th>"Gate"</th><th>"Status"</th><th>"Quorum"</th><th>"When"</th></tr></thead>
                        <tbody>
                            {approvals.iter().map(|gate| view! {
                                <tr>
                                    <td class="mono">{gate.gate.clone()}</td>
                                    <td><StatusPill status=gate.status.clone()/></td>
                                    <td class="mono">{format!("{} / {}", gate.approvals, gate.required_decisions)}</td>
                                    <td>{gate.created_at.map(|at| age(Some(at))).unwrap_or_else(|| "—".to_owned())}</td>
                                </tr>
                            }).collect_view()}
                        </tbody>
                    </table>
                }.into_any()
            }}
            <h3>"Explain context"</h3>
            {if inputs.is_null() || inputs.as_object().map(|object| object.is_empty()).unwrap_or(false) {
                view! { <p class="muted">"No inputs were supplied."</p> }.into_any()
            } else {
                view! { <pre class="mono">{pretty(&inputs)}</pre> }.into_any()
            }}
            <h3>"Run tree"</h3>
            {let depth = tree.depth;
            let parent = short_id(tree.parent_run_id.as_deref());
            let children = tree.children;
            view! {
                <div class="kv">
                    <dt>"Depth"</dt><dd class="mono">{depth.to_string()}</dd>
                    <dt>"Parent"</dt><dd class="mono">{parent}</dd>
                </div>
                {if children.is_empty() {
                    view! { <p class="muted">"No child runs (a call step starts one)."</p> }.into_any()
                } else {
                    view! {
                        <table class="table">
                            <thead><tr><th>"Child"</th><th>"Step"</th><th>"Status"</th><th>"Depth"</th></tr></thead>
                            <tbody>
                                {children.into_iter().map(|child| {
                                    let id = child.id;
                                    let href = format!("/runs/{id}");
                                    let id_short = short_id(Some(&id));
                                    let step_short = short_id(child.step_id.as_deref());
                                    let status = child.status;
                                    let depth = child.depth;
                                    view! {
                                        <tr>
                                            <td class="mono"><A href=href>{id_short}</A></td>
                                            <td class="mono">{step_short}</td>
                                            <td><StatusPill status=status/></td>
                                            <td class="mono">{depth.to_string()}</td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                    }.into_any()
                }}
            }}
        </section>
    }
    .into_any()
}
