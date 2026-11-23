//! Decisions — the ladder's recorded decisions and the policies that produce them.
//!
//! A dual-control gate is an approval, not a decision row; the screen links to `/approvals`
//! rather than inventing a second decide path.

use leptos::prelude::*;
use leptos_router::components::A;
use serde_json::{Value, json};

use crate::api::ApiError;
use crate::components::{Empty, ErrorPanel, StatusPill, Tile, fmt_int};
use crate::screens::v2_data::{int, read_rows, text};
use crate::state::AppCtx;

#[derive(Clone, Debug, Default)]
struct DecisionsData {
    decisions: Vec<Value>,
    policies: Vec<Value>,
}

async fn load(ctx: AppCtx) -> Result<DecisionsData, ApiError> {
    let client = ctx.client();
    let mut data = DecisionsData::default();
    if ctx.has_action("ladder.list") {
        data.decisions =
            read_rows(&client, "ladder.list", json!({ "page": { "limit": 50 } })).await?;
    }
    if ctx.has_action("decision_policies.list") {
        data.policies = read_rows(
            &client,
            "decision_policies.list",
            json!({ "page": { "limit": 50 } }),
        )
        .await?;
    }
    Ok(data)
}

#[component]
pub fn DecisionInboxScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let data = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        async move { load(ctx).await }
    });
    let retry = Callback::new(move |_| data.refetch());

    view! {
        <section class="screen" aria-labelledby="decisions-title">
            <header class="screen-head">
                <div>
                    <h1 id="decisions-title">"Decisions"</h1>
                    <p class="muted">"The ladder's recorded decisions and the policies behind them."</p>
                </div>
                <button class="btn" on:click=move |_| data.refetch()>"Refresh"</button>
            </header>
            {move || match data.get() {
                None => view! { <p class="muted">"Loading…"</p> }.into_any(),
                Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                Some(Ok(data)) => view! { <DecisionsBody data=data/> }.into_any(),
            }}
        </section>
    }
}

#[component]
fn DecisionsBody(data: DecisionsData) -> impl IntoView {
    let decisions = data.decisions;
    let policies = data.policies;
    view! {
        <div class="grid-3">
            <Tile label="Decisions".to_owned() value=fmt_int(decisions.len() as i64) detail="recorded rows".to_owned()/>
            <Tile label="Policies".to_owned() value=fmt_int(policies.len() as i64) detail="decision policies".to_owned()/>
        </div>
        <p class="muted">
            "A dual-control gate is approved in "
            <A href="/approvals">"Approvals"</A>
            "; the missing second approver is shown there, not guessed here."
        </p>
        <h2 id="decisions">"Recorded decisions"</h2>
        {if decisions.is_empty() {
            view! { <Empty message="No ladder decision in this tenant yet.".to_owned()/> }.into_any()
        } else {
            view! {
                <table class="table" data-testid="decisions-table">
                    <thead><tr><th>"Kind"</th><th>"Rung"</th><th>"Question"</th><th>"Answer"</th><th>"Project"</th></tr></thead>
                    <tbody>
                        {decisions.into_iter().map(|row| view! {
                            <tr>
                                <td>{text(&row, "kind")}</td>
                                <td><StatusPill status=text(&row, "rung")/></td>
                                <td>{text(&row, "question")}</td>
                                <td>{text(&row, "answer")}</td>
                                <td class="mono">{text(&row, "project_id")}</td>
                            </tr>
                        }).collect_view()}
                    </tbody>
                </table>
            }.into_any()
        }}
        <h2 id="policies">"Decision policies"</h2>
        {if policies.is_empty() {
            view! { <Empty message="No decision policy in this tenant yet.".to_owned()/> }.into_any()
        } else {
            view! {
                <table class="table" data-testid="decisions-policies">
                    <thead><tr><th>"Kind"</th><th>"Key"</th><th>"Status"</th><th>"Version"</th></tr></thead>
                    <tbody>
                        {policies.into_iter().map(|row| view! {
                            <tr>
                                <td>{text(&row, "kind")}</td>
                                <td class="mono">{text(&row, "key")}</td>
                                <td><StatusPill status=text(&row, "status")/></td>
                                <td class="mono">{int(&row, "version").to_string()}</td>
                            </tr>
                        }).collect_view()}
                    </tbody>
                </table>
            }.into_any()
        }}
    }
}
