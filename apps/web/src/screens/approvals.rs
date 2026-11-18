//! Approvals — the list that reads a gate with no board, card, run or eval.
//!
//! Every destructive action asks for one of these before it runs (SPEC_V2 §11.4, #282), and the
//! console's destructive flow creates them (#285). Most of the actions it gates belong to no
//! parent object — `documents.delete`, `secrets.delete`, `models.delete` — so this screen, not a
//! card's cockpit, is where their gate is read and decided.
//!
//! A row shows what an approver needs to judge it: the action, the **target** it authorises, the
//! proposed input, who asked, and how long it has waited. The decide control is present for every
//! gate the caller may decide; where the tenant forbids self-approval and the caller is the
//! requester, it is **disabled with the reason** rather than hidden — a screen that silently
//! omits the control reads as "nothing to do here", which is wrong.

use std::collections::BTreeMap;

use leptos::prelude::*;
use serde_json::{Value, json};

use crate::api::{ApiError, InvokeOptions, Page};
use crate::components::{Empty, ErrorPanel, Planned, age, short_id};
use crate::models::ApprovalOut;
use crate::state::AppCtx;

#[derive(Clone, Debug)]
struct ApprovalsData {
    rows: Vec<ApprovalOut>,
    /// The tenant's policy per action: `(approval, self_approval)`.
    policies: BTreeMap<String, (String, String)>,
    /// The caller's own principal id, so a self-gate is recognisable.
    me: String,
}

fn parse<T: serde::de::DeserializeOwned>(value: Value, what: &str) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Network(format!("{what}: {error}")))
}

fn load(ctx: AppCtx) -> LocalResource<Result<ApprovalsData, ApiError>> {
    LocalResource::new(move || async move {
        // Re-read on a tenant switch like every other screen.
        let _tenant = ctx.tenant.get();
        let client = ctx.client();
        let page: Page<ApprovalOut> = parse(
            client
                .invoke(
                    "approvals.list",
                    json!({"status": "pending", "limit": 100}),
                    InvokeOptions::default(),
                )
                .await?
                .output,
            "approvals",
        )?;
        // One policy read per distinct action, not per row. An action with no policy row
        // answers the strict default; a refused read is the same default, never a waiver.
        let mut policies = BTreeMap::new();
        for row in &page.items {
            if policies.contains_key(&row.gate) {
                continue;
            }
            let pair = crate::approvals::policy_for(&client, &row.gate).await;
            policies.insert(row.gate.clone(), pair);
        }
        let me = ctx.me.get().map(|me| me.principal.id).unwrap_or_default();
        Ok(ApprovalsData {
            rows: page.items,
            policies,
            me,
        })
    })
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

#[component]
pub fn Approvals() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let data = load(ctx);
    let retry = Callback::new(move |_| data.refetch());
    let notice = RwSignal::new(Option::<String>::None);

    let decide = move |approval_id: String, decision: &'static str| {
        let ctx = ctx;
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "approvals.decide",
                    json!({"approval_id": approval_id, "decision": decision}),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => {
                    notice.set(Some(match decision {
                        "approve" => "Approved.".to_owned(),
                        _ => "Rejected.".to_owned(),
                    }));
                    data.refetch();
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    };

    view! {
        <section class="screen">
            <header class="screen-head">
                <div>
                    <h1>"Approvals"</h1>
                    <p class="muted">
                        "Gates waiting for a person. A destructive action runs only after the gate \
                         naming its target is approved."
                    </p>
                </div>
            </header>

            {move || notice.get().map(|text| view! { <p class="notice">{text}</p> })}

            {move || {
                if !ctx.has_action("approvals.list") {
                    return view! {
                        <Planned
                            title="Approvals".to_owned()
                            detail="This role cannot list approvals, so no gate is shown here."
                                .to_owned()
                        />
                    }.into_any();
                }
                match data.get() {
                    None => view! { <p class="muted">"Loading…"</p> }.into_any(),
                    Some(Err(error)) => view! {
                        <ErrorPanel error=error on_retry=retry/>
                    }.into_any(),
                    Some(Ok(loaded)) if loaded.rows.is_empty() => view! {
                        <Empty message="No gate is waiting. A destructive action that needs \
                             approval appears here the moment it is asked for.".to_owned()/>
                    }.into_any(),
                    Some(Ok(loaded)) => {
                        let me = loaded.me.clone();
                        let policies = loaded.policies.clone();
                        let rows = loaded.rows.iter().cloned().map(|row| {
                            let (.., self_approval) = policies
                                .get(&row.gate)
                                .cloned()
                                .unwrap_or_else(|| ("required".to_owned(), "forbidden".to_owned()));
                            let mine = !me.is_empty()
                                && row.requested_by.as_deref() == Some(me.as_str());
                            // The requester may not decide their own gate unless the tenant
                            // allows it; say why, do not hide the control.
                            let refusal =
                                crate::approvals::self_gate_refusal(mine, &self_approval);
                            let approve_id = row.id.clone();
                            let reject_id = row.id.clone();
                            view! {
                                <article class="panel approval-row" data-approval=row.id.clone()>
                                    <header class="approval-head">
                                        <span class="mono action">{row.gate.clone()}</span>
                                        <span class="muted">{age(row.created_at)}</span>
                                    </header>
                                    <p class="approval-target mono" data-target=row.target.clone().unwrap_or_default()>
                                        {row.target.clone().unwrap_or_else(|| "(no target)".to_owned())}
                                    </p>
                                    <p class="muted">
                                        "Requested by "
                                        {row.requested_by_label.clone()
                                            .or_else(|| row.requested_by.as_ref().map(|id| short_id(Some(id))))
                                            .unwrap_or_else(|| "an unknown principal".to_owned())}
                                        {" · "}
                                        {row.approvals}
                                        " of "
                                        {row.required_decisions}
                                        " decisions"
                                    </p>
                                    {row.description.clone().map(|text| view! { <p>{text}</p> })}
                                    {row.proposed_input.clone().map(|input| view! {
                                        <pre class="mono proposed">{pretty(&input)}</pre>
                                    })}
                                    {row.proposed_answer.clone().map(|text| view! {
                                        <p class="muted">{format!("Proposed: {text}")}</p>
                                    })}
                                    <div class="approval-actions">
                                        <button
                                            class="btn small"
                                            disabled=refusal.is_some()
                                            title=refusal.clone().unwrap_or_default()
                                            on:click=move |_| decide(approve_id.clone(), "approve")
                                        >"Approve"</button>
                                        <button
                                            class="btn small ghost"
                                            disabled=refusal.is_some()
                                            title=refusal.clone().unwrap_or_default()
                                            on:click=move |_| decide(reject_id.clone(), "reject")
                                        >"Reject"</button>
                                        {refusal.clone().map(|reason| view! {
                                            <span class="muted refusal">{reason}</span>
                                        })}
                                    </div>
                                </article>
                            }
                        }).collect_view();
                        view! { <div class="approvals">{rows}</div> }.into_any()
                    }
                }
            }}
        </section>
    }
}
