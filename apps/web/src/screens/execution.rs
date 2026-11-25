//! The execution room — the tenant's runs and the run inspector's provenance.
//!
//! `runs.list` is the queue; `runs.get` is the trace (steps, gates, process, inputs and the tree),
//! and `tool_calls.list` is the run's tools ledger. The run carries the executor kind and the job
//! binding / release pin it was created with (T18-3c), so the history and the detail render the
//! pin instead of naming it as a gap.

use leptos::prelude::*;
use leptos_router::components::A;
use serde_json::json;

use crate::api::{ApiError, InvokeOptions, Page};
use crate::components::{Empty, ErrorPanel, StatusPill, Tile, age, fmt_int, short_id};
use crate::models::{RunGet, RunOut, ToolCallOut};
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

/// The run's tools ledger — the panel's version, credential alias and evidence.
async fn load_tool_calls(ctx: AppCtx, run_id: &str) -> Result<Vec<ToolCallOut>, ApiError> {
    let value = ctx
        .client()
        .invoke(
            "tool_calls.list",
            json!({ "run_id": run_id, "limit": 50 }),
            InvokeOptions::default(),
        )
        .await?
        .output;
    let page: Page<ToolCallOut> = serde_json::from_value(value)
        .map_err(|error| ApiError::Network(format!("tool calls: {error}")))?;
    Ok(page.items)
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
    let tools = RwSignal::new(Option::<Result<Vec<ToolCallOut>, ApiError>>::None);
    let detail_loading = RwSignal::new(false);
    let load_detail = move |id: String| {
        detail_loading.set(true);
        detail.set(None);
        tools.set(None);
        leptos::task::spawn_local(async move {
            // The ledger is set before the run so the detail's first render already has it.
            let tool_result = load_tool_calls(ctx, &id).await;
            tools.set(Some(tool_result));
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
                                        <th>"Executor"</th><th>"Binding"</th><th>"Release pin"</th>
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
                                                    <td class="mono">
                                                        {run.executor_kind.clone().unwrap_or_else(|| "—".to_owned())}
                                                    </td>
                                                    <td class="mono">
                                                        {run.job_binding_id
                                                            .as_deref()
                                                            .map(|id| short_id(Some(id)))
                                                            .unwrap_or_else(|| "—".to_owned())}
                                                    </td>
                                                    <td class="mono">
                                                        {run.release_hash
                                                            .as_deref()
                                                            .map(|hash| short_id(Some(hash)))
                                                            .unwrap_or_else(|| "—".to_owned())}
                                                    </td>
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
                            Some(Ok(run)) => run_detail(run, tools.get()),
                        }}
                        <section class="panel" data-testid="execution-tools">
                            <h2>"Tools"</h2>
                            {match tools {
                                None => view! {
                                    <p class="muted">"Reading the run's tool calls…"</p>
                                }.into_any(),
                                Some(Err(error)) => view! {
                                    <div class="callout bad">
                                        <div class="hd">"Tool ledger unavailable"</div>
                                        <p>{format!("{error}")}</p>
                                    </div>
                                }.into_any(),
                                Some(Ok(calls)) if calls.is_empty() => view! {
                                    <p class="muted">"No tool call recorded for this run."</p>
                                }.into_any(),
                                Some(Ok(calls)) => view! {
                                    <table class="table" data-testid="execution-tool-calls">
                                        <thead><tr>
                                            <th>"Tool"</th><th>"Version"</th><th>"Status"</th>
                                            <th>"Credential alias"</th><th>"Evidence"</th>
                                        </tr></thead>
                                        <tbody>
                                            {calls.into_iter().map(|call| view! {
                                                <tr>
                                                    <td class="mono">{call.tool.clone()}</td>
                                                    <td class="mono">
                                                        {format!(
                                                            "{}@{} · {}",
                                                            call.toolkit,
                                                            call.toolkit_version,
                                                            short_id(Some(&call.tool_id)),
                                                        )}
                                                    </td>
                                                    <td><StatusPill status=call.status.clone()/></td>
                                                    <td class="mono">
                                                        {call.credential_alias.clone().unwrap_or_else(|| "—".to_owned())}
                                                    </td>
                                                    <td class="mono">
                                                        {if call.evidence.is_null() {
                                                            call.error_message
                                                                .clone()
                                                                .unwrap_or_else(|| "—".to_owned())
                                                        } else {
                                                            pretty(&call.evidence)
                                                        }}
                                                    </td>
                                                </tr>
                                            }).collect_view()}
                                        </tbody>
                                    </table>
                                }.into_any(),
                            }}
                        </section>
                    }.into_any()
                }
            }}
        </section>
    }
}

fn run_detail(run: RunGet, tools: Option<Result<Vec<ToolCallOut>, ApiError>>) -> AnyView {
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
                <dt>"Executor"</dt>
                <dd class="mono">{row.executor_kind.clone().unwrap_or_else(|| "—".to_owned())}</dd>
                <dt>"Job binding"</dt>
                <dd class="mono">{row.job_binding_id.clone().unwrap_or_else(|| "—".to_owned())}</dd>
                <dt>"Release pin"</dt>
                <dd class="mono">{row.release_hash.clone().unwrap_or_else(|| "—".to_owned())}</dd>
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
