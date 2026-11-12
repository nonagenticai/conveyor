//! Copilot — a conversation, its turns and the proposal a turn may raise (S12 screen 10).
//!
//! The chat transport is a route, not an action (`POST /api/copilot/conversations`,
//! `POST …/{id}/turns`, `GET …/turns`); the proposal a turn ends in is an ordinary `approval`
//! row, so it is decided with `approvals.decide` — the same primitive every other channel
//! answers. The screen invents no second action system.
//!
//! The model is configuration. An unconfigured inference provider is shown as that failure,
//! never as an empty answer.

use leptos::prelude::*;
use serde::Deserialize;
use serde_json::{Value, json};
use wasm_bindgen::JsCast;

use crate::api::{ApiError, InvokeOptions};
use crate::components::{Empty, ErrorPanel, age, short_id};
use crate::sse::{Frame, Subscription, subscribe};
use crate::state::AppCtx;

#[derive(Clone, Debug, Deserialize)]
struct Turn {
    id: String,
    seq: i32,
    role: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    created_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Clone, Debug, Deserialize)]
struct TurnPage {
    #[serde(default)]
    items: Vec<Turn>,
}

#[derive(Clone, Debug, Deserialize)]
struct CopilotAction {
    name: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ActionSurface {
    #[serde(default)]
    actions: Vec<CopilotAction>,
}

#[derive(Clone)]
struct Chat {
    turns: Vec<Turn>,
    surface: Vec<String>,
}

fn parse<T: serde::de::DeserializeOwned>(value: Value, what: &str) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Network(format!("{what}: {error}")))
}

fn load(
    ctx: AppCtx,
    conversation: RwSignal<Option<String>>,
) -> LocalResource<Result<Chat, ApiError>> {
    LocalResource::new(move || {
        // Every reactive input is read before the first await.
        let _tenant = ctx.tenant.get();
        let id = conversation.get();
        let client = ctx.client();
        async move {
            let surface: ActionSurface =
                parse(client.get("/api/copilot/actions").await?, "copilot actions")?;
            let mut names: Vec<String> = surface
                .actions
                .into_iter()
                .map(|action| action.name)
                .collect();
            names.sort();
            let turns = match id {
                Some(id) => {
                    let page: TurnPage = parse(
                        client
                            .get(&format!("/api/copilot/conversations/{id}/turns?limit=100"))
                            .await?,
                        "turns",
                    )?;
                    page.items
                }
                None => Vec::new(),
            };
            Ok(Chat {
                turns,
                surface: names,
            })
        }
    })
}

#[component]
pub fn CopilotScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let conversation = RwSignal::new(Option::<String>::None);
    let data = load(ctx, conversation);
    let retry = Callback::new(move |_| data.refetch());
    let notice = RwSignal::new(Option::<String>::None);
    let draft = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let proposal = RwSignal::new(Option::<String>::None);
    let subscription = RwSignal::new(Option::<Subscription>::None);
    let subscribed_for = RwSignal::new(Option::<String>::None);

    // The declared live topic. A turn's terminal row and a proposal both land on it.
    Effect::new(move |_| {
        let id = conversation.get();
        if subscribed_for.get_untracked() == id {
            return;
        }
        subscription.with_untracked(|current| {
            if let Some(sub) = current {
                sub.abort();
            }
        });
        subscribed_for.set(id.clone());
        let Some(id) = id else {
            subscription.set(None);
            return;
        };
        let client = ctx.client();
        match subscribe(
            client,
            format!("conversation:{id}"),
            None,
            move |frame: Result<Frame, ApiError>| {
                if frame.is_ok() {
                    data.refetch();
                }
            },
        ) {
            Ok(sub) => subscription.set(Some(sub)),
            Err(error) => notice.set(Some(error.to_string())),
        }
    });

    on_cleanup(move || {
        subscription.with_untracked(|current| {
            if let Some(sub) = current {
                sub.abort();
            }
        });
    });

    let start = Callback::new(move |_: ()| {
        let ctx = ctx;
        busy.set(true);
        leptos::task::spawn_local(async move {
            match ctx
                .client()
                .post(
                    "/api/copilot/conversations",
                    json!({"title": format!("Conversation {}", chrono::Utc::now().format("%H:%M"))}),
                )
                .await
            {
                Ok(value) => match value.get("id").and_then(Value::as_str) {
                    Some(id) => {
                        conversation.set(Some(id.to_owned()));
                        proposal.set(None);
                        notice.set(None);
                    }
                    None => notice.set(Some("the conversation response carried no id".into())),
                },
                Err(error) => notice.set(Some(error.to_string())),
            }
            busy.set(false);
        });
    });

    let send = Callback::new(move |_: ()| {
        let Some(id) = conversation.get_untracked() else {
            notice.set(Some("Start a conversation first.".into()));
            return;
        };
        let body = draft.get_untracked().trim().to_owned();
        if body.is_empty() {
            notice.set(Some("Write a message first.".into()));
            return;
        }
        let ctx = ctx;
        busy.set(true);
        leptos::task::spawn_local(async move {
            match ctx
                .client()
                .post(
                    &format!("/api/copilot/conversations/{id}/turns"),
                    json!({"body": body}),
                )
                .await
            {
                Ok(value) => {
                    draft.set(String::new());
                    proposal.set(
                        value
                            .get("proposal_id")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                    );
                    data.refetch();
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
            busy.set(false);
        });
    });

    let decide = Callback::new(move |(id, decision): (String, &'static str)| {
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "approvals.decide",
                    json!({"approval_id": id, "decision": decision}),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => {
                    proposal.set(None);
                    data.refetch();
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    });

    view! {
        <section class="screen">
            {move || {
                // The chat transport is a route, not an action (S10's declared exemption):
                // there is no `copilot.chat` in the manifest to gate on. A caller who cannot
                // open a conversation gets the route's own 401/403 in the error panel.
                view! {
                    <header class="screen-head" style="flex-wrap:wrap">
                        <div>
                            <h1>"Copilot"</h1>
                            <p class="muted">"A conversation over the same action registry the board uses; a proposal is an approval, decided by the same primitive."</p>
                        </div>
                        <div class="filterbar">
                            <button class="btn primary" on:click=move |_| start.run(()) disabled=move || busy.get()>
                                {move || if conversation.get().is_some() { "New conversation" } else { "Start a conversation" }}
                            </button>
                            <button class="btn" on:click=move |_| data.refetch()>"Refresh"</button>
                        </div>
                    </header>
                    {move || notice.get().map(|message| view! {
                        <div class="callout warn" role="status"><p>{message}</p></div>
                    })}
                    {move || match data.get() {
                        None => view! { <p class="muted">"Reading the copilot surface…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=retry/>
                        }.into_any(),
                        Some(Ok(chat)) => view! {
                            <ChatBody chat=chat draft=draft busy=busy send=send
                                proposal=proposal decide=decide
                                on_start=Callback::new(move |_| start.run(()))
                                conversation=conversation/>
                        }.into_any(),
                    }}
                }
                .into_any()
            }}
        </section>
    }
}

#[component]
fn ChatBody(
    chat: Chat,
    draft: RwSignal<String>,
    busy: RwSignal<bool>,
    send: Callback<()>,
    proposal: RwSignal<Option<String>>,
    decide: Callback<(String, &'static str)>,
    on_start: Callback<()>,
    conversation: RwSignal<Option<String>>,
) -> impl IntoView {
    let turns = chat.turns.clone();
    let surface = chat.surface.clone();

    view! {
        <div class="split">
            <div>
                {move || match conversation.get() {
                    None => view! {
                        <div class="panel" role="status">
                            <h2>"No conversation open"</h2>
                            <p class="muted">"Start one; the copilot answers through the same action registry, and anything that needs a decision becomes an approval."</p>
                            <button class="btn primary" on:click=move |_| on_start.run(())>"Start a conversation"</button>
                        </div>
                    }.into_any(),
                    Some(id) => view! {
                        <section class="panel">
                            <div class="ph">
                                <span>"Conversation"</span>
                                <span class="muted mono">{short_id(Some(&id))}</span>
                            </div>
                            {if turns.is_empty() {
                                view! { <Empty message="No turn yet. Ask for something.".to_owned()/> }.into_any()
                            } else {
                                view! {
                                    <div class="turns">
                                        {turns.clone().into_iter().map(|turn| {
                                            let mine = turn.role == "user";
                                            view! {
                                                <div class=if mine { "turn user" } else { "turn assistant" }>
                                                    <div class="th">
                                                        <span class="pill muted">{turn.role.clone()}</span>
                                                        <span class="muted">{age(turn.created_at)}</span>
                                                    </div>
                                                    <p>{turn.body.clone()}</p>
                                                    <span class="adv muted mono">
                                                        {format!("turn {} · {}", turn.seq, short_id(Some(&turn.id)))}
                                                    </span>
                                                </div>
                                            }
                                        }).collect_view()}
                                    </div>
                                }.into_any()
                            }}
                            <form class="composer" on:submit=move |event| {
                                event.prevent_default();
                                send.run(());
                            }>
                                <textarea
                                    rows="3"
                                    prop:value=move || draft.get()
                                    on:input=move |event| draft.set(event_target_value(&event))
                                    placeholder="Ask the copilot to do something in this tenant."
                                ></textarea>
                                <button class="btn primary" type="submit" disabled=move || busy.get()>
                                    {move || if busy.get() { "Thinking…" } else { "Send" }}
                                </button>
                            </form>
                        </section>
                        {move || proposal.get().map(|id| view! {
                            <section class="panel" style="margin-top:12px">
                                <div class="ph"><span>"Proposal"</span><span class="muted mono">{short_id(Some(&id))}</span></div>
                                <p class="muted">
                                    "The turn stopped at a proposal. Deciding it is an ordinary approval; approve to execute it, reject to leave the tenant unchanged."
                                </p>
                                <div style="display:flex;gap:8px">
                                    {{
                                        let approve = id.clone();
                                        let reject = id.clone();
                                        view! {
                                            <button class="btn primary" on:click=move |_| decide.run((approve.clone(), "approve"))>"Approve and run"</button>
                                            <button class="btn" on:click=move |_| decide.run((reject.clone(), "reject"))>"Reject"</button>
                                        }
                                    }}
                                </div>
                            </section>
                        })}
                    }.into_any(),
                }}
            </div>
            <div>
                <section class="panel">
                    <div class="ph"><span>"Surface"</span><span class="muted">{surface.len()}</span></div>
                    <p class="muted">"What the copilot may propose in this tenant; the allowlist is the authorization."</p>
                    <div class="chips">
                        {surface.into_iter().map(|name| view! {
                            <span class="chip mono">{name}</span>
                        }).collect_view()}
                    </div>
                </section>
            </div>
        </div>
    }
}

fn event_target_value(event: &web_sys::Event) -> String {
    event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
        .map(|area| area.value())
        .unwrap_or_default()
}
