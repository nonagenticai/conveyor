//! Sources & retrieval — the capability-bound knowledge packs and the intake catalogue
//! (T18-5, #566; SPEC_V2 §10.5.2).
//!
//! Retrieval is `knowledge.query`: the capability's pinned packs are searched through the one
//! retrieval path, and the upstream's hits are returned unchanged — a required pack that is
//! missing is `409 knowledge_degraded`, an optional one is a warning on an answer, never a silent
//! empty set. The source-transformation side is the T17 asset catalogue (`authoring.assets`):
//! every SOP/RDF/OWL/BPMN intake becomes a draft behind the review gate, and the screen names T16
//! for the ingestion itself rather than inventing an inspector the backend does not expose.

use leptos::prelude::*;
use serde_json::Value;

use crate::api::{ApiError, InvokeOptions};
use crate::components::{Empty, ErrorPanel, Planned, StatusPill};
use crate::screens::v2_data::{read_rows, text};
use crate::state::AppCtx;

#[component]
pub fn SourcesScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let refresh = RwSignal::new(0_u32);
    let (capability, set_capability) = signal(String::new());
    let (question, set_question) = signal(String::new());
    let (busy, set_busy) = signal(false);
    let (result, set_result) = signal(Option::<Result<Value, ApiError>>::None);

    let capabilities = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let _ = refresh.get();
        let client = ctx.client();
        async move {
            if !ctx.has_action("capabilities.list") {
                return Ok(Vec::new());
            }
            read_rows(
                &client,
                "capabilities.list",
                serde_json::json!({ "limit": 100 }),
            )
            .await
        }
    });

    let assets = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let _ = refresh.get();
        let client = ctx.client();
        async move {
            if !ctx.has_action("authoring.assets") {
                return Ok(Vec::new());
            }
            read_rows(&client, "authoring.assets", serde_json::json!({})).await
        }
    });

    let retry = Callback::new(move |_| refresh.update(|n| *n += 1));

    let ask = move |_| {
        let capability = capability.get_untracked();
        let question = question.get_untracked();
        let client = ctx.client();
        set_busy.set(true);
        leptos::task::spawn_local(async move {
            let outcome = client
                .invoke(
                    "knowledge.query",
                    serde_json::json!({ "capability_id": capability, "query": question }),
                    InvokeOptions::default(),
                )
                .await
                .map(|invocation| invocation.output);
            set_result.set(Some(outcome));
            set_busy.set(false);
        });
    };

    view! {
        <section class="screen" data-testid="sources-retrieval">
            <div class="screen-head">
                <h1>"Sources & retrieval"</h1>
                <p class="muted">
                    "Retrieval is capability-bound: the conveyor owns the binding, strata owns the
                    packs. A source becomes an asset only through T16's ingestion and T17's review
                    gate; the catalogue below is the drafts those gates produced."
                </p>
            </div>

            <div class="grid two">
                <div class="panel" data-testid="sources-retrieval-query">
                    <h2>"Ask a capability"</h2>
                    {move || {
                        if !ctx.has_action("knowledge.query") {
                            return view! {
                                <Planned title="Retrieval".to_owned() detail="knowledge.query is not in this role's manifest.".to_owned()/>
                            }.into_any();
                        }
                        view! {
                            <form class="toolbar" on:submit=move |event| {
                                event.prevent_default();
                                ask(());
                            }>
                                <label class="field">
                                    <span>"Capability"</span>
                                    <select on:change=move |event| set_capability.set(event_target_value(&event))>
                                        <option value="">"Choose a capability"</option>
                                        {move || match capabilities.get() {
                                            Some(Ok(rows)) => rows.into_iter().map(|row| {
                                                let id = text(&row, "id");
                                                view! {
                                                    <option value=id>{format!("{} · {}", text(&row, "key"), text(&row, "name"))}</option>
                                                }
                                            }).collect_view().into_any(),
                                            _ => view! { <option value="">"Reading capabilities…"</option> }.into_any(),
                                        }}
                                    </select>
                                </label>
                                <label class="field">
                                    <span>"Question"</span>
                                    <input
                                        type="text"
                                        placeholder="What does the delivery capability say about rollback?"
                                        prop:value=move || question.get()
                                        on:input=move |event| set_question.set(event_target_value(&event))
                                    />
                                </label>
                                <button class="btn" type="submit" disabled=move || busy.get()>
                                    "Ask the packs"
                                </button>
                            </form>
                            {move || match result.get() {
                                None => view! { <p class="muted">"An answer names the packs that contributed and every warning; an empty answer is a fact, not a silent success."</p> }.into_any(),
                                Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                                Some(Ok(body)) => {
                                    let packs = body.get("packs").and_then(Value::as_array).cloned().unwrap_or_default();
                                    let hits = body.get("hits").and_then(Value::as_array).cloned().unwrap_or_default();
                                    let warnings = body.get("warnings").and_then(Value::as_array).cloned().unwrap_or_default();
                                    let degraded = body.get("degraded").and_then(Value::as_bool).unwrap_or(false);
                                    view! {
                                        <p class="muted" data-testid="sources-packs">
                                            {format!("{} pack(s) · {} hit(s)", packs.len(), hits.len())}
                                            {if degraded { " · degraded" } else { "" }}
                                        </p>
                                        {(!warnings.is_empty()).then(|| view! {
                                            <ul class="muted">{warnings.into_iter().map(|warning| view! { <li>{warning.to_string()}</li> }).collect_view()}</ul>
                                        })}
                                        {if hits.is_empty() {
                                            view! { <Empty message="No hit: the packs answered with an empty set. That is a result, not an error.".to_owned()/> }.into_any()
                                        } else {
                                            view! {
                                                <table class="table">
                                                    <thead><tr><th>"Hit"</th></tr></thead>
                                                    <tbody>
                                                        {hits.into_iter().map(|hit| view! { <tr><td class="mono">{hit.to_string()}</td></tr> }).collect_view()}
                                                    </tbody>
                                                </table>
                                            }.into_any()
                                        }}
                                    }.into_any()
                                }
                            }}
                        }.into_any()
                    }}
                </div>

                <div class="panel" data-testid="sources-catalogue">
                    <h2>"The source catalogue"</h2>
                    {move || match assets.get() {
                        None => view! { <p class="muted">"Reading the intake catalogue…"</p> }.into_any(),
                        Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                        Some(Ok(rows)) if rows.is_empty() => view! {
                            <Empty message="No authored asset yet. An SOP/RDF/OWL/BPMN intake (T16) drafts one; T17's review publishes it.".to_owned()/>
                        }.into_any(),
                        Some(Ok(rows)) => view! {
                            <table class="table">
                                <thead><tr><th>"Key"</th><th>"Kind"</th><th>"Status"</th><th>"Intake"</th></tr></thead>
                                <tbody>
                                    {rows.into_iter().map(|row| view! {
                                        <tr>
                                            <td><strong>{text(&row, "key")}</strong></td>
                                            <td>{text(&row, "kind")}</td>
                                            <td><StatusPill status=text(&row, "status")/></td>
                                            <td class="mono">{short(&text(&row, "intake_id"))}</td>
                                        </tr>
                                    }).collect_view()}
                                </tbody>
                            </table>
                        }.into_any(),
                    }}
                    <p class="muted">
                        "The source transformation itself is T16's ingest path; the review and
                        publication gate is T17's authoring screen. This catalogue is the read side."
                    </p>
                </div>
            </div>
        </section>
    }
}

fn short(value: &str) -> String {
    if value == "—" || value.is_empty() {
        return "—".to_owned();
    }
    value.chars().take(8).collect()
}
