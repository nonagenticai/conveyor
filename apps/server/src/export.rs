#![forbid(unsafe_code)]
//! `conveyor admin fixture|import|export|diff board` — the board-export/v1 toolchain (S05 §5).
//!
//! The live Planka database is five `[agent-contract-test]` cards, so a dump cannot be
//! evidence. `board-export/v1` is a **defined, key-referenced** document (S5-D1): every
//! reference is a stable key (`list-1`, `card-7`, `field-2`) and no uuid appears anywhere, so
//! two exports of the same board compare equal and a replay has something to prove.
//!
//! * `fixture` writes a deterministic document from a seed. It touches no database.
//! * `import` replays a document through the **real actions**, never raw SQL, acting as the
//!   tenant's own actor (`store::identity::tenant_actor`). One action invocation per row is
//!   what makes the replay exercise validation, events and webhooks rather than a bulk copy.
//! * `export` re-reads a board and writes the same shape, so `diff export(import(F)) F`
//!   measures fidelity.
//! * `diff` prints every differing JSON path and exits non-zero on any. `source` and
//!   `project.key` are references, not content: `source` names the producer, and an import
//!   that finds its key taken appends a suffix (there is no project delete action), so those
//!   two are reported separately and do not fail the gate.
//!
//! **What is not carried:** archived cards (open question 4's default — an archive is history
//! and would make every diff permanent), row ids, versions and server timestamps. Comments
//! carry their author by display name and import records the acting actor as the author; a
//! document naming a different author is refused rather than silently reattributed.

use std::collections::{BTreeSet, HashMap};

use conveyor_actions::invoke::{Authority, InvokeRequest, invoke};
use conveyor_auth::Role;
use conveyor_domain::TenantId;
use conveyor_store as store;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

/// The only format this module writes or accepts.
pub const FORMAT: &str = "board-export/v1";

/// `projects.create` names the board it creates; there is no `boards.update`, so an import
/// can only reproduce this name.
const BOARD_NAME: &str = "Board";

/// The five lanes `projects.create` creates from the `standard` board template.
const TEMPLATE_KINDS: [&str; 5] = ["intake", "active", "waiting", "done", "archive"];

/// How many extra-lane kinds a generated fixture cycles through (never `archive` — a card
/// cannot be born in the archive lane, S5-D6).
const EXTRA_KINDS: [&str; 3] = ["active", "waiting", "done"];

/// The three labels every generated fixture carries.
const LABELS: [(&str, &str); 3] = [("Bug", "red"), ("Feature", "green"), ("Docs", "blue")];

/// One page of cards/comments while exporting.
const EXPORT_PAGE: i64 = 200;

// ── errors ─────────────────────────────────────────────────────────────────────────────

/// Why an export/import command failed.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Refused(String),
    #[error("{action} refused ({status}): {detail}")]
    Action {
        /// The action that refused.
        action: String,
        /// Its HTTP status (a 409 is a taken project key, which the import retries).
        status: u16,
        /// Its problem detail.
        detail: String,
    },
}

impl ExportError {
    fn refused(message: impl Into<String>) -> Self {
        Self::Refused(message.into())
    }
}

impl From<store::list::ListStoreError> for ExportError {
    fn from(error: store::list::ListStoreError) -> Self {
        match error {
            store::list::ListStoreError::Database(error) => Self::Database(error),
            other => Self::refused(other.to_string()),
        }
    }
}

// ── fixture generation ────────────────────────────────────────────────────────────────

/// The knobs `admin fixture board` exposes.
#[derive(Clone, Debug)]
pub struct FixtureOptions {
    /// How many live cards to emit.
    pub cards: usize,
    /// How many lanes; must be at least the template's five.
    pub lists: usize,
    /// How many comments to distribute over the cards.
    pub comments: usize,
    /// How many checklist items to distribute over the cards.
    pub tasks: usize,
    /// How many board fields every card gets a value for.
    pub fields: usize,
    /// The seed; it names the project and nothing else, so the output is byte-stable.
    pub seed: u64,
}

/// A decimal position as the wire number, `1024 * step`.
fn position(step: usize) -> Value {
    json!(1024.0_f64 * step as f64)
}

/// The kind of lane `index` (0-based) in a generated board.
fn lane_kind(index: usize) -> &'static str {
    if index < TEMPLATE_KINDS.len() {
        TEMPLATE_KINDS[index]
    } else {
        EXTRA_KINDS[(index - TEMPLATE_KINDS.len()) % EXTRA_KINDS.len()]
    }
}

/// The display name of lane `index`; the first five match what `projects.create` writes.
fn lane_name(index: usize) -> String {
    if index < TEMPLATE_KINDS.len() {
        let mut chars = lane_kind(index).chars();
        match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    } else {
        format!("Lane {}", index + 1)
    }
}

/// The value a generated fixture records for one field of one card.
fn field_value(kind: &str, card: usize, field: usize) -> Value {
    match kind {
        "number" => json!(card + field),
        "checkbox" => json!(true),
        "date" => json!("2026-01-01"),
        "select" => json!("option-1"),
        _ => json!(format!("value-{card}-{field}")),
    }
}

/// Build the deterministic fixture document.
pub fn fixture_document(options: &FixtureOptions) -> Result<Value, ExportError> {
    if options.cards == 0 {
        return Err(ExportError::refused("--cards must be at least 1"));
    }
    if options.lists < TEMPLATE_KINDS.len() {
        return Err(ExportError::refused(format!(
            "--lists must be at least {}: `projects.create` writes the template's five lanes \
             and there is no `lists.delete` action to remove one",
            TEMPLATE_KINDS.len()
        )));
    }

    let key = format!("FIX-{}", options.seed);
    let lists: Vec<Value> = (0..options.lists)
        .map(|index| {
            json!({
                "key": format!("list-{}", index + 1),
                "name": lane_name(index),
                "kind": lane_kind(index),
                "position": position(index + 1),
                "color": Value::Null,
                "wip_limit": Value::Null,
            })
        })
        .collect();

    let labels: Vec<Value> = LABELS
        .iter()
        .enumerate()
        .map(|(index, (name, color))| {
            json!({
                "key": format!("label-{}", index + 1),
                "name": name,
                "color": color,
                "position": position(index + 1),
            })
        })
        .collect();

    let field_kinds = ["text", "number", "date", "select", "checkbox"];
    let fields: Vec<Value> = (0..options.fields)
        .map(|index| {
            json!({
                "key": format!("field-{}", index + 1),
                "group_name": "",
                "group_position": json!(0.0_f64),
                "name": format!("Field {}", index + 1),
                "kind": field_kinds[index % field_kinds.len()],
                "options": if field_kinds[index % field_kinds.len()] == "select" {
                    json!(["option-1", "option-2"])
                } else {
                    json!([])
                },
                "position": position(index + 1),
            })
        })
        .collect();

    let members = vec![json!({
        "key": "member-1",
        "user": "Alice",
        "role": "owner",
        "can_comment": true,
        "watching": true,
    })];

    // Cards are emitted LIST-MAJOR (lane by lane, each lane in position order), which is the
    // order `export` re-reads them in. Cards never land in the archive lane (S5-D6).
    let open_lanes: Vec<usize> = (0..options.lists)
        .filter(|index| lane_kind(*index) != "archive")
        .collect();
    let mut per_lane: HashMap<usize, Vec<usize>> = HashMap::new();
    for card in 0..options.cards {
        let lane = open_lanes[card % open_lanes.len()];
        per_lane.entry(lane).or_default().push(card);
    }

    let mut card_key = 0usize;
    let mut comment_key = 0usize;
    let mut task_list_key = 0usize;
    let mut task_key = 0usize;
    let mut cards = Vec::with_capacity(options.cards);
    for lane in &open_lanes {
        let indices = per_lane.get(lane).cloned().unwrap_or_default();
        for (slot, card_index) in indices.iter().enumerate() {
            card_key += 1;

            let comment_indices: Vec<usize> = (0..options.comments)
                .filter(|index| index % options.cards == *card_index)
                .collect();
            let comments: Vec<Value> = comment_indices
                .into_iter()
                .map(|index| {
                    comment_key += 1;
                    json!({
                        "key": format!("comment-{comment_key}"),
                        "author": "Alice",
                        "body": format!("Comment {}", index + 1),
                    })
                })
                .collect();

            let task_indices: Vec<usize> = (0..options.tasks)
                .filter(|index| index % options.cards == *card_index)
                .collect();
            let task_lists = if task_indices.is_empty() {
                Vec::new()
            } else {
                task_list_key += 1;
                let tasks: Vec<Value> = task_indices
                    .into_iter()
                    .enumerate()
                    .map(|(slot, index)| {
                        task_key += 1;
                        json!({
                            "key": format!("task-{task_key}"),
                            "name": format!("Task {}", index + 1),
                            "done": false,
                            "position": position(slot + 1),
                        })
                    })
                    .collect();
                vec![json!({
                    "key": format!("task_list-{task_list_key}"),
                    "name": "Checklist",
                    "position": position(1),
                    "hide_completed": false,
                    "show_on_front": false,
                    "tasks": tasks,
                })]
            };

            let mut values = serde_json::Map::new();
            for (field, entry) in fields.iter().enumerate() {
                let field_key = entry["key"].as_str().unwrap_or_default().to_owned();
                let kind = entry["kind"].as_str().unwrap_or("text");
                values.insert(field_key, field_value(kind, *card_index, field));
            }

            cards.push(json!({
                "key": format!("card-{card_key}"),
                "list_key": format!("list-{}", lane + 1),
                "position": position(slot + 1),
                "name": format!("Card {}", card_key),
                "description": "",
                "card_type_code": if card_key % 2 == 1 { "feature" } else { "bug" },
                "due_at": Value::Null,
                "parent_key": Value::Null,
                "archived_at": Value::Null,
                "labels": [format!("label-{}", (card_key - 1) % LABELS.len() + 1)],
                "comments": comments,
                "task_lists": task_lists,
                "custom_field_values": values,
            }));
        }
    }

    Ok(json!({
        "format": FORMAT,
        "source": "fixture",
        "project": {
            "key": key,
            "name": format!("Fixture {}", options.seed),
            "project_type_code": "delivery",
            "description": "",
        },
        "boards": [{
            "board": { "name": BOARD_NAME },
            "lists": lists,
            "labels": labels,
            "custom_fields": fields,
            "members": members,
            "cards": cards,
        }],
    }))
}

// ── export ────────────────────────────────────────────────────────────────────────────

/// Re-read one board and render `board-export/v1`.
///
/// The board is resolved by scanning the active tenants in their own scope: `--board` names
/// no tenant, and every tenant table is FORCE-RLS'd, so there is no unscoped read to lean on.
pub async fn export_board(pool: &PgPool, board_id: Uuid) -> Result<Value, ExportError> {
    for tenant in store::tenants::active_ids(pool).await? {
        let mut tx = store::begin_scoped(pool, tenant).await?;
        let Some(board) = store::board::get_board_tx(&mut tx, tenant, board_id).await? else {
            tx.rollback().await?;
            continue;
        };
        let project = store::project::get_project_tx(&mut tx, tenant, board.project_id)
            .await?
            .ok_or_else(|| {
                ExportError::refused(format!("board {board_id} names a project that is gone"))
            })?;
        let document = build_export_document(&mut tx, tenant, &project, &board).await?;
        tx.commit().await?;
        return Ok(document);
    }
    Err(ExportError::refused(format!(
        "no live board {board_id} in any active tenant"
    )))
}

async fn build_export_document(
    tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
    tenant: TenantId,
    project: &conveyor_domain::Project,
    board: &conveyor_domain::Board,
) -> Result<Value, ExportError> {
    let lists = store::list::list_records_tx(tx, tenant, board.id).await?;
    let mut list_keys: HashMap<Uuid, String> = HashMap::new();
    for (index, list) in lists.iter().enumerate() {
        list_keys.insert(list.list.id, format!("list-{}", index + 1));
    }

    let labels = store::board::list_labels_tx(tx, tenant, board.id).await?;
    let mut label_keys: HashMap<Uuid, String> = HashMap::new();
    for (index, label) in labels.iter().enumerate() {
        label_keys.insert(label.id, format!("label-{}", index + 1));
    }

    let fields = store::board::list_custom_fields_tx(tx, tenant, board.id).await?;
    let mut field_keys: HashMap<Uuid, String> = HashMap::new();
    for (index, field) in fields.iter().enumerate() {
        field_keys.insert(field.id, format!("field-{}", index + 1));
    }

    let members = store::board::list_board_members_tx(tx, tenant, board.id).await?;

    // Every live card, keyed in the order the document emits them: lane by lane.
    let mut live = Vec::new();
    let mut cursor = None;
    loop {
        let page = store::card::card_page_tx(tx, tenant, board.id, cursor, EXPORT_PAGE).await?;
        let done = page.len() < EXPORT_PAGE as usize;
        if let Some(last) = page.last() {
            cursor = Some((last.position, last.card.id));
        }
        live.extend(page);
        if done {
            break;
        }
    }
    let mut grouped: Vec<Vec<store::card::CardRecord>> = Vec::with_capacity(lists.len());
    for list in &lists {
        grouped.push(
            live.iter()
                .filter(|card| card.card.list_id == list.list.id)
                .cloned()
                .collect(),
        );
    }
    let mut card_keys: HashMap<Uuid, String> = HashMap::new();
    let mut ordered: Vec<&store::card::CardRecord> = Vec::with_capacity(live.len());
    for group in &grouped {
        for card in group {
            ordered.push(card);
        }
    }
    for (index, card) in ordered.iter().enumerate() {
        card_keys.insert(card.card.id, format!("card-{}", index + 1));
    }

    let mut card_key = 0usize;
    let mut comment_key = 0usize;
    let mut task_list_key = 0usize;
    let mut task_key = 0usize;
    let mut cards = Vec::with_capacity(ordered.len());
    for record in &ordered {
        card_key += 1;
        let card_id = record.card.id;

        let card_labels = store::label::list_card_labels_tx(tx, tenant, card_id).await?;
        let label_refs: Vec<Value> = card_labels
            .iter()
            .filter_map(|label| label_keys.get(&label.id).cloned())
            .map(Value::String)
            .collect();

        let mut comments = Vec::new();
        let mut comment_cursor: Option<(chrono::DateTime<chrono::Utc>, Uuid)> = None;
        loop {
            let page =
                store::comment::comment_page_tx(tx, tenant, card_id, comment_cursor, EXPORT_PAGE)
                    .await?;
            let done = page.len() < EXPORT_PAGE as usize;
            if let Some(last) = page.last() {
                comment_cursor = Some((last.created_at, last.id));
            }
            for comment in page {
                comment_key += 1;
                comments.push(json!({
                    "key": format!("comment-{comment_key}"),
                    "author": comment.author_name.unwrap_or_default(),
                    "body": comment.body,
                }));
            }
            if done {
                break;
            }
        }

        let task_lists = store::task::list_task_lists_tx(tx, tenant, card_id).await?;
        let tasks = store::task::list_tasks_for_card_tx(tx, tenant, card_id).await?;
        let mut by_list: HashMap<Uuid, Vec<&store::task::TaskRecord>> = HashMap::new();
        for task in &tasks {
            by_list.entry(task.task_list_id).or_default().push(task);
        }
        let mut task_list_docs = Vec::with_capacity(task_lists.len());
        for list in &task_lists {
            task_list_key += 1;
            let mut task_docs = Vec::new();
            for task in by_list.get(&list.id).cloned().unwrap_or_default() {
                task_key += 1;
                task_docs.push(json!({
                    "key": format!("task-{task_key}"),
                    "name": task.name,
                    "done": task.done,
                    "position": conveyor_actions::board::views::position_f64(task.position),
                }));
            }
            task_list_docs.push(json!({
                "key": format!("task_list-{task_list_key}"),
                "name": list.name,
                "position": conveyor_actions::board::views::position_f64(list.position),
                "hide_completed": list.hide_completed,
                "show_on_front": list.show_on_front,
                "tasks": task_docs,
            }));
        }

        let values = store::custom_field::list_values_tx(tx, tenant, card_id).await?;
        let mut value_docs = serde_json::Map::new();
        for value in values {
            if let Some(key) = field_keys.get(&value.custom_field_id) {
                value_docs.insert(key.clone(), value.content);
            }
        }

        cards.push(json!({
            "key": format!("card-{card_key}"),
            "list_key": list_keys
                .get(&record.card.list_id)
                .cloned()
                .unwrap_or_default(),
            "position": conveyor_actions::board::views::position_f64(record.position),
            "name": record.name,
            "description": record.description,
            "card_type_code": record.card_type_code,
            "due_at": record.due_at,
            "parent_key": record
                .parent_card_id
                .and_then(|id| card_keys.get(&id).cloned()),
            "archived_at": record.card.archived_at,
            "labels": label_refs,
            "comments": comments,
            "task_lists": task_list_docs,
            "custom_field_values": value_docs,
        }));
    }

    Ok(json!({
        "format": FORMAT,
        "source": "conveyor",
        "project": {
            "key": project.key,
            "name": project.name,
            "project_type_code": project.project_type_code,
            "description": project.description,
        },
        "boards": [{
            "board": { "name": board.name },
            "lists": lists.iter().enumerate().map(|(index, list)| json!({
                "key": format!("list-{}", index + 1),
                "name": list.list.name,
                "kind": list.list.kind.as_str(),
                "position": conveyor_actions::board::views::position_f64(list.position),
                "color": list.color,
                "wip_limit": list.list.wip_limit,
            })).collect::<Vec<Value>>(),
            "labels": labels.iter().enumerate().map(|(index, label)| json!({
                "key": format!("label-{}", index + 1),
                "name": label.name,
                "color": label.color,
                "position": conveyor_actions::board::views::position_f64(label.position),
            })).collect::<Vec<Value>>(),
            "custom_fields": fields.iter().enumerate().map(|(index, field)| json!({
                "key": format!("field-{}", index + 1),
                "group_name": field.group_name,
                "group_position": conveyor_actions::board::views::position_f64(field.group_position),
                "name": field.name,
                "kind": field.kind,
                "options": field.options,
                "position": conveyor_actions::board::views::position_f64(field.position),
            })).collect::<Vec<Value>>(),
            "members": members.iter().enumerate().map(|(index, member)| json!({
                "key": format!("member-{}", index + 1),
                "user": member.display_name,
                "role": member.role,
                "can_comment": member.can_comment,
                "watching": member.watching,
            })).collect::<Vec<Value>>(),
            "cards": cards,
        }],
    }))
}

// ── import ───────────────────────────────────────────────────────────────────────────

/// Replay one `board-export/v1` document through the real actions.
///
/// Returns the report the CLI prints: `{board_id, project_id, cards, lists, …}`.
pub async fn import_board(
    pool: &PgPool,
    tenant: TenantId,
    document: &Value,
    app_key: &str,
) -> Result<Value, ExportError> {
    if document.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(ExportError::refused(format!(
            "not a {FORMAT} document (format is {:?})",
            document.get("format")
        )));
    }
    let project = document
        .get("project")
        .ok_or_else(|| ExportError::refused("the document has no project"))?;
    let boards = document
        .get("boards")
        .and_then(Value::as_array)
        .ok_or_else(|| ExportError::refused("the document has no boards[]"))?;
    if boards.len() != 1 {
        return Err(ExportError::refused(format!(
            "board-export/v1 carries exactly one board in this build; the document has {}",
            boards.len()
        )));
    }
    let board_doc = &boards[0];
    if board_doc
        .get("board")
        .and_then(|board| board.get("name"))
        .and_then(Value::as_str)
        != Some(BOARD_NAME)
    {
        return Err(ExportError::refused(format!(
            "`projects.create` names its board {BOARD_NAME:?} and there is no boards.update; \
             the document names another board"
        )));
    }

    let actor = store::identity::tenant_actor(pool, tenant)
        .await?
        .ok_or_else(|| {
            ExportError::refused(
                "the tenant has no active member to act as; seed one before importing",
            )
        })?;
    let actor_name = {
        let mut tx = store::begin_scoped(pool, tenant).await?;
        let name = store::identity::display_name_tx(&mut tx, actor).await?;
        tx.commit().await?;
        name.unwrap_or_default()
    };
    let importer = Importer {
        pool,
        tenant,
        actor,
        actor_name,
        app_key: app_key.to_owned(),
    };

    // 1. The project (and its board plus the template's five lanes) in one action.
    let key = project
        .get("key")
        .and_then(Value::as_str)
        .ok_or_else(|| ExportError::refused("project.key is required"))?;
    let title = project
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| ExportError::refused("project.name is required"))?;
    let project_input = |key: &str| {
        json!({
            "title": title,
            "key": key,
            "project_type_code": project.get("project_type_code").cloned().unwrap_or(Value::Null),
            "description": project.get("description").cloned().unwrap_or(json!("")),
        })
    };
    let project_id = match importer
        .call("projects.create", project_input(key), None)
        .await
    {
        Ok(value) => id_of(&value, "id")?,
        // A repeated gate run must not need a database reset. There is no project delete
        // action, so the import takes the next free key and reports it; `diff` treats
        // `project.key` as a reference, not content.
        Err(ExportError::Action { status: 409, .. }) => {
            let suffixed = format!("{key}-{}", Uuid::now_v7().simple());
            let value = importer
                .call("projects.create", project_input(&suffixed), None)
                .await?;
            id_of(&value, "id")?
        }
        Err(other) => return Err(other),
    };

    let view = importer
        .call("projects.get", json!({ "id": project_id }), None)
        .await?;
    let board_id = view["boards"]
        .as_array()
        .and_then(|boards| boards.first())
        .ok_or_else(|| ExportError::refused("projects.create returned no board"))?
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| ExportError::refused("the created board has no id"))?;

    // 2. Reconcile the template's lanes with the document's, in position order.
    let board_view = importer
        .call("boards.get", json!({ "id": board_id }), None)
        .await?;
    let existing_lists = board_view
        .get("lists")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let wanted_lists = board_doc
        .get("lists")
        .and_then(Value::as_array)
        .ok_or_else(|| ExportError::refused("boards[0].lists is required"))?;
    if wanted_lists.len() < existing_lists.len() {
        return Err(ExportError::refused(format!(
            "the document names {} lanes but the template created {} and there is no \
             lists.delete action",
            wanted_lists.len(),
            existing_lists.len()
        )));
    }
    let mut list_ids: HashMap<String, String> = HashMap::new();
    let mut previous_list_id: Option<String> = None;
    for (index, wanted) in wanted_lists.iter().enumerate() {
        let doc_key = required_key(wanted, "lists")?;
        let name = wanted
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| ExportError::refused(format!("lane {doc_key} has no name")))?;
        let kind = wanted
            .get("kind")
            .and_then(Value::as_str)
            .ok_or_else(|| ExportError::refused(format!("lane {doc_key} has no kind")))?;
        let color = wanted.get("color").cloned().unwrap_or(Value::Null);
        let wip_limit = wanted.get("wip_limit").cloned().unwrap_or(Value::Null);
        let id = if let Some(existing) = existing_lists.get(index) {
            let id = existing
                .get("list")
                .and_then(|list| list.get("id"))
                .and_then(Value::as_str)
                .ok_or_else(|| ExportError::refused("boards.get returned a lane without an id"))?
                .to_owned();
            let version = existing
                .get("list")
                .and_then(|list| list.get("version"))
                .and_then(Value::as_i64)
                .ok_or_else(|| {
                    ExportError::refused("boards.get returned a lane without a version")
                })?;
            importer
                .call(
                    "lists.update",
                    json!({
                        "id": id,
                        "name": name,
                        "kind": kind,
                        "color": color,
                        "wip_limit": wip_limit,
                    }),
                    Some(version),
                )
                .await?;
            id
        } else {
            let created = importer
                .call(
                    "lists.create",
                    json!({
                        "board_id": board_id,
                        "name": name,
                        "kind": kind,
                        "color": color,
                        "wip_limit": wip_limit,
                        "after_id": previous_list_id,
                    }),
                    None,
                )
                .await?;
            id_of(&created, "id")?
        };
        list_ids.insert(doc_key, id.clone());
        previous_list_id = Some(id);
    }

    // 3. Labels and fields, before any card can reference them.
    let mut label_ids: HashMap<String, String> = HashMap::new();
    for label in board_doc
        .get("labels")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let doc_key = required_key(&label, "labels")?;
        let created = importer
            .call(
                "labels.create",
                json!({
                    "board_id": board_id,
                    "name": label.get("name").cloned().unwrap_or(json!("")),
                    "color": label.get("color").cloned().unwrap_or(json!("grey")),
                }),
                None,
            )
            .await?;
        label_ids.insert(doc_key, id_of(&created, "id")?);
    }

    let mut field_ids: HashMap<String, String> = HashMap::new();
    for field in board_doc
        .get("custom_fields")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let doc_key = required_key(&field, "custom_fields")?;
        let created = importer
            .call(
                "custom_fields.create",
                json!({
                    "board_id": board_id,
                    "name": field.get("name").cloned().unwrap_or(json!("")),
                    "kind": field.get("kind").cloned().unwrap_or(json!("text")),
                    "group_name": field.get("group_name").cloned().unwrap_or(json!("")),
                    "group_position": field.get("group_position").cloned().unwrap_or(json!(0.0)),
                    "options": field.get("options").cloned().unwrap_or(json!([])),
                }),
                None,
            )
            .await?;
        field_ids.insert(doc_key, id_of(&created, "id")?);
    }

    // 4. Members, by display name (the document carries no user ids).
    let mut members_imported = 0usize;
    for member in board_doc
        .get("members")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let name = member
            .get("user")
            .and_then(Value::as_str)
            .ok_or_else(|| ExportError::refused("a member has no `user` display name"))?;
        let user = {
            let mut tx = store::begin_scoped(pool, tenant).await?;
            let users = store::board::resolve_member_users_tx(&mut tx, tenant, name).await?;
            tx.commit().await?;
            match users.as_slice() {
                [user] => *user,
                [] => {
                    return Err(ExportError::refused(format!(
                        "no active tenant member is named {name:?}"
                    )));
                }
                _ => {
                    return Err(ExportError::refused(format!(
                        "more than one active tenant member is named {name:?}; the export \
                         format cannot disambiguate"
                    )));
                }
            }
        };
        importer
            .call(
                "boards.set_member",
                json!({
                    "board_id": board_id,
                    "user_id": user,
                    "role": member.get("role").cloned().unwrap_or(json!("member")),
                    "can_comment": member.get("can_comment").cloned().unwrap_or(json!(true)),
                    "watching": member.get("watching").cloned().unwrap_or(json!(true)),
                }),
                None,
            )
            .await?;
        members_imported += 1;
    }

    // 5. Cards, in document order (which is lane-major, so positions append exactly).
    let cards = board_doc
        .get("cards")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut card_ids: HashMap<String, String> = HashMap::new();
    let mut comments_imported = 0usize;
    let mut tasks_imported = 0usize;
    let mut values_imported = 0usize;
    let mut labels_attached = 0usize;
    for card in &cards {
        let doc_key = required_key(card, "cards")?;
        let list_key = card
            .get("list_key")
            .and_then(Value::as_str)
            .ok_or_else(|| ExportError::refused(format!("card {doc_key} has no list_key")))?;
        let list_id = list_ids.get(list_key).ok_or_else(|| {
            ExportError::refused(format!("card {doc_key} names an unknown lane {list_key:?}"))
        })?;
        let parent_card_id = match card.get("parent_key").and_then(Value::as_str) {
            None => None,
            Some(parent) => {
                let id = card_ids.get(parent).ok_or_else(|| {
                    ExportError::refused(format!(
                        "card {doc_key} names parent {parent:?}, which is not earlier in the \
                         document; `cards.create` is the only action that sets a parent"
                    ))
                })?;
                Some(Uuid::parse_str(id).map_err(|error| {
                    ExportError::refused(format!("card {doc_key} has an unusable parent: {error}"))
                })?)
            }
        };
        let created = importer
            .call(
                "cards.create",
                json!({
                    "board_id": board_id,
                    "list_id": list_id,
                    "name": card.get("name").cloned().unwrap_or(json!("")),
                    "description": card.get("description").cloned().unwrap_or(json!("")),
                    "card_type_code": card.get("card_type_code").cloned().unwrap_or(json!("feature")),
                    "due_at": card.get("due_at").cloned().unwrap_or(Value::Null),
                    "parent_card_id": parent_card_id,
                }),
                None,
            )
            .await?;
        let card_id = id_of(&created, "id")?;
        let mut version = created.get("version").and_then(Value::as_i64).unwrap_or(1);

        for label in card
            .get("labels")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            let label_key = label.as_str().ok_or_else(|| {
                ExportError::refused(format!("card {doc_key} has a non-string label"))
            })?;
            let label_id = label_ids.get(label_key).ok_or_else(|| {
                ExportError::refused(format!(
                    "card {doc_key} names an unknown label {label_key:?}"
                ))
            })?;
            let attached = importer
                .call(
                    "cards.add_label",
                    json!({ "card_id": card_id, "label_id": label_id }),
                    Some(version),
                )
                .await?;
            version = attached
                .get("version")
                .and_then(Value::as_i64)
                .unwrap_or(version + 1);
            labels_attached += 1;
        }

        for comment in card
            .get("comments")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            let author = comment
                .get("author")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if author != importer.actor_name {
                return Err(ExportError::refused(format!(
                    "card {doc_key} has a comment authored by {author:?}; import records the \
                     acting member ({:?}) as every comment's author",
                    importer.actor_name
                )));
            }
            importer
                .call(
                    "cards.comment",
                    json!({
                        "id": card_id,
                        "body": comment.get("body").cloned().unwrap_or(json!("")),
                    }),
                    None,
                )
                .await?;
            comments_imported += 1;
        }

        for task_list in card
            .get("task_lists")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            let created_list = importer
                .call(
                    "task_lists.create",
                    json!({
                        "card_id": card_id,
                        "name": task_list.get("name").cloned().unwrap_or(json!("Checklist")),
                    }),
                    None,
                )
                .await?;
            let task_list_id = id_of(&created_list, "id")?;
            for task in task_list
                .get("tasks")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
            {
                let created_task = importer
                    .call(
                        "tasks.create",
                        json!({
                            "task_list_id": task_list_id,
                            "name": task.get("name").cloned().unwrap_or(json!("")),
                        }),
                        None,
                    )
                    .await?;
                let task_id = id_of(&created_task, "id")?;
                if task.get("done").and_then(Value::as_bool).unwrap_or(false) {
                    let task_version = created_task
                        .get("version")
                        .and_then(Value::as_i64)
                        .unwrap_or(1);
                    importer
                        .call(
                            "tasks.update",
                            json!({ "id": task_id, "done": true }),
                            Some(task_version),
                        )
                        .await?;
                }
                tasks_imported += 1;
            }
        }

        for (field_key, content) in card
            .get("custom_field_values")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
        {
            let field_id = field_ids.get(&field_key).ok_or_else(|| {
                ExportError::refused(format!(
                    "card {doc_key} names an unknown field {field_key:?}"
                ))
            })?;
            importer
                .call(
                    "custom_fields.set_value",
                    json!({
                        "card_id": card_id,
                        "custom_field_id": field_id,
                        "content": content,
                    }),
                    None,
                )
                .await?;
            values_imported += 1;
        }

        if card
            .get("archived_at")
            .is_some_and(|value| !value.is_null())
        {
            importer
                .call("cards.archive", json!({ "id": card_id }), Some(version))
                .await?;
        }

        card_ids.insert(doc_key, card_id);
    }

    Ok(json!({
        "board_id": board_id,
        "project_id": project_id,
        "cards": cards.len(),
        "lists": wanted_lists.len(),
        "labels": label_ids.len(),
        "labels_attached": labels_attached,
        "custom_fields": field_ids.len(),
        "members": members_imported,
        "comments": comments_imported,
        "tasks": tasks_imported,
        "custom_field_values": values_imported,
    }))
}

fn required_key(value: &Value, what: &str) -> Result<String, ExportError> {
    value
        .get("key")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| ExportError::refused(format!("a {what} entry has no key")))
}

fn id_of(value: &Value, field: &str) -> Result<String, ExportError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| ExportError::refused(format!("an action's output has no {field}")))
}

/// One tenant-scoped action caller.
struct Importer<'a> {
    pool: &'a PgPool,
    tenant: TenantId,
    actor: Uuid,
    actor_name: String,
    app_key: String,
}

impl Importer<'_> {
    /// Invoke one action as the tenant's actor. The action's own transaction is the unit of
    /// work, exactly as an HTTP caller would get.
    async fn call(
        &self,
        action: &str,
        input: Value,
        precondition: Option<i64>,
    ) -> Result<Value, ExportError> {
        let outcome = invoke(
            self.pool,
            conveyor_actions::registry(),
            self.tenant,
            InvokeRequest {
                action: action.to_owned(),
                input,
                precondition: precondition.map(conveyor_actions::Precondition::Version),
                idempotency_key: None,
                actor_id: self.actor,
                actor_kind: "service".to_owned(),
                role: Role::KataExecutor,
                policy: Default::default(),
                authority: Authority::Internal,
                request_id: None,
                trace_id: None,
                depth: 0,
                app_key: self.app_key.clone(),
                usage: None,
            },
        )
        .await
        .map_err(|error| ExportError::Action {
            action: action.to_owned(),
            status: error.status,
            detail: error.detail,
        })?;
        Ok(outcome.body.get("output").cloned().unwrap_or(Value::Null))
    }
}

// ── diff ─────────────────────────────────────────────────────────────────────────────

/// Every differing path between two documents, as human-readable lines.
///
/// `source` and `project.key` are reported but do not count as content deltas (see the module
/// doc): the first names the producer, the second is a reference an import may have to
/// re-mint. Every other difference is a delta.
pub fn diff_documents(left: &Value, right: &Value) -> (Vec<String>, Vec<String>) {
    let mut deltas = Vec::new();
    let mut references = Vec::new();
    diff_at("", Some(left), Some(right), &mut deltas, &mut references);
    (deltas, references)
}

fn diff_at(
    path: &str,
    left: Option<&Value>,
    right: Option<&Value>,
    deltas: &mut Vec<String>,
    references: &mut Vec<String>,
) {
    if left == right {
        return;
    }
    match (left, right) {
        (None, None) => {}
        (Some(Value::Object(a)), Some(Value::Object(b))) => {
            let keys: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
            for key in keys {
                let next = format!("{path}/{key}");
                diff_at(&next, a.get(key), b.get(key), deltas, references);
            }
        }
        (Some(Value::Array(a)), Some(Value::Array(b))) => {
            if a.len() != b.len() {
                push(
                    &format!("{path}/length"),
                    &json!(a.len()),
                    &json!(b.len()),
                    deltas,
                    references,
                );
            }
            for (index, (a, b)) in a.iter().zip(b.iter()).enumerate() {
                diff_at(
                    &format!("{path}/{index}"),
                    Some(a),
                    Some(b),
                    deltas,
                    references,
                );
            }
        }
        (Some(left), Some(right)) => push(path, left, right, deltas, references),
        (left, right) => push(
            path,
            left.unwrap_or(&Value::Null),
            right.unwrap_or(&Value::Null),
            deltas,
            references,
        ),
    }
}

fn push(
    path: &str,
    left: &Value,
    right: &Value,
    deltas: &mut Vec<String>,
    references: &mut Vec<String>,
) {
    let line = format!("{path}: {} != {}", compact(left), compact(right));
    if path == "/source" || path == "/project/key" {
        references.push(line);
    } else {
        deltas.push(line);
    }
}

fn compact(value: &Value) -> String {
    let rendered = serde_json::to_string(value).unwrap_or_else(|_| "?".to_owned());
    if rendered.chars().count() > 120 {
        format!("{}…", rendered.chars().take(120).collect::<String>())
    } else {
        rendered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(
        cards: usize,
        lists: usize,
        comments: usize,
        tasks: usize,
        fields: usize,
    ) -> FixtureOptions {
        FixtureOptions {
            cards,
            lists,
            comments,
            tasks,
            fields,
            seed: 7,
        }
    }

    #[test]
    fn a_fixture_is_deterministic_and_carries_no_uuids() {
        let first = fixture_document(&options(20, 5, 30, 40, 6)).expect("fixture");
        let second = fixture_document(&options(20, 5, 30, 40, 6)).expect("fixture");
        assert_eq!(first, second, "the same seed is the same document");
        let rendered = serde_json::to_string(&first).expect("json");
        assert!(
            !rendered.contains("0000-0000"),
            "no uuid may appear in a key-referenced document"
        );
        assert_eq!(
            first["boards"][0]["cards"].as_array().expect("cards").len(),
            20
        );
        let comments: usize = first["boards"][0]["cards"]
            .as_array()
            .expect("cards")
            .iter()
            .map(|card| card["comments"].as_array().map_or(0, Vec::len))
            .sum();
        assert_eq!(comments, 30);
        let tasks: usize = first["boards"][0]["cards"]
            .as_array()
            .expect("cards")
            .iter()
            .flat_map(|card| card["task_lists"].as_array().cloned().unwrap_or_default())
            .map(|list| list["tasks"].as_array().map_or(0, Vec::len))
            .sum();
        assert_eq!(tasks, 40);
    }

    #[test]
    fn a_fixture_refuses_fewer_lanes_than_the_template() {
        assert!(fixture_document(&options(1, 4, 0, 0, 0)).is_err());
        assert!(fixture_document(&options(0, 5, 0, 0, 0)).is_err());
    }

    #[test]
    fn diff_reports_content_and_separates_references() {
        let a =
            json!({"format": FORMAT, "source": "fixture", "project": {"key": "A", "name": "x"}});
        let b =
            json!({"format": FORMAT, "source": "conveyor", "project": {"key": "B", "name": "y"}});
        let (deltas, references) = diff_documents(&a, &b);
        assert_eq!(deltas.len(), 1, "{deltas:?}");
        assert!(deltas[0].contains("name"));
        assert_eq!(references.len(), 2, "{references:?}");
    }
}
