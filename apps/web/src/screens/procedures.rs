//! Procedures — the procedure catalogue (SPEC_V2 §10.5, destination **Procedures**).
//!
//! One read and no write: `procedures.list` returns a page of procedure versions, narrowed by
//! the status filter (`draft | published | retired`) and refetched when that filter changes or
//! Refresh is pressed. A procedure is the executable form of an SOP — its definition of ready,
//! definition of done and acceptance criteria are the judge's contract, and an unreviewed
//! SOP-derived procedure refuses publication.
//!
//! The table draws only what the read answers: key, title, status and version. SPEC_V2 §10.5's
//! SOP intake, importers, PDDL step editor, knowledge-pack pairing and publish are governed
//! actions, not controls this read-only screen invents.

use leptos::prelude::*;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{ApiError, InvokeOptions, Page};
use crate::components::{Empty, ErrorPanel, Planned, StatusPill, Tile};
use crate::state::AppCtx;

/// One procedure version as `procedures.list` returns it; only the catalogue fields are kept.
#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
struct ProcedureRow {
    id: String,
    key: String,
    title: String,
    status: String,
    #[serde(default)]
    version: i64,
}

fn parse<T: serde::de::DeserializeOwned>(value: Value, what: &str) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Network(format!("{what}: {error}")))
}

fn load(
    ctx: AppCtx,
    status: RwSignal<Option<String>>,
) -> LocalResource<Result<Vec<ProcedureRow>, ApiError>> {
    LocalResource::new(move || {
        // Read the tenant so a switch refetches under the new role.
        let _tenant = ctx.tenant.get();
        let status = status.get();
        async move {
            let input = match status {
                Some(status) => json!({"status": status, "limit": 100}),
                None => json!({"limit": 100}),
            };
            let page: Page<ProcedureRow> = parse(
                ctx.invoke("procedures.list", input, InvokeOptions::default())
                    .await?
                    .output,
                "procedures",
            )?;
            Ok(page.items)
        }
    })
}

/// The four tile counts over the returned page — total, then draft, published and retired — or
/// `None` until the read has answered (and when it failed; the panel below says why).
fn page_counts(items: &Option<Result<Vec<ProcedureRow>, ApiError>>) -> Option<[usize; 4]> {
    let Some(Ok(rows)) = items else {
        return None;
    };
    let count = |status: &str| rows.iter().filter(|row| row.status == status).count();
    Some([
        rows.len(),
        count("draft"),
        count("published"),
        count("retired"),
    ])
}

#[component]
pub fn Procedures() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let status = RwSignal::new(Option::<String>::None);
    let procedures = load(ctx, status);

    view! {
        <section class="screen">
            {move || {
                if !ctx.has_action("procedures.list") {
                    return view! {
                        <Planned
                            title="Procedures are planned".to_owned()
                            detail="procedures.list is not in this role's manifest; the screen appears when it lands.".to_owned()
                        />
                    }
                    .into_any();
                }
                view! {
                    <header class="screen-head">
                        <div>
                            <h1>"Procedures"</h1>
                            <p class="muted">
                                "a procedure is the executable form of an SOP — steps, definitions and the inspection the judge reads; an unreviewed SOP-derived procedure refuses publication."
                            </p>
                        </div>
                        <div class="filterbar">
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
                                    <option value="published">"published"</option>
                                    <option value="retired">"retired"</option>
                                </select>
                            </label>
                            <button class="btn" on:click=move |_| procedures.refetch()>"Refresh"</button>
                        </div>
                    </header>
                    {move || {
                        let counts = page_counts(&procedures.get());
                        let (total, drafts, published, retired) = match counts {
                            Some([total, drafts, published, retired]) => (
                                total.to_string(),
                                drafts.to_string(),
                                published.to_string(),
                                retired.to_string(),
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
                                <Tile label="Procedures".to_owned() value=total detail="in the returned page".to_owned()/>
                                <Tile label="Drafts".to_owned() value=drafts detail="status draft in the returned page".to_owned()/>
                                <Tile label="Published".to_owned() value=published detail="status published in the returned page".to_owned()/>
                                <Tile label="Retired".to_owned() value=retired detail="status retired in the returned page".to_owned()/>
                            </div>
                        }
                    }}
                    {move || match procedures.get() {
                        None => view! { <p class="muted">"Reading procedures…"</p> }.into_any(),
                        Some(Err(error)) => view! {
                            <ErrorPanel error=error on_retry=Callback::new(move |_| procedures.refetch())/>
                        }.into_any(),
                        Some(Ok(items)) if items.is_empty() => view! {
                            <Empty message="No procedure in this tenant carries that status.".to_owned()/>
                        }.into_any(),
                        Some(Ok(items)) => view! {
                            <div class="tablewrap">
                                <table class="table">
                                    <thead><tr>
                                        <th>"Key"</th><th>"Title"</th><th>"Status"</th><th class="n">"Version"</th>
                                    </tr></thead>
                                    <tbody>
                                        {items.iter().map(|procedure| view! {
                                            <tr>
                                                <td class="mono">{procedure.key.clone()}</td>
                                                <td>{procedure.title.clone()}</td>
                                                <td><StatusPill status=procedure.status.clone()/></td>
                                                <td class="n">{procedure.version.to_string()}</td>
                                            </tr>
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        }.into_any(),
                    }}
                }
                .into_any()
            }}
        </section>
    }
}
