//! Memory and the lesson lifecycle — the hot rows a work package learned and the propose →
//! review → promote path to doctrine (T18-5, #566; SPEC_V2 §10.5.2).
//!
//! `memory.recall` returns the card's hot rows (the record) and, when asked, the semantic cold
//! hits (the derived digest). The lesson family exposes `lessons.propose|review|promote` only —
//! there is no `lessons.list` read in this build — so the screen shows the lesson each action
//! returns and says so; it never invents a list. A proposal with fewer than two independent
//! evidence refs is the server's `422 lesson_evidence_insufficient`, shown verbatim.

use leptos::prelude::*;
use serde_json::{Value, json};

use crate::api::{ApiError, InvokeOptions};
use crate::components::{Empty, ErrorPanel, StatusPill};
use crate::screens::v2_data::{int, text};
use crate::state::AppCtx;

#[component]
pub fn MemoryScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let (card, set_card) = signal(String::new());
    let (query, set_query) = signal(String::new());
    let (busy, set_busy) = signal(false);
    let (recall, set_recall) = signal(Option::<Result<Value, ApiError>>::None);
    let (notice, set_notice) = signal(Option::<String>::None);

    let (statement, set_statement) = signal(String::new());
    let (kind_one, set_kind_one) = signal("card".to_owned());
    let (ref_one, set_ref_one) = signal(String::new());
    let (kind_two, set_kind_two) = signal("run".to_owned());
    let (ref_two, set_ref_two) = signal(String::new());
    let (lesson_id, set_lesson_id) = signal(String::new());
    let (decision, set_decision) = signal("approve".to_owned());
    let (note, set_note) = signal(String::new());
    let (lesson, set_lesson) = signal(Option::<Result<Value, ApiError>>::None);

    let recall_fn = move |_| {
        let client = ctx.client();
        let input = json!({
            "card_id": card.get_untracked(),
            "query": query.get_untracked(),
        });
        set_busy.set(true);
        leptos::task::spawn_local(async move {
            let outcome = client
                .invoke("memory.recall", input, InvokeOptions::default())
                .await
                .map(|invocation| invocation.output);
            set_recall.set(Some(outcome));
            set_busy.set(false);
        });
    };

    let propose = move |_| {
        let client = ctx.client();
        let mut evidence = Vec::new();
        if !ref_one.get_untracked().trim().is_empty() {
            evidence
                .push(json!({ "kind": kind_one.get_untracked(), "id": ref_one.get_untracked() }));
        }
        if !ref_two.get_untracked().trim().is_empty() {
            evidence
                .push(json!({ "kind": kind_two.get_untracked(), "id": ref_two.get_untracked() }));
        }
        let input = json!({ "statement": statement.get_untracked(), "evidence": evidence });
        set_busy.set(true);
        leptos::task::spawn_local(async move {
            match client
                .invoke("lessons.propose", input, InvokeOptions::default())
                .await
            {
                Ok(invocation) => {
                    set_notice.set(None);
                    if let Some(id) = invocation.output.get("id").and_then(Value::as_str) {
                        set_lesson_id.set(id.to_owned());
                    }
                    set_lesson.set(Some(Ok(invocation.output)));
                }
                Err(error) => {
                    set_notice.set(Some(format!("Refused · {error}")));
                    set_lesson.set(Some(Err(error)));
                }
            }
            set_busy.set(false);
        });
    };

    let review = move |_| {
        let client = ctx.client();
        let input = json!({
            "lesson_id": lesson_id.get_untracked(),
            "decision": decision.get_untracked(),
            "note": note.get_untracked(),
        });
        set_busy.set(true);
        leptos::task::spawn_local(async move {
            match client
                .invoke("lessons.review", input, InvokeOptions::default())
                .await
            {
                Ok(invocation) => {
                    set_notice.set(None);
                    set_lesson.set(Some(Ok(invocation.output)));
                }
                Err(error) => {
                    set_notice.set(Some(format!("Refused · {error}")));
                    set_lesson.set(Some(Err(error)));
                }
            }
            set_busy.set(false);
        });
    };

    let promote = move |_| {
        let client = ctx.client();
        let input = json!({ "lesson_id": lesson_id.get_untracked() });
        set_busy.set(true);
        leptos::task::spawn_local(async move {
            match client
                .invoke("lessons.promote", input, InvokeOptions::default())
                .await
            {
                Ok(invocation) => {
                    set_notice.set(None);
                    set_lesson.set(Some(Ok(invocation.output)));
                }
                Err(error) => {
                    set_notice.set(Some(format!("Refused · {error}")));
                    set_lesson.set(Some(Err(error)));
                }
            }
            set_busy.set(false);
        });
    };

    let retry = Callback::new(move |_| {
        set_recall.set(None);
        set_lesson.set(None);
    });

    view! {
        <section class="screen" data-testid="memory-workbench">
            <div class="screen-head">
                <h1>"Memory"</h1>
                <p class="muted">
                    "A work package's hot rows are the record; the semantic digest is derived from
                    them, never a replacement. A lesson becomes doctrine only through two independent
                    evidence references and a person's review."
                </p>
            </div>

            {move || notice.get().map(|message| view! {
                <p class="panel note" data-testid="memory-notice">{message}</p>
            })}

            <div class="grid two">
                <div class="panel" data-testid="memory-recall">
                    <h2>"Recall a work package"</h2>
                    {move || {
                        if !ctx.has_action("memory.recall") {
                            return view! { <p class="muted">"memory.recall is not in this role's manifest."</p> }.into_any();
                        }
                        view! {
                            <form class="toolbar" on:submit=move |event| {
                                event.prevent_default();
                                recall_fn(());
                            }>
                                <label class="field">
                                    <span>"Card id"</span>
                                    <input type="text" placeholder="the work package (card) UUID"
                                        prop:value=move || card.get()
                                        on:input=move |event| set_card.set(event_target_value(&event))/>
                                </label>
                                <label class="field">
                                    <span>"Question (optional)"</span>
                                    <input type="text" placeholder="search the digest"
                                        prop:value=move || query.get()
                                        on:input=move |event| set_query.set(event_target_value(&event))/>
                                </label>
                                <button class="btn" type="submit" disabled=move || busy.get()>"Recall"</button>
                            </form>
                            {move || match recall.get() {
                                None => view! { <p class="muted">"Hot rows first; a cold hit can never replace one."</p> }.into_any(),
                                Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                                Some(Ok(body)) => {
                                    let hot = body.get("hot").and_then(Value::as_array).cloned().unwrap_or_default();
                                    let cold = body.get("cold").and_then(Value::as_array).cloned().unwrap_or_default();
                                    let degraded = body.get("degraded").and_then(Value::as_bool).unwrap_or(false);
                                    view! {
                                        <p class="muted" data-testid="memory-counts">
                                            {format!("{} hot row(s) · {} cold hit(s)", hot.len(), cold.len())}
                                            {if degraded { " · the semantic half is degraded" } else { "" }}
                                        </p>
                                        {if hot.is_empty() {
                                            view! { <Empty message="No hot row exists for this card: no run has recorded a fact, attempt, decision or blocker yet.".to_owned()/> }.into_any()
                                        } else {
                                            view! {
                                                <table class="table">
                                                    <thead><tr><th>"Kind"</th><th>"Source"</th><th>"Body"</th></tr></thead>
                                                    <tbody>
                                                        {hot.into_iter().map(|row| view! {
                                                            <tr>
                                                                <td><StatusPill status=text(&row, "kind")/></td>
                                                                <td>{text(&row, "source")}</td>
                                                                <td>{text(&row, "body")}</td>
                                                            </tr>
                                                        }).collect_view()}
                                                    </tbody>
                                                </table>
                                            }.into_any()
                                        }}
                                        {(!cold.is_empty()).then(|| view! {
                                            <h3>"Cold hits (the derived digest)"</h3>
                                            <ul>{cold.into_iter().map(|hit| view! { <li class="mono">{hit.to_string()}</li> }).collect_view()}</ul>
                                        })}
                                    }.into_any()
                                }
                            }}
                        }.into_any()
                    }}
                </div>

                <div class="panel" data-testid="memory-lessons">
                    <h2>"The lesson lifecycle"</h2>
                    <p class="muted">
                        "No lessons.list read exists in this build; the family exposes propose, review
                        and promote, so the screen shows the lesson each call returns (T8)."
                    </p>
                    <form class="toolbar" on:submit=move |event| {
                        event.prevent_default();
                        propose(());
                    }>
                        <label class="field">
                            <span>"Statement"</span>
                            <input type="text" placeholder="what the organisation should learn"
                                prop:value=move || statement.get()
                                on:input=move |event| set_statement.set(event_target_value(&event))/>
                        </label>
                        <label class="field">
                            <span>"Evidence 1"</span>
                            <select on:change=move |event| set_kind_one.set(event_target_value(&event))>
                                <option value="card">"card"</option>
                                <option value="run">"run"</option>
                            </select>
                            <input type="text" placeholder="card or run UUID"
                                prop:value=move || ref_one.get()
                                on:input=move |event| set_ref_one.set(event_target_value(&event))/>
                        </label>
                        <label class="field">
                            <span>"Evidence 2"</span>
                            <select on:change=move |event| set_kind_two.set(event_target_value(&event))>
                                <option value="run">"run"</option>
                                <option value="card">"card"</option>
                            </select>
                            <input type="text" placeholder="a different card or run UUID"
                                prop:value=move || ref_two.get()
                                on:input=move |event| set_ref_two.set(event_target_value(&event))/>
                        </label>
                        <button class="btn" type="submit" disabled=move || busy.get()>"Propose"</button>
                    </form>

                    <form class="toolbar" on:submit=move |event| {
                        event.prevent_default();
                        review(());
                    }>
                        <label class="field">
                            <span>"Lesson id"</span>
                            <input type="text" placeholder="the proposed lesson UUID"
                                prop:value=move || lesson_id.get()
                                on:input=move |event| set_lesson_id.set(event_target_value(&event))/>
                        </label>
                        <label class="field">
                            <span>"Decision"</span>
                            <select on:change=move |event| set_decision.set(event_target_value(&event))>
                                <option value="approve">"approve"</option>
                                <option value="reject">"reject"</option>
                            </select>
                        </label>
                        <label class="field">
                            <span>"Note"</span>
                            <input type="text" placeholder="the reviewer's note"
                                prop:value=move || note.get()
                                on:input=move |event| set_note.set(event_target_value(&event))/>
                        </label>
                        <button class="btn" type="submit" disabled=move || busy.get()>"Review"</button>
                        <button class="btn" type="button" disabled=move || busy.get() on:click=move |_| promote(())>
                            "Promote"
                        </button>
                    </form>

                    {move || match lesson.get() {
                        None => view! { <p class="muted">"The lifecycle is proposed → reviewed → promoted (or rejected)."</p> }.into_any(),
                        Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                        Some(Ok(body)) => view! {
                            <table class="table" data-testid="memory-lesson">
                                <thead><tr><th>"Id"</th><th>"State"</th><th>"Independent"</th><th>"Statement"</th></tr></thead>
                                <tbody>
                                    <tr>
                                        <td class="mono">{text(&body, "id")}</td>
                                        <td><StatusPill status=text(&body, "state")/></td>
                                        <td class="num">{int(&body, "independent_count")}</td>
                                        <td>{text(&body, "statement")}</td>
                                    </tr>
                                </tbody>
                            </table>
                        }.into_any(),
                    }}
                </div>
            </div>
        </section>
    }
}
