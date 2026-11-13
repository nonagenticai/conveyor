//! `conveyor admin intake` — a GitHub issue becomes a card on a project's board.
//!
//! This is the programme's real intake primitive: the operator names a project key, an issue
//! number and a card type, and the command resolves that project's board and `intake` lane. The
//! card is created through `cards.create`, so it is a normal card with a normal event row, and
//! its meta carries the issue link the design site joins on. A second intake of the same issue
//! is refused rather than duplicated.

use conveyor_domain::TenantId;
use conveyor_store as store;
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::action::{ActionCaller, CallError};

/// The card the command created, and where it landed.
#[derive(Debug)]
pub struct IntakeReport {
    /// The new card id.
    pub card_id: String,
    /// The resolved board.
    pub board_id: Uuid,
    /// The resolved intake lane.
    pub list_id: Uuid,
    /// The action's full output, for the caller's log.
    pub output: Value,
}

/// Intake one issue as a card. `plan` and `process_key` are recorded only when supplied.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    pool: &PgPool,
    tenant: TenantId,
    project_key: &str,
    issue: i64,
    title: &str,
    card_type: &str,
    process_key: Option<&str>,
    plan: Option<&str>,
    app_key: &str,
) -> Result<IntakeReport, IntakeError> {
    let mut tx = store::begin_scoped(pool, tenant).await?;
    let project_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM project WHERE tenant_id = $1 AND key = $2")
            .bind(tenant.as_uuid())
            .bind(project_key)
            .fetch_optional(&mut *tx)
            .await?;
    let Some(project_id) = project_id else {
        return Err(IntakeError::UnknownProject(project_key.to_owned()));
    };
    let board_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM board
          WHERE tenant_id = $1 AND project_id = $2
          ORDER BY position, id LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(project_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(board_id) = board_id else {
        return Err(IntakeError::NoBoard(project_key.to_owned()));
    };
    let list_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM list
          WHERE tenant_id = $1 AND board_id = $2 AND kind = 'intake'
          ORDER BY position, id LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(board_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(list_id) = list_id else {
        return Err(IntakeError::NoIntakeLane(project_key.to_owned()));
    };
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM card
          WHERE tenant_id = $1 AND board_id = $2 AND archived_at IS NULL
            AND meta ->> 'issue' = $3
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(board_id)
    .bind(issue.to_string())
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    if let Some(card_id) = existing {
        return Err(IntakeError::AlreadyIntaken { card_id, issue });
    }

    let mut meta = Map::new();
    meta.insert("issue".to_owned(), json!(issue));
    meta.insert("unit_key".to_owned(), json!(project_key));
    meta.insert("origin".to_owned(), json!("github"));
    if let Some(key) = process_key {
        meta.insert("process_key".to_owned(), json!(key));
    }
    if let Some(path) = plan {
        meta.insert("plan".to_owned(), json!(path));
    }

    let caller = ActionCaller::new(pool, tenant, app_key).await?;
    let output = caller
        .call(
            "cards.create",
            json!({
                "board_id": board_id,
                "list_id": list_id,
                "name": title,
                "card_type_code": card_type,
                "meta": Value::Object(meta),
            }),
            None,
        )
        .await?;
    let card_id = output
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_default();
    Ok(IntakeReport {
        card_id,
        board_id,
        list_id,
        output,
    })
}

/// Why intake could not run.
#[derive(Debug, thiserror::Error)]
pub enum IntakeError {
    /// The database refused a read.
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    /// The project key is unknown in this tenant.
    #[error("no project with key {0:?}")]
    UnknownProject(String),
    /// The project has no board.
    #[error("project {0:?} has no board")]
    NoBoard(String),
    /// The project's board has no intake lane.
    #[error("project {0:?} has no intake lane")]
    NoIntakeLane(String),
    /// A non-archived card already names this issue.
    #[error("issue #{issue} is already intaken as card {card_id}")]
    AlreadyIntaken {
        /// The existing card.
        card_id: Uuid,
        /// The issue number.
        issue: i64,
    },
    /// The create action refused.
    #[error(transparent)]
    Call(#[from] CallError),
}
