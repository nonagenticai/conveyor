//! Agents — the capability agents the organisation can bind.

use leptos::prelude::*;
use serde_json::{Value, json};

use crate::api::ApiError;
use crate::components::{Empty, ErrorPanel, StatusPill, Tile, fmt_int};
use crate::screens::v2_data::{count, read_rows, text};
use crate::state::AppCtx;

#[derive(Clone, Debug, Default)]
struct AgentsData {
    agents: Vec<Value>,
}

async fn load(ctx: AppCtx) -> Result<AgentsData, ApiError> {
    let client = ctx.client();
    let agents = if ctx.has_action("capability_agents.list") {
        read_rows(
            &client,
            "capability_agents.list",
            json!({ "page": { "limit": 50 } }),
        )
        .await?
    } else {
        Vec::new()
    };
    Ok(AgentsData { agents })
}

#[component]
pub fn AgentsScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let data = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        async move { load(ctx).await }
    });
    let retry = Callback::new(move |_| data.refetch());

    view! {
        <section class="screen" aria-labelledby="agents-title">
            <header class="screen-head">
                <div>
                    <h1 id="agents-title">"Agents"</h1>
                    <p class="muted">"The capability agents a binding can execute, and the actions each may call."</p>
                </div>
                <button class="btn" on:click=move |_| data.refetch()>"Refresh"</button>
            </header>
            {move || match data.get() {
                None => view! { <p class="muted">"Loading…"</p> }.into_any(),
                Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                Some(Ok(data)) => view! { <AgentsBody data=data/> }.into_any(),
            }}
        </section>
    }
}

#[component]
fn AgentsBody(data: AgentsData) -> impl IntoView {
    let agents = data.agents;
    view! {
        <div class="grid-3">
            <Tile label="Agents".to_owned() value=fmt_int(agents.len() as i64) detail="capability agents".to_owned()/>
        </div>
        {if agents.is_empty() {
            view! { <Empty message="No capability agents in this tenant yet.".to_owned()/> }.into_any()
        } else {
            view! {
                <table class="table" data-testid="agents-table">
                    <thead><tr><th>"Key"</th><th>"Status"</th><th>"Model"</th><th>"Prompt"</th><th>"Allowed actions"</th></tr></thead>
                    <tbody>
                        {agents.into_iter().map(|row| view! {
                            <tr>
                                <td class="mono">{text(&row, "key")}</td>
                                <td><StatusPill status=text(&row, "status")/></td>
                                <td class="mono">{text(&row, "model_ref")}</td>
                                <td class="mono">{text(&row, "prompt_ref")}</td>
                                <td>{fmt_int(count(&row, "allowed_actions") as i64)}</td>
                            </tr>
                        }).collect_view()}
                    </tbody>
                </table>
            }.into_any()
        }}
    }
}
