//! Today — the landing route. It answers one question: what needs a person?
//!
//! It composes existing reads only: `approvals.list` (pending, oldest first) and `runs.list`
//! (failed/running). There is no `andon.get`, no `cards.list` and no expiry sweep: those are
//! contract gaps recorded in the plan, not things this screen invents.

use leptos::prelude::*;
use leptos_router::components::A;
use serde_json::json;

use crate::api::{ApiError, InvokeOptions, Page};
use crate::components::{Empty, ErrorPanel, StatusPill, age};
use crate::models::{ApprovalOut, RunOut};
use crate::sse;
use crate::state::AppCtx;

#[derive(Clone, Debug)]
struct TodayData {
    approvals: Vec<ApprovalOut>,
    runs: Vec<RunOut>,
}

fn load(ctx: AppCtx) -> LocalResource<Result<TodayData, ApiError>> {
    LocalResource::new(move || async move {
        // Read the tenant so a switch refetches this projection.
        let _tenant = ctx.tenant.get();
        let client = ctx.client();

        let approvals = client
            .invoke(
                "approvals.list",
                json!({ "status": "pending", "limit": 50 }),
                InvokeOptions::default(),
            )
            .await?
            .output;
        let approvals: Page<ApprovalOut> = serde_json::from_value(approvals)
            .map_err(|error| ApiError::Network(format!("approvals: {error}")))?;

        let failed = client
            .invoke(
                "runs.list",
                json!({ "status": "failed", "limit": 25 }),
                InvokeOptions::default(),
            )
            .await?
            .output;
        let failed: Page<RunOut> = serde_json::from_value(failed)
            .map_err(|error| ApiError::Network(format!("runs (failed): {error}")))?;

        let running = client
            .invoke(
                "runs.list",
                json!({ "status": "running", "limit": 25 }),
                InvokeOptions::default(),
            )
            .await?
            .output;
        let running: Page<RunOut> = serde_json::from_value(running)
            .map_err(|error| ApiError::Network(format!("runs (running): {error}")))?;

        let mut runs = failed.items;
        runs.extend(running.items);
        Ok(TodayData {
            approvals: approvals.items,
            runs,
        })
    })
}

#[component]
pub fn Today() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let data = load(ctx);
    let subscription = RwSignal::new(None::<sse::Subscription>);

    Effect::new(move |_| {
        let _tenant = ctx.tenant.get();
        subscription.update(|slot| {
            if let Some(old) = slot.take() {
                old.abort();
            }
        });
        let target = data;
        match sse::subscribe(
            ctx.client(),
            "run,approval".to_owned(),
            None,
            move |frame| match frame {
                Ok(frame)
                    if frame.event == "resync"
                        || frame.event.starts_with("run.")
                        || frame.event.starts_with("approval.") =>
                {
                    target.refetch();
                }
                _ => {}
            },
        ) {
            Ok(sub) => subscription.set(Some(sub)),
            Err(error) => ctx.report(&error),
        }
    });

    on_cleanup(move || {
        subscription.update(|slot| {
            if let Some(old) = slot.take() {
                old.abort();
            }
        });
    });

    let retry = Callback::new(move |_| data.refetch());
    let greeting = move || {
        let name = ctx
            .me
            .get()
            .map(|me| me.principal.display_name)
            .unwrap_or_else(|| "there".to_owned());
        greeting_for(&name)
    };

    view! {
        <section class="screen" aria-labelledby="today-title">
            <header class="screen-head">
                <div>
                    <h1 id="today-title">"Today"</h1>
                    <p class="muted">{greeting}</p>
                </div>
                <button class="btn" on:click=move |_| data.refetch()>"Refresh"</button>
            </header>
            {move || match data.get() {
                None => view! { <p class="muted">"Loading…"</p> }.into_any(),
                Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                Some(Ok(data)) => view! { <TodayBody data=data/> }.into_any(),
            }}
        </section>
    }
}

#[component]
fn TodayBody(data: TodayData) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let tenant_line = move || match ctx.active_tenant() {
        Some(tenant) => format!(
            "tenant {} · role {}",
            tenant.tenant_id,
            crate::app::role_label(&tenant.role)
        ),
        None => "tenant —".to_owned(),
    };
    let approvals = data.approvals;
    let runs = data.runs;
    let summary = format!(
        "{} {} need you · {} {} in flight",
        approvals.len(),
        if approvals.len() == 1 {
            "thing"
        } else {
            "things"
        },
        runs.len(),
        if runs.len() == 1 { "run" } else { "runs" },
    );

    view! {
        <p class="muted summary">{summary}</p>
        <p class="adv mono">{tenant_line}</p>
        <div class="grid-2">
            <section class="panel" aria-labelledby="approvals-title">
                <div class="panel-head">
                    <h2 id="approvals-title">"Waiting on you"</h2>
                    <span class="count">{approvals.len()}</span>
                </div>
                {if approvals.is_empty() {
                    view! { <Empty message="No pending decisions.".to_owned()/> }.into_any()
                } else {
                    view! {
                        <div class="rows">
                            {approvals.into_iter().map(|approval| view! {
                                <article class="row-card">
                                    <div class="row-main">
                                        <div class="row-title">{approval.gate}</div>
                                        <div class="row-sub">
                                            {approval.card_id.clone().map(|id| view! {
                                                <A href=format!("/card/{id}")>{short(&id)}</A>
                                            })}
                                            {approval.run_id.clone().map(|id| view! {
                                                <span class="mono">{short(&id)}</span>
                                            })}
                                            <span class="muted">{approval.kind}</span>
                                        </div>
                                    </div>
                                    <div class="row-side">
                                        <span class="age">{age(approval.created_at)}</span>
                                        <StatusPill status=approval.status/>
                                    </div>
                                </article>
                            }).collect_view()}
                        </div>
                    }.into_any()
                }}
            </section>

            <section class="panel" aria-labelledby="runs-title">
                <div class="panel-head">
                    <h2 id="runs-title">"Runs that stopped"</h2>
                    <span class="count">{runs.len()}</span>
                </div>
                {if runs.is_empty() {
                    view! { <Empty message="Nothing is stuck.".to_owned()/> }.into_any()
                } else {
                    view! {
                        <div class="rows">
                            {runs.into_iter().map(|run| {
                                let card = run.card_id.clone();
                                view! {
                                    <article class="row-card">
                                        <div class="row-main">
                                            <div class="row-title">
                                                {match card.clone() {
                                                    Some(id) => view! { <A href=format!("/card/{id}")>{short(&id)}</A> }.into_any(),
                                                    None => view! { <span class="mono">{short(&run.id)}</span> }.into_any(),
                                                }}
                                            </div>
                                            <div class="row-sub">
                                                <span class="muted">{run.error_class.clone().unwrap_or_default()}</span>
                                                {run.started_at.map(|at| view! {
                                                    <span class="muted">"started " {age(Some(at))} " ago"</span>
                                                })}
                                            </div>
                                        </div>
                                        <div class="row-side">
                                            <StatusPill status=run.status/>
                                        </div>
                                    </article>
                                }
                            }).collect_view()}
                        </div>
                    }.into_any()
                }}
            </section>
        </div>
    }
}

fn greeting_for(name: &str) -> String {
    let hour = js_sys::Date::new_0().get_hours();
    let part = if hour < 12 {
        "morning"
    } else if hour < 18 {
        "afternoon"
    } else {
        "evening"
    };
    format!("Good {part}, {name}")
}

fn short(id: &str) -> String {
    id.chars().take(8).collect()
}
