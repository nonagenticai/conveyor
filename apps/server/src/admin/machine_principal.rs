#![forbid(unsafe_code)]
//! `conveyor admin machine-principal ensure` — provision the row a licence bearer resolves to
//! (#760).
//!
//! A licence bearer proves a machine; `crates/auth` refuses it unless `app_user` holds a row keyed
//! `licence:<subject_id>` whose kind is `agent` or `service`. This command creates that row and its
//! tenant membership from a **live token**, so the subject comes from introspection rather than a
//! value copied out of a document. It is idempotent and refuses to touch a human row.

use conveyor_api::Config;
use conveyor_auth::MachineAuthenticator;
use conveyor_store as store;
use sqlx::PgPool;
use uuid::Uuid;

/// The role a machine principal holds: the code's synthetic tenant-admin for machines
/// (`crates/auth/src/principal.rs`).
pub const MACHINE_ROLE: &str = "kata_executor";

/// What was ensured — never the token.
#[derive(Debug)]
pub struct Ensured {
    /// `app_user.oidc_subject`, i.e. `licence:<subject_id>`.
    pub subject: String,
    /// The `app_user` row.
    pub user_id: Uuid,
    /// The membership role.
    pub role: &'static str,
    /// True when this call created the `app_user` row.
    pub created: bool,
}

/// Resolve the token at the licences service and ensure the machine principal and membership.
pub async fn ensure(
    pool: &PgPool,
    config: &Config,
    tenant_slug: &str,
    token: &str,
) -> Result<Ensured, String> {
    if token.trim().is_empty() {
        return Err("the token is empty".to_owned());
    }
    let base = config
        .licences_base_url
        .as_deref()
        .filter(|base| !base.trim().is_empty())
        .ok_or_else(|| "LICENCES_BASE_URL is not set".to_owned())?;
    let machine = MachineAuthenticator::new(base, &config.app_key)
        .map_err(|error| format!("licences client: {error}"))?;
    let identity = machine
        .identify(token)
        .await
        .map_err(|error| format!("the token does not prove a machine: {error}"))?;

    let tenant = store::tenants::id_by_slug(pool, tenant_slug)
        .await
        .map_err(|error| format!("read tenant {tenant_slug:?}: {error}"))?
        .ok_or_else(|| format!("no tenant {tenant_slug:?}"))?;
    if tenant != identity.tenant_id {
        return Err(format!(
            "the token's tenant {} is not {tenant_slug:?}",
            identity.tenant_id
        ));
    }

    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin: {error}"))?;
    // `membership` carries FORCE RLS keyed on `app_current_tenant()`. An admin command is not
    // the action engine, which sets this per transaction itself, so it names the tenant here.
    sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
        .bind(tenant.as_uuid().to_string())
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("set tenant context: {error}"))?;
    let existing: Option<(Uuid, String)> =
        sqlx::query_as("SELECT id, kind FROM app_user WHERE oidc_subject = $1")
            .bind(&identity.subject)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|error| format!("read app_user: {error}"))?;
    let (user_id, created) = match existing {
        Some((id, kind)) if kind == "service" || kind == "agent" => (id, false),
        Some((_, kind)) => {
            // A licence bearer may never act as a person, and this command refuses to convert a
            // person's row into a machine's — the provisioning is wrong, not the row.
            return Err(format!(
                "{} resolves to a {kind:?} row; refusing to act as that principal",
                identity.subject
            ));
        }
        None => {
            let (id,): (Uuid,) = sqlx::query_as(
                "INSERT INTO app_user (oidc_subject, display_name, kind, status) \
                 VALUES ($1, $2, 'service', 'active') RETURNING id",
            )
            .bind(&identity.subject)
            .bind(format!("licence session ({tenant_slug})"))
            .fetch_one(&mut *tx)
            .await
            .map_err(|error| format!("create app_user: {error}"))?;
            (id, true)
        }
    };
    sqlx::query(
        "INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, $3) \
         ON CONFLICT (tenant_id, user_id) DO NOTHING",
    )
    .bind(tenant.as_uuid())
    .bind(user_id)
    .bind(MACHINE_ROLE)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("ensure membership: {error}"))?;
    tx.commit()
        .await
        .map_err(|error| format!("commit: {error}"))?;

    Ok(Ensured {
        subject: identity.subject,
        user_id,
        role: MACHINE_ROLE,
        created,
    })
}
