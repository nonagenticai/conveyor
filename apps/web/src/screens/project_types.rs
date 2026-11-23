//! Project types — the tenant's project requests and their promotion path.

use leptos::prelude::*;
use serde_json::{Value, json};

use crate::api::{ApiError, InvokeOptions};
use crate::components::{Empty, ErrorPanel, StatusPill, Tile, fmt_int};
use crate::screens::v2_data::{read_rows, text};
use crate::state::AppCtx;

#[derive(Clone, Debug, Default)]
struct RequestsData {
    requests: Vec<Value>,
}

async fn load(ctx: AppCtx) -> Result<RequestsData, ApiError> {
    let client = ctx.client();
    let requests = if ctx.has_action("project_requests.list") {
        read_rows(
            &client,
            "project_requests.list",
            json!({ "page": { "limit": 50 } }),
        )
        .await?
    } else {
        Vec::new()
    };
    Ok(RequestsData { requests })
}

#[component]
pub fn ProjectTypesScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let data = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        async move { load(ctx).await }
    });
    let retry = Callback::new(move |_| data.refetch());
    let notice = RwSignal::new(Option::<String>::None);

    let approve = Callback::new(move |id: String| {
        let ctx = ctx;
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "project_requests.approve",
                    json!({ "id": id }),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(invocation) => {
                    notice.set(Some(format!(
                        "Request {} approved.",
                        text(&invocation.output, "id")
                    )));
                    data.refetch();
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    });

    view! {
        <section class="screen" aria-labelledby="project-types-title">
            <header class="screen-head">
                <div>
                    <h1 id="project-types-title">"Project types"</h1>
                    <p class="muted">"The tenant's project requests, from draft to approved and promoted."</p>
                </div>
                <button class="btn" on:click=move |_| data.refetch()>"Refresh"</button>
            </header>
            {move || notice.get().map(|text| view! { <p class="notice" role="alert">{text}</p> })}
            {move || match data.get() {
                None => view! { <p class="muted">"Loading…"</p> }.into_any(),
                Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                Some(Ok(data)) => view! { <RequestsBody data=data approve=approve/> }.into_any(),
            }}
        </section>
    }
}

#[component]
fn RequestsBody(data: RequestsData, approve: Callback<String>) -> impl IntoView {
    let requests = data.requests;
    view! {
        <div class="grid-3">
            <Tile label="Requests".to_owned() value=fmt_int(requests.len() as i64) detail="project requests".to_owned()/>
        </div>
        {if requests.is_empty() {
            view! { <Empty message="No project request in this tenant yet.".to_owned()/> }.into_any()
        } else {
            view! {
                <table class="table" data-testid="project-types-table">
                    <thead><tr><th>"External ref"</th><th>"Origin"</th><th>"Status"</th><th>"Repository"</th><th>"Project"</th><th></th></tr></thead>
                    <tbody>
                        {requests.into_iter().map(|row| {
                            let id = text(&row, "id");
                            let status = text(&row, "status");
                            let can_approve = status == "draft";
                            let approve_id = id.clone();
                            view! {
                                <tr>
                                    <td class="mono">{text(&row, "external_ref")}</td>
                                    <td>{text(&row, "external_origin")}</td>
                                    <td><StatusPill status=status/></td>
                                    <td class="mono">{text(&row, "repository_url")}</td>
                                    <td class="mono">{text(&row, "project_id")}</td>
                                    <td>
                                        {can_approve.then(|| view! {
                                            <button class="btn small" on:click=move |_| approve.run(approve_id.clone())>
                                                "Approve"
                                            </button>
                                        })}
                                    </td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            }.into_any()
        }}
    }
}
