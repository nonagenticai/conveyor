//! Sign in: the dev cookie locally, auth-code + PKCE in production. One gate, two modes.

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::auth::{self, AuthModes};
use crate::state::AppCtx;

#[component]
pub fn SignIn() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let login = RwSignal::new("alice".to_owned());
    let error = RwSignal::new(None::<String>);
    let modes = move || AuthModes::from_config(&ctx.config.get());

    let submit = move |_| {
        let value = login.get_untracked();
        leptos::task::spawn_local(async move {
            if let Err(failure) = auth::dev_login(&value).await {
                error.set(Some(failure.to_string()));
            }
        });
    };
    let start_oidc = move |_| {
        let Some(config) = ctx.config.get_untracked() else {
            return;
        };
        if let Err(failure) = auth::begin_login(&config) {
            error.set(Some(failure.to_string()));
        }
    };
    let on_input = move |event: web_sys::Event| {
        let value = event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
            .map(|input| input.value())
            .unwrap_or_default();
        login.set(value);
    };

    view! {
        <main class="center">
            <div class="panel signin">
                <div class="brand">
                    <span class="mark">"C"</span>
                    <span>"Conveyor"</span>
                </div>
                <h1>"Sign in"</h1>
                <p class="muted">"Conveyor uses your organisation identity."</p>
                {move || {
                    let modes = modes();
                    view! {
                        <div class="stack">
                            {modes.dev.then(|| view! {
                                <div class="field">
                                    <label for="dev-login">"Development login"</label>
                                    <input
                                        id="dev-login"
                                        type="text"
                                        autocomplete="username"
                                        prop:value=move || login.get()
                                        on:input=on_input
                                    />
                                    <button class="btn primary" on:click=submit>"Sign in (dev)"</button>
                                </div>
                            })}
                            {modes.oidc.then(|| view! {
                                <div class="stack">
                                    <button class="btn primary" on:click=start_oidc>"Continue with Keycloak"</button>
                                    {modes.issuer.clone().map(|issuer| view! {
                                        <p class="muted mono">{issuer}</p>
                                    })}
                                </div>
                            })}
                            {(!modes.dev && !modes.oidc).then(|| view! {
                                <p class="error">"No sign-in method is configured on this server."</p>
                            })}
                        </div>
                    }
                }}
                {move || error.get().map(|failure| view! {
                    <p class="error" role="alert">{failure}</p>
                })}
            </div>
        </main>
    }
}
