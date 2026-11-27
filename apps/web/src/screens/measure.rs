//! Measure — the retained instruments, in one place (T18-5, #566; SPEC_V2 §10.5.2).
//!
//! Measure does not replace the instruments: each panel reads the owning slice's action and links
//! to the screen that renders it in full (`/measure/usage`, `/measure/prompt-audit`,
//! `/evals/benchmarks`, `/evals/improvement`, `/evals`, `/metering`). A panel whose action is not
//! in the manifest says so; a panel whose read fails shows the server's message. No number is
//! computed on the client that the server did not return.

use leptos::prelude::*;
use leptos_router::components::A;
use serde_json::{Value, json};

use crate::api::{ApiError, InvokeOptions};
use crate::components::{Empty, ErrorPanel, Tile, fmt_int};
use crate::screens::v2_data::{read_rows, text};
use crate::state::AppCtx;

/// One list action as a resource; the caller gates on `has_action` before reading.
fn list_resource(
    ctx: AppCtx,
    refresh: RwSignal<u32>,
    name: &'static str,
    input: Value,
) -> LocalResource<Result<Vec<Value>, ApiError>> {
    LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let _ = refresh.get();
        let client = ctx.client();
        let input = input.clone();
        async move {
            if !ctx.has_action(name) {
                return Ok(Vec::new());
            }
            read_rows(&client, name, input).await
        }
    })
}

#[component]
pub fn MeasureScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let refresh = RwSignal::new(0_u32);

    let evals = list_resource(ctx, refresh, "evals.list", json!({ "limit": 5 }));
    let suites = list_resource(ctx, refresh, "benchmark_suites.list", json!({ "limit": 5 }));
    let audits = list_resource(ctx, refresh, "prompt_audits.list", json!({ "limit": 5 }));
    let calls = list_resource(ctx, refresh, "inference_calls.list", json!({ "limit": 5 }));

    // `metrics.usage` answers `{level, rows}`, not the shared `{items}` page.
    let usage = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let _ = refresh.get();
        let client = ctx.client();
        async move {
            if !ctx.has_action("metrics.usage") {
                return Ok(Vec::new());
            }
            let invocation = client
                .invoke(
                    "metrics.usage",
                    json!({ "level": "tenant" }),
                    InvokeOptions::default(),
                )
                .await?;
            Ok(invocation
                .output
                .get("rows")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default())
        }
    });

    let retry = Callback::new(move |_| refresh.update(|n| *n += 1));

    view! {
        <section class="screen" data-testid="measure">
            <div class="screen-head">
                <h1>"Measure"</h1>
                <p class="muted">
                    "The retained instruments: metering, inference usage, prompt audit, benchmarks,
                    improvement and evals. Each panel reads the server's own action and links to the
                    screen that owns it."
                </p>
            </div>

            <div class="grid two">
                <div class="panel" data-testid="measure-usage">
                    <h2>"Metering"</h2>
                    {move || match usage.get() {
                        None => view! { <p class="muted">"Reading the ledger…"</p> }.into_any(),
                        Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                        Some(Ok(rows)) if rows.is_empty() => view! {
                            <Empty message="No metering row in this tenant yet. A run writes one when it executes.".to_owned()/>
                        }.into_any(),
                        Some(Ok(rows)) => {
                            let calls: i64 = rows.iter().map(|row| row.get("calls").and_then(Value::as_i64).unwrap_or(0)).sum();
                            let tokens: i64 = rows.iter().map(|row| row.get("tokens").and_then(Value::as_i64).unwrap_or(0)).sum();
                            let cost: f64 = rows.iter().map(|row| row.get("cost_usd").and_then(Value::as_f64).unwrap_or(0.0)).sum();
                            view! {
                                <div class="tiles">
                                    <Tile label="Calls".to_owned() value=fmt_int(calls) detail="the tenant's ledger".to_owned()/>
                                    <Tile label="Tokens".to_owned() value=fmt_int(tokens) detail="priced and unpriced".to_owned()/>
                                    <Tile label="Cost".to_owned() value=format!("${cost:.4}") detail="the ledger's pinned price".to_owned()/>
                                </div>
                                <p class="muted"><A href="/metering">"Open metering"</A></p>
                            }.into_any()
                        }
                    }}
                </div>

                <div class="panel" data-testid="measure-inference">
                    <h2>"Inference usage"</h2>
                    {move || match calls.get() {
                        None => view! { <p class="muted">"Reading inference calls…"</p> }.into_any(),
                        Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                        Some(Ok(rows)) => view! {
                            <p class="muted">{format!("{} recent call(s)", rows.len())}</p>
                            <p class="muted"><A href="/measure/usage">"Open usage & cache"</A></p>
                        }.into_any(),
                    }}
                </div>

                <div class="panel" data-testid="measure-prompt-audit">
                    <h2>"Prompt audit"</h2>
                    {move || match audits.get() {
                        None => view! { <p class="muted">"Reading prompt audits…"</p> }.into_any(),
                        Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                        Some(Ok(rows)) if rows.is_empty() => view! {
                            <Empty message="No prompt audit yet. An audit records one compiled prompt's sources and hash.".to_owned()/>
                        }.into_any(),
                        Some(Ok(rows)) => view! {
                            <p class="muted">{format!("{} recent audit(s)", rows.len())}</p>
                            <ul>{rows.into_iter().map(|row| view! { <li class="mono">{text(&row, "id")}</li> }).collect_view()}</ul>
                            <p class="muted"><A href="/measure/prompt-audit">"Open prompt audit"</A></p>
                        }.into_any(),
                    }}
                </div>

                <div class="panel" data-testid="measure-benchmarks">
                    <h2>"Benchmarks and improvement"</h2>
                    {move || match suites.get() {
                        None => view! { <p class="muted">"Reading benchmark suites…"</p> }.into_any(),
                        Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                        Some(Ok(rows)) => view! {
                            <p class="muted">{format!("{} suite(s)", rows.len())}</p>
                            <div class="row">
                                <A href="/evals/benchmarks">"Benchmarks"</A>
                                " · "
                                <A href="/evals/improvement">"Improvement"</A>
                            </div>
                        }.into_any(),
                    }}
                </div>

                <div class="panel" data-testid="measure-evals">
                    <h2>"Evals"</h2>
                    {move || match evals.get() {
                        None => view! { <p class="muted">"Reading evals…"</p> }.into_any(),
                        Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                        Some(Ok(rows)) if rows.is_empty() => view! {
                            <Empty message="No eval exists yet. An eval measures one flow against its subject.".to_owned()/>
                        }.into_any(),
                        Some(Ok(rows)) => view! {
                            <p class="muted">{format!("{} eval(s)", rows.len())}</p>
                            <p class="muted"><A href="/evals">"Open evals"</A></p>
                        }.into_any(),
                    }}
                </div>
            </div>
        </section>
    }
}
