//! Search — one ranked read over the three arms (S12 screen 8).
//!
//! The page is a reader of `search.query` and nothing else: no second query, no client-side
//! re-ranking, no invented score. Three rules are shown rather than hidden:
//!
//! - **A score is comparable only within one response.** The weights are the contract, the
//!   absolute values are not — the note rides on the count line instead of a tooltip nobody
//!   opens, because 0.8 here and 0.8 in the next answer are different instruments.
//! - **Only a requirement has a page.** A principle and an ADR are governed assets with a
//!   `kind` and no detail route, so their row shows the kind instead of linking somewhere
//!   that would 404; a catalogue entry is a label with no page at all.
//! - **An empty query is a state, not an error.** Nothing is sent until something is typed,
//!   so a blank page never reports "no results" for a question nobody asked.
//!
//! `kind` and `status` narrow the governed-asset arm only (a card has no status column and a
//! catalogue entry has no body), so the filter bar says so instead of implying otherwise.

use leptos::prelude::*;
use leptos_router::components::A;
use serde::Deserialize;
use serde_json::json;

use crate::api::{ApiError, InvokeOptions, Page};
use crate::components::{ErrorPanel, Planned, StatusPill, short_id};
use crate::state::AppCtx;

/// The arm vocabulary, exactly as `search.query` names it; the empty value searches all three.
const ARMS: [(&str, &str); 4] = [
    ("", "every arm"),
    ("governed_asset", "governed assets"),
    ("card", "cards"),
    ("catalogue_entry", "catalogue entries"),
];

/// `kind` and `status` narrow governed assets only — the other two arms ignore them.
const KINDS: [(&str, &str); 4] = [
    ("", "every kind"),
    ("requirement", "requirement"),
    ("principle", "principle"),
    ("adr", "adr"),
];
const STATUSES: [(&str, &str); 6] = [
    ("", "every status"),
    ("draft", "draft"),
    ("in_review", "in review"),
    ("approved", "approved"),
    ("superseded", "superseded"),
    ("archived", "archived"),
];

/// The group order: the arm that answers a design question first.
const GROUPS: [&str; 3] = ["governed_asset", "card", "catalogue_entry"];

/// The longest query the action accepts, after trimming (`MAX_Q_CHARS`).
const MAX_Q_CHARS: usize = 200;

/// One ranked hit, exactly as `SearchHitOut` writes it.
#[derive(Clone, Debug, Deserialize)]
struct SearchHit {
    entity_type: String,
    id: String,
    title: String,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    snippet: Option<String>,
    score: f64,
}

/// The group heading for an arm. A fourth value is drawn rather than dropped.
fn group_label(entity_type: &str) -> &'static str {
    match entity_type {
        "governed_asset" => "Governed assets",
        "card" => "Cards",
        "catalogue_entry" => "Catalogue entries",
        _ => "Other results",
    }
}

/// Bucket the response by `entity_type`, in [`GROUPS`] order, without losing a hit the
/// vocabulary does not name.
fn grouped(hits: &[SearchHit]) -> Vec<(String, Vec<SearchHit>)> {
    let mut groups: Vec<(String, Vec<SearchHit>)> = Vec::new();
    for entity_type in GROUPS {
        let rows: Vec<SearchHit> = hits
            .iter()
            .filter(|hit| hit.entity_type == entity_type)
            .cloned()
            .collect();
        if !rows.is_empty() {
            groups.push((entity_type.to_owned(), rows));
        }
    }
    for hit in hits
        .iter()
        .filter(|hit| !GROUPS.contains(&hit.entity_type.as_str()))
    {
        match groups
            .iter_mut()
            .find(|(entity_type, _)| *entity_type == hit.entity_type)
        {
            Some((_, rows)) => rows.push(hit.clone()),
            None => groups.push((hit.entity_type.clone(), vec![hit.clone()])),
        }
    }
    groups
}

/// `ts_headline` marks its match with `<b>`; the row is text, not markup, so the markers are
/// stripped rather than escaped into the page.
fn plain(snippet: &str) -> String {
    snippet.replace("<b>", "").replace("</b>", "")
}

fn parse<T: serde::de::DeserializeOwned>(
    value: serde_json::Value,
    what: &str,
) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Network(format!("{what}: {error}")))
}

fn load(
    ctx: AppCtx,
    query: RwSignal<String>,
    arm: RwSignal<String>,
    kind: RwSignal<String>,
    status: RwSignal<String>,
) -> LocalResource<Result<Vec<SearchHit>, ApiError>> {
    LocalResource::new(move || {
        // Every reactive input is read before the first await.
        let _tenant = ctx.tenant.get();
        let q = query.get();
        let arm = arm.get();
        let kind = kind.get();
        let status = status.get();
        async move {
            if q.is_empty() {
                return Ok(Vec::new());
            }
            // `PageIn` is flattened into the action input, so `limit` is top-level. Only
            // `limit` is honoured: a ranked page has no stable cursor to continue from.
            let mut input = json!({"q": q, "limit": 50});
            if !arm.is_empty() {
                input["types"] = json!([arm]);
            }
            if !kind.is_empty() {
                input["kind"] = json!(kind);
            }
            if !status.is_empty() {
                input["status"] = json!(status);
            }
            let page: Page<SearchHit> = parse(
                ctx.invoke("search.query", input, InvokeOptions::default())
                    .await?
                    .output,
                "search",
            )?;
            Ok(page.items)
        }
    })
}

#[component]
pub fn SearchScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let draft = RwSignal::new(String::new());
    let query = RwSignal::new(String::new());
    let arm = RwSignal::new(String::new());
    let kind = RwSignal::new(String::new());
    let status = RwSignal::new(String::new());
    let notice = RwSignal::new(Option::<String>::None);
    let data = load(ctx, query, arm, kind, status);
    let retry = Callback::new(move |_| data.refetch());

    let on_submit = move |event: web_sys::SubmitEvent| {
        event.prevent_default();
        let raw = draft.get_untracked();
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            query.set(String::new());
            notice.set(Some("Type at least one character to search.".to_owned()));
            return;
        }
        let length = trimmed.chars().count();
        if length > MAX_Q_CHARS {
            notice.set(Some(format!(
                "The query is {length} characters; search.query accepts at most {MAX_Q_CHARS} after trimming."
            )));
            return;
        }
        notice.set(None);
        // An unchanged query must still re-run: Search is also the retry gesture.
        let unchanged = query.get_untracked() == trimmed;
        query.set(trimmed.to_owned());
        if unchanged {
            data.refetch();
        }
    };

    view! {
        <section class="screen measure">
            {move || {
                if !ctx.has_action("search.query") {
                    return view! {
                        <Planned
                            title="Search is planned".to_owned()
                            detail="search.query is not in this role's manifest. The page appears when the action lands, not before.".to_owned()
                        />
                    }
                    .into_any();
                }
                view! {
                    <header class="screen-head">
                        <div>
                            <h1>"Search"</h1>
                            <p class="muted">
                                "One ranked read over governed assets, cards and catalogue entries. A score is comparable only within one response."
                            </p>
                        </div>
                    </header>
                    <form class="filterbar" on:submit=on_submit>
                        <label class="filter">
                            <span>"Query"</span>
                            <input
                                type="text"
                                aria-label="Search query"
                                maxlength="200"
                                placeholder="A title, a key, a phrase"
                                prop:value=move || draft.get()
                                on:input=move |event| draft.set(event_target_value(&event))
                            />
                        </label>
                        <label class="filter">
                            <span>"Arm"</span>
                            <select
                                aria-label="Search arm"
                                on:change=move |event| arm.set(event_target_value(&event))
                            >
                                {ARMS.iter().map(|(value, label)| {
                                    let value = *value;
                                    view! {
                                        <option value=value selected=move || arm.get() == value>{*label}</option>
                                    }
                                }).collect_view()}
                            </select>
                        </label>
                        <label class="filter">
                            <span>"Asset kind"</span>
                            <select
                                aria-label="Governed asset kind"
                                on:change=move |event| kind.set(event_target_value(&event))
                            >
                                {KINDS.iter().map(|(value, label)| {
                                    let value = *value;
                                    view! {
                                        <option value=value selected=move || kind.get() == value>{*label}</option>
                                    }
                                }).collect_view()}
                            </select>
                        </label>
                        <label class="filter">
                            <span>"Asset status"</span>
                            <select
                                aria-label="Governed asset status"
                                on:change=move |event| status.set(event_target_value(&event))
                            >
                                {STATUSES.iter().map(|(value, label)| {
                                    let value = *value;
                                    view! {
                                        <option value=value selected=move || status.get() == value>{*label}</option>
                                    }
                                }).collect_view()}
                            </select>
                        </label>
                        <button class="btn primary" type="submit">"Search"</button>
                    </form>
                    <p class="hint">
                        "search.query — kind and status narrow the governed-asset arm only; cards and catalogue entries ignore them"
                    </p>
                    {move || notice.get().map(|message| view! {
                        <div class="callout warn" role="status"><p>{message}</p></div>
                    })}
                    {move || {
                        if query.get().is_empty() {
                            return view! {
                                <div class="panel" role="status">
                                    <h2>"Type to search"</h2>
                                    <p class="muted">
                                        "Nothing is sent until there is something to match. A card opens its cockpit, a requirement opens its page, and a catalogue entry is a label with no page of its own."
                                    </p>
                                </div>
                            }
                            .into_any();
                        }
                        match data.get() {
                            None => view! { <p class="muted">"Ranking the three arms…"</p> }.into_any(),
                            Some(Err(error)) => view! {
                                <ErrorPanel error=error on_retry=retry/>
                            }.into_any(),
                            Some(Ok(hits)) => {
                                if hits.is_empty() {
                                    view! {
                                        <div class="panel" role="status">
                                            <h2>"No results"</h2>
                                            <p class="muted">
                                                {format!("Nothing in this tenant matches “{}” with these filters.", query.get())}
                                            </p>
                                        </div>
                                    }
                                    .into_any()
                                } else {
                                    view! { <Results hits=hits query=query.get()/> }.into_any()
                                }
                            }
                        }
                    }}
                }
                .into_any()
            }}
        </section>
    }
}

#[component]
fn Results(hits: Vec<SearchHit>, query: String) -> impl IntoView {
    let total = hits.len();
    let groups = grouped(&hits);
    view! {
        <p class="muted summary">
            {format!(
                "{total} {} for “{query}” · the score is comparable only within this response, never across queries",
                if total == 1 { "result" } else { "results" }
            )}
        </p>
        {groups.into_iter().map(|(entity_type, rows)| {
            let count = rows.len();
            view! {
                <section class="panel">
                    <div class="panel-head">
                        <h2>{group_label(&entity_type)}</h2>
                        <span class="muted">
                            {format!("{count} {}", if count == 1 { "hit" } else { "hits" })}
                        </span>
                    </div>
                    <table class="wtbl">
                        <thead>
                            <tr>
                                <th>"Title"</th>
                                <th>"Kind"</th>
                                <th>"Status"</th>
                                <th>"Excerpt"</th>
                                <th class="n" title="Comparable only within this response">"Score"</th>
                                <th class="adv">"Id"</th>
                            </tr>
                        </thead>
                        <tbody>
                            {rows.into_iter().map(|hit| view! { <HitRow hit=hit/> }).collect_view()}
                        </tbody>
                    </table>
                </section>
            }
        }).collect_view()}
    }
}

/// One hit. Whether the title is a link is the only routing decision the screen makes, and
/// it is made from the hit's own `entity_type`/`kind` — never from a guess.
#[component]
fn HitRow(hit: SearchHit) -> impl IntoView {
    let title = hit.title.clone();
    let id = hit.id.clone();
    let title_cell = match (hit.entity_type.as_str(), hit.kind.as_deref()) {
        ("governed_asset", Some("requirement")) => view! {
            <A href=format!("/design/requirements/{id}")>{title.clone()}</A>
        }
        .into_any(),
        ("card", _) => view! {
            <A href=format!("/card/{id}")>{title.clone()}</A>
        }
        .into_any(),
        _ => view! { <span>{title.clone()}</span> }.into_any(),
    };
    let key = hit.key.clone();
    let kind = hit.kind.clone().unwrap_or_else(|| "—".to_owned());
    let status = hit.status.clone();
    let excerpt = hit
        .snippet
        .as_deref()
        .map(plain)
        .filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| "—".to_owned());
    let score = hit.score;
    let short = short_id(Some(&hit.id));
    view! {
        <tr>
            <td>
                {title_cell}
                {key.map(|key| view! { <div class="mono">{key}</div> })}
            </td>
            <td>{kind}</td>
            <td>{status.map(|status| view! { <StatusPill status=status/> })}</td>
            <td>{excerpt}</td>
            <td class="n">{format!("{score:.2}")}</td>
            <td class="adv mono">{short}</td>
        </tr>
    }
}
