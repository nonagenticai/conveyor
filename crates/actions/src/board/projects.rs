#![forbid(unsafe_code)]
//! `projects.create` · `.get` · `.close`.
//!
//! `create` is the one action that makes a project usable: it writes the project, its board,
//! the template's five lanes, the **start card** and that card's **root run** in ONE
//! transaction (S5-D5 — there is no `boards.create`, and a board-less project is unusable;
//! S13 §5.8 — a project nobody is analysing is no more usable than a board-less one). The wire
//! vocabulary is fr's (`title`, `problem_statement`, `desired_outcome`);
//! `project_type`/`repository_url` are the signed channel's names for
//! `project_type_code`/`repo_url` and S06a maps them (S05 §5).

use conveyor_domain::{CatalogueKind, ListKind};
use conveyor_store as store;
use conveyor_store::project::{NewProject, ProjectPatch};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::board::views::{BoardOut, ProjectOut, ProjectView, board_out, project_out};
use crate::board::{invalid, list_error, parent_error, store_error};
use crate::control::run_error;
use crate::control::runs::{emit_run_event, read_record, run_out};
use crate::control::views::RunOut;
use crate::invoke::{ActionError, HandlerCtx};
use crate::process::views::ProcessRunStartIn;
use crate::spec::HandlerOutput;

use super::{ProjectCloseIn, ProjectCreateIn, ProjectExecuteIn, ProjectUpdateIn, views::IdIn};

/// The board every new project gets. One board per project today, but the FK is the
/// contract, not a 1:1 assumption (data-arch §2.3).
const DEFAULT_BOARD_NAME: &str = "Board";

/// The template code `projects.create` reads.
const DEFAULT_TEMPLATE_CODE: &str = "standard";

/// The card type of the start card every fresh project is born with (S13 §5.8).
const START_CARD_TYPE_CODE: &str = "project_start_request_analysis";

/// The published process the start card's root run executes (S13 §5.8).
const START_PROCESS_KEY: &str = "project_start_request_analysis.READY";

/// What the start card is called. The name names the work, not the project: one project, one
/// start card, and the reader should not have to open it to know what it is.
const START_CARD_NAME: &str = "Project start request analysis";

/// A key is what a human types; it is short, uppercase and URL-shaped.
fn validate_key(raw: &str) -> Result<String, ActionError> {
    let key = raw.trim().to_ascii_uppercase();
    let valid = !key.is_empty()
        && key.len() <= 64
        && key
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_' || b == b'-');
    if !valid {
        return Err(invalid(
            "key",
            "invalid_value",
            "must be 1–64 characters of A-Z, 0-9, '_' or '-'",
        ));
    }
    Ok(key)
}

fn project_view(
    project: &conveyor_domain::Project,
    boards: &[conveyor_domain::Board],
) -> ProjectView {
    ProjectView {
        project: project_out(project),
        boards: boards.iter().map(board_out).collect::<Vec<BoardOut>>(),
    }
}

/// Create a project, its board, the template's lanes, the start card and the start card's root
/// run.
#[crate::action(
    name = "projects.create",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn projects_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProjectCreateIn,
) -> Result<HandlerOutput<ProjectOut>, ActionError> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err(invalid("title", "required", "must not be empty"));
    }

    // A project type that names no live catalogue row would make every move deny, so it is
    // refused at creation rather than at the first drag.
    if let Some(code) = input.project_type_code.as_deref() {
        let code = conveyor_domain::Code::parse(code)
            .map_err(|error| invalid("project_type_code", "invalid_value", error.to_string()))?;
        let entry = store::catalogue::get_catalogue_entry_tx(
            &mut *tx,
            ctx.tenant,
            CatalogueKind::ProjectType,
            &code,
        )
        .await
        .map_err(|error| {
            tracing::error!(%error, "read project type");
            ActionError::internal()
        })?;
        if entry.is_none_or(|entry| !entry.is_active) {
            return Err(invalid(
                "project_type_code",
                "invalid_value",
                "no active project_type catalogue row has this code in this tenant",
            ));
        }
    }

    let key = match input.key.as_deref() {
        Some(raw) => validate_key(raw)?,
        None => store::project::derive_project_key_tx(&mut *tx, ctx.tenant, title)
            .await
            .map_err(|error| match error {
                store::project::ProjectStoreError::KeyExhausted { .. } => ActionError::new(
                    409,
                    "conflict",
                    "every derived key for this title is taken; supply an explicit `key`",
                ),
                other => {
                    tracing::error!(error = %other, "derive project key");
                    ActionError::internal()
                }
            })?,
    };

    let mut meta = serde_json::Map::new();
    for (member, value) in [
        ("problem_statement", input.problem_statement.as_ref()),
        ("desired_outcome", input.desired_outcome.as_ref()),
    ] {
        if let Some(value) = value {
            meta.insert(member.to_owned(), serde_json::Value::String(value.clone()));
        }
    }

    let project = store::project::insert_project_tx(
        &mut *tx,
        ctx.tenant,
        &NewProject {
            key,
            name: title.to_owned(),
            description: input.description.unwrap_or_default(),
            status: None,
            programme_id: input.programme_id,
            project_type_code: input.project_type_code,
            phase_code: None,
            owner_user_id: Some(ctx.actor_id),
            repo_url: input.repo_url,
            meta: serde_json::Value::Object(meta),
        },
    )
    .await
    .map_err(|error| match error {
        store::project::ProjectStoreError::Parent(parent) => {
            parent_error(parent, "projects.create")
        }
        other => {
            if let store::project::ProjectStoreError::Database(database) = &other
                && database
                    .as_database_error()
                    .is_some_and(sqlx::error::DatabaseError::is_unique_violation)
            {
                return ActionError::new(
                    409,
                    "conflict",
                    "a project with this key already exists in this tenant",
                );
            }
            tracing::error!(error = %other, "insert project");
            ActionError::internal()
        }
    })?;

    insert_board_from_template(&mut *tx, &ctx, project.id, None).await?;

    // S13 §5.8: the start card and its root run are part of CREATING the project, so they ride
    // this transaction. Last, because `process_runs_start` consumes `ctx`.
    insert_start_card_and_run(&mut *tx, ctx, project.id).await?;

    Ok(HandlerOutput::created(project_out(&project)).with_etag(project.version.etag()))
}

/// Create the project's start card in the board's intake lane and start its root run (S13 §5.8).
///
/// Both writes run in the caller's transaction: a project that exists without its start card is
/// a project nobody is analysing, and a start card without its run is a card no worker will pick
/// up. A tenant whose seed lacks the card type or the published process gets a typed `409`
/// naming what is missing — a fresh project must never be created into a state the engine
/// cannot act on. `ctx` is consumed because `process_runs_start` takes it by value.
async fn insert_start_card_and_run(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    project_id: Uuid,
) -> Result<(), ActionError> {
    // The board `insert_board_from_template` just wrote: one board per project today, at
    // position 0, so the first row IS the board the lanes were created on.
    let boards = store::board::list_boards_tx(&mut *tx, ctx.tenant, project_id)
        .await
        .map_err(|error| store_error(error, "projects.create"))?;
    let board = boards.first().ok_or_else(|| {
        ActionError::new(
            409,
            "board-template-missing",
            "the project has no board; the board template apply wrote nothing",
        )
    })?;

    let intake = store::list::list_of_kind_tx(&mut *tx, ctx.tenant, board.id, ListKind::Intake)
        .await
        .map_err(|error| list_error(error, "projects.create"))?
        .ok_or_else(|| {
            ActionError::new(
                409,
                "board-template-missing",
                format!(
                    "the project's board has no intake lane; the {DEFAULT_TEMPLATE_CODE:?} board \
                     template must list `intake`"
                ),
            )
        })?;

    // A card type is NOT a `catalogue_entry` (data-arch §10 Q1): it lives in its own table, and
    // `project_start_request_analysis` is one of the codes the catalogue seed imports.
    let card_type =
        store::board::get_card_type_by_code_tx(&mut *tx, ctx.tenant, START_CARD_TYPE_CODE)
            .await
            .map_err(|error| {
                tracing::error!(%error, "read the start card type");
                ActionError::internal()
            })?
            .filter(|card_type| card_type.is_active)
            .ok_or_else(|| {
                ActionError::new(
                    409,
                    "card-type-not-seeded",
                    format!(
                        "no active card_type row has the code {START_CARD_TYPE_CODE:?} in this \
                         tenant; run the catalogue seed (`migrate --seed`)"
                    ),
                )
            })?;

    // The same position arithmetic every other card creation uses, so the start card sits at the
    // end of the intake lane exactly like a card a person filed.
    store::position::lock_board_tx(&mut *tx, ctx.tenant, board.id)
        .await
        .map_err(|error| store_error(error, "projects.create"))?;
    let position =
        store::position::next_card_position_tx(&mut *tx, ctx.tenant, board.id, intake.list.id)
            .await
            .map_err(|error| store_error(error, "projects.create"))?;
    let mut new_card =
        store::card::NewCard::new(intake.list.id, card_type.id, START_CARD_NAME, position);
    new_card.created_by = Some(ctx.actor_id);
    let card = store::card::insert_card_tx(&mut *tx, ctx.tenant, &new_card)
        .await
        .map_err(|error| match error {
            store::card::CardStoreError::Parent(parent) => parent_error(parent, "projects.create"),
            other => {
                tracing::error!(error = %other, "insert the start card");
                ActionError::internal()
            }
        })?;

    // The start card is a card like any other: board subscribers — and `wip.pull` — learn about
    // it from `card.created`, never from a poll. Emitted before the run so the event order is
    // the same a person's `cards.create` then `process_runs.start` produces.
    let record = store::card::get_card_record_tx(&mut *tx, ctx.tenant, card.id)
        .await
        .map_err(|error| store_error(error, "projects.create"))?
        .ok_or_else(|| ActionError::resource_not_found("card"))?;
    crate::board::events::card_event(
        &mut *tx,
        &ctx,
        &record,
        crate::board::events::CARD_CREATED,
        serde_json::json!({
            "card_id": record.card.id,
            "list_id": record.card.list_id,
            "position": crate::board::views::position_f64(record.position),
        }),
        serde_json::to_value(crate::board::views::card_out(&record))
            .unwrap_or(serde_json::Value::Null),
        serde_json::Value::Null,
    )
    .await?;

    // The run is the point: without it the card sits in intake forever. `process_runs_start`
    // answers a key with no published version as `404 not_found` (no row) or `409
    // process.not_published` (drafts only); both mean "this tenant's seed lacks the process",
    // so both are re-typed as the one `409` naming the key — never a `500`, and never a `404`
    // for a row the caller never asked about.
    if let Err(error) = crate::process::runs::process_runs_start(
        &mut *tx,
        ctx,
        ProcessRunStartIn {
            card_id: card.id,
            process_id: None,
            process_key: Some(START_PROCESS_KEY.to_owned()),
            inputs: serde_json::json!({}),
            parent_run_id: None,
            step_id: None,
            workflow_step_id: None,
        },
    )
    .await
    {
        match (error.status, error.code) {
            // ⛔ There is NO universal start process. A tenant may run accounting, marketing,
            // software development or human project management; which process (if any) starts a
            // project is that tenant's content. An absent start process is therefore NOT an
            // error: the project, board and start card exist, and a person or the tenant's own
            // automation can start whatever process it wants on that card.
            (404, "not_found") | (409, "process.not_published") => {
                tracing::info!(
                    process_key = START_PROCESS_KEY,
                    "projects.create: the tenant has no published start process; the project and \
                     its start card were created without a run"
                );
                return Ok(());
            }
            _ => return Err(error),
        }
    }
    Ok(())
}

/// Create a project's board and its lanes from a `board_template` catalogue row.
///
/// Shared by `projects.create` and `project_requests.promote`: a promoted project that had
/// no board would be unusable, and two implementations of "what lanes does a new project
/// get" is how the two paths drift. A template that is absent, inactive or lists no lane
/// makes the project unusable (S5-D5), so it is a `409 board-template-missing` — never an
/// invented lane list. `template_code` defaults to [`DEFAULT_TEMPLATE_CODE`].
pub(crate) async fn insert_board_from_template(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    project_id: Uuid,
    template_code: Option<&str>,
) -> Result<(), ActionError> {
    let raw = template_code.unwrap_or(DEFAULT_TEMPLATE_CODE);
    let code = conveyor_domain::Code::parse(raw)
        .map_err(|error| invalid("board_template_code", "invalid_value", error.to_string()))?;
    let kinds = store::board::template_lanes_tx(&mut *tx, ctx.tenant, &code)
        .await
        .map_err(|error| {
            tracing::error!(%error, "read board template");
            ActionError::internal()
        })?
        .ok_or_else(|| {
            ActionError::new(
                409,
                "board-template-missing",
                format!(
                    "no active board_template catalogue row (code {raw:?}) lists any lane for this \
                     tenant; run the catalogue seed (`migrate --dev-seed`)"
                ),
            )
        })?;
    // Validate every kind before the first write, so an invalid template leaves no board.
    for raw_kind in &kinds {
        super::parse_kind(raw_kind, "board_template.lists")?;
    }
    store::board::ensure_board_from_template_tx(
        &mut *tx,
        ctx.tenant,
        project_id,
        DEFAULT_BOARD_NAME,
        &kinds,
        serde_json::json!({ "default_card_type_code": "feature" }),
    )
    .await
    .map_err(|error| match error {
        store::board::BoardStoreError::UnknownLane(raw) => {
            invalid("board_template.lists", "invalid_value", raw)
        }
        other => {
            tracing::error!(%other, "insert board from template");
            ActionError::internal()
        }
    })?;
    Ok(())
}

/// One project with its boards.
#[crate::action(
    name = "projects.get",
    version = 1,
    kind = "internal",
    scopes = ["projects:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn projects_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<ProjectView, ActionError> {
    let project = load_project(tx, ctx.tenant, input.id).await?;
    let boards = store::board::list_boards_tx(&mut *tx, ctx.tenant, project.id)
        .await
        .map_err(|error| store_error(error, "projects.get"))?;
    Ok(project_view(&project, &boards))
}

/// Close a project. Status-guarded, not version-guarded (CONTRACT §3).
#[crate::action(
    name = "projects.close",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant"
)]
async fn projects_close(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProjectCloseIn,
) -> Result<ProjectView, ActionError> {
    let closed =
        store::project::close_project_tx(&mut *tx, ctx.tenant, input.id, input.reason.as_deref())
            .await
            .map_err(|error| store_error(error, "projects.close"))?;
    let project = match closed {
        Some(project) => project,
        None => {
            // Absent or already terminal: the re-read is what distinguishes 404 from 409.
            let current = load_project(tx, ctx.tenant, input.id).await?;
            return Err(ActionError::new(
                409,
                "conflict",
                format!(
                    "project {} is {}; only an open project can be closed",
                    current.id, current.status
                ),
            ));
        }
    };
    let boards = store::board::list_boards_tx(&mut *tx, ctx.tenant, project.id)
        .await
        .map_err(|error| store_error(error, "projects.close"))?;
    Ok(project_view(&project, &boards))
}

/// Edit a project's own fields. Version-guarded: a stale `If-Match` writes nothing.
#[crate::action(
    name = "projects.update",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn projects_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProjectUpdateIn,
) -> Result<HandlerOutput<ProjectView>, ActionError> {
    let action = "projects.update";
    let project = load_project(tx, ctx.tenant, input.id).await?;
    let boards = store::board::list_boards_tx(&mut *tx, ctx.tenant, project.id)
        .await
        .map_err(|error| store_error(error, action))?;
    let expected = crate::control::require_precondition_with(
        &ctx,
        action,
        serde_json::to_value(project_view(&project, &boards)).unwrap_or(serde_json::Value::Null),
    )?;
    if let Some(name) = input.name.as_deref()
        && name.trim().is_empty()
    {
        return Err(invalid("name", "required", "must not be empty"));
    }
    if let Some(status) = input.status.as_deref()
        && !matches!(
            status,
            "draft" | "active" | "paused" | "closed" | "archived"
        )
    {
        return Err(invalid(
            "status",
            "invalid_value",
            "must be draft, active, paused, closed or archived",
        ));
    }
    let mut changed = Vec::new();
    if input.name.is_some() {
        changed.push("name");
    }
    if input.description.is_some() {
        changed.push("description");
    }
    if input.status.is_some() {
        changed.push("status");
    }
    if input.repo_url.is_some() {
        changed.push("repo_url");
    }
    if input.meta.is_some() {
        changed.push("meta");
    }
    if changed.is_empty() {
        return Err(invalid(
            "name",
            "invalid_value",
            "nothing to update: supply name, description, status, repo_url or meta",
        ));
    }
    // Metadata is merged, not replaced: the seed owns keys this action does not (`issues`,
    // `depends_on`, `plan_status`), and an edit must not erase them.
    let meta = input.meta.map(|extra| {
        let mut merged = project.meta.clone();
        if let (serde_json::Value::Object(target), serde_json::Value::Object(source)) =
            (&mut merged, extra)
        {
            for (key, value) in source {
                target.insert(key, value);
            }
        }
        merged
    });
    let patch = ProjectPatch {
        name: input.name.map(|name| name.trim().to_owned()),
        description: input.description,
        status: input.status,
        repo_url: input.repo_url,
        meta,
    };
    let updated = store::project::update_project_tx(
        &mut *tx, ctx.tenant, input.id, expected, &patch,
    )
    .await
    .map_err(|error| match error {
        store::project::ProjectStoreError::NotFound => ActionError::resource_not_found("project"),
        store::project::ProjectStoreError::VersionMismatch { current } => {
            ActionError::precondition(format!(
                "{action}: the project is at version {}, not the version the caller supplied",
                current.get()
            ))
        }
        other => {
            tracing::error!(error = %other, "update project");
            ActionError::internal()
        }
    })?;
    let boards = store::board::list_boards_tx(&mut *tx, ctx.tenant, updated.id)
        .await
        .map_err(|error| store_error(error, action))?;
    let event = store::NewEvent {
        topic: format!("project:{}", updated.id),
        event_type: "project.updated".to_owned(),
        aggregate_type: "project".to_owned(),
        aggregate_id: updated.id,
        payload: serde_json::json!({"id": updated.id, "changed": changed}),
        correlation_id: None,
        causation_id: None,
    };
    store::outbox::append(&mut *tx, ctx.tenant, &event)
        .await
        .map_err(|error| {
            tracing::error!(%error, "append project event");
            ActionError::internal()
        })?;
    let etag = crate::control::etag_of(i64::try_from(updated.version.get()).unwrap_or(i64::MAX));
    Ok(HandlerOutput::ok(project_view(&updated, &boards)).with_etag(etag))
}

async fn load_project(
    tx: &mut PgConnection,
    tenant: conveyor_store::TenantId,
    id: Uuid,
) -> Result<conveyor_domain::Project, ActionError> {
    store::project::get_project_tx(&mut *tx, tenant, id)
        .await
        .map_err(|error| store_error(error, "projects.get"))?
        .ok_or_else(|| ActionError::resource_not_found("project"))
}

/// Start — or re-attach to — the project's durable execution task (S06c).
///
/// The task id IS the run id. `create-or-return` is the whole concurrency story: the partial
/// unique index `uq_run_project_live` (migration 0023) decides which caller created the run,
/// and only that caller enqueues `run.execute` and announces `run.created`; the loser returns
/// the same live run, so a retrying agent cannot start two executions.
#[crate::action(
    name = "projects.execute",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant",
    task = true
)]
async fn projects_execute(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProjectExecuteIn,
) -> Result<HandlerOutput<RunOut>, ActionError> {
    let project = load_project(tx, ctx.tenant, input.project_id).await?;
    if project.status != "active" {
        return Err(ActionError::new(
            409,
            "conflict",
            format!(
                "project {} is {}; only an active project can be executed",
                project.id, project.status
            ),
        ));
    }
    let (run, created) = store::run::create_project_execution_tx(&mut *tx, ctx.tenant, project.id)
        .await
        .map_err(|error| run_error(error, "projects.execute"))?;
    if created {
        store::run::enqueue_run_execute_tx(&mut *tx, ctx.tenant, run.id)
            .await
            .map_err(|error| store_error(error, "projects.execute"))?;
    }
    let record = read_record(tx, &ctx, run.id, "projects.execute").await?;
    let out = run_out(&record);
    if created {
        emit_run_event(
            &mut *tx,
            &ctx,
            run.id,
            "run.created",
            serde_json::json!({ "run_id": run.id, "card_id": null, "attempt": run.attempt }),
            serde_json::to_value(&out).unwrap_or(serde_json::Value::Null),
        )
        .await?;
    }
    // Always 200: `create-or-return` has one representation, whether this call created the
    // run or re-attached to it. The ETag is the run's version either way.
    Ok(HandlerOutput::ok(out).with_etag(run.version.etag()))
}
