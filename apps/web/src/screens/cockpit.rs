//! The card cockpit: one canonical route per card (`/card/{id}`), never a modal.
//!
//! Every write carries the card's version as `If-Match` except `approvals.decide`, whose
//! contract is status-guarded (a quorum, not a row version). A 412 refetches; it never
//! guesses the new version.

use std::collections::BTreeMap;

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
    /// The tenant's policy per gate: `(approval, self_approval)`.
    policies: BTreeMap<String, (String, String)>,
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
        // One policy read per distinct gate, not per row.
        let mut policies = BTreeMap::new();
        for approval in &approvals.items {
            if policies.contains_key(&approval.gate) {
                continue;
            }
            let pair = crate::approvals::policy_for(&client, &approval.gate).await;
            policies.insert(approval.gate.clone(), pair);
        }
        Ok::<CockpitData, ApiError>(CockpitData {
            card,
            runs: runs.items,
            approvals: approvals.items,
            policies,
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

    // #375: the body must survive a refetch. `match data.get()` re-created the whole subtree —
    // the Save button included — on every new value, so an SSE refresh that landed while someone
    // was editing detached the control under the pointer. The last good value now lives in
    // `body`; `CockpitBody` is created once and every region inside it reads `body` reactively,
    // so a refresh updates text and lists in place instead of remounting the screen.
    let body = RwSignal::new(None::<CockpitData>);
    let body_error = RwSignal::new(None::<ApiError>);
    Effect::new(move |_| match data.get() {
        None => {}
        Some(Ok(next)) => {
            body.set(Some(next));
            body_error.set(None);
        }
        Some(Err(error)) => body_error.set(Some(error)),
    });

    let retry = Callback::new(move |_| data.refetch());
    view! {
        <Show when=move || body.get().is_none() && body_error.get().is_none()>
            <p class="muted">"Loading the card…"</p>
        </Show>
        {move || body_error.get().map(|error| view! {
            <ErrorPanel error=error on_retry=retry/>
        })}
        <Show when=move || body.get().is_some() && body_error.get().is_none()>
            <CockpitBody data=body resource=data/>
        </Show>
    }
}

// `data` is the last fetched cockpit value, read reactively: this component is created once per
// card, and a refetch updates its regions in place (#375).
#[component]
fn CockpitBody(
    data: RwSignal<Option<CockpitData>>,
    resource: LocalResource<Result<CockpitData, ApiError>>,
) -> impl IntoView {
    // The parent renders this only while `body` holds a value, and a refetch never clears it.
    // Deriving a non-optional signal here keeps every region reactive without unwrapping the
    // `Option` at each of the reads below.
    let data = Signal::derive(move || {
        data.get()
            .expect("CockpitBody is rendered only while the cockpit body is loaded")
    });
    let ctx = expect_context::<AppCtx>();
    let me = ctx.me.get().map(|me| me.principal.id);
    // The claim controls read it in one closure and the decisions list in another; both are
    // `move` closures created here, so each takes its own copy.
    let me_for_decisions = me.clone();

    // The card edit is a small inline form: title + description under the row's own If-Match.
    // A 412 is the caller's stale read, so the screen refetches rather than guessing a version.
    // The draft and the version it is based on are captured when the form OPENS — a refetch that
    // lands mid-edit changes neither, which is the other half of #375.
    let can_edit = ctx.has_action("cards.update");
    let editing = RwSignal::new(false);
    let opening = data.get_untracked().card.card;
    let name_draft = RwSignal::new(opening.name.clone());
    let description_draft = RwSignal::new(opening.description.clone());
    let base_version = RwSignal::new(opening.version);
    let save_edit = move |_| {
        let current = data.get_untracked().card.card;
        let name = name_draft.get_untracked().trim().to_owned();
        let description = description_draft.get_untracked();
        if name.is_empty() {
            return;
        }
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "cards.update",
                    json!({ "id": current.id, "name": name, "description": description }),
                    InvokeOptions {
                        if_match: Some(base_version.get_untracked()),
                        idempotency_key: None,
                        approval_id: None,
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
    let toggle_edit = move |_| {
        editing.update(|on| {
            if !*on {
                let current = data.get_untracked().card.card;
                base_version.set(current.version);
                name_draft.set(current.name.clone());
                description_draft.set(current.description.clone());
            }
            *on = !*on;
        });
    };

    let claim = move |_| {
        let current = data.get_untracked().card.card;
        act(
            ctx,
            resource,
            "cards.claim",
            json!({ "card_id": current.id }),
            Some(current.version),
        );
    };
    let release = move |_| {
        let current = data.get_untracked().card.card;
        act(
            ctx,
            resource,
            "cards.release",
            json!({ "card_id": current.id }),
            None,
        );
    };
    let new_run = move |_| {
        let ctx = ctx;
        let card_id = data.get_untracked().card.card.id;
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
                    <h1>{move || data.get().card.card.name.clone()}</h1>
                    <p class="muted">
                        {move || data.get().card.card.card_type_code.clone()}
                        " · list " {move || short(&data.get().card.card.list_id)}
                        " · updated " {move || age(data.get().card.card.updated_at)}
                    </p>
                </div>
                <div class="actions">
                    {can_edit.then(|| view! {
                        <button class="btn" on:click=toggle_edit>
                            {move || if editing.get() { "Cancel" } else { "Edit" }}
                        </button>
                    })}
                    {move || {
                        let card = data.get().card.card;
                        let mine = card.assignee_id.is_some() && card.assignee_id == me;
                        if mine {
                            view! { <button class="btn" on:click=release.clone()>"Release"</button> }
                                .into_any()
                        } else {
                            view! { <button class="btn" on:click=claim.clone()>"Claim"</button> }
                                .into_any()
                        }
                    }}
                    <button class="btn primary" on:click=new_run>"New run"</button>
                </div>
            </header>

            <div class="grid-2">
                <section class="panel">
                    <div class="panel-head"><h2>"The card"</h2></div>
                    // The form is gated on `editing` ALONE. It must never depend on `data`, or a
                    // refetch would re-create the Save button under the pointer (#375).
                    <Show
                        when=move || editing.get()
                        fallback=move || {
                            let description = data.get().card.card.description.clone();
                            if description.trim().is_empty() {
                                view! { <p class="muted">"No description yet."</p> }.into_any()
                            } else {
                                view! { <p class="card-description">{description}</p> }.into_any()
                            }
                        }
                    >
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
                    </Show>
                    <div class="labels">
                        {move || data.get().card.labels.into_iter().map(|label| view! {
                            <span class="label">{label.name}</span>
                        }).collect_view()}
                    </div>
                    <span class="adv mono">{move || {
                        let card = data.get().card.card;
                        format!("card {} · v{}", card.id, card.version)
                    }}</span>
                </section>

                <section class="panel">
                    <div class="panel-head">
                        <h2>"Decisions"</h2>
                        <span class="count">{move || data.get().approvals.iter().filter(|approval| approval.status == "pending").count()}</span>
                    </div>
                    {move || {
                        let approvals = data.get().approvals;
                        let policies = data.get().policies;
                        let me = me_for_decisions.clone();
                        if approvals.is_empty() {
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
                                    let mine = me.is_some()
                                        && approval.requested_by.as_deref() == me.as_deref();
                                    let (.., self_approval) = policies
                                        .get(&approval.gate)
                                        .cloned()
                                        .unwrap_or_else(|| {
                                            ("required".to_owned(), "forbidden".to_owned())
                                        });
                                    // The requester may not decide their own gate unless the
                                    // tenant allows it; say why, do not hide the control.
                                    let refusal =
                                        crate::approvals::self_gate_refusal(mine, &self_approval);
                                    let disabled = refusal.is_some();
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
                                                        <button class="btn small primary" disabled=disabled on:click=approve>"Approve"</button>
                                                        <button class="btn small" disabled=disabled on:click=reject>"Reject"</button>
                                                        {refusal.map(|reason| view! {
                                                            <span class="muted">{reason}</span>
                                                        })}
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
                        }
                    }}
                </section>
            </div>
            <section class="panel">
                <div class="panel-head">
                    <h2>"Runs"</h2>
                    <span class="count">{move || data.get().runs.len()}</span>
                </div>
                {move || {
                    let runs = data.get().runs;
                    if runs.is_empty() {
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
                    }
                }}
            </section>

            <section class="panel">
                <div class="panel-head">
                    <h2>"Comments"</h2>
                    <span class="count">{move || data.get().card.comments.len()}</span>
                </div>
                {move || {
                    let comments = data.get().card.comments;
                    if comments.is_empty() {
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
                    }
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
                    approval_id: None,
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
