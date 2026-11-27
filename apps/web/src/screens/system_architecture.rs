//! System architecture — the ArchiMate 3.2 models, their generated views, the traceability
//! matrix and the app's own UI map (T18-5, #566; SPEC_V2 §10.5.2).
//!
//! The map is the tenant's real graph: `models.list` names the models, `model_views.list` their
//! views, `models.traceability` the element-level coverage, and the SVG comes from
//! `GET /api/models/{id}/render` through the same object-URL path the trace screen uses (an
//! `<img src>` cannot carry the bearer). The UI map is the build's own route table, not tenant
//! data — a static, reviewable description of the screens this binary serves.
//!
//! A tenant with no model says so; nothing is drawn from a fixture.

use leptos::prelude::*;
use serde_json::json;

use crate::components::{Empty, ErrorPanel, Planned};
use crate::screens::models::Diagram;
use crate::screens::v2_data::{int, read_rows, text};
use crate::state::AppCtx;

/// The UI half of the map: every destination this shell serves, its route and the action family
/// it reads. This is the binary's own structure (the same table `app.rs` routes), not a fixture
/// row and not a tenant record.
const UI_MAP: [(&str, &str, &str); 14] = [
    ("Today", "/", "cards · runs"),
    (
        "Portfolio",
        "/portfolio",
        "portfolio.in_flight · programmes",
    ),
    ("Programme", "/programme", "programmes.list · benefits"),
    (
        "Organisation",
        "/organisation",
        "industries · departments · job_catalogue",
    ),
    ("Tools", "/tools", "tools · toolkits · tool_bindings"),
    ("Procedures", "/procedures", "procedures · capabilities"),
    ("Authoring", "/authoring", "authoring.assets"),
    ("Architecture capability", "/design/adm", "architecture.*"),
    (
        "System architecture",
        "/architecture",
        "models.* · model_views.* · trace_links.*",
    ),
    (
        "Knowledge design",
        "/knowledge",
        "job_releases.list · departments.list",
    ),
    (
        "Sources & retrieval",
        "/knowledge/sources",
        "knowledge.query · authoring.assets",
    ),
    ("Memory", "/knowledge/memory", "memory.recall · lessons.*"),
    ("Measure", "/measure", "metrics.usage · evals · benchmarks"),
    ("Operations", "/operations", "ops.*"),
];

#[component]
pub fn SystemArchitectureScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let refresh = RwSignal::new(0_u32);
    let selected_model = RwSignal::new(Option::<String>::None);
    let selected_view = RwSignal::new(Option::<String>::None);

    let models = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let _ = refresh.get();
        let client = ctx.client();
        async move {
            if !ctx.has_action("models.list") {
                return Ok(Vec::new());
            }
            read_rows(&client, "models.list", json!({ "limit": 100 })).await
        }
    });

    // Default to the first model once the list arrives; a selected model that disappeared falls
    // back to the first too, so the map never points at a row the tenant cannot see.
    Effect::new(move |_| {
        let Some(Ok(rows)) = models.get() else {
            return;
        };
        let current = selected_model.get_untracked();
        let still_there = current
            .as_deref()
            .is_some_and(|id| rows.iter().any(|row| text(row, "id") == id));
        if !still_there {
            selected_model.set(rows.first().map(|row| text(row, "id")));
        }
    });

    let views = LocalResource::new(move || {
        let model = selected_model.get();
        let _tenant = ctx.tenant.get();
        let _ = refresh.get();
        let client = ctx.client();
        async move {
            let Some(model) = model else {
                return Ok(Vec::new());
            };
            if !ctx.has_action("model_views.list") {
                return Ok(Vec::new());
            }
            read_rows(
                &client,
                "model_views.list",
                json!({ "model_id": model, "limit": 100 }),
            )
            .await
        }
    });

    // Default the drawn view to the first view of the selected model.
    Effect::new(move |_| {
        let Some(Ok(rows)) = views.get() else {
            return;
        };
        let current = selected_view.get_untracked();
        let still_there = current
            .as_deref()
            .is_some_and(|id| rows.iter().any(|row| text(row, "id") == id));
        if !still_there {
            selected_view.set(rows.first().map(|row| text(row, "id")));
        }
    });

    let traceability = LocalResource::new(move || {
        let model = selected_model.get();
        let _tenant = ctx.tenant.get();
        let _ = refresh.get();
        let client = ctx.client();
        async move {
            let Some(model) = model else {
                return Ok(None);
            };
            if !ctx.has_action("models.traceability") {
                return Ok(None);
            }
            let invocation = client
                .invoke(
                    "models.traceability",
                    json!({ "model_id": model }),
                    crate::api::InvokeOptions::default(),
                )
                .await?;
            Ok(Some(invocation.output))
        }
    });

    let generate = Callback::new(move |view_id: String| {
        let client = ctx.client();
        leptos::task::spawn_local(async move {
            let result = client
                .invoke(
                    "model_views.generate",
                    json!({ "view_id": view_id }),
                    crate::api::InvokeOptions::default(),
                )
                .await;
            match result {
                Ok(_) => ctx.toast.set(Some(
                    "The view was laid out from the model's elements.".into(),
                )),
                Err(error) => ctx.toast.set(Some(format!("Refused · {error}"))),
            }
            refresh.update(|n| *n += 1);
        });
    });

    let retry = Callback::new(move |_| refresh.update(|n| *n += 1));

    view! {
        <section class="screen" data-testid="system-architecture">
            <div class="screen-head">
                <h1>"System architecture"</h1>
                <p class="muted">
                    "The tenant's architecture models as ArchiMate 3.2 graphs: the models, their
                    generated views and rendered diagrams, the element-level traceability matrix,
                    and the UI map this binary serves."
                </p>
            </div>

            {move || {
                if !ctx.has_action("models.list") {
                    view! {
                        <Planned
                            title="System architecture".to_owned()
                            detail="models.list is not in this role's manifest. The map appears when the action lands, not before.".to_owned()
                        />
                    }.into_any()
                } else {
                    view! {
                        <div class="grid two">
                            <div class="panel" data-testid="system-models">
                                <h2>"Models"</h2>
                                {move || match models.get() {
                                    None => view! { <p class="muted">"Reading models…"</p> }.into_any(),
                                    Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                                    Some(Ok(rows)) if rows.is_empty() => view! {
                                        <Empty message="No architecture model exists in this tenant. Create one on the trace screen; the map renders it here.".to_owned()/>
                                    }.into_any(),
                                    Some(Ok(rows)) => view! {
                                        <table class="table">
                                            <thead>
                                                <tr><th>"Title"</th><th>"Language"</th><th>"Head"</th><th>"Published"</th><th></th></tr>
                                            </thead>
                                            <tbody>
                                                {rows.into_iter().map(|row| {
                                                    let id = text(&row, "id");
                                                    let id_for_button = id.clone();
                                                    let id_for_class = id.clone();
                                                    view! {
                                                        <tr>
                                                            <td><strong>{text(&row, "title")}</strong></td>
                                                            <td>{format!("{} {}", text(&row, "language"), text(&row, "language_version"))}</td>
                                                            <td class="num">{int(&row, "head_revision")}</td>
                                                            <td>
                                                                {if row.get("published_revision_id").and_then(serde_json::Value::as_str).is_some() {
                                                                    "published".to_owned()
                                                                } else {
                                                                    "draft".to_owned()
                                                                }}
                                                            </td>
                                                            <td>
                                                                <button
                                                                    class="btn small"
                                                                    class:selected=move || selected_model.get().as_deref() == Some(id_for_class.as_str())
                                                                    on:click=move |_| selected_model.set(Some(id_for_button.clone()))
                                                                >
                                                                    "Map"
                                                                </button>
                                                            </td>
                                                        </tr>
                                                    }
                                                }).collect_view()}
                                            </tbody>
                                        </table>
                                    }.into_any(),
                                }}
                            </div>

                            <div class="panel" data-testid="system-views">
                                <h2>"Views"</h2>
                                {move || match views.get() {
                                    None => view! { <p class="muted">"Reading views…"</p> }.into_any(),
                                    Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                                    Some(Ok(rows)) if rows.is_empty() => view! {
                                        <Empty message="This model has no view yet. A view is what the renderer draws; create one on the trace screen.".to_owned()/>
                                    }.into_any(),
                                    Some(Ok(rows)) => view! {
                                        <table class="table">
                                            <thead><tr><th>"Name"</th><th>"Kind"</th><th class="num">"Version"</th><th></th></tr></thead>
                                            <tbody>
                                                {rows.into_iter().map(|row| {
                                                    let id = text(&row, "id");
                                                    let id_for_draw = id.clone();
                                                    let id_for_class = id.clone();
                                                    let id_for_generate = id.clone();
                                                    view! {
                                                        <tr>
                                                            <td><strong>{text(&row, "name")}</strong></td>
                                                            <td>{text(&row, "kind")}</td>
                                                            <td class="num">{int(&row, "version")}</td>
                                                            <td>
                                                                <button
                                                                    class="btn small"
                                                                    class:selected=move || selected_view.get().as_deref() == Some(id_for_class.as_str())
                                                                    on:click=move |_| selected_view.set(Some(id_for_draw.clone()))
                                                                >
                                                                    "Draw"
                                                                </button>
                                                                {ctx.has_action("model_views.generate").then(|| {
                                                                    view! {
                                                                        <button class="btn small" on:click=move |_| generate.run(id_for_generate.clone())>
                                                                            "Lay out"
                                                                        </button>
                                                                    }
                                                                })}
                                                            </td>
                                                        </tr>
                                                    }
                                                }).collect_view()}
                                            </tbody>
                                        </table>
                                    }.into_any(),
                                }}
                                {move || match selected_view.get() {
                                    Some(_) => view! { <Diagram model_id=selected_model.get().unwrap_or_default() selected=selected_view/> }.into_any(),
                                    None => view! { <p class="muted">"Select a view to draw its diagram."</p> }.into_any(),
                                }}
                            </div>
                        </div>

                        <div class="panel" data-testid="system-traceability">
                            <h2>"Traceability"</h2>
                            {move || match traceability.get() {
                                None => view! { <p class="muted">"Reading the traceability matrix…"</p> }.into_any(),
                                Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                                Some(Ok(None)) => view! { <Empty message="Select a model; models.traceability is not in this role's manifest.".to_owned()/> }.into_any(),
                                Some(Ok(Some(body))) => {
                                    let elements = body.get("elements").and_then(serde_json::Value::as_array).cloned().unwrap_or_default();
                                    let untraced = body.get("untraced").and_then(serde_json::Value::as_i64).unwrap_or(0);
                                    view! {
                                        <p class="muted">{format!("{} element(s) · {} untraced", elements.len(), untraced)}</p>
                                        <table class="table">
                                            <thead><tr><th>"Element"</th><th>"Kind"</th><th>"Governed asset"</th><th class="num">"Out"</th><th class="num">"In"</th></tr></thead>
                                            <tbody>
                                                {elements.into_iter().map(|row| view! {
                                                    <tr>
                                                        <td>{text(&row, "name")}</td>
                                                        <td>{text(&row, "kind")}</td>
                                                        <td class="mono">{row.get("governed_asset_id").and_then(serde_json::Value::as_str).unwrap_or("—").to_owned()}</td>
                                                        <td class="num">{int(&row, "outgoing")}</td>
                                                        <td class="num">{int(&row, "incoming")}</td>
                                                    </tr>
                                                }).collect_view()}
                                            </tbody>
                                        </table>
                                    }.into_any()
                                }
                            }}
                        </div>
                    }.into_any()
                }
            }}

            <div class="panel" data-testid="system-ui-map">
                <h2>"The UI map"</h2>
                <p class="muted">
                    "The destinations this binary serves, their routes and the action families each
                    reads. The build's own structure, not a tenant row."
                </p>
                <table class="table">
                    <thead><tr><th>"Destination"</th><th>"Route"</th><th>"Reads"</th></tr></thead>
                    <tbody>
                        {UI_MAP.into_iter().map(|(name, route, reads)| view! {
                            <tr>
                                <td><strong>{name}</strong></td>
                                <td class="mono">{route}</td>
                                <td class="muted">{reads}</td>
                            </tr>
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
        </section>
    }
}
