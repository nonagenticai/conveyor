//! The cadence tick — T10 (#82, SPEC_V2 §8.2).
//!
//! One pass materialises every `programme_cadence(kind='pi')` row still in `planned`: the
//! standing backlog becomes draft cards ordered by WSJF inside the active lane's WIP slots. The
//! materialisation moves the cadence to `materialised` in the same transaction, so a second pass
//! creates nothing — which is what makes the tick idempotent rather than merely repeatable.
//!
//! A failure on one cadence rolls that cadence back and is logged; the pass continues, so one
//! misconfigured cadence cannot strand the tenant's others.

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use conveyor_store::spine::{self, PlanStoreError};
use sqlx::PgPool;
use uuid::Uuid;

/// Materialise every planned PI cadence in one tenant; returns the cards created, in order.
pub async fn tick(pool: &PgPool, tenant: TenantId) -> Result<Vec<Uuid>, PlanStoreError> {
    // The listing runs in a tenant-scoped transaction: `programme_cadence` is FORCE-RLS, and a
    // bare pool read has no `app_current_tenant` to match.
    let mut listing = begin_scoped(pool, tenant).await?;
    let cadences: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM programme_cadence
          WHERE tenant_id = $1 AND kind = 'pi' AND state = 'planned'
          ORDER BY created_at, id",
    )
    .bind(tenant.as_uuid())
    .fetch_all(&mut *listing)
    .await?;
    listing.commit().await?;
    let mut created = Vec::new();
    for cadence in cadences {
        let mut tx = begin_scoped(pool, tenant).await?;
        match spine::materialise_pi_tx(&mut tx, tenant, cadence).await {
            Ok(materialised) => {
                created.extend(materialised.cards);
                tx.commit().await?;
            }
            Err(error) => {
                tx.rollback().await?;
                tracing::warn!(%error, %cadence, "cadence materialisation failed");
            }
        }
    }
    Ok(created)
}
