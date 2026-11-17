//! The tenant's inference endpoint (conveyor#160 P2) — where this tenant's model calls go.
//!
//! The destination is ordinary configuration and is shown; the credential is write-only. Saving
//! with an empty credential leaves the sealed `inference.token` untouched, so a tenant can change
//! the address without re-entering the secret. The server enforces the role (`tenants:write`);
//! this screen only puts the form in front of it.

use leptos::prelude::*;
use serde::Deserialize;
use wasm_bindgen::JsCast;

use crate::api::{ApiError, InvokeOptions};
use crate::components::{ErrorPanel, Planned};
use crate::state::AppCtx;

#[derive(Clone, Debug, Default, Deserialize)]
struct EndpointState {
    #[serde(default)]
    destination_url: Option<String>,
    #[serde(default)]
    credential_present: bool,
}

fn load(ctx: AppCtx) -> LocalResource<Result<EndpointState, ApiError>> {
    LocalResource::new(move || async move {
        let _tenant = ctx.tenant.get();
        let output = ctx
            .client()
            .invoke(
                "inference_endpoint.get",
                serde_json::json!({}),
                InvokeOptions::default(),
            )
            .await?
            .output;
        serde_json::from_value(output)
            .map_err(|error| ApiError::Network(format!("inference endpoint: {error}")))
    })
}

fn input_value(event: &web_sys::Event) -> String {
    event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|input| input.value())
        .unwrap_or_default()
}

#[component]
pub fn InferenceEndpoint() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let data = load(ctx);
    let retry = Callback::new(move |_| data.refetch());
    let destination = RwSignal::new(String::new());
    let credential = RwSignal::new(String::new());
    let notice = RwSignal::new(Option::<String>::None);

    // Fill the destination once the row loads; the credential is never read back.
    Effect::new(move |_| {
        if let Some(Ok(state)) = data.get() {
            destination.set(state.destination_url.unwrap_or_default());
        }
    });

    let save = move |_| {
        let ctx = ctx;
        let url = destination.get_untracked().trim().to_owned();
        let secret = credential.get_untracked();
        leptos::task::spawn_local(async move {
            let mut input = serde_json::json!({ "destination_url": url });
            if !secret.trim().is_empty() {
                input["credential"] = serde_json::Value::String(secret);
            }
            match ctx
                .invoke("inference_endpoint.set", input, InvokeOptions::default())
                .await
            {
                Ok(_) => {
                    credential.set(String::new());
                    notice.set(Some(
                        "Saved. The next model call uses this destination.".into(),
                    ));
                    data.refetch();
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
        });
    };

    view! {
        <section class="screen">
            {move || {
                if !ctx.has_action("inference_endpoint.get") {
                    return view! {
                        <Planned
                            title="Inference endpoint is planned".to_owned()
                            detail="inference_endpoint.get is not in this role's manifest.".to_owned()
                        />
                    }
                    .into_any();
                }
                view! {
                    <header class="screen-head">
                        <div>
                            <h1>"Inference endpoint"</h1>
                            <p class="muted">
                                "Where this tenant's model calls go. The destination is ordinary configuration; the credential is sealed and never shown again."
                            </p>
                        </div>
                        <div class="filterbar">
                            <button class="btn" on:click=move |_| data.refetch()>"Refresh"</button>
                            <button class="btn primary" on:click=save>"Save"</button>
                        </div>
                    </header>
                    {move || notice.get().map(|message| view! {
                        <div class="panel notice" role="status"><p>{message}</p></div>
                    })}
                    {move || match data.get() {
                        None => view! { <p class="muted">"Loading the endpoint…"</p> }.into_any(),
                        Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                        Some(Ok(state)) => {
                            let credential_present = state.credential_present;
                            let current = state.destination_url.clone().unwrap_or_default();
                            view! {
                                <div class="panel stack">
                                    <p class="muted">
                                        {if credential_present {
                                            "A sealed credential is set."
                                        } else {
                                            "No credential is set; the service decides whether an anonymous call is acceptable."
                                        }}
                                    </p>
                                    <p class="muted mono">{current}</p>
                                    <div class="field">
                                        <label for="inference-destination">"Destination URL"</label>
                                        <input
                                            id="inference-destination"
                                            type="url"
                                            autocomplete="off"
                                            placeholder="https://router.example/v1"
                                            prop:value=move || destination.get()
                                            on:input=move |event| destination.set(input_value(&event))
                                        />
                                    </div>
                                    <div class="field">
                                        <label for="inference-credential">"Credential (optional)"</label>
                                        <input
                                            id="inference-credential"
                                            type="password"
                                            autocomplete="off"
                                            placeholder=move || if credential_present {
                                                "leave blank to keep the current credential"
                                            } else {
                                                "the router's bearer token"
                                            }
                                            prop:value=move || credential.get()
                                            on:input=move |event| credential.set(input_value(&event))
                                        />
                                    </div>
                                </div>
                            }.into_any()
                        }
                    }}
                }
                .into_any()
            }}
        </section>
    }
}
