//! The run queue: one page of runs, newest first, with the state machine shown as a pill.
//!
//! There is no `runs.get` action and no `GET /api/runs/{id}` route, so this screen is the
//! queue only; the run-detail destination is a recorded contract gap, not a faked panel.

use leptos::prelude::*;
use leptos_router::components::A;
use serde_json::json;
use wasm_bindgen::JsCast;

use crate::api::{ApiError, InvokeOptions, Page};
use crate::components::{Empty, ErrorPanel, StatusPill, age};
use crate::models::RunOut;
use crate::sse;
use crate::state::AppCtx;

const FILTERS: [(&str, &str); 8] = [
    ("all", "All"),
    ("queued", "Queued"),
    ("running", "Running"),
    ("waiting_approval", "Waiting"),
    ("succeeded", "Succeeded"),
    ("failed", "Failed"),
    ("cancelled", "Cancelled"),
    ("expired", "Expired"),
];

#[component]
pub fn RunQueue() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let status = RwSignal::new("all".to_owned());
    let data = LocalResource::new(move || async move {
        let _tenant = ctx.tenant.get();
        let filter = status.get();
        let input = if filter == "all" {
            json!({ "limit": 50 })
        } else {
            json!({ "status": filter, "limit": 50 })
        };
        let value = ctx
            .invoke("runs.list", input, InvokeOptions::default())
            .await?
            .output;
        let page: Page<RunOut> = serde_json::from_value(value)
            .map_err(|error| ApiError::Network(format!("runs: {error}")))?;
        Ok::<Vec<RunOut>, ApiError>(page.items)
    });

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
            "run".to_owned(),
            None,
            move |frame| match frame {
                Ok(frame) if frame.event == "resync" || frame.event.starts_with("run.") => {
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

    let on_filter = move |event: web_sys::Event| {
        let value = event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::HtmlSelectElement>().ok())
            .map(|select| select.value())
            .unwrap_or_else(|| "all".to_owned());
        status.set(value);
    };
    let retry = Callback::new(move |_| data.refetch());

    view! {
        <section class="screen">
            <header class="screen-head">
                <div>
                    <h1>"Runs"</h1>
                    <p class="muted">"The queue, newest first. Every row is the state machine's current position."</p>
                </div>
                <label class="tenant-switch">
                    <span class="sr-only">"Status filter"</span>
                    <select on:change=on_filter>
                        {FILTERS.iter().map(|(value, label)| view! {
                            <option value=*value selected=move || status.get() == *value>{*label}</option>
                        }).collect_view()}
                    </select>
                </label>
            </header>
            {move || match data.get() {
                None => view! { <p class="muted">"Loading…"</p> }.into_any(),
                Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                Some(Ok(runs)) if runs.is_empty() => view! {
                    <div class="panel"><Empty message="No runs in this filter.".to_owned()/></div>
                }.into_any(),
                Some(Ok(runs)) => view! {
                    <div class="panel">
                        <div class="rows">
                            {runs.into_iter().map(|run| {
                                let card = run.card_id.clone();
                                let run_href = format!("/runs/{}", run.id);
                                let run_label = short(&run.id);
                                view! {
                                    <article class="row-card">
                                        <div class="row-main">
                                            <div class="row-title">
                                                <A href=run_href><span class="mono">{run_label}</span></A>
                                                {card.map(|id| view! {
                                                    <A href=format!("/card/{id}")><span class="muted">" · card"</span></A>
                                                })}
                                            </div>
                                            <div class="row-sub">
                                                <span class="mono">"attempt " {run.attempt}</span>
                                                <span class="muted">{run.error_class.clone().unwrap_or_default()}</span>
                                                {run.started_at.map(|at| view! {
                                                    <span class="muted">"started " {age(Some(at))} " ago"</span>
                                                })}
                                                <span class="age">"created " {age(run.created_at)}</span>
                                            </div>
                                        </div>
                                        <div class="row-side">
                                            <StatusPill status=run.status/>
                                            <span class="adv mono">{format!("{} · v{}", run.id, run.version)}</span>
                                        </div>
                                    </article>
                                }
                            }).collect_view()}
                        </div>
                    </div>
                }.into_any(),
            }}
        </section>
    }
}

fn short(id: &str) -> String {
    id.chars().take(8).collect()
}
