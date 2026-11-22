//! Authoring — the authored-asset catalogue (SPEC_V2 §10.5, destination **Authoring**; §5.7).
//!
//! One read and no write: `authoring.assets` returns a page of authored assets, narrowed by the
//! kind and status filters and refetched when either changes or Refresh is pressed. Every output
//! of the family is a draft behind a review gate. Intake, draft, review and publish are the
//! governed actions `authoring.intake`, `authoring.draft`, `authoring.review` and
//! `authoring.publish`; publication is a gate, never a write this screen performs. A chat message
//! or document can never grant a tool, widen an effect ceiling or change an approval policy.
//!
//! The table draws only what the read answers: kind, key, status, version and publication date.

use chrono::{DateTime, Utc};
use leptos::prelude::*;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{ApiError, InvokeOptions, Page};
use crate::components::{Empty, ErrorPanel, Planned, StatusPill, Tile};
use crate::state::AppCtx;

/// One authored asset as `authoring.assets` returns it; only the catalogue fields are kept.
#[derive(Clone, Debug, Deserialize)]
struct AssetRow {
    kind: String,
    key: String,
    status: String,
    #[serde(default)]
    version: i64,
    #[serde(default)]
    published_at: Option<DateTime<Utc>>,
}

fn parse<T: serde::de::DeserializeOwned>(value: Value, what: &str) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Network(format!("{what}: {error}")))
}

fn load(
    ctx: AppCtx,
    kind: RwSignal<Option<String>>,
    status: RwSignal<Option<String>>,
) -> LocalResource<Result<Vec<AssetRow>, ApiError>> {
    LocalResource::new(move || {
        // Read the tenant so a switch refetches under the new role.
        let _tenant = ctx.tenant.get();
        let kind = kind.get();
        let status = status.get();
        async move {
            let mut input = serde_json::Map::new();
            if let Some(kind) = kind {
                input.insert("kind".to_owned(), Value::String(kind));
            }
            if let Some(status) = status {
                input.insert("status".to_owned(), Value::String(status));
            }
            input.insert("limit".to_owned(), json!(100));
            let page: Page<AssetRow> = parse(
                ctx.invoke(
                    "authoring.assets",
                    Value::Object(input),
                    InvokeOptions::default(),
                )
                .await?
                .output,
                "authoring",
            )?;
            Ok(page.items)
        }
    })
}

/// The publication date as `YYYY-MM-DD`; `—` until the asset is published.
fn date(at: Option<DateTime<Utc>>) -> String {
    at.map(|at| at.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "—".to_owned())
}

/// The four tile counts over the returned page — total, then draft, reviewed and published — or
/// `None` until the read has answered (and when it failed; the panel below says why).
fn page_counts(items: &Option<Result<Vec<AssetRow>, ApiError>>) -> Option<[usize; 4]> {
    let Some(Ok(rows)) = items else {
        return None;
    };
    let count = |status: &str| rows.iter().filter(|row| row.status == status).count();
    Some([
        rows.len(),
        count("draft"),
        count("reviewed"),
        count("published"),
    ])
}

#[component]
pub fn Authoring() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let kind = RwSignal::new(Option::<String>::None);
    let status = RwSignal::new(Option::<String>::None);
    let assets = load(ctx, kind, status);

    view! {
        <section class="screen">
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
                            <h1>"Authoring"</h1>
                            <p class="muted">
                                "every output is a draft behind a review gate; a chat message or document can never grant a tool, widen an effect ceiling or change an approval policy."
                            </p>
                        </div>
                        <div class="filterbar">
                            <label class="filter">
                                <span>"Kind"</span>
                                <select
                                    aria-label="Kind"
                                    on:change=move |event| {
                                        let value = event_target_value(&event);
                                        kind.set((!value.is_empty()).then_some(value));
                                    }
                                >
                                    <option value="">"All"</option>
                                    <option value="industry">"industry"</option>
                                    <option value="department">"department"</option>
                                    <option value="job">"job"</option>
                                    <option value="capability">"capability"</option>
                                    <option value="procedure">"procedure"</option>
                                    <option value="knowledge_pack">"knowledge_pack"</option>
                                    <option value="project_template">"project_template"</option>
                                    <option value="policy">"policy"</option>
                                    <option value="tool_request">"tool_request"</option>
                                </select>
                            </label>
                            <label class="filter">
                                <span>"Status"</span>
                                <select
                                    aria-label="Status"
                                    on:change=move |event| {
                                        let value = event_target_value(&event);
                                        status.set((!value.is_empty()).then_some(value));
                                    }
                                >
                                    <option value="">"All"</option>
                                    <option value="draft">"draft"</option>
                                    <option value="reviewed">"reviewed"</option>
                                    <option value="rejected">"rejected"</option>
                                    <option value="published">"published"</option>
                                </select>
                            </label>
                            <button class="btn" on:click=move |_| assets.refetch()>"Refresh"</button>
                        </div>
                    </header>
                    {move || {
                        let counts = page_counts(&assets.get());
                        let (total, drafts, reviewed, published) = match counts {
                            Some([total, drafts, reviewed, published]) => (
                                total.to_string(),
                                drafts.to_string(),
                                reviewed.to_string(),
                                published.to_string(),
                            ),
                            None => (
                                "—".to_owned(),
                                "—".to_owned(),
                                "—".to_owned(),
                                "—".to_owned(),
                            ),
                        };
                        view! {
                            <div class="tiles">
                                <Tile label="Assets".to_owned() value=total detail="in the returned page".to_owned()/>
                                <Tile label="Drafts".to_owned() value=drafts detail="status draft in the returned page".to_owned()/>
                                <Tile label="Reviewed".to_owned() value=reviewed detail="status reviewed in the returned page".to_owned()/>
                                <Tile label="Published".to_owned() value=published detail="status published in the returned page".to_owned()/>
                            </div>
                        }
                    }}
                    {move || match assets.get() {
                        None => view! { <p class="muted">"Reading authored assets…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=Callback::new(move |_| assets.refetch())/>
                        }.into_any(),
                        Some(Ok(items)) if items.is_empty() => view! {
                            <Empty message="No asset in this tenant carries that kind and status.".to_owned()/>
                        }.into_any(),
                        Some(Ok(items)) => view! {
                            <div class="tablewrap">
                                <table class="table">
                                    <thead><tr>
                                        <th>"Kind"</th><th>"Key"</th><th>"Status"</th>
                                        <th class="n">"Version"</th><th>"Published at"</th>
                                    </tr></thead>
                                    <tbody>
                                        {items.iter().map(|asset| view! {
                                            <tr>
                                                <td><StatusPill status=asset.kind.clone()/></td>
                                                <td class="mono">{asset.key.clone()}</td>
                                                <td><StatusPill status=asset.status.clone()/></td>
                                                <td class="n">{asset.version.to_string()}</td>
                                                <td>{date(asset.published_at)}</td>
                                            </tr>
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        }.into_any(),
                    }}
                    <section class="panel">
                        <h2>"From intake to publication"</h2>
                        <p class="muted">
                            "Intake records the brief and its documents; draft writes one asset citing that intake; review records the human decision an approved approval carries; publish releases the reviewed asset. Those four are the governed actions authoring.intake, authoring.draft, authoring.review and authoring.publish — this screen performs none of them. It reads the catalogue, and publication happens only through its gate."
                        </p>
                    </section>
                }
                .into_any()
            }}
        </section>
    }
}
