//! Trace / architecture — models, views and the rendered diagram (S12 screen 6).
//!
//! Two facts shape this screen. **The diagram is bytes, not JSON**: `GET
//! /api/models/{id}/render` answers an SVG document, so the screen fetches it with the
//! caller's credential and shows it through an object URL — an `<img src>` cannot carry an
//! `Authorization` header (ADR 0001, CONTRACT §6). **The graph is written through actions**:
//! `models.*`, `model_views.*` and `trace_links.*`, never SQL from the browser.
//!
//! The trace links are the asset/card edges. A model element names the governed asset it
//! represents in `meta.governed_asset_id`, and the panel lists every `trace_link` row that
//! touches one of those assets. `models.traceability` is the element-level coverage matrix;
//! it counts `model_element` edges, which no action writes today, so the matrix reports zeros
//! honestly rather than inventing a relation the API does not have.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use serde::Deserialize;
use serde_json::{Value, json};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, Request, RequestCredentials, RequestInit, RequestMode, Response};

use crate::api::{ApiError, Client, InvokeOptions, Page, problem};
use crate::components::{Empty, ErrorPanel, Planned, StatusPill, age, short_id};
use crate::state::AppCtx;

// ── the documents the actions return ─────────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize)]
struct ModelOut {
    id: String,
    #[serde(default)]
    project_id: Option<String>,
    language: String,
    language_version: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    head_revision: i64,
    #[serde(default)]
    published_revision_id: Option<String>,
    #[serde(default)]
    meta: Value,
    #[serde(default)]
    version: i64,
    #[serde(default)]
    updated_at: Option<DateTime<Utc>>,
    /// Populated by `models.get`, absent from `models.list`.
    #[serde(default)]
    latest_validation_run: Option<ValidationRun>,
}

#[derive(Clone, Debug, Deserialize)]
struct ValidationRun {
    #[serde(default)]
    id: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    ruleset: String,
    #[serde(default)]
    ruleset_version: String,
    #[serde(default)]
    content_hash: String,
    #[serde(default)]
    findings: Value,
    #[serde(default)]
    created_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct Finding {
    #[serde(default)]
    code: String,
    #[serde(default)]
    severity: String,
    #[serde(default)]
    pointer: String,
    #[serde(default)]
    message: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ViewOut {
    id: String,
    #[serde(default)]
    kind: String,
    name: String,
    #[serde(default)]
    version: i64,
}

#[derive(Clone, Debug, Deserialize)]
struct TraceabilityOut {
    #[serde(default)]
    elements: Vec<ElementTrace>,
    #[serde(default)]
    untraced: i64,
}

#[derive(Clone, Debug, Deserialize)]
struct ElementTrace {
    element_id: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    governed_asset_id: Option<String>,
    #[serde(default)]
    outgoing: i64,
    #[serde(default)]
    incoming: i64,
}

#[derive(Clone, Debug, Deserialize)]
struct TraceLinkOut {
    id: String,
    #[serde(default)]
    from_type: String,
    #[serde(default)]
    from_id: String,
    #[serde(default)]
    to_type: String,
    #[serde(default)]
    to_id: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    created_at: Option<DateTime<Utc>>,
}

#[derive(Clone)]
struct DetailData {
    model: ModelOut,
    views: Vec<ViewOut>,
    traceability: Option<TraceabilityOut>,
    links: Vec<TraceLinkOut>,
}

fn parse<T: serde::de::DeserializeOwned>(value: Value, what: &str) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Network(format!("{what}: {error}")))
}

fn findings_of(run: &ValidationRun) -> Vec<Finding> {
    serde_json::from_value(run.findings.clone()).unwrap_or_default()
}

// ── the rendered diagram ────────────────────────────────────────────────────────────────

fn js_error(value: JsValue) -> ApiError {
    ApiError::Network(value.as_string().unwrap_or_else(|| format!("{value:?}")))
}

/// Mint an object URL for the browser `Blob` the response produced.
///
/// This crate's `web-sys` feature set has no `Blob` binding (`Blob` is not in the enabled
/// list and the sibling crates do not turn it on), so the same global `URL.createObjectURL`
/// that `Url::create_object_url_with_blob` wraps is called through `js_sys`. The object URL
/// is the point: the browser decodes the SVG without the header problem an `<img src>` has.
fn object_url(blob: &JsValue) -> Result<String, ApiError> {
    let global = js_sys::global();
    let url = js_sys::Reflect::get(&global, &JsValue::from_str("URL")).map_err(js_error)?;
    let create =
        js_sys::Reflect::get(&url, &JsValue::from_str("createObjectURL")).map_err(js_error)?;
    let create: js_sys::Function = create
        .dyn_into()
        .map_err(|_| ApiError::Network("URL.createObjectURL is missing".into()))?;
    create
        .call1(&url, blob)
        .map_err(js_error)?
        .as_string()
        .ok_or_else(|| ApiError::Network("createObjectURL returned no string".into()))
}

/// `GET /api/models/{id}/render?view_id=…&format=svg`, as an object URL.
async fn render_svg(client: &Client, model_id: &str, view_id: &str) -> Result<String, ApiError> {
    let window = web_sys::window().ok_or_else(|| ApiError::Network("no window".into()))?;
    let headers = Headers::new().map_err(js_error)?;
    headers.set("accept", "image/svg+xml").map_err(js_error)?;
    if let Some(token) = &client.token {
        headers
            .set("authorization", &format!("Bearer {token}"))
            .map_err(js_error)?;
    }
    if let Some(tenant) = &client.tenant {
        headers.set("x-active-tenant", tenant).map_err(js_error)?;
    }

    let init = RequestInit::new();
    init.set_method("GET");
    init.set_headers(&headers);
    init.set_mode(RequestMode::SameOrigin);
    init.set_credentials(RequestCredentials::SameOrigin);

    let path = format!("/api/models/{model_id}/render?format=svg&view_id={view_id}");
    let request = Request::new_with_str_and_init(&path, &init).map_err(js_error)?;
    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(js_error)?;
    let response: Response = response
        .dyn_into()
        .map_err(|_| ApiError::Network("fetch did not return a Response".into()))?;
    if !(200..300).contains(&response.status()) {
        let text = JsFuture::from(response.text().map_err(js_error)?)
            .await
            .map_err(js_error)?;
        return Err(problem(
            response.status(),
            &text.as_string().unwrap_or_default(),
        ));
    }

    let blob = JsFuture::from(response.blob().map_err(js_error)?)
        .await
        .map_err(js_error)?;
    object_url(&blob)
}

#[component]
fn Diagram(model_id: String, selected: RwSignal<Option<String>>) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let diagram = LocalResource::new(move || {
        // Every reactive input is read before the first await.
        let model_id = model_id.clone();
        let view_id = selected.get();
        let token = ctx.token.get();
        let tenant = ctx.tenant.get();
        async move {
            let Some(view_id) = view_id.filter(|id| !id.is_empty()) else {
                return Err(ApiError::NotFound("no view to draw".to_owned()));
            };
            render_svg(&Client { token, tenant }, &model_id, &view_id).await
        }
    });

    // One object URL at a time: the previous blob is revoked when the view changes, so a
    // tenant browsing ten views does not pin ten decoded documents in memory.
    let shown = RwSignal::new(Option::<String>::None);
    Effect::new(move |_| {
        let next = match diagram.get() {
            Some(Ok(url)) => Some(url),
            _ => None,
        };
        let previous = shown.get_untracked();
        if previous != next {
            if let Some(old) = previous {
                let _ = web_sys::Url::revoke_object_url(&old);
            }
            shown.set(next);
        }
    });
    on_cleanup(move || {
        if let Some(url) = shown.get_untracked() {
            let _ = web_sys::Url::revoke_object_url(&url);
        }
    });

    view! {
        <section class="panel" aria-label="Diagram">
            <div class="ph">
                <span>"Diagram"</span>
                <span class="muted">"GET /api/models/{id}/render · svg"</span>
            </div>
            {move || match diagram.get() {
                None => view! { <p class="muted">"Drawing the view…"</p> }.into_any(),
                Some(Err(ApiError::NotFound(detail))) if detail == "no view to draw" => view! {
                    <Empty message="Select a view to draw.".to_owned()/>
                }.into_any(),
                Some(Err(error)) => view! {
                    <div class="callout bad" role="alert">
                        <div class="hd">"The diagram did not render"</div>
                        <p>{error.to_string()}</p>
                    </div>
                }.into_any(),
                Some(Ok(_)) => view! {
                    <img
                        alt="Rendered model"
                        src=move || shown.get().unwrap_or_default()
                        style="max-width:100%;height:auto;display:block;background:var(--surface);border:1px solid var(--rule);border-radius:6px"
                    />
                }.into_any(),
            }}
        </section>
    }
}

// ── the list ────────────────────────────────────────────────────────────────────────────

fn load_models(ctx: AppCtx) -> LocalResource<Result<Vec<ModelOut>, ApiError>> {
    LocalResource::new(move || {
        // Every reactive input is read before the first await.
        let _tenant = ctx.tenant.get();
        let client = ctx.client();
        async move {
            let page: Page<ModelOut> = parse(
                client
                    .invoke(
                        "models.list",
                        json!({"limit": 100}),
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                "models",
            )?;
            Ok(page.items)
        }
    })
}

#[component]
pub fn ModelsScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let data = load_models(ctx);
    let refresh = Callback::new(move |_| data.refetch());
    let notice = RwSignal::new(Option::<String>::None);

    view! {
        <section class="screen">
            {move || {
                if !ctx.has_action("models.list") {
                    return view! {
                        <Planned
                            title="Architecture is planned".to_owned()
                            detail="models.list is not in this role's manifest. The screen appears when the action lands, not before.".to_owned()
                        />
                    }
                    .into_any();
                }
                view! {
                    <header class="screen-head">
                        <div>
                            <h1>"Architecture"</h1>
                            <p class="muted">{move || match data.get() {
                                Some(Ok(models)) => format!(
                                    "{} model{} · one profile each, fixed at create",
                                    models.len(),
                                    if models.len() == 1 { "" } else { "s" }
                                ),
                                _ => "models, views and the rendered diagram".to_owned(),
                            }}</p>
                        </div>
                        <div class="filterbar">
                            <button class="btn" on:click=move |_| refresh.run(())>"Refresh"</button>
                        </div>
                    </header>
                    {move || notice.get().map(|message| view! {
                        <div class="callout warn" role="status"><p>{message}</p></div>
                    })}
                    {move || ctx.has_action("models.create").then(|| view! {
                        <CreateModel refresh=refresh notice=notice/>
                    })}
                    {move || match data.get() {
                        None => view! { <p class="muted">"Reading the models…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=refresh/>
                        }.into_any(),
                        Some(Ok(models)) if models.is_empty() => view! {
                            <Empty message="No model in this tenant yet.".to_owned()/>
                        }.into_any(),
                        Some(Ok(models)) => view! {
                            <section class="panel" aria-label="Models">
                                <div class="ph">
                                    <span>"Models"</span>
                                    <span class="muted">{format!("{} rows", models.len())}</span>
                                </div>
                                <div class="tablewrap">
                                <table class="table">
                                    <thead><tr>
                                        <th>"Model"</th><th>"Profile"</th>
                                        <th class="n">"Revision"</th><th class="n">"Version"</th>
                                        <th>"Updated"</th>
                                    </tr></thead>
                                    <tbody>
                                        {models.into_iter().map(|model| view! {
                                            <tr>
                                                <td>
                                                    <A href=format!("/design/models/{}", model.id)>
                                                        {model.title.clone()}
                                                    </A>
                                                    {(!model.description.is_empty()).then(|| view! {
                                                        <div class="muted">{model.description.clone()}</div>
                                                    })}
                                                </td>
                                                <td class="mono">
                                                    {format!("{} {}", model.language, model.language_version)}
                                                </td>
                                                <td class="n">{model.head_revision}</td>
                                                <td class="n">{model.version}</td>
                                                <td>{age(model.updated_at)}</td>
                                            </tr>
                                        }).collect_view()}
                                    </tbody>
                                </table>
                                </div>
                            </section>
                        }.into_any(),
                    }}
                    <p class="hint adv blk" style="margin-top:12px">
                        "models.list · models.create · the profile pair is archimate 3.2 or uml 2.5.1"
                    </p>
                }
                .into_any()
            }}
        </section>
    }
}

#[component]
fn CreateModel(refresh: Callback<()>, notice: RwSignal<Option<String>>) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let title = RwSignal::new(String::new());
    let language = RwSignal::new("archimate".to_owned());
    let language_version = RwSignal::new("3.2".to_owned());
    let busy = RwSignal::new(false);

    let on_language = move |event: web_sys::Event| {
        let value = event_target_value(&event);
        language_version.set(if value == "uml" { "2.5.1" } else { "3.2" }.to_owned());
        language.set(value);
    };

    let on_submit = move |event: web_sys::SubmitEvent| {
        event.prevent_default();
        let title_value = title.get_untracked().trim().to_owned();
        if title_value.is_empty() {
            notice.set(Some("A title is required.".into()));
            return;
        }
        let input = json!({
            "title": title_value,
            "language": language.get_untracked(),
            "language_version": language_version.get_untracked(),
        });
        let ctx = ctx;
        busy.set(true);
        leptos::task::spawn_local(async move {
            match ctx
                .invoke("models.create", input, InvokeOptions::default())
                .await
            {
                Ok(_) => {
                    title.set(String::new());
                    notice.set(Some("Model created as a draft graph.".into()));
                    refresh.run(());
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
            busy.set(false);
        });
    };

    view! {
        <section class="panel" aria-label="Create a model">
            <div class="ph">
                <span>"Create a model"</span>
                <span class="muted">"the profile is fixed for the model's lifetime"</span>
            </div>
            <form class="formrow" on:submit=on_submit>
                <label class="filter">
                    <span>"Title"</span>
                    <input
                        prop:value=move || title.get()
                        on:input=move |event| title.set(event_target_value(&event))
                        placeholder="Payments platform"
                    />
                </label>
                <label class="filter">
                    <span>"Language"</span>
                    <select on:change=on_language>
                        <option value="archimate" selected=move || language.get() == "archimate">"archimate"</option>
                        <option value="uml" selected=move || language.get() == "uml">"uml"</option>
                    </select>
                </label>
                <label class="filter">
                    <span>"Version"</span>
                    <select on:change=move |event| language_version.set(event_target_value(&event))>
                        <option value="3.2" selected=move || language_version.get() == "3.2">"3.2"</option>
                        <option value="2.5.1" selected=move || language_version.get() == "2.5.1">"2.5.1"</option>
                    </select>
                </label>
                <button class="btn pri" type="submit" disabled=move || busy.get()>
                    "Create model"
                </button>
            </form>
        </section>
    }
}

// ── the detail ──────────────────────────────────────────────────────────────────────────

fn load_detail<F>(ctx: AppCtx, model_id: F) -> LocalResource<Result<DetailData, ApiError>>
where
    F: Fn() -> String + 'static,
{
    LocalResource::new(move || {
        // Every reactive input is read before the first await.
        let id = model_id();
        let _tenant = ctx.tenant.get();
        let client = ctx.client();
        let has_traceability = ctx.has_action("models.traceability");
        let has_trace_links = ctx.has_action("trace_links.list");
        async move {
            if id.is_empty() {
                return Err(ApiError::NotFound("no model in the route".to_owned()));
            }
            let model: ModelOut = parse(
                client
                    .invoke("models.get", json!({"id": id}), InvokeOptions::default())
                    .await?
                    .output,
                "model",
            )?;
            let views: Page<ViewOut> = parse(
                client
                    .invoke(
                        "model_views.list",
                        json!({"model_id": id, "limit": 100}),
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                "model views",
            )?;
            let traceability = if has_traceability {
                Some(parse::<TraceabilityOut>(
                    client
                        .invoke(
                            "models.traceability",
                            json!({"model_id": id}),
                            InvokeOptions::default(),
                        )
                        .await?
                        .output,
                    "traceability",
                )?)
            } else {
                None
            };
            // The asset/card edges: only the governed assets this model's elements claim can
            // connect the model to the trace register. The list action issues no cursor, so
            // one bounded page is read and narrowed here.
            let assets: BTreeSet<String> = traceability
                .as_ref()
                .map(|report| {
                    report
                        .elements
                        .iter()
                        .filter_map(|element| element.governed_asset_id.clone())
                        .collect()
                })
                .unwrap_or_default();
            let links = if has_trace_links && !assets.is_empty() {
                let page: Page<TraceLinkOut> = parse(
                    client
                        .invoke(
                            "trace_links.list",
                            json!({"limit": 200}),
                            InvokeOptions::default(),
                        )
                        .await?
                        .output,
                    "trace links",
                )?;
                page.items
                    .into_iter()
                    .filter(|link| assets.contains(&link.from_id) || assets.contains(&link.to_id))
                    .collect()
            } else {
                Vec::new()
            };
            Ok(DetailData {
                model,
                views: views.items,
                traceability,
                links,
            })
        }
    })
}

#[component]
pub fn ModelDetail() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let params = use_params_map();
    let model_id = move || params.get().get("model_id").unwrap_or_default();
    let data = load_detail(ctx, model_id);
    let refresh = Callback::new(move |_| data.refetch());
    let notice = RwSignal::new(Option::<String>::None);

    view! {
        <section class="screen">
            {move || {
                if !ctx.has_action("models.get") {
                    return view! {
                        <Planned
                            title="Architecture is planned".to_owned()
                            detail="models.get is not in this role's manifest. The screen appears when the action lands, not before.".to_owned()
                        />
                    }
                    .into_any();
                }
                if model_id().is_empty() {
                    return view! { <p class="muted">"No model in the route."</p> }.into_any();
                }
                view! {
                    {move || notice.get().map(|message| view! {
                        <div class="callout warn" role="status"><p>{message}</p></div>
                    })}
                    {move || match data.get() {
                        None => view! { <p class="muted">"Reading the model…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=refresh/>
                        }.into_any(),
                        Some(Ok(body)) => view! {
                            <ModelBody data=body refresh=refresh notice=notice/>
                        }.into_any(),
                    }}
                }
                .into_any()
            }}
        </section>
    }
}

#[component]
fn ModelBody(
    data: DetailData,
    refresh: Callback<()>,
    notice: RwSignal<Option<String>>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let model = data.model.clone();
    let views = data.views.clone();
    let traceability = data.traceability.clone();
    let links = data.links.clone();
    let first_view = views.first().map(|view| view.id.clone());
    let selected = RwSignal::new(first_view.clone());
    let model_id = model.id.clone();

    let on_validate = {
        let model_id = model_id.clone();
        move |_| {
            let id = model_id.clone();
            let ctx = ctx;
            leptos::task::spawn_local(async move {
                match ctx
                    .invoke(
                        "models.validate",
                        json!({"id": id}),
                        InvokeOptions::default(),
                    )
                    .await
                {
                    Ok(invocation) => {
                        let queued = invocation
                            .output
                            .get("queued")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        notice.set(Some(if queued {
                            "Validation queued; the worker records the run. Refresh to read it."
                                .to_owned()
                        } else {
                            "An identical validation run is already live.".to_owned()
                        }));
                        refresh.run(());
                    }
                    Err(error) => notice.set(Some(error.to_string())),
                }
            });
        }
    };

    let run = model.latest_validation_run.clone();
    let findings = run.as_ref().map(findings_of).unwrap_or_default();
    let elements = traceability
        .as_ref()
        .map(|report| report.elements.clone())
        .unwrap_or_default();
    let untraced = traceability
        .as_ref()
        .map(|report| report.untraced)
        .unwrap_or(0);
    let default_asset = elements
        .iter()
        .find_map(|element| element.governed_asset_id.clone())
        .unwrap_or_default();
    let subtitle = format!(
        "{} {} · revision {} · version {}",
        model.language, model.language_version, model.head_revision, model.version
    );

    view! {
        <header class="screen-head">
            <div>
                <h1>{model.title.clone()}</h1>
                <p class="muted">{subtitle}</p>
                {(!model.description.is_empty()).then(|| view! {
                    <p class="screen-note">{model.description.clone()}</p>
                })}
            </div>
            <div class="filterbar">
                {ctx.has_action("models.validate").then(|| view! {
                    <button class="btn pri" on:click=on_validate>"Validate"</button>
                })}
                <button class="btn" on:click=move |_| refresh.run(())>"Refresh"</button>
            </div>
        </header>

        <div class="split">
            <div>
                {if views.is_empty() {
                    view! {
                        <section class="panel" aria-label="Diagram">
                            <div class="ph"><span>"Diagram"</span></div>
                            <Empty message="No view yet — generate one from the model's elements to draw it.".to_owned()/>
                        </section>
                    }.into_any()
                } else {
                    view! {
                        <section class="panel" aria-label="Views">
                            <div class="ph">
                                <span>"View"</span>
                                <span class="muted">{format!("{} view{}", views.len(), if views.len() == 1 { "" } else { "s" })}</span>
                            </div>
                            <label class="filter">
                                <span>"View"</span>
                                <select on:change=move |event| selected.set(Some(event_target_value(&event)))>
                                    {views.iter().map(|view| {
                                        let id = view.id.clone();
                                        let id_for_selected = id.clone();
                                        let label = format!("{} · {} · v{}", view.name, view.kind, view.version);
                                        view! {
                                            <option
                                                value=id
                                                selected=move || selected.get().as_deref() == Some(id_for_selected.as_str())
                                            >
                                                {label}
                                            </option>
                                        }
                                    }).collect_view()}
                                </select>
                            </label>
                        </section>
                        <div style="margin-top:12px">
                            <Diagram model_id=model_id.clone() selected=selected/>
                        </div>
                    }.into_any()
                }}

                <section class="panel" style="margin-top:12px" aria-label="Validation">
                    <div class="ph">
                        <span>"Validation"</span>
                        {match run.as_ref() {
                            Some(run) => view! {
                                <span class="muted">
                                    {format!("{} · {}", run.ruleset, run.ruleset_version)}
                                </span>
                            }.into_any(),
                            None => view! { <span class="muted">"not run"</span> }.into_any(),
                        }}
                    </div>
                    {match run.as_ref() {
                        None => view! {
                            <Empty message="No validation run yet — Validate queues one.".to_owned()/>
                        }.into_any(),
                        Some(run) => view! {
                            <div style="display:flex;gap:8px;align-items:center;flex-wrap:wrap;margin-bottom:8px">
                                <StatusPill status=run.status.clone()/>
                                {run.created_at.map(|at| view! {
                                    <span class="muted">{age(Some(at))}</span>
                                })}
                                <span class="adv muted mono" style="margin-left:auto">
                                    {format!("run {} · {}", short_id(Some(&run.id)), run.content_hash.chars().take(12).collect::<String>())}
                                </span>
                            </div>
                            {if findings.is_empty() {
                                view! {
                                    <Empty message="No findings — the graph matches its profile.".to_owned()/>
                                }.into_any()
                            } else {
                                view! {
                                    {findings.iter().map(|finding| {
                                        let tone = if finding.severity == "error" { "finding high" } else { "finding med" };
                                        let severity = if finding.severity.is_empty() { "finding".to_owned() } else { finding.severity.clone() };
                                        let pill = if finding.severity == "error" { "pill bad" } else { "pill wait" };
                                        view! {
                                            <div class=tone>
                                                <div class="fh">
                                                    <span class=pill>{severity}</span>
                                                    <span class="pill muted">{finding.code.clone()}</span>
                                                    <span class="muted mono">{finding.pointer.clone()}</span>
                                                </div>
                                                <p>{finding.message.clone()}</p>
                                            </div>
                                        }
                                    }).collect_view()}
                                }.into_any()
                            }}
                        }.into_any(),
                    }}
                </section>

                <p class="hint adv blk" style="margin-top:12px">
                    "models.get · model_views.list · models.validate · models.traceability"
                </p>
            </div>

            <div>
                <section class="panel" aria-label="Traceability matrix">
                    <div class="ph">
                        <span>"Traceability"</span>
                        <span class="muted">{format!("{untraced} untraced")}</span>
                    </div>
                    {if elements.is_empty() {
                        view! { <Empty message="This model holds no elements.".to_owned()/> }.into_any()
                    } else {
                        view! {
                            <div class="tablewrap">
                            <table class="wtbl matrix">
                                <thead><tr>
                                    <th>"Element"</th><th>"Kind"</th>
                                    <th class="n">"Out"</th><th class="n">"In"</th>
                                </tr></thead>
                                <tbody>
                                    {elements.into_iter().map(|element| {
                                        let tone = if element.outgoing + element.incoming > 0 { "dot" } else { "fixed" };
                                        view! {
                                            <tr>
                                                <td>
                                                    <span class=tone>"•"</span>
                                                    {" "}
                                                    {element.name.clone()}
                                                    <span class="adv mono" style="margin-left:6px">
                                                        {short_id(Some(&element.element_id))}
                                                    </span>
                                                </td>
                                                <td class="mono">{element.kind.clone()}</td>
                                                <td class="n">{element.outgoing}</td>
                                                <td class="n">{element.incoming}</td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                            </div>
                            <p class="muted" style="font-size:10.5px;margin-top:8px">
                                "Counts are trace_link rows whose end is this element id."
                            </p>
                        }.into_any()
                    }}
                </section>

                <section class="panel" style="margin-top:12px" aria-label="Trace links">
                    <div class="ph">
                        <span>"Trace links"</span>
                        <span class="muted">{links.len()}</span>
                    </div>
                    {if links.is_empty() {
                        view! {
                            <Empty message="No asset/card edge touches this model's governed assets.".to_owned()/>
                        }.into_any()
                    } else {
                        view! {
                            <div class="tablewrap">
                            <table class="wtbl">
                                <thead><tr>
                                    <th>"From"</th><th>"To"</th><th>"Kind"</th><th>"When"</th><th></th>
                                </tr></thead>
                                <tbody>
                                    {links.into_iter().map(|link| {
                                        let id = link.id.clone();
                                        view! {
                                            <tr>
                                                <td class="mono">
                                                    {format!("{}:{}", link.from_type, short_id(Some(&link.from_id)))}
                                                </td>
                                                <td class="mono">
                                                    {format!("{}:{}", link.to_type, short_id(Some(&link.to_id)))}
                                                </td>
                                                <td>{link.kind.clone()}</td>
                                                <td>{age(link.created_at)}</td>
                                                <td>
                                                    {ctx.has_action("trace_links.delete").then(|| view! {
                                                        <button class="btn small" on:click=move |_| {
                                                            let id = id.clone();
                                                            let ctx = ctx;
                                                            leptos::task::spawn_local(async move {
                                                                match ctx.invoke("trace_links.delete", json!({"id": id}), InvokeOptions::default()).await {
                                                                    Ok(_) => refresh.run(()),
                                                                    Err(error) => notice.set(Some(error.to_string())),
                                                                }
                                                            });
                                                        }>"Remove"</button>
                                                    })}
                                                </td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                            </div>
                        }.into_any()
                    }}
                    {move || ctx.has_action("trace_links.create").then(|| view! {
                        <LinkForm default_asset=default_asset.clone() refresh=refresh notice=notice/>
                    })}
                </section>

                <section class="panel" style="margin-top:12px" aria-label="Model">
                    <div class="ph"><span>"Model"</span></div>
                    <div class="kv">
                        <dt>"Language"</dt>
                        <dd class="mono">{format!("{} {}", model.language, model.language_version)}</dd>
                        <dt>"Project"</dt>
                        <dd class="mono">{model.project_id.clone().unwrap_or_else(|| "none".to_owned())}</dd>
                        <dt>"Published"</dt>
                        <dd class="mono">{model.published_revision_id.clone().map(|id| short_id(Some(&id))).unwrap_or_else(|| "not published".to_owned())}</dd>
                        <dt class="adv">"Model id"</dt><dd class="adv mono">{model.id.clone()}</dd>
                        <dt class="adv">"Meta"</dt><dd class="adv mono">{model.meta.to_string()}</dd>
                    </div>
                </section>
            </div>
        </div>
    }
}

#[component]
fn LinkForm(
    default_asset: String,
    refresh: Callback<()>,
    notice: RwSignal<Option<String>>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let from_type = RwSignal::new("governed_asset".to_owned());
    let from_id = RwSignal::new(default_asset);
    let to_type = RwSignal::new("card".to_owned());
    let to_id = RwSignal::new(String::new());
    let kind = RwSignal::new("traces".to_owned());
    let busy = RwSignal::new(false);

    let on_submit = move |event: web_sys::SubmitEvent| {
        event.prevent_default();
        let from_id_value = from_id.get_untracked().trim().to_owned();
        let to_id_value = to_id.get_untracked().trim().to_owned();
        if from_id_value.is_empty() || to_id_value.is_empty() {
            notice.set(Some("Both ends need an id.".into()));
            return;
        }
        let input = json!({
            "from_type": from_type.get_untracked(),
            "from_id": from_id_value,
            "to_type": to_type.get_untracked(),
            "to_id": to_id_value,
            "kind": kind.get_untracked(),
        });
        let ctx = ctx;
        busy.set(true);
        leptos::task::spawn_local(async move {
            match ctx
                .invoke("trace_links.create", input, InvokeOptions::default())
                .await
            {
                Ok(_) => {
                    to_id.set(String::new());
                    notice.set(Some("Trace link written.".into()));
                    refresh.run(());
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
            busy.set(false);
        });
    };

    view! {
        <form class="formrow" on:submit=on_submit style="margin-top:10px">
            <label class="filter">
                <span>"From type"</span>
                <select on:change=move |event| from_type.set(event_target_value(&event))>
                    <option value="governed_asset">"governed_asset"</option>
                    <option value="card">"card"</option>
                </select>
            </label>
            <label class="filter">
                <span>"From id"</span>
                <input
                    prop:value=move || from_id.get()
                    on:input=move |event| from_id.set(event_target_value(&event))
                    placeholder="00000000-0000-0000-0000-000000000000"
                />
            </label>
            <label class="filter">
                <span>"To type"</span>
                <select on:change=move |event| to_type.set(event_target_value(&event))>
                    <option value="card">"card"</option>
                    <option value="governed_asset">"governed_asset"</option>
                </select>
            </label>
            <label class="filter">
                <span>"To id"</span>
                <input
                    prop:value=move || to_id.get()
                    on:input=move |event| to_id.set(event_target_value(&event))
                    placeholder="00000000-0000-0000-0000-000000000000"
                />
            </label>
            <label class="filter">
                <span>"Kind"</span>
                <select on:change=move |event| kind.set(event_target_value(&event))>
                    <option value="traces">"traces"</option>
                    <option value="realises">"realises"</option>
                    <option value="derives">"derives"</option>
                    <option value="satisfies">"satisfies"</option>
                    <option value="conflicts">"conflicts"</option>
                </select>
            </label>
            <button class="btn pri" type="submit" disabled=move || busy.get()>
                "Add trace link"
            </button>
        </form>
    }
}

fn event_target_value(event: &web_sys::Event) -> String {
    event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|input| input.value())
        .or_else(|| {
            event
                .target()
                .and_then(|target| target.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
                .map(|area| area.value())
        })
        .or_else(|| {
            event
                .target()
                .and_then(|target| target.dyn_into::<web_sys::HtmlSelectElement>().ok())
                .map(|select| select.value())
        })
        .unwrap_or_default()
}
