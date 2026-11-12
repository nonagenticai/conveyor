#![forbid(unsafe_code)]
//! The board surface's request and response documents.
//!
//! Every type derives `serde` **and** `schemars::JsonSchema` from the same fields, so the
//! input validator, the OpenAPI operation and the MCP tool cannot disagree (S04 §5.1).
//!
//! `position` is a **display** number (`f64`) on the wire and an exact `numeric` in the
//! database. A client never sends a position back — `lists.move`/`cards` address a slot by
//! `after_id` — so the only lossy conversion happens on a value nothing does arithmetic on.
//! The server's ordering, midpoints and cursors are all `Decimal`.

use chrono::{DateTime, Utc};
use conveyor_domain::{Board, Project};
use conveyor_store::board::{BoardMemberRecord, CustomFieldRecord, LabelRecord};
use conveyor_store::card::CardRecord;
use conveyor_store::list::ListRecord;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// CONTRACT §6's shared `IdIn` envelope: one row id, nothing else.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct IdIn {
    /// The row id, resolved under the caller's tenant.
    pub id: Uuid,
}

/// A project as every board read returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProjectOut {
    /// The project id.
    pub id: String,
    /// The short key, unique per tenant.
    pub key: String,
    /// The display name.
    pub name: String,
    /// Free text.
    pub description: String,
    /// `draft | active | paused | closed | archived`.
    pub status: String,
    /// The programme this project rolls up to.
    pub programme_id: Option<String>,
    /// The project-type code (the matrix row's `code`).
    pub project_type_code: Option<String>,
    /// The current delivery phase.
    pub phase_code: Option<String>,
    /// The accountable user.
    pub owner_user_id: Option<String>,
    /// The source repository.
    pub repo_url: Option<String>,
    /// Kind-specific fields (`problem_statement`, `desired_outcome`, …).
    pub meta: Value,
    /// The row version (the `ETag` of a representation).
    pub version: i64,
}

/// One board.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BoardOut {
    /// The board id.
    pub id: String,
    /// The owning project.
    pub project_id: String,
    /// The display name.
    pub name: String,
    /// The row version.
    pub version: i64,
}

/// `projects.get` / `projects.close`: the project and its boards.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProjectView {
    /// The project.
    pub project: ProjectOut,
    /// Every board of the project, in position order.
    pub boards: Vec<BoardOut>,
}

/// `projects.create` — one project, one board and the template's lanes in one transaction.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProjectCreateIn {
    /// The display name. A missing `key` is derived from it.
    pub title: String,
    /// The project-type code (`catalogue_entry(kind='project_type').code`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_type_code: Option<String>,
    /// Free text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Stored under `meta.problem_statement`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem_statement: Option<String>,
    /// Stored under `meta.desired_outcome`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desired_outcome: Option<String>,
    /// The source repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_url: Option<String>,
    /// A programme id, resolved under the caller's tenant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub programme_id: Option<Uuid>,
    /// An explicit key; absent means derived (`slug(title)`, uppercased, collision-suffixed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// `projects.close` — a status-guarded transition, not a version-guarded edit.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProjectCloseIn {
    /// The project id.
    pub id: Uuid,
    /// Recorded under `meta.close_reason`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// `projects.execute` — start (or re-attach to) the project's durable execution task.
///
/// The action is S06c's task *start*: over `/mcp` it returns `resultType:"task"` whose task id
/// is the run it created. Over HTTP it returns the run itself.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProjectExecuteIn {
    /// The project to execute.
    pub project_id: Uuid,
}

/// One lane.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ListOut {
    /// The lane id.
    pub id: String,
    /// The owning board.
    pub board_id: String,
    /// The display name.
    pub name: String,
    /// `intake | active | waiting | done | archive`.
    pub kind: String,
    /// Order within the board (display only).
    pub position: f64,
    /// Optional colour token.
    pub color: Option<String>,
    /// The soft WIP cap.
    pub wip_limit: Option<i32>,
    /// The row version.
    pub version: i64,
}

/// One lane with its live-card count (`boards.get`).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BoardListOut {
    /// The lane.
    pub list: ListOut,
    /// Cards on the lane with `archived_at IS NULL`.
    pub card_count: i64,
}

/// One label.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LabelOut {
    /// The label id.
    pub id: String,
    /// The owning board.
    pub board_id: String,
    /// The display name, unique per board.
    pub name: String,
    /// Free-text colour token.
    pub color: String,
    /// Order within the board.
    pub position: f64,
    /// The row version.
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// One custom field (the group is flattened into two columns, S5-D3).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CustomFieldOut {
    /// The field id.
    pub id: String,
    /// The owning board.
    pub board_id: String,
    /// The flattened group label (`''` when ungrouped).
    pub group_name: String,
    /// Order of the group within the board.
    pub group_position: f64,
    /// The field's own name, unique per board.
    pub name: String,
    /// `text | number | date | select | checkbox`.
    pub kind: String,
    /// The `select` options.
    pub options: Value,
    /// Order within the group.
    pub position: f64,
    /// The row version.
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// One board member.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BoardMemberOut {
    /// The membership id.
    pub id: String,
    /// The board.
    pub board_id: String,
    /// `app_user.id`.
    pub user_id: String,
    /// The member's display name.
    pub display_name: String,
    /// `owner | admin | member | viewer`.
    pub role: String,
    /// Whether the member may comment.
    pub can_comment: bool,
    /// Whether the member watches the board.
    pub watching: bool,
    /// The row version.
    pub version: i64,
    /// When the membership was created.
    pub created_at: DateTime<Utc>,
}

/// `boards.get` — the board, its lanes with counts, labels, fields and members. No cards.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BoardView {
    /// The board.
    pub board: BoardOut,
    /// Lanes in position order, each with its live-card count.
    pub lists: Vec<BoardListOut>,
    /// Labels in position order.
    pub labels: Vec<LabelOut>,
    /// Custom fields in group then field order.
    pub fields: Vec<CustomFieldOut>,
    /// Members in role then name order.
    pub members: Vec<BoardMemberOut>,
}

/// One card.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CardOut {
    /// The card id.
    pub id: String,
    /// The owning board (denormalised from the lane).
    pub board_id: String,
    /// The lane the card sits in — its state.
    pub list_id: String,
    /// The card type's stable code.
    pub card_type_code: String,
    /// The name.
    pub name: String,
    /// Free text.
    pub description: String,
    /// Order within the lane (display only).
    pub position: f64,
    /// A parent card, for subtasks.
    pub parent_card_id: Option<String>,
    /// When the card is due.
    pub due_at: Option<DateTime<Utc>>,
    /// When the due date was marked done.
    pub due_done_at: Option<DateTime<Utc>>,
    /// The exclusion stamp; `Some` means archived.
    pub archived_at: Option<DateTime<Utc>>,
    /// The current claimer, when leased.
    pub assignee_id: Option<String>,
    /// The lane the card came from.
    pub prev_list_id: Option<String>,
    /// When the card last changed lane.
    pub list_changed_at: DateTime<Utc>,
    /// Kind-specific fields.
    pub meta: Value,
    /// The row version (the `ETag` of a representation).
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// `cards.get` / `GET /api/cards/{id}`: the card and its satellites.
///
/// The satellites are read in the same transaction as the card, so the document is one
/// consistent snapshot. `comments` is the newest [`conveyor_store::comment::CARD_VIEW_LIMIT`]
/// messages — the whole thread is `GET /api/cards/{id}/comments`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CardView {
    /// The card.
    pub card: CardOut,
    /// The labels the card carries, in board position order.
    pub labels: Vec<LabelOut>,
    /// The newest comments of the card, oldest first.
    pub comments: Vec<CommentOut>,
    /// The card's checklists, each with its items.
    pub task_lists: Vec<TaskListOut>,
    /// The card's values for the board's custom fields, in field order.
    pub custom_field_values: Vec<CustomFieldValueOut>,
}

/// One comment.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CommentOut {
    /// The comment id.
    pub id: String,
    /// The card the comment belongs to.
    pub card_id: String,
    /// The author, when one is recorded.
    pub author_id: Option<String>,
    /// The author's display name, when the row has an author.
    pub author_name: Option<String>,
    /// The body.
    pub body: String,
    /// When the body was last edited; absent on an unedited comment.
    pub edited_at: Option<DateTime<Utc>>,
    /// The row version (the `ETag` of a representation).
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// One checklist.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TaskListOut {
    /// The checklist id.
    pub id: String,
    /// The card the checklist belongs to.
    pub card_id: String,
    /// Display name.
    pub name: String,
    /// Order within the card (display only).
    pub position: f64,
    /// Whether finished items are hidden.
    pub hide_completed: bool,
    /// Whether the checklist is shown on the card front.
    pub show_on_front: bool,
    /// The items, in position order.
    pub tasks: Vec<TaskOut>,
    /// The row version.
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// One checklist item.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TaskOut {
    /// The item id.
    pub id: String,
    /// The checklist the item belongs to.
    pub task_list_id: String,
    /// Display name.
    pub name: String,
    /// Whether the item is finished.
    pub done: bool,
    /// Order within the checklist (display only).
    pub position: f64,
    /// The row version.
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// One card's value for one board field.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CustomFieldValueOut {
    /// The card.
    pub card_id: String,
    /// The field.
    pub custom_field_id: String,
    /// The value, read whole.
    pub content: Value,
    /// The row version.
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// `cards.move`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CardsMoveIn {
    /// The card id.
    pub id: Uuid,
    /// The lane to move into; it must be on the card's board.
    pub list_id: Uuid,
    /// Land immediately after this card of the target lane; absent appends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_id: Option<Uuid>,
}

/// `cards.duplicate`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CardDuplicateIn {
    /// The card to copy.
    pub id: Uuid,
    /// The lane the copy is born in; absent means the source card's lane.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_id: Option<Uuid>,
    /// The copy's name; absent means the source card's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// `cards.comment`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CardsCommentIn {
    /// The card to comment on.
    pub id: Uuid,
    /// The comment body.
    pub body: String,
}

/// `cards.list` — one page of a board's active cards, optionally restricted to one lane.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CardsListIn {
    /// The board whose cards are read.
    pub board_id: Uuid,
    /// Restrict the page to one lane; absent means every lane.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_id: Option<Uuid>,
    /// Cursor pagination.
    #[serde(default)]
    pub page: crate::views::PageIn,
}

/// `comments.update`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CommentUpdateIn {
    /// The comment id.
    pub id: Uuid,
    /// The replacement body.
    pub body: String,
}

/// `task_lists.create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TaskListCreateIn {
    /// The card the checklist belongs to.
    pub card_id: Uuid,
    /// Display name.
    pub name: String,
    /// Insert immediately after this checklist of the card; absent appends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_id: Option<Uuid>,
}

/// `task_lists.update`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TaskListUpdateIn {
    /// The checklist id.
    pub id: Uuid,
    /// Display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Whether finished items are hidden.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hide_completed: Option<bool>,
    /// Whether the checklist is shown on the card front.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_on_front: Option<bool>,
}

/// `tasks.create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TaskCreateIn {
    /// The checklist the item belongs to.
    pub task_list_id: Uuid,
    /// Display name.
    pub name: String,
    /// Insert immediately after this item of the checklist; absent appends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_id: Option<Uuid>,
}

/// `tasks.update`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TaskUpdateIn {
    /// The item id.
    pub id: Uuid,
    /// Display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Whether the item is finished.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done: Option<bool>,
}

/// `tasks.move`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TaskMoveIn {
    /// The item id.
    pub id: Uuid,
    /// The checklist to move into; it must belong to the same card.
    pub task_list_id: Uuid,
    /// Land immediately after this item of the target checklist; absent appends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_id: Option<Uuid>,
}

/// `custom_fields.create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CustomFieldCreateIn {
    /// The owning board.
    pub board_id: Uuid,
    /// The field's own name, unique per board.
    pub name: String,
    /// `text | number | date | select | checkbox`.
    pub kind: String,
    /// The flattened group label; absent means ungrouped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_name: Option<String>,
    /// Order of the group within the board.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_position: Option<f64>,
    /// The `select` options.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Value>,
}

/// `custom_fields.update`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CustomFieldUpdateIn {
    /// The field id.
    pub id: Uuid,
    /// The field's own name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `text | number | date | select | checkbox`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The flattened group label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_name: Option<String>,
    /// Order of the group within the board.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_position: Option<f64>,
    /// The `select` options.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Value>,
}

/// `custom_fields.set_value`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CustomFieldValueIn {
    /// The card.
    pub card_id: Uuid,
    /// The board field.
    pub custom_field_id: Uuid,
    /// The value; its JSON type must match the field's kind.
    pub content: Value,
}

/// `custom_fields.clear_value`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CustomFieldValueIdIn {
    /// The card.
    pub card_id: Uuid,
    /// The board field.
    pub custom_field_id: Uuid,
}

/// `labels.create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LabelCreateIn {
    /// The owning board.
    pub board_id: Uuid,
    /// Display name, unique per board.
    pub name: String,
    /// Free-text colour token.
    pub color: String,
}

/// `labels.update`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LabelUpdateIn {
    /// The label id.
    pub id: Uuid,
    /// Display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Free-text colour token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Order within the board (display only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<f64>,
}

/// `cards.add_label` / `cards.remove_label`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CardLabelIn {
    /// The card.
    pub card_id: Uuid,
    /// The label; it must be on the card's board.
    pub label_id: Uuid,
}

/// `boards.set_member`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BoardMemberIn {
    /// The board.
    pub board_id: Uuid,
    /// The user to add or update; resolved under the caller's tenant.
    pub user_id: Uuid,
    /// `owner | admin | member | viewer`; absent keeps the stored role (default `member`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Whether the member may comment; absent keeps the stored value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub can_comment: Option<bool>,
    /// Whether the member watches the board; absent keeps the stored value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watching: Option<bool>,
}

/// `lists.create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ListCreateIn {
    /// The owning board.
    pub board_id: Uuid,
    /// The display name.
    pub name: String,
    /// `intake | active | waiting | done | archive`.
    pub kind: String,
    /// Optional colour token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// The soft WIP cap; must be > 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wip_limit: Option<i32>,
    /// Insert immediately after this lane; absent appends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_id: Option<Uuid>,
}

/// `lists.update`. Absent members are unchanged.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ListUpdateIn {
    /// The lane id.
    pub id: Uuid,
    /// The new display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The new kind. A change that would strand a stamped card is `409`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The new colour token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// The new WIP cap; must be > 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wip_limit: Option<i32>,
}

/// `lists.move`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ListMoveIn {
    /// The lane id.
    pub id: Uuid,
    /// Move immediately after this lane; absent moves to the end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_id: Option<Uuid>,
}

/// `cards.create`. The board is the lane's board; a disagreement is refused.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CardCreateIn {
    /// The owning board (must be the lane's board).
    pub board_id: Uuid,
    /// The lane the card is born in; any kind except `archive` (S5-D6).
    pub list_id: Uuid,
    /// The name.
    pub name: String,
    /// Free text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The card type's stable code; resolved under the caller's tenant.
    pub card_type_code: String,
    /// When the card is due.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_at: Option<DateTime<Utc>>,
    /// A parent card on the same board.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_card_id: Option<Uuid>,
}

/// `cards.update`. Absent members are unchanged.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CardUpdateIn {
    /// The card id.
    pub id: Uuid,
    /// The new name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The new description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The new due instant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_at: Option<DateTime<Utc>>,
    /// `true` stamps the due date done, `false` clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_done: Option<bool>,
}

/// `webhooks.create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WebhookCreateIn {
    /// The board whose events it receives; absent means the whole tenant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board_id: Option<Uuid>,
    /// The endpoint a delivery is POSTed to (`http`/`https` only).
    pub url: String,
    /// The registered event types it accepts; absent or empty means every event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<String>>,
    /// Registered event types it refuses even when `events` names them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excluded_events: Option<Vec<String>>,
    /// Whether deliveries start immediately; absent means active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
}

/// `webhooks.update`. Absent members are unchanged.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WebhookUpdateIn {
    /// The subscription id.
    pub id: Uuid,
    /// The endpoint (`http`/`https` only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The registered event types it accepts; `[]` means every event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<String>>,
    /// Registered event types it refuses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excluded_events: Option<Vec<String>>,
    /// Whether deliveries run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
}

/// One webhook subscription.
///
/// `secret` is present ONLY on the `webhooks.create` response — it is the one time the
/// signing bearer token is readable. Every read (the board settings route, `webhooks.update`)
/// omits the member entirely rather than returning `null`, so a client cannot mistake its
/// absence for "no secret".
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WebhookOut {
    /// The subscription id.
    pub id: String,
    /// The board it watches; absent means tenant-wide.
    pub board_id: Option<String>,
    /// The endpoint.
    pub url: String,
    /// The registered event types it accepts; empty means every event.
    pub events: Vec<String>,
    /// Registered event types it refuses.
    pub excluded_events: Vec<String>,
    /// Whether deliveries run.
    pub active: bool,
    /// The row version (the `ETag`).
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
    /// The bearer token, returned once on creation and never by a read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
}

/// `webhooks.test` — one synchronous delivery's outcome.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WebhookDeliveryOut {
    /// The subscription tested.
    pub webhook_id: String,
    /// The endpoint.
    pub url: String,
    /// Whether the endpoint answered 2xx.
    pub ok: bool,
    /// The status the endpoint answered, when it answered.
    pub status_code: Option<u16>,
    /// How many HTTP attempts this call made (always 1 for the synchronous test).
    pub attempts: u8,
    /// How long the call took, in milliseconds.
    pub duration_ms: u64,
    /// Why the call failed, when it did.
    pub error: Option<String>,
}

/// `webhooks.*`'s wire form of a stored subscription. `secret` is never set here.
pub fn webhook_out(record: &conveyor_store::webhook::WebhookRecord) -> WebhookOut {
    WebhookOut {
        id: record.id.to_string(),
        board_id: record.board_id.map(|id| id.to_string()),
        url: record.url.clone(),
        events: record.events.clone(),
        excluded_events: record.excluded_events.clone(),
        active: record.active,
        version: record.version.max(1),
        created_at: record.created_at,
        updated_at: record.updated_at,
        secret: None,
    }
}

// ── the projections ─────────────────────────────────────────────────────────────────────

/// The wire form of a stored decimal: a display number, never an arithmetic input.
pub fn position_f64(position: rust_decimal::Decimal) -> f64 {
    use rust_decimal::prelude::ToPrimitive;
    position.to_f64().unwrap_or(0.0)
}

/// Map a stored project row.
pub fn project_out(project: &Project) -> ProjectOut {
    ProjectOut {
        id: project.id.to_string(),
        key: project.key.clone(),
        name: project.name.clone(),
        description: project.description.clone(),
        status: project.status.clone(),
        programme_id: project.programme_id.map(|id| id.to_string()),
        project_type_code: project.project_type_code.clone(),
        phase_code: project.phase_code.clone(),
        owner_user_id: project.owner_user_id.map(|id| id.to_string()),
        repo_url: project.repo_url.clone(),
        meta: project.meta.clone(),
        version: i64::try_from(project.version.get()).unwrap_or(i64::MAX),
    }
}

/// Map a stored board row.
pub fn board_out(board: &Board) -> BoardOut {
    BoardOut {
        id: board.id.to_string(),
        project_id: board.project_id.to_string(),
        name: board.name.clone(),
        version: i64::try_from(board.version.get()).unwrap_or(i64::MAX),
    }
}

/// Map a stored lane row.
pub fn list_out(record: &ListRecord) -> ListOut {
    ListOut {
        id: record.list.id.to_string(),
        board_id: record.list.board_id.to_string(),
        name: record.list.name.clone(),
        kind: record.list.kind.as_str().to_owned(),
        position: position_f64(record.position),
        color: record.color.clone(),
        wip_limit: record.list.wip_limit,
        version: i64::try_from(record.list.version.get()).unwrap_or(i64::MAX),
    }
}

/// Map a stored label row.
pub fn label_out(record: &LabelRecord) -> LabelOut {
    LabelOut {
        id: record.id.to_string(),
        board_id: record.board_id.to_string(),
        name: record.name.clone(),
        color: record.color.clone(),
        position: position_f64(record.position),
        version: record.version.max(1),
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

/// Map a stored custom-field row.
pub fn custom_field_out(record: &CustomFieldRecord) -> CustomFieldOut {
    CustomFieldOut {
        id: record.id.to_string(),
        board_id: record.board_id.to_string(),
        group_name: record.group_name.clone(),
        group_position: position_f64(record.group_position),
        name: record.name.clone(),
        kind: record.kind.clone(),
        options: record.options.clone(),
        position: position_f64(record.position),
        version: record.version.max(1),
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

/// Map a stored board-membership row.
pub fn board_member_out(record: &BoardMemberRecord) -> BoardMemberOut {
    BoardMemberOut {
        id: record.id.to_string(),
        board_id: record.board_id.to_string(),
        user_id: record.user_id.to_string(),
        display_name: record.display_name.clone(),
        role: record.role.clone(),
        can_comment: record.can_comment,
        watching: record.watching,
        version: record.version.max(1),
        created_at: record.created_at,
    }
}

/// Map a stored card row.
pub fn card_out(record: &CardRecord) -> CardOut {
    CardOut {
        id: record.card.id.to_string(),
        board_id: record.card.board_id.to_string(),
        list_id: record.card.list_id.to_string(),
        card_type_code: record.card_type_code.clone(),
        name: record.name.clone(),
        description: record.description.clone(),
        position: position_f64(record.position),
        parent_card_id: record.parent_card_id.map(|id| id.to_string()),
        due_at: record.due_at,
        due_done_at: record.due_done_at,
        archived_at: record.card.archived_at,
        assignee_id: record.card.assignee_id.map(|id| id.to_string()),
        prev_list_id: record.card.prev_list_id.map(|id| id.to_string()),
        list_changed_at: record.card.list_changed_at,
        meta: record.meta.clone(),
        version: i64::try_from(record.card.version.get()).unwrap_or(i64::MAX),
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

/// Map a stored comment row.
pub fn comment_out(record: &conveyor_store::comment::CommentRecord) -> CommentOut {
    CommentOut {
        id: record.id.to_string(),
        card_id: record.card_id.to_string(),
        author_id: record.author_id.map(|id| id.to_string()),
        author_name: record.author_name.clone(),
        body: record.body.clone(),
        edited_at: record.edited_at,
        version: record.version.max(1),
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

/// Map a stored checklist item.
pub fn task_out(record: &conveyor_store::task::TaskRecord) -> TaskOut {
    TaskOut {
        id: record.id.to_string(),
        task_list_id: record.task_list_id.to_string(),
        name: record.name.clone(),
        done: record.done,
        position: position_f64(record.position),
        version: record.version.max(1),
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

/// Map a stored checklist and the items already read for it.
pub fn task_list_out(
    record: &conveyor_store::task::TaskListRecord,
    tasks: &[conveyor_store::task::TaskRecord],
) -> TaskListOut {
    TaskListOut {
        id: record.id.to_string(),
        card_id: record.card_id.to_string(),
        name: record.name.clone(),
        position: position_f64(record.position),
        hide_completed: record.hide_completed,
        show_on_front: record.show_on_front,
        tasks: tasks.iter().map(task_out).collect(),
        version: record.version.max(1),
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

/// Map a stored custom-field value.
pub fn custom_field_value_out(
    record: &conveyor_store::custom_field::CustomFieldValueRecord,
) -> CustomFieldValueOut {
    CustomFieldValueOut {
        card_id: record.card_id.to_string(),
        custom_field_id: record.custom_field_id.to_string(),
        content: record.content.clone(),
        version: record.version.max(1),
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}
