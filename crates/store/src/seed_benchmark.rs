#![forbid(unsafe_code)]
//! The embedded S17 benchmark seed — `seed/benchmark/verified50-v1.json` (SPEC D24).
//!
//! The artifact is embedded at compile time exactly like the S07 catalogue export
//! (`crate::seed::CATALOGUE_EXPORT`): a deployment carries it even when `seed/` is absent from
//! the image, and a run never calls Hugging Face (CONTRACT §19). The file is generated once by
//! `tools/seed-benchmark` through the Rust `make_test_spec` port; this module only parses and
//! imports it.

use sqlx::PgPool;
use uuid::Uuid;

use crate::benchmark::{self, SeedDocument};
use crate::scope::begin_scoped;
use crate::seed::SeedError;
use crate::tenants;
use conveyor_domain::TenantId;

/// The checked-in benchmark artifact.
const BENCHMARK_SEED: &str = include_str!("../../../seed/benchmark/verified50-v1.json");

/// The embedded artifact as [`SeedDocument`].
pub fn seed_document() -> Result<SeedDocument, SeedError> {
    serde_json::from_str(BENCHMARK_SEED)
        .map_err(|err| SeedError::BenchmarkDocument(err.to_string()))
}

/// Import the embedded benchmark suite into one tenant, or into every active tenant.
///
/// `Some(slug)` targets exactly that tenant and an unknown slug is [`SeedError::UnknownTenant`];
/// `None` targets every row of [`tenants::active_ids`]. The import is idempotent by
/// `seed_sha256`, so a second run creates nothing and reports `false`.
///
/// Returns `(tenant slug, suites created)` per target.
pub async fn seed_benchmark(
    pool: &PgPool,
    tenant_slug: Option<&str>,
) -> Result<Vec<(String, usize)>, SeedError> {
    let doc = seed_document()?;
    let targets = match tenant_slug {
        Some(slug) => {
            let tenant_id: Option<Uuid> =
                sqlx::query_scalar("SELECT id FROM tenant WHERE slug = $1")
                    .bind(slug)
                    .fetch_optional(pool)
                    .await?;
            let tenant_id = tenant_id.ok_or_else(|| SeedError::UnknownTenant(slug.to_owned()))?;
            vec![(tenant_id, slug.to_owned())]
        }
        None => {
            let mut targets = Vec::new();
            for tenant in tenants::active_ids(pool).await? {
                let slug: Option<String> =
                    sqlx::query_scalar("SELECT slug FROM tenant WHERE id = $1")
                        .bind(tenant.as_uuid())
                        .fetch_optional(pool)
                        .await?;
                let slug = slug.ok_or(SeedError::Missing("tenant", "active tenant re-read"))?;
                targets.push((tenant.as_uuid(), slug));
            }
            targets
        }
    };

    let mut results = Vec::with_capacity(targets.len());
    for (tenant_id, slug) in targets {
        let tenant = TenantId::from_uuid(tenant_id)?;
        let mut tx = begin_scoped(pool, tenant).await?;
        let (_suite, created) = benchmark::seed_suite_tx(&mut tx, tenant, &doc).await?;
        tx.commit().await?;
        results.push((slug, usize::from(created)));
    }
    Ok(results)
}
