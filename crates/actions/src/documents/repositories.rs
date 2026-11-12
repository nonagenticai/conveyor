//! `repositories.*` — the repo registry, as references only (S09 §5.3).
//!
//! The conveyor never clones, mirrors or indexes a repository: a row says where the code lives and which
//! credential secret (if any) may reach it, and `coding_jobs.create` is what actually names a repository to a
//! coding service. `project_repository` is the link between a project and a repository, and it is the only
//! many-to-many row this slice adds.
//!
//! `url` is the identity: `(tenant, url)` is unique and there is no update action for it. A repository that moved
//! is a new row, and the old one is deleted — which is what keeps `repository.linked` events unambiguous.

use conveyor_store as store;
use conveyor_store::repository::{NewRepository, RepositoryRecord};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use super::{emit_document_event, invalid};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// A repository as every read of it returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RepositoryOut {
    /// The row id.
    pub id: String,
    /// `github` today.
    pub provider: String,
    /// The clone URL — the repository's identity.
    pub url: String,
    /// The branch a job uses when it names none.
    pub default_branch: String,
    /// The provider's installation handle.
    pub installation_ref: Option<String>,
    /// The `secret` row that holds the credential.
    pub credential_secret_id: Option<String>,
    /// The optimistic-concurrency version (`ETag`).
    pub version: i64,
    /// When the row was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the row last changed.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<RepositoryRecord> for RepositoryOut {
    fn from(record: RepositoryRecord) -> Self {
        Self {
            id: record.id.to_string(),
            provider: record.provider,
            url: record.url,
            default_branch: record.default_branch,
            installation_ref: record.installation_ref,
            credential_secret_id: record.credential_secret_id.map(|id| id.to_string()),
            version: i64::try_from(record.version.get()).unwrap_or(i64::MAX),
            created_at: record.created_at,
            updated_at: record.updated_at,
        }
    }
}

/// Register one repository.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RepositoryCreateIn {
    /// `github` unless the caller says otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// The clone URL.
    pub url: String,
    /// The default branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
    /// The provider's installation handle, opaque.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installation_ref: Option<String>,
    /// The credential secret, resolved in the caller's tenant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_secret_id: Option<Uuid>,
}

/// Why a repository operation failed, as the caller's status.
fn repository_error(error: store::repository::RepositoryStoreError, action: &str) -> ActionError {
    use store::repository::RepositoryStoreError as E;
    match error {
        E::NotFound => ActionError::resource_not_found("such repository"),
        E::UrlTaken => ActionError::new(
            409,
            "conflict",
            format!("{action}: a repository with that url already exists in this tenant"),
        ),
        E::AlreadyLinked => ActionError::new(
            409,
            "conflict",
            format!("{action}: that repository is already linked to the project"),
        ),
        E::VersionMismatch { current } => ActionError::precondition(format!(
            "{action}: the repository is at version {}, not the version the caller supplied",
            current.get()
        )),
        E::Parent(error) => crate::control::parent_error(error, action),
        E::Database(error) => crate::control::store_error(error, action),
    }
}

/// Register a repository.
#[crate::action(
    name = "repositories.create",
    version = 1,
    kind = "internal",
    scopes = ["documents:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn repositories_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RepositoryCreateIn,
) -> Result<HandlerOutput<RepositoryOut>, ActionError> {
    let action = "repositories.create";
    let url = input.url.trim();
    if url.is_empty() {
        return Err(invalid("url", "required", "must not be empty"));
    }
    let record = store::repository::create_repository_tx(
        &mut *tx,
        ctx.tenant,
        &NewRepository {
            provider: input
                .provider
                .as_deref()
                .unwrap_or("github")
                .trim()
                .to_owned(),
            url: url.to_owned(),
            default_branch: input
                .default_branch
                .as_deref()
                .unwrap_or("main")
                .trim()
                .to_owned(),
            installation_ref: input.installation_ref.map(|value| value.trim().to_owned()),
            credential_secret_id: input.credential_secret_id,
        },
    )
    .await
    .map_err(|error| repository_error(error, action))?;
    emit_document_event(
        &mut *tx,
        &ctx,
        "repository.created",
        record.id,
        json!({"id": record.id, "url": record.url}),
    )
    .await?;
    Ok(HandlerOutput::created(RepositoryOut::from(record)))
}

/// The id input every single-repository read takes.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RepositoryIdIn {
    /// The repository id.
    pub id: Uuid,
}

/// Read one repository.
#[crate::action(
    name = "repositories.get",
    version = 1,
    kind = "internal",
    scopes = ["documents:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn repositories_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RepositoryIdIn,
) -> Result<HandlerOutput<RepositoryOut>, ActionError> {
    let record = store::repository::get_repository_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| repository_error(error, "repositories.get"))?
        .ok_or_else(|| ActionError::resource_not_found("such repository"))?;
    let etag = crate::control::etag_of(record.version.get() as i64);
    Ok(HandlerOutput::ok(RepositoryOut::from(record)).with_etag(etag))
}

/// The list takes no filter: a tenant has few repositories and `url` is unique.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct RepositoryListIn {}

/// This tenant's repositories.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RepositoryListOut {
    /// The repositories, ordered by url.
    pub items: Vec<RepositoryOut>,
}

/// List this tenant's repositories.
#[crate::action(
    name = "repositories.list",
    version = 1,
    kind = "internal",
    scopes = ["documents:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn repositories_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    _input: RepositoryListIn,
) -> Result<HandlerOutput<RepositoryListOut>, ActionError> {
    let records = store::repository::list_repositories_tx(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| repository_error(error, "repositories.list"))?;
    Ok(HandlerOutput::ok(RepositoryListOut {
        items: records.into_iter().map(RepositoryOut::from).collect(),
    }))
}

/// Delete one repository and every project link to it.
#[crate::action(
    name = "repositories.delete",
    version = 1,
    kind = "internal",
    scopes = ["documents:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant"
)]
async fn repositories_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RepositoryIdIn,
) -> Result<HandlerOutput<()>, ActionError> {
    let action = "repositories.delete";
    let current = store::repository::get_repository_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| repository_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("such repository"))?;
    let expected = crate::control::require_precondition_with(
        &ctx,
        action,
        serde_json::to_value(RepositoryOut::from(current)).unwrap_or(serde_json::Value::Null),
    )?;
    // The store's delete takes the version too: the precondition is enforced on the destructive
    // statement itself, not only on the read above it.
    store::repository::delete_repository_tx(&mut *tx, ctx.tenant, input.id, expected)
        .await
        .map_err(|error| repository_error(error, action))?;
    Ok(HandlerOutput::no_content(()))
}

/// Link a repository to a project.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RepositoryLinkIn {
    /// The project.
    pub project_id: Uuid,
    /// The repository.
    pub repository_id: Uuid,
    /// The role the project plays with this repository (`primary` by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// Link a repository to a project.
#[crate::action(
    name = "repositories.link",
    version = 1,
    kind = "internal",
    scopes = ["documents:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn repositories_link(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RepositoryLinkIn,
) -> Result<HandlerOutput<()>, ActionError> {
    let role = input.role.as_deref().unwrap_or("primary").trim();
    store::repository::link_repository_tx(
        &mut *tx,
        ctx.tenant,
        input.project_id,
        input.repository_id,
        if role.is_empty() { "primary" } else { role },
    )
    .await
    .map_err(|error| repository_error(error, "repositories.link"))?;
    emit_document_event(
        &mut *tx,
        &ctx,
        "repository.linked",
        input.repository_id,
        json!({"id": input.repository_id, "project_id": input.project_id}),
    )
    .await?;
    Ok(HandlerOutput::no_content(()))
}

/// Unlink a repository from a project.
#[crate::action(
    name = "repositories.unlink",
    version = 1,
    kind = "internal",
    scopes = ["documents:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant"
)]
async fn repositories_unlink(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RepositoryLinkIn,
) -> Result<HandlerOutput<()>, ActionError> {
    store::repository::unlink_repository_tx(
        &mut *tx,
        ctx.tenant,
        input.project_id,
        input.repository_id,
    )
    .await
    .map_err(|error| repository_error(error, "repositories.unlink"))?;
    Ok(HandlerOutput::no_content(()))
}
