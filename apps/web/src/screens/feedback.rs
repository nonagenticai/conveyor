//! Feedback — a bug, a feature request, or another note about the platform itself (#842).
//!
//! The page is the tenant's own: a form that captures the route it was sent from, and the
//! tenant's submissions with the operator's status. The cross-tenant view and the triage are the
//! operator's, in `conveyor admin feedback`; this screen never claims to see another tenant.

use leptos::prelude::*;
use serde_json::json;
use wasm_bindgen::JsCast;

use crate::api::{ApiError, InvokeOptions};
use crate::components::{Empty, ErrorPanel, Planned, StatusPill, age};
use crate::state::AppCtx;

#[derive(Clone, Debug, serde::Deserialize)]
struct FeedbackRow {
    id: String,
    kind: String,
    title: String,
    body: String,
    route: Option<String>,
    status: String,
    created_at: String,
}

#[derive(Clone, Debug, serde::Deserialize)]
struct FeedbackData {
    items: Vec<FeedbackRow>,
}

fn load(ctx: AppCtx) -> LocalResource<Result<Vec<FeedbackRow>, ApiError>> {
    LocalResource::new(move || async move {
        // Read every reactive input before the first await.
        let _tenant = ctx.tenant.get();
        let client = ctx.client();
        let value = client
            .invoke(
                "feedback.list",
                json!({"limit": 100}),
                InvokeOptions::default(),
            )
            .await?
            .output;
        let data: FeedbackData = serde_json::from_value(value)
            .map_err(|error| ApiError::Network(format!("feedback: {error}")))?;
        Ok(data.items)
    })
}

/// The route the submission is sent from. The operator reads this instead of a screenshot.
fn current_route() -> String {
    web_sys::window()
        .and_then(|window| window.location().pathname().ok())
        .unwrap_or_else(|| "/feedback".to_owned())
}

fn event_target_value(event: &web_sys::Event) -> String {
    event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|input| input.value())
        .unwrap_or_default()
}

fn area_value(event: &web_sys::Event) -> String {
    event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
        .map(|area| area.value())
        .unwrap_or_default()
}

#[component]
pub fn FeedbackScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let data = load(ctx);
    let retry = Callback::new(move |_| data.refetch());
    let notice = RwSignal::new(Option::<String>::None);
    let kind = RwSignal::new("bug".to_owned());
    let title = RwSignal::new(String::new());
    let body = RwSignal::new(String::new());
    let busy = RwSignal::new(false);

    let on_submit = move |event: web_sys::SubmitEvent| {
        event.prevent_default();
        let kind_value = kind.get_untracked();
        let title_text = title.get_untracked().trim().to_owned();
        let body_text = body.get_untracked().trim().to_owned();
        if title_text.is_empty() || body_text.is_empty() {
            notice.set(Some("A title and a description are required.".into()));
            return;
        }
        let route = current_route();
        let ctx = ctx;
        busy.set(true);
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "feedback.create",
                    json!({"kind": kind_value, "title": title_text, "body": body_text, "route": route}),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => {
                    notice.set(Some(
                        "Thank you — the platform operator will see this.".into(),
                    ));
                    title.set(String::new());
                    body.set(String::new());
                    data.refetch();
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
            busy.set(false);
        });
    };

    view! {
        <section class="screen">
            {move || {
                if !ctx.has_action("feedback.list") {
                    return view! {
                        <Planned
                            title="Feedback is planned".to_owned()
                            detail="feedback.list is not in this role's manifest, so there is nothing to show.".to_owned()
                        />
                    }
                    .into_any();
                }
                view! {
                    <header class="screen-head">
                        <div>
                            <h1>"Feedback"</h1>
                            <p class="muted">
                                "Report a bug or request a feature for the Conveyor platform itself. The platform operator reads every tenant's submissions."
                            </p>
                        </div>
                        <button class="btn" on:click=move |_| data.refetch()>"Refresh"</button>
                    </header>
                    {move || notice.get().map(|message| view! {
                        <div class="panel notice" role="status"><p>{message}</p></div>
                    })}
                    {move || ctx.has_action("feedback.create").then(|| view! {
                        <form class="panel" on:submit=on_submit>
                            <h2>"Send feedback"</h2>
                            <div class="formrow">
                                <label class="filter">
                                    <span>"Kind"</span>
                                    <select
                                        aria-label="Feedback kind"
                                        on:change=move |event| {
                                            if let Some(select) = event.target().and_then(|t| t.dyn_into::<web_sys::HtmlSelectElement>().ok()) {
                                                kind.set(select.value());
                                            }
                                        }
                                    >
                                        <option value="bug" selected=move || kind.get() == "bug">"Bug"</option>
                                        <option value="feature" selected=move || kind.get() == "feature">"Feature request"</option>
                                        <option value="other" selected=move || kind.get() == "other">"Other"</option>
                                    </select>
                                </label>
                                <label class="filter">
                                    <span>"Title"</span>
                                    <input
                                        type="text"
                                        prop:value=move || title.get()
                                        on:input=move |event| title.set(event_target_value(&event))
                                        placeholder="One line"
                                    />
                                </label>
                            </div>
                            <label class="filter">
                                <span>"What happened, or what would help?"</span>
                                <textarea
                                    class="inp"
                                    rows="5"
                                    prop:value=move || body.get()
                                    on:input=move |event| body.set(area_value(&event))
                                    placeholder="Steps, the page you were on, what you expected. Never include passwords or tokens."
                                ></textarea>
                            </label>
                            <p class="muted mono">{move || format!("Sent from: {}", current_route())}</p>
                            <button class="btn primary" type="submit" disabled=move || busy.get()>
                                {move || if busy.get() { "Sending…" } else { "Send feedback" }}
                            </button>
                        </form>
                    })}
                    {move || match data.get() {
                        None => view! { <p class="muted">"Loading feedback…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=retry/>
                        }.into_any(),
                        Some(Ok(rows)) if rows.is_empty() => view! {
                            <Empty message="No feedback yet. Yours will appear here.".to_owned()/>
                        }.into_any(),
                        Some(Ok(rows)) => view! {
                            <div class="panel">
                                <h2>"Your tenant's submissions"</h2>
                                <table class="table">
                                    <thead>
                                        <tr>
                                            <th>"Kind"</th>
                                            <th>"Status"</th>
                                            <th>"Title"</th>
                                            <th>"Sent from"</th>
                                            <th>"When"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {rows.into_iter().map(|row| view! {
                                            <tr data-testid="feedback-row" data-id=row.id.clone()>
                                                <td>{row.kind.clone()}</td>
                                                <td><StatusPill status=row.status.clone()/></td>
                                                <td>
                                                    <strong>{row.title.clone()}</strong>
                                                    <small class="muted">{row.body.clone()}</small>
                                                </td>
                                                <td class="mono">
                                                    {row.route.clone().unwrap_or_else(|| "—".to_owned())}
                                                </td>
                                                <td>
                                                    {age(row.created_at.parse::<chrono::DateTime<chrono::Utc>>().ok())}
                                                </td>
                                            </tr>
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        }.into_any(),
                    }}
                }
                .into_any()
            }}
        </section>
    }
}
