//! Improve — the flow figures and the kata queue (SPEC_V2 §10.5, destination **Improve**).
//!
//! Two reads and no write: `metrics.flow` derives WIP, throughput, lead time and flow efficiency
//! at read time from the board's own rows (`card`, `list` and the `card.moved` outbox). A figure
//! whose source ledger does not exist is returned in `unavailable` with its reason and rendered
//! that way — never as a zero. `kata.list` is the kata queue (T5): each kata pins its instrument
//! and split, and adoption/revert stay governed actions, not this screen's write.

use leptos::prelude::*;
use serde::Deserialize;
use serde_json::json;

use crate::api::{ApiError, InvokeOptions, Page};
use crate::components::{Empty, ErrorPanel, Planned, StatusPill, Tile};
use crate::state::AppCtx;

/// A figure `metrics.flow` could not compute, with the reason: unavailable, never zero.
#[derive(Clone, Debug, Deserialize)]
struct Unavailable {
    metric: String,
    reason: String,
}

/// The flow family as `metrics.flow` returns it; only the rendered fields are kept.
#[derive(Clone, Debug, Deserialize)]
struct FlowReport {
    wip: i64,
    throughput: i64,
    #[serde(default)]
    lead_time_seconds: Option<f64>,
    #[serde(default)]
    flow_efficiency: Option<f64>,
    #[serde(default)]
    unavailable: Vec<Unavailable>,
}

fn load_flow(ctx: AppCtx) -> LocalResource<Result<FlowReport, ApiError>> {
    LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        async move {
            let output = ctx
                .invoke("metrics.flow", json!({}), InvokeOptions::default())
                .await?
                .output;
            serde_json::from_value(output)
                .map_err(|error| ApiError::Network(format!("flow: {error}")))
        }
    })
}

/// One kata as `kata.list` returns it; only the rendered fields are kept.
#[derive(Clone, Debug, Deserialize)]
struct Kata {
    signature: String,
    #[serde(default)]
    instrument_kind: String,
    #[serde(default)]
    instrument_version: String,
    #[serde(default)]
    split: String,
    status: String,
    #[serde(default)]
    outcome: Option<String>,
}

fn load_kata(ctx: AppCtx) -> LocalResource<Result<Vec<Kata>, ApiError>> {
    LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        async move {
            let page: Page<Kata> = serde_json::from_value(
                ctx.invoke("kata.list", json!({"limit": 50}), InvokeOptions::default())
                    .await?
                    .output,
            )
            .map_err(|error| ApiError::Network(format!("kata queue: {error}")))?;
            Ok(page.items)
        }
    })
}

/// A duration in seconds as `Xd Yh` or `N min`; `—` when the read has no figure for it. An
/// unmeasured span is a null, and a null is never drawn as zero minutes.
fn duration(seconds: Option<f64>) -> String {
    let Some(seconds) = seconds.filter(|value| value.is_finite() && *value >= 0.0) else {
        return "—".to_owned();
    };
    let seconds = seconds.round() as i64;
    if seconds >= 86_400 {
        format!("{}d {}h", seconds / 86_400, (seconds % 86_400) / 3_600)
    } else {
        format!("{} min", seconds / 60)
    }
}

/// A ratio as a percentage; `—` when the read has no figure. An unmeasured ratio is never 0%.
fn percent(value: Option<f64>) -> String {
    match value.filter(|value| value.is_finite()) {
        Some(value) => format!("{}%", (value * 100.0).round() as i64),
        None => "—".to_owned(),
    }
}

#[component]
pub fn Improve() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let flow = load_flow(ctx);
    let kata = load_kata(ctx);

    view! {
        <section class="screen">
            {move || {
                if !ctx.has_action("metrics.flow") {
                    return view! {
                        <Planned
                            title="Improve is planned".to_owned()
                            detail="metrics.flow is not in this role's manifest; the screen appears when it lands.".to_owned()
                        />
                    }
                    .into_any();
                }
                view! {
                    <header class="screen-head">
                        <div>
                            <h1>"Improve"</h1>
                            <p class="muted">
                                "Flow figures are read-time derivations; a figure whose source ledger does not exist is listed as unavailable with its reason, never shown as zero."
                            </p>
                        </div>
                    </header>
                    {move || match flow.get() {
                        None => view! { <p class="muted">"Reading the flow figures…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=Callback::new(move |_| flow.refetch())/>
                        }.into_any(),
                        Some(Ok(report)) => {
                            let wip = report.wip.to_string();
                            let throughput = report.throughput.to_string();
                            let lead = duration(report.lead_time_seconds);
                            let efficiency = percent(report.flow_efficiency);
                            let unavailable = report.unavailable;
                            view! {
                                <div class="tiles">
                                    <Tile
                                        label="WIP".to_owned()
                                        value=wip
                                        detail="cards in active or waiting lists now".to_owned()
                                    />
                                    <Tile
                                        label="Throughput".to_owned()
                                        value=throughput
                                        detail="cards moved into a done list, last 90 days".to_owned()
                                    />
                                    <Tile
                                        label="Lead time".to_owned()
                                        value=lead
                                        detail="average created → done, last 90 days".to_owned()
                                    />
                                    <Tile
                                        label="Flow efficiency".to_owned()
                                        value=efficiency
                                        detail="active time ÷ lead time, per card".to_owned()
                                    />
                                </div>
                                <section class="panel">
                                    <h2>"Unavailable figures"</h2>
                                    {if unavailable.is_empty() {
                                        view! {
                                            <Empty message="Every flow figure has a source ledger in this tenant.".to_owned()/>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <div class="tablewrap">
                                                <table class="table">
                                                    <thead><tr>
                                                        <th>"Metric"</th><th>"Reason"</th>
                                                    </tr></thead>
                                                    <tbody>
                                                        {unavailable.iter().map(|row| view! {
                                                            <tr>
                                                                <td class="mono">{row.metric.clone()}</td>
                                                                <td>{row.reason.clone()}</td>
                                                            </tr>
                                                        }).collect_view()}
                                                    </tbody>
                                                </table>
                                            </div>
                                        }.into_any()
                                    }}
                                </section>
                            }.into_any()
                        }
                    }}
                    <section class="panel">
                        <h2>"Kata queue"</h2>
                        {move || {
                            if !ctx.has_action("kata.list") {
                                return view! {
                                    <Planned
                                        title="The kata queue is unavailable".to_owned()
                                        detail="kata.list is not in this role's manifest; the queue appears when the action is, not before.".to_owned()
                                    />
                                }
                                .into_any();
                            }
                            match kata.get() {
                                None => view! { <p class="muted">"Reading the kata queue…"</p> }.into_any(),
                                Some(Err(error)) => view! {
                                    <ErrorPanel error=error on_retry=Callback::new(move |_| kata.refetch())/>
                                }
                                .into_any(),
                                Some(Ok(items)) if items.is_empty() => view! {
                                    <Empty message="No kata is recorded in this tenant; one opens from a coach escalation or a review finding.".to_owned()/>
                                }
                                .into_any(),
                                Some(Ok(items)) => view! {
                                    <div class="tablewrap">
                                        <table class="table">
                                            <thead><tr>
                                                <th>"Signature"</th><th>"Instrument"</th><th>"Split"</th>
                                                <th>"Status"</th><th>"Outcome"</th>
                                            </tr></thead>
                                            <tbody>
                                                {items.iter().map(|row| view! {
                                                    <tr>
                                                        <td class="mono">{row.signature.clone()}</td>
                                                        <td>{format!("{} {}", row.instrument_kind, row.instrument_version).trim().to_owned()}</td>
                                                        <td><StatusPill status=row.split.clone()/></td>
                                                        <td><StatusPill status=row.status.clone()/></td>
                                                        <td>{row.outcome.clone().unwrap_or_else(|| "—".to_owned())}</td>
                                                    </tr>
                                                }).collect_view()}
                                            </tbody>
                                        </table>
                                    </div>
                                }
                                .into_any(),
                            }
                        }}
                    </section>
                }
                .into_any()
            }}
        </section>
    }
}
