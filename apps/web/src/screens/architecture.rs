//! Architecture — the TOGAF ADM capability (T9, SPEC_V2 §8.1, §10.3, D63).
//!
//! Four tabs, one capability: the Preliminary phase (the tailored profile, the artefact
//! standard, the Board's charter), the ADM phase rail over an architecture project's `wbs_node`
//! rows, the typed artefact repository (with the entries and links the actions return), and the
//! Board and its contracts.
//!
//! **The phase-advance refusal is the destination's point.** A phase closes on approved
//! artefacts and a live contract, never on a click alone, and the server's refusal is shown
//! verbatim: `architecture_artifact_missing` names the artefact, `architecture_contract_not_live`
//! names the contract. A control whose action is absent from the manifest is not rendered.

use leptos::prelude::*;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{ApiError, InvokeOptions, Page};
use crate::components::{Empty, ErrorPanel, Planned, StatusPill};
use crate::state::AppCtx;

const TABS: [(&str, &str); 5] = [
    ("capability", "Capability"),
    ("phases", "ADM phases"),
    ("artefacts", "Artefact repository"),
    ("board", "Board & compliance"),
    ("decisions", "Decisions"),
];

/// The Board's stable key: one default Architecture Board per tenant.
const BOARD_KEY: &str = "architecture-board";

fn parse<T: serde::de::DeserializeOwned>(value: Value, what: &str) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Network(format!("{what}: {error}")))
}

#[derive(Clone, Debug, Default, Deserialize)]
struct ProfileGet {
    #[serde(default)]
    profile: Option<ProfileRow>,
    #[serde(default)]
    standard: Vec<StandardRow>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct ProfileRow {
    #[serde(default)]
    key: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    artifact_requirements: Value,
    #[serde(default)]
    contract_kinds: Value,
    #[serde(default)]
    status: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct StandardRow {
    #[serde(default)]
    id: String,
    #[serde(default)]
    key: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    artifact_kind: String,
    #[serde(default)]
    phase_code: String,
    #[serde(default)]
    plateau: String,
    #[serde(default)]
    mandatory: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct BodyRow {
    #[serde(default)]
    id: String,
    #[serde(default)]
    key: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    charter: String,
    #[serde(default)]
    commissioning_authority: String,
    #[serde(default)]
    status: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct ProjectRow {
    #[serde(default)]
    id: String,
    #[serde(default)]
    key: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    status: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct WbsRow {
    #[serde(default)]
    id: String,
    #[serde(default)]
    position: f64,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    meta: Value,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct ArtifactRow {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    artifact_kind: String,
    #[serde(default)]
    phase_code: String,
    #[serde(default)]
    plateau: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    artifact_version: i64,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct EntryRow {
    #[serde(default)]
    id: String,
    #[serde(default)]
    axis_a: String,
    #[serde(default)]
    axis_b: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct LinkRow {
    #[serde(default)]
    id: String,
    #[serde(default)]
    from_artifact_id: String,
    #[serde(default)]
    to_artifact_id: String,
    #[serde(default)]
    kind: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct ContractRow {
    #[serde(default)]
    id: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    phase_code: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    status: String,
}

/// One decision record as `decisions.list` returns it: the register row, with its meta.
#[derive(Clone, Debug, Default, Deserialize)]
#[allow(dead_code)]
struct DecisionRow {
    #[serde(default)]
    key: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    supersedes_id: Option<String>,
    #[serde(default)]
    superseded_by_id: Option<String>,
    #[serde(default)]
    meta: Value,
}

/// A string (or array of strings) from the decision's meta, for the register's columns.
fn meta_text(meta: &Value, key: &str) -> Option<String> {
    match meta.get(key) {
        Some(Value::String(text)) if !text.is_empty() => Some(text.clone()),
        Some(Value::Array(items)) if !items.is_empty() => Some(
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", "),
        ),
        Some(Value::Number(number)) => Some(number.to_string()),
        _ => None,
    }
}

#[component]
pub fn ArchitectureScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let (tab, set_tab) = signal("capability".to_owned());
    let refresh = RwSignal::new(0_u32);
    let (notice, set_notice) = signal(String::new());
    let (busy, set_busy) = signal(false);
    let selected = RwSignal::new(Option::<String>::None);
    let selected_artifact = RwSignal::new(Option::<String>::None);
    let (query, set_query) = signal(String::new());
    let (submitted, set_submitted) = signal(String::new());

    let profile = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let _ = refresh.get();
        let client = ctx.client();
        async move {
            parse::<ProfileGet>(
                client
                    .invoke(
                        "architecture.profile_get",
                        json!({}),
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                "the ADM profile",
            )
        }
    });

    let board = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let _ = refresh.get();
        let client = ctx.client();
        async move {
            match client
                .invoke(
                    "architecture.board_get",
                    json!({ "key": BOARD_KEY }),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(invocation) => parse::<BodyRow>(invocation.output, "the Board").map(Some),
                Err(ApiError::NotFound(_)) => Ok(None),
                Err(error) => Err(error),
            }
        }
    });

    let projects = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let _ = refresh.get();
        let client = ctx.client();
        async move {
            parse::<Page<ProjectRow>>(
                client
                    .invoke("portfolio.in_flight", json!({}), InvokeOptions::default())
                    .await?
                    .output,
                "in-flight projects",
            )
            .map(|page| page.items)
        }
    });

    let phases = LocalResource::new(move || {
        let project = selected.get();
        let _tenant = ctx.tenant.get();
        let _ = refresh.get();
        let client = ctx.client();
        async move {
            let Some(project) = project else {
                return Ok(Vec::new());
            };
            parse::<Page<WbsRow>>(
                client
                    .invoke(
                        "wbs.list",
                        json!({ "project_id": project }),
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                "the phase nodes",
            )
            .map(|page| {
                page.items
                    .into_iter()
                    .filter(|row| row.kind == "phase")
                    .collect()
            })
        }
    });

    let contracts = LocalResource::new(move || {
        let project = selected.get();
        let _tenant = ctx.tenant.get();
        let _ = refresh.get();
        let client = ctx.client();
        async move {
            let mut input = json!({});
            if let Some(project) = project {
                input["project_id"] = json!(project);
            }
            parse::<Page<ContractRow>>(
                client
                    .invoke(
                        "architecture.contracts_list",
                        input,
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                "contracts",
            )
            .map(|page| page.items)
        }
    });

    let artefacts = LocalResource::new(move || {
        let text = submitted.get();
        let _tenant = ctx.tenant.get();
        let _ = refresh.get();
        let client = ctx.client();
        async move {
            let mut input = json!({});
            if !text.is_empty() {
                input["query"] = json!(text);
            }
            parse::<Page<ArtifactRow>>(
                client
                    .invoke(
                        "architecture.repository_search",
                        input,
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                "the artefact repository",
            )
            .map(|page| page.items)
        }
    });

    // The architecture decision register: every ADR is a governed asset; this is the read the
    // tranche review and the compliance review cite (R10's screen test).
    let decisions = LocalResource::new(move || {
        let _tenant = ctx.tenant.get();
        let _ = refresh.get();
        let client = ctx.client();
        async move {
            parse::<Page<DecisionRow>>(
                client
                    .invoke(
                        "decisions.list",
                        json!({"limit": 200}),
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                "the decision register",
            )
            .map(|page| page.items)
        }
    });

    let entries = LocalResource::new(move || {
        let id = selected_artifact.get();
        let _tenant = ctx.tenant.get();
        let client = ctx.client();
        async move {
            let Some(id) = id else {
                return Ok(Vec::new());
            };
            parse::<Page<EntryRow>>(
                client
                    .invoke(
                        "architecture.artifacts_entries",
                        json!({ "id": id }),
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                "the artefact's entries",
            )
            .map(|page| page.items)
        }
    });

    let links = LocalResource::new(move || {
        let id = selected_artifact.get();
        let _tenant = ctx.tenant.get();
        let client = ctx.client();
        async move {
            let Some(id) = id else {
                return Ok(Vec::new());
            };
            parse::<Page<LinkRow>>(
                client
                    .invoke(
                        "architecture.artifacts_links",
                        json!({ "id": id }),
                        InvokeOptions::default(),
                    )
                    .await?
                    .output,
                "the artefact's links",
            )
            .map(|page| page.items)
        }
    });

    let retry = Callback::new(move |_| refresh.update(|n| *n += 1));

    let advance = move |phase: String| {
        let Some(project) = selected.get_untracked() else {
            set_notice.set("Choose a project before advancing a phase.".to_owned());
            return;
        };
        let client = ctx.client();
        set_busy.set(true);
        leptos::task::spawn_local(async move {
            match client
                .invoke(
                    "architecture.phases_advance",
                    json!({ "project_id": project, "phase_code": phase }),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(invocation) => {
                    let next = invocation
                        .output
                        .get("next_phase")
                        .and_then(Value::as_str)
                        .unwrap_or("—")
                        .to_owned();
                    set_notice.set(format!("Phase closed · the next open phase is {next}"));
                }
                Err(error) => set_notice.set(format!("Refused · {error}")),
            }
            set_busy.set(false);
            refresh.update(|n| *n += 1);
        });
    };

    let open_board = move |_| {
        let client = ctx.client();
        set_busy.set(true);
        leptos::task::spawn_local(async move {
            match client
                .invoke(
                    "architecture.board_open",
                    json!({
                        "kind": "architecture_board",
                        "key": BOARD_KEY,
                        "name": "Architecture Board",
                        "charter": "Approve the principles and the contracts; waive a deviation only with an accepting owner and a reassessment date.",
                        "commissioning_authority": "the programme board",
                    }),
                    InvokeOptions::default(),
                )
                .await
            {
                Ok(_) => set_notice.set("The Board is open.".to_owned()),
                Err(error) => set_notice.set(format!("Refused · {error}")),
            }
            set_busy.set(false);
            refresh.update(|n| *n += 1);
        });
    };

    view! {
        <section class="screen">
            <div class="screen-head">
                <h1>"Architecture"</h1>
                <p class="muted">
                    "The TOGAF ADM as a capability and a project: the Preliminary phase's profile,
                    standard and Board; Preliminary and A–H as phase nodes closing on typed
                    artefacts and live contracts; the artefact repository; compliance and waivers."
                </p>
            </div>

            <nav class="tabs" aria-label="Architecture tabs">
                {TABS
                    .iter()
                    .map(|(id, label)| {
                        let id = (*id).to_owned();
                        let current = id.clone();
                        let active = move || tab.get() == current;
                        view! {
                            <button
                                type="button"
                                class="tab"
                                class:selected=active
                                on:click=move |_| set_tab.set(id.clone())
                            >
                                {*label}
                            </button>
                        }
                    })
                    .collect_view()}
            </nav>

            {move || {
                (!notice.get().is_empty())
                    .then(|| view! { <p class="panel note">{notice.get()}</p> })
            }}

            {move || match tab.get().as_str() {
                "capability" => {
                    view! {
                        <div class="grid two">
                            <div class="panel">
                                <h2>"The tailored ADM profile"</h2>
                                {move || match profile.get() {
                                    None => view! { <p class="muted">"Reading the profile…"</p> }.into_any(),
                                    Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                                    Some(Ok(body)) => match body.profile {
                                        None => view! {
                                            <Empty message="No profile is tailored in this tenant: tailor one before a phase can advance.".to_owned()/>
                                        }.into_any(),
                                        Some(row) => view! {
                                            <div>
                                                <p><strong>{row.name}</strong> " · " <span class="mono">{row.key}</span> " · "<StatusPill status=row.status/></p>
                                                <h3>"Mandatory artefacts by phase and plateau"</h3>
                                                <pre class="mono" style="white-space:pre-wrap">{row.artifact_requirements.to_string()}</pre>
                                                <h3>"The contract that closes each phase"</h3>
                                                <pre class="mono" style="white-space:pre-wrap">{row.contract_kinds.to_string()}</pre>
                                            </div>
                                        }.into_any(),
                                    },
                                }}
                            </div>
                            <div class="panel">
                                <h2>"The Board"</h2>
                                {move || match board.get() {
                                    None => view! { <p class="muted">"Reading the Board…"</p> }.into_any(),
                                    Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                                    Some(Ok(None)) => view! {
                                        <div>
                                            <Empty message="No Architecture Board is open in this tenant.".to_owned()/>
                                            {ctx.has_action("architecture.board_open").then(|| view! {
                                                <button class="btn" disabled=move || busy.get() on:click=open_board>
                                                    "Open the Board"
                                                </button>
                                            })}
                                        </div>
                                    }.into_any(),
                                    Some(Ok(Some(row))) => view! {
                                        <div>
                                            <p><strong>{row.name}</strong> " · "<StatusPill status=row.status/></p>
                                            <p class="muted">{row.charter}</p>
                                            <p class="mono muted">"Commissioned by: " {row.commissioning_authority}</p>
                                        </div>
                                    }.into_any(),
                                }}
                                <h3>"The artefact standard"</h3>
                                {move || match profile.get() {
                                    Some(Ok(body)) => view! {
                                        <table class="table">
                                            <thead>
                                                <tr><th>"Type"</th><th>"Kind"</th><th>"Phase"</th><th>"Plateau"</th><th>"Mandatory"</th></tr>
                                            </thead>
                                            <tbody>
                                                {body
                                                    .standard
                                                    .into_iter()
                                                    .map(|row| view! {
                                                        <tr>
                                                            <td><strong>{row.name}</strong> " " <span class="mono muted">{row.key}</span></td>
                                                            <td>{row.artifact_kind}</td>
                                                            <td>{row.phase_code}</td>
                                                            <td>{row.plateau}</td>
                                                            <td>{if row.mandatory { "mandatory" } else { "optional" }}</td>
                                                        </tr>
                                                    })
                                                    .collect_view()}
                                            </tbody>
                                        </table>
                                    }.into_any(),
                                    _ => view! { <p class="muted">"Reading the standard…"</p> }.into_any(),
                                }}
                            </div>
                        </div>
                    }.into_any()
                }
                "phases" => {
                    view! {
                        <div class="panel">
                            <h2>"The phase rail"</h2>
                            <label class="filter">
                                <span>"Architecture project"</span>
                                <select on:change=move |event| {
                                    let value = event_target_value(&event);
                                    selected.set((!value.is_empty()).then_some(value));
                                }>
                                    <option value="">"Choose a project"</option>
                                    {move || match projects.get() {
                                        Some(Ok(rows)) => rows.into_iter().map(|row| view! {
                                            <option value=row.id.clone()>{format!("{} · {}", row.key, row.name)}</option>
                                        }).collect_view().into_any(),
                                        _ => view! { <option value="">"Reading projects…"</option> }.into_any(),
                                    }}
                                </select>
                            </label>
                            {move || match phases.get() {
                                None => view! { <p class="muted">"Reading the phases…"</p> }.into_any(),
                                Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                                Some(Ok(rows)) if rows.is_empty() => view! {
                                    <Empty message="Choose an architecture project to read its Preliminary and A–H nodes.".to_owned()/>
                                }.into_any(),
                                Some(Ok(rows)) => view! {
                                    <table class="table">
                                        <thead>
                                            <tr><th>"#"</th><th>"Phase"</th><th>"Status"</th><th>"Evidence"</th><th></th></tr>
                                        </thead>
                                        <tbody>
                                            {rows.into_iter().map(|row| {
                                                let code = row
                                                    .meta
                                                    .get("phase_code")
                                                    .and_then(Value::as_str)
                                                    .unwrap_or("—")
                                                    .to_owned();
                                                let advance = advance.clone();
                                                let phase = code.clone();
                                                view! {
                                                    <tr>
                                                        <td class="num">{row.position}</td>
                                                        <td><strong>{row.title}</strong> " " <span class="mono muted">{code}</span></td>
                                                        <td><StatusPill status=row.status.clone()/></td>
                                                        <td class="mono muted">{row.meta.get("evidence").map(Value::to_string).unwrap_or_default()}</td>
                                                        <td>
                                                            {ctx.has_action("architecture.phases_advance").then(|| view! {
                                                                <button class="btn small" disabled=move || busy.get() on:click=move |_| advance(phase.clone())>
                                                                    "Advance"
                                                                </button>
                                                            })}
                                                        </td>
                                                    </tr>
                                                }
                                            }).collect_view()}
                                        </tbody>
                                    </table>
                                }.into_any(),
                            }}
                            <p class="muted">
                                "A phase closes only when every artefact its profile marks mandatory is
                                approved and its contract is live; the refusal names what is missing."
                            </p>
                        </div>
                    }.into_any()
                }
                "artefacts" => {
                    view! {
                        <div class="grid two">
                            <div class="panel">
                                <h2>"The repository"</h2>
                                <form class="toolbar" on:submit=move |event| {
                                    event.prevent_default();
                                    set_submitted.set(query.get_untracked());
                                }>
                                    <input
                                        type="search"
                                        placeholder="Search title, purpose or metadata; reuse edges are followed"
                                        prop:value=move || query.get()
                                        on:input=move |event| set_query.set(event_target_value(&event))
                                    />
                                    <button class="btn" type="submit">"Search"</button>
                                </form>
                                {move || match artefacts.get() {
                                    None => view! { <p class="muted">"Reading the repository…"</p> }.into_any(),
                                    Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                                    Some(Ok(rows)) if rows.is_empty() => view! {
                                        <Empty message="No approved artefact matches. Approve one, or clear the search.".to_owned()/>
                                    }.into_any(),
                                    Some(Ok(rows)) => view! {
                                        <table class="table">
                                            <thead>
                                                <tr><th>"Title"</th><th>"Kind"</th><th>"Phase"</th><th>"Plateau"</th><th>"Status"</th><th class="num">"Version"</th></tr>
                                            </thead>
                                            <tbody>
                                                {rows.into_iter().map(|row| {
                                                    let id = row.id.clone();
                                                    view! {
                                                        <tr
                                                            class:selected=move || selected_artifact.get().as_deref() == Some(id.as_str())
                                                            on:click=move |_| selected_artifact.set(Some(row.id.clone()))
                                                            style="cursor:pointer"
                                                        >
                                                            <td><strong>{row.title}</strong></td>
                                                            <td>{row.artifact_kind}</td>
                                                            <td>{row.phase_code}</td>
                                                            <td>{row.plateau}</td>
                                                            <td><StatusPill status=row.status/></td>
                                                            <td class="num">{row.artifact_version}</td>
                                                        </tr>
                                                    }
                                                }).collect_view()}
                                            </tbody>
                                        </table>
                                    }.into_any(),
                                }}
                            </div>
                            <div class="panel">
                                <h2>"The inspector"</h2>
                                {move || match selected_artifact.get() {
                                    None => view! { <Empty message="Choose an artefact to read its entries and links.".to_owned()/> }.into_any(),
                                    Some(id) => view! {
                                        <div>
                                            <h3>"Entries"</h3>
                                            {move || match entries.get() {
                                                Some(Ok(rows)) if !rows.is_empty() => view! {
                                                    <table class="table">
                                                        <thead><tr><th>"Axis A"</th><th>"Axis B"</th></tr></thead>
                                                        <tbody>
                                                            {rows.into_iter().map(|row| view! {
                                                                <tr><td>{row.axis_a}</td><td>{row.axis_b}</td></tr>
                                                            }).collect_view()}
                                                        </tbody>
                                                    </table>
                                                }.into_any(),
                                                Some(Ok(_)) => view! { <p class="muted">"No entries: a diagram's body is its document."</p> }.into_any(),
                                                Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                                                None => view! { <p class="muted">"Reading entries…"</p> }.into_any(),
                                            }}
                                            <h3>"Links"</h3>
                                            {move || match links.get() {
                                                Some(Ok(rows)) if !rows.is_empty() => view! {
                                                    <table class="table">
                                                        <thead><tr><th>"Kind"</th><th>"From"</th><th>"To"</th></tr></thead>
                                                        <tbody>
                                                            {rows.into_iter().map(|row| view! {
                                                                <tr>
                                                                    <td>{row.kind}</td>
                                                                    <td class="mono">{row.from_artifact_id}</td>
                                                                    <td class="mono">{row.to_artifact_id}</td>
                                                                </tr>
                                                            }).collect_view()}
                                                        </tbody>
                                                    </table>
                                                }.into_any(),
                                                Some(Ok(_)) => view! { <p class="muted">"No links yet."</p> }.into_any(),
                                                Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                                                None => view! { <p class="muted">"Reading links…"</p> }.into_any(),
                                            }}
                                            <p class="mono muted">{id}</p>
                                        </div>
                                    }.into_any(),
                                }}
                            </div>
                        </div>
                    }.into_any()
                }
                _ => {
                    view! {
                        <div class="panel">
                            <h2>"Contracts and compliance"</h2>
                            {move || match board.get() {
                                Some(Ok(None)) => view! {
                                    <Empty message="No Board is open, so no decision has a governance body to name.".to_owned()/>
                                }.into_any(),
                                Some(Ok(Some(row))) => view! {
                                    <p><strong>{row.name}</strong> " · "<StatusPill status=row.status/> " · " <span class="muted">{row.charter}</span></p>
                                }.into_any(),
                                Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                                None => view! { <p class="muted">"Reading the Board…"</p> }.into_any(),
                            }}
                            {move || match contracts.get() {
                                None => view! { <p class="muted">"Reading contracts…"</p> }.into_any(),
                                Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                                Some(Ok(rows)) if rows.is_empty() => view! {
                                    <Empty message="No contract on this project. Contracts are requested by the architecture project's team.".to_owned()/>
                                }.into_any(),
                                Some(Ok(rows)) => view! {
                                    <table class="table">
                                        <thead><tr><th>"Kind"</th><th>"Phase"</th><th>"Title"</th><th>"Status"</th></tr></thead>
                                        <tbody>
                                            {rows.into_iter().map(|row| view! {
                                                <tr>
                                                    <td><strong>{row.kind}</strong></td>
                                                    <td>{row.phase_code}</td>
                                                    <td>{row.title}</td>
                                                    <td><StatusPill status=row.status/></td>
                                                </tr>
                                            }).collect_view()}
                                        </tbody>
                                    </table>
                                }.into_any(),
                            }}
                            <p class="muted">
                                "A delivery project's design gate resolves only against a live
                                contract; a compliance review records exactly one of the six levels,
                                and a waiver needs an accepting owner and a reassessment date."
                            </p>
                        </div>
                    }.into_any()
                }
                "decisions" => {
                    if !ctx.has_action("decisions.list") {
                        return view! {
                            <Planned
                                title="The decision register is planned".to_owned()
                                detail="decisions.list is not in this role's manifest — the register appears when the action lands.".to_owned()
                            />
                        }
                        .into_any();
                    }
                    view! {
                        <div class="panel">
                            <h2>"The architecture decision register"</h2>
                            <p class="muted">
                                "Every ADR is a governed asset: the number, the status, the decision
                                and its supersession chain are rows; a decision's approval is a
                                gate, not this read. The tr3 tranche review cites this register."
                            </p>
                            {move || match decisions.get() {
                                None => view! { <p class="muted">"Reading the register…"</p> }.into_any(),
                                Some(Err(error)) => view! {
                                    <ErrorPanel error=error on_retry=Callback::new(move |_| decisions.refetch())/>
                                }.into_any(),
                                Some(Ok(rows)) if rows.is_empty() => view! {
                                    <Empty message="No decision recorded — an ADR is created by `decisions.create`.".to_owned()/>
                                }.into_any(),
                                Some(Ok(rows)) => view! {
                                    <div class="tablewrap">
                                        <table class="table">
                                            <thead><tr>
                                                <th>"Number"</th><th>"Decision"</th><th>"Status"</th>
                                                <th>"Decided at"</th><th>"Deciders"</th><th>"Supersedes"</th>
                                            </tr></thead>
                                            <tbody>
                                                {rows.into_iter().map(|row| {
                                                    let number = meta_text(&row.meta, "number")
                                                        .filter(|text| !text.is_empty())
                                                        .unwrap_or_else(|| row.key.clone());
                                                    let decided = meta_text(&row.meta, "decided_at")
                                                        .unwrap_or_else(|| "—".to_owned());
                                                    let deciders = meta_text(&row.meta, "deciders")
                                                        .unwrap_or_else(|| "—".to_owned());
                                                    let supersedes = row
                                                        .supersedes_id
                                                        .as_deref()
                                                        .map(|id| id.chars().take(8).collect::<String>())
                                                        .unwrap_or_else(|| "—".to_owned());
                                                    view! {
                                                        <tr>
                                                            <td class="mono">{number}</td>
                                                            <td>{row.title.clone()}</td>
                                                            <td><StatusPill status=row.status.clone()/></td>
                                                            <td>{decided}</td>
                                                            <td>{deciders}</td>
                                                            <td class="mono">{supersedes}</td>
                                                        </tr>
                                                    }
                                                }).collect_view()}
                                            </tbody>
                                        </table>
                                    </div>
                                }.into_any(),
                            }}
                        </div>
                    }.into_any()
                }
            }}
        </section>
    }
}
