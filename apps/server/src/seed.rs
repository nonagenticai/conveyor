//! `migrate --dev-seed` / `migrate --seed`: a thin facade over [`conveyor_store::seed`].
//!
//! The statements live in `conveyor_store::seed` because `store` is the only crate that
//! speaks SQL (CONTRACT §1). This module keeps the call site (`seed::dev(&pool)`, and
//! `seed::seed_catalogue(...)` for the full catalogue import) and the fixture constants in the
//! binary, where the CLI and its tests already import them.

pub use conveyor_store::seed::{
    CatalogueExport, DEV_TENANTS, DEV_USERS, SeedError, catalogue_export, seed_catalogue,
};
pub use conveyor_store::seed_benchmark::seed_benchmark;
pub use conveyor_store::seed_programme::{ProgrammeCounts, seed_programme};

use conveyor_domain::TenantId;
use sqlx::PgPool;
use uuid::Uuid;

/// Insert the development tenants, users, memberships and licence rows.
///
/// The **full** process catalogue (`seed/processes/**`, S13/D29) hangs off the 17 capabilities,
/// which `migrate --seed` imports (`seed_catalogue`). It is therefore applied by `migrate` AFTER
/// `--seed` (`main.rs`), never here: a `--dev-seed`-only run has one fixture capability
/// (`project_management`, seeded by `seed::catalogue`) and must not try the 280 against it.
///
/// Refused by the caller unless `APP_ENV` is an explicit development/test value.
pub async fn dev(pool: &PgPool) -> Result<(), SeedError> {
    conveyor_store::seed::dev(pool).await
}

/// Apply the **default process catalogue** (`seed/processes/**`) to one tenant or to every
/// active one.
///
/// It runs after `seed_catalogue` because every process agent hangs off a `capability` row that
/// import creates; a tenant with no capabilities gets the loader's typed `MissingCapability`
/// naming the code, never a silent skip. Idempotent: a second run creates nothing.
pub async fn seed_process_catalogue(
    pool: &PgPool,
    tenant_slug: Option<&str>,
) -> Result<Vec<(String, conveyor_store::process_seed::SeedCounts)>, SeedError> {
    let targets: Vec<(Uuid, String)> = match tenant_slug {
        Some(slug) => {
            let id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM tenant WHERE slug = $1")
                .bind(slug)
                .fetch_optional(pool)
                .await?;
            let id = id.ok_or_else(|| SeedError::UnknownTenant(slug.to_owned()))?;
            vec![(id, slug.to_owned())]
        }
        None => {
            let mut out = Vec::new();
            for tenant in conveyor_store::tenants::active_ids(pool).await? {
                let slug: Option<String> =
                    sqlx::query_scalar("SELECT slug FROM tenant WHERE id = $1")
                        .bind(tenant.as_uuid())
                        .fetch_optional(pool)
                        .await?;
                let slug = slug.ok_or(SeedError::Missing("tenant", "active tenant re-read"))?;
                out.push((tenant.as_uuid(), slug));
            }
            out
        }
    };
    let mut results = Vec::with_capacity(targets.len());
    for (id, slug) in targets {
        // A process catalogue is only runnable against its vocabulary: a gate step names a
        // `catalogue_entry(kind='gate')`, a card-type process names a `card_type`, and a map
        // names a `project_type`. Install that vocabulary FIRST, idempotently, so one command
        // yields a tenant that can actually run — before 2026-11-10 a tenant seeded with only
        // `--process-catalogue` had processes whose gates the executor refused as unknown.
        crate::seed::seed_catalogue(pool, Some(&slug)).await?;
        let tenant = TenantId::from_uuid(id)?;
        let counts = conveyor_store::process_seed::apply_all(pool, tenant)
            .await
            .map_err(|error| SeedError::Catalogue(format!("seed/processes: {error}")))?;
        results.push((slug, counts));
    }
    Ok(results)
}
