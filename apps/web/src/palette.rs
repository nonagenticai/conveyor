//! The ⌘K palette: screens, objects and actions in one list.
//!
//! The action list is the same role-filtered manifest the server serves, so an action this
//! role cannot invoke is never offered. Picking an action opens its raw JSON payload (the
//! form view is the next layer); anything a screen does not expose is one keystroke away.

use leptos::prelude::*;
use leptos_router::components::A;
use serde_json::Value;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

use crate::api::InvokeOptions;
use crate::state::AppCtx;

const SCREENS: &[(&str, &str)] = &[
    ("Today", "/"),
    ("Board", "/boards"),
    ("Runs", "/runs"),
    ("Members", "/settings/members"),
    ("Roles & permissions", "/settings/roles"),
];

#[component]
pub fn Palette() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let query = RwSignal::new(String::new());
    let selected = RwSignal::new(None::<String>);
    let payload = RwSignal::new("{}".to_owned());
    let result = RwSignal::new(None::<String>);
    let input_ref = NodeRef::<leptos::html::Input>::new();

    let keydown =
        Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |event: web_sys::KeyboardEvent| {
            if (event.meta_key() || event.ctrl_key()) && event.key().eq_ignore_ascii_case("k") {
                event.prevent_default();
                ctx.palette_open.update(|open| *open = !*open);
            } else if event.key() == "Escape" {
                ctx.palette_open.set(false);
            }
        });
    if let Some(window) = web_sys::window() {
        let _ =
            window.add_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref());
    }
    // The palette is mounted once for the app's lifetime; the listener is deliberately
    // forgotten rather than moved into `on_cleanup` (which requires Send + Sync, and a
    // `Closure` over a DOM callback is neither).
    keydown.forget();

    Effect::new(move |_| {
        if ctx.palette_open.get() {
            if let Some(input) = input_ref.get() {
                let _ = input.focus();
            }
        } else {
            selected.set(None);
            query.set(String::new());
            result.set(None);
        }
    });

    let on_query = move |event: web_sys::Event| {
        query.set(input_value(&event));
    };
    let on_payload = move |event: web_sys::Event| {
        payload.set(textarea_value(&event));
    };
    let close = move |_| ctx.palette_open.set(false);

    view! {
        {move || ctx.palette_open.get().then(|| {
            let filter = query.get().to_lowercase();
            let screens: Vec<(&str, &str)> = SCREENS
                .iter()
                .copied()
                .filter(|(name, _)| filter.is_empty() || name.to_lowercase().contains(&filter))
                .collect();
            let actions: Vec<String> = ctx
                .action_names()
                .into_iter()
                .filter(|name| filter.is_empty() || name.to_lowercase().contains(&filter))
                .take(24)
                .collect();
            view! {
                <div class="palette-backdrop" on:click=close>
                    <div class="palette" on:click=move |event| event.stop_propagation()>
                        <input
                            node_ref=input_ref
                            class="palette-input"
                            type="text"
                            placeholder="Search screens and actions…"
                            prop:value=move || query.get()
                            on:input=on_query
                        />
                        {move || match selected.get() {
                            None => view! {
                                <div class="palette-list">
                                    <h3>"Screens"</h3>
                                    {screens.iter().copied().map(|(name, path)| view! {
                                        <A href=path>
                                            <span
                                                class="palette-item"
                                                on:click=move |_| ctx.palette_open.set(false)
                                            >
                                                <span>{name}</span>
                                                <span class="muted mono">{path}</span>
                                            </span>
                                        </A>
                                    }).collect_view()}
                                    <h3>"Actions · the served manifest"</h3>
                                    {actions.iter().cloned().map(|name| {
                                        let selected_name = name.clone();
                                        view! {
                                            <button
                                                class="palette-item"
                                                on:click=move |_| {
                                                    selected.set(Some(selected_name.clone()));
                                                    payload.set("{}".to_owned());
                                                    result.set(None);
                                                }
                                            >
                                                <span class="mono">{name}</span>
                                            </button>
                                        }
                                    }).collect_view()}
                                </div>
                            }.into_any(),
                            Some(name) => view! {
                                <div class="palette-action">
                                    <h3>{name.clone()}</h3>
                                    <p class="muted">"The payload is the action's `input`. The manifest is the contract; the schema is one `actions.describe` away."</p>
                                    <textarea prop:value=move || payload.get() on:input=on_payload></textarea>
                                    <div class="palette-actions">
                                        <button class="btn" on:click=move |_| selected.set(None)>"Back"</button>
                                        <button class="btn primary" on:click=move |_| {
                                            let action = name.clone();
                                            let raw = payload.get_untracked();
                                            let input: Value = match serde_json::from_str(&raw) {
                                                Ok(input) => input,
                                                Err(error) => {
                                                    result.set(Some(format!("invalid JSON: {error}")));
                                                    return;
                                                }
                                            };
                                            result.set(Some("invoking…".to_owned()));
                                            let ctx = ctx;
                                            leptos::task::spawn_local(async move {
                                                match ctx.invoke(&action, input, InvokeOptions::default()).await {
                                                    Ok(invocation) => result.set(Some(format!(
                                                        "ok · request {}",
                                                        invocation.request_id.unwrap_or_else(|| "—".to_owned())
                                                    ))),
                                                    Err(error) => result.set(Some(error.to_string())),
                                                }
                                            });
                                        }>"Invoke"</button>
                                    </div>
                                    {move || result.get().map(|message| view! {
                                        <p class="palette-result mono" role="status">{message}</p>
                                    })}
                                </div>
                            }.into_any(),
                        }}
                    </div>
                </div>
            }
        })}
    }
}

fn input_value(event: &web_sys::Event) -> String {
    event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|input| input.value())
        .unwrap_or_default()
}

fn textarea_value(event: &web_sys::Event) -> String {
    event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
        .map(|input| input.value())
        .unwrap_or_default()
}
