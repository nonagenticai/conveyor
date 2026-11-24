//! Authoring — the tenant's authored assets and the four steps that produce them
//! (SPEC_V2 §10.5 destination **Authoring**; §5.7 the asset lifecycle).
//!
//! The reviewed reference is the design pack's authoring page (`docs/design/v2.js`): an intake
//! queue, a draft catalogue, review & publish and the dependency status. The four tabs here are
//! that page's, rendered with the app's own classes and no `v2-*` CSS.
//!
//! Every read is an action's page (`authoring.assets`, `approvals.list`) and every write is the
//! owning slice's action: `authoring.intake|draft|review|publish` and `approvals.request|decide`.
//! Two measured constraints shape the screen rather than being papered over:
//!
//! * **There is no intake read.** The family exposes intake write-only, so the queue is the draft
//!   catalogue grouped by `AssetOut.intake_id`, plus the intake this screen has just recorded from
//!   its own action's response. The brief/sources of past intakes wait on `authoring.intakes`
//!   (issue #655).
//! * **The dependency order is enforced at the draft, not at publish** (`asset_draft_tx`), so the
//!   refusal this screen surfaces is the draft's `422 asset_dependency_unpublished`.
//!
//! No fixture row is rendered anywhere: an empty tab names the action that would create the row.

use std::collections::HashMap;

use leptos::prelude::*;
use serde_json::{Value, json};
use wasm_bindgen::JsCast;

use crate::api::{ApiError, InvokeOptions};
use crate::components::{Empty, ErrorPanel, Planned, StatusPill, Tile, fmt_int, short_id};
use crate::screens::v2_data::{int, read_rows, text};
use crate::state::AppCtx;

/// The gate `authoring.review` reads a decision from (`crates/store/src/authoring.rs`).
const REVIEW_GATE: &str = "authoring.review";

/// The §5.7 asset kinds, in dependency order — the draft form's choices and the rail's order.
const ASSET_KINDS: [(&str, &str); 9] = [
    ("industry", "industry"),
    ("department", "department"),
    ("job", "job"),
    ("capability", "capability"),
    ("procedure", "procedure"),
    ("knowledge_pack", "knowledge pack"),
    ("project_template", "project template"),
    ("policy", "policy"),
    ("tool_request", "tool request"),
];

/// §5.7's review depth, as the mock renders it: who reviews each kind and what publication is.
const REVIEW_DEPTH: [(&str, &str, &str); 6] = [
    (
        "Organisation (industry, department)",
        "human · tenant admin",
        "release + gate",
    ),
    ("Job", "human · department owner", "release + binding"),
    (
        "Capability / procedure",
        "human · domain reviewer",
        "versioned release",
    ),
    (
        "Knowledge pack",
        "human · knowledge owner",
        "pinned pack version",
    ),
    (
        "Policy / approval",
        "human · governance",
        "gated policy change",
    ),
    ("Tool request", "security review", "tool publication"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Intake,
    Catalogue,
    Review,
    Dependencies,
}

const TABS: [(Tab, &str); 4] = [
    (Tab::Intake, "Intake queue"),
    (Tab::Catalogue, "Draft catalogue"),
    (Tab::Review, "Review & publish"),
    (Tab::Dependencies, "Dependency status"),
];

/// The tenant's rows for the whole screen: the asset page and the pending + approved gates.
#[derive(Clone, Debug, Default)]
struct Data {
    assets: Vec<Value>,
    gates: Vec<Value>,
}

/// One shared draft form. The signals are `Copy`, so a panel can build its own handler without
/// cloning the whole form.
#[derive(Clone, Copy)]
struct Form {
    brief: RwSignal<String>,
    sources: RwSignal<String>,
    intake_id: RwSignal<String>,
    kind: RwSignal<String>,
    key: RwSignal<String>,
    payload: RwSignal<String>,
    depends_on: RwSignal<String>,
}

impl Form {
    fn new() -> Self {
        Self {
            brief: RwSignal::new(String::new()),
            sources: RwSignal::new(String::new()),
            intake_id: RwSignal::new(String::new()),
            kind: RwSignal::new("department".to_owned()),
            key: RwSignal::new(String::new()),
            payload: RwSignal::new("{\n  \"title\": \"\"\n}".to_owned()),
            depends_on: RwSignal::new(String::new()),
        }
    }
}

async fn load(ctx: AppCtx) -> Result<Data, ApiError> {
    let client = ctx.client();
    let mut data = Data::default();
    if ctx.has_action("authoring.assets") {
        data.assets = read_rows(&client, "authoring.assets", json!({ "limit": 200 })).await?;
    }
    if ctx.has_action("approvals.list") {
        // The page fields are flattened into the input (`PageIn`); the nested `{"page": …}` shape
        // older screens use is ignored and silently defaults to 50 rows.
        let pending = read_rows(
            &client,
            "approvals.list",
            json!({ "status": "pending", "limit": 200 }),
        )
        .await?;
        let approved = read_rows(
            &client,
            "approvals.list",
            json!({ "status": "approved", "limit": 200 }),
        )
        .await?;
        data.gates = pending.into_iter().chain(approved).collect();
    }
    Ok(data)
}

fn error_text(error: &ApiError) -> String {
    match error {
        ApiError::Invalid { detail, errors } if !errors.is_empty() => {
            let fields: Vec<String> = errors
                .iter()
                .map(|error| format!("{}: {} [{}]", error.field, error.detail, error.code))
                .collect();
            format!("{detail} · {}", fields.join(" · "))
        }
        other => other.to_string(),
    }
}

fn input_value(event: &web_sys::Event) -> String {
    event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|input| input.value())
        .unwrap_or_default()
}

fn area_value(event: &web_sys::Event) -> String {
    event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
        .map(|area| area.value())
        .unwrap_or_default()
}

fn select_value(event: &web_sys::Event) -> String {
    event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlSelectElement>().ok())
        .map(|select| select.value())
        .unwrap_or_default()
}

/// A string field of a row, or `None` — the exact-match form, never the `—` placeholder.
fn field(row: &Value, key: &str) -> Option<String> {
    row.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// The id strings of an array field (`depends_on`).
fn ids_of(row: &Value, key: &str) -> Vec<String> {
    row.get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// A timestamp as `YYYY-MM-DD`; `—` when absent.
fn date(row: &Value, key: &str) -> String {
    let Some(at) = field(row, key) else {
        return "—".to_owned();
    };
    at.chars().take(10).collect()
}

/// One intake as the queue can know it: the id every draft cites, and what it has produced.
#[derive(Clone, Debug)]
struct IntakeSummary {
    id: String,
    drafts: usize,
    reviewed: usize,
    published: usize,
    kinds: Vec<String>,
}

impl IntakeSummary {
    fn state(&self) -> &'static str {
        if self.published > 0 {
            "published"
        } else if self.reviewed > 0 {
            "in review"
        } else {
            "drafted"
        }
    }
}

/// The intakes the catalogue can name, newest-touched first (the read is `updated_at` DESC).
fn intakes(assets: &[Value]) -> Vec<IntakeSummary> {
    let mut order: Vec<String> = Vec::new();
    let mut by_id: HashMap<String, IntakeSummary> = HashMap::new();
    for asset in assets {
        let Some(id) = field(asset, "intake_id") else {
            continue;
        };
        if !by_id.contains_key(&id) {
            order.push(id.clone());
            by_id.insert(
                id.clone(),
                IntakeSummary {
                    id: id.clone(),
                    drafts: 0,
                    reviewed: 0,
                    published: 0,
                    kinds: Vec::new(),
                },
            );
        }
        let Some(summary) = by_id.get_mut(&id) else {
            continue;
        };
        summary.drafts += 1;
        match field(asset, "status").as_deref() {
            Some("published") => summary.published += 1,
            Some("reviewed") => summary.reviewed += 1,
            _ => {}
        }
        let kind = text(asset, "kind");
        if !summary.kinds.contains(&kind) {
            summary.kinds.push(kind);
        }
    }
    order
        .into_iter()
        .filter_map(|id| by_id.remove(&id))
        .collect()
}

/// The most advanced gate for one asset's review: an approved gate is what a review may name.
fn gate_status(gates: &[Value], target: &str) -> &'static str {
    let matching = |status: &str| {
        gates.iter().any(|row| {
            field(row, "gate").as_deref() == Some(REVIEW_GATE)
                && field(row, "target").as_deref() == Some(target)
                && field(row, "status").as_deref() == Some(status)
        })
    };
    if matching("approved") {
        "approved"
    } else if matching("pending") {
        "pending · a second person decides"
    } else if matching("rejected") {
        "rejected"
    } else {
        "none · request it"
    }
}

fn approved_gate(gates: &[Value], target: &str) -> Option<String> {
    gates
        .iter()
        .find(|row| {
            field(row, "gate").as_deref() == Some(REVIEW_GATE)
                && field(row, "target").as_deref() == Some(target)
                && field(row, "status").as_deref() == Some("approved")
        })
        .and_then(|row| field(row, "id"))
}

fn find_asset<'a>(assets: &'a [Value], id: &str) -> Option<&'a Value> {
    assets
        .iter()
        .find(|row| field(row, "id").as_deref() == Some(id))
}

/// Every dependency of one asset resolved against the catalogue: (id, status, published).
fn dependency_lines(assets: &[Value], asset: &Value) -> Vec<(String, String, bool)> {
    ids_of(asset, "depends_on")
        .into_iter()
        .map(|id| {
            let status = find_asset(assets, &id)
                .map(|row| text(row, "status"))
                .unwrap_or_else(|| "absent".to_owned());
            let published = status == "published";
            (id, status, published)
        })
        .collect()
}

fn blockers(assets: &[Value]) -> usize {
    assets
        .iter()
        .map(|asset| {
            dependency_lines(assets, asset)
                .into_iter()
                .filter(|(_, _, published)| !published)
                .count()
        })
        .sum()
}

#[component]
pub fn Authoring() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let data = LocalResource::new(move || {
        // Read the tenant so a switch refetches under the new role.
        let _tenant = ctx.tenant.get();
        async move { load(ctx).await }
    });
    let refresh = Callback::new(move |_| data.refetch());
    let tab = RwSignal::new(Tab::Intake);
    let selected = RwSignal::new(Option::<String>::None);
    let notice = RwSignal::new(Option::<String>::None);
    let last_intake = RwSignal::new(Option::<Value>::None);
    let form = Form::new();

    view! {
        <section class="screen" aria-labelledby="authoring-title">
            {move || {
                if !ctx.has_action("authoring.assets") {
                    return view! {
                        <Planned
                            title="Authoring is planned".to_owned()
                            detail="authoring.assets is not in this role's manifest; the screen appears when it lands.".to_owned()
                        />
                    }
                    .into_any();
                }
                view! {
                    <header class="screen-head">
                        <div>
                            <h1 id="authoring-title">"Authoring"</h1>
                            <p class="muted">
                                "a brief and its documents become reviewed assets: every output is a draft behind a review gate, the dependency order holds at the draft, and a chat message or document can never grant a tool, widen an effect ceiling or change an approval policy."
                            </p>
                        </div>
                        <div class="filterbar">
                            <button class="btn" on:click=move |_| refresh.run(())>"Refresh"</button>
                        </div>
                    </header>
                    {move || notice.get().map(|message| view! {
                        <p class="notice" role="alert">{message}</p>
                    })}
                    <div class="filterbar" role="tablist">
                        {TABS.into_iter().map(|(entry, label)| view! {
                            <button
                                class="btn small"
                                class:primary=move || tab.get() == entry
                                role="tab"
                                aria-selected=move || (tab.get() == entry).to_string()
                                on:click=move |_| tab.set(entry)
                            >
                                {label}
                            </button>
                        }).collect_view()}
                    </div>
                    {move || match data.get() {
                        None => view! { <p class="muted">"Reading the tenant's authored assets…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=refresh/>
                        }.into_any(),
                        Some(Ok(loaded)) => view! {
                            {match tab.get() {
                                Tab::Intake => intake_panel(loaded.clone(), form, last_intake, ctx, refresh, notice),
                                Tab::Catalogue => catalogue_panel(loaded.clone(), form, selected, ctx, refresh, notice),
                                Tab::Review => review_panel(loaded.clone(), selected, ctx, refresh, notice),
                                Tab::Dependencies => dependencies_panel(loaded.clone()),
                            }}
                        }.into_any(),
                    }}
                }
                .into_any()
            }}
        </section>
    }
}

// ── intake queue ────────────────────────────────────────────────────────────────────────

fn intake_panel(
    data: Data,
    form: Form,
    last: RwSignal<Option<Value>>,
    ctx: AppCtx,
    refresh: Callback<()>,
    notice: RwSignal<Option<String>>,
) -> AnyView {
    let summaries = intakes(&data.assets);
    let total: usize = summaries.iter().map(|intake| intake.drafts).sum();

    let submit = move |_event: web_sys::MouseEvent| {
        let brief = form.brief.get().trim().to_owned();
        let sources: Vec<Value> = form
            .sources
            .get()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| Value::String(line.to_owned()))
            .collect();
        if brief.is_empty() {
            notice.set(Some("Intake refused: a brief is required".to_owned()));
            return;
        }
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "authoring.intake",
                    json!({ "brief": brief, "sources": sources }),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(invocation) => {
                    last.set(Some(invocation.output));
                    form.brief.set(String::new());
                    form.sources.set(String::new());
                    notice.set(Some(
                        "Intake recorded; the draft catalogue can now cite it.".to_owned(),
                    ));
                    refresh.run(());
                }
                Err(error) => notice.set(Some(format!("Intake refused: {}", error_text(&error)))),
            }
        });
    };

    view! {
        <div class="grid-2" style="margin-top:12px">
            <section class="panel" data-testid="authoring-intakes">
                <div class="ph">
                    <span>"Intake queue"</span>
                    <span class="muted">{format!("{} draft(s) across {} intake(s)", total, summaries.len())}</span>
                </div>
                {move || match last.get() {
                    Some(intake) => view! {
                        <div class="callout">
                            <strong>"Recorded in this session"</strong>
                            <div class="mono">{text(&intake, "id")}</div>
                            <p>{text(&intake, "brief")}</p>
                            <p class="muted">{format!("status {} · version {}", text(&intake, "status"), int(&intake, "version"))}</p>
                        </div>
                    }.into_any(),
                    None => view! {
                        <p class="muted">"No intake has been recorded from this screen yet."</p>
                    }.into_any(),
                }}
                {if summaries.is_empty() {
                    view! {
                        <Empty message="No draft in this tenant cites an intake yet; record a brief below and draft from it in the Draft catalogue.".to_owned()/>
                    }.into_any()
                } else {
                    view! {
                        <div class="tablewrap">
                            <table class="table">
                                <thead><tr>
                                    <th>"Intake"</th><th>"Produces"</th><th>"Kinds"</th><th>"State"</th>
                                </tr></thead>
                                <tbody>
                                    {summaries.iter().map(|intake| view! {
                                        <tr>
                                            <td class="mono">{short_id(Some(&intake.id))}</td>
                                            <td>{format!("{} draft(s)", intake.drafts)}</td>
                                            <td>{intake.kinds.join(", ")}</td>
                                            <td><StatusPill status=intake.state().to_owned()/></td>
                                        </tr>
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    }.into_any()
                }}
                <p class="muted">
                    "The queue groups the draft catalogue by the intake each draft cites: T17 exposes the intake write-only, so a recorded brief and its sources are not readable back. The read is T17b (#655); until it lands this queue names the intake every draft already carries."
                </p>
            </section>
            <section class="panel">
                <div class="ph">
                    <span>"Record an intake"</span>
                    <span class="muted">"chat brief + documents"</span>
                </div>
                <label class="field"><span>"Brief"</span>
                    <textarea class="inp" rows="3" placeholder="Describe the department, jobs, procedures or policies you need"
                        prop:value=move || form.brief.get()
                        on:input=move |event| form.brief.set(area_value(&event))></textarea></label>
                <label class="field"><span>"Documents or references — one per line"</span>
                    <textarea class="inp" rows="3" placeholder="a URL, a document id, or an object reference"
                        prop:value=move || form.sources.get()
                        on:input=move |event| form.sources.set(area_value(&event))></textarea></label>
                <button class="btn primary" on:click=submit>"Record the intake"</button>
                <p class="muted">
                    "The brief is recorded; the drafts it should produce are written in the Draft catalogue, one asset at a time, and nothing publishes itself."
                </p>
            </section>
        </div>
    }
    .into_any()
}

// ── draft catalogue ─────────────────────────────────────────────────────────────────────

fn catalogue_panel(
    data: Data,
    form: Form,
    selected: RwSignal<Option<String>>,
    ctx: AppCtx,
    refresh: Callback<()>,
    notice: RwSignal<Option<String>>,
) -> AnyView {
    let kind_filter = RwSignal::new(Option::<String>::None);
    let status_filter = RwSignal::new(Option::<String>::None);

    let drafts = data
        .assets
        .iter()
        .filter(|row| field(row, "status").as_deref() == Some("draft"))
        .count();
    let reviewed = data
        .assets
        .iter()
        .filter(|row| field(row, "status").as_deref() == Some("reviewed"))
        .count();
    let published = data
        .assets
        .iter()
        .filter(|row| field(row, "status").as_deref() == Some("published"))
        .count();
    let total = data.assets.len();

    // The intake choices the form can offer: every id the catalogue names, plus the one just
    // recorded. The read is the page, so the form's choice list is the page's ids.
    let mut intake_ids: Vec<String> = intakes(&data.assets)
        .into_iter()
        .map(|intake| intake.id)
        .collect();
    for asset in &data.assets {
        if let Some(id) = field(asset, "intake_id")
            && !intake_ids.contains(&id)
        {
            intake_ids.push(id);
        }
    }

    let ids_for_select = intake_ids.clone();
    let assets = data.assets.clone();

    let submit = move |_event: web_sys::MouseEvent| {
        let intake_id = form.intake_id.get().trim().to_owned();
        let kind = form.kind.get();
        let key = form.key.get().trim().to_owned();
        let payload_text = form.payload.get();
        let depends_on: Vec<String> = form
            .depends_on
            .get()
            .split(',')
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .collect();
        if intake_id.is_empty() || key.is_empty() {
            notice.set(Some(
                "Draft refused: an intake id and an asset key are required".to_owned(),
            ));
            return;
        }
        let payload: Value = match serde_json::from_str(&payload_text) {
            Ok(payload) => payload,
            Err(error) => {
                notice.set(Some(format!(
                    "Draft refused: the payload is not JSON ({error})"
                )));
                return;
            }
        };
        if !payload.is_object() {
            notice.set(Some(
                "Draft refused: the payload must be a JSON object".to_owned(),
            ));
            return;
        }
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "authoring.draft",
                    json!({
                        "intake_id": intake_id,
                        "kind": kind,
                        "key": key,
                        "payload": payload,
                        "depends_on": depends_on,
                    }),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(invocation) => {
                    selected.set(field(&invocation.output, "id"));
                    form.key.set(String::new());
                    notice.set(Some(
                        "Draft written; the review gate is the next step.".to_owned(),
                    ));
                    refresh.run(());
                }
                Err(error) => notice.set(Some(format!("Draft refused: {}", error_text(&error)))),
            }
        });
    };

    let rows = move || {
        let kind = kind_filter.get();
        let status = status_filter.get();
        let rows: Vec<Value> = assets
            .iter()
            .filter(|row| {
                kind.as_deref()
                    .is_none_or(|kind| field(row, "kind").as_deref() == Some(kind))
                    && status
                        .as_deref()
                        .is_none_or(|status| field(row, "status").as_deref() == Some(status))
            })
            .cloned()
            .collect();
        if rows.is_empty() {
            return view! {
                <Empty message="No asset in this tenant carries that kind and status; authoring.draft creates one from an intake.".to_owned()/>
            }
            .into_any();
        }
        view! {
            <div class="tablewrap">
                <table class="table" data-testid="authoring-table">
                    <thead><tr>
                        <th>"Kind"</th><th>"Key"</th><th>"Intake"</th><th>"Depends on"</th>
                        <th>"Status"</th><th class="n">"Version"</th><th>"Published"</th><th></th>
                    </tr></thead>
                    <tbody>
                        {rows.into_iter().map(|asset| {
                            let id = field(&asset, "id").unwrap_or_default();
                            let open = id.clone();
                            let depends = ids_of(&asset, "depends_on").len();
                            view! {
                                <tr>
                                    <td><StatusPill status=text(&asset, "kind")/></td>
                                    <td class="mono">{text(&asset, "key")}</td>
                                    <td class="mono">{short_id(field(&asset, "intake_id").as_deref())}</td>
                                    <td class="n">{depends.to_string()}</td>
                                    <td><StatusPill status=text(&asset, "status")/></td>
                                    <td class="n">{int(&asset, "version").to_string()}</td>
                                    <td>{date(&asset, "published_at")}</td>
                                    <td><button class="btn small" on:click=move |_| selected.set(Some(open.clone()))>"Select"</button></td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
        }
        .into_any()
    };

    view! {
        <div style="margin-top:12px">
            <div class="tiles">
                <Tile label="Assets".to_owned() value=fmt_int(total as i64) detail="in the returned page".to_owned()/>
                <Tile label="Drafts".to_owned() value=fmt_int(drafts as i64) detail="awaiting the review gate".to_owned()/>
                <Tile label="Reviewed".to_owned() value=fmt_int(reviewed as i64) detail="released name a gate".to_owned()/>
                <Tile label="Published".to_owned() value=fmt_int(published as i64) detail="the tenant's live catalogue".to_owned()/>
            </div>
            <div class="filterbar">
                <label class="filter">
                    <span>"Kind"</span>
                    <select aria-label="Kind" on:change=move |event| {
                        let value = select_value(&event);
                        kind_filter.set((!value.is_empty()).then_some(value));
                    }>
                        <option value="">"All"</option>
                        {ASSET_KINDS.iter().map(|(kind, _label)| view! {
                            <option value=*kind>{*kind}</option>
                        }).collect_view()}
                    </select>
                </label>
                <label class="filter">
                    <span>"Status"</span>
                    <select aria-label="Status" on:change=move |event| {
                        let value = select_value(&event);
                        status_filter.set((!value.is_empty()).then_some(value));
                    }>
                        <option value="">"All"</option>
                        <option value="draft">"draft"</option>
                        <option value="reviewed">"reviewed"</option>
                        <option value="rejected">"rejected"</option>
                        <option value="published">"published"</option>
                    </select>
                </label>
            </div>
            {rows}
            <section class="panel" style="margin-top:12px">
                <div class="ph">
                    <span>"Draft an asset"</span>
                    <span class="muted">"one asset, citing an intake"</span>
                </div>
                <div class="grid-2">
                    <div>
                        <label class="field"><span>"Intake"</span>
                            <select aria-label="Intake" on:change=move |event| {
                                form.intake_id.set(select_value(&event));
                            }>
                                <option value="">"Choose a recorded intake"</option>
                                {ids_for_select.iter().map(|id| view! {
                                    <option value=id.clone()>{id.clone()}</option>
                                }).collect_view()}
                            </select></label>
                        <label class="field"><span>"Or paste the intake id"</span>
                            <input class="inp" prop:value=move || form.intake_id.get()
                                on:input=move |event| form.intake_id.set(input_value(&event))/></label>
                        <label class="field"><span>"Kind"</span>
                            <select aria-label="Asset kind" prop:value=move || form.kind.get()
                                on:change=move |event| form.kind.set(select_value(&event))>
                                {ASSET_KINDS.iter().map(|(kind, label)| view! {
                                    <option value=*kind>{*label}</option>
                                }).collect_view()}
                            </select></label>
                        <label class="field"><span>"Key"</span>
                            <input class="inp" placeholder="the tenant's own identifier"
                                prop:value=move || form.key.get()
                                on:input=move |event| form.key.set(input_value(&event))/></label>
                    </div>
                    <div>
                        <label class="field"><span>"Payload (a JSON object)"</span>
                            <textarea class="inp" rows="6" prop:value=move || form.payload.get()
                                on:input=move |event| form.payload.set(area_value(&event))></textarea></label>
                        <label class="field"><span>"Depends on — asset ids, comma separated"</span>
                            <input class="inp" placeholder="each must already be published in this tenant"
                                prop:value=move || form.depends_on.get()
                                on:input=move |event| form.depends_on.set(input_value(&event))/></label>
                    </div>
                </div>
                <button class="btn primary" on:click=submit>"Draft the asset"</button>
                <p class="muted">
                    "§5.7's order is enforced here, at the draft: a dependency that is not a published asset of this tenant refuses the write with 422 asset_dependency_unpublished, naming the asset."
                </p>
            </section>
            <section class="panel" style="margin-top:12px">
                <h2>"From intake to publication"</h2>
                <p class="muted">
                    "Intake records the brief and its documents; draft writes one asset citing that intake; review records the human decision an approved authoring.review gate carries; publish releases the reviewed asset. Every one is a governed action — this screen invokes them and refetches the read model after each write. No write here is optimistic and no refusal is hidden."
                </p>
            </section>
        </div>
    }
    .into_any()
}

// ── review & publish ───────────────────────────────────────────────────────────────────

fn review_panel(
    data: Data,
    selected: RwSignal<Option<String>>,
    ctx: AppCtx,
    refresh: Callback<()>,
    notice: RwSignal<Option<String>>,
) -> AnyView {
    let gates = data.gates.clone();
    let assets = data.assets.clone();
    let chosen = selected
        .get()
        .and_then(|id| find_asset(&assets, &id).cloned());
    let Some(asset) = chosen else {
        return view! {
            <section class="panel" style="margin-top:12px" data-testid="authoring-review">
                <div class="ph"><span>"Review &amp; publish"</span></div>
                <p class="muted">"Select a draft in the Draft catalogue to request its review gate, record the decision and publish it."</p>
            </section>
        }
        .into_any();
    };

    let id = field(&asset, "id").unwrap_or_default();
    let gate = gate_status(&gates, &id);
    let approved = approved_gate(&gates, &id);
    let asset_gates: Vec<Value> = gates
        .iter()
        .filter(|row| {
            field(row, "gate").as_deref() == Some(REVIEW_GATE)
                && field(row, "target").as_deref() == Some(id.as_str())
        })
        .cloned()
        .collect();

    let request_id = id.clone();
    let request = move |_event: web_sys::MouseEvent| {
        let target = request_id.clone();
        let description = "the authoring review gate for this asset".to_owned();
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "approvals.request",
                    json!({
                        "gate": REVIEW_GATE,
                        "kind": "publication",
                        "target": target,
                        "description": description,
                    }),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => {
                    notice.set(Some(
                        "Review gate requested; a second person decides it.".to_owned(),
                    ));
                    refresh.run(());
                }
                Err(error) => notice.set(Some(format!(
                    "Gate request refused: {}",
                    error_text(&error)
                ))),
            }
        });
    };

    let review_id = id.clone();
    let review_approval = approved.clone();
    let record = move |_event: web_sys::MouseEvent| {
        let asset_id = review_id.clone();
        let Some(approval_id) = review_approval.clone() else {
            notice.set(Some(
                "No approved review gate for this asset yet".to_owned(),
            ));
            return;
        };
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "authoring.review",
                    json!({ "asset_id": asset_id, "approval_id": approval_id }),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => {
                    notice.set(Some("Review recorded from the approved gate.".to_owned()));
                    refresh.run(());
                }
                Err(error) => notice.set(Some(format!("Review refused: {}", error_text(&error)))),
            }
        });
    };

    let publish_id = id.clone();
    let publish = move |_event: web_sys::MouseEvent| {
        let asset_id = publish_id.clone();
        leptos::task::spawn_local(async move {
            match ctx
                .invoke(
                    "authoring.publish",
                    json!({ "asset_id": asset_id }),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => {
                    notice.set(Some("Published; the release is recorded.".to_owned()));
                    refresh.run(());
                }
                Err(error) => notice.set(Some(format!("Publish refused: {}", error_text(&error)))),
            }
        });
    };

    let status = text(&asset, "status");
    let can_review = approved.is_some() && ctx.has_action("authoring.review");
    let can_publish = status == "reviewed" && ctx.has_action("authoring.publish");
    let can_request = ctx.has_action("approvals.request") && status == "draft";
    let review_disabled = !can_review;
    let publish_disabled = !can_publish;

    view! {
        <div class="grid-2" style="margin-top:12px" data-testid="authoring-review">
            <section class="panel">
                <div class="ph">
                    <span>{format!("{} · {}", text(&asset, "kind"), text(&asset, "key"))}</span>
                    <StatusPill status=status.clone()/>
                </div>
                <dl class="kv">
                    <dt>"Asset"</dt><dd class="mono">{short_id(Some(&id))}</dd>
                    <dt>"Intake"</dt><dd class="mono">{short_id(field(&asset, "intake_id").as_deref())}</dd>
                    <dt>"Version"</dt><dd>{int(&asset, "version").to_string()}</dd>
                    <dt>"Review gate"</dt><dd><StatusPill status=gate.to_owned()/></dd>
                    <dt>"Published"</dt><dd>{date(&asset, "published_at")}</dd>
                </dl>
                <div class="filterbar">
                    {(can_request).then(|| view! {
                        <button class="btn small" on:click=request>"Request the review gate"</button>
                    })}
                    {(approved.is_some()).then(|| view! {
                        <button class="btn small primary" disabled=review_disabled on:click=record>"Record the review"</button>
                    })}
                    {(ctx.has_action("authoring.publish")).then(|| view! {
                        <button class="btn small primary" disabled=publish_disabled on:click=publish>"Publish the asset"</button>
                    })}
                </div>
                <p class="muted">
                    "The requester cannot decide their own gate: a second person decides a listed gate, and the review then names that approved gate. Publication is a release, never a bypass: a draft or a rejected asset is refused, and so is an asset of this kind and key that is already published."
                </p>
            </section>
            <section class="panel">
                <div class="ph">
                    <span>"Review depth per asset"</span>
                    <span class="muted">"§5.7"</span>
                </div>
                <table class="table">
                    <thead><tr><th>"Asset"</th><th>"Reviewer"</th><th>"Publication is"</th></tr></thead>
                    <tbody>
                        {REVIEW_DEPTH.iter().map(|(kind, reviewer, release)| view! {
                            <tr><td>{*kind}</td><td>{*reviewer}</td><td>{*release}</td></tr>
                        }).collect_view()}
                    </tbody>
                </table>
                <div class="ph"><span>"Gates for this asset"</span></div>
                {if asset_gates.is_empty() {
                    view! { <p class="muted">"No review gate names this asset yet."</p> }.into_any()
                } else {
                    view! {
                        <ul class="steps">
                            {asset_gates.iter().map(|row| {
                                let decisions = row
                                    .get("decisions")
                                    .and_then(Value::as_array)
                                    .cloned()
                                    .unwrap_or_default();
                                view! {
                                    <li>
                                        <StatusPill status=text(row, "status")/>
                                        <span class="mono">{short_id(field(row, "id").as_deref())}</span>
                                        <span class="muted">{text(row, "description")}</span>
                                        {decisions.iter().map(|decision| view! {
                                            <div class="muted">
                                                {format!("{}: {} — {}", text(decision, "approver"), text(decision, "decision"), text(decision, "note"))}
                                            </div>
                                        }).collect_view()}
                                    </li>
                                }
                            }).collect_view()}
                        </ul>
                    }.into_any()
                }}
            </section>
        </div>
    }
    .into_any()
}

// ── dependency status ───────────────────────────────────────────────────────────────────

fn dependencies_panel(data: Data) -> AnyView {
    let counts: HashMap<String, (usize, usize)> = ASSET_KINDS
        .iter()
        .map(|(kind, _label)| {
            let rows: Vec<&Value> = data
                .assets
                .iter()
                .filter(|row| field(row, "kind").as_deref() == Some(*kind))
                .collect();
            let published = rows
                .iter()
                .filter(|row| field(row, "status").as_deref() == Some("published"))
                .count();
            ((*kind).to_owned(), (rows.len(), published))
        })
        .collect();
    let with_dependencies: Vec<Value> = data
        .assets
        .iter()
        .filter(|row| !ids_of(row, "depends_on").is_empty())
        .cloned()
        .collect();
    let blockers = blockers(&data.assets);
    let blocker_label = if blockers == 0 {
        "no blocker".to_owned()
    } else {
        format!("{blockers} blocker(s)")
    };
    let assets = data.assets.clone();

    view! {
        <section class="panel" style="margin-top:12px" data-testid="authoring-dependencies">
            <div class="ph">
                <span>"Dependency status"</span>
                <StatusPill status=blocker_label/>
            </div>
            <p class="muted">
                "§5.7's order: industry → department → job/capability → procedure → knowledge pack → policies/templates → tools. A draft may cite only an asset of this tenant that is already published; the check runs at the draft and again at the release."
            </p>
            <div class="tablewrap">
                <table class="table">
                    <thead><tr><th>"Kind"</th><th class="n">"Rows"</th><th class="n">"Published"</th><th>"Review"</th></tr></thead>
                    <tbody>
                        {ASSET_KINDS.iter().map(|(kind, label)| {
                            let (total, published) = counts.get(*kind).copied().unwrap_or((0, 0));
                            view! {
                                <tr>
                                    <td>{*label}</td>
                                    <td class="n">{total.to_string()}</td>
                                    <td class="n">{published.to_string()}</td>
                                    <td class="muted">"human — see Review & publish"</td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
            {if with_dependencies.is_empty() {
                view! {
                    <Empty message="No draft in this tenant cites another asset; authoring.draft's depends_on is what creates that edge.".to_owned()/>
                }.into_any()
            } else {
                view! {
                    <div class="tablewrap">
                        <table class="table">
                            <thead><tr><th>"Asset"</th><th>"Kind"</th><th>"Key"</th><th>"Dependencies"</th></tr></thead>
                            <tbody>
                                {with_dependencies.iter().map(|asset| {
                                    let lines = dependency_lines(&assets, asset);
                                    view! {
                                        <tr>
                                            <td class="mono">{short_id(field(asset, "id").as_deref())}</td>
                                            <td>{text(asset, "kind")}</td>
                                            <td class="mono">{text(asset, "key")}</td>
                                            <td>
                                                {lines.iter().map(|(id, status, published)| {
                                                    let mark = if *published { "✓" } else { "!" };
                                                    view! {
                                                        <div>
                                                            <span class="mono">{format!("{mark} {} · {}", short_id(Some(id)), status)}</span>
                                                        </div>
                                                    }
                                                }).collect_view()}
                                            </td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                    </div>
                }.into_any()
            }}
        </section>
    }
    .into_any()
}
