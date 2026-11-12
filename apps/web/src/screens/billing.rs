//! Cost & usage — the billing panel, shared by the Platform tenant detail and the Usage
//! screen.
//!
//! Three sources, each with its OWN availability: billing (app-scoped money and quota),
//! costs (tenant-wide, split by project) and actors (per person). A source that fails says
//! so with the upstream's reason — never `$0.00`, because "you spent nothing" and "we could
//! not ask" are different claims.
//!
//! ⚠️ **Only the billing source is app-scoped.** `/api/costs` has no app axis, so the
//! project split can include other products in the same tenant; the panel says so on the
//! section, not in a footnote.

use leptos::prelude::*;
use serde_json::Value;

use crate::api::ApiError;
use crate::components::{ErrorPanel, Tile, fmt_compact, short_id};
use crate::models::{BillingSource, TenantBilling};
use crate::state::AppCtx;

fn load(ctx: AppCtx, tenant_id: String) -> LocalResource<Result<TenantBilling, ApiError>> {
    LocalResource::new(move || {
        let client = ctx.client();
        let path = format!("/api/tenants/{tenant_id}/billing");
        async move {
            let value = client.get(&path).await?;
            serde_json::from_value::<TenantBilling>(value)
                .map_err(|error| ApiError::Network(format!("billing: {error}")))
        }
    })
}

/// A money string from billing (decimals arrive as STRINGS, never numbers).
fn money(value: &Value, pointer: &str) -> Option<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(|raw| format!("${raw}"))
}

fn integer(value: &Value, pointer: &str) -> Option<i64> {
    value.pointer(pointer).and_then(Value::as_i64)
}

/// The conveyor's own licence entry inside `billing.licenses[]`.
fn app_licence(source: &BillingSource) -> Option<&Value> {
    source
        .data
        .as_ref()?
        .get("licenses")?
        .as_array()?
        .iter()
        .find(|licence| licence.get("app_key").and_then(Value::as_str) == Some("conveyor"))
}

fn unavailable(source: &BillingSource, what: &str) -> impl IntoView {
    let reason = source
        .error
        .clone()
        .unwrap_or_else(|| "no reason given".to_owned());
    view! {
        <p class="muted">
            {format!("{what} is unavailable — ")}<span class="mono">{reason}</span>
        </p>
    }
}

/// The panel. `tenant_id` is the tenant whose spend is shown.
#[component]
pub fn BillingPanel(tenant_id: String) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let billing = load(ctx, tenant_id);
    let advanced = ctx.advanced;

    view! {
        <section class="panel">
            <header class="panel-head">
                <div>
                    <h2>"Cost & usage"</h2>
                    <p class="muted">
                        "From billing_service. Money is the service's figure — the conveyor stores none."
                    </p>
                </div>
            </header>
            <Suspense fallback=move || view! { <p class="muted">"Loading cost & usage…"</p> }>
                {move || match billing.get() {
                    None => view! { <p class="muted">"Loading…"</p> }.into_any(),
                    Some(Err(error)) => view! {
                        <ErrorPanel error=error on_retry=Callback::new(move |_| billing.refetch())/>
                    }.into_any(),
                    Some(Ok(document)) => {
                        let month = document.month.clone();
                        let licence = app_licence(&document.billing).cloned();
                        let billed = licence
                            .as_ref()
                            .and_then(|licence| money(licence, "/costs/billed_cost_usd"))
                            .unwrap_or_else(|| "—".to_owned());
                        let requests = licence
                            .as_ref()
                            .and_then(|licence| integer(licence, "/usage/request_count"))
                            .map(fmt_compact)
                            .unwrap_or_else(|| "—".to_owned());
                        let quota = licence
                            .as_ref()
                            .and_then(|licence| licence.pointer("/usage/quota_pct").map(|v| v.to_string()))
                            .map(|pct| format!("{pct}%"))
                            .unwrap_or_else(|| "—".to_owned());
                        let markup = document
                            .billing
                            .data
                            .as_ref()
                            .and_then(|data| data.get("effective_markup_pct").cloned())
                            .map(|pct| format!("{pct}%"))
                            .unwrap_or_else(|| "—".to_owned());
                        let billing_source = document.billing.clone();
                        let costs_source = document.costs.clone();
                        let actors_source = document.actors.clone();
                        let month_label = month.clone();
                        view! {
                            <p class="muted">
                                "Month "<span class="mono">{month_label}</span>
                                " · app " <span class="mono">"conveyor"</span>
                            </p>
                            {if document.billing.available {
                                view! {
                                    <div class="tiles">
                                        <Tile label="Billed".to_string() value=billed detail="conveyor licence".to_string()/>
                                        <Tile label="Requests".to_string() value=requests detail="this month".to_string()/>
                                        <Tile label="Quota used".to_string() value=quota detail="of the plan".to_string()/>
                                        <Tile label="Markup".to_string() value=markup detail="applied by billing".to_string()/>
                                    </div>
                                }.into_any()
                            } else {
                                unavailable(&billing_source, "Billing").into_any()
                            }}

                            <h3>"By project " <span class="badge">"tenant-wide"</span></h3>
                            {if document.costs.available {
                                let rows = document
                                    .costs
                                    .data
                                    .as_ref()
                                    .and_then(|data| data.get("by_project").and_then(Value::as_array).cloned())
                                    .unwrap_or_default();
                                if rows.is_empty() {
                                    view! { <p class="muted">"No project-attributed cost in this window."</p> }.into_any()
                                } else {
                                    view! {
                                        <table class="table">
                                            <thead><tr><th>"Project"</th><th>"Requests"</th><th>"Cost"</th></tr></thead>
                                            <tbody>
                                                {rows.into_iter().map(|row| {
                                                    let id = row.get("project_id").and_then(Value::as_str).map(str::to_owned);
                                                    let requests = row.get("request_count").and_then(Value::as_i64).map(fmt_compact).unwrap_or_else(|| "—".into());
                                                    let cost = money(&row, "/cost_usd").unwrap_or_else(|| "—".into());
                                                    view! {
                                                        <tr>
                                                            <td class="mono">{short_id(id.as_deref())}</td>
                                                            <td>{requests}</td>
                                                            <td>{cost}</td>
                                                        </tr>
                                                    }
                                                }).collect_view()}
                                            </tbody>
                                        </table>
                                    }.into_any()
                                }
                            } else {
                                unavailable(&costs_source, "The project split").into_any()
                            }}

                            <h3>"By person"</h3>
                            {if document.actors.available {
                                let rows = document
                                    .actors
                                    .data
                                    .as_ref()
                                    .and_then(|data| data.get("by_actor").and_then(Value::as_array).cloned())
                                    .unwrap_or_default();
                                if rows.is_empty() {
                                    view! { <p class="muted">"No actor-attributed cost in this window."</p> }.into_any()
                                } else {
                                    view! {
                                        <table class="table">
                                            <thead><tr><th>"Actor"</th><th>"Requests"</th><th>"Cost"</th></tr></thead>
                                            <tbody>
                                                {rows.into_iter().map(|row| {
                                                    let id = row.get("actor_id").and_then(Value::as_str).map(str::to_owned);
                                                    let requests = row.get("request_count").and_then(Value::as_i64).map(fmt_compact).unwrap_or_else(|| "—".into());
                                                    let cost = money(&row, "/cost_usd").unwrap_or_else(|| "—".into());
                                                    view! {
                                                        <tr>
                                                            <td class="mono">{short_id(id.as_deref())}</td>
                                                            <td>{requests}</td>
                                                            <td>{cost}</td>
                                                        </tr>
                                                    }
                                                }).collect_view()}
                                            </tbody>
                                        </table>
                                    }.into_any()
                                }
                            } else {
                                unavailable(&actors_source, "The per-person split").into_any()
                            }}

                            {move || advanced.get().then(|| view! {
                                <details class="advanced">
                                    <summary>"Raw billing document"</summary>
                                    <pre class="mono">{serde_json::to_string_pretty(&serde_json::json!({
                                        "billing": document.billing.data,
                                        "costs": document.costs.data,
                                        "actors": document.actors.data,
                                    })).unwrap_or_default()}</pre>
                                </details>
                            })}
                        }.into_any()
                    }
                }}
            </Suspense>
        </section>
    }
}
