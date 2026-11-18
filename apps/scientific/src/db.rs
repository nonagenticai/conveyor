//! The evidence-review database seam.
//!
//! Every review route opens ONE transaction and binds the tenant and actor inside it, before its
//! first statement. `set_config(..., is_local => true)` is transaction-scoped and discarded at
//! commit, which is the property that makes it safe behind PgBouncer's transaction pooling: a
//! session-scoped GUC would leak one reviewer's identity into the next reviewer's transaction,
//! intermittently and invisibly.
//!
//! The runtime role is the non-owner role from the deployment's `DATABASE_URL`. That is what makes
//! `FORCE ROW LEVEL SECURITY` bind: the owner and any superuser bypass it.

use sqlx::PgPool;
use sqlx::postgres::{PgPoolOptions, Postgres};
use sqlx::{Executor, Transaction};

use crate::actor::Actor;

/// The GUC names the migration's policies and helpers read. Repeated in the SQL by necessity;
/// `tests/evidence_review_schema.rs` proves the policy set and these agree.
pub const TENANT_GUC: &str = "app.current_tenant";
pub const ACTOR_GUC: &str = "app.actor_subject_id";
pub const ROLE_GUC: &str = "app.actor_role";

/// The server-side ceiling every review transaction runs under.
pub const STATEMENT_TIMEOUT_MS: u32 = 30_000;

/// The pool. Lazy: constructing state never blocks startup on Postgres, and a route that needs
/// the database reports its absence rather than taking the process down.
pub fn connect_lazy(url: &str) -> PgPool {
    PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect_lazy(url)
        .unwrap_or_else(|error| panic!("DATABASE_URL is not a Postgres URL: {error}"))
}

/// A transaction with the tenant and the actor bound, and a statement ceiling installed.
///
/// Order is the property: the timeout goes on before `bind_actor`, because binding is itself a
/// round trip and it is the first statement that pays for a stale pooled connection.
pub async fn begin<'a>(
    pool: &'a PgPool,
    actor: &Actor,
) -> Result<Transaction<'a, Postgres>, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    (&mut *transaction)
        .execute(sqlx::raw_sql("SET LOCAL statement_timeout = 30000"))
        .await?;
    set_config(&mut transaction, TENANT_GUC, &actor.tenant.to_string()).await?;
    set_config(&mut transaction, ACTOR_GUC, actor.actor_subject()).await?;
    set_config(&mut transaction, ROLE_GUC, actor.role.as_str()).await?;
    Ok(transaction)
}

async fn set_config(
    transaction: &mut Transaction<'_, Postgres>,
    name: &str,
    value: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT set_config($1, $2, true)")
        .bind(name)
        .bind(value)
        .execute(&mut **transaction)
        .await
        .map(|_| ())
}
