//! Parent-id resolution inside the caller's tenant (CONTRACT §2).
//!
//! A single-column foreign key does not enforce tenancy: referential-integrity checks
//! bypass RLS, so a `tenant_id = A` row can name tenant B's parent id and the FK accepts it.
//! Every store write therefore resolves each parent id **inside the caller's own scope, in
//! the caller's transaction**, before the row is written; a miss is a distinct error the
//! caller maps to a 404/403 (or a 422 for a structural refusal), never an FK violation.
//!
//! The table names are a static whitelist on [`Parent`]. No caller-supplied string ever
//! reaches the SQL, so this helper cannot become an injection point.
//!
//! `app_user` is the one parent that is deliberately global: the table carries no
//! `tenant_id` and no RLS (data-architecture §2.1). Its resolution is an existence check;
//! every other parent is checked with `tenant_id = $1`.

use conveyor_domain::TenantId;
use sqlx::PgConnection;
use uuid::Uuid;

/// A table a tenant-owned row may name in one of its foreign-key columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parent {
    /// `programme`.
    Programme,
    /// `project`.
    Project,
    /// `board`.
    Board,
    /// `list`.
    List,
    /// `card`.
    Card,
    /// `task_list` — S05's checklist container.
    TaskList,
    /// `custom_field` — S05's board field.
    CustomField,
    /// `card_type`.
    CardType,
    /// `run`.
    Run,
    /// `model` — S08's governance model header.
    Model,
    /// `model_element`.
    ModelElement,
    /// `model_relationship`.
    ModelRelationship,
    /// `model_view`.
    ModelView,
    /// `view_object`.
    ViewObject,
    /// `secret` — S09's sealed value, referenced by `repository.credential_secret_id`.
    Secret,
    /// `repository` — S09's repo registry, referenced by `project_repository`.
    Repository,
    /// `capability` — S15's aggregate, referenced by `capability_agent` and `work_package_memory`.
    Capability,
    /// `app_user` — global, no `tenant_id`, no RLS.
    AppUser,
}

impl Parent {
    /// The static table name. This is the only source of the identifier in the query.
    pub const fn table(self) -> &'static str {
        match self {
            Parent::Programme => "programme",
            Parent::Project => "project",
            Parent::Board => "board",
            Parent::List => "list",
            Parent::Card => "card",
            Parent::TaskList => "task_list",
            Parent::CustomField => "custom_field",
            Parent::CardType => "card_type",
            Parent::Run => "run",
            Parent::Model => "model",
            Parent::ModelElement => "model_element",
            Parent::ModelRelationship => "model_relationship",
            Parent::ModelView => "model_view",
            Parent::ViewObject => "view_object",
            Parent::Secret => "secret",
            Parent::Repository => "repository",
            Parent::Capability => "capability",
            Parent::AppUser => "app_user",
        }
    }

    /// True when the table is tenant-owned (it carries `tenant_id` and RLS).
    pub const fn is_tenant_scoped(self) -> bool {
        !matches!(self, Parent::AppUser)
    }
}

impl std::fmt::Display for Parent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.table())
    }
}

/// A parent id that did not resolve in the caller's scope, or a failure while resolving it.
#[derive(Debug, thiserror::Error)]
pub enum ParentError {
    /// The id does not exist for this tenant (or, for `app_user`, at all).
    #[error("the referenced {parent} does not exist")]
    NotFound {
        /// The table the id was meant to name.
        parent: Parent,
    },
    /// The database refused the lookup.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Resolve `id` as a live row of `parent` for `tenant`, in the caller's transaction.
///
/// A miss is [`ParentError::NotFound`]; it is checked BEFORE the caller's insert/update, so
/// the statement never reaches the FK (which would accept another tenant's id).
pub async fn ensure_parent_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    parent: Parent,
    id: Uuid,
) -> Result<(), ParentError> {
    let present: bool = if parent.is_tenant_scoped() {
        let sql = format!(
            "SELECT EXISTS (SELECT 1 FROM {} WHERE tenant_id = $1 AND id = $2)",
            parent.table()
        );
        sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
            .bind(tenant.as_uuid())
            .bind(id)
            .fetch_one(&mut *conn)
            .await?
    } else {
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM app_user WHERE id = $1)")
            .bind(id)
            .fetch_one(&mut *conn)
            .await?
    };
    if present {
        Ok(())
    } else {
        Err(ParentError::NotFound { parent })
    }
}

/// [`ensure_parent_tx`] for an optional parent column: `None` is a valid value.
pub async fn ensure_optional_parent_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    parent: Parent,
    id: Option<Uuid>,
) -> Result<(), ParentError> {
    match id {
        Some(id) => ensure_parent_tx(conn, tenant, parent, id).await,
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_whitelist_covers_every_parent_and_only_app_user_is_global() {
        for parent in [
            Parent::Programme,
            Parent::Project,
            Parent::Board,
            Parent::List,
            Parent::Card,
            Parent::TaskList,
            Parent::CustomField,
            Parent::CardType,
            Parent::Run,
            Parent::Capability,
        ] {
            assert!(parent.is_tenant_scoped(), "{parent} carries tenant_id");
            assert!(
                !parent
                    .table()
                    .contains(|c: char| !c.is_ascii_lowercase() && c != '_'),
                "{parent} is a plain identifier"
            );
        }
        assert!(!Parent::AppUser.is_tenant_scoped());
        assert_eq!(Parent::AppUser.table(), "app_user");
    }
}
