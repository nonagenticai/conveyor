//! The card cockpit: one canonical route per card (`/card/{id}`), never a modal.
//!
//! Every write carries the card's version as `If-Match` except `approvals.decide`, whose
//! contract is status-guarded (a quorum, not a row version). A 412 refetches; it never
//! guesses the new version.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use serde_json::json;

use crate::api::{ApiError, InvokeOptions, Page};
use crate::components::{Empty, ErrorPanel, StatusPill, age};
use crate::models::{ApprovalOut, CardView, RunOut};
use crate::sse;
use crate::state::AppCtx;

#[derive(Clone, Debug)]
struct CockpitData {
    card: CardView,
    runs: Vec<RunOut>,
    approvals: Vec<ApprovalOut>,
}

#[component]
pub fn Cockpit() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let params = use_params_map();
    let card_id = move || params.get().get("card_id");
    let data = LocalResource::new(move || async move {
        let id = card_id().ok_or_else(|| ApiError::NotFound("no card in the route".to_owned()))?;
        let _tenant = ctx.tenant.get();
        let client = ctx.client();
        let value = client.get(&format!("/api/cards/{id}")).await?;
        let card: CardView = serde_json::from_value(value)
            .map_err(|error| ApiError::Network(format!("card: {error}")))?;
        let runs = client
            .invoke(
                "runs.list",
                json!({ "card_id": id, "limit": 50 }),
                InvokeOptions::default(),
            )
            .await?
            .output;
        let runs: Page<RunOut> = serde_json::from_value(runs)
            .map_err(|error| ApiError::Network(format!("runs: {error}")))?;
        let approvals = client
            .invoke(
                "approvals.list",
                json!({ "card_id": id, "limit": 50 }),
                InvokeOptions::default(),
            )
            .await?
            .output;
        let approvals: Page<ApprovalOut> = serde_json::from_value(approvals)
            .map_err(|error| ApiError::Network(format!("approvals: {error}")))?;
        Ok::<CockpitData, ApiError>(CockpitData {
            card,
            runs: runs.items,
            approvals: approvals.items,
        })
    });

    let subscription = RwSignal::new(None::<sse::Subscription>);
    Effect::new(move |_| {
        let Some(id) = card_id() else { return };
        subscription.update(|slot| {
            if let Some(old) = slot.take() {
                old.abort();
            }
        });
        let target = data;
        match sse::subscribe(
            ctx.client(),
            format!("card:{id},run,approval"),
            None,
            move |frame| match frame {
                Ok(frame)
                    if frame.event == "resync"
                        || frame.event.starts_with("card.")
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
    view! {
        {move || match data.get() {
            None => view! { <p class="muted">"Loading the card…"</p> }.into_any(),
            Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
            Some(Ok(cockpit)) => view! { <CockpitBody data=cockpit resource=data/> }.into_any(),
        }}
    }
}

#[component]
fn CockpitBody(
    data: CockpitData,
    resource: LocalResource<Result<CockpitData, ApiError>>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let me = ctx.me.get().map(|me| me.principal.id);
    let card = data.card.card;
    let labels = data.card.labels;
    let comments = data.card.comments;
    let runs = data.runs;
    let approvals = data.approvals;
    let card_for_actions = card.clone();
    let card_description = card.description.clone();
    let claimed_by_me = card.assignee_id.is_some() && card.assignee_id == me;
    let card_for_claim = card_for_actions.clone();
    let card_for_release = card_for_actions.clone();
    let card_for_run = card_for_actions.clone();

    // The card edit is a small inline form: title + description under the row's own If-Match.
    // A 412 is the caller's stale read, so the screen refetches rather than guessing a version.
    let can_edit = ctx.has_action("cards.update");
    let editing = RwSignal::new(false);
    let name_draft = RwSignal::new(card_for_actions.name.clone());
    let description_draft = RwSignal::new(card_for_actions.description.clone());
    let edit_id = card_for_actions.id.clone();
    let edit_version = card_for_actions.version;
    let save_edit = move |_| {
        let id = edit_id.clone();
        let name = name_draft.get_untracked().trim().to_owned();
        let description = description_draft.get_untracked();
        if name.is_empty() {
            return;
        }
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "cards.update",
                    json!({ "id": id, "name": name, "description": description }),
                    InvokeOptions {
                        if_match: Some(edit_version),
                        idempotency_key: None,
                    },
                )
                .await
            {
                Ok(_) => {
                    editing.set(false);
                    resource.refetch();
                }
                Err(error) => ctx.report(&error),
            }
        });
    };

    let claim = move |_| {
        act(
            ctx,
            resource,
            "cards.claim",
            json!({ "card_id": card_for_claim.id.clone() }),
            Some(card_for_claim.version),
        );
    };
    let release = move |_| {
        act(
            ctx,
            resource,
            "cards.release",
            json!({ "card_id": card_for_release.id.clone() }),
            None,
        );
    };
    let new_run = move |_| {
        let ctx = ctx;
        let card_id = card_for_run.id.clone();
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "runs.create",
                    json!({ "card_id": card_id }),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(created) => {
                    let run_id = created
                        .output
                        .get("id")
                        .and_then(|value| value.as_str())
                        .map(str::to_owned);
                    if let Some(run_id) = run_id {
                        let _ = ctx
                            .invoke(
                                "runs.start",
                                json!({ "id": run_id }),
                                InvokeOptions::default(),
                            )
                            .await;
                    }
                    resource.refetch();
                }
                Err(error) => ctx.report(&error),
            }
        });
    };

    view! {
        <section class="screen cockpit">
            <header class="screen-head">
                <div>
                    <p class="muted"><A href="/boards">"Board"</A>" / card"</p>
                    <h1>{card.name.clone()}</h1>
                    <p class="muted">
                        {card.card_type_code.clone()}
                        " · list " {short(&card.list_id)}
                        " · updated " {age(card.updated_at)}
                    </p>
                </div>
                <div class="actions">
                    {can_edit.then(|| view! {
                        <button class="btn" on:click=move |_| editing.update(|on| *on = !*on)>
                            {move || if editing.get() { "Cancel" } else { "Edit" }}
                        </button>
                    })}
                    {(!claimed_by_me).then(|| view! {
                        <button class="btn" on:click=claim>"Claim"</button>
                    })}
                    {claimed_by_me.then(|| view! {
                        <button class="btn" on:click=release>"Release"</button>
                    })}
                    <button class="btn primary" on:click=new_run>"New run"</button>
                </div>
            </header>

            <div class="grid-2">
                <section class="panel">
                    <div class="panel-head"><h2>"The card"</h2></div>
                    {move || if editing.get() {
                        view! {
                            <div class="form">
                                <label class="filter">
                                    <span>"Title"</span>
                                    <input
                                        type="text"
                                        prop:value=move || name_draft.get()
                                        on:input=move |event| name_draft.set(event_target_value(&event))
                                    />
                                </label>
                                <label class="filter">
                                    <span>"Description"</span>
                                    <textarea
                                        rows="4"
                                        prop:value=move || description_draft.get()
                                        on:input=move |event| description_draft.set(event_target_value(&event))
                                    ></textarea>
                                </label>
                                <button class="btn primary" on:click=save_edit.clone()>"Save"</button>
                            </div>
                        }.into_any()
                    } else if card_description.trim().is_empty() {
                        view! { <p class="muted">"No description yet."</p> }.into_any()
                    } else {
                        view! { <p class="card-description">{card_description.clone()}</p> }.into_any()
                    }}
                    <div class="labels">
                        {labels.into_iter().map(|label| view! {
                            <span class="label">{label.name}</span>
                        }).collect_view()}
                    </div>
                    <span class="adv mono">{format!("card {} · v{}", card.id, card.version)}</span>
                </section>

                <section class="panel">
                    <div class="panel-head">
                        <h2>"Decisions"</h2>
                        <span class="count">{approvals.iter().filter(|approval| approval.status == "pending").count()}</span>
                    </div>
                    {if approvals.is_empty() {
                        view! { <Empty message="No approval gates on this card.".to_owned()/> }.into_any()
                    } else {
                        view! {
                            <div class="rows">
                                {approvals.into_iter().map(|approval| {
                                    let decision_id = approval.id.clone();
                                    let approve = move |_| {
                                        decide(ctx, resource, decision_id.clone(), "approve");
                                    };
                                    let decision_id = approval.id.clone();
                                    let reject = move |_| {
                                        decide(ctx, resource, decision_id.clone(), "reject");
                                    };
                                    let pending = approval.status == "pending";
                                    let decisions = approval.decisions.clone();
                                    view! {
                                        <article class="row-card">
                                            <div class="row-main">
                                                <div class="row-title">{approval.gate}</div>
                                                <div class="row-sub">
                                                    <span class="muted">{approval.kind}</span>
                                                    <span class="age">{age(approval.created_at)}</span>
                                                </div>
                                            </div>
                                            <div class="row-side">
                                                <StatusPill status=approval.status/>
                                                {pending.then(|| view! {
                                                    <span class="decision-buttons">
                                                        <button class="btn small primary" on:click=approve>"Approve"</button>
                                                        <button class="btn small" on:click=reject>"Reject"</button>
                                                    </span>
                                                })}
                                            </div>
                                            {(!decisions.is_empty()).then(|| view! {
                                                <div class="row-sub" style="flex-basis:100%">
                                                    {decisions.into_iter().map(|decision| {
                                                        let channel = decision
                                                            .via
                                                            .as_ref()
                                                            .and_then(|via| via.get("channel"))
                                                            .and_then(|value| value.as_str())
                                                            .unwrap_or("unknown")
                                                            .to_owned();
                                                        let agent = decision.decided_by_kind != "human";
                                                        view! {
                                                            <span class="pill" class:bad=agent>
                                                                {format!("{} · {} · {} · {}", decision.approver, decision.decided_by_kind, channel, age(Some(decision.decided_at)))}
                                                            </span>
                                                        }
                                                    }).collect_view()}
                                                </div>
                                            })}
                                        </article>
                                    }
                                }).collect_view()}
                            </div>
                        }.into_any()
                    }}
                </section>
            </div>

            <section class="panel">
                <div class="panel-head">
                    <h2>"Runs"</h2>
                    <span class="count">{runs.len()}</span>
                </div>
                {if runs.is_empty() {
                    view! { <Empty message="No runs yet. New run creates and starts one.".to_owned()/> }.into_any()
                } else {
                    view! {
                        <div class="rows">
                            {runs.into_iter().map(|run| {
                                let run_id = run.id.clone();
                                let start = move |_| {
                                    act(ctx, resource, "runs.start", json!({ "id": run_id.clone() }), None);
                                };
                                let queued = run.status == "queued";
                                view! {
                                    <article class="row-card">
                                        <div class="row-main">
                                            <div class="row-title mono">{short(&run.id)}</div>
                                            <div class="row-sub">
                                                <span class="muted">{run.error_class.clone().unwrap_or_default()}</span>
                                                {run.started_at.map(|at| view! {
                                                    <span class="muted">"started " {age(Some(at))} " ago"</span>
                                                })}
                                            </div>
                                        </div>
                                        <div class="row-side">
                                            <StatusPill status=run.status/>
                                            {queued.then(|| view! {
                                                <button class="btn small" on:click=start>"Start"</button>
                                            })}
                                        </div>
                                    </article>
                                }
                            }).collect_view()}
                        </div>
                    }.into_any()
                }}
            </section>

            <section class="panel">
                <div class="panel-head">
                    <h2>"Comments"</h2>
                    <span class="count">{comments.len()}</span>
                </div>
                {if comments.is_empty() {
                    view! { <Empty message="No comments.".to_owned()/> }.into_any()
                } else {
                    view! {
                        <div class="comments">
                            {comments.into_iter().map(|comment| view! {
                                <article class="comment">
                                    <div class="row-sub">
                                        <b>{comment.author_name.unwrap_or_else(|| "someone".to_owned())}</b>
                                        <span class="age">{age(comment.created_at)}</span>
                                    </div>
                                    <p>{comment.body}</p>
                                </article>
                            }).collect_view()}
                        </div>
                    }.into_any()
                }}
            </section>
        </section>
    }
}

fn decide(
    ctx: AppCtx,
    resource: LocalResource<Result<CockpitData, ApiError>>,
    approval_id: String,
    decision: &'static str,
) {
    act(
        ctx,
        resource,
        "approvals.decide",
        json!({ "approval_id": approval_id, "decision": decision }),
        None,
    );
}

fn act(
    ctx: AppCtx,
    resource: LocalResource<Result<CockpitData, ApiError>>,
    name: &'static str,
    input: serde_json::Value,
    if_match: Option<i64>,
) {
    leptos::task::spawn_local(async move {
        let result = ctx
            .invoke(
                name,
                input,
                InvokeOptions {
                    if_match,
                    idempotency_key: None,
                },
            )
            .await;
        match result {
            Ok(_) => resource.refetch(),
            Err(error) => ctx.report(&error),
        }
    });
}

fn short(id: &str) -> String {
    id.chars().take(8).collect()
}
