//! Board index and the lane board.
//!
//! Reads are the literal projections `GET /api/projects` (one keyset page, `next_cursor`
//! carried), `GET /api/projects/{id}`, `GET /api/boards/{id}` and
//! `GET /api/boards/{id}/cards` (keyset pages, `next_cursor` carried). Writes are actions
//! named in the manifest — `projects.create`, `cards.create`, `cards.update`, `cards.archive`,
//! `cards.unarchive`, `cards.add_label`, `cards.remove_label`, `labels.create`,
//! `custom_fields.create`, `custom_fields.set_value`, `custom_fields.clear_value`,
//! `lists.create`, `lists.update`, `lists.move` and `cards.move` — and a control whose action
//! is absent is not rendered. Every edit that replaces a row carries its `version` as
//! `If-Match`; a 412 refetches, never guesses a new version.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};
use serde::Deserialize;
use serde_json::{Value, json};
use wasm_bindgen::JsCast;

use crate::api::{ApiError, Client, InvokeOptions, Page};
use crate::components::{Empty, ErrorPanel, age};
use crate::models::{BoardView, CardOut, CardView, LabelOut, ListOut, ProjectOut, ProjectView};
use crate::sse;
use crate::state::AppCtx;

/// The lane kinds `lists.create` accepts.
const LANE_KINDS: [&str; 5] = ["intake", "active", "waiting", "done", "archive"];

/// One keyset page of projects, with the cursor that continues it.
#[derive(Clone, Debug)]
struct ProjectPage {
    views: Vec<ProjectView>,
    next_cursor: Option<String>,
}

#[derive(Clone, Debug)]
struct BoardData {
    board: BoardView,
    cards: Vec<CardOut>,
    cards_cursor: Option<String>,
}

/// The subset of a `boards.get` custom field this screen renders. `BoardView.fields` is a
/// `Vec<Value>` in the web model; the action output is `CustomFieldOut`, and an entry this
/// screen cannot read is skipped rather than invented.
#[derive(Clone, Debug, Deserialize)]
struct FieldBrief {
    id: String,
    name: String,
    #[serde(default)]
    kind: String,
}

/// The board screen's shared, mutable state. It lives in `BoardScreen`, above the render that
/// the resource drives, so an appended card page and an open edit survive a re-render.
#[derive(Clone)]
struct BoardShared {
    resource: LocalResource<Result<BoardData, ApiError>>,
    drag: RwSignal<Option<(String, i64)>>,
    cards: RwSignal<Vec<CardOut>>,
    cards_cursor: RwSignal<Option<String>>,
    more_loading: RwSignal<bool>,
    editing: RwSignal<Option<String>>,
    edit_name: RwSignal<String>,
    edit_description: RwSignal<String>,
    archived: RwSignal<Vec<CardOut>>,
    /// New-card drafts, keyed by list id. They live above the resource-driven render so an SSE
    /// refetch cannot recreate the form and lose what a person has typed.
    new_card_name: RwSignal<std::collections::HashMap<String, String>>,
    new_card_type: RwSignal<std::collections::HashMap<String, String>>,
}

/// `BoardShared` plus the board snapshot the body rendered. Each component receives one prop,
/// so no child re-reads state behind the render's back.
#[derive(Clone)]
struct BoardCtx {
    board_id: String,
    resource: LocalResource<Result<BoardData, ApiError>>,
    drag: RwSignal<Option<(String, i64)>>,
    cards: RwSignal<Vec<CardOut>>,
    cards_cursor: RwSignal<Option<String>>,
    more_loading: RwSignal<bool>,
    editing: RwSignal<Option<String>>,
    edit_name: RwSignal<String>,
    edit_description: RwSignal<String>,
    archived: RwSignal<Vec<CardOut>>,
    new_card_name: RwSignal<std::collections::HashMap<String, String>>,
    new_card_type: RwSignal<std::collections::HashMap<String, String>>,
    labels: Vec<LabelOut>,
    fields: Vec<FieldBrief>,
    lanes: Vec<ListOut>,
    lane_ids: Vec<String>,
}

async fn load_projects(client: Client, cursor: Option<String>) -> Result<ProjectPage, ApiError> {
    let path = match cursor {
        Some(cursor) => format!("/api/projects?limit=50&cursor={cursor}"),
        None => "/api/projects?limit=50".to_owned(),
    };
    let value = client.get(&path).await?;
    let page: Page<ProjectOut> = serde_json::from_value(value)
        .map_err(|error| ApiError::Network(format!("projects: {error}")))?;
    let mut views = Vec::with_capacity(page.items.len());
    for project in page.items {
        let value = client.get(&format!("/api/projects/{}", project.id)).await?;
        let view: ProjectView = serde_json::from_value(value)
            .map_err(|error| ApiError::Network(format!("project: {error}")))?;
        views.push(view);
    }
    Ok(ProjectPage {
        views,
        next_cursor: page.next_cursor,
    })
}

async fn load_cards(
    client: &Client,
    board_id: &str,
    cursor: Option<&str>,
) -> Result<Page<CardOut>, ApiError> {
    let path = match cursor {
        Some(cursor) => format!("/api/boards/{board_id}/cards?limit=200&cursor={cursor}"),
        None => format!("/api/boards/{board_id}/cards?limit=200"),
    };
    let value = client.get(&path).await?;
    serde_json::from_value(value).map_err(|error| ApiError::Network(format!("cards: {error}")))
}

fn field_briefs(fields: &[Value]) -> Vec<FieldBrief> {
    fields
        .iter()
        .filter_map(|field| serde_json::from_value(field.clone()).ok())
        .collect()
}

/// A local validation refusal, reported through the same toast an action failure uses.
fn warn(ctx: AppCtx, message: &str) {
    ctx.report(&ApiError::Invalid {
        detail: message.to_owned(),
        errors: Vec::new(),
    });
}

/// One write: `done(true)` on success, `done(false)` on the one failure whose correct answer
/// is a refetch (412 — the object moved), `finish` always.
fn run_write(
    ctx: AppCtx,
    name: &'static str,
    input: Value,
    if_match: Option<i64>,
    done: impl FnOnce(bool) + 'static,
    finish: impl FnOnce() + 'static,
) {
    leptos::task::spawn_local(async move {
        let result = ctx
            .invoke(
                name,
                input,
                InvokeOptions {
                    if_match,
                    idempotency_key: None,
                },
            )
            .await;
        match result {
            Ok(_) => done(true),
            Err(error) => {
                if matches!(&error, ApiError::Precondition(_)) {
                    done(false);
                }
                ctx.report(&error);
            }
        }
        finish();
    });
}

/// A custom-field value carries the JSON type its kind declares (`custom_fields.set_value`
/// refuses anything else).
fn field_content(kind: &str, raw: &str) -> Result<Value, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("A value is required; use Clear to remove one.".to_owned());
    }
    match kind {
        "number" => raw
            .parse::<f64>()
            .map(|value| json!(value))
            .map_err(|_| "That field takes a number.".to_owned()),
        "checkbox" => match raw.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Ok(json!(true)),
            "false" | "0" | "no" => Ok(json!(false)),
            _ => Err("That field takes true or false.".to_owned()),
        },
        _ => Ok(json!(raw)),
    }
}

#[component]
pub fn BoardIndex() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let data = LocalResource::new(move || async move {
        let _tenant = ctx.tenant.get();
        load_projects(ctx.client(), None).await
    });
    let retry = Callback::new(move |_| data.refetch());

    // Page one is the resource; every "load more" page lands in `more`. A refetch of page one
    // invalidates the pages appended after it, because the cursor they continued from is gone.
    let more = RwSignal::new(Vec::<ProjectView>::new());
    let cursor = RwSignal::new(None::<String>);
    let loading = RwSignal::new(false);
    Effect::new(move |_| {
        if let Some(Ok(page)) = data.get() {
            more.set(Vec::new());
            cursor.set(page.next_cursor);
        }
    });

    let title = RwSignal::new(String::new());
    let project_type = RwSignal::new("delivery".to_owned());
    let description = RwSignal::new(String::new());
    let creating = RwSignal::new(false);

    let load_more = move |_| {
        let Some(next) = cursor.get_untracked() else {
            return;
        };
        if loading.get_untracked() {
            return;
        }
        loading.set(true);
        let ctx = ctx;
        leptos::task::spawn_local(async move {
            match load_projects(ctx.client(), Some(next)).await {
                Ok(page) => {
                    more.update(|views| views.extend(page.views));
                    cursor.set(page.next_cursor);
                }
                Err(error) => ctx.report(&error),
            }
            loading.set(false);
        });
    };

    let create = move |event: web_sys::SubmitEvent| {
        event.prevent_default();
        let name = title.get_untracked().trim().to_owned();
        if name.is_empty() {
            warn(ctx, "A project title is required.");
            return;
        }
        if creating.get_untracked() {
            return;
        }
        let kind = project_type.get_untracked().trim().to_owned();
        let text = description.get_untracked().trim().to_owned();
        let mut input = json!({ "title": name });
        if !kind.is_empty() {
            input["project_type_code"] = json!(kind);
        }
        if !text.is_empty() {
            input["description"] = json!(text);
        }
        creating.set(true);
        run_write(
            ctx,
            "projects.create",
            input,
            None,
            move |ok| {
                if ok {
                    title.set(String::new());
                    description.set(String::new());
                }
                data.refetch();
            },
            move || creating.set(false),
        );
    };

    view! {
        <section class="screen">
            <header class="screen-head">
                <div>
                    <h1>"Board"</h1>
                    <p class="muted">"Projects and their boards."</p>
                </div>
            </header>
            {move || ctx.has_action("projects.create").then(|| view! {
                <form class="panel" on:submit=create>
                    <h2>"New project"</h2>
                    <p class="muted">
                        "The project's board, its template lanes and its start card are created with it."
                    </p>
                    <div class="formrow">
                        <label class="filter">
                            <span>"Project title"</span>
                            <input
                                prop:value=move || title.get()
                                on:input=move |event| title.set(event_target_value(&event))
                                placeholder="Checkout redesign"
                            />
                        </label>
                        <label class="filter">
                            <span>"Project type"</span>
                            <input
                                prop:value=move || project_type.get()
                                on:input=move |event| project_type.set(event_target_value(&event))
                                placeholder="delivery"
                            />
                        </label>
                        <label class="filter">
                            <span>"Project description"</span>
                            <input
                                prop:value=move || description.get()
                                on:input=move |event| description.set(event_target_value(&event))
                                placeholder="What this project delivers"
                            />
                        </label>
                        <button class="btn primary" type="submit" disabled=move || creating.get()>
                            {move || if creating.get() { "Creating…" } else { "Create project" }}
                        </button>
                    </div>
                </form>
            })}
            {move || match data.get() {
                None => view! { <p class="muted">"Loading projects…"</p> }.into_any(),
                Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
                Some(Ok(page)) => {
                    let mut views = page.views;
                    views.extend(more.get());
                    if views.is_empty() {
                        view! {
                            <div class="panel"><Empty message="No projects yet.".to_owned()/></div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="project-list">
                                {views.into_iter().map(|view| view! {
                                    <section class="panel project">
                                        <div class="panel-head">
                                            <h2>{view.project.name}</h2>
                                            <span class="mono muted">{view.project.key}</span>
                                        </div>
                                        <p class="muted">{view.project.description}</p>
                                        <div class="project-boards">
                                            {view.boards.into_iter().map(|board| view! {
                                                <A href=format!("/board/{}", board.id)>
                                                    <span class="board-link">{board.name}</span>
                                                </A>
                                            }).collect_view()}
                                        </div>
                                    </section>
                                }).collect_view()}
                            </div>
                            {cursor.get().map(|_| view! {
                                <div class="panel" style="margin-top:12px">
                                    <button
                                        class="btn"
                                        on:click=load_more
                                        disabled=move || loading.get()
                                    >
                                        {move || if loading.get() { "Loading projects…" } else { "Load more projects" }}
                                    </button>
                                </div>
                            })}
                        }.into_any()
                    }
                }
            }}
        </section>
    }
}

#[component]
pub fn BoardScreen() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let params = use_params_map();
    let board_id = move || params.get().get("board_id");
    let data = LocalResource::new(move || async move {
        let id =
            board_id().ok_or_else(|| ApiError::NotFound("no board in the route".to_owned()))?;
        let _tenant = ctx.tenant.get();
        let client = ctx.client();
        let value = client.get(&format!("/api/boards/{id}")).await?;
        let board: BoardView = serde_json::from_value(value)
            .map_err(|error| ApiError::Network(format!("board: {error}")))?;
        let cards = load_cards(&client, &id, None).await?;
        Ok::<BoardData, ApiError>(BoardData {
            board,
            cards: cards.items,
            cards_cursor: cards.next_cursor,
        })
    });

    let subscription = RwSignal::new(None::<sse::Subscription>);
    Effect::new(move |_| {
        let Some(id) = board_id() else { return };
        subscription.update(|slot| {
            if let Some(old) = slot.take() {
                old.abort();
            }
        });
        let target = data;
        match sse::subscribe(
            ctx.client(),
            format!("board:{id}"),
            None,
            move |frame| match frame {
                Ok(frame)
                    if frame.event == "resync"
                        || frame.event.starts_with("card.")
                        || frame.event.starts_with("list.") =>
                {
                    target.refetch();
                }
                _ => {}
            },
        ) {
            Ok(sub) => subscription.set(Some(sub)),
            Err(error) => ctx.report(&error),
        }
    });
    on_cleanup(move || {
        subscription.update(|slot| {
            if let Some(old) = slot.take() {
                old.abort();
            }
        });
    });

    let drag: RwSignal<Option<(String, i64)>> = RwSignal::new(None);
    let retry = Callback::new(move |_| data.refetch());

    // Paging state lives above the render the resource drives: an appended page survives a
    // re-render, and page one's refetch resets it. A route change to another board also
    // clears the archive strip, so a card archived on one board is not offered on the next.
    let cards = RwSignal::new(Vec::<CardOut>::new());
    let cards_cursor = RwSignal::new(None::<String>);
    let more_loading = RwSignal::new(false);
    let editing = RwSignal::new(Option::<String>::None);
    let edit_name = RwSignal::new(String::new());
    let edit_description = RwSignal::new(String::new());
    let archived = RwSignal::new(Vec::<CardOut>::new());
    let new_card_name = RwSignal::new(std::collections::HashMap::<String, String>::new());
    let new_card_type = RwSignal::new(std::collections::HashMap::<String, String>::new());
    Effect::new(move |_| {
        if let Some(Ok(body)) = data.get() {
            cards.set(body.cards);
            cards_cursor.set(body.cards_cursor);
        }
    });
    Effect::new(move |_| {
        let _ = board_id();
        archived.set(Vec::new());
    });

    let shared = BoardShared {
        resource: data,
        drag,
        cards,
        cards_cursor,
        more_loading,
        editing,
        edit_name,
        edit_description,
        archived,
        new_card_name,
        new_card_type,
    };

    view! {
        {move || match data.get() {
            None => view! { <p class="muted">"Loading the board…"</p> }.into_any(),
            Some(Err(error)) => view! { <ErrorPanel error=error on_retry=retry/> }.into_any(),
            Some(Ok(board_data)) => view! {
                <BoardBody data=board_data shared=shared.clone()/>
            }.into_any(),
        }}
    }
}

#[component]
fn BoardBody(data: BoardData, shared: BoardShared) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let board = data.board;
    let board_name = board.board.name.clone();
    let lane_count = board.lists.len();
    let lanes: Vec<ListOut> = board.lists.into_iter().map(|lane| lane.list).collect();
    let lane_ids: Vec<String> = lanes.iter().map(|lane| lane.id.clone()).collect();
    let labels = board.labels;
    let fields = field_briefs(&board.fields);
    let board_ctx = BoardCtx {
        board_id: board.board.id.clone(),
        resource: shared.resource,
        drag: shared.drag,
        cards: shared.cards,
        cards_cursor: shared.cards_cursor,
        more_loading: shared.more_loading,
        editing: shared.editing,
        edit_name: shared.edit_name,
        edit_description: shared.edit_description,
        archived: shared.archived,
        new_card_name: shared.new_card_name,
        new_card_type: shared.new_card_type,
        labels,
        fields,
        lanes,
        lane_ids,
    };
    let panels_board = board_ctx.clone();
    let load_board_id = board_ctx.board_id.clone();
    let cards = board_ctx.cards;
    let cards_cursor = board_ctx.cards_cursor;
    let more_loading = board_ctx.more_loading;
    let resource = board_ctx.resource;
    let archived = board_ctx.archived;

    let load_more: Callback<()> = Callback::new(move |_| {
        let Some(next) = cards_cursor.get_untracked() else {
            return;
        };
        if more_loading.get_untracked() {
            return;
        }
        let board_id = load_board_id.clone();
        more_loading.set(true);
        leptos::task::spawn_local(async move {
            let client = ctx.client();
            match load_cards(&client, &board_id, Some(next.as_str())).await {
                Ok(page) => {
                    cards.update(|items| items.extend(page.items));
                    cards_cursor.set(page.next_cursor);
                }
                Err(error) => ctx.report(&error),
            }
            more_loading.set(false);
        });
    });

    // An archived card is no longer on the board page, so Unarchive reads its current version
    // from `GET /api/cards/{id}` first; a 412 on the way back refetches, never guesses.
    let unarchive: Callback<String> = Callback::new(move |id: String| {
        leptos::task::spawn_local(async move {
            let client = ctx.client();
            let card_id = id.clone();
            let value = match client.get(&format!("/api/cards/{card_id}")).await {
                Ok(value) => value,
                Err(error) => {
                    ctx.report(&error);
                    return;
                }
            };
            let view: CardView = match serde_json::from_value(value) {
                Ok(view) => view,
                Err(error) => {
                    ctx.report(&ApiError::Network(format!("card: {error}")));
                    return;
                }
            };
            let result = ctx
                .invoke(
                    "cards.unarchive",
                    json!({ "id": card_id }),
                    InvokeOptions {
                        if_match: Some(view.card.version),
                        idempotency_key: None,
                    },
                )
                .await;
            match result {
                Ok(_) => {
                    archived.update(|items| items.retain(|card| card.id != card_id));
                    resource.refetch();
                }
                Err(error) => {
                    if matches!(&error, ApiError::Precondition(_)) {
                        resource.refetch();
                    }
                    ctx.report(&error);
                }
            }
        });
    });

    view! {
        <section class="screen board-screen">
            <header class="screen-head">
                <div>
                    <h1>{board_name}</h1>
                    <p class="muted">{lane_count} " lanes · drag a card, or focus it and use ← →"</p>
                </div>
                <A href="/boards"><span class="btn">"All boards"</span></A>
            </header>

            {move || {
                let labels_open = ctx.has_action("labels.create")
                    || ctx.has_action("cards.add_label")
                    || ctx.has_action("cards.remove_label");
                let fields_open = ctx.has_action("custom_fields.create")
                    || ctx.has_action("custom_fields.set_value")
                    || ctx.has_action("custom_fields.clear_value");
                let lane_open = ctx.has_action("lists.create");
                if !(labels_open || fields_open || lane_open) {
                    return ().into_any();
                }
                view! {
                    <div class="filterbar" style="align-items:stretch">
                        {labels_open.then(|| view! { <LabelsPanel board=panels_board.clone()/> })}
                        {fields_open.then(|| view! { <FieldsPanel board=panels_board.clone()/> })}
                        {lane_open.then(|| view! { <NewLaneForm board=panels_board.clone()/> })}
                    </div>
                }.into_any()
            }}

            <div class="lanes">
                {board_ctx.lanes.clone().into_iter().map(|lane| view! {
                    <Lane lane=lane board=board_ctx.clone()/>
                }).collect_view()}
            </div>

            {move || cards_cursor.get().map(|_| view! {
                <div class="panel" style="margin-top:12px">
                    <button class="btn" on:click=move |_| load_more.run(()) disabled=move || more_loading.get()>
                        {move || if more_loading.get() { "Loading more cards…" } else { "Load more cards" }}
                    </button>
                </div>
            })}

            {move || {
                let items = archived.get();
                if items.is_empty() {
                    return ().into_any();
                }
                let allowed = ctx.has_action("cards.unarchive");
                view! {
                    <section class="panel archived" style="margin-top:12px">
                        <h2>"Archived on this page load"</h2>
                        <p class="muted">
                            "These cards left the board since it loaded. Unarchive reads the card's current version first, so a card that moved is never overwritten."
                        </p>
                        <div class="formrow">
                            {items.into_iter().map(|card| {
                                let id = card.id.clone();
                                let name = card.name.clone();
                                view! {
                                    <div class="formrow">
                                        <span>{name}</span>
                                        {allowed.then(|| view! {
                                            <button
                                                class="btn small"
                                                on:click=move |_| unarchive.run(id.clone())
                                            >
                                                "Unarchive"
                                            </button>
                                        })}
                                    </div>
                                }
                            }).collect_view()}
                        </div>
                    </section>
                }.into_any()
            }}
        </section>
    }
}

#[component]
fn Lane(lane: ListOut, board: BoardCtx) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let resource = board.resource;
    let drag = board.drag;
    let cards = board.cards;
    let lane_id = lane.id.clone();
    let lane_kind = lane.kind.clone();
    let lane_version = lane.version;
    let display_name = lane.name.clone();
    let toggle_name = lane.name.clone();
    let renaming = RwSignal::new(false);
    let rename_name = RwSignal::new(lane.name);

    let drop = {
        let lane_id = lane_id.clone();
        move |event: web_sys::DragEvent| {
            event.prevent_default();
            if let Some((card_id, version)) = drag.get_untracked() {
                drag.set(None);
                move_card(ctx, resource, card_id, lane_id.clone(), version);
            }
        }
    };

    let move_lane: Callback<bool> = {
        let board = board.clone();
        let lane_id = lane_id.clone();
        Callback::new(move |right: bool| {
            let lanes = board.lanes.clone();
            let Some(index) = lanes.iter().position(|item| item.id == lane_id) else {
                return;
            };
            // `lists.move` lands a lane immediately after `after_id` (absent appends). There
            // is no "before the first lane", so a one-step-left move from the second lane
            // moves the first lane after it — the same swap, named from the other side.
            let (moved_id, moved_version, after_id) = if right {
                if index + 1 >= lanes.len() {
                    return;
                }
                (
                    lane_id.clone(),
                    lane_version,
                    Some(lanes[index + 1].id.clone()),
                )
            } else if index == 0 {
                return;
            } else if index == 1 {
                (lanes[0].id.clone(), lanes[0].version, Some(lane_id.clone()))
            } else {
                (
                    lane_id.clone(),
                    lane_version,
                    Some(lanes[index - 2].id.clone()),
                )
            };
            let mut input = json!({ "id": moved_id });
            if let Some(after) = after_id {
                input["after_id"] = json!(after);
            }
            run_write(
                ctx,
                "lists.move",
                input,
                Some(moved_version),
                move |_| resource.refetch(),
                || {},
            );
        })
    };

    let count_list = lane_id.clone();
    let cards_list = lane_id.clone();
    let rename_id = lane_id.clone();
    let form_list = lane_id;
    let form_kind = lane_kind.clone();
    let cards_board = board.clone();
    let form_board = board;
    let card_kind = lane_kind;

    view! {
        <section class="lane" on:dragover=move |event| event.prevent_default() on:drop=drop>
            <header class="lane-head">
                <h2>{display_name}</h2>
                <span class="count">
                    {move || cards.get().iter().filter(|card| card.list_id == count_list).count()}
                </span>
            </header>

            {move || {
                let can_update = ctx.has_action("lists.update");
                let can_move = ctx.has_action("lists.move");
                if !(can_update || can_move) {
                    return ().into_any();
                }
                let toggle = {
                    let name = toggle_name.clone();
                    move |_| {
                        renaming.update(|open| *open = !*open);
                        rename_name.set(name.clone());
                    }
                };
                let left = move |_| move_lane.run(false);
                let right = move |_| move_lane.run(true);
                view! {
                    <div class="formrow" style="margin-bottom:8px">
                        {can_update.then(|| view! {
                            <button class="btn small" on:click=toggle>"Rename"</button>
                        })}
                        {can_move.then(|| view! {
                            <>
                                <button
                                    class="btn small"
                                    aria-label="Move lane left"
                                    on:click=left
                                >
                                    "←"
                                </button>
                                <button
                                    class="btn small"
                                    aria-label="Move lane right"
                                    on:click=right
                                >
                                    "→"
                                </button>
                            </>
                        })}
                    </div>
                }.into_any()
            }}

            {move || {
                if !renaming.get() {
                    return ().into_any();
                }
                let save = {
                    let id = rename_id.clone();
                    move |event: web_sys::MouseEvent| {
                        event.stop_propagation();
                        let name = rename_name.get_untracked().trim().to_owned();
                        if name.is_empty() {
                            warn(ctx, "A lane name is required.");
                            return;
                        }
                        run_write(
                            ctx,
                            "lists.update",
                            json!({ "id": id.clone(), "name": name }),
                            Some(lane_version),
                            move |_| {
                                renaming.set(false);
                                resource.refetch();
                            },
                            || {},
                        );
                    }
                };
                let cancel = move |event: web_sys::MouseEvent| {
                    event.stop_propagation();
                    renaming.set(false);
                };
                view! {
                    <div class="formrow" style="margin-bottom:8px">
                        <label class="filter">
                            <span>"Lane name"</span>
                            <input
                                prop:value=move || rename_name.get()
                                on:input=move |event| rename_name.set(event_target_value(&event))
                            />
                        </label>
                        <button class="btn small primary" on:click=save>"Save"</button>
                        <button class="btn small" on:click=cancel>"Cancel"</button>
                    </div>
                }.into_any()
            }}

            <div class="lane-cards">
                {move || {
                    let board = cards_board.clone();
                    cards.get()
                        .into_iter()
                        .filter(|card| card.list_id == cards_list)
                        .map(|card| view! {
                            <CardTile
                                card=card
                                lane_kind=card_kind.clone()
                                current_list=cards_list.clone()
                                board=board.clone()
                            />
                        })
                        .collect_view()
                }}
            </div>

            {move || {
                let allowed = ctx.has_action("cards.create") && form_kind != "archive";
                let list_id = form_list.clone();
                let board = form_board.clone();
                allowed.then(|| view! { <NewCardForm list_id=list_id.clone() board=board.clone()/> })
            }}
        </section>
    }
}

#[component]
fn CardTile(
    card: CardOut,
    lane_kind: String,
    current_list: String,
    board: BoardCtx,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let resource = board.resource;
    let drag = board.drag;
    let editing = board.editing;
    let edit_name = board.edit_name;
    let edit_description = board.edit_description;
    let archived = board.archived;
    let lane_ids = board.lane_ids;
    let navigate = use_navigate();

    let strip_card = card.clone();
    let id = card.id.clone();
    let actions_id = id.clone();
    let edit_open_id = id.clone();
    let save_id = id.clone();
    let name = card.name.clone();
    let title = card.name.clone();
    let description = card.description.clone();
    let card_type = card.card_type_code.clone();
    let changed = card.list_changed_at;
    let advanced = format!("{} · v{}", card.id, card.version);
    let version = card.version;

    let open = {
        let id = id.clone();
        let navigate = navigate.clone();
        move |_| navigate(&format!("/card/{id}"), Default::default())
    };
    let drag_start = {
        let id = id.clone();
        move |_| drag.set(Some((id.clone(), version)))
    };
    let keyboard = {
        let id = id.clone();
        let current_list = current_list.clone();
        move |event: web_sys::KeyboardEvent| {
            let key = event.key();
            if key != "ArrowLeft" && key != "ArrowRight" {
                return;
            }
            // An arrow typed into the edit form must not move the card.
            if let Some(element) = event
                .target()
                .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
                && matches!(element.tag_name().as_str(), "INPUT" | "SELECT" | "TEXTAREA")
            {
                return;
            }
            let Some(position) = lane_ids.iter().position(|id| id == &current_list) else {
                return;
            };
            let target = if key == "ArrowLeft" && position > 0 {
                Some(lane_ids[position - 1].clone())
            } else if key == "ArrowRight" && position + 1 < lane_ids.len() {
                Some(lane_ids[position + 1].clone())
            } else {
                None
            };
            if let Some(target) = target {
                event.prevent_default();
                move_card(ctx, resource, id.clone(), target, version);
            }
        }
    };

    let stop_click = move |event: web_sys::MouseEvent| event.stop_propagation();
    let edit_open = move || editing.get().as_deref() == Some(edit_open_id.as_str());
    let plain_title = move || ctx.has_action("cards.update") || ctx.has_action("cards.archive");

    view! {
        <article
            class="card"
            style="position:relative"
            draggable="true"
            tabindex="0"
            on:dragstart=drag_start
            on:click=open
            on:keydown=keyboard
        >
            <div
                class="card-title"
                style=move || if plain_title() { "padding-right:112px" } else { "" }
            >
                {title}
            </div>
            <div class="card-meta">
                <span class="muted">{card_type}</span>
                <span class="age">{age(changed)}</span>
            </div>
            <span class="adv mono">{advanced}</span>
            <span class="adv mono">{lane_kind}</span>

            {move || {
                let allowed_edit = ctx.has_action("cards.update");
                let allowed_archive = ctx.has_action("cards.archive");
                if !(allowed_edit || allowed_archive) {
                    return ().into_any();
                }
                let edit_click = {
                    let id = actions_id.clone();
                    let name = name.clone();
                    let description = description.clone();
                    move |event: web_sys::MouseEvent| {
                        event.stop_propagation();
                        editing.set(Some(id.clone()));
                        edit_name.set(name.clone());
                        edit_description.set(description.clone());
                    }
                };
                let archive_click = {
                    let id = actions_id.clone();
                    let strip_card = strip_card.clone();
                    move |event: web_sys::MouseEvent| {
                        event.stop_propagation();
                        let strip_card = strip_card.clone();
                        run_write(
                            ctx,
                            "cards.archive",
                            json!({ "id": id.clone() }),
                            Some(version),
                            move |ok| {
                                if ok {
                                    archived.update(|items| items.push(strip_card.clone()));
                                }
                                resource.refetch();
                            },
                            || {},
                        );
                    }
                };
                view! {
                    <div
                        style="position:absolute;top:6px;right:6px;display:flex;gap:4px"
                        on:click=stop_click
                    >
                        {allowed_edit.then(|| view! {
                            <button class="btn small" on:click=edit_click>"Edit"</button>
                        })}
                        {allowed_archive.then(|| view! {
                            <button class="btn small" on:click=archive_click>"Archive"</button>
                        })}
                    </div>
                }.into_any()
            }}

            {move || {
                if !edit_open() {
                    return ().into_any();
                }
                let save = {
                    let id = save_id.clone();
                    move |event: web_sys::MouseEvent| {
                        event.stop_propagation();
                        let new_name = edit_name.get_untracked().trim().to_owned();
                        if new_name.is_empty() {
                            warn(ctx, "A card name is required.");
                            return;
                        }
                        run_write(
                            ctx,
                            "cards.update",
                            json!({
                                "id": id.clone(),
                                "name": new_name,
                                "description": edit_description.get_untracked().trim().to_owned(),
                            }),
                            Some(version),
                            move |_| {
                                editing.set(None);
                                resource.refetch();
                            },
                            || {},
                        );
                    }
                };
                let cancel = move |event: web_sys::MouseEvent| {
                    event.stop_propagation();
                    editing.set(None);
                };
                view! {
                    <div
                        class="card-edit"
                        style="margin-top:8px;display:flex;flex-direction:column;gap:6px"
                        on:click=stop_click
                    >
                        <label class="filter">
                            <span>"Card name"</span>
                            <input
                                prop:value=move || edit_name.get()
                                on:input=move |event| edit_name.set(event_target_value(&event))
                            />
                        </label>
                        <label class="filter">
                            <span>"Card description"</span>
                            <input
                                prop:value=move || edit_description.get()
                                on:input=move |event| edit_description.set(event_target_value(&event))
                            />
                        </label>
                        <div class="formrow">
                            <button class="btn small primary" on:click=save>"Save"</button>
                            <button class="btn small" on:click=cancel>"Cancel"</button>
                        </div>
                    </div>
                }.into_any()
            }}
        </article>
    }
}

#[component]
fn LabelsPanel(board: BoardCtx) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let resource = board.resource;
    let cards = board.cards;
    let board_id = board.board_id.clone();
    let label_list = board.labels.clone();
    let label_rows = board.labels;
    let name = RwSignal::new(String::new());
    let color = RwSignal::new("accent".to_owned());
    let busy = RwSignal::new(false);

    let create = move |event: web_sys::SubmitEvent| {
        event.prevent_default();
        let label = name.get_untracked().trim().to_owned();
        if label.is_empty() {
            warn(ctx, "A label name is required.");
            return;
        }
        let token = color.get_untracked().trim().to_owned();
        if token.is_empty() {
            warn(ctx, "A label color is required.");
            return;
        }
        busy.set(true);
        run_write(
            ctx,
            "labels.create",
            json!({ "board_id": board_id.clone(), "name": label, "color": token }),
            None,
            move |ok| {
                if ok {
                    name.set(String::new());
                }
                resource.refetch();
            },
            move || busy.set(false),
        );
    };

    view! {
        <section class="panel" style="flex:1 1 260px">
            <h2>"Labels"</h2>
            <p class="muted">"Board labels. Give one to a card with the row for that card."</p>
            <div class="formrow">
                {label_list.iter().map(|label| view! {
                    <span class="label">{label.name.clone()}</span>
                }).collect_view()}
            </div>
            {ctx.has_action("labels.create").then(|| view! {
                <form class="formrow" on:submit=create>
                    <label class="filter">
                        <span>"Label name"</span>
                        <input
                            prop:value=move || name.get()
                            on:input=move |event| name.set(event_target_value(&event))
                        />
                    </label>
                    <label class="filter">
                        <span>"Label color"</span>
                        <input
                            prop:value=move || color.get()
                            on:input=move |event| color.set(event_target_value(&event))
                        />
                    </label>
                    <button class="btn small primary" type="submit" disabled=move || busy.get()>
                        "Create label"
                    </button>
                </form>
            })}
            {move || {
                let allowed =
                    ctx.has_action("cards.add_label") || ctx.has_action("cards.remove_label");
                let rows = cards.get();
                if !allowed || rows.is_empty() {
                    return ().into_any();
                }
                view! {
                    <div>
                        {rows.into_iter().map(|card| view! {
                            <LabelRow
                                card_id=card.id
                                card_name=card.name
                                version=card.version
                                labels=label_rows.clone()
                                resource=resource
                            />
                        }).collect_view()}
                    </div>
                }.into_any()
            }}
        </section>
    }
}

#[component]
fn LabelRow(
    card_id: String,
    card_name: String,
    version: i64,
    labels: Vec<LabelOut>,
    resource: LocalResource<Result<BoardData, ApiError>>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let pick = RwSignal::new(String::new());
    let add_id = card_id.clone();
    let remove_id = card_id.clone();

    let add = move |_| {
        let label_id = pick.get_untracked();
        if label_id.is_empty() {
            warn(ctx, "Choose a label first.");
            return;
        }
        run_write(
            ctx,
            "cards.add_label",
            json!({ "card_id": add_id.clone(), "label_id": label_id }),
            Some(version),
            move |_| resource.refetch(),
            || {},
        );
    };
    let remove = move |_| {
        let label_id = pick.get_untracked();
        if label_id.is_empty() {
            warn(ctx, "Choose a label first.");
            return;
        }
        run_write(
            ctx,
            "cards.remove_label",
            json!({ "card_id": remove_id.clone(), "label_id": label_id }),
            Some(version),
            move |_| resource.refetch(),
            || {},
        );
    };

    view! {
        <div class="formrow">
            <span>{card_name}</span>
            <select on:change=move |event| pick.set(event_target_value(&event))>
                <option value="">"Choose a label"</option>
                {labels.into_iter().map(|label| view! {
                    <option value=label.id.clone()>{label.name.clone()}</option>
                }).collect_view()}
            </select>
            {ctx.has_action("cards.add_label").then(|| view! {
                <button class="btn small" on:click=add>"Add label"</button>
            })}
            {ctx.has_action("cards.remove_label").then(|| view! {
                <button class="btn small" on:click=remove>"Remove label"</button>
            })}
        </div>
    }
}

#[component]
fn FieldsPanel(board: BoardCtx) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let resource = board.resource;
    let cards = board.cards;
    let board_id = board.board_id.clone();
    let field_list = board.fields.clone();
    let field_rows = board.fields;
    let name = RwSignal::new(String::new());
    let kind = RwSignal::new("text".to_owned());
    let busy = RwSignal::new(false);

    let create = move |event: web_sys::SubmitEvent| {
        event.prevent_default();
        let field = name.get_untracked().trim().to_owned();
        if field.is_empty() {
            warn(ctx, "A field name is required.");
            return;
        }
        let field_kind = kind.get_untracked().trim().to_owned();
        if field_kind.is_empty() {
            warn(ctx, "A field kind is required.");
            return;
        }
        busy.set(true);
        run_write(
            ctx,
            "custom_fields.create",
            json!({ "board_id": board_id.clone(), "name": field, "kind": field_kind }),
            None,
            move |ok| {
                if ok {
                    name.set(String::new());
                }
                resource.refetch();
            },
            move || busy.set(false),
        );
    };

    view! {
        <section class="panel" style="flex:1 1 300px">
            <h2>"Fields"</h2>
            <p class="muted">
                "Board fields: text, number, date, select or checkbox. A value must match its kind."
            </p>
            <div class="formrow">
                {field_list.iter().map(|field| view! {
                    <span class="label">{format!("{} · {}", field.name, field.kind)}</span>
                }).collect_view()}
            </div>
            {ctx.has_action("custom_fields.create").then(|| view! {
                <form class="formrow" on:submit=create>
                    <label class="filter">
                        <span>"Field name"</span>
                        <input
                            prop:value=move || name.get()
                            on:input=move |event| name.set(event_target_value(&event))
                        />
                    </label>
                    <label class="filter">
                        <span>"Field kind"</span>
                        <input
                            prop:value=move || kind.get()
                            on:input=move |event| kind.set(event_target_value(&event))
                            placeholder="text"
                        />
                    </label>
                    <button class="btn small primary" type="submit" disabled=move || busy.get()>
                        "Create field"
                    </button>
                </form>
            })}
            {move || {
                let allowed = ctx.has_action("custom_fields.set_value")
                    || ctx.has_action("custom_fields.clear_value");
                let rows = cards.get();
                if !allowed || rows.is_empty() {
                    return ().into_any();
                }
                view! {
                    <div>
                        {rows.into_iter().map(|card| view! {
                            <FieldRow
                                card_id=card.id
                                card_name=card.name
                                fields=field_rows.clone()
                                resource=resource
                            />
                        }).collect_view()}
                    </div>
                }.into_any()
            }}
        </section>
    }
}

#[component]
fn FieldRow(
    card_id: String,
    card_name: String,
    fields: Vec<FieldBrief>,
    resource: LocalResource<Result<BoardData, ApiError>>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let pick = RwSignal::new(String::new());
    let content = RwSignal::new(String::new());
    let set_id = card_id.clone();
    let clear_id = card_id;
    let set_fields = fields.clone();

    let set = move |_| {
        let field_id = pick.get_untracked();
        let Some(field) = set_fields.iter().find(|field| field.id == field_id) else {
            warn(ctx, "Choose a field first.");
            return;
        };
        let value = match field_content(&field.kind, &content.get_untracked()) {
            Ok(value) => value,
            Err(message) => {
                warn(ctx, &message);
                return;
            }
        };
        run_write(
            ctx,
            "custom_fields.set_value",
            json!({
                "card_id": set_id.clone(),
                "custom_field_id": field_id,
                "content": value,
            }),
            None,
            move |_| resource.refetch(),
            || {},
        );
    };
    let clear = move |_| {
        let field_id = pick.get_untracked();
        if field_id.is_empty() {
            warn(ctx, "Choose a field first.");
            return;
        }
        run_write(
            ctx,
            "custom_fields.clear_value",
            json!({ "card_id": clear_id.clone(), "custom_field_id": field_id }),
            None,
            move |_| resource.refetch(),
            || {},
        );
    };

    view! {
        <div class="formrow">
            <span>{card_name}</span>
            <select on:change=move |event| pick.set(event_target_value(&event))>
                <option value="">"Choose a field"</option>
                {fields.iter().map(|field| {
                    let value = field.id.clone();
                    let label = format!("{} · {}", field.name, field.kind);
                    view! { <option value=value>{label}</option> }
                }).collect_view()}
            </select>
            <label class="filter">
                <span>"Field value"</span>
                <input
                    prop:value=move || content.get()
                    on:input=move |event| content.set(event_target_value(&event))
                />
            </label>
            {ctx.has_action("custom_fields.set_value").then(|| view! {
                <button class="btn small" on:click=set>"Set"</button>
            })}
            {ctx.has_action("custom_fields.clear_value").then(|| view! {
                <button class="btn small" on:click=clear>"Clear"</button>
            })}
        </div>
    }
}

#[component]
fn NewCardForm(list_id: String, board: BoardCtx) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let resource = board.resource;
    let board_id = board.board_id;
    // The draft lives in the screen's shared state, keyed by list: the resource-driven render
    // recreates this form on every refetch (an SSE resync does it right after a board loads),
    // and a form-local signal would silently lose what the person typed.
    let names = board.new_card_name;
    let types = board.new_card_type;
    let busy = RwSignal::new(false);

    let submit = {
        let list_id = list_id.clone();
        move |event: web_sys::SubmitEvent| {
            event.prevent_default();
            let card_name = names
                .get_untracked()
                .get(&list_id)
                .cloned()
                .unwrap_or_default()
                .trim()
                .to_owned();
            if card_name.is_empty() {
                warn(ctx, "A card name is required.");
                return;
            }
            let type_code = types
                .get_untracked()
                .get(&list_id)
                .cloned()
                .unwrap_or_else(|| "feature".to_owned())
                .trim()
                .to_owned();
            if type_code.is_empty() {
                warn(ctx, "A card type is required.");
                return;
            }
            busy.set(true);
            let clear_key = list_id.clone();
            run_write(
                ctx,
                "cards.create",
                json!({
                    "board_id": board_id.clone(),
                    "list_id": list_id.clone(),
                    "name": card_name,
                    "card_type_code": type_code,
                }),
                None,
                move |ok| {
                    if ok {
                        names.update(|map| {
                            map.remove(&clear_key);
                        });
                    }
                    resource.refetch();
                },
                move || busy.set(false),
            );
        }
    };

    let name_value = {
        let list_id = list_id.clone();
        move || names.get().get(&list_id).cloned().unwrap_or_default()
    };
    let name_input = {
        let list_id = list_id.clone();
        move |event| {
            let value = event_target_value(&event);
            names.update(|map| {
                map.insert(list_id.clone(), value);
            });
        }
    };
    let type_value = {
        let list_id = list_id.clone();
        move || {
            types
                .get()
                .get(&list_id)
                .cloned()
                .unwrap_or_else(|| "feature".to_owned())
        }
    };
    let type_input = {
        let list_id = list_id.clone();
        move |event| {
            let value = event_target_value(&event);
            types.update(|map| {
                map.insert(list_id.clone(), value);
            });
        }
    };

    view! {
        <form class="formrow" style="margin-top:8px" on:submit=submit>
            <label class="filter">
                <span>"Card name"</span>
                <input prop:value=name_value on:input=name_input placeholder="New card"/>
            </label>
            <label class="filter">
                <span>"Card type"</span>
                <input prop:value=type_value on:input=type_input placeholder="feature"/>
            </label>
            <button class="btn small primary" type="submit" disabled=move || busy.get()>
                {move || if busy.get() { "Adding…" } else { "Add card" }}
            </button>
        </form>
    }
}

#[component]
fn NewLaneForm(board: BoardCtx) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let resource = board.resource;
    let board_id = board.board_id;
    let name = RwSignal::new(String::new());
    let kind = RwSignal::new("active".to_owned());
    let busy = RwSignal::new(false);

    let create = move |event: web_sys::SubmitEvent| {
        event.prevent_default();
        let lane_name = name.get_untracked().trim().to_owned();
        if lane_name.is_empty() {
            warn(ctx, "A lane name is required.");
            return;
        }
        busy.set(true);
        run_write(
            ctx,
            "lists.create",
            json!({
                "board_id": board_id.clone(),
                "name": lane_name,
                "kind": kind.get_untracked(),
            }),
            None,
            move |ok| {
                if ok {
                    name.set(String::new());
                }
                resource.refetch();
            },
            move || busy.set(false),
        );
    };

    view! {
        <section class="panel" style="flex:1 1 220px">
            <h2>"New lane"</h2>
            <p class="muted">"A lane appends to the right of the board."</p>
            <form class="formrow" on:submit=create>
                <label class="filter">
                    <span>"New lane name"</span>
                    <input
                        prop:value=move || name.get()
                        on:input=move |event| name.set(event_target_value(&event))
                    />
                </label>
                <label class="filter">
                    <span>"Lane kind"</span>
                    <select on:change=move |event| kind.set(event_target_value(&event))>
                        {LANE_KINDS.iter().map(|value| view! {
                            <option value=*value selected=move || kind.get() == *value>{*value}</option>
                        }).collect_view()}
                    </select>
                </label>
                <button class="btn small primary" type="submit" disabled=move || busy.get()>
                    {move || if busy.get() { "Creating…" } else { "Create lane" }}
                </button>
            </form>
        </section>
    }
}

fn move_card(
    ctx: AppCtx,
    resource: LocalResource<Result<BoardData, ApiError>>,
    card_id: String,
    list_id: String,
    version: i64,
) {
    leptos::task::spawn_local(async move {
        let result = ctx
            .invoke(
                "cards.move",
                json!({ "id": card_id, "list_id": list_id }),
                InvokeOptions {
                    if_match: Some(version),
                    idempotency_key: None,
                },
            )
            .await;
        match result {
            Ok(_) => resource.refetch(),
            Err(error) => ctx.report(&error),
        }
    });
}
