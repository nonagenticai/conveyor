//! Requirements — governed assets, their revisions and trace links (S12 screen 5).
//!
//! A requirement is a governed asset: one row, a status machine, and an append-only revision
//! history. The screen shows the three things the data model makes true and hides nothing:
//!
//! - **An edit is a revision.** Saving writes with `If-Match` on the asset's version; a stale
//!   version is a 412 the screen reports instead of overwriting someone.
//! - **A transition is status-guarded, not version-guarded.** The permitted moves are drawn
//!   from the current status, and the API's refusal is shown verbatim when it disagrees.
//! - **A trace link is the only connection to a card.** The link panel lists the edges for
//!   this asset and creates them through `trace_links.create`; no second relation is invented.

use chrono::{DateTime, Utc};
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use serde::Deserialize;
use serde_json::{Value, json};
use wasm_bindgen::JsCast;

use crate::api::{ApiError, InvokeOptions, Page};
use crate::components::{Empty, ErrorPanel, Planned, StatusPill, age, short_id};
use crate::state::AppCtx;

/// The statuses the editor offers as a filter. The set is the domain's, not a UI invention.
const STATUSES: [(&str, &str); 4] = [
    ("draft", "Draft"),
    ("in_review", "In review"),
    ("approved", "Approved"),
    ("superseded", "Superseded"),
];

#[derive(Clone, Debug, Deserialize)]
struct Asset {
    id: String,
    key: String,
    title: String,
    #[serde(default)]
    body: String,
    status: String,
    version: i64,
    #[serde(default)]
    superseded_by_id: Option<String>,
    #[serde(default)]
    updated_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize)]
struct Revision {
    seq: i32,
    label: String,
    status: String,
    #[serde(default)]
    author_id: Option<String>,
    #[serde(default)]
    content_hash: String,
    #[serde(default)]
    created_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize)]
struct AssetDetail {
    #[serde(flatten)]
    asset: Asset,
    #[serde(default)]
    revisions: Vec<Revision>,
}

#[derive(Clone, Debug, Deserialize)]
struct FieldChange {
    field: String,
    #[serde(default)]
    from: Option<Value>,
    #[serde(default)]
    to: Option<Value>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct RevisionDiff {
    #[serde(default)]
    added: Vec<FieldChange>,
    #[serde(default)]
    removed: Vec<FieldChange>,
    #[serde(default)]
    changed: Vec<FieldChange>,
}

#[derive(Clone, Debug, Deserialize)]
struct TraceLink {
    id: String,
    to_id: String,
    kind: String,
    #[serde(default)]
    created_at: Option<DateTime<Utc>>,
}

#[derive(Clone)]
struct ListData {
    items: Vec<Asset>,
}

#[derive(Clone)]
struct DetailData {
    detail: AssetDetail,
    links: Vec<TraceLink>,
}

fn parse<T: serde::de::DeserializeOwned>(value: Value, what: &str) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Network(format!("{what}: {error}")))
}

fn load_list(
    ctx: AppCtx,
    status: RwSignal<Option<String>>,
) -> LocalResource<Result<ListData, ApiError>> {
    LocalResource::new(move || {
        // Every reactive input is read before the first await.
        let _tenant = ctx.tenant.get();
        let filter = status.get();
        let client = ctx.client();
        async move {
            let mut input = json!({"limit": 50});
            if let Some(filter) = filter {
                input["status"] = json!(filter);
            }
            let page: Page<Asset> = parse(
                client
                    .invoke("requirements.list", input, InvokeOptions::default())
                    .await?
                    .output,
                "requirements",
            )?;
            Ok(ListData { items: page.items })
        }
    })
}

fn load_detail<F>(ctx: AppCtx, id: F) -> LocalResource<Result<DetailData, ApiError>>
where
    F: Fn() -> String + 'static,
{
    LocalResource::new(move || {
        let id = id();
        let _tenant = ctx.tenant.get();
        let client = ctx.client();
        async move {
            if id.is_empty() {
                return Err(ApiError::NotFound("no requirement in the route".to_owned()));
            }
            let detail: AssetDetail = parse(
                client
                    .invoke(
                        "requirements.get",
                        json!({"id": id}),
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                "requirement",
            )?;
            let links: Page<TraceLink> = parse(
                client
                    .invoke(
                        "trace_links.list",
                        json!({"from_type": "governed_asset", "from_id": id, "limit": 100}),
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                "trace links",
            )?;
            Ok(DetailData {
                detail,
                links: links.items,
            })
        }
    })
}

#[component]
pub fn RequirementsScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let status = RwSignal::new(Option::<String>::None);
    let data = load_list(ctx, status);
    let retry = Callback::new(move |_| data.refetch());

    let title = RwSignal::new(String::new());
    let body = RwSignal::new(String::new());
    let notice = RwSignal::new(Option::<String>::None);
    let busy = RwSignal::new(false);

    let on_create = move |event: web_sys::SubmitEvent| {
        event.prevent_default();
        let title_value = title.get_untracked().trim().to_owned();
        if title_value.is_empty() {
            notice.set(Some("A title is required.".into()));
            return;
        }
        let body_value = body.get_untracked().trim().to_owned();
        let mut input = json!({"title": title_value});
        if !body_value.is_empty() {
            input["body"] = json!(body_value);
        }
        let ctx = ctx;
        busy.set(true);
        leptos::task::spawn_local(async move {
            match ctx
                .invoke("requirements.create", input, InvokeOptions::default())
                .await
            {
                Ok(_) => {
                    title.set(String::new());
                    body.set(String::new());
                    notice.set(Some("Requirement created as a draft.".into()));
                    data.refetch();
                }
                Err(error) => notice.set(Some(error.to_string())),
            }
            busy.set(false);
        });
    };

    view! {
        <section class="screen">
            {move || {
                if !ctx.has_action("requirements.list") {
                    return view! {
                        <Planned
                            title="Requirements is planned".to_owned()
                            detail="requirements.list is not in this role's manifest, so there is no register to show.".to_owned()
                        />
                    }
                    .into_any();
                }
                view! {
                    <header class="screen-head">
                        <div>
                            <h1>"Requirements"</h1>
                            <p class="muted">"The governed register: one key per requirement, every edit a revision."</p>
                        </div>
                        <div class="filterbar">
                            <label class="filter">
                                <span>"Status"</span>
                                <select
                                    on:change=move |event| {
                                        let value = event_target_value(&event);
                                        status.set(if value.is_empty() { None } else { Some(value) });
                                    }
                                    aria-label="Filter by status"
                                >
                                    <option value="">"Every status"</option>
                                    {STATUSES.iter().map(|(value, label)| view! {
                                        <option value=*value>{*label}</option>
                                    }).collect_view()}
                                </select>
                            </label>
                            <button class="btn" on:click=move |_| data.refetch()>"Refresh"</button>
                        </div>
                    </header>
                    {move || notice.get().map(|message| view! {
                        <div class="callout warn" role="status"><p>{message}</p></div>
                    })}
                    {move || match data.get() {
                        None => view! { <p class="muted">"Reading the register…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=retry/>
                        }.into_any(),
                        Some(Ok(body_data)) => {
                            if body_data.items.is_empty() {
                                view! {
                                    <Empty message="No requirement matches this filter. Create one below.".to_owned()/>
                                }.into_any()
                            } else {
                                view! {
                                    <div style="overflow-x:auto">
                                    <table class="table">
                                        <thead>
                                            <tr>
                                                <th>"Key"</th>
                                                <th>"Title"</th>
                                                <th>"Status"</th>
                                                <th class="num">"Version"</th>
                                                <th>"Updated"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {body_data.items.into_iter().map(|asset| view! {
                                                <tr>
                                                    <td class="mono">{asset.key.clone()}</td>
                                                    <td>
                                                        <A href=format!("/design/requirements/{}", asset.id)>
                                                            {asset.title.clone()}
                                                        </A>
                                                    </td>
                                                    <td><StatusPill status=asset.status.clone()/></td>
                                                    <td class="num">{asset.version}</td>
                                                    <td>{age(asset.updated_at)}</td>
                                                </tr>
                                            }).collect_view()}
                                        </tbody>
                                    </table>
                                    </div>
                                }.into_any()
                            }
                        }
                    }}
                    {move || ctx.has_action("requirements.create").then(|| view! {
                        <form class="panel" on:submit=on_create>
                            <h2>"New requirement"</h2>
                            <p class="muted">
                                "It starts as a draft. The key is derived from the title; an edit becomes a new revision."
                            </p>
                            <div class="formrow">
                                <label class="filter">
                                    <span>"Title"</span>
                                    <input
                                        prop:value=move || title.get()
                                        on:input=move |event| title.set(event_target_value(&event))
                                        placeholder="What must be true"
                                    />
                                </label>
                                <button class="btn primary" type="submit" disabled=move || busy.get()>
                                    {move || if busy.get() { "Creating…" } else { "Create draft" }}
                                </button>
                            </div>
                            <label class="filter" style="margin-top:8px">
                                <span>"Body"</span>
                                <textarea
                                    rows="3"
                                    prop:value=move || body.get()
                                    on:input=move |event| body.set(event_target_value(&event))
                                    placeholder="The statement, its rationale, its acceptance criteria."
                                ></textarea>
                            </label>
                        </form>
                    })}
                }
                .into_any()
            }}
        </section>
    }
}

#[component]
pub fn RequirementDetail() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let params = use_params_map();
    let id = move || params.get().get("id").unwrap_or_default();
    let data = load_detail(ctx, id);
    let refresh = Callback::new(move |_| data.refetch());

    view! {
        <section class="screen">
            {move || {
                if id().is_empty() {
                    return view! { <p class="muted">"No requirement in the route."</p> }.into_any();
                }
                view! {
                    <A href="/design/requirements">"← All requirements"</A>
                    {move || match data.get() {
                        None => view! { <p class="muted">"Reading the requirement…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=refresh/>
                        }.into_any(),
                        Some(Ok(body)) => view! {
                            <RequirementBody data=body refresh=refresh/>
                        }.into_any(),
                    }}
                }
                .into_any()
            }}
        </section>
    }
}

#[component]
fn RequirementBody(data: DetailData, refresh: Callback<()>) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let asset = data.detail.asset.clone();
    let revisions = data.detail.revisions.clone();
    let links = data.links.clone();
    let asset_id = asset.id.clone();
    let version = asset.version;
    let status = asset.status.clone();

    let notice = RwSignal::new(Option::<String>::None);
    let title = RwSignal::new(asset.title.clone());
    let body = RwSignal::new(asset.body.clone());
    let busy = RwSignal::new(false);

    let on_save = Callback::new({
        let asset_id = asset_id.clone();
        move |event: web_sys::SubmitEvent| {
            event.prevent_default();
            let title_value = title.get_untracked().trim().to_owned();
            if title_value.is_empty() {
                notice.set(Some("A title is required.".into()));
                return;
            }
            let mut input = json!({"id": asset_id, "title": title_value});
            input["body"] = json!(body.get_untracked());
            let ctx = ctx;
            busy.set(true);
            leptos::task::spawn_local(async move {
                match ctx
                    .invoke(
                        "requirements.update",
                        input,
                        InvokeOptions {
                            if_match: Some(version),
                            ..InvokeOptions::default()
                        },
                    )
                    .await
                {
                    Ok(_) => {
                        notice.set(Some("Saved as a new revision.".into()));
                        refresh.run(());
                    }
                    Err(error) => notice.set(Some(error.to_string())),
                }
                busy.set(false);
            });
        }
    });

    // The moves the status machine permits, drawn from the asset's own status. A move the API
    // still refuses surfaces verbatim; the UI never pretends to know the whole guard.
    let allowed: Vec<(&'static str, &'static str)> = match status.as_str() {
        "draft" => vec![("in_review", "Submit for review"), ("archived", "Archive")],
        "in_review" => vec![("approved", "Approve"), ("draft", "Return to draft")],
        "approved" => vec![("superseded", "Supersede"), ("archived", "Archive")],
        _ => Vec::new(),
    };

    view! {
        <header class="screen-head" style="margin-top:8px">
            <div>
                <h1>{asset.title.clone()}</h1>
                <p class="muted">
                    <span class="mono">{asset.key.clone()}</span>
                    {" · "}
                    <StatusPill status=status.clone()/>
                    {" · "}
                    {format!("version {version}")}
                    {asset.updated_at.map(|at| format!(" · updated {}", age(Some(at))))}
                    {asset.superseded_by_id.clone().map(|id| view! {
                        {" · superseded by "}
                        <A href=format!("/design/requirements/{id}")>{short_id(Some(&id))}</A>
                    })}
                </p>
            </div>
        </header>
        {move || notice.get().map(|message| view! {
            <div class="callout warn" role="status"><p>{message}</p></div>
        })}
        <div class="split">
            <div>
                <section class="panel">
                    <div class="ph"><span>"Statement"</span></div>
                    {move || ctx.has_action("requirements.update").then(|| view! {
                        <form on:submit=move |event| on_save.run(event)>
                            <label class="filter">
                                <span>"Title"</span>
                                <input
                                    prop:value=move || title.get()
                                    on:input=move |event| title.set(event_target_value(&event))
                                />
                            </label>
                            <label class="filter" style="margin-top:8px">
                                <span>"Body"</span>
                                <textarea
                                    rows="8"
                                    prop:value=move || body.get()
                                    on:input=move |event| body.set(event_target_value(&event))
                                ></textarea>
                            </label>
                            <button class="btn primary" type="submit" disabled=move || busy.get() style="margin-top:8px">
                                {move || if busy.get() { "Saving…" } else { "Save revision" }}
                            </button>
                        </form>
                    })}
                    {move || (!ctx.has_action("requirements.update")).then(|| view! {
                        <p class="muted">{asset.body.clone()}</p>
                    })}
                </section>

                <section class="panel" style="margin-top:12px">
                    <div class="ph"><span>"Revisions"</span><span class="muted">{revisions.len()}</span></div>
                    {if revisions.is_empty() {
                        view! { <Empty message="No revision has been written yet.".to_owned()/> }.into_any()
                    } else {
                        view! {
                            <div style="overflow-x:auto">
                            <table class="table">
                                <thead>
                                    <tr>
                                        <th class="num">"Seq"</th>
                                        <th>"Label"</th>
                                        <th>"Status"</th>
                                        <th>"Author"</th>
                                        <th>"Hash"</th>
                                        <th>"When"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {revisions.iter().map(|revision| view! {
                                        <tr>
                                            <td class="num mono">{revision.seq}</td>
                                            <td>{revision.label.clone()}</td>
                                            <td><StatusPill status=revision.status.clone()/></td>
                                            <td class="mono">{short_id(revision.author_id.as_deref())}</td>
                                            <td class="mono">{short_id(Some(&revision.content_hash))}</td>
                                            <td>{age(revision.created_at)}</td>
                                        </tr>
                                    }).collect_view()}
                                </tbody>
                            </table>
                            </div>
                        }.into_any()
                    }}
                </section>

                <ComparePanel asset_id=asset_id.clone() revisions=revisions.clone()/>
                <TracePanel asset_id=asset_id.clone() links=links refresh=refresh notice=notice/>
            </div>
            <div>
                <section class="panel">
                    <div class="ph"><span>"Status"</span></div>
                    {if allowed.is_empty() {
                        view! { <p class="muted">"This requirement's status has no forward move in the register."</p> }.into_any()
                    } else {
                        view! {
                            <div style="display:flex;flex-direction:column;gap:6px">
                                {allowed.iter().map(|(to, label)| {
                                    let to = *to;
                                    let label = *label;
                                    let asset_id = asset_id.clone();
                                    view! {
                                        <button
                                            class="btn"
                                            on:click=move |_| transition(ctx, refresh, notice, asset_id.clone(), to)
                                        >{label}</button>
                                    }
                                }).collect_view()}
                            </div>
                        }.into_any()
                    }}
                    <p class="muted" style="margin-top:10px">
                        "A supersede needs a successor; the API refuses one without it. The refusal is shown above."
                    </p>
                </section>
            </div>
        </div>
    }
}

fn transition(
    ctx: AppCtx,
    refresh: Callback<()>,
    notice: RwSignal<Option<String>>,
    asset_id: String,
    to: &'static str,
) {
    leptos::task::spawn_local(async move {
        match ctx
            .invoke(
                "requirements.transition",
                json!({"id": asset_id, "to": to}),
                InvokeOptions::default(),
            )
            .await
        {
            Ok(_) => refresh.run(()),
            Err(error) => notice.set(Some(error.to_string())),
        }
    });
}

#[component]
fn ComparePanel(asset_id: String, revisions: Vec<Revision>) -> impl IntoView {
    let seqs: Vec<i32> = revisions.iter().map(|revision| revision.seq).collect();
    let newest = seqs.first().copied().unwrap_or(0);
    let oldest = seqs.last().copied().unwrap_or(0);
    let from = RwSignal::new(oldest);
    let to = RwSignal::new(newest);
    let diff = RwSignal::new(Option::<Result<RevisionDiff, String>>::None);
    let ctx = expect_context::<AppCtx>();
    let has_two = seqs.len() >= 2;
    let seqs_for_disabled = seqs.clone();
    let seqs_from = seqs.clone();
    let seqs_to = seqs.clone();

    let compare = Callback::new({
        let asset_id = asset_id.clone();
        move |_: ()| {
            let asset_id = asset_id.clone();
            let from = from.get_untracked();
            let to = to.get_untracked();
            leptos::task::spawn_local(async move {
                match ctx
                    .invoke(
                        "requirements.compare",
                        json!({"id": asset_id, "from": from, "to": to}),
                        InvokeOptions::default(),
                    )
                    .await
                {
                    Ok(invocation) => {
                        match parse::<RevisionDiff>(invocation.output, "revision diff") {
                            Ok(parsed) => diff.set(Some(Ok(parsed))),
                            Err(error) => diff.set(Some(Err(error.to_string()))),
                        }
                    }
                    Err(error) => diff.set(Some(Err(error.to_string()))),
                }
            });
        }
    });

    view! {
        <section class="panel" style="margin-top:12px">
            <div class="ph">
                <span>"Compare revisions"</span>
                <button class="btn small" on:click=move |_| compare.run(()) disabled=move || seqs_for_disabled.len() < 2>"Compare"</button>
            </div>
            {if !has_two {
                view! { <p class="muted">"Two revisions are needed before a diff exists."</p> }.into_any()
            } else {
                view! {
                    <div class="formrow">
                        <label class="filter">
                            <span>"From"</span>
                            <select on:change=move |event| {
                                if let Ok(value) = event_target_value(&event).parse::<i32>() {
                                    from.set(value);
                                }
                            }>
                                {seqs_from.into_iter().map(|seq| view! {
                                    <option value=seq.to_string() selected=move || from.get() == seq>{format!("seq {seq}")}</option>
                                }).collect_view()}
                            </select>
                        </label>
                        <label class="filter">
                            <span>"To"</span>
                            <select on:change=move |event| {
                                if let Ok(value) = event_target_value(&event).parse::<i32>() {
                                    to.set(value);
                                }
                            }>
                                {seqs_to.into_iter().map(|seq| view! {
                                    <option value=seq.to_string() selected=move || to.get() == seq>{format!("seq {seq}")}</option>
                                }).collect_view()}
                            </select>
                        </label>
                    </div>
                    {move || match diff.get() {
                        None => view! { <p class="muted">"Choose two revisions and compare."</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <div class="panel problem" role="alert"><p>{error}</p></div>
                        }.into_any(),
                        Some(Ok(diff)) => {
                            let empty = diff.added.is_empty() && diff.removed.is_empty() && diff.changed.is_empty();
                            if empty {
                                view! { <Empty message="These revisions are identical.".to_owned()/> }.into_any()
                            } else {
                                view! {
                                    <div style="overflow-x:auto">
                                    <table class="table">
                                        <thead><tr><th>"Field"</th><th>"Before"</th><th>"After"</th></tr></thead>
                                        <tbody>
                                            {diff.added.into_iter().map(|change| view! {
                                                <tr><td class="mono">{change.field}</td><td class="muted">"—"</td><td>{render_value(change.to)}</td></tr>
                                            }).collect_view()}
                                            {diff.removed.into_iter().map(|change| view! {
                                                <tr><td class="mono">{change.field}</td><td>{render_value(change.from)}</td><td class="muted">"—"</td></tr>
                                            }).collect_view()}
                                            {diff.changed.into_iter().map(|change| view! {
                                                <tr><td class="mono">{change.field}</td><td>{render_value(change.from)}</td><td>{render_value(change.to)}</td></tr>
                                            }).collect_view()}
                                        </tbody>
                                    </table>
                                    </div>
                                }.into_any()
                            }
                        }
                    }}
                }.into_any()
            }}
        </section>
    }
}

fn render_value(value: Option<Value>) -> String {
    match value {
        None => "—".to_owned(),
        Some(Value::String(text)) => text,
        Some(value) => value.to_string(),
    }
}

#[component]
fn TracePanel(
    asset_id: String,
    links: Vec<TraceLink>,
    refresh: Callback<()>,
    notice: RwSignal<Option<String>>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let card_id = RwSignal::new(String::new());
    let kind = RwSignal::new("traces".to_owned());

    let on_link = Callback::new({
        let asset_id = asset_id.clone();
        move |event: web_sys::SubmitEvent| {
            event.prevent_default();
            let to_id = card_id.get_untracked().trim().to_owned();
            if to_id.is_empty() {
                notice.set(Some("A card id is required.".into()));
                return;
            }
            let asset_id = asset_id.clone();
            let kind_value = kind.get_untracked();
            let ctx = ctx;
            leptos::task::spawn_local(async move {
                match ctx
                    .invoke(
                        "trace_links.create",
                        json!({
                            "from_type": "governed_asset",
                            "from_id": asset_id,
                            "to_type": "card",
                            "to_id": to_id,
                            "kind": kind_value,
                        }),
                        InvokeOptions::default(),
                    )
                    .await
                {
                    Ok(_) => {
                        card_id.set(String::new());
                        notice.set(Some("Trace link written.".into()));
                        refresh.run(());
                    }
                    Err(error) => notice.set(Some(error.to_string())),
                }
            });
        }
    });

    view! {
        <section class="panel" style="margin-top:12px">
            <div class="ph"><span>"Trace links"</span><span class="muted">{links.len()}</span></div>
            {if links.is_empty() {
                view! { <Empty message="No card traces this requirement yet.".to_owned()/> }.into_any()
            } else {
                view! {
                    <div style="overflow-x:auto">
                    <table class="table">
                        <thead><tr><th>"Link"</th><th>"Kind"</th><th>"Card"</th><th>"When"</th><th></th></tr></thead>
                        <tbody>
                            {links.into_iter().map(|link| {
                                let id = link.id.clone();
                                view! {
                                    <tr>
                                        <td class="mono">{short_id(Some(&link.id))}</td>
                                        <td>{link.kind.clone()}</td>
                                        <td class="mono">
                                            <A href=format!("/card/{}", link.to_id)>{short_id(Some(&link.to_id))}</A>
                                        </td>
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
                <form class="formrow" on:submit=move |event| on_link.run(event) style="margin-top:8px">
                    <label class="filter">
                        <span>"Card id"</span>
                        <input
                            prop:value=move || card_id.get()
                            on:input=move |event| card_id.set(event_target_value(&event))
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
                    <button class="btn" type="submit">"Add link"</button>
                </form>
            })}
            <p class="muted" style="margin-top:8px">
                {format!("asset {}", short_id(Some(&asset_id)))}
                {" · the link is the only relation between a requirement and a card."}
            </p>
        </section>
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
